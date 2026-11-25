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

/// The one entry for a launch handed to this copy. Before setup has finished
/// it waits in [`Handoff`]; nothing is read or shown until [`ready`].
fn hand_over(app: &AppHandle, launch: Launch) {
    let Some(launch) = admit(app, launch) else {
        return;
    };
    let visible = app
        .get_webview_window("main")
        .and_then(|window| checked(window.is_visible(), NativeIssue::Visibility));
    activate(app, &launch, visible);
}

/// Setup has finished: answer the launches that arrived during it, in order.
/// The picker was hidden when each arrived, so a toggle among them shows it.
pub(crate) fn ready(app: &AppHandle) {
    for launch in waiting(app) {
        activate(app, &launch, None);
    }
}

fn admit<R: tauri::Runtime>(app: &AppHandle<R>, launch: Launch) -> Option<Launch> {
    app.state::<Mutex<Handoff>>().lock().ok()?.admit(launch)
}

fn waiting<R: tauri::Runtime>(app: &AppHandle<R>) -> Vec<Launch> {
    app.state::<Mutex<Handoff>>()
        .lock()
        .map(|mut handoff| handoff.ready())
        .unwrap_or_default()
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
    /// `None` once setup has finished; until then, the launches waiting for it.
    starting: Option<Vec<Launch>>,
}

impl Default for Handoff {
    fn default() -> Self {
        Self {
            notice: Notice::default(),
            starting: Some(Vec::new()),
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
    /// The launch to answer now, or `None` while it waits for setup.
    fn admit(&mut self, launch: Launch) -> Option<Launch> {
        match &mut self.starting {
            Some(waiting) => {
                waiting.push(launch);
                None
            }
            None => Some(launch),
        }
    }

    /// Setup has finished; every later launch is answered as it arrives.
    fn ready(&mut self) -> Vec<Launch> {
        self.starting.take().unwrap_or_default()
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

    /// The installed-update test launched a second copy while the first was
    /// still creating its window, and the handoff read the library owner
    /// before setup had created it. The app here has no owner, settings or
    /// window at all: a launch must wait without reading any of them.
    #[test]
    fn a_launch_during_startup_waits_and_is_answered_once_ready() {
        let app = tauri::test::mock_builder()
            .manage(Mutex::new(Handoff::default()))
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let app = app.handle();
        let launch = |command: &[&str]| Launch {
            args: args(command),
            cwd: "/synthetic".into(),
        };
        assert_eq!(admit(app, launch(&[])), None);
        assert_eq!(admit(app, launch(&["toggle"])), None);
        let early = waiting(app);
        assert_eq!(early, [launch(&[]), launch(&["toggle"])]);
        // Each waited while the picker was hidden, so the toggle shows it.
        let handoff = app.state::<Mutex<Handoff>>();
        for launch in &early {
            assert_eq!(
                handoff.lock().unwrap().decide(&launch.args, None, false, "0.3.6", None),
                Action::Picker
            );
        }
        // Answered once; later launches are answered as they arrive.
        assert!(waiting(app).is_empty());
        assert_eq!(admit(app, launch(&[])), Some(launch(&[])));
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
