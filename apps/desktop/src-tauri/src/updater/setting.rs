//! Settings owns automatic update checks. One worker saves choices in order,
//! off the main thread, and publishes each saved answer.

use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};

use tauri::{AppHandle, Emitter, Manager};

use super::{Updates, available, check, tell};
use crate::diagnostics::{NativeIssue, report};
use crate::preferences::SettingsFile;

/// Whether checks run on their own, as last saved.
pub(crate) fn automatic(app: &AppHandle) -> bool {
    app.state::<Updates>().automatic.load(Ordering::SeqCst)
}

/// Apply a saved choice.
pub(super) fn set_automatic(app: &AppHandle, on: bool) {
    app.state::<Updates>().automatic.store(on, Ordering::SeqCst);
}

/// The worker's queue, started on first use and again after a failure.
#[derive(Default)]
pub(super) struct Saver(Mutex<Option<Sender<bool>>>);

/// Queue a change. If the worker cannot run, nothing changes and the person
/// is told.
pub(super) fn change(app: &AppHandle, on: bool) {
    let updates = app.state::<Updates>();
    let Ok(mut queue) = updates.saver.0.lock() else {
        return;
    };
    if queue.is_none() {
        *queue = start(app).ok();
    }
    let sent = queue.as_ref().map_or(Err(()), |changes| changes.send(on).map_err(drop));
    if sent.is_err() {
        *queue = None;
        tell(app, "Ibis could not change whether it checks for updates. Try again.");
    }
}

fn start(app: &AppHandle) -> Result<Sender<bool>, ()> {
    let (changes, queue) = mpsc::channel::<bool>();
    let handle = app.clone();
    let worker = std::thread::Builder::new()
        .name("update-setting".to_owned())
        .spawn(move || serve(queue, |on| save(&handle, on)));
    report(worker.as_ref().map(drop), NativeIssue::UpdateSetting);
    worker.map(|_| changes).map_err(drop)
}

/// Save each change in turn.
fn serve(queue: Receiver<bool>, mut save: impl FnMut(bool)) {
    for on in queue {
        save(on);
    }
}

/// Saved first, then applied: Settings and the running app change only once
/// the choice is on disk, so quitting at any moment never loses a choice that
/// was shown as made. Settings hears what applies either way.
fn save(app: &AppHandle, on: bool) {
    let file = app.state::<SettingsFile>();
    let saved = file
        .exclusive(|path| {
            let before = ibis_engine::settings::load(path).settings;
            let saved = ibis_engine::settings::update(path, |settings| settings.update_checks = Some(on))
                .map_err(crate::library::CommandError::from)?;
            file.remember_native_changes(&before, &saved);
            set_automatic(app, on);
            Ok::<_, crate::library::CommandError>(())
        })
        .and_then(|saved| saved);
    match saved {
        Ok(()) => {
            crate::library::refresh_settings_notice(app, &app.state::<SettingsFile>());
        }
        Err(problem) => tell(
            app,
            &format!("Your choice could not be saved, so nothing changed: {problem}."),
        ),
    }
    for window in ["main", crate::settings_window::LABEL] {
        report(
            app.emit_to(window, "update:automatic", automatic(app)),
            NativeIssue::UpdateSetting,
        );
    }
}

/// Apply an explicit live-file choice without another save or consent prompt.
pub(crate) fn apply_file_setting(app: &AppHandle, saved: Option<bool>) {
    app.state::<Updates>().question.store(false, Ordering::SeqCst);
    set_automatic(app, saved.unwrap_or(false));
    for window in ["main", crate::settings_window::LABEL] {
        report(
            app.emit_to(window, "update:automatic", automatic(app)),
            NativeIssue::UpdateSetting,
        );
    }
}

/// Whether update checks run on their own; `None` when this build has no updates.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn update_checks(app: AppHandle) -> Option<bool> {
    available(&app).then(|| automatic(&app))
}

/// Turn automatic checks on or off from Settings. The saved answer arrives
/// as the `update:automatic` event; changes are queued in click order.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) fn set_update_checks(app: AppHandle, enabled: bool) {
    if available(&app) {
        change(&app, enabled);
    }
}

/// The tray's and menu bar's "Check for Updates…": check now and answer in one
/// native prompt, never by opening Settings. An update found is the offer
/// (install now or later); otherwise the prompt says what the check found.
pub(crate) fn check_from_menu(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let said = if available(&app) {
            menu_answer(check(&app, true).await)
        } else {
            Some("about-unavailable")
        };
        if let Some(id) = said {
            // A menu click leaves Ibis inactive; the prompt must come to the front.
            #[cfg(target_os = "macos")]
            crate::panel::activate_for_dialog(&app);
            tell(
                &app,
                &ibis_engine::catalogue::text(id, None, &[]).unwrap_or_else(|| id.to_owned()),
            );
        }
    });
}

/// What the menu's check says, by catalogue id; an offer speaks for itself.
fn menu_answer(result: super::CheckResult) -> Option<&'static str> {
    match result {
        super::CheckResult::Offered => None,
        super::CheckResult::Newest => Some("about-check-newest"),
        super::CheckResult::NotRun => Some("about-check-busy"),
        super::CheckResult::Failed => Some("about-check-failed"),
    }
}

/// Settings' Check for Updates, in About: the result shows beside the button.
/// An update found still opens the offer.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn check_for_updates(app: AppHandle) -> super::CheckResult {
    if available(&app) {
        check(&app, true).await
    } else {
        super::CheckResult::NotRun
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_check_answers_every_outcome_in_its_prompt_and_an_offer_alone() {
        use super::super::CheckResult;
        assert_eq!(menu_answer(CheckResult::Offered), None, "the offer is the prompt");
        for (result, id) in [
            (CheckResult::Newest, "about-check-newest"),
            (CheckResult::NotRun, "about-check-busy"),
            (CheckResult::Failed, "about-check-failed"),
        ] {
            assert_eq!(menu_answer(result), Some(id));
            let said = ibis_engine::catalogue::text(id, None, &[]);
            assert!(said.is_some_and(|text| !text.is_empty()), "{id} is in the catalogue");
        }
    }

    #[test]
    fn changes_are_saved_in_the_order_they_were_made() -> Result<(), Box<dyn std::error::Error>> {
        let (changes, queue) = mpsc::channel();
        for on in [true, false, false, true] {
            changes.send(on)?;
        }
        drop(changes);
        let mut saved = Vec::new();
        serve(queue, |on| saved.push(on));
        assert_eq!(saved, [true, false, false, true]);
        Ok(())
    }
}
