//! Native row actions return intent only; the picker still owns references and copying.

use std::sync::{Mutex, mpsc};

use tauri::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::picker::ChooserOpen;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RowAction {
    Copy,
    Toggle,
}

struct Pending {
    ids: [MenuId; 2],
    action: Option<RowAction>,
}

impl Pending {
    fn select(&mut self, id: &MenuId) {
        if self.action.is_none() {
            self.action = self
                .ids
                .iter()
                .position(|candidate| candidate == id)
                .map(|index| [RowAction::Copy, RowAction::Toggle][index]);
        }
    }
}

#[derive(Default)]
pub(crate) struct RowContext(Mutex<Option<Pending>>);

// One listener for the application's lifetime, never one accumulating per popup.
pub(crate) fn selected(app: &AppHandle, event: MenuEvent) {
    if let Ok(mut state) = app.state::<RowContext>().0.lock()
        && let Some(pending) = state.as_mut()
    {
        pending.select(event.id());
    }
}

/// Show only native Copy and Add/Remove actions. Cancellation returns no action.
#[tauri::command]
#[specta::specta]
pub(crate) async fn row_context(window: WebviewWindow, added: bool) -> Result<Option<RowAction>, String> {
    const FAILURE: &str = "the reference menu is not available";
    if window.label() != "main" {
        return Err(FAILURE.into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let Some(lease) = window.state::<ChooserOpen>().begin() else {
            return Ok(None);
        };
        let generation = window.state::<crate::picker::PickerGeneration>().current();
        if !window.is_visible().map_err(|_| FAILURE)? {
            return Ok(None);
        }
        let copy = MenuItem::new(&window, "Copy", true, None::<&str>).map_err(|_| FAILURE)?;
        let toggle = MenuItem::new(
            &window,
            if added {
                "Remove from Citation"
            } else {
                "Add to Citation"
            },
            true,
            None::<&str>,
        )
        .map_err(|_| FAILURE)?;
        let menu = Menu::with_items(&window, &[&copy, &toggle]).map_err(|_| FAILURE)?;
        *window.state::<RowContext>().0.lock().map_err(|_| FAILURE)? = Some(Pending {
            ids: [copy.id().clone(), toggle.id().clone()],
            action: None,
        });
        let shown = window.popup_menu(&menu).map_err(|_| FAILURE.to_string());
        // Native popup blocks until closed, but Tauri queues the menu event.
        // Enqueue from this worker AFTER popup returns: dispatch from the main
        // thread would run inline, incorrectly treating a selection as cancel.
        let (send, receive) = mpsc::sync_channel(1);
        let owner = window.clone();
        if window
            .run_on_main_thread(move || {
                let action = owner
                    .state::<RowContext>()
                    .0
                    .lock()
                    .map_err(|_| FAILURE.to_string())
                    .map(|mut state| state.take().and_then(|pending| pending.action));
                let result = shown
                    .and(action)
                    .map(|action| action.filter(|_| owner.is_visible().unwrap_or(false)));
                // Release/reconcile in this same main-thread turn; a newer
                // show cannot interleave a worker's blocking native queries.
                let result = finish(lease, result, || {
                    crate::picker::reconcile_cancelled_menu(&owner, generation)
                });
                let _ = send.send(result);
            })
            .is_err()
        {
            if let Ok(mut state) = window.state::<RowContext>().0.lock() {
                state.take();
            }
            return Err(FAILURE.to_string());
        }
        receive.recv().map_err(|_| FAILURE.to_string())?
    })
    .await
    .map_err(|_| FAILURE.to_string())?
}

fn finish<T>(
    lease: T,
    result: Result<Option<RowAction>, String>,
    cancelled: impl FnOnce(),
) -> Result<Option<RowAction>, String> {
    drop(lease);
    if !matches!(result, Ok(Some(_))) {
        cancelled();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_reconciles_the_ignored_blur_after_lease_release_but_selection_does_not() {
        let chooser = ChooserOpen::default();
        for action in [None, Some(RowAction::Copy), Some(RowAction::Toggle)] {
            let lease = chooser.begin().unwrap_or_else(|| panic!("menu owns chooser"));
            assert!(!crate::picker::hides_on_blur(true, chooser.is_open(), false));
            let mut reconciled = false;
            let result = finish(lease, Ok(action), || {
                reconciled = true;
                assert!(crate::picker::hides_on_blur(true, chooser.is_open(), false));
                assert!(!crate::picker::hides_on_blur(true, chooser.is_open(), true));
            });
            assert_eq!(result, Ok(action));
            assert_eq!(reconciled, action.is_none());
        }
    }

    #[test]
    fn only_this_popups_first_exact_action_can_complete() {
        let mut pending = Pending {
            ids: [MenuId::new("copy-new"), MenuId::new("toggle-new")],
            action: None,
        };
        for foreign in ["copy-old", "quit", "", "toggle-new-extra"] {
            pending.select(&MenuId::new(foreign));
            assert_eq!(pending.action, None);
        }
        pending.select(&MenuId::new("toggle-new"));
        pending.select(&MenuId::new("copy-new"));
        assert_eq!(pending.action, Some(RowAction::Toggle));
        pending.action = None;
        pending.select(&MenuId::new("copy-new"));
        assert_eq!(pending.action, Some(RowAction::Copy));
    }
}
