//! Whether update checks run on their own. First launch, the tray and the
//! picker's More menu (#1130) all queue changes, in the order they were
//! made, to one worker that saves them off the main thread. A change is
//! saved before it is applied, and the menus show only saved choices: the
//! tray tick and More follow the `update:automatic` event sent after each save.

use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};

use tauri::{AppHandle, Emitter, Manager};

use super::{AutomaticMenuItem, Updates, available, check_now, tell};
use crate::diagnostics::{NativeIssue, report};
use crate::preferences::SettingsFile;

/// Whether checks run on their own, as last saved.
pub(crate) fn automatic(app: &AppHandle) -> bool {
    app.state::<Updates>().automatic.load(Ordering::SeqCst)
}

/// Apply a saved choice.
pub(super) fn set_automatic(app: &AppHandle, on: bool) {
    app.state::<Updates>().automatic.store(on, Ordering::SeqCst);
    show(app, on);
}

/// Show `on` in the tray. Queued, never awaited, so the worker never waits
/// on the main thread.
fn show(app: &AppHandle, on: bool) {
    if let Some(item) = app.try_state::<AutomaticMenuItem>() {
        let item = item.0.clone();
        let queued = app.run_on_main_thread(move || report(item.set_checked(on), NativeIssue::AutomaticMenuUpdate));
        report(queued, NativeIssue::AutomaticMenuUpdate);
    }
}

/// The tray item or More was clicked: ask for the opposite of what both show.
/// Both clicks arrive on the main thread, so they are queued in click order.
pub(crate) fn toggle_automatic(app: &AppHandle) {
    let saved = automatic(app);
    // The tray ticks its item itself on a click; it shows only saved choices.
    show(app, saved);
    change(app, !saved);
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
    report(worker.as_ref().map(drop), NativeIssue::AutomaticMenuUpdate);
    worker.map(|_| changes).map_err(drop)
}

/// Save each change in turn.
fn serve(queue: Receiver<bool>, mut save: impl FnMut(bool)) {
    for on in queue {
        save(on);
    }
}

/// Saved first, then applied: the menus and the running app change only once
/// the choice is on disk, so quitting at any moment never loses a choice that
/// was shown as made. More hears what applies either way.
fn save(app: &AppHandle, on: bool) {
    match app.state::<SettingsFile>().update(|s| s.update_checks = Some(on)) {
        Ok(_) => {
            set_automatic(app, on);
            crate::library::refresh_settings_notice(app, &app.state::<SettingsFile>());
        }
        Err(problem) => tell(
            app,
            &format!("Your choice could not be saved, so nothing changed: {problem}."),
        ),
    }
    report(app.emit_to("main", "update:automatic", automatic(app)), NativeIssue::AutomaticMenuUpdate);
}

/// Whether update checks run on their own; `None` when this build has no updates.
#[tauri::command]
#[specta::specta]
pub(crate) async fn update_checks(app: AppHandle) -> Option<bool> {
    available(&app).then(|| automatic(&app))
}

/// Flip automatic checks, as the tray item does. The saved answer arrives
/// as the `update:automatic` event. Not async, so it runs on the main thread
/// in click order with the tray's clicks.
#[tauri::command]
#[specta::specta]
pub(crate) fn toggle_update_checks(app: AppHandle) {
    if available(&app) {
        toggle_automatic(&app);
    }
}

/// "Check for Updates…": the same check the tray starts.
#[tauri::command]
#[specta::specta]
pub(crate) async fn check_for_updates(app: AppHandle) {
    if available(&app) {
        check_now(&app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
