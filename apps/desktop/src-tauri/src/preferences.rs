//! Saving choices made in the picker to the settings file.

use crate::library::CommandError;

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use ibis_engine::settings::{self, Density};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::library::Library;
use crate::output::OutputRef;

pub(crate) use ibis_engine::settings_backup::{settings_message, settings_problem};

/// This notice is for the interface only: its settings path must not be logged.
pub(crate) fn settings_notice(path: &Path, loaded: &settings::Loaded) -> Option<String> {
    if loaded.problem.is_none()
        && let Err(problem) = look_from_settings(&loaded.settings)
    {
        return Some(problem);
    }
    let Some(problem) = loaded.problem.as_ref() else {
        // A sync service's copy is named, never merged (#1561). The scan is an
        // optional warning: a stalled synced folder only skips it, after at
        // most the file deadline.
        let scanned = path.to_path_buf();
        let copies = crate::settings_backup::within(move || settings::sync_conflicts(&scanned)).unwrap_or_default();
        return (!copies.is_empty()).then(|| {
            format!(
                "A synced copy of settings.json is beside it ({}). Ibis uses settings.json; delete the copy once you have checked it. Settings file: {}",
                copies.join(", "),
                path.display()
            )
        });
    };
    Some({
        if loaded.primary_read == settings::PrimaryRead::Missing && loaded.from_backup {
            return Some(format!(
                "settings.json is missing; using the backup. The next save will restore settings.json and keep the backup unchanged. Settings file: {}",
                path.display()
            ));
        }
        let recovery = if loaded.from_backup { "the backup" } else { "defaults" };
        let saving = "Changes are not being saved while this problem remains.";
        let problem_path = match problem {
            settings::SettingsError::Backup(_) => path.with_extension("json.bak"),
            _ => path.to_path_buf(),
        };
        format!(
            "{}; using {recovery}. {saving} Settings file: {}",
            settings_message(problem),
            problem_path.display()
        )
    })
}

/// Where the settings file is, for saving choices made in the picker.
#[derive(Clone)]
pub(crate) struct SettingsFile {
    pub(crate) path: PathBuf,
    /// Orders every save of this file by when it was asked for (decision 0040).
    owner: Arc<ibis_engine::settings::Owner>,
    /// Whether bird facts and milestones are on, as last saved or read.
    surprises: Arc<std::sync::atomic::AtomicBool>,
    receipts: Arc<Mutex<Receipts>>,
    last_valid: Arc<Mutex<PickerPreferences>>,
    sizing_revision: Arc<Mutex<u64>>,
    native: Arc<Mutex<NativeSettings>>,
    native_revision: Arc<std::sync::atomic::AtomicU64>,
    native_apply: Arc<Mutex<()>>,
    pub(crate) delights: Arc<Mutex<crate::delights::DelightOwner>>,
}

#[derive(Clone, PartialEq, Eq)]
struct NativeSettings {
    library: Option<settings::LibrarySetting>,
    profile: Option<String>,
    updates: Option<bool>,
    endpoint: Option<settings::EditorEndpoint>,
}
impl From<&settings::Settings> for NativeSettings {
    fn from(settings: &settings::Settings) -> Self {
        Self {
            library: settings.library.clone(),
            profile: settings.zotero_profile.clone(),
            updates: settings.update_checks,
            endpoint: settings.editor_endpoint.clone(),
        }
    }
}

impl SettingsFile {
    fn apply_text_size<R: tauri::Runtime>(&self, app: &AppHandle<R>, larger: bool, revision: u64) {
        let Ok(mut current) = self.sizing_revision.lock() else {
            return;
        };
        if *current > revision {
            return;
        }
        *current = revision;
        if let Some(sizing) = app.try_state::<crate::sizing::Sizing>() {
            sizing.set_larger_text(larger);
        }
    }

    pub(crate) fn new(path: PathBuf) -> Self {
        let loaded = settings::load(&path).settings;
        let surprises = loaded.small_surprises.unwrap_or(true);
        let last_valid = PickerPreferences::from_settings(&loaded);
        let native = NativeSettings::from(&loaded);
        let delights = crate::delights::DelightOwner::new(loaded.delights);
        SettingsFile {
            owner: Arc::new(ibis_engine::settings::Owner::open(path.clone())),
            path,
            surprises: Arc::new(std::sync::atomic::AtomicBool::new(surprises)),
            receipts: Arc::default(),
            last_valid: Arc::new(Mutex::new(last_valid)),
            sizing_revision: Arc::default(),
            native: Arc::new(Mutex::new(native)),
            native_revision: Arc::default(),
            native_apply: Arc::default(),
            delights: Arc::new(Mutex::new(delights)),
        }
    }

    pub(crate) fn update(
        &self,
        change: impl FnOnce(&mut settings::Settings),
    ) -> Result<settings::Settings, CommandError> {
        self.update_when(|| true, change)?
            .ok_or(CommandError::SettingsUnavailable)
    }

    /// Best-effort usage bookkeeping after a successful key citation. Copy
    /// commits run on the native thread: returning their reply (and hiding the
    /// picker) must never wait for disk or the owner's outside-edit callbacks.
    pub(crate) fn record_key_format_use(&self) {
        let _ = self.owner.submit(|settings| {
            settings.key_formats_used = true;
            Ok::<_, settings::SettingsError>(())
        });
    }

    /// Run `work` on the settings file while no save can interleave, such as a
    /// restore that must find the settings it was shown.
    pub(crate) fn exclusive<T>(&self, work: impl FnOnce(&Path) -> T) -> Result<T, CommandError> {
        self.owner.in_turn(work).ok_or(CommandError::SettingsUnavailable)
    }

    /// Conditional writes test their immutable owner after acquiring the writer.
    pub(crate) fn update_when(
        &self,
        current: impl FnOnce() -> bool,
        change: impl FnOnce(&mut settings::Settings),
    ) -> Result<Option<settings::Settings>, CommandError> {
        self.exclusive(|path| {
            if !current() {
                return Ok(None);
            }
            let before = settings::load(path).settings;
            settings::update(path, change)
                .inspect(|saved| self.remember_native_changes(&before, saved))
                .map(Some)
                .map_err(CommandError::from)
        })?
    }

    /// Hold the owner's queue until the returned sender is dropped.
    #[cfg(test)]
    pub(crate) fn hold(&self) -> std::sync::mpsc::Sender<()> {
        let (release, released) = std::sync::mpsc::channel::<()>();
        let (held, holding) = std::sync::mpsc::channel();
        let file = self.clone();
        std::thread::spawn(move || {
            let _ = file.exclusive(|_| {
                let _ = held.send(());
                let _ = released.recv();
            });
        });
        let _ = holding.recv();
        release
    }

    /// Apply edits made to the file outside Ibis while it runs (decision 0042):
    /// the windows show them as they show a change made in Settings, and a
    /// file that cannot be used is shown in the notice and never written.
    pub(crate) fn watch(&self, app: AppHandle) -> Option<ibis_engine::settings::Watching> {
        self.watch_with(app, |file, app, revision, settings| {
            file.apply_native_later(app, revision, settings)
        })
    }

    fn watch_with<R: tauri::Runtime>(
        &self,
        app: AppHandle<R>,
        native: impl Fn(&Self, &AppHandle<R>, u64, &settings::Settings) + Send + Sync + 'static,
    ) -> Option<ibis_engine::settings::Watching> {
        let file = self.clone();
        self.owner.watch(move |revision, loaded| {
            file.apply_file_edit(&app, revision, loaded, &native);
        })
    }

    fn apply_file_edit<R: tauri::Runtime>(
        &self,
        app: &AppHandle<R>,
        revision: u64,
        loaded: settings::Loaded,
        native: &impl Fn(&Self, &AppHandle<R>, u64, &settings::Settings),
    ) {
        crate::library::refresh_settings_notice(app, self);
        if loaded.problem.is_some() {
            return;
        }
        if look_from_settings(&loaded.settings).is_err() {
            return;
        }
        let preferences = PickerPreferences::from_settings(&loaded.settings);
        let fields = match self.last_valid.lock() {
            Ok(mut previous) => {
                let fields = changed_fields(*previous, preferences);
                *previous = preferences;
                fields
            }
            Err(_) => return,
        };
        self.surprises
            .store(loaded.settings.small_surprises.unwrap_or(true), Ordering::SeqCst);
        self.apply_text_size(app, preferences.larger_text, revision);
        if let Some(history) = app.try_state::<crate::session_references::SessionReferences>() {
            let _ = history.set_enabled(loaded.settings.recent_citations.unwrap_or(true));
        }
        crate::shortcut_setting::apply_file_shortcut(app, loaded.settings.shortcut.here().map(str::to_owned), revision);
        crate::tray::show_in_menu_bar(app, preferences.menu_bar, revision);
        native(self, app, revision, &loaded.settings);
        crate::diagnostics::report(
            app.emit("writings:changed", ()),
            crate::diagnostics::NativeIssue::PickerEvent,
        );
        // Native reports and formatting-only edits can repeat the same valid
        // preferences. Refresh notices/effects, but publish only real changes.
        if fields.is_empty() {
            return;
        }
        let change = PreferenceChange {
            revision,
            fields,
            preferences,
        };
        for window in ["main", "settings"] {
            crate::diagnostics::report(
                app.emit_to(window, "preferences:changed", change.clone()),
                crate::diagnostics::NativeIssue::PickerEvent,
            );
        }
    }

    /// Call in the settings turn after an app-owned native choice was saved.
    pub(crate) fn remember_native_changes(&self, before: &settings::Settings, after: &settings::Settings) {
        if let Ok(mut native) = self.native.lock() {
            if before.library != after.library {
                native.library = after.library.clone();
            }
            if before.zotero_profile != after.zotero_profile {
                native.profile = after.zotero_profile.clone();
            }
            if before.update_checks != after.update_checks {
                native.updates = after.update_checks;
            }
            if before.editor_endpoint != after.editor_endpoint {
                native.endpoint = after.editor_endpoint.clone();
            }
        }
    }

    fn apply_native_later(&self, app: &AppHandle, revision: u64, settings: &settings::Settings) {
        self.native_revision.store(revision, Ordering::SeqCst);
        let desired = NativeSettings::from(settings);
        if self.native.lock().is_ok_and(|native| *native == desired) {
            return;
        }
        let (file, app) = (self.clone(), app.clone());
        let library_request = app.state::<crate::library::LoadRequests>().0.load(Ordering::SeqCst);
        tauri::async_runtime::spawn_blocking(move || {
            // Native owners may themselves save a choice. Never call them in
            // the watch callback, which runs inside the settings owner's queue.
            let Ok(_applying) = file.native_apply.lock() else {
                return;
            };
            let endpoint = file
                .exclusive(|path| {
                    if file.native_revision.load(Ordering::SeqCst) != revision {
                        return None;
                    }
                    let loaded = settings::load(path);
                    if loaded.problem.is_some() {
                        return None;
                    }
                    let current = NativeSettings::from(&loaded.settings);
                    let Ok(mut previous) = file.native.lock() else {
                        return None;
                    };
                    if current.library == desired.library
                        && current.profile == desired.profile
                        && (previous.library != desired.library
                            || (previous.profile != desired.profile
                                && matches!(desired.library, Some(settings::LibrarySetting::Zotero(_)))))
                        && app.state::<crate::library::LoadRequests>().0.load(Ordering::SeqCst) == library_request
                    {
                        crate::library::apply_file_settings(&app, desired.library.clone(), desired.profile.clone());
                        previous.library = desired.library.clone();
                    }
                    if current.profile == desired.profile {
                        previous.profile = desired.profile.clone();
                    }
                    if current.updates == desired.updates && previous.updates != desired.updates {
                        crate::updater::setting::apply_file_setting(&app, desired.updates);
                        previous.updates = desired.updates;
                    }
                    let endpoint = (current.endpoint == desired.endpoint && previous.endpoint != desired.endpoint)
                        .then(|| desired.endpoint.clone());
                    if endpoint.is_some() {
                        previous.endpoint = desired.endpoint.clone();
                    }
                    endpoint
                })
                .ok()
                .flatten();
            // Endpoint replacement shares the UI's native lock, whose order
            // is native -> settings. It must run after the queue turn ends.
            if let Some(endpoint) = endpoint {
                crate::editor_endpoint::apply_file_setting(&app, endpoint);
            }
        });
    }

    /// The Zotero profile chosen for reading while Zotero is closed.
    pub(crate) fn zotero_profile(&self) -> Option<String> {
        settings::load(&self.path).settings.zotero_profile
    }

    /// Refresh under the library lock so a reload cannot carry forward a stale
    /// recovery warning.
    pub(crate) fn refresh_notice(&self, library: &mut Library) {
        library.settings_notice = settings_notice(&self.path, &settings::load(&self.path));
    }
}

/// Settings owns these choices until the picker consumes the same event fields.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Accent {
    #[default]
    System,
    Ibis,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct LookPreferences {
    pub(crate) appearance: Appearance,
    pub(crate) accent: Accent,
}

impl Appearance {
    pub(crate) fn is_system(&self) -> bool {
        *self == Self::System
    }
}

impl SettingsFile {
    /// The saved Appearance, for a window that shows without the picker's
    /// preferences: the Copied notice follows Ibis's Light or Dark, not only the system's.
    pub(crate) fn appearance(&self) -> Appearance {
        self.last_valid
            .lock()
            .ok()
            .and_then(|preferences| preferences.appearance)
            .unwrap_or_default()
    }
}

fn look_from_settings(settings: &settings::Settings) -> Result<LookPreferences, String> {
    let read = |key: &str| {
        settings
            .unknown
            .get(key)
            .cloned()
            .unwrap_or(serde_json::Value::String("system".into()))
    };
    Ok(LookPreferences {
        appearance: serde_json::from_value(read("appearance"))
            .map_err(|_| "The appearance setting is not valid.".to_string())?,
        accent: serde_json::from_value(read("accent")).map_err(|_| "The accent setting is not valid.".to_string())?,
    })
}

/// How the picker looks and what it formats with, remembered across restarts
/// (ADR 0020).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub(crate) struct PickerPreferences {
    /// The default output: an id from the engine's list; an id it does not
    /// list was read as the engine's default.
    #[schemars(with = "String")]
    pub(crate) output: OutputRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<String>")]
    pub(crate) key_output: Option<OutputRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) appearance: Option<Appearance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) accent: Option<Accent>,
    pub(crate) density: Density,
    pub(crate) larger_text: bool,
    /// Enter pastes into the app the picker was opened from; on by default.
    pub(crate) paste: bool,
    /// The menu bar item; on by default.
    pub(crate) menu_bar: bool,
    /// Recent citations per app; on by default. Off records nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) recent_citations: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) small_surprises: Option<bool>,
}

impl PickerPreferences {
    fn from_settings(settings: &settings::Settings) -> Self {
        let look = look_from_settings(settings).unwrap_or_default();
        PickerPreferences {
            output: crate::output::stored(ibis_engine::format::usual(settings, None).id.as_deref()),
            key_output: settings
                .key_output
                .as_deref()
                .and_then(crate::output::listed)
                .filter(|output| matches!(output.spec(), ibis_core::contract::OutputSpec::Key { .. })),
            appearance: Some(look.appearance),
            accent: Some(look.accent),
            density: settings.density.unwrap_or(Density::Comfortable),
            larger_text: settings.larger_text.unwrap_or(false),
            paste: settings.paste.unwrap_or(true),
            menu_bar: settings.menu_bar.unwrap_or(true),
            recent_citations: Some(settings.recent_citations.unwrap_or(true)),
            small_surprises: Some(settings.small_surprises.unwrap_or(true)),
        }
    }
}

/// Only a field that changed on disk can replace a window's pending choice.
fn changed_fields(before: PickerPreferences, after: PickerPreferences) -> Vec<String> {
    let before = serde_json::to_value(before).unwrap_or_default();
    let after = serde_json::to_value(after).unwrap_or_default();
    let keys: std::collections::BTreeSet<_> = before
        .as_object()
        .into_iter()
        .flat_map(|value| value.keys())
        .chain(after.as_object().into_iter().flat_map(|value| value.keys()))
        .collect();
    keys.into_iter()
        .filter(|key| before.get(*key) != after.get(*key))
        .cloned()
        .collect()
}

/// The authoritative state after one locked patch and flush.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub(crate) struct PickerSave {
    preferences: PickerPreferences,
}

/// The saved snapshot and the identity of the write that published it.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub(crate) struct PreferenceChange {
    revision: u64,
    /// The fields this save set, so a window can tell a choice from an
    /// unchanged value in the snapshot.
    fields: Vec<String>,
    #[serde(flatten)]
    preferences: PickerPreferences,
}

/// Only fields deliberately changed in one window cross the write boundary.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PickerPreferencesPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) appearance: Option<Appearance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) accent: Option<Accent>,
    /// Only an id the engine lists; any other refuses the whole change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<String>")]
    pub(crate) output: Option<OutputRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<String>")]
    pub(crate) key_output: Option<OutputRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) density: Option<Density>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) larger_text: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) paste: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) menu_bar: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) recent_citations: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) small_surprises: Option<bool>,
}

impl PickerPreferencesPatch {
    /// The names of the fields this change sets, as the interface spells them.
    fn fields(&self) -> Vec<String> {
        serde_json::to_value(self)
            .ok()
            .and_then(|value| value.as_object().map(|fields| fields.keys().cloned().collect()))
            .unwrap_or_default()
    }
}

#[cfg(test)]
impl From<PickerPreferences> for PickerPreferencesPatch {
    fn from(value: PickerPreferences) -> Self {
        Self {
            appearance: value.appearance,
            accent: value.accent,
            output: Some(value.output),
            key_output: value.key_output,
            density: Some(value.density),
            larger_text: Some(value.larger_text),
            paste: Some(value.paste),
            menu_bar: Some(value.menu_bar),
            recent_citations: value.recent_citations,
            small_surprises: value.small_surprises,
        }
    }
}

/// The preferences the picker starts with.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn picker_preferences(
    settings_file: State<'_, SettingsFile>,
) -> Result<PickerPreferences, CommandError> {
    // Off the main thread: a settings write in progress must never stall a paste.
    let settings_file = settings_file.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // In turn, so it reads every change asked for before it, and publishes
        // what it read before any later change can.
        settings_file
            .exclusive(|path| {
                let preferences = read_preferences(path)?;
                settings_file
                    .surprises
                    .store(preferences.small_surprises.unwrap_or(true), Ordering::SeqCst);
                Ok(preferences)
            })
            .map_err(|_| CommandError::SettingsUnavailable)?
    })
    .await
    .map_err(|_| CommandError::NotPrepared)?
}

fn read_preferences(path: &Path) -> Result<PickerPreferences, CommandError> {
    read_preference_settings(path).map(|saved| PickerPreferences::from_settings(&saved))
}

fn read_preference_settings(path: &Path) -> Result<settings::Settings, CommandError> {
    let loaded = settings::load(path);
    if let Some(problem) = &loaded.problem
        && !loaded.recovering()
    {
        return Err(CommandError::from(problem));
    }
    look_from_settings(&loaded.settings)
        .map_err(|_| CommandError::Presented((&settings::SettingsError::Invalid(String::new())).into()))?;
    Ok(loaded.settings)
}

/// Remember picker preferences in the order they were chosen (decision 0040).
/// Runs at IPC dispatch, so two windows and two quick choices keep their order:
/// it applies what must not wait for disk, hands the change to the settings
/// owner and returns its receipt at once. Other settings in the file are kept.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) fn save_picker_preferences<R: tauri::Runtime>(
    preferences: PickerPreferencesPatch,
    settings_file: State<'_, SettingsFile>,
    app: AppHandle<R>,
    window: tauri::Window<R>,
) -> Result<u64, CommandError> {
    let larger_text = preferences.larger_text;
    let changes_menu_bar = preferences.menu_bar.is_some();
    let fields = preferences.fields();
    // A change made in Settings reaches the picker; the picker's own are already shown.
    let from_elsewhere = window.label() != "main";
    let file = settings_file.inner().clone();
    let event_app = app.clone();
    // What follows a save runs once it is saved, whether or not a window asks
    // how it went: a window may close first.
    let receipt = settings_file.submit_preferences(preferences, move |receipt, saved| {
        let Ok(saved) = saved else { return };
        let preferences = PickerPreferences::from_settings(saved);
        if fields.iter().any(|field| field == "recent_citations")
            && let Some(history) = app.try_state::<crate::session_references::SessionReferences>()
        {
            let _ = history.set_enabled(saved.recent_citations.unwrap_or(true));
        }
        if changes_menu_bar {
            crate::tray::show_in_menu_bar(app.app_handle(), preferences.menu_bar, receipt);
        }
        let change = PreferenceChange {
            revision: receipt,
            fields,
            preferences,
        };
        crate::library::refresh_settings_notice(&app, &file);
        if from_elsewhere {
            crate::diagnostics::report(
                app.emit_to("main", "preferences:changed", change.clone()),
                crate::diagnostics::NativeIssue::PickerEvent,
            );
        }
        crate::diagnostics::report(
            app.emit_to("settings", "preferences:changed", change),
            crate::diagnostics::NativeIssue::PickerEvent,
        );
    })?;
    if let Some(larger) = larger_text {
        settings_file.apply_text_size(&event_app, larger, receipt);
    }
    Ok(receipt)
}

/// The saved preferences once a submitted change has run.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn picker_preferences_saved(
    receipt: u64,
    settings_file: State<'_, SettingsFile>,
) -> Result<PickerSave, CommandError> {
    let submitted = settings_file.take_receipt(receipt)?;
    tauri::async_runtime::spawn_blocking(move || {
        let saved = submitted.outcome.wait().unwrap_or(Err(CommandError::NotPrepared))?;
        Ok(PickerSave {
            preferences: PickerPreferences::from_settings(&saved),
        })
    })
    .await
    .map_err(|_| CommandError::NotPrepared)?
}

/// Preference changes handed to the owner, by receipt, until their window asks
/// how they went. A window that closes first never asks, so only the latest
/// [`MAX_RECEIPTS`] are kept; the change itself is saved either way.
#[derive(Default)]
struct Receipts {
    submitted: std::collections::BTreeMap<u64, Submitted>,
}

const MAX_RECEIPTS: usize = 256;

struct Submitted {
    outcome: settings::Pending<Result<settings::Settings, CommandError>>,
}

impl SettingsFile {
    pub(crate) fn surprises_enabled(&self) -> bool {
        self.surprises.load(Ordering::SeqCst)
    }

    fn submit_preferences(
        &self,
        preferences: PickerPreferencesPatch,
        after: impl FnOnce(u64, &Result<settings::Settings, CommandError>) + Send + 'static,
    ) -> Result<u64, CommandError> {
        let mut receipts = self.receipts.lock().map_err(|_| CommandError::SettingsUnavailable)?;
        let surprises = Arc::clone(&self.surprises);
        let last_valid = Arc::clone(&self.last_valid);
        let (receipt, outcome) = self.owner.submit_on_file_with_receipt(
            move |path| save_preferences_at(path, preferences),
            move |receipt, saved| {
                if let Ok(saved) = &saved {
                    surprises.store(saved.small_surprises.unwrap_or(true), Ordering::SeqCst);
                    if let Ok(mut previous) = last_valid.lock() {
                        *previous = PickerPreferences::from_settings(saved);
                    }
                }
                after(receipt, &saved);
                saved
            },
        );
        let receipt = receipt.ok_or(CommandError::SettingsUnavailable)?;
        receipts.submitted.insert(receipt, Submitted { outcome });
        while receipts.submitted.len() > MAX_RECEIPTS {
            receipts.submitted.pop_first();
        }
        Ok(receipt)
    }

    fn take_receipt(&self, receipt: u64) -> Result<Submitted, CommandError> {
        self.receipts
            .lock()
            .ok()
            .and_then(|mut receipts| receipts.submitted.remove(&receipt))
            .ok_or(CommandError::NotPrepared)
    }

    #[cfg(test)]
    fn save_preferences(
        &self,
        preferences: impl Into<PickerPreferencesPatch>,
        output: Option<&str>,
    ) -> Result<settings::Settings, CommandError> {
        let mut preferences = preferences.into();
        if let Some(id) = output {
            preferences.output =
                Some(crate::output::listed(id).ok_or(ibis_core::contract::CitationError::UnsupportedOutput)?);
        }
        let receipt = self.submit_preferences(preferences, |_, _| ())?;
        self.take_receipt(receipt)?
            .outcome
            .wait()
            .unwrap_or(Err(CommandError::NotPrepared))
    }
}

/// One preference change, as the owner runs it: only the fields it names.
fn save_preferences_at(path: &Path, preferences: PickerPreferencesPatch) -> Result<settings::Settings, CommandError> {
    read_preference_settings(path)?;
    if preferences
        .output
        .is_some_and(|output| !matches!(output.spec(), ibis_core::contract::OutputSpec::Csl { .. }))
    {
        return Err(format_problem("document-format", "app-settings-error-document-format"));
    }
    if preferences
        .key_output
        .is_some_and(|output| !matches!(output.spec(), ibis_core::contract::OutputSpec::Key { .. }))
    {
        return Err(format_problem("key-format", "app-settings-error-key-format"));
    }
    if std::fs::metadata(path).is_ok_and(|metadata| metadata.permissions().readonly()) {
        return Err(CommandError::Presented(ibis_core::contract::Problem {
            code: "settings-read-only".into(),
            info: ibis_core::contract::RecoveryInfo::new(
                "app-settings-error-read-only",
                ibis_core::contract::Recovery::ReviewSettings,
                ibis_core::contract::FocusTarget::Settings,
            ),
        }));
    }
    settings::update(path, |s| {
        if let Some(appearance) = preferences.appearance {
            if appearance == Appearance::System {
                s.unknown.remove("appearance");
            } else if let Ok(value) = serde_json::to_value(appearance) {
                s.unknown.insert("appearance".into(), value);
            }
        }
        if let Some(accent) = preferences.accent {
            if accent == Accent::System {
                s.unknown.remove("accent");
            } else if let Ok(value) = serde_json::to_value(accent) {
                s.unknown.insert("accent".into(), value);
            }
        }
        if let Some(output) = preferences.output {
            s.output = Some(output.id().to_owned());
        }
        if let Some(output) = preferences.key_output {
            s.key_output = Some(output.id().to_owned());
        }
        // Defaults stay unwritten, so the file only names real choices.
        if let Some(density) = preferences.density {
            s.density = Some(density).filter(|d| *d != Density::Comfortable);
        }
        if let Some(larger) = preferences.larger_text {
            s.larger_text = Some(larger).filter(|larger| *larger);
        }
        if let Some(paste) = preferences.paste {
            s.paste = Some(paste).filter(|paste| !paste);
        }
        if let Some(enabled) = preferences.small_surprises {
            s.small_surprises = Some(enabled).filter(|on| !on);
        }
        if let Some(shown) = preferences.menu_bar {
            s.menu_bar = Some(shown).filter(|shown| !shown);
        }
        if let Some(on) = preferences.recent_citations {
            s.recent_citations = Some(on).filter(|on| !on);
        }
    })
    .map_err(CommandError::from)
}

fn format_problem(code: &str, message: &str) -> CommandError {
    use ibis_core::contract::{FocusTarget, Problem, Recovery, RecoveryInfo};
    CommandError::Presented(Problem {
        code: code.into(),
        info: RecoveryInfo::new(message, Recovery::ChooseStyle, FocusTarget::FormatPicker),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_engine::settings::LibrarySetting;
    use tauri::Manager;

    #[test]
    fn key_format_bookkeeping_returns_while_settings_are_busy_and_preserves_live_edits()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("settings.json");
        std::fs::write(&path, br#"{"output":"apa"}"#)?;
        let file = SettingsFile::new(path.clone());
        let writing = file.hold();
        let (sent, returned) = std::sync::mpsc::channel();
        let copy_file = file.clone();
        let copy = std::thread::spawn(move || {
            copy_file.record_key_format_use();
            let _ = sent.send(());
        });
        let completed = returned.recv_timeout(std::time::Duration::from_secs(3));
        let edited = std::fs::write(
            &path,
            br#"{"shortcut":"Control+Alt+F12","future_choice":{"kept":true}}"#,
        );
        drop(writing);
        copy.join().map_err(|_| "copy bookkeeping panicked")?;
        edited?;
        file.exclusive(|_| ())?;
        assert!(completed.is_ok(), "copy completion waited for settings I/O");
        let loaded = settings::load(&path);
        assert!(loaded.problem.is_none());
        assert!(loaded.settings.key_formats_used);
        assert_eq!(loaded.settings.shortcut.here(), Some("Control+Alt+F12"));
        assert_eq!(
            loaded.settings.output, None,
            "copy must not recreate a removed preference"
        );
        assert_eq!(
            loaded.settings.unknown["future_choice"],
            serde_json::json!({"kept": true})
        );
        Ok(())
    }

    #[test]
    fn key_format_bookkeeping_keeps_an_invalid_live_file_and_never_retries_the_failed_save()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("settings.json");
        std::fs::write(path.with_extension("json.bak"), b"{}")?;
        std::fs::write(&path, b"{ still editing")?;
        let file = SettingsFile::new(path.clone());
        file.record_key_format_use();
        file.exclusive(|_| ())?;
        assert_eq!(std::fs::read(&path)?, b"{ still editing");
        std::fs::write(&path, b"{}")?;
        file.exclusive(|_| ())?;
        assert!(!settings::load(&path).settings.key_formats_used);
        file.record_key_format_use();
        file.exclusive(|_| ())?;
        assert!(settings::load(&path).settings.key_formats_used);
        Ok(())
    }

    #[test]
    fn live_shortcut_edits_and_side_file_restores_leave_the_original_registered()
    -> Result<(), Box<dyn std::error::Error>> {
        use tauri_plugin_global_shortcut::Shortcut;

        for side_files_present in [false, true] {
            let directory = tempfile::tempdir()?;
            let path = directory.path().join("settings.json");
            let file = SettingsFile::new(path.clone());
            file.update(|settings| settings.shortcut.set_here(Some("Control+Shift+Y".into())))?;
            let original: Shortcut = "Control+Shift+Y".parse()?;
            let edited: Shortcut = "Control+Alt+F12".parse()?;
            if side_files_present {
                for suffix in ["json.bak", "json.tmp"] {
                    std::fs::write(path.with_extension(suffix), b"{}")?;
                }
            }
            let prior: Vec<_> = ["json", "json.bak", "json.tmp"]
                .map(|suffix| {
                    let path = path.with_extension(suffix);
                    let bytes = std::fs::read(&path).ok();
                    (path, bytes)
                })
                .into();
            let app = tauri::test::mock_builder()
                .manage(file.clone())
                .build(tauri::test::mock_context(tauri::test::noop_assets()))?;
            // Only the OS backend is simulated: use the production replacement
            // rule to track which key would actually open the picker.
            let registered = Arc::new(Mutex::new(vec![original]));
            let backend = Arc::clone(&registered);
            let _watching = file
                .watch_with(app.handle().clone(), move |_, _, _, settings| {
                    let key: Shortcut = settings
                        .shortcut
                        .here()
                        .unwrap_or_default()
                        .parse()
                        .unwrap_or_else(|error| panic!("{error}"));
                    let current = backend.lock().unwrap_or_else(|error| panic!("{error}"))[0];
                    if key != current {
                        crate::picker::replace_with(
                            Some(current),
                            key,
                            |key| {
                                backend.lock().unwrap_or_else(|error| panic!("{error}")).push(key);
                                Ok(())
                            },
                            |key| {
                                backend
                                    .lock()
                                    .unwrap_or_else(|error| panic!("{error}"))
                                    .retain(|saved| *saved != key);
                                Ok(())
                            },
                        )
                        .unwrap_or_else(|error| panic!("{error}"));
                    }
                })
                .ok_or("settings folder could not be watched")?;
            std::fs::write(&path, br#"{"shortcut":"Control+Alt+F12"}"#)?;
            // Queue barriers control observation, independently of OS timing.
            file.exclusive(|_| ())?;
            assert_eq!(*registered.lock().map_err(|_| "registration poisoned")?, [edited]);
            for (path, bytes) in prior {
                if let Some(bytes) = bytes {
                    std::fs::write(path, bytes)?;
                } else if path.exists() {
                    std::fs::remove_file(path)?;
                }
            }
            file.exclusive(|_| ())?;
            assert_eq!(settings::load(&path).settings.shortcut.here(), Some("Control+Shift+Y"));
            assert_eq!(*registered.lock().map_err(|_| "registration poisoned")?, [original]);
        }
        Ok(())
    }

    #[test]
    fn repeated_or_unchanged_live_preferences_publish_no_change_event() {
        use tauri::Listener;
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("settings.json");
        settings::ensure_file(&path).unwrap_or_else(|error| panic!("{error}"));
        let app = tauri::test::mock_builder()
            .manage(SettingsFile::new(path.clone()))
            .manage(Mutex::new(crate::library::Library::default()))
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap_or_else(|error| panic!("{error}"));
        let main = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap_or_else(|error| panic!("{error}"));
        let (sent, events) = std::sync::mpsc::channel();
        main.listen("preferences:changed", move |event| {
            let _ = sent.send(event.payload().to_owned());
        });
        let native = std::cell::Cell::new(None);
        let apply_native = |_: &SettingsFile, _: &AppHandle<_>, _, saved: &settings::Settings| {
            native.set(Some(saved.update_checks));
        };
        let file = app.state::<SettingsFile>();
        let apply = |revision, text| {
            std::fs::write(&path, text).unwrap_or_else(|error| panic!("{error}"));
            file.apply_file_edit(app.handle(), revision, settings::load(&path), &apply_native);
        };
        apply(3, r#"{"larger_text":true,"update_checks":false}"#);
        let event = events.try_recv().unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&event).unwrap_or_default()["fields"],
            serde_json::json!(["larger_text"])
        );
        // Repeat a captured report, reformat the file, then repair invalid
        // edits to the same valid choices. None changes the shown preferences.
        for (revision, text) in [
            (5, r#"{"larger_text":true,"update_checks":false}"#),
            (7, r#"{ "update_checks": false, "larger_text": true }"#),
            (9, "{ still editing"),
            (
                11,
                r#"{"appearance":"invented","larger_text":false,"update_checks":true}"#,
            ),
            (13, r#"{"larger_text":true,"update_checks":false}"#),
        ] {
            apply(revision, text);
            let unexpected = events.try_recv();
            assert!(unexpected.is_err(), "revision {revision}: {unexpected:?}");
            assert_eq!(native.get(), Some(Some(false)));
        }
        assert!(
            app.state::<Mutex<crate::library::Library>>()
                .lock()
                .unwrap_or_else(|error| panic!("{error}"))
                .settings_notice
                .is_none()
        );
        // A native-only choice still applies without a preference event.
        apply(15, r#"{"larger_text":true,"update_checks":true}"#);
        assert_eq!(native.get(), Some(Some(true)));
        assert!(events.try_recv().is_err());
        apply(17, r#"{"larger_text":false,"update_checks":true}"#);
        let event = events.try_recv().unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&event).unwrap_or_default()["larger_text"],
            false
        );
        assert_eq!(native.get(), Some(Some(true)));
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn live_file_edits_update_windows_and_native_choices_but_invalid_edits_keep_the_last_valid_state() {
        use tauri::Listener;
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("settings.json");
        settings::ensure_file(&path).unwrap_or_else(|error| panic!("{error}"));
        let app = tauri::test::mock_builder()
            .manage(SettingsFile::new(path.clone()))
            .manage(Mutex::new(crate::library::Library::default()))
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap_or_else(|error| panic!("{error}"));
        let main = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap_or_else(|error| panic!("{error}"));
        let (sent, events) = std::sync::mpsc::channel();
        main.listen("preferences:changed", move |event| {
            let _ = sent.send(event.payload().to_owned());
        });
        let native = Arc::new(Mutex::new(None));
        let applied = Arc::clone(&native);
        let file = app.state::<SettingsFile>();
        let _watching = file
            .watch_with(app.handle().clone(), move |_, _, _, saved| {
                *applied.lock().unwrap_or_else(|error| panic!("{error}")) = Some(saved.update_checks);
            })
            .unwrap_or_else(|| panic!("temporary settings folder is watchable"));
        std::fs::write(&path, br#"{"larger_text":true,"update_checks":false}"#)
            .unwrap_or_else(|error| panic!("{error}"));
        // Drain in the owner's turn, without relying on native event timing.
        file.exclusive(|_| ()).unwrap_or_else(|error| panic!("{error}"));
        let event = events
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap_or_else(|error| panic!("{error}"));
        let changed: serde_json::Value = serde_json::from_str(&event).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(changed["larger_text"], true);
        assert_eq!(changed["fields"], serde_json::json!(["larger_text"]));
        assert_eq!(
            *native.lock().unwrap_or_else(|error| panic!("{error}")),
            Some(Some(false))
        );
        std::fs::write(
            &path,
            br#"{"appearance":"invented","larger_text":false,"update_checks":true}"#,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        file.exclusive(|_| ()).unwrap_or_else(|error| panic!("{error}"));
        let unexpected = events.try_recv();
        assert!(unexpected.is_err(), "unexpected event: {unexpected:?}");
        assert_eq!(
            *native.lock().unwrap_or_else(|error| panic!("{error}")),
            Some(Some(false))
        );
        assert!(
            app.state::<Mutex<crate::library::Library>>()
                .lock()
                .unwrap_or_else(|error| panic!("{error}"))
                .settings_notice
                .is_some()
        );
        assert!(file.update(|saved| saved.menu_bar = Some(false)).is_err());
        assert!(std::fs::read_to_string(&path).unwrap_or_default().contains("invented"));
        std::fs::write(&path, br#"{"larger_text":false,"update_checks":true}"#)
            .unwrap_or_else(|error| panic!("{error}"));
        file.exclusive(|_| ()).unwrap_or_else(|error| panic!("{error}"));
        let event = events
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&event).unwrap_or_default()["larger_text"],
            false
        );
        assert_eq!(
            *native.lock().unwrap_or_else(|error| panic!("{error}")),
            Some(Some(true))
        );
    }

    #[test]
    fn a_file_edit_names_only_changed_fields_and_bad_look_values_are_reported() {
        let before = PickerPreferences::from_settings(&settings::Settings::default());
        let mut saved = settings::Settings {
            larger_text: Some(true),
            ..Default::default()
        };
        let after = PickerPreferences::from_settings(&saved);
        assert_eq!(changed_fields(before, after), ["larger_text"]);
        assert!(changed_fields(after, after).is_empty());
        saved
            .unknown
            .insert("appearance".into(), serde_json::json!("invented-invalid"));
        let loaded = settings::Loaded {
            settings: saved,
            problem: None,
            primary_read: settings::PrimaryRead::Present,
            from_backup: false,
        };
        assert_eq!(
            settings_notice(Path::new("settings.json"), &loaded).as_deref(),
            Some("The appearance setting is not valid.")
        );
        assert!(look_from_settings(&loaded.settings).is_err());
    }

    #[test]
    fn a_bad_file_edit_keeps_the_notice_in_the_last_valid_appearance() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        file.save_preferences(
            PickerPreferencesPatch {
                appearance: Some(Appearance::Dark),
                ..Default::default()
            },
            None,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        std::fs::write(&file.path, b"{ still editing").unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(file.appearance(), Appearance::Dark);
        assert!(
            file.save_preferences(
                PickerPreferencesPatch {
                    density: Some(Density::Compact),
                    ..Default::default()
                },
                None
            )
            .is_err()
        );
        assert_eq!(file.appearance(), Appearance::Dark);
    }

    fn out(id: &str) -> OutputRef {
        crate::output::listed(id).unwrap_or_else(|| panic!("{id} is listed"))
    }

    fn preference_request<R: tauri::Runtime>(
        window: &tauri::WebviewWindow<R>,
        preferences: PickerPreferences,
        output: Option<&str>,
    ) -> tauri::webview::InvokeRequest {
        let mut patch = serde_json::to_value(preferences).unwrap_or_default();
        if let Some(patch) = patch.as_object_mut() {
            match output {
                Some(id) => patch.insert("output".into(), id.into()),
                None => patch.remove("output"),
            };
        }
        let _ = window;
        command_request("save_picker_preferences", serde_json::json!({ "preferences": patch }))
    }

    /// Submit at dispatch, then ask how it went, as the interface does.
    fn save_over_ipc(
        window: &tauri::WebviewWindow<tauri::test::MockRuntime>,
        preferences: PickerPreferences,
        output: Option<&str>,
    ) -> Result<tauri::ipc::InvokeResponseBody, serde_json::Value> {
        let receipt = tauri::test::get_ipc_response(window, preference_request(window, preferences, output))?
            .deserialize::<u64>()
            .unwrap_or_else(|e| panic!("{e}"));
        tauri::test::get_ipc_response(window, saved_request(receipt))
    }

    fn saved_request(receipt: u64) -> tauri::webview::InvokeRequest {
        command_request("picker_preferences_saved", serde_json::json!({ "receipt": receipt }))
    }

    fn command_request(command: &str, body: serde_json::Value) -> tauri::webview::InvokeRequest {
        tauri::webview::InvokeRequest {
            cmd: command.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: if cfg!(windows) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .unwrap_or_else(|e| panic!("invalid fixture URL: {e}")),
            body: tauri::ipc::InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.into(),
        }
    }

    #[test]
    fn unasked_receipts_are_bounded_and_their_changes_still_saved() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        let receipts: Vec<u64> = (0..=MAX_RECEIPTS)
            .map(|_| {
                file.submit_preferences(
                    PickerPreferencesPatch {
                        density: Some(Density::Compact),
                        ..Default::default()
                    },
                    |_, _| (),
                )
                .unwrap_or_else(|e| panic!("{e}"))
            })
            .collect();
        assert!(
            file.take_receipt(receipts[0]).is_err(),
            "the oldest unasked receipt is dropped"
        );
        let last = file
            .take_receipt(receipts[MAX_RECEIPTS])
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(last.outcome.wait().is_some_and(|saved| saved.is_ok()));
        assert_eq!(
            read_preferences(&file.path).unwrap_or_else(|e| panic!("{e}")).density,
            Density::Compact
        );
    }

    #[test]
    fn choices_from_two_windows_land_in_the_order_they_were_made_whichever_reply_comes_first() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("settings.json");
        let app = tauri::test::mock_builder()
            .manage(SettingsFile::new(path.clone()))
            .build(tauri::generate_context!(test = true))
            .unwrap_or_else(|e| panic!("{e}"));
        let older_window = tauri::WebviewWindowBuilder::new(&app, "settings", Default::default())
            .build()
            .unwrap_or_else(|e| panic!("{e}"));
        let newer_window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap_or_else(|e| panic!("{e}"));
        // Dispatched in this order: Settings first, then the picker.
        let older = save_picker_preferences(
            PickerPreferencesPatch {
                density: Some(Density::Comfortable),
                accent: Some(Accent::Ibis),
                output: crate::output::listed("modern-language-association"),
                ..Default::default()
            },
            app.state::<SettingsFile>(),
            app.handle().clone(),
            older_window.as_ref().window(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let newer = save_picker_preferences(
            PickerPreferencesPatch {
                density: Some(Density::Compact),
                output: crate::output::listed("chicago-author-date"),
                ..Default::default()
            },
            app.state::<SettingsFile>(),
            app.handle().clone(),
            newer_window.as_ref().window(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert!(older < newer, "both windows share one order");
        // The newer window asks first; its choice still lands last.
        let saved = |receipt, window: &tauri::WebviewWindow<_>| {
            let _ = window;
            tauri::async_runtime::block_on(picker_preferences_saved(receipt, app.state::<SettingsFile>()))
                .unwrap_or_else(|e| panic!("{e}"))
        };
        assert_eq!(saved(newer, &newer_window).preferences.density, Density::Compact);
        assert_eq!(
            saved(older, &older_window).preferences.density,
            Density::Comfortable,
            "its own save, made first"
        );
        // Restart reads the newer choice, and keeps the older window's other field.
        let reopened = read_preferences(&path).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(reopened.density, Density::Compact);
        assert_eq!(reopened.output.id(), "chicago-author-date");
        assert_eq!(reopened.accent, Some(Accent::Ibis));
        // A receipt is answered once.
        assert!(app.state::<SettingsFile>().take_receipt(newer).is_err());
    }

    #[test]
    fn look_choices_reload_independently_and_defaults_stay_unwritten() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("settings.json");
        let file = SettingsFile::new(path.clone());
        file.update(|s| s.output = Some("modern-language-association".into()))
            .unwrap_or_else(|e| panic!("{e}"));
        file.save_preferences(
            PickerPreferencesPatch {
                appearance: Some(Appearance::Dark),
                ..Default::default()
            },
            None,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        file.save_preferences(
            PickerPreferencesPatch {
                accent: Some(Accent::Ibis),
                ..Default::default()
            },
            None,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let loaded = read_preferences(&path).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(loaded.appearance, Some(Appearance::Dark));
        assert_eq!(loaded.accent, Some(Accent::Ibis));
        file.save_preferences(
            PickerPreferencesPatch {
                appearance: Some(Appearance::System),
                accent: Some(Accent::System),
                ..Default::default()
            },
            None,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let saved = settings::load(&path).settings;
        assert!(!saved.unknown.contains_key("appearance"));
        assert!(!saved.unknown.contains_key("accent"));
        assert_eq!(saved.output.as_deref(), Some("modern-language-association"));
    }

    #[test]
    fn preferences_refuse_unreadable_invalid_and_read_only_files_without_disclosing_contents() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("settings.json");
        let file = SettingsFile::new(path.clone());
        for value in [
            r#"{"accent":"synthetic-private-value"}"#,
            r#"{"appearance":42}"#,
            "{ damaged",
        ] {
            std::fs::write(&path, value).unwrap_or_else(|e| panic!("{e}"));
            let Err(error) = read_preferences(&path) else {
                panic!("unreadable choices cannot become saved defaults");
            };
            assert!(!error.to_string().contains("synthetic-private-value"));
            assert!(
                file.save_preferences(
                    PickerPreferencesPatch {
                        density: Some(Density::Compact),
                        ..Default::default()
                    },
                    None
                )
                .is_err()
            );
            assert_eq!(std::fs::read_to_string(&path).unwrap_or_default(), value);
        }
        std::fs::write(&path, "{}").unwrap_or_else(|e| panic!("{e}"));
        let original = std::fs::metadata(&path).unwrap_or_else(|e| panic!("{e}")).permissions();
        let mut readonly = original.clone();
        readonly.set_readonly(true);
        std::fs::set_permissions(&path, readonly).unwrap_or_else(|e| panic!("{e}"));
        let refused = file.save_preferences(
            PickerPreferencesPatch {
                density: Some(Density::Compact),
                ..Default::default()
            },
            None,
        );
        std::fs::set_permissions(&path, original).unwrap_or_else(|e| panic!("{e}"));
        assert!(refused.is_err_and(|problem| problem.problem().code == "settings-read-only"));
        assert_eq!(
            read_preferences(&path).unwrap_or_else(|e| panic!("{e}")).density,
            Density::Comfortable
        );
        std::fs::remove_file(&path).unwrap_or_else(|e| panic!("{e}"));
        std::fs::create_dir(&path).unwrap_or_else(|e| panic!("{e}"));
        assert!(
            read_preferences(&path).is_err(),
            "a failed read must not silently return defaults"
        );
    }

    #[test]
    fn a_choice_reaches_the_picker_even_when_its_window_closes_before_asking() {
        use std::sync::mpsc;
        use std::time::Duration;
        use tauri::Listener;
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("settings.json");
        let app = tauri::test::mock_builder()
            .manage(SettingsFile::new(path.clone()))
            .invoke_handler(tauri::generate_handler![
                save_picker_preferences,
                picker_preferences_saved
            ])
            .build(tauri::generate_context!(test = true))
            .unwrap_or_else(|e| panic!("{e}"));
        let main = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap_or_else(|e| panic!("{e}"));
        let settings = tauri::WebviewWindowBuilder::new(&app, "settings", Default::default())
            .build()
            .unwrap_or_else(|e| panic!("{e}"));
        let (sent, events) = mpsc::channel();
        main.listen("preferences:changed", move |event| {
            let _ = sent.send(event.payload().to_string());
        });
        // Settings turns Recent citations off, then closes without asking how it went.
        let off = PickerPreferences {
            recent_citations: Some(false),
            ..PickerPreferences::from_settings(&settings::Settings::default())
        };
        tauri::test::get_ipc_response(&settings, preference_request(&settings, off, None))
            .unwrap_or_else(|e| panic!("{e:?}"));
        let payload = events
            .recv_timeout(Duration::from_secs(3))
            .unwrap_or_else(|e| panic!("the picker was never told: {e}"));
        assert_eq!(
            serde_json::from_str::<PickerPreferences>(&payload)
                .ok()
                .and_then(|seen| seen.recent_citations),
            Some(false)
        );
        assert_eq!(
            read_preferences(&path)
                .unwrap_or_else(|e| panic!("{e}"))
                .recent_citations,
            Some(false)
        );
    }

    #[test]
    fn picker_and_settings_writes_publish_preferences_to_the_other_window() {
        use std::sync::mpsc;
        use std::time::Duration;
        use tauri::Listener;
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let app = tauri::test::mock_builder()
            .manage(SettingsFile::new(directory.path().join("settings.json")))
            .invoke_handler(tauri::generate_handler![
                save_picker_preferences,
                picker_preferences_saved
            ])
            .build(tauri::generate_context!(test = true))
            .unwrap_or_else(|e| panic!("{e}"));
        let main = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap_or_else(|e| panic!("{e}"));
        let settings = tauri::WebviewWindowBuilder::new(&app, "settings", Default::default())
            .build()
            .unwrap_or_else(|e| panic!("{e}"));
        let (sent, events) = mpsc::channel();
        settings.listen("preferences:changed", move |event| {
            let _ = sent.send(event.payload().to_string());
        });
        let (sent, picker_events) = mpsc::channel();
        main.listen("preferences:changed", move |event| {
            let _ = sent.send(event.payload().to_string());
        });
        let chosen = PickerPreferences::from_settings(&settings::Settings::default());
        let mut previous_receipt = 0;
        for window in [&main, &settings] {
            let response = save_over_ipc(window, chosen, None);
            assert!(response.is_ok());
            let payload = events
                .recv_timeout(Duration::from_secs(3))
                .unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(serde_json::from_str::<PickerPreferences>(&payload).ok(), Some(chosen));
            let receipt = serde_json::from_str::<PreferenceChange>(&payload)
                .unwrap_or_else(|e| panic!("{e}"))
                .revision;
            assert!(receipt > previous_receipt);
            previous_receipt = receipt;
        }
        let payload = picker_events
            .recv_timeout(Duration::from_secs(3))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(serde_json::from_str::<PickerPreferences>(&payload).ok(), Some(chosen));
        assert_eq!(
            serde_json::from_str::<PreferenceChange>(&payload)
                .unwrap_or_else(|e| panic!("{e}"))
                .revision,
            previous_receipt
        );
    }

    #[test]
    fn preference_dispatch_returns_while_writer_is_busy_and_save_keeps_current_settings() {
        use std::sync::mpsc;
        use std::time::Duration;

        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("settings.json");
        std::fs::write(&path, r#"{"output":"nature","future_choice":"before dispatch"}"#)
            .unwrap_or_else(|e| panic!("{e}"));
        let app = tauri::test::mock_builder()
            .manage(SettingsFile::new(path.clone()))
            .invoke_handler(tauri::generate_handler![
                save_picker_preferences,
                picker_preferences_saved
            ])
            .build(tauri::generate_context!(test = true))
            .unwrap_or_else(|e| panic!("cannot build preference fixture: {e}"));
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap_or_else(|e| panic!("cannot build window fixture: {e}"));
        let file = app.state::<SettingsFile>();
        let writing = file.hold();
        let chosen = PickerPreferences {
            output: out("apa"),
            key_output: None,
            appearance: Some(Appearance::System),
            accent: Some(Accent::System),
            density: Density::Compact,
            larger_text: true,
            paste: true,
            menu_bar: true,
            recent_citations: Some(true),
            small_surprises: Some(true),
        };
        // The choice is taken at dispatch even while an earlier save holds the owner.
        let receipt = tauri::test::get_ipc_response(
            &window,
            preference_request(&window, chosen, Some("modern-language-association")),
        )
        .unwrap_or_else(|e| panic!("dispatch refused: {e:?}"))
        .deserialize::<u64>()
        .unwrap_or_else(|e| panic!("{e}"));
        let (dispatched, dispatch) = mpsc::channel();
        let (responded, response) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            window.as_ref().clone().on_message(
                saved_request(receipt),
                Box::new(move |_, _, reply, _, _| {
                    let _ = responded.send(reply);
                }),
            );
            let _ = dispatched.send(());
        });
        let returned = dispatch.recv_timeout(Duration::from_secs(3));
        let pending = matches!(response.try_recv(), Err(mpsc::TryRecvError::Empty));
        // Simulate an external edit after dispatch, before the writer can read.
        let current = serde_json::json!({
            "schema_version": 2, "output": "apa",
            "library": { "bibtex": "refs.bib" }, "shortcut": { "macos": "Alt+Space", "windows": "Alt+Space", "linux": "Alt+Space" }, "update_checks": false,
            "picker_windows": { "display-a": {
                "width": 840, "height": 600, "custom_size": true, "position": [0.1, 0.2]
            } },
            "future_choice": { "kept": [true, null, "参考"] },
        });
        let edited = std::fs::write(&path, current.to_string());
        drop(writing);
        let reply = response.recv_timeout(Duration::from_secs(3));
        // Release the lock and collect the reply even on a regression.
        assert!(returned.is_ok(), "IPC dispatch blocked behind a settings writer");
        assert!(pending, "save must wait for the existing writer");
        edited.unwrap_or_else(|e| panic!("{e}"));
        let tauri::ipc::InvokeResponse::Ok(body) = reply.unwrap_or_else(|e| panic!("save did not finish: {e}")) else {
            panic!("save unexpectedly failed");
        };
        assert_eq!(
            body.deserialize::<serde_json::Value>()
                .unwrap_or_else(|e| panic!("invalid reply: {e}")),
            serde_json::json!({"preferences": PickerPreferences { output: out("modern-language-association"), ..chosen }})
        );
        thread.join().unwrap_or_else(|_| panic!("dispatch thread panicked"));
        let loaded = settings::load(&path);
        assert!(loaded.problem.is_none());
        assert_eq!(
            PickerPreferences::from_settings(&loaded.settings),
            PickerPreferences {
                output: out(loaded.settings.output.as_deref().unwrap_or("apa")),
                ..chosen
            }
        );
        assert_eq!(loaded.settings.output.as_deref(), Some("modern-language-association"));
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default()).unwrap_or_else(|e| panic!("{e}"));
        for field in [
            "library",
            "shortcut",
            "update_checks",
            "picker_windows",
            "future_choice",
        ] {
            assert_eq!(saved[field], current[field], "queued save keeps the current {field}");
        }
    }

    #[test]
    fn preference_save_refuses_a_damaged_primary_with_or_without_a_backup() {
        for has_backup in [false, true] {
            let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
            let path = directory.path().join("settings.json");
            std::fs::write(&path, "{ damaged").unwrap_or_else(|e| panic!("{e}"));
            if has_backup {
                std::fs::write(path.with_extension("json.bak"), "{}").unwrap_or_else(|e| panic!("{e}"));
            }
            let file = SettingsFile::new(path.clone());
            let loaded = settings::load(&path);
            assert!(
                file.save_preferences(PickerPreferences::from_settings(&loaded.settings), None)
                    .is_err()
            );
            assert_eq!(std::fs::read(&path).unwrap_or_default(), b"{ damaged");
            assert!(
                settings_notice(&path, &settings::load(&path)).is_some_and(|notice| notice.contains("not being saved"))
            );
        }
    }

    #[test]
    fn preference_ipc_rejects_invalid_settings_without_private_details_or_overwriting() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("settings.json");
        let app = tauri::test::mock_builder()
            .manage(SettingsFile::new(path.clone()))
            .invoke_handler(tauri::generate_handler![
                save_picker_preferences,
                picker_preferences_saved
            ])
            .build(tauri::generate_context!(test = true))
            .unwrap_or_else(|e| panic!("cannot build preference fixture: {e}"));
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap_or_else(|e| panic!("cannot build window fixture: {e}"));
        let chosen = PickerPreferences::from_settings(&settings::Settings::default());
        for (text, expected) in [
            (r#"{"density":"synthetic-private-value"}"#, "settings-invalid"),
            (
                r#"{"schema_version":3,"private":"synthetic-private-value"}"#,
                "settings-newer",
            ),
        ] {
            std::fs::write(&path, text).unwrap_or_else(|e| panic!("{e}"));
            let result = save_over_ipc(&window, chosen, None);
            let error = result.err().unwrap_or_else(|| panic!("invalid settings were saved"));
            assert_eq!(error["code"], expected);
            assert!(error["message"]["id"].is_string());
            assert_eq!(
                error["recovery"]["action"],
                if expected == "settings-newer" {
                    "upgrade-ibis"
                } else {
                    "review-settings"
                }
            );
            assert!(!error.to_string().contains("synthetic-private-value"));
            assert_eq!(std::fs::read_to_string(&path).unwrap_or_default(), text);
        }
    }

    #[test]
    #[cfg(unix)]
    fn dangling_primary_notice_keeps_the_save_refusal_warning() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        let target = directory.path().join("synthetic-private-target");
        std::os::unix::fs::symlink(&target, &file.path).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(file.path.with_extension("json.bak"), "{}").unwrap_or_else(|e| panic!("{e}"));
        let loaded = settings::load(&file.path);
        let notice = settings_notice(&file.path, &loaded).unwrap_or_else(|| panic!("expected a recovery notice"));
        assert!(notice.contains("settings.json could not be read; using the backup"));
        assert!(notice.contains("Changes are not being saved while this problem remains."));
        assert!(!notice.contains("The next save"));
        assert!(!notice.contains("synthetic-private-target"));
        assert_eq!(
            file.save_preferences(PickerPreferences::from_settings(&loaded.settings), None)
                .err()
                .map(|error| error.problem().code),
            Some("settings-unreadable".to_string())
        );
        assert_eq!(std::fs::read_link(&file.path).unwrap_or_else(|e| panic!("{e}")), target);
        assert!(!target.exists());
        assert_eq!(
            std::fs::read(file.path.with_extension("json.bak")).unwrap_or_default(),
            b"{}"
        );
    }

    #[test]
    fn missing_primary_notices_and_failed_saves_name_the_refused_backup() {
        for (contents, expected) in [
            (
                br#"{"schema_version":99,"private":"synthetic-private-value"}"#.as_slice(),
                "settings.json.bak was written by a newer version of Ibis",
            ),
            (
                br#"{"density":"synthetic-private-value"}"#.as_slice(),
                "settings.json.bak is not valid",
            ),
            (b"\xff".as_slice(), "settings.json.bak could not be read"),
            (
                br#"{"library":{"zotero":["user",{"group":"synthetic-private-value"}]}}"#.as_slice(),
                "settings.json.bak is not valid: Zotero library entry 2 must be \"user\" or {\"group\": <non-negative integer ID>}",
            ),
        ] {
            let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
            let file = SettingsFile::new(directory.path().join("settings.json"));
            let backup = file.path.with_extension("json.bak");
            std::fs::write(&backup, contents).unwrap_or_else(|e| panic!("{e}"));
            let loaded = settings::load(&file.path);
            let notice = settings_notice(&file.path, &loaded).unwrap_or_else(|| panic!("expected a warning"));
            assert!(notice.starts_with(expected), "{notice}");
            assert!(notice.contains("using defaults"));
            assert!(notice.contains("Changes are not being saved"));
            assert!(!notice.contains("synthetic-private-value"));
            assert!(notice.ends_with(backup.to_string_lossy().as_ref()));
            for _ in 0..2 {
                assert_eq!(
                    file.save_preferences(PickerPreferences::from_settings(&loaded.settings), Some("nature"))
                        .err()
                        .map(|error| error.problem()),
                    loaded.problem.as_ref().map(|error| CommandError::from(error).problem())
                );
            }
            assert!(!file.path.exists());
            assert_eq!(std::fs::read(&backup).unwrap_or_else(|e| panic!("{e}")), contents);
        }
    }

    #[test]
    fn missing_primary_notice_explains_restoration_and_clears_after_a_successful_save() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        let backup = br#"{"output":"modern-language-association","future_choice":[true,null]}"#;
        std::fs::write(file.path.with_extension("json.bak"), backup).unwrap_or_else(|e| panic!("{e}"));
        let loaded = settings::load(&file.path);
        let notice = settings_notice(&file.path, &loaded).unwrap_or_else(|| panic!("expected a recovery notice"));
        assert!(notice.contains("settings.json is missing; using the backup"));
        assert!(notice.contains("The next save will restore settings.json and keep the backup unchanged."));
        assert!(!notice.contains("not being saved"));
        assert!(!notice.contains("invalid-N"));
        let saved = file
            .save_preferences(PickerPreferences::from_settings(&loaded.settings), None)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(saved, loaded.settings);
        let restarted = settings::load(&file.path);
        assert_eq!(restarted.settings, saved);
        assert!(settings_notice(&file.path, &restarted).is_none());
        assert_eq!(
            std::fs::read(file.path.with_extension("json.bak")).unwrap_or_default(),
            backup
        );
    }

    #[test]
    fn unreadable_settings_report_no_operating_system_payload() {
        let loaded = settings::Loaded {
            settings: settings::Settings::default(),
            problem: Some(settings::SettingsError::Unreadable(std::io::Error::other(
                "synthetic-private-detail",
            ))),
            primary_read: settings::PrimaryRead::Unreadable,
            from_backup: false,
        };
        let notice = settings_notice(Path::new("synthetic/settings.json"), &loaded)
            .unwrap_or_else(|| panic!("expected a settings recovery notice"));
        assert!(notice.contains("could not be read"));
        assert!(!notice.contains("synthetic-private-detail"));
    }

    #[test]
    fn invalid_and_newer_settings_report_the_recovery_and_file_without_raw_error_payloads() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = dir.path().join("settings.json");
        let private = "synthetic-private-payload";
        for (contents, problem) in [
            (format!(r#"{{"density":"{private}"}}"#), "not valid"),
            (r#"{"schema_version":3}"#.to_string(), "newer version"),
            (
                format!(r#"{{"schema_version":2,"library":{{"zotero":["user",{{"group":"{private}"}}]}}}}"#),
                "Zotero library entry 2 must be \"user\" or {\"group\": <non-negative integer ID>}",
            ),
        ] {
            std::fs::write(&path, &contents).unwrap_or_else(|e| panic!("{e}"));
            let loaded = settings::load(&path);
            let notice =
                settings_notice(&path, &loaded).unwrap_or_else(|| panic!("expected a settings recovery notice"));
            assert!(notice.contains(problem));
            assert!(notice.contains("defaults"));
            assert!(notice.contains("not being saved"));
            assert!(notice.contains(path.to_string_lossy().as_ref()));
            assert!(!notice.contains(private));
            let Err(failed) = SettingsFile::new(path.clone()).update(|s| s.output = Some("nature".into())) else {
                panic!("expected the invalid settings save to fail");
            };
            assert_eq!(
                failed.problem(),
                CommandError::from(
                    loaded
                        .problem
                        .as_ref()
                        .unwrap_or_else(|| panic!("expected a settings problem"))
                )
                .problem()
            );
            assert!(!serde_json::to_string(&failed).unwrap_or_default().contains(private));
            assert_eq!(
                std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{e}")),
                contents
            );
            std::fs::write(path.with_extension("json.bak"), "{}").unwrap_or_else(|e| panic!("{e}"));
            let recovered = settings::load(&path);
            let notice =
                settings_notice(&path, &recovered).unwrap_or_else(|| panic!("expected a settings recovery notice"));
            assert!(notice.contains("backup"));
            assert!(notice.contains(problem));
            assert!(!notice.contains(private));
            assert!(notice.contains("not being saved"));
            assert!(!notice.contains("settings.json.invalid-N"));
            std::fs::remove_file(path.with_extension("json.bak")).unwrap_or_else(|e| panic!("{e}"));
        }
        std::fs::remove_file(&path).unwrap_or_else(|e| panic!("{e}"));
        assert!(settings_notice(&path, &settings::load(&path)).is_none());
    }

    #[test]
    fn settings_recovery_and_preference_changes_preserve_damaged_files_and_refuse_newer_files() {
        for contents in [
            b"".as_slice(),
            b"{invalid synthetic settings",
            b"\xff",
            br#"{"schema_version":999}"#,
        ] {
            for has_backup in [false, true] {
                let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
                let file = SettingsFile::new(directory.path().join("settings.json"));
                let backup = file.path.with_extension("json.bak");
                let temporary = file.path.with_extension("json.tmp");
                let backup_contents = br#"{"schema_version":2,"output":"modern-language-association"}"#;
                std::fs::write(&file.path, contents).unwrap_or_else(|e| panic!("{e}"));
                std::fs::write(&temporary, b"synthetic unfinished save").unwrap_or_else(|e| panic!("{e}"));
                if has_backup {
                    std::fs::write(&backup, backup_contents).unwrap_or_else(|e| panic!("{e}"));
                }
                let loaded = settings::load(&file.path);
                assert!(loaded.problem.is_some());
                assert_eq!(loaded.from_backup, has_backup);
                assert_eq!(
                    loaded.settings.output.as_deref(),
                    has_backup.then_some("modern-language-association")
                );
                assert_eq!(std::fs::read(&file.path).unwrap_or_else(|e| panic!("{e}")), contents);
                let chosen = PickerPreferences::from_settings(&loaded.settings);
                let saved = file.save_preferences(chosen, Some("chicago-author-date"));
                assert!(saved.is_err());
                assert!(!file.path.with_extension("json.invalid-1").exists());
                assert_eq!(std::fs::read(&file.path).unwrap_or_else(|e| panic!("{e}")), contents);
                if has_backup {
                    assert_eq!(
                        std::fs::read(&backup).unwrap_or_else(|e| panic!("{e}")),
                        backup_contents
                    );
                } else {
                    assert!(!backup.exists());
                }
                assert_eq!(
                    std::fs::read(&temporary).unwrap_or_else(|e| panic!("{e}")),
                    b"synthetic unfinished save"
                );
            }
        }
    }

    #[test]
    fn settings_saves_from_several_threads_never_lose_each_other() {
        let dir = std::env::temp_dir().join(format!("ibis-concurrent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = SettingsFile::new(dir.join("settings.json"));
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(&file.path, "{}").unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(file.path.with_extension("json.bak"), "{}").unwrap_or_else(|e| panic!("{e}"));
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let file = file.clone();
                std::thread::spawn(move || {
                    for _ in 0..20 {
                        let saved = if i % 2 == 0 {
                            file.update(|s| s.library = Some(LibrarySetting::Bibtex(PathBuf::from("refs.bib"))))
                        } else {
                            file.update(|s| s.output = Some("modern-language-association".into()))
                        };
                        saved.unwrap_or_else(|e| panic!("{e}"));
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap_or_else(|_| panic!("a saving thread panicked"));
        }
        let loaded = settings::load(&file.path);
        assert!(loaded.problem.is_none());
        assert_eq!(
            loaded.settings.library,
            Some(LibrarySetting::Bibtex(PathBuf::from("refs.bib")))
        );
        assert_eq!(loaded.settings.output.as_deref(), Some("modern-language-association"));
        assert!(!file.path.with_extension("json.invalid-1").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn picker_preferences_save_only_real_choices_and_keep_other_settings() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        file.update(|s| s.output = Some("modern-language-association".into()))
            .unwrap_or_else(|e| panic!("{e}"));
        let chosen = PickerPreferences {
            output: out("chicago-author-date"),
            key_output: None,
            appearance: Some(Appearance::System),
            accent: Some(Accent::System),
            density: Density::Compact,
            larger_text: true,
            paste: false,
            menu_bar: true,
            recent_citations: Some(true),
            small_surprises: Some(true),
        };
        let saved = file.save_preferences(chosen, None).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(saved.paste, Some(false));
        assert_eq!(saved.output.as_deref(), Some("chicago-author-date"));
        assert_eq!(
            PickerPreferences::from_settings(&saved),
            PickerPreferences {
                output: out(saved.output.as_deref().unwrap_or("apa")),
                ..chosen
            }
        );
        let defaults = PickerPreferences {
            output: out("apa"),
            key_output: None,
            appearance: Some(Appearance::System),
            accent: Some(Accent::System),
            density: Density::Comfortable,
            larger_text: false,
            paste: true,
            menu_bar: true,
            recent_citations: Some(true),
            small_surprises: Some(true),
        };
        let saved = file.save_preferences(defaults, None).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            PickerPreferences::from_settings(&saved),
            PickerPreferences {
                output: out(saved.output.as_deref().unwrap_or("apa")),
                ..defaults
            }
        );
        let text = std::fs::read_to_string(&file.path).unwrap_or_default();
        for field in ["density", "larger_text", "paste", "menu_bar", "recent_citations"] {
            assert!(!text.contains(field), "{field} is not written for the default");
        }
    }

    #[test]
    fn a_removed_accent_reads_as_following_the_mac() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("settings.json");
        std::fs::write(&path, r#"{"accent":"purple","larger_text":true}"#).unwrap_or_else(|e| panic!("{e}"));
        let loaded = read_preferences(&path).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(loaded.accent, Some(Accent::System));
        assert!(loaded.larger_text);
    }

    #[test]
    fn an_off_choice_survives_restart_and_on_stays_unwritten() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        assert!(PickerPreferences::from_settings(&settings::Settings::default()).paste);

        let original = serde_json::json!({
            "schema_version": 2, "shortcut": { "macos": "Alt+Space", "windows": "Alt+Space", "linux": "Alt+Space" }, "library": { "bibtex": "refs.bib" },
            "output": "ieee", "update_checks": false,
            "density": "compact", "larger_text": true, "future_choice": { "kept": [true, null] },
        });
        std::fs::write(&file.path, original.to_string()).unwrap_or_else(|e| panic!("{e}"));
        let mut chosen = PickerPreferences::from_settings(&settings::load(&file.path).settings);
        chosen.paste = false;
        file.save_preferences(chosen, None).unwrap_or_else(|e| panic!("{e}"));
        let restarted = SettingsFile::new(file.path.clone());
        let loaded = settings::load(&restarted.path);
        assert!(loaded.problem.is_none());
        assert_eq!(
            PickerPreferences::from_settings(&loaded.settings),
            PickerPreferences {
                output: out(loaded.settings.output.as_deref().unwrap_or("apa")),
                ..chosen
            }
        );
        let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file.path).unwrap_or_default())
            .unwrap_or_else(|e| panic!("{e}"));
        for (field, value) in original.as_object().unwrap_or_else(|| panic!("expected an object")) {
            assert_eq!(&saved[field], value, "{field} survives the choice");
        }
        assert_eq!(saved["paste"], false);

        chosen.paste = true;
        restarted
            .save_preferences(chosen, None)
            .unwrap_or_else(|e| panic!("{e}"));
        let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file.path).unwrap_or_default())
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(saved.get("paste").is_none(), "the on default stays unwritten");
        assert_eq!(saved["future_choice"], original["future_choice"]);
    }

    #[test]
    fn document_and_key_preferences_save_independently_and_explicit_standard_choices_are_yours() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        let patch = PickerPreferencesPatch {
            output: Some(out("modern-language-association")),
            key_output: Some(out("typst")),
            ..Default::default()
        };
        file.save_preferences(patch, None).unwrap_or_else(|e| panic!("{e}"));
        let loaded = settings::load(&file.path);
        assert_eq!(loaded.settings.output.as_deref(), Some("modern-language-association"));
        assert_eq!(loaded.settings.key_output.as_deref(), Some("typst"));
        let patch = PickerPreferencesPatch {
            output: Some(out("apa")),
            key_output: Some(out("pandoc")),
            ..Default::default()
        };
        file.save_preferences(patch, None).unwrap_or_else(|e| panic!("{e}"));
        let loaded = settings::load(&file.path);
        assert_eq!(loaded.settings.output.as_deref(), Some("apa"));
        assert_eq!(loaded.settings.key_output.as_deref(), Some("pandoc"));
        assert_eq!(
            ibis_engine::format::usual(&loaded.settings, None).from,
            ibis_engine::format::FormatFrom::Yours
        );
        let tex = ibis_core::destination::AppId {
            system: ibis_core::destination::System::Macos,
            id: "TeXShop".into(),
        };
        assert_eq!(
            ibis_engine::format::usual(&loaded.settings, Some(&tex)).from,
            ibis_engine::format::FormatFrom::Yours
        );
        assert_eq!(PickerPreferences::from_settings(&loaded.settings).output.id(), "apa");
        assert!(
            file.save_preferences(
                PickerPreferencesPatch {
                    output: Some(out("typst")),
                    ..Default::default()
                },
                None
            )
            .is_err()
        );
        assert!(
            file.save_preferences(
                PickerPreferencesPatch {
                    key_output: Some(out("apa")),
                    ..Default::default()
                },
                None
            )
            .is_err()
        );
    }

    #[test]
    fn choosing_a_key_never_overwrites_invalid_newer_or_unreadable_settings() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        let chosen = PickerPreferences {
            output: out("apa"),
            key_output: None,
            appearance: Some(Appearance::System),
            accent: Some(Accent::System),
            density: Density::Comfortable,
            larger_text: false,
            paste: true,
            menu_bar: true,
            recent_citations: Some(true),
            small_surprises: Some(true),
        };
        for text in [
            r#"{"density":"synthetic-private-value"}"#,
            r#"{"larger_text":"synthetic-private-value"}"#,
            r#"{"schema_version":3,"larger_text":true,"future_choice":{"kept":true}}"#,
        ] {
            std::fs::write(&file.path, text).unwrap_or_else(|e| panic!("{e}"));
            let failed = file.save_preferences(chosen, Some("chicago-author-date"));
            assert!(failed.is_err());
            assert!(
                !failed
                    .err()
                    .unwrap_or_else(|| panic!("expected a failed settings save"))
                    .to_string()
                    .contains("synthetic-private-value")
            );
            assert_eq!(std::fs::read_to_string(&file.path).unwrap_or_default(), text);
        }
        std::fs::remove_file(&file.path).unwrap_or_else(|e| panic!("{e}"));
        std::fs::create_dir(&file.path).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            file.save_preferences(chosen, Some("chicago-author-date"))
                .err()
                .unwrap_or_else(|| panic!("expected an unreadable settings save to fail"))
                .problem()
                .code,
            "settings-unreadable"
        );
        assert!(file.path.is_dir());
    }
    #[test]
    fn field_patches_from_two_stale_windows_keep_both_intents_in_either_write_order() {
        use std::sync::mpsc;
        for first in [0, 1] {
            let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
            let file = SettingsFile::new(directory.path().join("settings.json"));
            file.update(|s| s.output = Some("apa".into()))
                .unwrap_or_else(|e| panic!("{e}"));
            let baseline = PickerPreferences::from_settings(&settings::load(&file.path).settings);
            let mut view = baseline;
            view.density = Density::Compact;
            // Each client sends only the field its user changed from the same baseline.
            let patches = [
                serde_json::json!({ "output": "chicago-author-date" }),
                serde_json::json!({ "density": view.density }),
            ];
            std::thread::scope(|scope| {
                let (completed, results) = mpsc::channel();
                let mut starts = Vec::new();
                for (index, patch) in patches.into_iter().enumerate() {
                    let (start, released) = mpsc::channel();
                    starts.push(start);
                    let file = &file;
                    let completed = completed.clone();
                    scope.spawn(move || {
                        released.recv().unwrap_or_else(|e| panic!("{e}"));
                        let patch: PickerPreferencesPatch =
                            serde_json::from_value(patch).unwrap_or_else(|e| panic!("{e}"));
                        let saved = file.save_preferences(patch, None).unwrap_or_else(|e| panic!("{e}"));
                        completed.send((index, saved)).unwrap_or_else(|e| panic!("{e}"));
                    });
                }
                starts[first].send(()).unwrap_or_else(|e| panic!("{e}"));
                let (index, _) = results.recv().unwrap_or_else(|e| panic!("{e}"));
                assert_eq!(index, first);
                starts[1 - first].send(()).unwrap_or_else(|e| panic!("{e}"));
                let (_, saved) = results.recv().unwrap_or_else(|e| panic!("{e}"));
                assert_eq!(saved.output.as_deref(), Some("chicago-author-date"));
                assert_eq!(saved.density, Some(Density::Compact));
            });
        }
    }

    #[test]
    fn an_absent_output_keeps_the_saved_one_and_an_unlisted_one_is_refused() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        file.update(|s| s.output = Some("apa".into()))
            .unwrap_or_else(|e| panic!("{e}"));
        let patch: PickerPreferencesPatch =
            serde_json::from_value(serde_json::json!({"larger_text":true})).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            file.save_preferences(patch, None)
                .unwrap_or_else(|e| panic!("{e}"))
                .output
                .as_deref(),
            Some("apa")
        );
        let patch: PickerPreferencesPatch = serde_json::from_value(serde_json::json!({"output":"chicago-author-date"}))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            file.save_preferences(patch, None)
                .unwrap_or_else(|e| panic!("{e}"))
                .output
                .as_deref(),
            Some("chicago-author-date")
        );
        // Only an id the engine lists crosses the boundary; the old shapes do not.
        for refused in [
            serde_json::json!({"output":"no-such-output"}),
            serde_json::json!({"output":{"key":"chicago-author-date"}}),
            serde_json::json!({"citation_style":"apa"}),
            serde_json::json!({"document":"synthetic"}),
        ] {
            assert!(
                serde_json::from_value::<PickerPreferencesPatch>(refused.clone()).is_err(),
                "{refused}"
            );
        }
    }
}
