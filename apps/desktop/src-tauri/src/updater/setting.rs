//! Whether update checks run on their own. First launch, the tray and the
//! picker's More menu (#1130) all queue the choice the person made, in the
//! order they made it, to one worker that saves it off the main thread. A
//! change is saved before it is applied, and the menus show only saved
//! choices: the tray tick and More follow the `update:automatic` event sent
//! after each save.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

use tauri::{AppHandle, Emitter, Manager};

use super::{Updates, available, check_now, tell};
use crate::diagnostics::{NativeIssue, report};
use crate::preferences::SettingsFile;

pub(crate) const AUTOMATIC_ON: &str = "update_auto_on";
pub(crate) const AUTOMATIC_OFF: &str = "update_auto_off";

/// The tray shows one of two items with fixed click intents.
pub(crate) struct AutomaticMenuItem {
    menu: tauri::menu::Menu<tauri::Wry>,
    choices: [tauri::menu::CheckMenuItem<tauri::Wry>; 2],
    position: usize,
    shown: AtomicBool,
}

impl AutomaticMenuItem {
    pub(crate) fn new(
        menu: tauri::menu::Menu<tauri::Wry>,
        choices: [tauri::menu::CheckMenuItem<tauri::Wry>; 2],
        position: usize,
    ) -> Self {
        Self {
            menu,
            choices,
            position,
            shown: false.into(),
        }
    }

    /// Each native item has an immutable intent in its ID. Replace the
    /// displayed item after saving, so an older queued click keeps its intent
    /// even if a newer save repaints the check before its event is handled.
    fn show(&self, on: bool) -> tauri::Result<()> {
        let shown = self.shown.load(Ordering::SeqCst);
        let item = &self.choices[usize::from(on)];
        item.set_checked(on)?;
        if shown != on {
            self.menu.insert(item, self.position)?;
            if let Err(error) = self.menu.remove(&self.choices[usize::from(shown)]) {
                report(self.menu.remove(item), NativeIssue::AutomaticMenuUpdate);
                return Err(error);
            }
            self.shown.store(on, Ordering::SeqCst);
        }
        Ok(())
    }
}

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
    let handle = app.clone();
    let queued = app.run_on_main_thread(move || {
        if let Some(item) = handle.try_state::<AutomaticMenuItem>() {
            report(item.show(on), NativeIssue::AutomaticMenuUpdate);
        }
    });
    report(queued, NativeIssue::AutomaticMenuUpdate);
}

/// The clicked item's ID records the choice it displayed. Never read its
/// mutable tick: a queued save may have repainted it before this event.
pub(crate) fn toggle_automatic(app: &AppHandle, id: &str) {
    if let Some(asked) = tray_choice(id) {
        show(app, automatic(app));
        change(app, asked);
    }
}

fn tray_choice(id: &str) -> Option<bool> {
    match id {
        AUTOMATIC_ON => Some(true),
        AUTOMATIC_OFF => Some(false),
        _ => None,
    }
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
    report(
        app.emit_to("main", "update:automatic", automatic(app)),
        NativeIssue::AutomaticMenuUpdate,
    );
}

/// Whether update checks run on their own; `None` when this build has no updates.
#[tauri::command]
#[specta::specta]
pub(crate) async fn update_checks(app: AppHandle) -> Option<bool> {
    available(&app).then(|| automatic(&app))
}

/// Turn automatic checks on or off, as asked in More. The saved answer
/// arrives as the `update:automatic` event. Not async, so it runs on the main
/// thread and is queued in click order with the tray's clicks.
#[tauri::command]
#[specta::specta]
pub(crate) fn set_update_checks(app: AppHandle, enabled: bool) {
    if available(&app) {
        change(&app, enabled);
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
    fn tray_click_keeps_its_intent_after_a_queued_repaint() -> Result<(), Box<dyn std::error::Error>> {
        // The person clicked the checked item to turn updates off. A queued
        // save repaints the check before the event arrives, but not its ID.
        let clicked_id = AUTOMATIC_OFF;
        let (changes, queue) = mpsc::channel();
        for on in [Some(true), Some(true), tray_choice(clicked_id)].into_iter().flatten() {
            changes.send(on)?;
        }
        drop(changes);
        let mut saved = Vec::new();
        serve(queue, |on| saved.push(on));
        assert_eq!(saved, [true, true, false]);
        assert_eq!(tray_choice(AUTOMATIC_ON), Some(true));
        assert_eq!(tray_choice("update_now"), None);
        Ok(())
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
