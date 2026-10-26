//! Signed, opt-in updates from GitHub Releases (ADR 0019).
//!
//! Nothing is checked until the person says yes, once, at first launch or in
//! the menu. A check asks GitHub for one small file; the package is downloaded
//! and installed only after the person agrees, and only if its signature
//! matches the update key built into the app.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::menu::CheckMenuItem;
use tauri::{AppHandle, Emitter, Manager};

mod progress;
#[cfg(any(target_os = "macos", test))]
use tauri_plugin_dialog::MessageDialogResult;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
use tauri_plugin_updater::{Update, Updater, UpdaterExt};

use crate::diagnostics::{NativeIssue, application_message, report};
use crate::preferences::SettingsFile;

/// Automatic checks wait a little after start, then repeat this often.
const FIRST_CHECK: Duration = Duration::from_secs(30);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
// Bound the small manifest request without shortening package downloads.
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const DOWNLOAD_STALL_TIMEOUT: Duration = Duration::from_secs(60);

/// What the first-launch question and the menu item explain.
pub(crate) const QUESTION: &str = "Ibis can check GitHub for a newer version a few times a day. \
It asks GitHub for one small file; GitHub sees your IP address and the Ibis version. \
Nothing from your library is sent, and updates install only when you agree.";

#[derive(Default)]
pub(crate) struct Updates {
    automatic: AtomicBool,
    /// One check or installation at a time.
    busy: Arc<AtomicBool>,
    /// A version the person chose not to install; automatic checks don't offer it again.
    declined: Mutex<Option<String>>,
    progress: Mutex<progress::Progress>,
    #[cfg(any(target_os = "macos", test))]
    installation_failure: InstallationFailure,
}

struct BusyGuard(Arc<AtomicBool>);

impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl Updates {
    fn begin(&self) -> Option<BusyGuard> {
        (!self.busy.swap(true, Ordering::SeqCst)).then(|| BusyGuard(Arc::clone(&self.busy)))
    }
}

/// One fixed recovery notice, retained until positively dismissed. No timer:
/// a busy native owner defers it until another actual picker presentation.
#[cfg(any(target_os = "macos", test))]
#[derive(Default)]
struct InstallationFailure(AtomicBool);

#[cfg(any(target_os = "macos", test))]
impl InstallationFailure {
    fn begin(&self, dialogs: &crate::picker::ChooserOpen) -> Option<crate::picker::ChooserLease> {
        if !self.0.load(Ordering::SeqCst) {
            return None;
        }
        let owner = dialogs.begin()?;
        self.0.load(Ordering::SeqCst).then_some(owner)
    }

    fn answered(&self, result: MessageDialogResult) {
        if matches!(result, MessageDialogResult::Ok) {
            self.0.store(false, Ordering::SeqCst);
        }
    }
}

/// The menu item that turns automatic checks on and off.
pub(crate) struct AutomaticMenuItem(pub(crate) CheckMenuItem<tauri::Wry>);

/// Builds without the update key (development, or before the key exists) have
/// no updates at all, so the menu items and the question are left out.
pub(crate) fn available(app: &AppHandle) -> bool {
    has_update_key(app.config().plugins.0.get("updater"))
}

fn has_update_key(config: Option<&serde_json::Value>) -> bool {
    config
        .and_then(|updater| updater.get("pubkey"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|key| !key.trim().is_empty())
}

/// Called once at start with the saved choice.
/// `can_save` is false when the settings file could not be read: the answer
/// would be lost, so Ibis does not ask until the file is fixed.
pub(crate) fn start(app: &AppHandle, saved: Option<bool>, can_save: bool) {
    if !available(app) {
        return;
    }
    // Development builds, including the ones CI tests, never ask or check by
    // themselves; "Check for Updates…" still works.
    if cfg!(debug_assertions) {
        return;
    }
    match saved {
        Some(on) => set_automatic(app, on),
        None if can_save => ask(app),
        None => {}
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        loop {
            if app.state::<Updates>().automatic.load(Ordering::SeqCst) {
                tauri::async_runtime::block_on(check(&app, false));
            }
            std::thread::sleep(CHECK_EVERY);
        }
    });
}

fn ask(app: &AppHandle) {
    let handle = app.clone();
    app.dialog()
        .message(QUESTION)
        .title("Check for updates automatically?")
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Check Automatically".into(),
            "Not Now".into(),
        ))
        .show(move |yes| save_automatic(&handle, yes));
}

/// The menu item was clicked: flip the saved choice.
pub(crate) fn toggle_automatic(app: &AppHandle) {
    let on = !app.state::<Updates>().automatic.load(Ordering::SeqCst);
    save_automatic(app, on);
}

fn save_automatic(app: &AppHandle, on: bool) {
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

fn set_automatic(app: &AppHandle, on: bool) {
    app.state::<Updates>().automatic.store(on, Ordering::SeqCst);
    if let Some(item) = app.try_state::<AutomaticMenuItem>() {
        report(item.0.set_checked(on), NativeIssue::AutomaticMenuUpdate);
    }
}

/// "Check for Updates…" in the menu.
pub(crate) fn check_now(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { check(&app, true).await });
}

/// The request names Ibis and its version, as `PRIVACY.md` says, instead of
/// the plugin's own name.
pub(crate) fn user_agent<R: tauri::Runtime>(app: &AppHandle<R>) -> String {
    format!("Ibis/{}", app.package_info().version)
}

fn updater<R: tauri::Runtime>(app: &AppHandle<R>, timeout: Duration) -> tauri_plugin_updater::Result<Updater> {
    updater_builder(app, DOWNLOAD_STALL_TIMEOUT)?.timeout(timeout).build()
}

fn updater_builder<R: tauri::Runtime>(
    app: &AppHandle<R>,
    stall_timeout: Duration,
) -> tauri_plugin_updater::Result<tauri_plugin_updater::UpdaterBuilder> {
    Ok(app
        .updater_builder()
        .header("User-Agent", user_agent(app))?
        .configure_client(move |client| client.read_timeout(stall_timeout))
        .version_comparator(|current, release| {
            // The shared signing key authenticates versions, not app identity.
            // Only plain releases or nightly prereleases in the same channel
            // may advance; never relax the plugin's no-downgrade comparison.
            let same_channel = (current.pre.is_empty() && release.version.pre.is_empty())
                || (current.pre.as_str().starts_with("nightly.")
                    && release.version.pre.as_str().starts_with("nightly."));
            same_channel && release.version > current
        }))
}

async fn find_update(client: &Updater, download_timeout: Duration) -> tauri_plugin_updater::Result<Option<Update>> {
    Ok(client.check().await?.map(|mut update| {
        // The plugin deliberately does not carry the manifest timeout over.
        update.timeout = Some(download_timeout);
        update
    }))
}

async fn check(app: &AppHandle, asked: bool) {
    let updates = app.state::<Updates>();
    let Some(busy) = updates.begin() else {
        if asked {
            tell(
                app,
                "Ibis is already checking for updates, or an update is waiting for your answer.",
            );
        }
        return;
    };
    let found = match updater(app, CHECK_TIMEOUT) {
        Ok(updater) => find_update(&updater, DOWNLOAD_TIMEOUT)
            .await
            .map_err(|e| failure_kind(&e)),
        Err(e) => Err(failure_kind(&e)),
    };
    if !asked {
        application_message(&automatic_outcome(&found));
    }
    match found {
        Ok(Some(update)) => {
            let declined = updates.declined.lock().ok().and_then(|d| d.clone());
            if asked || declined.as_deref() != Some(update.version.as_str()) {
                offer(app, update, busy);
            }
        }
        Ok(None) if asked => tell(
            app,
            &format!("You have the newest version, {}.", app.package_info().version),
        ),
        // Automatic checks stay quiet and try again later.
        Err(_) if asked => tell(
            app,
            "Ibis could not check for updates. Check your internet connection and try again later.",
        ),
        _ => {}
    }
}

/// Why a check failed, from a fixed list: the error text itself can carry
/// addresses or paths, so it is never shown or logged.
fn failure_kind(error: &tauri_plugin_updater::Error) -> &'static str {
    use tauri_plugin_updater::Error as E;
    match error {
        E::Reqwest(_) | E::Network(_) | E::Http(_) => "network",
        E::ReleaseNotFound => "no update file",
        E::TargetNotFound(_) | E::TargetsNotFound(_) => "no package for this system",
        E::Serialization(_) | E::Semver(_) | E::InvalidUpdaterFormat => "unreadable update file",
        _ => "other",
    }
}

/// One line on standard error per automatic check, so a check that finds
/// nothing or fails quietly can still be told apart in a log.
fn automatic_outcome(found: &Result<Option<Update>, &'static str>) -> String {
    match found {
        Ok(Some(update)) => format!("automatic update check: {} is available", update.version),
        Ok(None) => "automatic update check: up to date".into(),
        Err(kind) => format!("automatic update check failed: {kind}"),
    }
}

fn offer(app: &AppHandle, update: Update, busy: BusyGuard) {
    let handle = app.clone();
    present_offer(
        app,
        offer_text(&app.package_info().version.to_string(), &update),
        move |install| {
            if install {
                tauri::async_runtime::spawn(async move { install_update(&handle, update, busy).await });
            } else {
                let updates = handle.state::<Updates>();
                if let Ok(mut declined) = updates.declined.lock() {
                    *declined = Some(update.version.clone());
                }
            }
        },
    );
}

#[cfg(any(target_os = "macos", test))]
fn offer_parent<R: tauri::Runtime>(app: &AppHandle<R>) -> Option<tauri::WebviewWindow<R>> {
    app.get_webview_window("main")
}

fn present_offer(app: &AppHandle, text: String, answer: impl FnOnce(bool) + Send + 'static) {
    let dialog = app
        .dialog()
        .message(text)
        .title("An update is available")
        .buttons(offer_buttons());
    #[cfg(target_os = "macos")]
    let (dialog, owner) = {
        let Some(owner) = app.state::<crate::picker::ChooserOpen>().begin() else {
            return;
        };
        let Some(parent) = offer_parent(app) else {
            report(Err::<(), ()>(()), NativeIssue::Window);
            return;
        };
        crate::picker::show_picker(app);
        crate::panel::activate_for_dialog(app);
        (dialog.parent(&parent), owner)
    };
    let handle = app.clone();
    dialog.show(move |accepted| {
        let callback = handle.clone();
        let result = handle.run_on_main_thread(move || {
            #[cfg(target_os = "macos")]
            let owner = owner;
            if accepted {
                crate::picker::show_picker(&callback);
            }
            // Pinned Wry executes nested main-thread tasks inline, so the
            // actual panel-show hook still sees this owner. Never hold it
            // for the download.
            #[cfg(target_os = "macos")]
            drop(owner);
            answer(accepted);
        });
        report(result, NativeIssue::Window);
    });
}

fn offer_buttons() -> MessageDialogButtons {
    if cfg!(target_os = "windows") {
        // rfd's custom TaskDialog actions have no usable InvokePattern in the
        // installed-update reports. Prefer the OS's standard Yes/No actions.
        MessageDialogButtons::YesNo
    } else {
        MessageDialogButtons::OkCancelCustom("Install and Restart".into(), "Later".into())
    }
}

pub(crate) fn offer_text(current: &str, update: &Update) -> String {
    let notes = update
        .body
        .as_deref()
        .map(str::trim)
        .filter(|notes| !notes.is_empty())
        .map(|notes| format!("\n\n{}", shorten(notes, 600)))
        .unwrap_or_default();
    let question = if cfg!(target_os = "windows") {
        "\n\nInstall this update and restart Ibis?"
    } else {
        ""
    };
    format!(
        "Ibis {} is available; you have {current}.{notes}\n\nIbis will download it, check its signature, install it and restart.{question}",
        update.version
    )
}

fn shorten(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(limit - 1).collect::<String>())
    }
}

const INSTALL_FAILURE_MESSAGE: &str = "The update could not be installed. Try again later. If Ibis no longer opens, download and reinstall the newest version from GitHub.";

async fn install_update(app: &AppHandle, update: Update, _busy: BusyGuard) {
    let attempt = app.state::<Updates>().progress.lock().ok().and_then(|mut p| p.begin());
    let Some(attempt) = attempt else {
        publish_progress(app);
        installation_failed(app);
        return;
    };
    publish_progress(app);
    // Only this operation refreshes retained state for late listeners. Chunk
    // callbacks update bytes; they do not send IPC, focus or acquire an owner.
    let handle = app.clone();
    let (refresh, stopped) = std::sync::mpsc::channel::<()>();
    let _publisher = std::thread::spawn(move || {
        while matches!(
            stopped.recv_timeout(Duration::from_secs(1)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ) {
            publish_progress(&handle);
        }
    });
    let result = update
        .download_and_install(
            |bytes, total| change_progress(app, attempt, Some((bytes, total)), "downloading"),
            || {
                change_progress(app, attempt, None, "installing");
                publish_progress(app);
            },
        )
        .await;
    drop(refresh);
    match result {
        Ok(()) => app.restart(),
        Err(_) => {
            change_progress(app, attempt, None, "failed");
            publish_progress(app);
            installation_failed(app);
        }
    }
}

fn change_progress(app: &AppHandle, attempt: u32, chunk: Option<(usize, Option<u64>)>, phase: &'static str) {
    if let Ok(mut progress) = app.state::<Updates>().progress.lock() {
        progress.change(attempt, chunk, phase);
    }
}

fn publish_progress(app: &AppHandle) {
    let snapshot = app
        .state::<Updates>()
        .progress
        .lock()
        .ok()
        .and_then(|mut p| p.snapshot());
    if let Some(snapshot) = snapshot {
        report(
            app.emit_to("main", "update:progress", snapshot),
            NativeIssue::PickerEvent,
        );
    }
}

fn installation_failed(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        app.state::<Updates>()
            .installation_failure
            .0
            .store(true, Ordering::SeqCst);
        present_installation_failure(app, true);
    }
    #[cfg(not(target_os = "macos"))]
    tell(app, INSTALL_FAILURE_MESSAGE);
}

/// Called after an actual native panel show, never from a timer or at login.
pub(crate) fn picker_presented(app: &AppHandle) {
    publish_progress(app);
    #[cfg(target_os = "macos")]
    present_installation_failure(app, false);
}

#[cfg(target_os = "macos")]
fn present_installation_failure(app: &AppHandle, show: bool) {
    let handle = app.clone();
    report(
        app.run_on_main_thread(move || {
            let updates = handle.state::<Updates>();
            // An older pending recovery must not interrupt a new download.
            if updates
                .progress
                .lock()
                .is_ok_and(|p| p.0.as_ref().is_some_and(|s| s.phase != "failed"))
            {
                return;
            }
            let Some(owner) = updates
                .installation_failure
                .begin(&handle.state::<crate::picker::ChooserOpen>())
            else {
                return;
            };
            let Some(window) = crate::diagnostics::checked(offer_parent(&handle).ok_or(()), NativeIssue::Window) else {
                return;
            };
            if show {
                // The lease also covers showing the parent; its actual-show
                // hook sees the owner and cannot recursively present a sheet.
                crate::picker::show_picker(&handle);
            }
            if crate::diagnostics::checked(window.is_visible(), NativeIssue::Visibility) != Some(true) {
                return;
            }
            // Same fallible AppKit handle path as parent(); Tao's display
            // handle is infallible. Preflight on the main thread only.
            if crate::diagnostics::checked(window.ns_view(), NativeIssue::Window).is_none() {
                return;
            }
            crate::panel::activate_for_dialog(&handle);
            let callback = handle.clone();
            handle
                .dialog()
                .message(INSTALL_FAILURE_MESSAGE)
                .title("Updates")
                .buttons(MessageDialogButtons::Ok)
                .parent(&window)
                .show_with_result(move |result| {
                    let _owner = owner;
                    callback.state::<Updates>().installation_failure.answered(result);
                });
        }),
        NativeIssue::Window,
    );
}

fn tell(app: &AppHandle, text: &str) {
    app.dialog().message(text).title("Updates").show(|_| {});
}

#[cfg(test)]
mod network_tests;
#[cfg(test)]
mod tests;
