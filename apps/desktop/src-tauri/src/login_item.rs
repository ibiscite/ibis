//! Starting Ibis when the person logs in (#525).
//!
//! The system keeps the answer, never `settings.json`: people also switch
//! login items off in System Settings > General > Login Items (macOS), Task
//! Manager's Startup apps (Windows) or their desktop's startup settings
//! (Linux), so every read asks the system again.
//!
//! - macOS 13 and later: `SMAppService.mainApp`, listed under Open at Login.
//! - Windows: a value in the current user's `Run` key, named after the app;
//!   Task Manager's switch for it counts.
//! - Linux: a per-channel XDG autostart file named after the app identifier.
//!
//! Ibis and Ibis Nightly have different names and identifiers, so each has
//! its own entry; debug builds use another one again.

#[cfg(any(target_os = "linux", all(test, unix)))]
mod linux;

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager};

/// Passed by the login entry (Windows and Linux), so Ibis starts without
/// showing the picker. macOS reports a login launch in its open event instead.
pub(crate) const AT_LOGIN_ARG: &str = "--at-login";

/// Why the choice is not what the person asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LoginItemProblem {
    /// This system offers no way to do it (macOS before 13).
    Unsupported,
    /// macOS registered Ibis but waits for the person to allow it.
    NeedsApproval,
    /// The system refused the change.
    Refused,
    /// The current state could not be read.
    Unreadable,
}

/// Whether Ibis starts when the person logs in, as the system reports it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub(crate) struct LoginItem {
    /// The app's name, such as "Ibis" or "Ibis Nightly", for the interface.
    pub(crate) name: String,
    pub(crate) enabled: bool,
    pub(crate) problem: Option<LoginItemProblem>,
}

/// What the system reported, before it is named for the interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Reading {
    pub(crate) enabled: bool,
    pub(crate) problem: Option<LoginItemProblem>,
}

/// A failed system call. Its details are dropped on purpose: they can name
/// the person's home folder.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Failed;

/// One platform's login entry.
pub(crate) trait Entry: Send + Sync {
    fn is_enabled(&self) -> Result<bool, Failed>;
    fn enable(&self) -> Result<(), Failed>;
    fn disable(&self) -> Result<(), Failed>;
    /// Registered, but the system waits for the person to allow it (macOS).
    fn awaiting_approval(&self) -> bool {
        false
    }
    /// Point an entry that is on at this copy of Ibis, where that is safe.
    fn refresh(&self) -> Result<(), Failed> {
        Ok(())
    }
}

/// The state now, as the system reports it.
pub(crate) fn read(entry: Option<&dyn Entry>) -> Reading {
    match entry.map(|entry| (entry.is_enabled(), entry)) {
        None => Reading {
            enabled: false,
            problem: Some(LoginItemProblem::Unsupported),
        },
        Some((Ok(false), entry)) if entry.awaiting_approval() => Reading {
            enabled: false,
            problem: Some(LoginItemProblem::NeedsApproval),
        },
        Some((Ok(enabled), _)) => Reading { enabled, problem: None },
        Some((Err(Failed), _)) => Reading {
            enabled: false,
            problem: Some(LoginItemProblem::Unreadable),
        },
    }
}

/// Ask for `on`, then report what the system actually did. `approval` is
/// true where the system can accept a registration but wait for the person
/// to allow it (macOS). Read that status even when registration fails.
pub(crate) fn change(entry: Option<&dyn Entry>, on: bool, approval: bool) -> Reading {
    let Some(entry) = entry else {
        return read(None);
    };
    let _ = if on { entry.enable() } else { entry.disable() };
    match entry.is_enabled() {
        Ok(enabled) if enabled == on => Reading { enabled, problem: None },
        Ok(enabled) if on && approval && entry.awaiting_approval() => Reading {
            enabled,
            problem: Some(LoginItemProblem::NeedsApproval),
        },
        Ok(enabled) => Reading {
            enabled,
            problem: Some(LoginItemProblem::Refused),
        },
        Err(Failed) => Reading {
            enabled: false,
            problem: Some(LoginItemProblem::Unreadable),
        },
    }
}

/// The app's login entry, or `None` where the system has none to offer.
pub(crate) struct LoginItems {
    name: String,
    entry: Option<Box<dyn Entry>>,
    /// Serialize Settings changes with system reads.
    changing: Mutex<()>,
}

impl LoginItems {
    pub(crate) fn new(name: &str, entry: Option<Box<dyn Entry>>) -> Self {
        LoginItems {
            name: name.to_string(),
            entry,
            changing: Mutex::new(()),
        }
    }

    #[cfg(test)]
    pub(crate) fn available(&self) -> bool {
        self.entry.is_some()
    }

    fn named(&self, reading: Reading) -> LoginItem {
        LoginItem {
            name: self.name.clone(),
            enabled: reading.enabled,
            problem: reading.problem,
        }
    }

    pub(crate) fn reading(&self) -> Reading {
        read(self.entry.as_deref())
    }

    pub(crate) fn read(&self) -> LoginItem {
        let _changing = self.changing.lock();
        self.named(self.reading())
    }

    pub(crate) fn change(&self, on: bool) -> LoginItem {
        let _changing = self.changing.lock();
        self.named(change(self.entry.as_deref(), on, cfg!(target_os = "macos")))
    }

    /// An entry left by an earlier copy of Ibis (a moved AppImage, say) is
    /// pointed at this one where the platform can tell that is safe. Only
    /// when it is on: a switched-off entry stays off.
    pub(crate) fn refresh(&self) {
        let _changing = self.changing.lock();
        if let Some(entry) = &self.entry
            && entry.is_enabled() == Ok(true)
        {
            crate::diagnostics::report(
                entry.refresh(),
                crate::diagnostics::NativeIssue::LoginRegistrationRefresh,
            );
        }
    }
}

/// The name each build registers under: "Ibis", "Ibis Nightly", or a separate
/// development name so `pnpm tauri dev` never replaces an installed entry.
fn entry_name(product: &str) -> String {
    if cfg!(debug_assertions) {
        format!("{product} (development)")
    } else {
        product.to_string()
    }
}

/// The login entry for this app.
pub(crate) fn for_app(app: &AppHandle) -> LoginItems {
    let product = &app.package_info().name;
    LoginItems::new(product, platform_entry(app, &entry_name(product)))
}

#[cfg(target_os = "linux")]
fn platform_entry(app: &AppHandle, name: &str) -> Option<Box<dyn Entry>> {
    // $XDG_CONFIG_HOME, or ~/.config when it is not set (XDG autostart spec).
    let config = app.path().config_dir().ok()?;
    let identifier = &app.config().identifier;
    // An AppImage runs from a temporary mount; the entry must name the file.
    let env = app.env();
    let program = linux::launch_program(
        std::env::current_exe().ok(),
        env.appimage.map(std::path::PathBuf::from),
        env.appdir.map(std::path::PathBuf::from),
        option_env!("IBIS_PACKAGE_LAUNCHER"),
    )?;
    Some(Box::new(linux::DesktopFile::for_identity(
        &config.join("autostart"),
        identifier,
        cfg!(debug_assertions),
        name,
        program,
    )))
}

#[cfg(target_os = "macos")]
fn platform_entry(_app: &AppHandle, name: &str) -> Option<Box<dyn Entry>> {
    let mut builder = auto_launch::AutoLaunchBuilder::new();
    builder.set_app_name(name);
    // Registers the running app bundle; fails to build before macOS 13.
    builder.set_macos_launch_mode(auto_launch::MacOSLaunchMode::SMAppService);
    builder
        .build()
        .ok()
        .map(|entry| Box::new(SystemEntry(entry)) as Box<dyn Entry>)
}

#[cfg(target_os = "windows")]
fn platform_entry(_app: &AppHandle, name: &str) -> Option<Box<dyn Entry>> {
    windows_entry(name, &std::env::current_exe().ok()?).map(|entry| Box::new(SystemEntry(entry)) as Box<dyn Entry>)
}

/// Building only describes the product-named entry; it never registers it.
#[cfg(target_os = "windows")]
fn windows_entry(name: &str, program: &std::path::Path) -> Option<auto_launch::AutoLaunch> {
    let mut builder = auto_launch::AutoLaunchBuilder::new();
    // Keep the product name: both Run and StartupApproved use it, not the app ID.
    builder
        .set_app_name(name)
        // Quoted: install and home folders can contain spaces.
        .set_app_path(&format!("\"{}\"", program.to_str()?))
        .set_args(&[AT_LOGIN_ARG])
        // Writes only the current user's key, even when running as administrator.
        // Reading and removing also look at the all-users key, as auto-launch does.
        .set_windows_enable_mode(auto_launch::WindowsEnableMode::CurrentUser);
    builder.build().ok()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn platform_entry(_app: &AppHandle, _name: &str) -> Option<Box<dyn Entry>> {
    None
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
struct SystemEntry(auto_launch::AutoLaunch);

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl Entry for SystemEntry {
    fn is_enabled(&self) -> Result<bool, Failed> {
        self.0.is_enabled().map_err(|_| Failed)
    }
    fn enable(&self) -> Result<(), Failed> {
        self.0.enable().map_err(|_| Failed)
    }
    fn disable(&self) -> Result<(), Failed> {
        match self.0.disable() {
            // Windows reports an absent value as an error; it is already off.
            Err(auto_launch::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            result => result.map_err(|_| Failed),
        }
    }
    #[cfg(target_os = "macos")]
    fn awaiting_approval(&self) -> bool {
        use smappservice_rs::{AppService, ServiceStatus, ServiceType};
        AppService::new(ServiceType::MainApp).status() == ServiceStatus::RequiresApproval
    }
}

/// Whether the Windows or Linux login entry started this process.
pub(crate) fn launched_at_login(args: impl IntoIterator<Item = std::ffi::OsString>) -> bool {
    args.into_iter().skip(1).any(|arg| arg == AT_LOGIN_ARG)
}

/// Keep the picker hidden until the shortcut or the menu opens it.
pub(crate) fn start_hidden(context: &mut tauri::Context<tauri::Wry>) {
    if let Some(window) = context
        .config_mut()
        .app
        .windows
        .iter_mut()
        .find(|window| window.label == "main")
    {
        window.visible = false;
        window.focus = false;
    }
}

/// macOS starts login items with an open event that says so. Must run on the
/// main thread while the app finishes launching (Tauri's `setup`).
#[cfg(target_os = "macos")]
pub(crate) fn launched_as_macos_login_item() -> bool {
    use objc2_foundation::NSAppleEventManager;
    // Apple Event codes from AERegistry.h and AEDataModel.h.
    const OPEN_APPLICATION: u32 = u32::from_be_bytes(*b"oapp");
    const PROPERTY_DATA: u32 = u32::from_be_bytes(*b"prdt");
    const LAUNCHED_AS_LOGIN_ITEM: u32 = u32::from_be_bytes(*b"lgit");
    NSAppleEventManager::sharedAppleEventManager()
        .currentAppleEvent()
        .is_some_and(|event| {
            event.eventID() == OPEN_APPLICATION
                && event
                    .paramDescriptorForKeyword(PROPERTY_DATA)
                    .is_some_and(|data| data.enumCodeValue() == LAUNCHED_AS_LOGIN_ITEM)
        })
}

/// Read the system state for Settings.
pub(crate) fn current(app: &AppHandle) -> LoginItem {
    app.try_state::<LoginItems>()
        .map_or_else(|| unsupported(app), |items| items.read())
}

fn unsupported(app: &AppHandle) -> LoginItem {
    LoginItems::new(&app.package_info().name, None).read()
}

/// Change the choice in the system and return its actual state to Settings.
pub(crate) fn set(app: &AppHandle, on: bool) -> LoginItem {
    app.try_state::<LoginItems>()
        .map_or_else(|| unsupported(app), |items| items.change(on))
}

/// Whether Ibis starts when the person logs in, read from the system now.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn login_item(app: AppHandle) -> LoginItem {
    // Registry, file and ServiceManagement calls block; keep them off the
    // async workers.
    let fallback = unsupported(&app);
    tauri::async_runtime::spawn_blocking(move || current(&app))
        .await
        .unwrap_or(fallback)
}

/// Start Ibis when the person logs in, or stop doing so.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn set_login_item(app: AppHandle, enabled: bool) -> LoginItem {
    let fallback = unsupported(&app);
    tauri::async_runtime::spawn_blocking(move || set(&app, enabled))
        .await
        .unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A system that does what it is told, or refuses, or needs approval.
    struct FakeEntry {
        on: std::sync::Mutex<bool>,
        refuse: bool,
        approve: bool,
        registration_error: bool,
        refresh_error: bool,
        unreadable: bool,
        /// Registrations made but not allowed yet.
        pending: std::sync::atomic::AtomicBool,
        refreshed: std::sync::atomic::AtomicUsize,
        enabled: std::sync::atomic::AtomicUsize,
        disabled: std::sync::atomic::AtomicUsize,
    }

    impl FakeEntry {
        fn new(on: bool) -> Self {
            FakeEntry {
                on: std::sync::Mutex::new(on),
                refuse: false,
                approve: true,
                registration_error: false,
                refresh_error: false,
                unreadable: false,
                pending: false.into(),
                refreshed: 0.into(),
                enabled: 0.into(),
                disabled: 0.into(),
            }
        }
        fn set(&self, on: bool) -> Result<(), Failed> {
            if self.refuse {
                return Err(Failed);
            }
            *self.on.lock().map_err(|_| Failed)? = on && self.approve;
            self.pending
                .store(on && !self.approve, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }

    impl Entry for FakeEntry {
        fn is_enabled(&self) -> Result<bool, Failed> {
            if self.unreadable {
                return Err(Failed);
            }
            self.on.lock().map(|on| *on).map_err(|_| Failed)
        }
        fn enable(&self) -> Result<(), Failed> {
            self.enabled.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.set(true)?;
            if self.registration_error { Err(Failed) } else { Ok(()) }
        }
        fn disable(&self) -> Result<(), Failed> {
            self.disabled.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.set(false)
        }
        fn awaiting_approval(&self) -> bool {
            self.pending.load(std::sync::atomic::Ordering::SeqCst)
        }
        fn refresh(&self) -> Result<(), Failed> {
            self.refreshed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if self.refresh_error { Err(Failed) } else { Ok(()) }
        }
    }

    const fn item(enabled: bool, problem: Option<LoginItemProblem>) -> Reading {
        Reading { enabled, problem }
    }

    #[test]
    fn reading_reports_the_system_state() {
        assert_eq!(read(Some(&FakeEntry::new(true))), item(true, None));
        assert_eq!(read(Some(&FakeEntry::new(false))), item(false, None));
        assert_eq!(read(None), item(false, Some(LoginItemProblem::Unsupported)));
        let broken = FakeEntry {
            unreadable: true,
            ..FakeEntry::new(true)
        };
        // Never shown as on when that cannot be confirmed.
        assert_eq!(read(Some(&broken)), item(false, Some(LoginItemProblem::Unreadable)));
    }

    #[test]
    fn a_change_the_system_accepts_has_no_problem() {
        for on in [true, false] {
            let entry = FakeEntry::new(!on);
            assert_eq!(change(Some(&entry), on, false), item(on, None));
            // Asking again for what is already so is fine too.
            assert_eq!(change(Some(&entry), on, false), item(on, None));
        }
    }

    #[test]
    fn a_refusal_shows_the_real_state() {
        for on in [true, false] {
            let entry = FakeEntry {
                refuse: true,
                ..FakeEntry::new(!on)
            };
            assert_eq!(
                change(Some(&entry), on, true),
                item(!on, Some(LoginItemProblem::Refused))
            );
        }
    }

    #[test]
    fn a_registration_waiting_for_approval_says_so_only_where_that_exists() {
        let waiting = FakeEntry {
            approve: false,
            ..FakeEntry::new(false)
        };
        assert_eq!(
            change(Some(&waiting), true, true),
            item(false, Some(LoginItemProblem::NeedsApproval))
        );
        assert_eq!(
            change(Some(&waiting), true, false),
            item(false, Some(LoginItemProblem::Refused))
        );
    }

    #[test]
    fn a_failed_registration_still_reads_its_new_approval_status() {
        let waiting = FakeEntry {
            approve: false,
            registration_error: true,
            ..FakeEntry::new(false)
        };
        assert_eq!(read(Some(&waiting)), item(false, None));
        assert_eq!(
            change(Some(&waiting), true, true),
            item(false, Some(LoginItemProblem::NeedsApproval))
        );
        assert_eq!(read(Some(&waiting)), item(false, Some(LoginItemProblem::NeedsApproval)));
        assert_eq!(
            change(Some(&waiting), true, false),
            item(false, Some(LoginItemProblem::Refused))
        );
        assert_eq!(change(Some(&waiting), false, true), item(false, None));
    }

    #[test]
    fn an_inactive_registration_without_pending_approval_is_refused() {
        struct InactiveEntry;
        impl Entry for InactiveEntry {
            fn is_enabled(&self) -> Result<bool, Failed> {
                Ok(false)
            }
            fn enable(&self) -> Result<(), Failed> {
                Ok(())
            }
            fn disable(&self) -> Result<(), Failed> {
                Ok(())
            }
        }
        assert_eq!(
            change(Some(&InactiveEntry), true, true),
            item(false, Some(LoginItemProblem::Refused))
        );
    }

    #[test]
    fn an_unreadable_outcome_is_not_shown_as_on() {
        let entry = FakeEntry {
            unreadable: true,
            ..FakeEntry::new(false)
        };
        assert_eq!(
            change(Some(&entry), true, false),
            item(false, Some(LoginItemProblem::Unreadable))
        );
        assert_eq!(
            change(None, true, false),
            item(false, Some(LoginItemProblem::Unsupported))
        );
    }

    #[test]
    fn refresh_preserves_system_state_and_skips_entries_that_are_not_on() {
        let off = LoginItems::new("Ibis", Some(Box::new(FakeEntry::new(false))));
        off.refresh();
        assert_eq!(off.reading(), item(false, None));
        let on = LoginItems::new("Ibis", Some(Box::new(FakeEntry::new(true))));
        on.refresh();
        assert_eq!(on.reading(), item(true, None));
        let unsupported = LoginItems::new("Ibis", None);
        unsupported.refresh();
        assert!(!unsupported.available());
        assert_eq!(unsupported.reading(), item(false, Some(LoginItemProblem::Unsupported)));
        // Only an entry that is on is offered a refresh.
        let entry = std::sync::Arc::new(FakeEntry::new(false));
        struct Shared(std::sync::Arc<FakeEntry>);
        impl Entry for Shared {
            fn is_enabled(&self) -> Result<bool, Failed> {
                self.0.is_enabled()
            }
            fn enable(&self) -> Result<(), Failed> {
                self.0.enable()
            }
            fn disable(&self) -> Result<(), Failed> {
                self.0.disable()
            }
            fn refresh(&self) -> Result<(), Failed> {
                self.0.refresh()
            }
        }
        let items = LoginItems::new("Ibis", Some(Box::new(Shared(entry.clone()))));
        items.refresh();
        assert_eq!(entry.refreshed.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(entry.enabled.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(entry.disabled.load(std::sync::atomic::Ordering::SeqCst), 0);
        let _ = items.change(true);
        items.refresh();
        assert_eq!(entry.refreshed.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(entry.enabled.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(entry.disabled.load(std::sync::atomic::Ordering::SeqCst), 0);

        for unreadable in [false, true] {
            let entry = std::sync::Arc::new(FakeEntry {
                unreadable,
                refresh_error: true,
                ..FakeEntry::new(true)
            });
            let items = LoginItems::new("Ibis", Some(Box::new(Shared(entry.clone()))));
            items.refresh();
            assert_eq!(entry.enabled.load(std::sync::atomic::Ordering::SeqCst), 0);
            assert_eq!(entry.disabled.load(std::sync::atomic::Ordering::SeqCst), 0);
            assert_eq!(
                entry.refreshed.load(std::sync::atomic::Ordering::SeqCst),
                usize::from(!unreadable)
            );
            assert!(entry.on.lock().is_ok_and(|on| *on));
            assert_eq!(
                items.reading(),
                if unreadable {
                    item(false, Some(LoginItemProblem::Unreadable))
                } else {
                    item(true, None)
                }
            );
        }
    }

    #[test]
    fn a_registration_waiting_for_approval_still_reads_as_waiting() {
        let waiting = FakeEntry {
            approve: false,
            ..FakeEntry::new(false)
        };
        assert_eq!(read(Some(&waiting)), item(false, None));
        let _ = change(Some(&waiting), true, true);
        assert_eq!(read(Some(&waiting)), item(false, Some(LoginItemProblem::NeedsApproval)));
        let _ = change(Some(&waiting), false, true);
        assert_eq!(read(Some(&waiting)), item(false, None));
    }

    #[test]
    fn the_interface_gets_the_apps_own_name() {
        let items = LoginItems::new("Ibis Nightly", Some(Box::new(FakeEntry::new(true))));
        assert_eq!(
            items.read(),
            LoginItem {
                name: "Ibis Nightly".into(),
                enabled: true,
                problem: None
            }
        );
        assert_eq!(items.change(false).name, "Ibis Nightly");
    }

    #[test]
    fn only_the_at_login_argument_after_the_program_counts() {
        let args = |list: &[&str]| list.iter().map(std::ffi::OsString::from).collect::<Vec<_>>();
        assert!(launched_at_login(args(&["ibis", "--at-login"])));
        assert!(!launched_at_login(args(&["ibis"])));
        assert!(!launched_at_login(args(&["--at-login"])));
        assert!(!launched_at_login(args(&["ibis", "--at-login=1", "at-login"])));
    }

    #[test]
    fn development_builds_never_share_the_installed_entry() {
        let name = entry_name("Ibis");
        assert_eq!(name == "Ibis", !cfg!(debug_assertions));
        assert_ne!(entry_name("Ibis"), entry_name("Ibis Nightly"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_entry_keeps_product_names_and_quoted_login_command() -> Result<(), Box<dyn std::error::Error>> {
        let configs: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../../../fixtures/startup/windows-identities.json"))?;
        let program = std::path::Path::new(r"C:\Synthetic Apps\Ibis\ibis.exe");
        for config in configs {
            let identifier = config["identifier"].as_str().ok_or("fixture identifier")?;
            let product = config["productName"].as_str().ok_or("fixture product name")?;
            for name in [product.to_string(), entry_name(product)] {
                let entry = windows_entry(&name, program).ok_or("build without registry access")?;
                assert_eq!(entry.get_app_name(), name);
                assert_ne!(entry.get_app_name(), identifier);
                assert_eq!(entry.get_app_path(), r#""C:\Synthetic Apps\Ibis\ibis.exe""#);
                assert_eq!(entry.get_args(), &[AT_LOGIN_ARG]);
            }
        }
        Ok(())
    }
}
