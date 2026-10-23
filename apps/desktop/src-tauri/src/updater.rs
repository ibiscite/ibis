//! Signed, opt-in updates from GitHub Releases (ADR 0019).
//!
//! Nothing is checked until the person says yes, once, at first launch or in
//! the menu. A check asks GitHub for one small file; the package is downloaded
//! and installed only after the person agrees, and only if its signature
//! matches the update key built into the app.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
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

/// What the first-launch question and the menu item explain.
pub(crate) const QUESTION: &str = "Ibis can check GitHub for a newer version a few times a day. \
It asks GitHub for one small file; GitHub sees your IP address and the Ibis version. \
Nothing from your library is sent, and updates install only when you agree.";

#[derive(Default)]
pub(crate) struct Updates {
    automatic: AtomicBool,
    /// One check or installation at a time.
    busy: AtomicBool,
    /// A version the person chose not to install; automatic checks don't offer it again.
    declined: Mutex<Option<String>>,
    progress: Mutex<progress::Progress>,
    #[cfg(any(target_os = "macos", test))]
    installation_failure: InstallationFailure,
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
    updater_builder(app)?.timeout(timeout).build()
}

fn updater_builder<R: tauri::Runtime>(
    app: &AppHandle<R>,
) -> tauri_plugin_updater::Result<tauri_plugin_updater::UpdaterBuilder> {
    Ok(app
        .updater_builder()
        .header("User-Agent", user_agent(app))?
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

async fn check(app: &AppHandle, asked: bool) {
    let updates = app.state::<Updates>();
    if updates.busy.swap(true, Ordering::SeqCst) {
        if asked {
            tell(
                app,
                "Ibis is already checking for updates, or an update is waiting for your answer.",
            );
        }
        return;
    }
    let found = match updater(app, CHECK_TIMEOUT) {
        Ok(updater) => updater.check().await.map_err(|e| failure_kind(&e)),
        Err(e) => Err(failure_kind(&e)),
    };
    if !asked {
        application_message(&automatic_outcome(&found));
    }
    match found {
        Ok(Some(update)) => {
            let declined = updates.declined.lock().ok().and_then(|d| d.clone());
            if asked || declined.as_deref() != Some(update.version.as_str()) {
                offer(app, update);
                // `offer` clears `busy` once the person has answered.
                return;
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
    updates.busy.store(false, Ordering::SeqCst);
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

fn offer(app: &AppHandle, update: Update) {
    let handle = app.clone();
    present_offer(
        app,
        offer_text(&app.package_info().version.to_string(), &update),
        move |install| {
            if install {
                tauri::async_runtime::spawn(async move { install_update(&handle, update).await });
            } else {
                let updates = handle.state::<Updates>();
                if let Ok(mut declined) = updates.declined.lock() {
                    *declined = Some(update.version.clone());
                }
                updates.busy.store(false, Ordering::SeqCst);
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
            app.state::<Updates>().busy.store(false, Ordering::SeqCst);
            return;
        };
        let Some(parent) = offer_parent(app) else {
            app.state::<Updates>().busy.store(false, Ordering::SeqCst);
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
        if result.is_err() {
            handle.state::<Updates>().busy.store(false, Ordering::SeqCst);
        }
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

async fn install_update(app: &AppHandle, update: Update) {
    let attempt = app.state::<Updates>().progress.lock().ok().and_then(|mut p| p.begin());
    let Some(attempt) = attempt else {
        publish_progress(app);
        app.state::<Updates>().busy.store(false, Ordering::SeqCst);
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
            app.state::<Updates>().busy.store(false, Ordering::SeqCst);
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
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn installation_recovery_waits_for_an_exclusive_owner() {
        let updates = Updates::default();
        let failure = &updates.installation_failure;
        let dialogs = crate::picker::ChooserOpen::default();
        assert!(failure.begin(&dialogs).is_none());
        failure.0.store(true, Ordering::SeqCst);
        let chooser = dialogs.begin().unwrap_or_else(|| panic!("chooser owner"));
        assert!(failure.begin(&dialogs).is_none());
        drop(chooser);
        let owner = failure.begin(&dialogs).unwrap_or_else(|| panic!("later actual show"));
        // An actual-show callback during initial presentation cannot recurse.
        assert!(failure.begin(&dialogs).is_none());
        drop(owner); // missing/hidden/unavailable parent leaves recovery pending
        assert!(failure.begin(&dialogs).is_some());
    }

    #[test]
    fn only_ok_dismisses_installation_recovery_and_drop_releases_blur() {
        let updates = Updates::default();
        let failure = &updates.installation_failure;
        let dialogs = crate::picker::ChooserOpen::default();
        failure.0.store(true, Ordering::SeqCst);
        for result in [
            MessageDialogResult::Cancel,
            MessageDialogResult::Yes,
            MessageDialogResult::No,
            MessageDialogResult::Custom("OK".into()),
        ] {
            let owner = failure.begin(&dialogs).unwrap_or_else(|| panic!("pending owner"));
            let callback = || {
                let _owner = owner;
                failure.answered(result);
            };
            callback();
            assert!(!dialogs.is_open());
            assert!(failure.0.load(Ordering::SeqCst));
        }
        let owner = failure.begin(&dialogs).unwrap_or_else(|| panic!("pending owner"));
        drop(move || {
            let _owner = owner;
            failure.answered(MessageDialogResult::Ok);
        });
        assert!(!dialogs.is_open());
        // A dropped native callback must not acknowledge an unseen message.
        let owner = failure.begin(&dialogs).unwrap_or_else(|| panic!("still pending"));
        owner.during(|| failure.answered(MessageDialogResult::Ok));
        assert!(!dialogs.is_open());
        assert!(failure.begin(&dialogs).is_none());
    }

    #[test]
    fn installation_failure_offers_recovery_without_claiming_rollback() {
        assert!(!INSTALL_FAILURE_MESSAGE.contains("unchanged"));
        assert!(INSTALL_FAILURE_MESSAGE.contains("Try again"));
        assert!(INSTALL_FAILURE_MESSAGE.contains("download"));
    }

    #[test]
    fn update_offer_requires_the_picker_instead_of_another_owned_window() -> Result<(), Box<dyn std::error::Error>> {
        let app = tauri::test::mock_builder().build(tauri::test::mock_context(tauri::test::noop_assets()))?;
        assert!(offer_parent(app.handle()).is_none());
        let _notice = tauri::WebviewWindowBuilder::new(&app, "notice", Default::default()).build()?;
        assert!(offer_parent(app.handle()).is_none());
        let _picker = tauri::WebviewWindowBuilder::new(&app, "main", Default::default()).build()?;
        assert_eq!(
            offer_parent(app.handle()).map(|window| window.label().to_owned()),
            Some("main".into())
        );
        Ok(())
    }

    #[test]
    fn update_offer_uses_standard_windows_consent_buttons() {
        if cfg!(target_os = "windows") {
            assert!(matches!(offer_buttons(), MessageDialogButtons::YesNo));
        } else {
            match offer_buttons() {
                MessageDialogButtons::OkCancelCustom(yes, no) => {
                    assert_eq!((yes.as_str(), no.as_str()), ("Install and Restart", "Later"));
                }
                _ => panic!("the update must offer an explicit accept and decline action"),
            }
        }
    }

    #[test]
    fn automatic_checks_log_a_fixed_outcome_never_the_error_text() {
        use tauri_plugin_updater::Error as E;
        let private = "https://example.invalid/private/path";
        for (error, kind) in [
            (E::Network(private.into()), "network"),
            (E::ReleaseNotFound, "no update file"),
            (E::TargetNotFound(private.into()), "no package for this system"),
            (E::TempDirNotFound, "other"),
        ] {
            let line = automatic_outcome(&Err(failure_kind(&error)));
            assert_eq!(line, format!("automatic update check failed: {kind}"));
            assert!(!line.contains("example"));
        }
        assert_eq!(automatic_outcome(&Ok(None)), "automatic update check: up to date");
    }

    #[test]
    fn updates_are_off_without_an_update_key() {
        assert!(!has_update_key(None));
        assert!(!has_update_key(Some(&json!({}))));
        assert!(!has_update_key(Some(&json!({ "pubkey": " " }))));
        assert!(has_update_key(Some(&json!({ "pubkey": "dW50cnVzdGVk" }))));
    }

    #[test]
    fn the_shipped_config_reads_only_the_fixed_stable_feed() {
        let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap_or_default();
        let updater = &config["plugins"]["updater"];
        assert_eq!(
            updater["endpoints"],
            json!(["https://github.com/ibiscite/ibis/releases/download/stable/latest.json"])
        );
        assert!(has_update_key(Some(updater)), "the Ibis update key must be built in");
        assert_eq!(updater["requireSignedVersion"], true);
        assert!(updater.get("dangerousInsecureTransportProtocol").is_none());
    }

    #[test]
    fn the_question_says_what_is_sent() {
        for words in ["GitHub", "IP address", "version", "Nothing from your library"] {
            assert!(QUESTION.contains(words), "missing {words}");
        }
    }

    #[test]
    fn long_release_notes_are_shortened() {
        assert_eq!(shorten("short", 600), "short");
        let long = "é".repeat(700);
        let short = shorten(&long, 600);
        assert_eq!(short.chars().count(), 600);
        assert!(short.ends_with('…'));
    }

    #[test]
    fn stalled_manifest_headers_and_body_time_out_and_allow_a_later_check() {
        use std::io::{BufRead, BufReader, Write};
        use tauri_plugin_updater::Error;

        for partial_body in [false, true] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap_or_else(|e| panic!("{e}"));
            let endpoint = format!(
                "http://{}/latest.json",
                listener.local_addr().unwrap_or_else(|e| panic!("{e}"))
            );
            listener.set_nonblocking(true).unwrap_or_else(|e| panic!("{e}"));
            let (release, held) = std::sync::mpsc::channel();
            let server = std::thread::spawn(move || {
                for attempt in 0..2 {
                    let deadline = std::time::Instant::now() + Duration::from_secs(3);
                    let (mut stream, _) = loop {
                        match listener.accept() {
                            Ok(connection) => break connection,
                            Err(error)
                                if error.kind() == std::io::ErrorKind::WouldBlock
                                    && std::time::Instant::now() < deadline =>
                            {
                                std::thread::sleep(Duration::from_millis(5));
                            }
                            Err(error) => panic!("accept: {error}"),
                        }
                    };
                    // Accepted sockets can inherit the listener mode on macOS.
                    stream.set_nonblocking(false).unwrap_or_else(|e| panic!("{e}"));
                    stream
                        .set_write_timeout(Some(Duration::from_secs(2)))
                        .unwrap_or_else(|e| panic!("{e}"));
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap_or_else(|e| panic!("{e}"));
                    let mut request = BufReader::new(&stream);
                    let mut line = String::new();
                    while request.read_line(&mut line).unwrap_or_else(|e| panic!("{e}")) > 2 {
                        line.clear();
                    }
                    if attempt == 0 {
                        if partial_body {
                            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 100\r\n\r\n{\"version\":").unwrap_or_else(|e| panic!("{e}"));
                        }
                        // A bounded server hold makes the pre-fix failure finite.
                        let _ = held.recv_timeout(Duration::from_secs(2));
                    } else {
                        stream
                            .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
                            .unwrap_or_else(|e| panic!("{e}"));
                    }
                }
            });
            let mut context = tauri::test::mock_context(tauri::test::noop_assets());
            context.config_mut().plugins.0.insert(
                "updater".into(),
                json!({ "pubkey": "synthetic", "endpoints": [endpoint], "dangerousInsecureTransportProtocol": true }),
            );
            let app = tauri::test::mock_builder()
                .plugin(tauri_plugin_updater::Builder::new().build())
                .build(context)
                .unwrap_or_else(|e| panic!("{e}"));
            let client = updater(app.handle(), Duration::from_millis(200)).unwrap_or_else(|e| panic!("{e}"));
            let stalled = tauri::async_runtime::block_on(client.check());
            let _ = release.send(());
            let recovered = tauri::async_runtime::block_on(client.check());
            server.join().unwrap_or_else(|_| panic!("server thread panicked"));
            assert!(
                matches!(&stalled, Err(Error::Reqwest(error)) if error.is_timeout()),
                "partial_body={partial_body}: expected request timeout"
            );
            assert!(matches!(recovered, Ok(None)), "later check must complete");
        }
    }

    /// A real check and download against a local server: the manifest shape the
    /// release workflow publishes, a matching signature, and refusals.
    #[test]
    fn signed_updates_are_found_and_verified_and_tampered_ones_refused() {
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/updates");
        let read = |name: &str| std::fs::read(fixtures.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let package = read("package.bin");
        let signature = String::from_utf8(read("package-versioned.bin.sig")).unwrap_or_default();
        let unversioned = String::from_utf8(read("package.bin.sig")).unwrap_or_default();
        let stable = String::from_utf8(read("package-stable.bin.sig")).unwrap_or_default();
        let forged = String::from_utf8(read("package-forged-comment.bin.sig")).unwrap_or_default();
        let key = String::from_utf8(read("test-key.pub")).unwrap_or_default();
        let mut tampered = package.clone();
        tampered[0] ^= 1;

        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap_or_else(|e| panic!("{e}"));
        let base = format!("http://{}", server.local_addr().unwrap_or_else(|e| panic!("{e}")));
        let platform = json!({ "signature": signature.trim(), "url": format!("{base}/package.bin") });
        let tampered_platform = json!({ "signature": signature.trim(), "url": format!("{base}/tampered.bin") });
        let manifest = |entry: &serde_json::Value, version: &str| {
            let platforms: serde_json::Map<_, _> = [
                "darwin-aarch64-app",
                "darwin-x86_64-app",
                "windows-x86_64-nsis",
                "linux-x86_64-appimage",
            ]
            .into_iter()
            .map(|target| (target.to_string(), entry.clone()))
            .collect();
            json!({ "version": version, "notes": "Synthetic notes.", "pub_date": "2099-12-31T23:59:00Z", "platforms": platforms })
                .to_string()
                .into_bytes()
        };
        let version = "9.9.9-nightly.209912312359";
        let files = [
            ("/latest.json", manifest(&platform, version)),
            ("/prefixed.json", manifest(&platform, &format!("v{version}"))),
            ("/replay.json", manifest(&platform, "9.9.9-nightly.209912312400")),
            (
                "/stable.json",
                manifest(
                    &json!({ "signature": stable.trim(), "url": format!("{base}/package.bin") }),
                    "9.9.9",
                ),
            ),
            (
                "/forged.json",
                manifest(
                    &json!({ "signature": forged.trim(), "url": format!("{base}/package.bin") }),
                    "9.9.9-nightly.209912312400",
                ),
            ),
            ("/metadata.json", manifest(&platform, &format!("{version}+other"))),
            ("/tampered.json", manifest(&tampered_platform, version)),
            (
                "/unversioned.json",
                manifest(
                    &json!({ "signature": unversioned.trim(), "url": format!("{base}/package.bin") }),
                    version,
                ),
            ),
            (
                "/interrupted.json",
                manifest(
                    &json!({ "signature": signature.trim(), "url": format!("{base}/interrupted.bin") }),
                    version,
                ),
            ),
            ("/interrupted.bin", package[..package.len() / 2].to_vec()),
            ("/package.bin", package.clone()),
            ("/tampered.bin", tampered),
        ];
        let agents = std::sync::Arc::new(Mutex::new(Vec::new()));
        let seen = agents.clone();
        std::thread::spawn(move || {
            use std::io::{BufRead, BufReader, Write};
            for stream in server.incoming().flatten() {
                let mut reader = BufReader::new(&stream);
                let mut request = String::new();
                if reader.read_line(&mut request).is_err() {
                    continue;
                }
                let mut header = String::new();
                while reader.read_line(&mut header).is_ok_and(|n| n > 2) {
                    if let Some(agent) = header.to_ascii_lowercase().strip_prefix("user-agent:")
                        && let Ok(mut seen) = seen.lock()
                    {
                        seen.push(agent.trim().to_string());
                    }
                    header.clear();
                }
                let path = request.split(' ').nth(1).unwrap_or_default();
                let body = files
                    .iter()
                    .find(|(name, _)| *name == path)
                    .map(|(_, body)| body.as_slice());
                let mut stream = &stream;
                let _ = match body {
                    Some(body) => write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len() + usize::from(path == "/interrupted.bin")
                    )
                    .and_then(|()| stream.write_all(body)),
                    None => write!(
                        stream,
                        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    ),
                };
            }
        });

        let app_with_key = |pubkey: &str, current: &str, strict: bool| {
            let mut context = tauri::test::mock_context(tauri::test::noop_assets());
            context.package_info_mut().version = current.parse().unwrap_or_else(|e| panic!("{e}"));
            context.config_mut().plugins.0.insert(
                "updater".into(),
                json!({ "pubkey": pubkey, "endpoints": [], "requireSignedVersion": strict, "dangerousInsecureTransportProtocol": true }),
            );
            tauri::test::mock_builder()
                .plugin(tauri_plugin_updater::Builder::new().build())
                .build(context)
                .unwrap_or_else(|e| panic!("{e}"))
        };
        let installed_dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let installed = installed_dir.path().join("synthetic-app");
        std::fs::write(&installed, b"installed app").unwrap_or_else(|e| panic!("{e}"));
        let check = |pubkey: &str, feed: &str, current: &str, strict: bool| {
            let app = app_with_key(pubkey, current, strict);
            let url = format!("{base}/{feed}").parse().unwrap_or_else(|e| panic!("{e}"));
            let updater = updater_builder(app.handle())
                .unwrap_or_else(|e| panic!("{e}"))
                .endpoints(vec![url])
                .unwrap_or_else(|e| panic!("{e}"))
                // Each published entry names its package type, so a .deb or
                // .rpm install is never offered the AppImage.
                .target("linux-x86_64-appimage")
                .executable_path(&installed)
                .timeout(CHECK_TIMEOUT)
                .no_proxy()
                .build()
                .unwrap_or_else(|e| panic!("{e}"));
            tauri::async_runtime::block_on(updater.check()).unwrap_or_else(|e| panic!("{e}"))
        };
        let fetch = |pubkey: &str, feed: &str| {
            let update = check(pubkey, feed, "0.1.1-nightly.20261019223000", true)
                .unwrap_or_else(|| panic!("the newer synthetic version was not offered"));
            assert_eq!(
                update.timeout, None,
                "manifest budget must not limit the package download"
            );
            assert!(offer_text("0.1.0", &update).contains("Synthetic notes."));
            if cfg!(target_os = "windows") {
                assert!(offer_text("0.1.0", &update).ends_with("Install this update and restart Ibis?"));
            }
            tauri::async_runtime::block_on(update.download(|_, _| {}, || {}))
        };

        let downloaded = fetch(&key, "latest.json").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(downloaded, package);
        assert!(
            fetch(&key, "tampered.json").is_err(),
            "a changed package must be refused"
        );
        // A key the package was not signed with.
        let other_key = String::from_utf8(read("other-key.pub")).unwrap_or_default();
        assert!(
            fetch(&other_key, "latest.json").is_err(),
            "a package signed with another key must be refused"
        );
        assert_eq!(fetch(&key, "prefixed.json").unwrap_or_else(|e| panic!("{e}")), package);
        assert!(matches!(
            fetch(&key, "replay.json"),
            Err(tauri_plugin_updater::Error::SignedVersionMismatch { .. })
        ));
        assert!(matches!(
            fetch(&key, "unversioned.json"),
            Err(tauri_plugin_updater::Error::MissingSignedVersion)
        ));
        assert!(
            fetch(&key, "forged.json").is_err(),
            "trusted comment must be authenticated"
        );
        assert!(
            fetch(&key, "metadata.json").is_err(),
            "build metadata is part of the signed version"
        );
        // Legacy 0.1.x clients accept the bound signature without the flag.
        let legacy =
            check(&key, "latest.json", "0.1.1-nightly.20261019223000", false).unwrap_or_else(|| panic!("legacy offer"));
        assert!(legacy.verify(&package).is_ok());
        let invalid = check(&key, "tampered.json", "0.1.1-nightly.20261019223000", true)
            .unwrap_or_else(|| panic!("tampered offer"));
        let finished = AtomicBool::new(false);
        assert!(
            tauri::async_runtime::block_on(invalid.download(|_, _| {}, || finished.store(true, Ordering::SeqCst)))
                .is_err()
        );
        assert!(
            finished.load(Ordering::SeqCst),
            "download finish precedes signature refusal"
        );
        // Both directions of channel substitution, and a genuine downgrade.
        assert!(check(&key, "stable.json", "0.1.1-nightly.20261019223000", true).is_none());
        assert!(check(&key, "latest.json", "0.1.1", true).is_none());
        assert!(check(&key, "latest.json", "9.9.9-nightly.209912312400", true).is_none());
        assert!(check(&key, "stable.json", "10.0.0", true).is_none());
        let stable_update = check(&key, "stable.json", "0.1.1", true).unwrap_or_else(|| panic!("stable offer"));
        assert!(stable_update.verify(&package).is_ok());
        // A cut-off download must fail before any installer is reached; a
        // later complete download succeeds. Cached partial bytes also fail.
        let interrupted = check(&key, "interrupted.json", "0.1.1-nightly.20261019223000", true)
            .unwrap_or_else(|| panic!("interrupted offer"));
        assert!(tauri::async_runtime::block_on(interrupted.download_and_install(|_, _| {}, || {})).is_err());
        assert!(interrupted.install(&package[..package.len() / 2]).is_err());
        assert_eq!(
            std::fs::read(&installed).unwrap_or_else(|e| panic!("{e}")),
            b"installed app"
        );
        assert_eq!(fetch(&key, "latest.json").unwrap_or_else(|e| panic!("{e}")), package);
        // Every request, manifest and package alike, names Ibis and only Ibis.
        let agents = agents.lock().map(|a| a.clone()).unwrap_or_default();
        assert_eq!(agents.len(), 28, "{agents:?}");
        assert!(agents.iter().all(|agent| agent.starts_with("ibis/")), "{agents:?}");
    }
}
