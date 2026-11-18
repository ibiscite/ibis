//! The output the picker starts with for the app it was opened from (#1202),
//! and the per-app choices Ibis remembers (decision 0031,
//! docs/decisions/0031-what-ibis-remembers-per-app.md).
//!
//! `apps.json`, beside `settings.json`, holds at most [`MAX_APPS`] app IDs,
//! each with its chosen output and optional paste/copy-only preference.
//! Never a window title, document name or
//! citation text. An unreadable file uses default formats and retains a durable
//! delivery-recovery refusal until each app gets an explicit delivery choice.

use ibis_engine::app_context::output;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ibis_core::destination::{AppId, AppPaste, CopyReason, PasteSafety, has_citation_plugins, lookup};
use ibis_engine::settings;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::library::CommandError;
use crate::preferences::SettingsFile;

/// Apps remembered; the least recently chosen goes first.
const MAX_APPS: usize = 200;
/// A file this large was not written by Ibis.
const MAX_BYTES: u64 = 256 * 1024;
const VERSION: u32 = 1;

/// The name both windows show, resolved when read, never saved: the installed
/// Mac bundle's display name, then the destination table, then the original ID.
pub(crate) fn display_name(app: &AppId) -> String {
    #[cfg(target_os = "macos")]
    if app.system == ibis_core::destination::System::Macos
        && let Some(name) = installed_name(&app.id)
    {
        return name;
    }
    lookup(app).map_or_else(|| app.id.clone(), |known| known.name.to_owned())
}

#[cfg(target_os = "macos")]
fn installed_name(id: &str) -> Option<String> {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSBundle, NSString};

    let url = NSWorkspace::sharedWorkspace().URLForApplicationWithBundleIdentifier(&NSString::from_str(id))?;
    if let Some(bundle) = NSBundle::bundleWithURL(&url) {
        for key in ["CFBundleDisplayName", "CFBundleName"] {
            if let Some(value) = bundle.objectForInfoDictionaryKey(&NSString::from_str(key))
                && let Some(name) = value.downcast_ref::<NSString>()
                && name.length() > 0
            {
                return Some(name.to_string());
            }
        }
    }
    url.URLByDeletingPathExtension()?
        .lastPathComponent()
        .map(|name| name.to_string())
}

/// What the picker needs to know about the app it was opened from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub(crate) struct PickerDestination {
    /// The app's own name; `None` when no app was captured.
    pub(crate) app: Option<String>,
    /// The captured OS app identifier, used to refuse a save after the target changes.
    pub(crate) app_id: Option<String>,
    /// Opaque app-location fingerprint; no path crosses IPC.
    pub(crate) app_identity: Option<String>,
    /// The engine's answer: the format the next citation uses and which level
    /// chose it (decision 0042). The picker shows it as it is.
    pub(crate) format: ibis_engine::format::FormatChoice,
    /// Your format for this kind, used by the return-to-usual action.
    pub(crate) usual_format: String,
    pub(crate) find_open: bool,
    /// This app needs a delivery choice after an unreadable per-app file; formats use defaults.
    pub(crate) unreadable: bool,
    /// The current paste policy permits an attempt; final delivery checks still apply.
    pub(crate) can_paste: bool,
    /// The separate delivery choice; never inferred from a remembered format.
    pub(crate) paste: Option<AppPaste>,
    /// An unrecognised app has a proven target but no delivery choice yet.
    pub(crate) paste_question: bool,
    /// A proven ordinary target can accept an explicit delivery choice, including recovery.
    pub(crate) can_choose_paste: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) writing: Option<ibis_engine::app_context::WritingView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) context_version: Option<(u32, u32)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) writing_context: Option<String>,
    /// The engine's answer to whether the format palette offers Projects.
    pub(crate) offers_projects: bool,
    /// How many references this app's reference list holds: those cited into
    /// it since Ibis opened. Paste Reference List is offered once there are any.
    pub(crate) cited_here: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Remembered {
    app: AppId,
    /// Formats moved to settings.json (decision 0040); one written here before
    /// is read and dropped, so the delivery choices beside it keep working.
    #[serde(default, skip_serializing, deserialize_with = "dropped")]
    output: (),
    #[serde(default, skip_serializing_if = "Option::is_none")]
    paste: Option<AppPaste>,
    /// Only unlisted apps approved for Paste need a location binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    identity: Option<String>,
    /// The one-time note about reference manager plugins has been shown.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    noted: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MemoryFile {
    version: u32,
    apps: Vec<Remembered>,
    /// Lost delivery choices remain refused until deliberately resolved.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    recovery: bool,
}

/// The per-app file, read once and written on every change.
pub(crate) struct AppMemory {
    path: PathBuf,
    /// Every app's choices, and whether lost choices still need resolving.
    state: Mutex<(Vec<Remembered>, bool)>,
}

fn dropped<'de, D: serde::Deserializer<'de>>(value: D) -> Result<(), D::Error> {
    serde::de::IgnoredAny::deserialize(value).map(|_| ())
}

fn normalized(app: &AppId) -> AppId {
    AppId {
        system: app.system,
        id: app.normalized(),
    }
}

impl AppMemory {
    /// Read `path`; a missing file is empty, anything else unreadable is
    /// reported and treated as empty.
    pub(crate) fn open(path: PathBuf) -> Self {
        let (apps, unreadable) = match read(&path) {
            Ok(file) => (file.apps, file.recovery),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (Vec::new(), false),
            Err(_) => (Vec::new(), true),
        };
        AppMemory {
            path,
            state: Mutex::new((apps, unreadable)),
        }
    }

    pub(crate) fn paste_for(&self, app: &AppId, identity: Option<&str>) -> Result<Option<AppPaste>, CopyReason> {
        let app = normalized(app);
        let state = self.state.lock().map_err(|_| CopyReason::PasteUnavailable)?;
        let entry = state.0.iter().find(|entry| entry.app == app);
        let paste = entry.and_then(|entry| entry.paste).filter(|paste| {
            *paste != AppPaste::Paste
                || lookup(&app).is_some()
                || identity.is_some() && entry.and_then(|entry| entry.identity.as_deref()) == identity
        });
        if state.1 && paste.is_none() {
            return Err(CopyReason::PasteUnavailable);
        }
        Ok(paste)
    }

    pub(crate) fn for_capture(&self, captured: &crate::paste::Captured) -> Result<Option<AppPaste>, CopyReason> {
        captured
            .target
            .app
            .as_ref()
            .map_or(Ok(None), |id| self.paste_for(id, captured.app_identity.as_deref()))
    }

    #[cfg(test)]
    pub(crate) fn paste(&self, app: &AppId) -> Result<Option<AppPaste>, CopyReason> {
        self.paste_for(app, test_identity(app).as_deref())
    }

    #[cfg(test)]
    pub(crate) fn set_choice(&self, app: &AppId, paste: AppPaste) -> io::Result<()> {
        self.set_bound_choice(app, paste, test_identity(app))
    }

    /// Remember how to deliver into `app`.
    fn set_bound_choice(&self, app: &AppId, paste: AppPaste, identity: Option<String>) -> io::Result<()> {
        let app = normalized(app);
        let paste = Some(paste);
        if paste == Some(AppPaste::Paste) && lookup(&app).is_some_and(|entry| entry.safety != PasteSafety::Normal) {
            return Err(io::Error::other("this app cannot be approved for automatic paste"));
        }
        if paste == Some(AppPaste::Paste) && lookup(&app).is_none() && identity.is_none() {
            return Err(io::Error::other("this app has no proven identity to approve"));
        }
        self.change(|apps| {
            let previous = apps.iter().find(|entry| entry.app == app);
            let identity = (paste == Some(AppPaste::Paste) && lookup(&app).is_none())
                .then_some(identity)
                .flatten();
            let noted = previous.is_some_and(|entry| entry.noted);
            apps.retain(|entry| entry.app != app);
            apps.push(Remembered {
                app,
                output: (),
                paste,
                identity,
                noted,
            });
        })
    }

    fn unreadable(&self) -> bool {
        self.state.lock().map_or(true, |state| state.1)
    }

    /// Whether a paste into `app` should show the one-time plugin note. It
    /// depends only on the app ID. Saying yes records it, so it shows once; a
    /// failed save says no, and the next paste tries again.
    pub(crate) fn take_plugin_note(&self, app: &AppId) -> bool {
        if !has_citation_plugins(app) || self.unreadable() {
            return false;
        }
        let app = normalized(app);
        let Ok(state) = self.state.lock() else { return false };
        if state.0.iter().any(|entry| entry.app == app && entry.noted) {
            return false;
        }
        drop(state);
        let mut first = false;
        let saved = self.change(|apps| {
            if let Some(entry) = apps.iter_mut().find(|entry| entry.app == app) {
                first = !entry.noted;
                entry.noted = true;
            } else {
                first = true;
                apps.push(Remembered {
                    app,
                    output: (),
                    paste: None,
                    identity: None,
                    noted: true,
                });
            }
        });
        first && saved.is_ok()
    }

    fn change(&self, change: impl FnOnce(&mut Vec<Remembered>)) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("app choices are unavailable"))?;
        let mut apps = state.0.clone();
        change(&mut apps);
        let excess = apps.len().saturating_sub(MAX_APPS);
        apps.drain(..excess);
        let recovery = state.1;
        write(&self.path, &apps, recovery)?;
        *state = (apps, recovery);
        Ok(())
    }
}

fn read(path: &Path) -> io::Result<MemoryFile> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    let mut text = String::new();
    fs::File::open(path)?.take(MAX_BYTES + 1).read_to_string(&mut text)?;
    if text.len() as u64 > MAX_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "too large"));
    }
    let mut file: MemoryFile = serde_json::from_str(&text).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
    if file.version != VERSION || file.apps.len() > MAX_APPS {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    let mut ids = std::collections::HashSet::new();
    for entry in &mut file.apps {
        entry.app = normalized(&entry.app);
        if entry.app.id.is_empty() || entry.app.id.chars().any(char::is_control) || !ids.insert(entry.app.clone()) {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        if entry
            .identity
            .as_ref()
            .is_some_and(|identity| identity.len() != 64 || !identity.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
    }
    Ok(file)
}

/// Write the whole file beside the old one, then swap it in, so a crash
/// leaves either file whole.
fn write(path: &Path, apps: &[Remembered], recovery: bool) -> io::Result<()> {
    let json = serde_json::to_string_pretty(&MemoryFile {
        version: VERSION,
        apps: apps.to_vec(),
        recovery,
    })?;
    if json.len() as u64 + 1 > MAX_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "too large"));
    }
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }
    ibis_core::atomic_file::AtomicFile::new(path, format!("{json}\n").as_bytes())?.replace()
}

/// The per-app file beside the settings file.
pub(crate) fn memory_path(settings: &Path) -> PathBuf {
    settings.with_file_name("apps.json")
}

/// Native delivery facts for the engine's captured context.
pub(crate) fn present(app: &AppHandle, context: &ibis_engine::app_context::ContextView) -> PickerDestination {
    let memory = app.state::<AppMemory>();
    let editor = app.state::<crate::editor_endpoint::Pending>();
    let last = if editor.current().is_some() {
        None
    } else {
        crate::paste::last_app(app)
    };
    let format = context.format.clone();
    let usual_format = context.usual_format.clone();
    let writing = context.writing.clone();
    let writing_context = context.writing_context.clone();
    let context_version = Some(context.context_version);
    let offers_projects = context.offers_projects;
    let Some((id, name, app_identity)) = last else {
        return PickerDestination {
            app: None,
            app_id: None,
            app_identity: None,
            format,
            usual_format,
            find_open: false,
            unreadable: false,
            can_paste: false,
            paste: None,
            paste_question: false,
            can_choose_paste: false,
            writing,
            context_version,
            writing_context,
            offers_projects,
            cited_here: 0,
        };
    };
    let can_paste = crate::paste::intends_paste(app, context.paste_enabled);
    let policy = memory.paste_for(&id, app_identity.as_deref());
    let can_choose_paste = crate::paste::may_choose_paste(app, context.paste_enabled);
    // Only an app that can answer sees the warning; the others never paste anyway.
    let unreadable = policy.is_err() && can_choose_paste;
    let ordinary = lookup(&id).is_none_or(|entry| entry.safety == PasteSafety::Normal);
    let paste = ordinary.then(|| policy.ok().flatten()).flatten();
    let paste_question = lookup(&id).is_none() && paste.is_none() && can_choose_paste;
    // The rows the list would paste: those of the open library, as `session_references` gives them.
    let cited_here = {
        use crate::library::engine_library::FileLibrary;
        let items = app
            .state::<crate::session_references::SessionReferences>()
            .lock()
            .cited_here(&id.normalized());
        app.state::<ibis_engine::library::Owner>()
            .current()
            .and_then(|file| file.session_rows(&items).ok())
            .map_or(0, |rows| rows.len() as u32)
    };
    PickerDestination {
        app: Some(name),
        app_id: Some(id.normalized()),
        app_identity,
        format,
        usual_format,
        find_open: ibis_core::destination::finds_in_app(&id),
        unreadable,
        can_paste,
        paste,
        paste_question,
        can_choose_paste,
        writing,
        context_version,
        writing_context,
        offers_projects,
        cited_here,
    }
}

/// Remember a format, a delivery choice or both for the captured app. Whichever
/// is absent keeps its saved value; only a delivery choice revokes outstanding
/// proofs. The format goes to the settings file first, then the delivery choice
/// to the per-app file; a delivery that fails after its format saved says so.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn remember_app_output<R: tauri::Runtime>(
    app: AppHandle<R>,
    memory: State<'_, AppMemory>,
    settings_file: State<'_, SettingsFile>,
    app_id: String,
    output: Option<String>,
    paste: Option<AppPaste>,
    identity: Option<String>,
) -> Result<(), AppChoiceError> {
    // Only an id the engine lists is remembered; anything else changes nothing.
    let output = output
        .map(|id| output::listed(&id).ok_or("this format is not available; choose another"))
        .transpose()
        .map_err(|_| AppChoiceError::not_saved())?;
    if output.is_none() && paste.is_none() {
        return Err(AppChoiceError::not_saved());
    }
    let target = |captured: Option<&crate::paste::Captured>| -> Result<AppId, String> {
        let captured = captured.ok_or("no app to remember a choice for")?;
        let id = captured.target.app.as_ref().ok_or("no app to remember a choice for")?;
        if id.normalized() != app_id || captured.app_identity != identity {
            return Err("the app changed; open the output menu again".into());
        }
        Ok(id.clone())
    };
    let delivery = || {
        if let Some(paste) = paste {
            crate::paste::change_destination(&app, true, |captured| {
                memory
                    .set_bound_choice(&target(captured)?, paste, identity.clone())
                    .map_err(|_| "the choice for this app could not be saved".into())
            })
            .map_err(|_| {
                if output.is_some() {
                    AppChoiceError::DeliveryNotSaved
                } else {
                    AppChoiceError::not_saved()
                }
            })?;
        }
        Ok(())
    };
    let Some(output) = output else { return delivery() };
    settings_file.owner.change_with(
        |settings| {
            crate::paste::change_destination(&app, paste.is_some(), |captured| {
                let id = target(captured)?;
                settings.remember_app_output(&id, output.id().to_owned());
                Ok(())
            })
            .map_err(|_| AppChoiceError::not_saved())
        },
        |saved| {
            saved?;
            delivery()
        },
    )
}

/// Forget only the current app's format, after checking the captured target.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn forget_app_output(
    app: AppHandle,
    settings_file: State<'_, SettingsFile>,
    app_id: String,
    identity: Option<String>,
) -> Result<(), CommandError> {
    settings_file
        .owner
        .change(|settings| {
            crate::paste::change_destination(&app, false, |captured| {
                let captured = captured.ok_or("no app to forget a format for")?;
                let id = captured.target.app.as_ref().ok_or("no app to forget a format for")?;
                if id.normalized() != app_id || captured.app_identity != identity {
                    return Err("the app changed; open the format menu again".into());
                }
                settings.forget_app_output(id);
                Ok(())
            })
            .map_err(|_| CommandError::SnapshotChanged)
        })
        .map(|_| ())
}

/// Why a choice for an app was not saved, so the picker can say which part
/// was kept without reading the words.
#[derive(Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(tag = "code", rename_all = "snake_case")]
pub(crate) enum AppChoiceError {
    NotSaved {
        problem: ibis_core::contract::Problem,
    },
    /// The format was saved; whether to paste into the app was not.
    DeliveryNotSaved,
}

impl AppChoiceError {
    fn not_saved() -> Self {
        Self::from(CommandError::Presented(ibis_core::contract::Problem {
            code: "not-saved".into(),
            info: ibis_core::contract::RecoveryInfo::new(
                "app-command-output-not-saved",
                ibis_core::contract::Recovery::Retry,
            ),
        }))
    }
}

impl From<settings::SettingsError> for AppChoiceError {
    fn from(error: settings::SettingsError) -> Self {
        Self::from(CommandError::from(error))
    }
}

impl std::error::Error for AppChoiceError {}
impl From<CommandError> for AppChoiceError {
    fn from(error: CommandError) -> Self {
        Self::NotSaved {
            problem: error.problem(),
        }
    }
}

impl std::fmt::Display for AppChoiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotSaved { problem } => f.write_str(&ibis_engine::catalogue::problem(
                problem,
                ibis_engine::catalogue::Audience::App,
            )),
            Self::DeliveryNotSaved => f.write_str("the format was saved, but the paste choice could not be"),
        }
    }
}

#[cfg(test)]
fn test_identity(app: &AppId) -> Option<String> {
    let path = if app.system == ibis_core::destination::System::Windows {
        app.id.as_str()
    } else {
        "/invented/Writer.app"
    };
    crate::paste::capture::app_identity(app.system, &app.id, path)
}

/// Read the per-app file once the settings location is known.
pub(crate) fn manage(app: &AppHandle, settings: &Path) {
    app.manage(AppMemory::open(memory_path(settings)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::destination::System;
    use ibis_engine::app_context::output::OutputRef;

    fn mac(id: &str) -> AppId {
        AppId {
            system: System::Macos,
            id: id.into(),
        }
    }

    fn word() -> AppId {
        mac("com.microsoft.word")
    }

    fn memory_in(directory: &tempfile::TempDir) -> AppMemory {
        AppMemory::open(directory.path().join("apps.json"))
    }

    fn listed(id: &str) -> Option<OutputRef> {
        Some(output::listed(id).unwrap_or_else(|| panic!("listed")))
    }

    /// The settings file and the per-app file side by side, as the app keeps them.
    struct Choices {
        directory: tempfile::TempDir,
        app: tauri::App<tauri::test::MockRuntime>,
    }

    impl Choices {
        fn new() -> io::Result<Self> {
            Self::open(tempfile::tempdir()?)
        }

        fn open(directory: tempfile::TempDir) -> io::Result<Self> {
            let app = tauri::test::mock_builder()
                .manage(SettingsFile::new(directory.path().join("settings.json")))
                .manage(memory_in(&directory))
                .manage(crate::paste::LastTarget::default())
                .build(tauri::test::mock_context(tauri::test::noop_assets()))
                .map_err(io::Error::other)?;
            Ok(Self { directory, app })
        }

        fn choose(&self, app: &AppId, output: Option<&str>, paste: Option<AppPaste>) -> Result<(), CommandError> {
            self.choose_for(app, &app.normalized(), output, paste, test_identity(app))
        }

        fn choose_for(
            &self,
            app: &AppId,
            expected: &str,
            output: Option<&str>,
            paste: Option<AppPaste>,
            identity: Option<String>,
        ) -> Result<(), CommandError> {
            let mut captured = crate::paste::Captured::unknown(app.system);
            captured.target.app = Some(app.clone());
            captured.app_identity = identity.clone();
            self.app.state::<crate::paste::LastTarget>().capture_fixture(captured);
            tauri::async_runtime::block_on(remember_app_output(
                self.app.handle().clone(),
                self.app.state(),
                self.app.state(),
                expected.into(),
                output.map(str::to_owned),
                paste,
                identity,
            ))
        }

        /// The format a restart reads for `app`.
        fn format(&self, app: &AppId) -> Option<OutputRef> {
            settings::load(&self.directory.path().join("settings.json"))
                .cites()
                .unwrap_or_else(|error| panic!("{error}"))
                .clone()
                .app_output(app)
                .and_then(output::listed)
        }

        fn reopened(&self) -> AppMemory {
            memory_in(&self.directory)
        }
    }

    #[test]
    fn the_plugin_note_shows_once_per_app_and_survives_a_restart() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let memory = memory_in(&directory);
        assert!(memory.take_plugin_note(&word()));
        assert!(!memory.take_plugin_note(&word()));
        assert!(memory.take_plugin_note(&mac("com.apple.iwork.pages")), "per app");
        assert!(!memory_in(&directory).take_plugin_note(&word()));
        Ok(())
    }

    #[test]
    fn another_executable_with_the_same_basename_does_not_inherit_paste() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let memory = AppMemory::open(directory.path().join("apps.json"));
        let first = AppId {
            system: System::Windows,
            id: r"C:\Invented\Writer\editor.exe".into(),
        };
        let other = AppId {
            system: System::Windows,
            id: r"C:\Invented\Viewer\editor.exe".into(),
        };
        memory.set_choice(&first, AppPaste::Paste)?;
        assert_eq!(memory.paste(&first), Ok(Some(AppPaste::Paste)));
        assert_eq!(memory.paste(&other), Ok(None));
        Ok(())
    }

    #[test]
    fn a_reused_mac_bundle_id_or_missing_identity_requires_new_approval() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("apps.json");
        let app = mac("org.example.writer");
        let first = crate::paste::capture::app_identity(System::Macos, &app.id, "/invented/Writer.app");
        let other = crate::paste::capture::app_identity(System::Macos, &app.id, "/invented/Viewer.app");
        let memory = AppMemory::open(path.clone());
        assert!(memory.set_bound_choice(&app, AppPaste::Paste, None).is_err());
        memory.set_bound_choice(&app, AppPaste::Paste, first.clone())?;
        let reopened = AppMemory::open(path.clone());
        assert_eq!(reopened.paste_for(&app, first.as_deref()), Ok(Some(AppPaste::Paste)));
        assert_eq!(reopened.paste_for(&app, other.as_deref()), Ok(None));
        assert_eq!(reopened.paste_for(&app, None), Ok(None));
        // A format save from the replacement program cannot carry the old approval forward.
        let choices = Choices::open(directory)?;
        choices
            .choose_for(&app, &app.normalized(), Some("pandoc"), None, other.clone())
            .map_err(io::Error::other)?;
        assert_eq!(reopened.paste_for(&app, other.as_deref()), Ok(None));
        let json = fs::read_to_string(path)?;
        assert!(!json.contains("/invented/"), "only an identity digest is saved");
        Ok(())
    }

    #[test]
    fn paste_choices_and_formats_stay_separate_and_durable() -> io::Result<()> {
        let choices = Choices::new()?;
        let app = mac("org.example.Writer");
        for paste in [AppPaste::Paste, AppPaste::CopyOnly] {
            choices
                .choose(&app, Some("pandoc"), Some(paste))
                .map_err(io::Error::other)?;
            assert_eq!(choices.format(&mac("ORG.EXAMPLE.WRITER")), listed("pandoc"));
            assert_eq!(choices.reopened().paste(&mac("ORG.EXAMPLE.WRITER")), Ok(Some(paste)));
            // A later format pick preserves delivery; a format alone never grants it.
            choices.choose(&app, Some("cite"), None).map_err(io::Error::other)?;
            assert_eq!(choices.reopened().paste(&app), Ok(Some(paste)));
            // A delivery answer keeps the format the person chose.
            choices.choose(&app, None, Some(paste)).map_err(io::Error::other)?;
            assert_eq!(choices.format(&app), listed("cite"));
        }
        let other = mac("org.example.other");
        choices.choose(&other, Some("pandoc"), None).map_err(io::Error::other)?;
        assert_eq!(choices.app.state::<AppMemory>().paste(&other), Ok(None));
        Ok(())
    }

    #[test]
    fn failed_or_stale_paste_choices_never_grant_approval() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("apps.json");
        let memory = AppMemory::open(path.clone());
        let app = mac("org.example.Writer");
        let choices = Choices::open(directory)?;
        assert!(
            choices
                .choose_for(
                    &app,
                    "org.example.other",
                    None,
                    Some(AppPaste::Paste),
                    test_identity(&app)
                )
                .is_err()
        );
        assert_eq!(memory.paste(&app), Ok(None));
        memory.set_choice(&app, AppPaste::CopyOnly)?;
        fs::remove_file(&path)?;
        fs::create_dir(&path)?;
        assert!(memory.set_choice(&app, AppPaste::Paste).is_err());
        assert_eq!(memory.paste(&app), Ok(Some(AppPaste::CopyOnly)));
        assert_eq!(AppMemory::open(path).paste(&app), Err(CopyReason::PasteUnavailable));
        Ok(())
    }

    #[test]
    fn no_refused_app_can_be_approved_by_a_destination_save() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let memory = AppMemory::open(directory.path().join("apps.json"));
        for entry in ibis_core::destination::APPS
            .iter()
            .filter(|entry| entry.safety != PasteSafety::Normal)
        {
            let app = AppId {
                system: entry.system,
                id: entry.id.into(),
            };
            assert!(memory.set_choice(&app, AppPaste::Paste).is_err(), "{}", entry.name);
            assert_eq!(memory.paste(&app), Ok(None));
        }
        Ok(())
    }

    #[test]
    fn the_file_holds_only_app_ids_and_choices_and_keeps_the_latest_apps() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = memory_path(&directory.path().join("settings.json"));
        let memory = AppMemory::open(path.clone());
        for n in 0..=MAX_APPS {
            memory
                .set_choice(&mac(&format!("org.example.app{n}")), AppPaste::CopyOnly)
                .unwrap_or_else(|e| panic!("{e}"));
        }
        let reopened = AppMemory::open(path.clone());
        assert_eq!(
            reopened.paste(&mac("org.example.app0")),
            Ok(None),
            "the oldest app is dropped"
        );
        assert_eq!(
            reopened.paste(&mac(&format!("org.example.app{MAX_APPS}"))),
            Ok(Some(AppPaste::CopyOnly))
        );
        let file: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap_or_default()).unwrap_or_default();
        assert_eq!(file["apps"].as_array().map(Vec::len), Some(MAX_APPS));
        assert_eq!(
            file["apps"][0],
            serde_json::json!({ "app": { "system": "macos", "id": "org.example.app1" }, "paste": "copy-only" })
        );
    }

    #[test]
    fn a_failed_or_oversized_save_keeps_the_previous_durable_choice() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("apps.json");
        let memory = AppMemory::open(path.clone());
        let browser = mac("example.browser");
        memory
            .set_choice(&browser, AppPaste::CopyOnly)
            .unwrap_or_else(|e| panic!("{e}"));
        let before = fs::read(&path).unwrap_or_default();
        assert!(
            memory
                .set_choice(&mac(&"x".repeat(MAX_BYTES as usize)), AppPaste::CopyOnly)
                .is_err()
        );
        assert_eq!(fs::read(&path).unwrap_or_default(), before);
        // A directory at the destination refuses the atomic replacement.
        fs::remove_file(&path).unwrap_or_else(|e| panic!("{e}"));
        fs::create_dir(&path).unwrap_or_else(|e| panic!("{e}"));
        assert!(memory.set_choice(&browser, AppPaste::Paste).is_err());
        assert_eq!(memory.paste(&browser), Ok(Some(AppPaste::CopyOnly)));
    }

    #[cfg(unix)]
    #[test]
    fn an_existing_temporary_symlink_is_never_truncated() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("apps.json");
        let occupied = directory.path().join("occupied");
        fs::write(&occupied, "synthetic existing data").unwrap_or_else(|e| panic!("{e}"));
        std::os::unix::fs::symlink(&occupied, path.with_extension("json.tmp-1")).unwrap_or_else(|e| panic!("{e}"));
        AppMemory::open(path)
            .set_choice(&mac("example.browser"), AppPaste::CopyOnly)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            fs::read_to_string(occupied).unwrap_or_default(),
            "synthetic existing data"
        );
    }

    #[test]
    fn always_copy_saves_through_the_command_with_read_only_settings() -> Result<(), Box<dyn std::error::Error>> {
        let (directory, path) = crate::settings_fixture::file();
        crate::settings_fixture::edit(&path, |settings| settings.portable.output = Some("ieee".into()))?;
        let before = fs::read(&path)?;
        let mut permissions = fs::metadata(&path)?.permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions)?;
        let choices = Choices::open(directory)?;
        let id = mac("example.writer");
        choices.choose(&id, None, Some(AppPaste::CopyOnly))?;
        assert_eq!(choices.reopened().paste(&id), Ok(Some(AppPaste::CopyOnly)));
        assert_eq!(fs::read(&path)?, before);
        assert!(!path.with_extension("json.bak").exists());
        Ok(())
    }

    #[test]
    fn always_copy_refuses_read_only_app_choices_and_keeps_the_policy_and_permissions()
    -> Result<(), Box<dyn std::error::Error>> {
        let choices = Choices::new()?;
        let id = word();
        choices.choose(&id, Some("ieee"), Some(AppPaste::Paste))?;
        let paths = ["settings.json", "apps.json"].map(|name| choices.directory.path().join(name));
        for path in &paths {
            let mut permissions = fs::metadata(path)?.permissions();
            permissions.set_readonly(true);
            fs::set_permissions(path, permissions)?;
        }
        let before = paths.each_ref().map(|path| {
            (
                fs::read(path).unwrap_or_default(),
                fs::metadata(path).map(|m| m.permissions()),
            )
        });
        assert!(matches!(
            choices.choose(&id, None, Some(AppPaste::CopyOnly)),
            Err(CommandError::Presented(_))
        ));
        assert_eq!(
            choices
                .app
                .state::<AppMemory>()
                .set_choice(&id, AppPaste::CopyOnly)
                .err()
                .map(|error| error.kind()),
            Some(io::ErrorKind::PermissionDenied)
        );
        assert_eq!(choices.app.state::<AppMemory>().paste(&id), Ok(Some(AppPaste::Paste)));
        assert_eq!(choices.reopened().paste(&id), Ok(Some(AppPaste::Paste)));
        for (path, (bytes, permissions)) in paths.iter().zip(before) {
            assert_eq!(fs::read(path)?, bytes);
            assert_eq!(fs::metadata(path)?.permissions(), permissions?);
        }
        Ok(())
    }
}
