//! Saving choices made in the picker to the settings file.

use crate::library::CommandError;
use ibis_engine::app_context::output;

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use ibis_engine::settings::{self, Density, PickerPlacement};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use ibis_engine::app_context::output::OutputRef;
use ibis_engine::library::{ChoiceStore, Owner};

pub(crate) use ibis_engine::settings_backup::{settings_message, settings_problem};

/// This notice is for the interface only: its settings path must not be logged.
pub(crate) fn settings_notice(path: &Path, loaded: &settings::Loaded) -> Option<String> {
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
        let description = if loaded.primary_read == settings::PrimaryRead::Missing && loaded.from_backup {
            "settings.json is missing".to_string()
        } else {
            settings_message(problem)
        };
        let recovery = if loaded.interface().is_ok() {
            "the backup"
        } else {
            "defaults"
        };
        let saving = "Changes are not being saved while this problem remains.";
        let problem_path = match problem {
            settings::SettingsError::Backup(_) => path.with_extension("json.bak"),
            _ => path.to_path_buf(),
        };
        format!(
            "{description}; using {recovery} for the interface. {saving} Restore a settings backup or repair settings.json before saving. Settings file: {}",
            problem_path.display()
        )
    })
}

/// Where the settings file is, for saving choices made in the picker.
#[derive(Clone)]
pub(crate) struct SettingsFile {
    pub(crate) path: PathBuf,
    notice: Arc<Mutex<Option<String>>>,
    /// Orders every save of this file by when it was asked for (decision 0040).
    pub(crate) owner: Arc<ibis_engine::settings::Owner>,
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

#[derive(Clone, Default, PartialEq, Eq)]
struct NativeSettings {
    library: Option<settings::LibrarySetting>,
    profile: Option<String>,
    updates: Option<bool>,
    endpoint: Option<settings::EditorEndpoint>,
}
impl From<&settings::Settings> for NativeSettings {
    fn from(settings: &settings::Settings) -> Self {
        Self {
            library: settings.portable.library.clone(),
            profile: settings.device.zotero_profile.clone(),
            updates: settings.device.update_checks,
            endpoint: settings.device.editor_endpoint.clone(),
        }
    }
}

impl ChoiceStore for SettingsFile {
    fn zotero_profile(&self) -> Result<Option<String>, ibis_core::contract::EngineError> {
        self.owner.zotero_profile()
    }

    fn save(&self, change: &mut dyn FnMut(&mut settings::Settings)) -> Result<(), String> {
        self.update(change).map_err(|error| error.to_string())?;
        self.refresh_notice();
        Ok(())
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
        let read = settings::load(&path);
        let loaded = read.interface().unwrap_or_default();
        let surprises = loaded.small_surprises.unwrap_or(true);
        let last_valid = PickerPreferences::from_interface(&loaded);
        let native = read.cites().map(NativeSettings::from).unwrap_or_default();
        let delights = crate::delights::DelightOwner::new(loaded.delights);
        SettingsFile {
            owner: Arc::new(ibis_engine::settings::Owner::open(path.clone())),
            notice: Arc::new(Mutex::new(settings_notice(&path, &read))),
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
            settings.device.key_formats_used = true;
            Ok::<_, settings::SettingsError>(())
        });
    }

    /// Conditional writes test their immutable owner after acquiring the writer.
    pub(crate) fn update_when(
        &self,
        current: impl FnOnce() -> bool,
        change: impl FnOnce(&mut settings::Settings),
    ) -> Result<Option<settings::Settings>, CommandError> {
        self.owner.change_with(
            |settings| {
                if !current() {
                    return Ok(None);
                }
                let before = settings.clone();
                change(settings);
                Ok::<_, settings::SettingsError>(Some(before))
            },
            |result| {
                result
                    .map(|(saved, before)| {
                        before.map(|before| {
                            self.remember_native_changes(&before, &saved);
                            saved
                        })
                    })
                    .map_err(CommandError::from)
            },
        )
    }

    #[cfg(test)]
    fn settle(&self) -> Result<(), String> {
        self.owner
            .read_with(|_| ())
            .ok_or_else(|| "settings are unavailable".to_string())
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
        let Ok(saved) = loaded.cites() else {
            return;
        };
        let preferences = PickerPreferences::from_settings(saved);
        let fields = match self.last_valid.lock() {
            Ok(mut previous) => {
                let fields = changed_fields(*previous, preferences);
                *previous = preferences;
                fields
            }
            Err(_) => return,
        };
        self.surprises
            .store(saved.portable.small_surprises.unwrap_or(true), Ordering::SeqCst);
        self.apply_text_size(app, preferences.larger_text, revision);
        if let Some(history) = app.try_state::<crate::session_references::SessionReferences>() {
            let _ = history.set_enabled(saved.portable.recent_citations.unwrap_or(true));
        }
        crate::shortcut_setting::apply_file_shortcut(app, saved.portable.shortcut.here().map(str::to_owned), revision);
        crate::tray::show_in_menu_bar(app, preferences.menu_bar, revision);
        native(self, app, revision, saved);
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
            if before.portable.library != after.portable.library {
                native.library = after.portable.library.clone();
            }
            if before.device.zotero_profile != after.device.zotero_profile {
                native.profile = after.device.zotero_profile.clone();
            }
            if before.device.update_checks != after.device.update_checks {
                native.updates = after.device.update_checks;
            }
            if before.device.editor_endpoint != after.device.editor_endpoint {
                native.endpoint = after.device.editor_endpoint.clone();
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
        let library_request = app.state::<Owner>().requests();
        tauri::async_runtime::spawn_blocking(move || {
            // Native owners may themselves save a choice. Never call them in
            // the watch callback, which runs inside the settings owner's queue.
            let Ok(_applying) = file.native_apply.lock() else {
                return;
            };
            let endpoint = file
                .owner
                .read_with(|loaded| {
                    let Ok(saved) = loaded.cites() else {
                        return None;
                    };
                    if file.native_revision.load(Ordering::SeqCst) != revision {
                        return None;
                    }
                    let current = NativeSettings::from(saved);
                    let Ok(mut previous) = file.native.lock() else {
                        return None;
                    };
                    if current.library == desired.library
                        && current.profile == desired.profile
                        && (previous.library != desired.library
                            || (previous.profile != desired.profile
                                && matches!(desired.library, Some(settings::LibrarySetting::Zotero(_)))))
                        && app.state::<Owner>().requests() == library_request
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
                .flatten();
            // Endpoint replacement shares the UI's native lock, whose order
            // is native -> settings. It must run after the queue turn ends.
            if let Some(endpoint) = endpoint {
                crate::editor_endpoint::apply_file_setting(&app, endpoint);
            }
        });
    }

    pub(crate) fn notice(&self) -> Option<String> {
        self.notice.lock().ok().and_then(|notice| notice.clone())
    }

    pub(crate) fn refresh_notice(&self) -> bool {
        let previous = self.notice();
        let next = settings_notice(&self.path, &settings::load(&self.path));
        let Ok(mut notice) = self.notice.lock() else {
            return false;
        };
        if *notice == previous && *notice != next {
            *notice = next;
            true
        } else {
            false
        }
    }
}

pub(crate) use settings::{Accent, Appearance};

impl SettingsFile {
    /// Placement uses the last valid snapshot, without disk I/O while showing.
    pub(crate) fn picker_placement(&self) -> PickerPlacement {
        self.last_valid
            .lock()
            .ok()
            .and_then(|preferences| preferences.picker_placement)
            .unwrap_or_default()
    }

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) picker_placement: Option<PickerPlacement>,
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
        Self::from_interface(&settings.into())
    }
    fn from_interface(settings: &settings::Interface) -> Self {
        PickerPreferences {
            output: output::stored(settings.output.as_deref()),
            key_output: settings
                .key_output
                .as_deref()
                .and_then(output::listed)
                .filter(|output| matches!(output.spec(), ibis_core::contract::OutputSpec::Key { .. })),
            appearance: Some(settings.appearance.unwrap_or_default()),
            accent: Some(settings.accent.unwrap_or_default()),
            density: settings.density.unwrap_or(Density::Comfortable),
            picker_placement: Some(settings.picker_placement.unwrap_or_default()),
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
    pub(crate) picker_placement: Option<PickerPlacement>,
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
            picker_placement: value.picker_placement,
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
    settings_file.preferences()
}

impl SettingsFile {
    pub(crate) fn preferences(&self) -> Result<PickerPreferences, CommandError> {
        self.owner
            .view()
            .interface()
            .map(|settings| PickerPreferences::from_interface(&settings))
            .map_err(CommandError::from)
    }
}

/// Remember picker preferences in the order they were chosen (decision 0040).
/// Runs at IPC dispatch, so two windows and two quick choices keep their order:
/// it applies what must not wait for disk, hands the change to the settings
/// owner and returns its receipt at once. Other settings in the file are kept.
#[tauri::command]
#[ibis_desktop_macros::command(main)]
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
            let _ = history.set_enabled(saved.portable.recent_citations.unwrap_or(true));
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
        let saved = submitted
            .outcome
            .wait()
            .unwrap_or(Err(CommandError::SettingsUnavailable))?;
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
        let (receipt, outcome) = self.owner.submit_with_receipt(
            move |settings| apply_preferences(settings, preferences),
            move |receipt, saved| {
                let saved = saved.map(|(settings, ())| settings);
                if let Ok(saved) = &saved {
                    surprises.store(saved.portable.small_surprises.unwrap_or(true), Ordering::SeqCst);
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
            preferences.output = Some(output::listed(id).ok_or(ibis_core::contract::CitationError::UnsupportedOutput)?);
        }
        let receipt = self.submit_preferences(preferences, |_, _| ())?;
        self.take_receipt(receipt)?
            .outcome
            .wait()
            .unwrap_or(Err(CommandError::SettingsUnavailable))
    }
}

/// One preference change, as the owner runs it: only the fields it names.
fn apply_preferences(s: &mut settings::Settings, preferences: PickerPreferencesPatch) -> Result<(), CommandError> {
    if preferences
        .output
        .is_some_and(|output| !matches!(output.spec(), ibis_core::contract::OutputSpec::Csl { .. }))
    {
        return Err(CommandError::DocumentFormat);
    }
    if preferences
        .key_output
        .is_some_and(|output| !matches!(output.spec(), ibis_core::contract::OutputSpec::Key { .. }))
    {
        return Err(CommandError::KeyFormat);
    }

    if let Some(appearance) = preferences.appearance {
        if appearance == Appearance::System {
            s.portable.appearance = None;
        } else {
            s.portable.appearance = Some(appearance);
        }
    }
    if let Some(accent) = preferences.accent {
        if accent == Accent::System {
            s.portable.accent = None;
        } else {
            s.portable.accent = Some(accent);
        }
    }
    if let Some(output) = preferences.output {
        s.portable.output = Some(output.id().to_owned());
    }
    if let Some(output) = preferences.key_output {
        s.portable.key_output = Some(output.id().to_owned());
    }
    // Defaults stay unwritten, so the file only names real choices.
    if let Some(placement) = preferences.picker_placement {
        s.portable.picker_placement = Some(placement).filter(|p| *p != PickerPlacement::Centre);
    }
    if let Some(density) = preferences.density {
        s.portable.density = Some(density).filter(|d| *d != Density::Comfortable);
    }
    if let Some(larger) = preferences.larger_text {
        s.portable.larger_text = Some(larger).filter(|larger| *larger);
    }
    if let Some(paste) = preferences.paste {
        s.portable.paste = Some(paste).filter(|paste| !paste);
    }
    if let Some(enabled) = preferences.small_surprises {
        s.portable.small_surprises = Some(enabled).filter(|on| !on);
    }
    if let Some(shown) = preferences.menu_bar {
        s.portable.menu_bar = Some(shown).filter(|shown| !shown);
    }
    if let Some(on) = preferences.recent_citations {
        s.portable.recent_citations = Some(on).filter(|on| !on);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_engine::settings::LibrarySetting;
    use tauri::Manager;

    fn saved(loaded: &settings::Loaded) -> &settings::Settings {
        loaded.cites().unwrap_or_else(|error| panic!("{error}"))
    }

    /// Save as an editor does: replace the file in one step. A plain write
    /// truncates and refills it, and the watcher can read the old and new
    /// bytes mixed (seen on Windows), which is not an edit anyone made.
    fn save_atomically(path: &std::path::Path, contents: &[u8]) {
        let temporary = path.with_extension("json.saving");
        std::fs::write(&temporary, contents).unwrap_or_else(|error| panic!("{error}"));
        std::fs::rename(&temporary, path).unwrap_or_else(|error| panic!("{error}"));
    }

    #[test]
    fn live_file_edits_update_windows_and_native_choices_but_invalid_edits_keep_the_last_valid_state() {
        use tauri::Listener;
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("settings.json");
        settings::Owner::open(path.clone())
            .ensure_file()
            .unwrap_or_else(|error| panic!("{error}"));
        let app = tauri::test::mock_builder()
            .manage(SettingsFile::new(path.clone()))
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
                *applied.lock().unwrap_or_else(|error| panic!("{error}")) = Some(saved.device.update_checks);
            })
            .unwrap_or_else(|| panic!("temporary settings folder is watchable"));
        save_atomically(&path, br#"{"larger_text":true,"update_checks":false}"#);
        // Drain in the owner's turn, without relying on native event timing.
        file.settle().unwrap_or_else(|error| panic!("{error}"));
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
        save_atomically(
            &path,
            br#"{"appearance":"invented","larger_text":false,"update_checks":true}"#,
        );
        file.settle().unwrap_or_else(|error| panic!("{error}"));
        let unexpected = events.try_recv();
        assert!(unexpected.is_err(), "unexpected event: {unexpected:?}");
        assert_eq!(
            *native.lock().unwrap_or_else(|error| panic!("{error}")),
            Some(Some(false))
        );
        assert!(app.state::<SettingsFile>().notice().is_some());
        assert!(file.update(|saved| saved.portable.menu_bar = Some(false)).is_err());
        assert!(std::fs::read_to_string(&path).unwrap_or_default().contains("invented"));
        save_atomically(&path, br#"{"larger_text":false,"update_checks":true}"#);
        file.settle().unwrap_or_else(|error| panic!("{error}"));
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

    fn out(id: &str) -> OutputRef {
        output::listed(id).unwrap_or_else(|| panic!("{id} is listed"))
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
            let Err(error) = SettingsFile::new(path.clone()).preferences() else {
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
        assert!(refused.is_err_and(|error| {
            let problem = error.problem();
            problem.code == "settings-read-only"
                && problem.info.recovery() == ibis_core::contract::Recovery::ReviewSettings
        }));
        assert_eq!(
            SettingsFile::new(path.clone())
                .preferences()
                .unwrap_or_else(|e| panic!("{e}"))
                .density,
            Density::Comfortable
        );
        std::fs::remove_file(&path).unwrap_or_else(|e| panic!("{e}"));
        std::fs::create_dir(&path).unwrap_or_else(|e| panic!("{e}"));
        assert!(
            SettingsFile::new(path.clone()).preferences().is_err(),
            "a failed read must not silently return defaults"
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
    fn preference_save_refuses_a_damaged_primary_with_or_without_a_backup() {
        for has_backup in [false, true] {
            let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
            let path = directory.path().join("settings.json");
            std::fs::write(&path, "{ damaged").unwrap_or_else(|e| panic!("{e}"));
            if has_backup {
                std::fs::write(path.with_extension("json.bak"), "{}").unwrap_or_else(|e| panic!("{e}"));
            }
            let file = SettingsFile::new(path.clone());
            assert!(
                file.save_preferences(PickerPreferences::from_settings(&settings::Settings::default()), None)
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
                            file.update(|s| {
                                s.portable.library = Some(LibrarySetting::Bibtex(PathBuf::from("refs.bib")))
                            })
                        } else {
                            file.update(|s| s.portable.output = Some("modern-language-association".into()))
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
            saved(&loaded).portable.library,
            Some(LibrarySetting::Bibtex(PathBuf::from("refs.bib")))
        );
        assert_eq!(
            saved(&loaded).portable.output.as_deref(),
            Some("modern-language-association")
        );
        assert!(!file.path.with_extension("json.invalid-1").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn picker_preferences_save_only_real_choices_and_keep_other_settings() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        file.update(|s| s.portable.output = Some("modern-language-association".into()))
            .unwrap_or_else(|e| panic!("{e}"));
        let chosen = PickerPreferences {
            output: out("chicago-author-date"),
            key_output: None,
            appearance: Some(Appearance::System),
            accent: Some(Accent::System),
            picker_placement: Some(PickerPlacement::BottomRight),
            density: Density::Compact,
            larger_text: true,
            paste: false,
            menu_bar: true,
            recent_citations: Some(true),
            small_surprises: Some(true),
        };
        let saved = file.save_preferences(chosen, None).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(saved.portable.paste, Some(false));
        assert_eq!(saved.portable.picker_placement, Some(PickerPlacement::BottomRight));
        assert_eq!(file.picker_placement(), PickerPlacement::BottomRight);
        assert_eq!(saved.portable.output.as_deref(), Some("chicago-author-date"));
        assert_eq!(
            PickerPreferences::from_settings(&saved),
            PickerPreferences {
                output: out(saved.portable.output.as_deref().unwrap_or("apa")),
                ..chosen
            }
        );
        let defaults = PickerPreferences {
            output: out("apa"),
            key_output: None,
            appearance: Some(Appearance::System),
            accent: Some(Accent::System),
            picker_placement: Some(PickerPlacement::Centre),
            density: Density::Comfortable,
            larger_text: false,
            paste: true,
            menu_bar: true,
            recent_citations: Some(true),
            small_surprises: Some(true),
        };
        let saved = file.save_preferences(defaults, None).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(file.picker_placement(), PickerPlacement::Centre);
        assert_eq!(
            PickerPreferences::from_settings(&saved),
            PickerPreferences {
                output: out(saved.portable.output.as_deref().unwrap_or("apa")),
                ..defaults
            }
        );
        let text = std::fs::read_to_string(&file.path).unwrap_or_default();
        for field in [
            "density",
            "picker_placement",
            "larger_text",
            "paste",
            "menu_bar",
            "recent_citations",
        ] {
            assert!(!text.contains(field), "{field} is not written for the default");
        }
    }

    #[test]
    fn field_patches_from_two_stale_windows_keep_both_intents_in_either_write_order() {
        use std::sync::mpsc;
        for first in [0, 1] {
            let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
            let file = SettingsFile::new(directory.path().join("settings.json"));
            file.update(|s| s.portable.output = Some("apa".into()))
                .unwrap_or_else(|e| panic!("{e}"));
            let baseline = PickerPreferences::from_settings(&crate::settings_fixture::read(&file.path));
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
                assert_eq!(saved.portable.output.as_deref(), Some("chicago-author-date"));
                assert_eq!(saved.portable.density, Some(Density::Compact));
            });
        }
    }

    #[test]
    fn an_absent_output_keeps_the_saved_one_and_an_unlisted_one_is_refused() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        file.update(|s| s.portable.output = Some("apa".into()))
            .unwrap_or_else(|e| panic!("{e}"));
        let patch: PickerPreferencesPatch =
            serde_json::from_value(serde_json::json!({"larger_text":true})).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            file.save_preferences(patch, None)
                .unwrap_or_else(|e| panic!("{e}"))
                .portable
                .output
                .as_deref(),
            Some("apa")
        );
        let patch: PickerPreferencesPatch = serde_json::from_value(serde_json::json!({"output":"chicago-author-date"}))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            file.save_preferences(patch, None)
                .unwrap_or_else(|e| panic!("{e}"))
                .portable
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
