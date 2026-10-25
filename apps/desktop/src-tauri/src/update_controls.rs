//! The tray's update controls, also in the picker's More menu so desktops
//! without a tray can reach them (#1130). Builds without an update key have
//! neither.

use tauri::AppHandle;

use crate::updater;

/// Whether update checks run on their own; `None` when this build has no updates.
#[tauri::command]
#[specta::specta]
pub(crate) async fn update_checks(app: AppHandle) -> Option<bool> {
    updater::available(&app).then(|| updater::automatic(&app))
}

/// Turn automatic checks on or off and save the choice, as the tray item does.
/// Answers what now applies.
#[tauri::command]
#[specta::specta]
pub(crate) async fn set_update_checks(app: AppHandle, enabled: bool) -> Option<bool> {
    if !updater::available(&app) {
        return None;
    }
    // The tray saves on the main thread; saving there too keeps the two in
    // order, so a later choice is never overwritten by an earlier one.
    let (answer, answered) = std::sync::mpsc::channel();
    let handle = app.clone();
    let queued = app.run_on_main_thread(move || {
        updater::save_automatic(&handle, enabled);
        let _ = answer.send(updater::automatic(&handle));
    });
    if queued.is_ok()
        && let Ok(Ok(on)) = tauri::async_runtime::spawn_blocking(move || answered.recv()).await
    {
        return Some(on);
    }
    Some(updater::automatic(&app))
}

/// "Check for Updates…": the same check the tray starts.
#[tauri::command]
#[specta::specta]
pub(crate) async fn check_for_updates(app: AppHandle) {
    if updater::available(&app) {
        updater::check_now(&app);
    }
}
