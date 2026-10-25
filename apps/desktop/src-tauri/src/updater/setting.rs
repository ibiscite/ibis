//! Whether update checks run on their own. First launch, the tray and the
//! picker's More menu (#1130) all change it through one worker, so changes
//! are saved in the order they were asked for and never on the main thread.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

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

/// Apply a saved choice at start, without saving it again.
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

/// The worker's queue and how many changes in it are not saved yet.
#[derive(Default)]
pub(super) struct Saver {
    queue: Mutex<Option<Sender<Change>>>,
    pending: AtomicUsize,
    leaving: AtomicBool,
}

/// Queue a change; the receiver gets what applies once it is saved. If the
/// worker cannot run, nothing is saved, the person is told, and the receiver
/// gets no answer.
pub(super) fn change(app: &AppHandle, on: Option<bool>) -> Receiver<bool> {
    let (answer, answered) = mpsc::channel();
    let updates = app.state::<Updates>();
    let saver = &updates.saver;
    let Ok(mut queue) = saver.queue.lock() else {
        return answered;
    };
    saver.pending.fetch_add(1, Ordering::SeqCst);
    if queue.is_none() {
        *queue = start(app).ok();
    }
    let sent = queue
        .as_ref()
        .map_or(Err(()), |changes| changes.send((on, answer)).map_err(drop));
    if sent.is_err() {
        // A worker that failed or stopped is started again next time.
        *queue = None;
        saver.pending.fetch_sub(1, Ordering::SeqCst);
        tell(app, "Ibis could not change whether it checks for updates. Try again.");
    }
    answered
}

fn start(app: &AppHandle) -> Result<Sender<Change>, ()> {
    let (changes, queue) = mpsc::channel::<Change>();
    let handle = app.clone();
    let worker = std::thread::Builder::new()
        .name("update-setting".to_owned())
        .spawn(move || {
            serve(
                queue,
                || automatic(&handle),
                |on| {
                    save(&handle, on);
                    handle.state::<Updates>().saver.pending.fetch_sub(1, Ordering::SeqCst);
                },
            );
        });
    report(worker.as_ref().map(drop), NativeIssue::AutomaticMenuUpdate);
    worker.map(|_| changes).map_err(drop)
}

/// Before quitting or restarting, give queued changes a moment to be saved,
/// so a choice the menus already show is not lost.
async fn drain(app: &AppHandle) {
    let handle = app.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || {
        let saver = &handle.state::<Updates>().saver;
        for _ in 0..DRAIN_STEPS {
            if saver.pending.load(Ordering::SeqCst) == 0 {
                return;
            }
            std::thread::sleep(DRAIN_STEP);
        }
    })
    .await;
}

/// Restart into an installed update once queued changes are saved; a
/// restart cannot be held like a quit.
pub(super) async fn restart(app: &AppHandle) -> ! {
    drain(app).await;
    app.restart()
}

/// Quitting while a change waits to be saved: hold the exit until it is
/// saved (or a few seconds pass), keeping the event loop running meanwhile.
pub(crate) fn hold_exit(app: &AppHandle, code: i32) -> bool {
    let saver = &app.state::<Updates>().saver;
    if saver.pending.load(Ordering::SeqCst) == 0 || saver.leaving.swap(true, Ordering::SeqCst) {
        return false;
    }
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        drain(&handle).await;
        handle.exit(code);
    });
    true
}

const DRAIN_STEP: Duration = Duration::from_millis(20);
const DRAIN_STEPS: u32 = 250;

/// Apply each change in turn; a flip reads the state the changes before it left.
fn serve(queue: Receiver<Change>, current: impl Fn() -> bool, mut save: impl FnMut(bool)) {
    for (on, answer) in queue {
        let on = on.unwrap_or_else(|| !current());
        save(on);
        let _ = answer.send(on);
    }
}

fn save(app: &AppHandle, on: bool) {
    set_automatic(app, on);
    if let Err(problem) = app.state::<SettingsFile>().update(|s| s.update_checks = Some(on)) {
        tell(
            app,
            &format!("Your choice applies until Ibis quits, but it could not be saved: {problem}."),
        );
    } else {
        crate::library::refresh_settings_notice(app, &app.state::<SettingsFile>());
    }
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
    fn changes_are_saved_in_order_and_flips_follow_the_one_before() -> Result<(), Box<dyn std::error::Error>> {
        let (changes, queue) = mpsc::channel();
        let mut answers = Vec::new();
        for on in [Some(true), None, Some(false), None, None] {
            let (answer, answered) = mpsc::channel();
            changes.send((on, answer))?;
            answers.push(answered);
        }
        drop(changes);
        let state = RefCell::new(false);
        let saved = RefCell::new(Vec::new());
        serve(
            queue,
            || *state.borrow(),
            |on| {
                *state.borrow_mut() = on;
                saved.borrow_mut().push(on);
            },
        );
        assert_eq!(saved.into_inner(), [true, false, false, true, false]);
        let answers = answers.iter().map(Receiver::recv).collect::<Result<Vec<_>, _>>()?;
        assert_eq!(answers, [true, false, false, true, false]);
        Ok(())
    }
}
