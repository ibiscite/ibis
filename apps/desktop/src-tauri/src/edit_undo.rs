//! The existing Edit menu's Undo slot follows the focused window's live notice.
//! Ordinary text undo is restored as soon as that window has no undo notice.
use std::collections::BTreeMap;
use std::sync::Mutex;

use serde::Serialize;
use tauri::menu::{MenuItem, MenuItemKind, Submenu};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

const ACTION: &str = "ibis-edit-undo";

// Muda treats '&' as a mnemonic marker even on macOS; names are literal text.
fn native_title(title: &str) -> String {
    title.replace('&', "&&")
}

#[derive(Clone, Default)]
struct Claim {
    revision: u64,
    title: Option<String>,
}

#[derive(Default)]
struct Claims(BTreeMap<String, Claim>);
impl Claims {
    fn publish(&mut self, window: &str, revision: u64, title: Option<String>) -> bool {
        let current = self.0.entry(window.to_string()).or_default();
        if revision <= current.revision {
            return false;
        }
        *current = Claim { revision, title };
        true
    }
    fn active(&self, window: &str) -> Option<&Claim> {
        self.0.get(window).filter(|claim| claim.title.is_some())
    }
}

struct Slot<R: Runtime> {
    menu: Submenu<R>,
    original: MenuItemKind<R>,
    custom: MenuItem<R>,
    position: usize,
    showing: bool,
}

impl<R: Runtime> Slot<R> {
    fn find(app: &AppHandle<R>) -> tauri::Result<Option<Self>> {
        let Some(menu) = app.menu() else { return Ok(None) };
        for item in menu.items()? {
            let MenuItemKind::Submenu(edit) = item else { continue };
            if edit.text()? != "Edit" {
                continue;
            }
            for (position, original) in edit.items()?.into_iter().enumerate() {
                let MenuItemKind::Predefined(native) = &original else {
                    continue;
                };
                if native.text()? != "Undo" {
                    continue;
                }
                let custom = MenuItem::with_id(app, ACTION, native.text()?, true, Some("CmdOrCtrl+Z"))?;
                return Ok(Some(Self {
                    menu: edit,
                    original,
                    custom,
                    position,
                    showing: false,
                }));
            }
        }
        Ok(None)
    }
    fn show(&mut self, title: Option<&str>) -> tauri::Result<()> {
        if let Some(title) = title {
            self.custom.set_text(native_title(title))?;
            if !self.showing {
                self.menu.remove(&self.original)?;
                if let Err(error) = self.menu.insert(&self.custom, self.position) {
                    let _ = self.menu.insert(&self.original, self.position);
                    return Err(error);
                }
                self.showing = true;
            }
        } else if self.showing {
            self.menu.remove(&self.custom)?;
            self.menu.insert(&self.original, self.position)?;
            self.showing = false;
        }
        Ok(())
    }
}

pub(crate) struct EditUndo<R: Runtime> {
    claims: Mutex<Claims>,
    slot: Mutex<Option<Slot<R>>>,
}
impl<R: Runtime> Default for EditUndo<R> {
    fn default() -> Self {
        Self {
            claims: Mutex::default(),
            slot: Mutex::default(),
        }
    }
}

fn focused<R: Runtime>(app: &AppHandle<R>) -> Option<String> {
    app.webview_windows()
        .into_iter()
        .find(|(_, window)| window.is_focused().unwrap_or(false))
        .map(|(label, _)| label)
}

fn refresh<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let state = app.state::<EditUndo<R>>();
    let title = focused(app).and_then(|window| state.claims.lock().ok()?.active(&window)?.title.clone());
    let Ok(mut slot) = state.slot.lock() else { return Ok(()) };
    if slot.is_none() {
        *slot = Slot::find(app)?;
    }
    if let Some(slot) = slot.as_mut() {
        slot.show(title.as_deref())?;
    }
    Ok(())
}

fn schedule<R: Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let _ = refresh(&handle);
    });
}

#[tauri::command]
#[specta::specta]
pub(crate) fn set_edit_undo<R: Runtime>(
    app: AppHandle<R>,
    window: tauri::Window<R>,
    state: State<'_, EditUndo<R>>,
    title: Option<String>,
    revision: u64,
) -> Result<(), String> {
    if !matches!(window.label(), "main" | "settings")
        || revision == 0
        || revision >= 1 << 53
        || title
            .as_ref()
            .is_some_and(|title| title.is_empty() || title.chars().count() > 512 || title.chars().any(char::is_control))
    {
        return Err("Undo action is unavailable.".into());
    }
    let accepted = state
        .claims
        .lock()
        .map_err(|_| "Undo action is unavailable.".to_string())?
        .publish(window.label(), revision, title);
    if accepted {
        schedule(&app);
    }
    Ok(())
}

#[derive(Clone, Serialize)]
struct UndoIntent {
    revision: u64,
}

pub(crate) fn selected<R: Runtime>(app: &AppHandle<R>, event: tauri::menu::MenuEvent) {
    if event.id().as_ref() != ACTION {
        return;
    }
    let Some(window) = focused(app) else { return };
    let intent = app.state::<EditUndo<R>>().claims.lock().ok().and_then(|claims| {
        claims.active(&window).map(|claim| UndoIntent {
            revision: claim.revision,
        })
    });
    if let Some(intent) = intent {
        let _ = app.emit_to(window, "edit:undo", intent);
    }
}

pub(crate) fn window_event<R: Runtime>(window: &tauri::Window<R>, event: &tauri::WindowEvent) {
    if matches!(event, tauri::WindowEvent::Destroyed)
        && let Ok(mut claims) = window.app_handle().state::<EditUndo<R>>().claims.lock()
    {
        claims.0.remove(window.label());
    }
    if matches!(event, tauri::WindowEvent::Focused(_) | tauri::WindowEvent::Destroyed) {
        schedule(window.app_handle());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn author_ampersands_are_literal_menu_text() {
        assert_eq!(
            native_title("Undo Remove Okafor & Lima, 2018"),
            "Undo Remove Okafor && Lima, 2018"
        );
        assert_eq!(
            native_title("Undo Remove Invented && Research"),
            "Undo Remove Invented &&&& Research"
        );
    }
    #[test]
    fn windows_and_delayed_menu_publications_cannot_consume_other_actions() {
        let mut claims = Claims::default();
        assert!(claims.publish("main", 2, Some("Undo Remove Invented, 2026".into())));
        assert!(!claims.publish("main", 1, None));
        assert!(claims.publish("settings", 1, Some("Undo Reset Look".into())));
        assert_eq!(claims.active("main").map(|claim| claim.revision), Some(2));
        assert!(claims.publish("main", 3, None));
        assert!(claims.active("main").is_none());
        assert_eq!(
            claims.active("settings").and_then(|claim| claim.title.as_deref()),
            Some("Undo Reset Look")
        );
        claims.0.remove("settings");
        assert!(claims.active("settings").is_none());
        assert!(claims.publish("settings", 1, Some("Undo Restore Default Shortcut".into())));
    }
}
