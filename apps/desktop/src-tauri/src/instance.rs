//! Decisions for a launch handed to the running app; the launcher exits in the plugin.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ibis_core::{atomic_file::AtomicFile, contract::Fingerprint};
use semver::Version;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

use crate::diagnostics::{NativeIssue, checked, report};

pub(crate) fn install(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder
        .manage(Mutex::new(Handoff::default()))
        // The native plugin only forwards argv/cwd. Publish before it can exit,
        // separately for each executable and app identifier (stable/Nightly).
        .plugin(
            tauri::plugin::Builder::<_, ()>::new("launch-version")
                .setup(|app, _| {
                    let published = (|| {
                        let cache = app.path().app_cache_dir().map_err(io::Error::other)?;
                        let executable = std::env::args_os()
                            .next()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned();
                        let cwd = std::env::current_dir()?.to_string_lossy().into_owned();
                        publish(&cache, &[executable], &cwd, &app.package_info().version.to_string())
                    })();
                    report(published, NativeIssue::Startup);
                    Ok(())
                })
                .build(),
        )
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            let handle = app.clone();
            // Serialize handoff decisions and presentation with shortcuts and OS reopen events.
            report(
                app.run_on_main_thread(move || hand_over(&handle, Launch { args, cwd })),
                NativeIssue::PickerEvent,
            );
        }))
}

fn version_path(cache: &Path, args: &[String], cwd: &str) -> Option<PathBuf> {
    // Use precisely the launch identity the plugin forwards, including relative
    // argv[0]. No executable lookup: an AppImage mount may already be gone.
    let executable = Path::new(cwd).join(args.first()?);
    let key = Fingerprint::of_bytes(executable.to_string_lossy().as_bytes());
    Some(cache.join("launches").join(key.as_str()))
}

fn publish(cache: &Path, args: &[String], cwd: &str, version: &str) -> io::Result<()> {
    let path = version_path(cache, args, cwd).ok_or(io::ErrorKind::InvalidInput)?;
    std::fs::create_dir_all(cache.join("launches"))?;
    AtomicFile::new(&path, version.as_bytes())?.replace()
}

fn launched_version(cache: &Path, args: &[String], cwd: &str) -> Option<String> {
    let path = version_path(cache, args, cwd)?;
    let mut version = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(256)
        .read_to_string(&mut version)
        .ok()?;
    Version::parse(&version).ok()?;
    Some(version)
}

fn needs_settings(app: &AppHandle) -> bool {
    if crate::settings_window::is_open(app) {
        return true;
    }
    #[cfg(target_os = "macos")]
    if crate::tray::hidden() {
        return true;
    }
    let view = app.state::<ibis_engine::library::Owner>().view();
    view.current.is_none() && view.requested.is_none()
}

/// An OS reopen and an ordinary second launch share the same destination.
#[cfg(target_os = "macos")]
pub(crate) fn reopen(app: &AppHandle) {
    hand_over(
        app,
        Launch {
            args: Vec::new(),
            cwd: String::new(),
        },
    );
}

/// The one entry for a launch handed to this copy. One launch is answered at
/// a time: a launch that arrives during setup, or while another is being
/// answered (a window build pumps native messages), waits its turn.
fn hand_over(app: &AppHandle, launch: Launch) {
    deliver(app, launch, &mut |launch, hidden| answer(app, &launch, hidden));
}

/// Setup has finished: answer the launches that arrived during it, in order.
pub(crate) fn ready(app: &AppHandle) {
    start(app, &mut |launch, hidden| answer(app, &launch, hidden));
}

/// A launch that arrived during setup found the picker hidden, so a toggle
/// among them shows it; a later one reads the picker as it is now.
fn answer(app: &AppHandle, launch: &Launch, hidden: bool) {
    let visible = if hidden {
        None
    } else {
        app.get_webview_window("main")
            .and_then(|window| checked(window.is_visible(), NativeIssue::Visibility))
    };
    activate(app, launch, visible);
}

fn handoff<R: tauri::Runtime>(app: &AppHandle<R>) -> std::sync::MutexGuard<'_, Handoff> {
    app.state::<Mutex<Handoff>>()
        .inner()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn deliver<R: tauri::Runtime>(app: &AppHandle<R>, launch: Launch, answer: &mut dyn FnMut(Launch, bool)) {
    // Bind first: the guard must be released before answering.
    let admitted = handoff(app).admit(launch);
    if let Some(launch) = admitted {
        answer(launch, false);
        drain(app, answer);
    }
}

fn start<R: tauri::Runtime>(app: &AppHandle<R>, answer: &mut dyn FnMut(Launch, bool)) {
    handoff(app).starting = false;
    drain(app, answer);
}

/// Answer what waited, oldest first, then open the gate.
fn drain<R: tauri::Runtime>(app: &AppHandle<R>, answer: &mut dyn FnMut(Launch, bool)) {
    loop {
        let next = handoff(app).next();
        let Some((launch, hidden)) = next else { return };
        answer(launch, hidden);
    }
}

fn activate(app: &AppHandle, launch: &Launch, visible: Option<bool>) {
    let Launch { args, cwd } = launch;
    let launched = app
        .path()
        .app_cache_dir()
        .ok()
        .and_then(|cache| launched_version(&cache, args, cwd));
    let action = app.state::<Mutex<Handoff>>().lock().ok().map(|mut handoff| {
        handoff.decide(
            args,
            visible,
            needs_settings(app),
            &app.package_info().version.to_string(),
            launched.as_deref(),
        )
    });
    let Some(action) = action else { return };
    if action == Action::Ignore {
        return;
    }
    // Claim before showing the picker, so its update offers cannot take the
    // native dialog owner first. A busy chooser leaves this notice pending.
    let owner = if action == Action::Hide { None } else { take_notice(app) };
    match action {
        Action::Picker => crate::picker::show_picker(app),
        Action::Settings => crate::settings_window::open(app, None),
        Action::Hide => crate::picker::dismiss_picker(app),
        Action::Ignore => return,
    }
    if let Some(owner) = owner {
        present_notice(app, owner);
    }
}

fn take_notice(app: &AppHandle) -> Option<crate::picker::ChooserLease> {
    app.state::<Mutex<Handoff>>()
        .lock()
        .ok()?
        .claim_notice(&app.state::<crate::picker::ChooserOpen>())
}

/// Like update recovery, a busy native dialog defers this notice until the
/// next real picker presentation. No timer opens or focuses a window.
pub(crate) fn picker_presented(app: &AppHandle) {
    if let Some(owner) = take_notice(app) {
        present_notice(app, owner);
    }
}

fn present_notice(app: &AppHandle, owner: crate::picker::ChooserLease) {
    let handle = app.clone();
    report(
        app.run_on_main_thread(move || {
            let text = ibis_engine::catalogue::text("app-newer-installed", None, &[]).unwrap_or_default();
            #[cfg(target_os = "macos")]
            crate::panel::activate_for_dialog(&handle);
            handle
                .dialog()
                .message(text)
                .title(&handle.package_info().name)
                .show(move |_| drop(owner));
        }),
        NativeIssue::Window,
    );
}

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Picker,
    Settings,
    Hide,
    Ignore,
}

/// A launch of another copy, as the single-instance plugin forwards it.
#[derive(Debug, PartialEq, Eq)]
struct Launch {
    args: Vec<String>,
    cwd: String,
}

struct Handoff {
    notice: Notice,
    /// Setup has not finished.
    starting: bool,
    /// A launch is being answered, or setup has not finished.
    busy: bool,
    /// Launches waiting their turn, and whether each arrived during setup.
    waiting: std::collections::VecDeque<(Launch, bool)>,
}

impl Default for Handoff {
    fn default() -> Self {
        Self {
            notice: Notice::default(),
            starting: true,
            busy: true,
            waiting: std::collections::VecDeque::new(),
        }
    }
}

#[derive(Default, PartialEq, Eq)]
enum Notice {
    #[default]
    Unseen,
    Pending,
    Shown,
}

impl Handoff {
    /// The launch to answer now, or `None` while it waits its turn.
    fn admit(&mut self, launch: Launch) -> Option<Launch> {
        if self.busy {
            self.waiting.push_back((launch, self.starting));
            return None;
        }
        self.busy = true;
        Some(launch)
    }

    /// The next waiting launch to answer; with none left, the gate opens.
    fn next(&mut self) -> Option<(Launch, bool)> {
        let next = self.waiting.pop_front();
        self.busy = next.is_some();
        next
    }

    fn decide(
        &mut self,
        args: &[String],
        visible: Option<bool>,
        settings: bool,
        current: &str,
        launched: Option<&str>,
    ) -> Action {
        if args.iter().skip(1).any(|arg| arg == crate::login_item::AT_LOGIN_ARG) {
            return Action::Ignore;
        }
        if is_toggle(args) {
            return if visible == Some(true) {
                Action::Hide
            } else {
                Action::Picker
            };
        }
        let newer_installed = self.notice == Notice::Unseen
            && launched
                .zip(Version::parse(current).ok())
                .is_some_and(|(launched, current)| {
                    Version::parse(launched).is_ok_and(|launched| {
                        launched.cmp_precedence(&current).is_gt()
                            && launched.pre.as_str().starts_with("nightly.")
                                == current.pre.as_str().starts_with("nightly.")
                    })
                });
        if newer_installed {
            self.notice = Notice::Pending;
        }
        if settings { Action::Settings } else { Action::Picker }
    }

    fn claim_notice(&mut self, dialogs: &crate::picker::ChooserOpen) -> Option<crate::picker::ChooserLease> {
        if self.notice != Notice::Pending {
            return None;
        }
        let owner = dialogs.begin()?;
        self.notice = Notice::Shown;
        Some(owner)
    }
}

fn is_toggle(args: &[String]) -> bool {
    matches!(args, [_, command] if command == "toggle") || ibis_core::toggle::receipt_path(args).is_some()
}

/// Answer `ibis toggle` ([`ibis_core::toggle`]). Only an existing, empty
/// regular file is written, so the argument can never create or overwrite a file.
pub(crate) fn sign_receipt(path: &Path) -> io::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() != 0 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut file = std::fs::OpenOptions::new().write(true).open(path)?;
    // Check what was opened too, in case the path changed after the first check.
    let opened = file.metadata()?;
    if !opened.is_file() || opened.len() != 0 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    file.write_all(ibis_core::toggle::RECEIPT)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(command: &[&str]) -> Vec<String> {
        std::iter::once("/synthetic/Ibis app/ibis-desktop")
            .chain(command.iter().copied())
            .map(String::from)
            .collect()
    }

    #[test]
    fn a_second_launch_shows_the_picker_or_first_run_settings() {
        for visible in [Some(false), Some(true), None] {
            for settings in [false, true] {
                assert_eq!(
                    Handoff::default().decide(&args(&[]), visible, settings, "0.3.6", Some("0.3.6")),
                    if settings { Action::Settings } else { Action::Picker }
                );
            }
        }
    }

    type App = tauri::AppHandle<tauri::test::MockRuntime>;

    fn mock_app() -> tauri::App<tauri::test::MockRuntime> {
        tauri::test::mock_builder()
            .manage(Mutex::new(Handoff::default()))
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap()
    }

    fn launch(name: &str) -> Launch {
        Launch {
            args: args(&[name]),
            cwd: "/synthetic".into(),
        }
    }

    /// Stands in for `answer`: records each answer, and while answering
    /// "first" a further launch arrives, as during a window build's pump.
    fn record(app: &App, log: &std::cell::RefCell<Vec<String>>, launch: Launch, hidden: bool) {
        let name = launch.args[1].clone();
        log.borrow_mut().push(format!("start {name} hidden={hidden}"));
        if name == "first" {
            deliver(app, super::tests::launch("nested"), &mut |launch, hidden| {
                record(app, log, launch, hidden)
            });
        }
        log.borrow_mut().push(format!("end {name}"));
    }

    /// The installed-update test launched a second copy while the first was
    /// still creating its window, and the handoff read the library owner
    /// before setup had created it. The app here has no owner, settings or
    /// window at all: a launch must wait without reading any of them.
    #[test]
    fn a_launch_during_startup_waits_and_is_answered_once_ready() {
        let app = mock_app();
        let app = app.handle();
        let log = std::cell::RefCell::new(Vec::new());
        let mut answer = |launch, hidden| record(app, &log, launch, hidden);
        deliver(app, launch("early"), &mut answer);
        deliver(app, launch("toggle"), &mut answer);
        assert!(log.borrow().is_empty());
        start(app, &mut answer);
        deliver(app, launch("later"), &mut answer);
        assert_eq!(
            *log.borrow(),
            [
                "start early hidden=true",
                "end early",
                "start toggle hidden=true",
                "end toggle",
                "start later hidden=false",
                "end later",
            ]
        );
        // The picker was hidden when the toggle arrived, so it shows it.
        assert_eq!(
            Handoff::default().decide(&args(&["toggle"]), None, false, "0.3.6", None),
            Action::Picker
        );
    }

    /// Answering a launch can build a window, which pumps native messages; a
    /// launch delivered then must not start a second build inside the first.
    #[test]
    fn a_launch_during_another_waits_until_it_is_answered() {
        let app = mock_app();
        let app = app.handle();
        let log = std::cell::RefCell::new(Vec::new());
        let mut answer = |launch, hidden| record(app, &log, launch, hidden);
        start(app, &mut answer);
        deliver(app, launch("first"), &mut answer);
        deliver(app, launch("after"), &mut answer);
        assert_eq!(
            *log.borrow(),
            [
                "start first hidden=false",
                "end first",
                "start nested hidden=false",
                "end nested",
                "start after hidden=false",
                "end after",
            ]
        );
    }

    #[test]
    fn a_newer_launch_shows_one_notice_in_the_running_copy() {
        let mut handoff = Handoff::default();
        let current = "0.3.7-nightly.20261114000000";
        let newer = "0.3.7-nightly.20261115000000";
        let first = handoff.decide(&args(&[]), Some(false), false, current, Some(newer));
        assert_eq!(first, Action::Picker);
        let dialogs = crate::picker::ChooserOpen::default();
        assert!(handoff.claim_notice(&dialogs).is_some());
        let again = handoff.decide(&args(&[]), Some(false), true, current, Some(newer));
        assert_eq!(again, Action::Settings);
        assert!(handoff.claim_notice(&dialogs).is_none());
    }

    #[test]
    fn only_the_exact_toggle_command_can_hide_the_picker() {
        for args in [
            vec![],
            vec!["toggle".into()],
            args(&["--toggle"]),
            args(&["Toggle"]),
            args(&["toggle", "extra"]),
            args(&["", "toggle"]),
            args(&["切り替え"]),
            args(&[&"toggle".repeat(100_000)]),
        ] {
            assert_eq!(
                Handoff::default().decide(&args, Some(true), false, "0.3.6", None),
                Action::Picker
            );
        }
        for command in [vec!["toggle"], vec!["toggle", "--receipt", "/synthetic/receipt"]] {
            for visible in [Some(false), Some(true), None] {
                assert_eq!(
                    Handoff::default().decide(&args(&command), visible, true, "0.3.6", None),
                    if visible == Some(true) {
                        Action::Hide
                    } else {
                        Action::Picker
                    }
                );
            }
        }
    }

    #[test]
    fn a_receipt_fills_only_an_existing_empty_file() {
        use ibis_core::toggle::RECEIPT;
        let dir = tempfile::tempdir().unwrap();
        let empty = dir.path().join("empty");
        std::fs::write(&empty, b"").unwrap();
        sign_receipt(&empty).unwrap();
        assert_eq!(std::fs::read(&empty).unwrap(), RECEIPT);
        // Already answered, missing, or a folder: nothing is written.
        assert!(sign_receipt(&empty).is_err());
        assert_eq!(std::fs::read(&empty).unwrap(), RECEIPT);
        let missing = dir.path().join("missing");
        assert!(sign_receipt(&missing).is_err());
        assert!(!missing.exists());
        assert!(sign_receipt(dir.path()).is_err());
    }
}
