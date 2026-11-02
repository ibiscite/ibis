//! Back up and restore settings from Settings › General (#1561). The rules are
//! the engine's (`ibis_core::settings::backup`), shared with `ibis settings`;
//! this module owns the system dialogs, the one pending restore and the
//! restart that applies it. Paste approvals in `apps.json` are never touched.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use ibis_core::destination::System;
use ibis_core::settings::backup::{self, BackupError, Note, Plan};
use ibis_core::settings::{self, LibrarySetting, SettingsError};
use serde::Serialize;
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::preferences::SettingsFile;

/// A file or folder that does not answer within this time is reported, never waited on.
const DEADLINE: Duration = Duration::from_secs(2);

/// Why a backup or restore did not happen; the Settings window words it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum BackupProblem {
    NotDownloaded,
    Unreadable,
    NotABackup,
    SettingsFile,
    Newer,
    Invalid,
    /// This computer's settings are damaged, from a newer Ibis or unreadable.
    SettingsUnusable,
    /// The settings changed, or another backup was chosen, after the preview.
    Changed,
    NotSaved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum BackupSystem {
    Macos,
    Windows,
    Linux,
}

/// One setting a restore changes. Values are as the settings file writes
/// them, as text; `None` is the default. A bibliography is its file name only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub(crate) struct RestoreChange {
    pub(crate) setting: String,
    pub(crate) from: Option<String>,
    pub(crate) to: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RestoreNote {
    LibraryNotFound,
    ShortcutKept,
    RepairsSettings,
}

/// What restoring the chosen backup would change, before anything does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub(crate) struct RestorePreview {
    /// Names this preview; only the latest one can be restored.
    pub(crate) ticket: u32,
    pub(crate) made_on: BackupSystem,
    pub(crate) made_by: String,
    pub(crate) changes: Vec<RestoreChange>,
    pub(crate) notes: Vec<RestoreNote>,
}

/// The one restore that may be applied: the latest preview's plan.
#[derive(Default)]
pub(crate) struct PendingRestore(Mutex<(u32, Option<Plan>)>);

impl PendingRestore {
    /// Start choosing a backup: any earlier preview, shown or still being
    /// chosen, can no longer be restored.
    fn begin(&self) -> Option<u32> {
        let mut pending = self.0.lock().ok()?;
        pending.0 = pending.0.wrapping_add(1);
        pending.1 = None;
        Some(pending.0)
    }

    /// Keep the plan of the latest choice; a choice that finished late is dropped.
    fn offer(&self, ticket: u32, plan: Plan) -> bool {
        let Ok(mut pending) = self.0.lock() else { return false };
        if pending.0 != ticket {
            return false;
        }
        pending.1 = Some(plan);
        true
    }

    fn take(&self, ticket: u32) -> Option<Plan> {
        let mut pending = self.0.lock().ok()?;
        if pending.0 != ticket {
            return None;
        }
        pending.1.take()
    }

    fn cancel(&self, ticket: u32) {
        let _ = self.take(ticket);
    }
}

/// Run file work on its own thread for at most [`DEADLINE`].
fn within<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("ibis-settings-backup".into())
        .spawn(move || {
            let _ = sender.send(work());
        })
        .ok()?;
    receiver.recv_timeout(DEADLINE).ok()
}

/// Where the save dialog starts: the folder people already sync, when there
/// is one (iCloud Drive on macOS, OneDrive on Windows), otherwise Documents.
fn backup_folder(
    system: System,
    home: Option<&Path>,
    var: impl Fn(&str) -> Option<PathBuf>,
    is_dir: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let home = home?;
    let synced = match system {
        System::Macos => vec![home.join("Library/Mobile Documents/com~apple~CloudDocs")],
        System::Windows => ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"]
            .into_iter()
            .filter_map(var)
            .collect(),
        System::Linux => Vec::new(),
    };
    synced
        .into_iter()
        .chain([home.join("Documents"), home.to_path_buf()])
        .find(|folder| is_dir(folder))
}

fn system(system: System) -> BackupSystem {
    match system {
        System::Macos => BackupSystem::Macos,
        System::Windows => BackupSystem::Windows,
        System::Linux => BackupSystem::Linux,
    }
}

fn problem(error: &BackupError) -> BackupProblem {
    match error {
        BackupError::NotDownloaded => BackupProblem::NotDownloaded,
        BackupError::Unreadable(_) => BackupProblem::Unreadable,
        BackupError::NotABackup => BackupProblem::NotABackup,
        BackupError::SettingsFile => BackupProblem::SettingsFile,
        BackupError::NewerFormat(_) | BackupError::Settings(SettingsError::NewerVersion(_)) => BackupProblem::Newer,
        BackupError::Invalid(_) | BackupError::Settings(_) => BackupProblem::Invalid,
        BackupError::Changed => BackupProblem::Changed,
    }
}

fn text(value: Option<&serde_json::Value>) -> Option<String> {
    match value? {
        serde_json::Value::Null => None,
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Bool(on) => Some(if *on { "on" } else { "off" }.into()),
        serde_json::Value::Object(library) => Some(match (library.get("bibtex"), library.get("zotero")) {
            (Some(serde_json::Value::String(name)), _) => name.clone(),
            (_, Some(serde_json::Value::String(_))) => "zotero".into(),
            (_, Some(serde_json::Value::Array(libraries))) => format!("zotero:{}", libraries.len()),
            (_, Some(group)) => format!(
                "zotero-group:{}",
                group.get("group").unwrap_or(&serde_json::Value::Null)
            ),
            _ => serde_json::Value::Object(library.clone()).to_string(),
        }),
        other => Some(other.to_string()),
    }
}

/// Turn the engine's plan into what the Settings window shows.
fn preview(ticket: u32, backup: &backup::Backup, plan: &Plan) -> RestorePreview {
    RestorePreview {
        ticket,
        made_on: system(backup.made_on),
        made_by: backup.made_by.clone(),
        changes: plan
            .changes
            .iter()
            .map(backup::shown)
            .map(|change| RestoreChange {
                from: text(change.from.as_ref()),
                to: text(change.to.as_ref()),
                setting: change.setting,
            })
            .collect(),
        notes: plan
            .notes
            .iter()
            .map(|note| match note {
                Note::LibraryNotFound => RestoreNote::LibraryNotFound,
                Note::ShortcutKept => RestoreNote::ShortcutKept,
                Note::RepairsSettings => RestoreNote::RepairsSettings,
            })
            .collect(),
    }
}

/// Ask where to save a backup and write it there. `None` when cancelled.
#[tauri::command]
#[specta::specta]
pub(crate) async fn back_up_settings(window: WebviewWindow) -> Result<Option<String>, BackupProblem> {
    let app = window.app_handle().clone();
    let file = app.state::<SettingsFile>().inner().clone();
    let loaded = file
        .exclusive(settings::load)
        .map_err(|_| BackupProblem::SettingsUnusable)?;
    // A backup of recovered or default settings would not be what was chosen.
    if loaded.problem.is_some() {
        return Err(BackupProblem::SettingsUnusable);
    }
    let text = backup::export(&loaded.settings, &app.package_info().version.to_string())
        .map_err(|_| BackupProblem::NotSaved)?;
    let folder = within(|| {
        backup_folder(
            System::HERE,
            std::env::home_dir().as_deref(),
            |name| {
                std::env::var_os(name)
                    .filter(|value| !value.is_empty())
                    .map(PathBuf::from)
            },
            Path::is_dir,
        )
    })
    .flatten();
    let name = format!("{} settings.json", app.package_info().name);
    let mut dialog = app
        .dialog()
        .file()
        .set_parent(&window)
        .set_title("Back Up Settings")
        .set_file_name(&name)
        .add_filter("Ibis settings backup", &["json"]);
    if let Some(folder) = folder {
        dialog = dialog.set_directory(folder);
    }
    let chosen = tauri::async_runtime::spawn_blocking(move || dialog.blocking_save_file())
        .await
        .map_err(|_| BackupProblem::NotSaved)?;
    let Some(chosen) = chosen else { return Ok(None) };
    let path = chosen.into_path().map_err(|_| BackupProblem::NotSaved)?;
    let shown = path.file_name().map(|name| name.to_string_lossy().into_owned());
    // The system dialog already asked before replacing a file. A save the
    // person started is waited for to the end, however slow the folder, so
    // its result is the one shown and nothing lands after a reported failure.
    tauri::async_runtime::spawn_blocking(move || backup::save(&path, &text, true))
        .await
        .ok()
        .and_then(Result::ok)
        .ok_or(BackupProblem::NotSaved)?;
    Ok(Some(shown.unwrap_or(name)))
}

/// Ask for a backup, check it whole and say what restoring it would change.
/// `None` when cancelled. Nothing changes yet.
#[tauri::command]
#[specta::specta]
pub(crate) async fn choose_settings_backup(window: WebviewWindow) -> Result<Option<RestorePreview>, BackupProblem> {
    let app = window.app_handle().clone();
    let ticket = app.state::<PendingRestore>().begin().ok_or(BackupProblem::Changed)?;
    let dialog = app
        .dialog()
        .file()
        .set_parent(&window)
        .set_title("Restore Settings")
        .add_filter("Ibis settings backup", &["json"]);
    let chosen = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_file())
        .await
        .map_err(|_| BackupProblem::Unreadable)?;
    let Some(chosen) = chosen else { return Ok(None) };
    let path = chosen.into_path().map_err(|_| BackupProblem::Unreadable)?;
    let backup = within(move || backup::read(&path))
        .ok_or(BackupProblem::Unreadable)?
        .map_err(|error| problem(&error))?;
    let file = app.state::<SettingsFile>().inner().clone();
    let loaded = file
        .exclusive(settings::load)
        .map_err(|_| BackupProblem::SettingsUnusable)?;
    // Damaged settings can be replaced; settings from a newer Ibis never are.
    if let Some(SettingsError::NewerVersion(_)) = &loaded.problem {
        return Err(BackupProblem::SettingsUnusable);
    }
    let found = match &backup.settings.library {
        Some(LibrarySetting::Bibtex(library)) => {
            let library = library.clone();
            within(move || library.try_exists().unwrap_or(false)).unwrap_or(false)
        }
        _ => true,
    };
    let plan =
        backup::plan(&loaded.settings, &backup, |_| found).repairing(backup::is_damaged(loaded.problem.as_ref()));
    if !app.state::<PendingRestore>().offer(ticket, plan.clone()) {
        return Ok(None);
    }
    Ok(Some(preview(ticket, &backup, &plan)))
}

/// Restore the previewed backup, then restart so every part of Ibis starts
/// from the restored settings. The replaced settings become `settings.json.bak`.
#[tauri::command]
#[specta::specta]
pub(crate) fn restore_settings_backup(
    app: AppHandle,
    pending: State<'_, PendingRestore>,
    file: State<'_, SettingsFile>,
    ticket: u32,
) -> Result<(), BackupProblem> {
    let plan = pending.take(ticket).ok_or(BackupProblem::Changed)?;
    file.exclusive(|path| backup::restore(path, &plan))
        .map_err(|_| BackupProblem::NotSaved)?
        .map_err(|error| match error {
            BackupError::Changed => BackupProblem::Changed,
            BackupError::Settings(_) => BackupProblem::SettingsUnusable,
            _ => BackupProblem::NotSaved,
        })?;
    app.request_restart();
    Ok(())
}

/// Forget the previewed restore.
#[tauri::command]
#[specta::specta]
pub(crate) fn cancel_settings_restore(pending: State<'_, PendingRestore>, ticket: u32) -> Result<(), BackupProblem> {
    pending.cancel(ticket);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::settings::Settings;

    fn plan(settings: &Settings) -> Plan {
        let text = backup::export(settings, "0.0.0").unwrap_or_else(|error| panic!("{error}"));
        let backup = backup::parse(&text).unwrap_or_else(|error| panic!("{error}"));
        backup::plan(&Settings::default(), &backup, |_| true)
    }

    #[test]
    fn only_the_latest_preview_can_be_restored_and_only_once() {
        let pending = PendingRestore::default();
        let first = pending.begin().unwrap_or_default();
        let second = pending.begin().unwrap_or_default();
        assert_ne!(first, second);
        assert!(pending.offer(second, plan(&Settings::default())));
        assert!(
            !pending.offer(first, plan(&Settings::default())),
            "a choice that finished late is dropped"
        );
        assert!(pending.take(first).is_none(), "a replaced preview is refused");
        assert!(pending.take(second).is_some(), "the latest preview survives a late one");
        assert!(pending.take(second).is_none(), "a restore runs once");
        let third = pending.begin().unwrap_or_default();
        assert!(pending.offer(third, plan(&Settings::default())));
        pending.cancel(third);
        assert!(pending.take(third).is_none());
    }

    #[test]
    fn the_save_dialog_starts_in_the_folder_people_sync() {
        let home = Path::new("/invented/home");
        let icloud = home.join("Library/Mobile Documents/com~apple~CloudDocs");
        let documents = home.join("Documents");
        let none = |_: &str| None;
        assert_eq!(
            backup_folder(System::Macos, Some(home), none, |p| p == icloud || p == documents),
            Some(icloud)
        );
        assert_eq!(
            backup_folder(System::Macos, Some(home), none, |p| p == documents),
            Some(documents.clone())
        );
        let onedrive = PathBuf::from("/invented/OneDrive");
        let var = |name: &str| (name == "OneDrive").then(|| onedrive.clone());
        assert_eq!(
            backup_folder(System::Windows, Some(home), var, |p| p == onedrive),
            Some(onedrive.clone())
        );
        assert_eq!(
            backup_folder(System::Windows, Some(home), var, |p| p == documents),
            Some(documents.clone())
        );
        assert_eq!(
            backup_folder(System::Linux, Some(home), none, |p| p == home),
            Some(home.to_path_buf())
        );
        assert_eq!(backup_folder(System::Linux, None, none, |_| true), None);
    }

    #[test]
    fn a_preview_shows_a_bibliography_by_its_name_and_values_as_text() {
        let settings = Settings {
            library: Some(LibrarySetting::Bibtex(PathBuf::from("/invented/private/thesis.bib"))),
            larger_text: Some(true),
            ..Settings::default()
        };
        let text = backup::export(&settings, "0.0.0").unwrap_or_else(|error| panic!("{error}"));
        let backup = backup::parse(&text).unwrap_or_else(|error| panic!("{error}"));
        let plan = backup::plan(&Settings::default(), &backup, |_| false);
        let shown = preview(7, &backup, &plan);
        assert_eq!(shown.ticket, 7);
        assert_eq!(shown.made_by, "0.0.0");
        assert_eq!(shown.notes, [RestoreNote::LibraryNotFound]);
        let library = shown.changes.iter().find(|change| change.setting == "library");
        assert_eq!(
            library.and_then(|change| change.to.clone()).as_deref(),
            Some("thesis.bib")
        );
        assert!(!format!("{shown:?}").contains("private"));
        let larger = shown.changes.iter().find(|change| change.setting == "larger_text");
        assert_eq!(
            larger.map(|change| (change.from.clone(), change.to.clone())),
            Some((None, Some("on".into())))
        );
    }

    #[test]
    fn a_restore_through_the_settings_writer_saves_and_keeps_the_replaced_file() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("settings.json");
        std::fs::write(&path, r#"{"density":"spacious"}"#).unwrap_or_else(|error| panic!("{error}"));
        let file = SettingsFile::new(path.clone());
        let settings = Settings {
            larger_text: Some(true),
            ..Settings::default()
        };
        let text = backup::export(&settings, "0.0.0").unwrap_or_else(|error| panic!("{error}"));
        let backup = backup::parse(&text).unwrap_or_else(|error| panic!("{error}"));
        let plan = backup::plan(&settings::load(&path).settings, &backup, |_| true);
        // A change from another window after the preview refuses the restore.
        file.update(|s| s.paste = Some(false))
            .unwrap_or_else(|error| panic!("{error}"));
        let refused = file
            .exclusive(|path| backup::restore(path, &plan))
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(matches!(refused, Err(BackupError::Changed)));
        let plan = backup::plan(&settings::load(&path).settings, &backup, |_| true);
        let restored = file
            .exclusive(|path| backup::restore(path, &plan))
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(restored.is_ok());
        assert_eq!(settings::load(&path).settings.larger_text, Some(true));
        let replaced = std::fs::read_to_string(path.with_extension("json.bak")).unwrap_or_default();
        assert!(replaced.contains("spacious"));
    }
}
