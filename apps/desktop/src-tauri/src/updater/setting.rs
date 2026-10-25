//! Whether update checks run on their own. First launch, the tray and the
//! picker's More menu (#1130) all change it through one worker, so changes
//! are saved in the order they were asked for and never on the main thread.
//! A change is saved before it is applied or shown.

use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};

use tauri::{AppHandle, Manager};

use super::{AutomaticMenuItem, Updates, available, check_now, tell};
use crate::diagnostics::{NativeIssue, report};
use crate::preferences::SettingsFile;

/// Turn checks on or off, or flip them (`None`) as they are when the change
/// is saved; the answer is what then applies.
type Change = (Option<bool>, Sender<bool>);

/// Whether checks run on their own, as the menus show it.
pub(crate) fn automatic(app: &AppHandle) -> bool {
    app.state::<Updates>().automatic.load(Ordering::SeqCst)
}

/// Apply a saved choice, or put the tray tick back after a failed change.
pub(super) fn set_automatic(app: &AppHandle, on: bool) {
    app.state::<Updates>().automatic.store(on, Ordering::SeqCst);
    // Queued, never awaited, so the worker never waits on the main thread.
    if let Some(item) = app.try_state::<AutomaticMenuItem>() {
        let item = item.0.clone();
        let queued = app.run_on_main_thread(move || report(item.set_checked(on), NativeIssue::AutomaticMenuUpdate));
        report(queued, NativeIssue::AutomaticMenuUpdate);
    }
}

/// The tray item was clicked: flip the saved choice.
pub(crate) fn toggle_automatic(app: &AppHandle) {
    change(app, None);
}

/// The worker's queue, started on first use and again after a failure.
#[derive(Default)]
pub(super) struct Saver(Mutex<Option<Sender<Change>>>);

/// Queue a change; the receiver gets what applies once it is saved. If the
/// worker cannot run, nothing changes, the person is told, and the receiver
/// gets no answer.
pub(super) fn change(app: &AppHandle, on: Option<bool>) -> Receiver<bool> {
    let (answer, answered) = mpsc::channel();
    let updates = app.state::<Updates>();
    let Ok(mut queue) = updates.saver.0.lock() else {
        return answered;
    };
    if queue.is_none() {
        *queue = start(app).ok();
    }
    let sent = queue
        .as_ref()
        .map_or(Err(()), |changes| changes.send((on, answer)).map_err(drop));
    if sent.is_err() {
        *queue = None;
        // The tray ticks its item itself; put it back.
        set_automatic(app, automatic(app));
        tell(app, "Ibis could not change whether it checks for updates. Try again.");
    }
    answered
}

fn start(app: &AppHandle) -> Result<Sender<Change>, ()> {
    let (changes, queue) = mpsc::channel::<Change>();
    let handle = app.clone();
    let worker = std::thread::Builder::new()
        .name("update-setting".to_owned())
        .spawn(move || serve(queue, || automatic(&handle), |on| save(&handle, on)));
    report(worker.as_ref().map(drop), NativeIssue::AutomaticMenuUpdate);
    worker.map(|_| changes).map_err(drop)
}

/// Apply each change in turn; a flip reads the state the changes before it
/// left. `save` answers what applies afterwards.
fn serve(queue: Receiver<Change>, current: impl Fn() -> bool, mut save: impl FnMut(bool) -> bool) {
    for (on, answer) in queue {
        let on = save(on.unwrap_or_else(|| !current()));
        let _ = answer.send(on);
    }
}

/// Saved first, then applied: the menus and the running app change only once
/// the choice is on disk, so quitting at any moment never loses a choice that
/// was shown as made.
fn save(app: &AppHandle, on: bool) -> bool {
    if let Err(problem) = app.state::<SettingsFile>().update(|s| s.update_checks = Some(on)) {
        let before = automatic(app);
        set_automatic(app, before);
        tell(
            app,
            &format!("Your choice could not be saved, so nothing changed: {problem}."),
        );
        return before;
    }
    set_automatic(app, on);
    crate::library::refresh_settings_notice(app, &app.state::<SettingsFile>());
    on
}

/// Whether update checks run on their own; `None` when this build has no updates.
#[tauri::command]
#[specta::specta]
pub(crate) async fn update_checks(app: AppHandle) -> Option<bool> {
    available(&app).then(|| automatic(&app))
}

/// Turn automatic checks on or off and save the choice, as the tray item does.
/// Answers what now applies.
#[tauri::command]
#[specta::specta]
pub(crate) async fn set_update_checks(app: AppHandle, enabled: bool) -> Option<bool> {
    if !available(&app) {
        return None;
    }
    let answered = change(&app, Some(enabled));
    match tauri::async_runtime::spawn_blocking(move || answered.recv()).await {
        Ok(Ok(on)) => Some(on),
        _ => Some(automatic(&app)),
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
    use std::cell::RefCell;

    #[test]
    fn changes_are_saved_in_order_and_a_failed_save_changes_nothing() -> Result<(), Box<dyn std::error::Error>> {
        let (changes, queue) = mpsc::channel();
        let mut answers = Vec::new();
        for on in [Some(true), None, Some(true), None] {
            let (answer, answered) = mpsc::channel();
            changes.send((on, answer))?;
            answers.push(answered);
        }
        drop(changes);
        let state = RefCell::new(false);
        let tried = RefCell::new(Vec::new());
        serve(
            queue,
            || *state.borrow(),
            |on| {
                tried.borrow_mut().push(on);
                // The third save fails: nothing is applied.
                if tried.borrow().len() != 3 {
                    *state.borrow_mut() = on;
                }
                *state.borrow()
            },
        );
        assert_eq!(tried.into_inner(), [true, false, true, true]);
        let answers = answers.iter().map(Receiver::recv).collect::<Result<Vec<_>, _>>()?;
        assert_eq!(answers, [true, false, false, true]);
        Ok(())
    }
}
