//! Saving choices made in the picker to the settings file.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use ibis_core::settings::{self, Density};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::library::Library;
use crate::output::OutputRef;

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
        if loaded.primary_read == settings::PrimaryRead::Missing && loaded.from_backup {
            return Some(format!(
                "settings.json is missing; using the backup. The next save will restore settings.json and keep the backup unchanged. Settings file: {}",
                path.display()
            ));
        }
        let recovery = if loaded.from_backup { "the backup" } else { "defaults" };
        let saving = if loaded.from_backup && settings::backup::needs_repair(loaded) {
            "The next save will keep the damaged file as settings.json.invalid-N and restore settings from the backup."
        } else {
            "Changes are not being saved while this problem remains."
        };
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
    pub(crate) delights: Arc<Mutex<crate::delights::DelightOwner>>,
}

impl SettingsFile {
    pub(crate) fn new(path: PathBuf) -> Self {
        let loaded = settings::load(&path).settings;
        let surprises = loaded.small_surprises.unwrap_or(true);
        let delights = crate::delights::DelightOwner::new(loaded.delights);
        SettingsFile {
            owner: Arc::new(ibis_engine::settings::Owner::open(path.clone())),
            path,
            surprises: Arc::new(std::sync::atomic::AtomicBool::new(surprises)),
            receipts: Arc::default(),
            delights: Arc::new(Mutex::new(delights)),
        }
    }

    pub(crate) fn update(&self, change: impl FnOnce(&mut settings::Settings)) -> Result<settings::Settings, String> {
        self.update_when(|| true, change)?
            .ok_or_else(|| "settings are unavailable".to_string())
    }

    /// Run `work` on the settings file while no save can interleave, such as a
    /// restore that must find the settings it was shown.
    pub(crate) fn exclusive<T>(&self, work: impl FnOnce(&Path) -> T) -> Result<T, String> {
        self.owner
            .in_turn(work)
            .ok_or_else(|| "settings are unavailable".to_string())
    }

    /// Conditional writes test their immutable owner after acquiring the writer.
    pub(crate) fn update_when(
        &self,
        current: impl FnOnce() -> bool,
        change: impl FnOnce(&mut settings::Settings),
    ) -> Result<Option<settings::Settings>, String> {
        self.exclusive(|path| {
            if !current() {
                return Ok(None);
            }
            settings::update(path, change)
                .map(Some)
                .map_err(|error| settings_message(&error))
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
        look_from_settings(&settings::load(&self.path).settings)
            .map(|look| look.appearance)
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
            output: crate::output::stored(settings.output.as_deref()),
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

/// The authoritative state after one locked patch and flush.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub(crate) struct PickerSave {
    preferences: PickerPreferences,
}

/// The saved snapshot and the identity of the write that published it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
pub(crate) struct PreferenceChange {
    revision: u64,
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

#[cfg(test)]
impl From<PickerPreferences> for PickerPreferencesPatch {
    fn from(value: PickerPreferences) -> Self {
        Self {
            appearance: value.appearance,
            accent: value.accent,
            output: Some(value.output),
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
pub(crate) async fn picker_preferences(settings_file: State<'_, SettingsFile>) -> Result<PickerPreferences, String> {
    // Off the main thread: a settings write in progress must never stall a paste.
    let settings_file = settings_file.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // In turn, so it reads every change asked for before it, and publishes
        // what it read before any later change can.
        settings_file.exclusive(|path| {
            let preferences = read_preferences(path)?;
            settings_file
                .surprises
                .store(preferences.small_surprises.unwrap_or(true), Ordering::SeqCst);
            Ok(preferences)
        })?
    })
    .await
    .map_err(|_| "settings are unavailable".to_string())?
}

fn read_preferences(path: &Path) -> Result<PickerPreferences, String> {
    read_preference_settings(path).map(|saved| PickerPreferences::from_settings(&saved))
}

fn read_preference_settings(path: &Path) -> Result<settings::Settings, String> {
    let loaded = settings::load(path);
    if let Some(problem) = &loaded.problem
        && !loaded.recovering()
    {
        return Err(settings_message(problem));
    }
    look_from_settings(&loaded.settings)?;
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
) -> Result<u64, String> {
    // Geometry takes the text size now, even if the save then fails.
    if let Some(larger) = preferences.larger_text
        && let Some(sizing) = app.try_state::<crate::sizing::Sizing>()
    {
        sizing.set_larger_text(larger);
    }
    let changes_menu_bar = preferences.menu_bar.is_some();
    // A change made in Settings reaches the picker; the picker's own are already shown.
    let from_elsewhere = window.label() != "main";
    let file = settings_file.inner().clone();
    // What follows a save runs once it is saved, whether or not a window asks
    // how it went: a window may close first.
    settings_file.submit_preferences(preferences, move |receipt, saved| {
        let Ok(saved) = saved else { return };
        let preferences = PickerPreferences::from_settings(saved);
        if changes_menu_bar {
            crate::tray::show_in_menu_bar(app.app_handle(), preferences.menu_bar, receipt);
        }
        let change = PreferenceChange {
            revision: receipt,
            preferences,
        };
        crate::library::refresh_settings_notice(&app, &file);
        if from_elsewhere {
            crate::diagnostics::report(
                app.emit_to("main", "preferences:changed", change),
                crate::diagnostics::NativeIssue::PickerEvent,
            );
        }
        crate::diagnostics::report(
            app.emit_to("settings", "preferences:changed", change),
            crate::diagnostics::NativeIssue::PickerEvent,
        );
    })
}

/// The saved preferences once a submitted change has run.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn picker_preferences_saved(
    receipt: u64,
    settings_file: State<'_, SettingsFile>,
) -> Result<PickerSave, String> {
    let submitted = settings_file.take_receipt(receipt)?;
    tauri::async_runtime::spawn_blocking(move || {
        let saved = submitted
            .outcome
            .recv()
            .unwrap_or_else(|_| Err("settings are unavailable".to_string()))?;
        Ok(PickerSave {
            preferences: PickerPreferences::from_settings(&saved),
        })
    })
    .await
    .map_err(|_| "picker preferences could not be saved".to_string())?
}

/// Preference changes handed to the owner, by receipt, until their window asks
/// how they went. A window that closes first never asks, so only the latest
/// [`MAX_RECEIPTS`] are kept; the change itself is saved either way.
#[derive(Default)]
struct Receipts {
    next: u64,
    submitted: std::collections::BTreeMap<u64, Submitted>,
}

const MAX_RECEIPTS: usize = 256;

struct Submitted {
    outcome: std::sync::mpsc::Receiver<Result<settings::Settings, String>>,
}

impl SettingsFile {
    pub(crate) fn surprises_enabled(&self) -> bool {
        self.surprises.load(Ordering::SeqCst)
    }

    fn submit_preferences(
        &self,
        preferences: PickerPreferencesPatch,
        after: impl FnOnce(u64, &Result<settings::Settings, String>) + Send + 'static,
    ) -> Result<u64, String> {
        let mut receipts = self
            .receipts
            .lock()
            .map_err(|_| "settings are unavailable".to_string())?;
        // The generated bridge represents receipts as JavaScript numbers.
        let receipt = receipts
            .next
            .checked_add(1)
            .filter(|value| *value < 1 << 53)
            .ok_or_else(|| "settings are unavailable".to_string())?;
        receipts.next = receipt;
        let surprises = Arc::clone(&self.surprises);
        // Submitted under the receipts lock, so receipts follow the owner's order.
        let saved = self.owner.submit_on_file(move |path| {
            let saved = save_preferences_at(path, preferences);
            // Published in the save's own turn, so no older read can undo it.
            if let Ok(saved) = &saved {
                surprises.store(saved.small_surprises.unwrap_or(true), Ordering::SeqCst);
            }
            saved
        });
        let (reply, outcome) = std::sync::mpsc::channel();
        tauri::async_runtime::spawn_blocking(move || {
            let saved = saved
                .wait()
                .unwrap_or_else(|| Err("settings are unavailable".to_string()));
            after(receipt, &saved);
            let _ = reply.send(saved);
        });
        receipts.submitted.insert(receipt, Submitted { outcome });
        while receipts.submitted.len() > MAX_RECEIPTS {
            receipts.submitted.pop_first();
        }
        Ok(receipt)
    }

    fn take_receipt(&self, receipt: u64) -> Result<Submitted, String> {
        self.receipts
            .lock()
            .ok()
            .and_then(|mut receipts| receipts.submitted.remove(&receipt))
            .ok_or_else(|| "preference change was not submitted".to_string())
    }

    #[cfg(test)]
    fn save_preferences(
        &self,
        preferences: impl Into<PickerPreferencesPatch>,
        output: Option<&str>,
    ) -> Result<settings::Settings, String> {
        let mut preferences = preferences.into();
        if let Some(id) = output {
            preferences.output = Some(crate::output::listed(id).ok_or("not a listed output")?);
        }
        let receipt = self.submit_preferences(preferences, |_, _| ())?;
        self.take_receipt(receipt)?
            .outcome
            .recv()
            .unwrap_or_else(|_| Err("settings are unavailable".to_string()))
    }
}

/// One preference change, as the owner runs it: only the fields it names.
fn save_preferences_at(path: &Path, preferences: PickerPreferencesPatch) -> Result<settings::Settings, String> {
    read_preference_settings(path)?;
    if std::fs::metadata(path).is_ok_and(|metadata| metadata.permissions().readonly()) {
        return Err("settings-read-only".into());
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
            s.output = (output.id() != ibis_engine::outputs::DEFAULT).then(|| output.id().to_owned());
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
    .map_err(|error| settings_message(&error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::settings::LibrarySetting;
    use tauri::Manager;

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
        assert!(last.outcome.recv().is_ok_and(|saved| saved.is_ok()));
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
                output: crate::output::listed("typst"),
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
                output: crate::output::listed("cite"),
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
        assert_eq!(reopened.output.id(), "cite");
        assert_eq!(reopened.accent, Some(Accent::Ibis));
        // A receipt is answered once.
        assert!(app.state::<SettingsFile>().take_receipt(newer).is_err());
    }

    #[test]
    fn look_choices_reload_independently_and_defaults_stay_unwritten() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("settings.json");
        let file = SettingsFile::new(path.clone());
        file.update(|s| s.output = Some("typst".into()))
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
        assert_eq!(saved.output.as_deref(), Some("typst"));
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
            assert!(!error.contains("synthetic-private-value"));
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
        assert!(refused.is_err_and(|problem| problem == "settings-read-only"));
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
        for (index, window) in [&main, &settings].into_iter().enumerate() {
            let response = save_over_ipc(window, chosen, None);
            assert!(response.is_ok());
            let payload = events
                .recv_timeout(Duration::from_secs(3))
                .unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(serde_json::from_str::<PickerPreferences>(&payload).ok(), Some(chosen));
            assert_eq!(
                serde_json::from_str::<PreferenceChange>(&payload)
                    .unwrap_or_else(|e| panic!("{e}"))
                    .revision,
                index as u64 + 1
            );
        }
        let payload = picker_events
            .recv_timeout(Duration::from_secs(3))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(serde_json::from_str::<PickerPreferences>(&payload).ok(), Some(chosen));
        assert_eq!(
            serde_json::from_str::<PreferenceChange>(&payload)
                .unwrap_or_else(|e| panic!("{e}"))
                .revision,
            2
        );
    }

    #[test]
    fn preference_dispatch_returns_while_writer_is_busy_and_save_keeps_current_settings() {
        use std::sync::mpsc;
        use std::time::Duration;

        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("settings.json");
        std::fs::write(&path, r#"{"output":"keys","future_choice":"before dispatch"}"#)
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
            output: out("pandoc"),
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
        let receipt = tauri::test::get_ipc_response(&window, preference_request(&window, chosen, Some("typst")))
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
                "width": 840, "height": 600, "custom_size": true, "x": 40, "y": 60
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
            serde_json::json!({"preferences": PickerPreferences { output: out("typst"), ..chosen }})
        );
        thread.join().unwrap_or_else(|_| panic!("dispatch thread panicked"));
        let loaded = settings::load(&path);
        assert!(loaded.problem.is_none());
        assert_eq!(
            PickerPreferences::from_settings(&loaded.settings),
            PickerPreferences {
                output: crate::output::stored(loaded.settings.output.as_deref()),
                ..chosen
            }
        );
        assert_eq!(loaded.settings.output.as_deref(), Some("typst"));
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
    fn preference_recovery_save_refreshes_and_publishes_the_settings_notice() {
        use std::sync::mpsc;
        use std::time::Duration;
        use tauri::Listener;

        for has_backup in [false, true] {
            let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
            let path = directory.path().join("settings.json");
            std::fs::write(&path, "{ damaged").unwrap_or_else(|e| panic!("{e}"));
            if has_backup {
                std::fs::write(path.with_extension("json.bak"), "{}").unwrap_or_else(|e| panic!("{e}"));
            }
            let startup_notice = settings_notice(&path, &settings::load(&path));
            let mut library = crate::library::Library::default();
            library.settings_notice = startup_notice.clone();
            let app = tauri::test::mock_builder()
                .manage(SettingsFile::new(path.clone()))
                .manage(Mutex::new(library))
                .invoke_handler(tauri::generate_handler![
                    save_picker_preferences,
                    picker_preferences_saved
                ])
                .build(tauri::generate_context!(test = true))
                .unwrap_or_else(|e| panic!("cannot build preference fixture: {e}"));
            let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
                .build()
                .unwrap_or_else(|e| panic!("cannot build window fixture: {e}"));
            let (sent, events) = mpsc::channel();
            app.listen("library:changed", move |event| {
                let _ = sent.send(event.payload().to_string());
            });
            // Even unchanged preferences complete recovery.
            let chosen = PickerPreferences::from_settings(&settings::load(&path).settings);
            let result = save_over_ipc(&window, chosen, None);
            assert_eq!(result.is_ok(), has_backup);
            let library = app.state::<Mutex<crate::library::Library>>();
            let library = library.lock().unwrap_or_else(|_| panic!("library lock poisoned"));
            if has_backup {
                assert_eq!(
                    library.settings_notice, None,
                    "successful recovery must clear the damaged-settings warning"
                );
                let event = events
                    .recv_timeout(Duration::from_secs(3))
                    .unwrap_or_else(|e| panic!("{e}"));
                let status: serde_json::Value = serde_json::from_str(&event).unwrap_or_else(|e| panic!("{e}"));
                assert_eq!(status.get("settings_notice"), Some(&serde_json::Value::Null));
            } else {
                assert_eq!(library.settings_notice, startup_notice);
                assert!(events.try_recv().is_err(), "failed saves must keep the warning");
            }
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
            (r#"{"density":"synthetic-private-value"}"#, "settings.json is not valid"),
            (
                r#"{"schema_version":3,"private":"synthetic-private-value"}"#,
                "settings.json was written by a newer version of Ibis",
            ),
        ] {
            std::fs::write(&path, text).unwrap_or_else(|e| panic!("{e}"));
            let result = save_over_ipc(&window, chosen, None);
            assert_eq!(result.err(), Some(serde_json::json!(expected)));
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
                .err(),
            Some("settings.json could not be read".to_string())
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
                    file.save_preferences(PickerPreferences::from_settings(&loaded.settings), Some("keys"))
                        .err(),
                    Some(expected.to_string())
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
        let backup = br#"{"output":"typst","future_choice":[true,null]}"#;
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
            let Err(failed) = SettingsFile::new(path.clone()).update(|s| s.output = Some("keys".into())) else {
                panic!("expected the invalid settings save to fail");
            };
            assert!(failed.contains(problem));
            assert!(!failed.contains(private));
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
            assert_eq!(
                notice.contains("not being saved"),
                contents.contains("schema_version\":3")
            );
            assert_eq!(
                notice.contains("settings.json.invalid-N"),
                !contents.contains("schema_version\":3")
            );
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
                let backup_contents = br#"{"schema_version":2,"output":"typst"}"#;
                std::fs::write(&file.path, contents).unwrap_or_else(|e| panic!("{e}"));
                std::fs::write(&temporary, b"synthetic unfinished save").unwrap_or_else(|e| panic!("{e}"));
                if has_backup {
                    std::fs::write(&backup, backup_contents).unwrap_or_else(|e| panic!("{e}"));
                }
                let loaded = settings::load(&file.path);
                assert!(loaded.problem.is_some());
                assert_eq!(loaded.from_backup, has_backup);
                assert_eq!(loaded.settings.output.as_deref(), has_backup.then_some("typst"));
                assert_eq!(std::fs::read(&file.path).unwrap_or_else(|e| panic!("{e}")), contents);
                let chosen = PickerPreferences::from_settings(&loaded.settings);
                let saved = file.save_preferences(chosen, Some("cite"));
                let recoverable = has_backup && contents != br#"{"schema_version":999}"#;
                if recoverable {
                    let saved = saved.unwrap_or_else(|e| panic!("recovery save failed: {e}"));
                    assert_eq!(saved.output.as_deref(), Some("cite"));
                    let restarted = settings::load(&file.path);
                    assert!(restarted.problem.is_none());
                    assert_eq!(restarted.settings, saved);
                    assert_eq!(
                        std::fs::read(file.path.with_extension("json.invalid-1")).unwrap_or_else(|e| panic!("{e}")),
                        contents
                    );
                    assert_eq!(
                        std::fs::read(&backup).unwrap_or_else(|e| panic!("{e}")),
                        backup_contents
                    );
                    assert_eq!(
                        std::fs::read(&temporary).unwrap_or_default(),
                        b"synthetic unfinished save"
                    );
                    continue;
                }
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
        std::fs::write(&file.path, "{ damaged").unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(file.path.with_extension("json.bak"), "{}").unwrap_or_else(|e| panic!("{e}"));
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let file = file.clone();
                std::thread::spawn(move || {
                    for _ in 0..20 {
                        let saved = if i % 2 == 0 {
                            file.update(|s| s.library = Some(LibrarySetting::Bibtex(PathBuf::from("refs.bib"))))
                        } else {
                            file.update(|s| s.output = Some("typst".into()))
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
        assert_eq!(loaded.settings.output.as_deref(), Some("typst"));
        assert_eq!(
            std::fs::read_to_string(file.path.with_extension("json.invalid-1")).unwrap_or_default(),
            "{ damaged"
        );
        assert!(!file.path.with_extension("json.invalid-2").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn picker_preferences_save_only_real_choices_and_keep_other_settings() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        file.update(|s| s.output = Some("typst".into()))
            .unwrap_or_else(|e| panic!("{e}"));
        let chosen = PickerPreferences {
            output: out("apa"),
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
        assert_eq!(saved.output.as_deref(), Some("apa"));
        assert_eq!(
            PickerPreferences::from_settings(&saved),
            PickerPreferences {
                output: crate::output::stored(saved.output.as_deref()),
                ..chosen
            }
        );
        let defaults = PickerPreferences {
            output: out("pandoc"),
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
                output: crate::output::stored(saved.output.as_deref()),
                ..defaults
            }
        );
        let text = std::fs::read_to_string(&file.path).unwrap_or_default();
        for field in [
            "output",
            "density",
            "larger_text",
            "paste",
            "menu_bar",
            "recent_citations",
        ] {
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
                output: crate::output::stored(loaded.settings.output.as_deref()),
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
    fn every_listed_output_is_saved_by_its_id_and_the_default_stays_unwritten() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        let chosen = PickerPreferences::from_settings(&settings::Settings::default());
        for output in ibis_engine::outputs::outputs() {
            let chosen = PickerPreferences {
                output: out(&output.id),
                ..chosen
            };
            file.save_preferences(chosen, None).unwrap_or_else(|e| panic!("{e}"));
            let loaded = settings::load(&file.path);
            let written = (output.id != ibis_engine::outputs::DEFAULT).then_some(output.id.as_str());
            assert_eq!(loaded.settings.output.as_deref(), written);
            assert_eq!(PickerPreferences::from_settings(&loaded.settings), chosen);
        }
        // A density save keeps an output changed in the file meanwhile.
        file.update(|s| s.output = Some("keys".into()))
            .unwrap_or_else(|e| panic!("{e}"));
        let density = PickerPreferencesPatch {
            density: Some(Density::Compact),
            ..Default::default()
        };
        let saved = file.save_preferences(density, None).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(saved.output.as_deref(), Some("keys"));
    }

    #[test]
    fn choosing_a_key_never_overwrites_invalid_newer_or_unreadable_settings() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        let chosen = PickerPreferences {
            output: out("pandoc"),
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
            let failed = file.save_preferences(chosen, Some("cite"));
            assert!(failed.is_err());
            assert!(
                !failed
                    .err()
                    .unwrap_or_else(|| panic!("expected a failed settings save"))
                    .contains("synthetic-private-value")
            );
            assert_eq!(std::fs::read_to_string(&file.path).unwrap_or_default(), text);
        }
        std::fs::remove_file(&file.path).unwrap_or_else(|e| panic!("{e}"));
        std::fs::create_dir(&file.path).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            file.save_preferences(chosen, Some("cite"))
                .err()
                .unwrap_or_else(|| panic!("expected an unreadable settings save to fail")),
            "settings.json could not be read"
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
                serde_json::json!({ "output": "cite" }),
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
                assert_eq!(saved.output.as_deref(), Some("cite"));
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
        let patch: PickerPreferencesPatch =
            serde_json::from_value(serde_json::json!({"output":"cite"})).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            file.save_preferences(patch, None)
                .unwrap_or_else(|e| panic!("{e}"))
                .output
                .as_deref(),
            Some("cite")
        );
        // Only an id the engine lists crosses the boundary; the old shapes do not.
        for refused in [
            serde_json::json!({"output":"no-such-output"}),
            serde_json::json!({"output":{"key":"cite"}}),
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
