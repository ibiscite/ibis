//! A settings backup to restore on this computer or another one (#1561).
//!
//! The backup is the settings file without what belongs to one computer, with
//! a marker, the system it was made on and the Ibis version that made it:
//!
//! ```json
//! {
//!   "ibis_settings_backup": 1,
//!   "made_by": "0.2.8",
//!   "made_on": "macos",
//!   "settings": { "schema_version": 2, "shortcut": { "macos": "Control+Command+R" } }
//! }
//! ```
//!
//! Restoring checks the whole file before anything changes, shows what will
//! change, refuses if the settings changed since that preview and saves through
//! the same atomic write as every other change, so the settings it replaces
//! become `settings.json.bak`. Paste approvals live in `apps.json` and are never
//! read or written here, so a backup cannot grant paste (decision 0031).

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use serde_json::{Map, Value};
use thiserror::Error;

use super::{LibrarySetting, MAX_SETTINGS_BYTES, PrimaryRead, SCHEMA_VERSION, Settings, SettingsError, Shortcuts};
use crate::destination::System;

/// Version of the backup format this build reads and writes.
pub const FORMAT: u64 = 1;
const MARKER: &str = "ibis_settings_backup";

/// Settings that belong to one computer and never travel: window places, quiet
/// device-local feedback, the editor endpoint, whose token opens the picker, the
/// update question, which each computer answers for itself because it allows
/// network use (decision 0008), and the Zotero profile, a name that means nothing
/// elsewhere. A restore keeps this computer's values for them.
pub const LOCAL: [&str; 5] = ["picker_windows", "delights", "editor_endpoint", "update_checks", "zotero_profile"];

/// A backup read and checked whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backup {
    /// The system the backup was made on.
    pub made_on: System,
    /// The version of Ibis that made it, as it wrote it.
    pub made_by: String,
    /// The settings it holds, without [`LOCAL`] settings.
    pub settings: Settings,
}

#[derive(Debug, Error)]
pub enum BackupError {
    /// A cloud placeholder: reading it would wait for a download.
    #[error("the backup is not downloaded to this computer yet")]
    NotDownloaded,
    #[error("the backup could not be read: {0}")]
    Unreadable(#[source] io::Error),
    #[error("this file is not an Ibis settings backup")]
    NotABackup,
    /// The live settings file was chosen in place of a backup.
    #[error("this is an Ibis settings file, not a backup")]
    SettingsFile,
    #[error("the backup was made by a newer version of Ibis (backup format {0})")]
    NewerFormat(u64),
    /// A broken backup; the reason never holds a setting's value.
    #[error("the backup is not valid: {0}")]
    Invalid(&'static str),
    /// The settings inside the backup, or this computer's, could not be used.
    #[error(transparent)]
    Settings(#[from] SettingsError),
    #[error("the settings changed since this restore was shown; look at the changes again")]
    Changed,
}

/// The backup text for `settings`, made by Ibis `version` on this system.
pub fn export(settings: &Settings, version: &str) -> Result<String, BackupError> {
    let mut written = match serde_json::to_value(settings) {
        Ok(Value::Object(written)) => written,
        _ => return Err(BackupError::Invalid("the settings could not be written")),
    };
    for local in LOCAL {
        written.remove(local);
    }
    // Every shortcut names its system, so the backup means the same anywhere.
    if !settings.shortcut.is_empty() {
        written.insert(
            "shortcut".into(),
            serde_json::to_value(settings.shortcut.by_system()).unwrap_or_default(),
        );
    }
    let backup = serde_json::json!({
        MARKER: FORMAT,
        "made_by": version,
        "made_on": System::HERE,
        "settings": written,
    });
    let text =
        serde_json::to_string_pretty(&backup).map_err(|_| BackupError::Invalid("the backup could not be written"))?;
    if text.len() as u64 >= MAX_SETTINGS_BYTES {
        return Err(BackupError::Invalid("the backup would be larger than 1 MiB"));
    }
    Ok(format!("{text}\n"))
}

/// Write a backup to `path`: the whole file is flushed under a temporary name
/// first, so an interrupted write never leaves half a backup under that name.
/// Without `replace`, a file already at `path`, even one that appears during
/// the write, is never replaced and the error is `AlreadyExists`. On a drive
/// without hard links that guarantee comes from creating `path` directly, and
/// a failed write there removes the file it created.
pub fn save(path: &Path, text: &str, replace: bool) -> io::Result<()> {
    if !replace && fs::symlink_metadata(path).is_ok() {
        return Err(io::ErrorKind::AlreadyExists.into());
    }
    let name = path.file_name().ok_or(io::ErrorKind::InvalidInput)?;
    // A fresh temporary name each time: one left by an interrupted write never
    // blocks a later backup under the same name.
    let (partial, mut file) = (0..100u32)
        .find_map(|attempt| {
            let partial = path.with_file_name(format!(
                ".{}.{}-{attempt}.partial",
                name.to_string_lossy(),
                std::process::id()
            ));
            match create_private(&partial) {
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => None,
                opened => Some(opened.map(|file| (partial, file))),
            }
        })
        .unwrap_or_else(|| Err(io::ErrorKind::AlreadyExists.into()))?;
    let written = io::Write::write_all(&mut file, text.as_bytes()).and_then(|()| file.sync_all());
    drop(file);
    let placed = written.and_then(|()| {
        if replace {
            return fs::rename(&partial, path);
        }
        // A link takes the name only if it is free. Some drives and cloud
        // folders have no links; there the name is created only if it is free.
        match fs::hard_link(&partial, path) {
            Err(error) if error.kind() != io::ErrorKind::AlreadyExists => {
                let mut file = create_private(path)?;
                io::Write::write_all(&mut file, text.as_bytes())
                    .and_then(|()| file.sync_all())
                    .inspect_err(|_| {
                        let _ = fs::remove_file(path);
                    })
            }
            other => other,
        }
    });
    let _ = fs::remove_file(&partial);
    placed
}

/// Create a new file that only its owner can read: a backup names a private
/// bibliography path, like the settings file.
fn create_private(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
}

/// A change as previews show it: a bibliography by its file name only, so
/// no private folder is shown or logged.
pub fn shown(change: &Change) -> Change {
    let shown = |value: &Option<Value>| match value {
        Some(Value::Object(library)) if change.setting == "library" => match library.get("bibtex") {
            Some(Value::String(path)) => {
                let name = path.rsplit(['/', '\\']).next().unwrap_or_default();
                Some(serde_json::json!({ "bibtex": name }))
            }
            _ => Some(Value::Object(library.clone())),
        },
        other => other.clone(),
    };
    Change {
        setting: change.setting.clone(),
        from: shown(&change.from),
        to: shown(&change.to),
    }
}

/// Read and check a whole backup file, without waiting for a cloud download.
pub fn read(path: &Path) -> Result<Backup, BackupError> {
    if path.extension().is_some_and(|extension| extension == "icloud") {
        return Err(BackupError::NotDownloaded);
    }
    let metadata = fs::metadata(path).map_err(BackupError::Unreadable)?;
    if placeholder(&metadata) {
        return Err(BackupError::NotDownloaded);
    }
    let bytes = super::read_regular_bytes(path).map_err(BackupError::Unreadable)?;
    let text = String::from_utf8(bytes).map_err(|_| BackupError::Invalid("the backup is not UTF-8 text"))?;
    parse(&text)
}

/// A file a sync service shows but has not downloaded: macOS marks it dataless,
/// and Windows marks it offline or to be fetched when opened or read.
fn placeholder(metadata: &fs::Metadata) -> bool {
    #[cfg(target_os = "macos")]
    {
        use std::os::macos::fs::MetadataExt;
        const SF_DATALESS: u32 = 0x4000_0000;
        metadata.st_flags() & SF_DATALESS != 0
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const OFFLINE: u32 = 0x1000;
        const RECALL_ON_OPEN: u32 = 0x4_0000;
        const RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;
        metadata.file_attributes() & (OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) != 0
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = metadata;
        false
    }
}

/// Check a whole backup's text.
pub fn parse(text: &str) -> Result<Backup, BackupError> {
    if text.len() as u64 > MAX_SETTINGS_BYTES {
        return Err(BackupError::Invalid("it is larger than 1 MiB"));
    }
    let Ok(Value::Object(mut backup)) = serde_json::from_str::<Value>(text) else {
        return Err(BackupError::NotABackup);
    };
    let Some(format) = backup.remove(MARKER) else {
        // Any object would parse as settings with unknown fields; a settings
        // file is one that holds at least one setting this build knows.
        let fields = backup.len();
        let settings = super::parse_value(Value::Object(backup));
        return Err(match settings {
            Ok(settings) if settings.unknown.len() < fields => BackupError::SettingsFile,
            _ => BackupError::NotABackup,
        });
    };
    match format.as_u64() {
        Some(FORMAT) => {}
        Some(newer) if newer > FORMAT => return Err(BackupError::NewerFormat(newer)),
        _ => return Err(BackupError::Invalid("its format is not one Ibis reads")),
    }
    let made_on = backup
        .remove("made_on")
        .and_then(|system| serde_json::from_value::<System>(system).ok())
        .ok_or(BackupError::Invalid("it does not say which system made it"))?;
    let made_by = match backup.remove("made_by") {
        Some(Value::String(version)) if version.len() <= 64 && !version.chars().any(char::is_control) => version,
        _ => return Err(BackupError::Invalid("it does not say which Ibis made it")),
    };
    let Some(Value::Object(mut settings)) = backup.remove("settings") else {
        return Err(BackupError::Invalid("it holds no settings"));
    };
    if !backup.is_empty() {
        return Err(BackupError::Invalid("it has entries Ibis does not know"));
    }
    for local in LOCAL {
        settings.remove(local);
    }
    // A shortcut written as plain text belongs to the system that made the backup.
    if let Some(Value::String(shortcut)) = settings.get("shortcut") {
        let by_system = Map::from_iter([(system_name(made_on), Value::String(shortcut.clone()))]);
        settings.insert("shortcut".into(), Value::Object(by_system));
    }
    let settings = super::parse_value(Value::Object(settings))?;
    Ok(Backup {
        made_on,
        made_by,
        settings,
    })
}

fn system_name(system: System) -> String {
    serde_json::to_value(system)
        .ok()
        .and_then(|name| name.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// One setting a restore changes. Values are as the settings file writes them,
/// and `None` means unset (the default). The opening shortcut is one setting
/// per system: `shortcut.macos`, `shortcut.windows` and `shortcut.linux`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub setting: String,
    pub from: Option<Value>,
    pub to: Option<Value>,
}

/// Something the person should know before restoring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    /// The backup's bibliography file is not on this computer. It is restored
    /// anyway and found again once the file is there.
    LibraryNotFound,
    /// The backup has no opening shortcut for this system; this computer's is kept.
    ShortcutKept,
    /// This computer's settings file is damaged or missing. Restoring writes
    /// it, even with the same settings, and keeps a damaged file beside it.
    RepairsSettings,
}

/// What a restore would do, worked out from the settings as they are now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The settings the plan was made from. A restore is refused if they changed.
    pub current: Settings,
    /// The settings after restoring.
    pub restored: Settings,
    pub changes: Vec<Change>,
    pub notes: Vec<Note>,
}

impl Plan {
    /// The settings file holds the backup's settings already, undamaged.
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty() && !self.notes.contains(&Note::RepairsSettings)
    }

    /// Mark the plan as replacing a damaged settings file.
    #[must_use]
    pub fn repairing(mut self, damaged: bool) -> Self {
        if damaged && !self.notes.contains(&Note::RepairsSettings) {
            self.notes.push(Note::RepairsSettings);
        }
        self
    }
}

/// What restoring `backup` over `current` changes. `exists` says whether a
/// file is on this computer; the caller bounds how long that may take.
pub fn plan(current: &Settings, backup: &Backup, exists: impl FnOnce(&Path) -> bool) -> Plan {
    let mut restored = backup.settings.clone();
    restored.schema_version = SCHEMA_VERSION;
    restored.picker_windows = current.picker_windows.clone();
    restored.delights = current.delights.clone();
    restored.editor_endpoint = current.editor_endpoint.clone();
    restored.update_checks = current.update_checks;
    restored.zotero_profile = current.zotero_profile.clone();
    let mut notes = Vec::new();
    // Each system keeps its own shortcut unless the backup has one for it.
    let mut shortcuts: BTreeMap<System, String> = current.shortcut.by_system().clone();
    if !backup.settings.shortcut.by_system().contains_key(&System::HERE) && current.shortcut.here().is_some() {
        notes.push(Note::ShortcutKept);
    }
    shortcuts.extend(backup.settings.shortcut.by_system().clone());
    restored.shortcut = Shortcuts::from(shortcuts);
    if let Some(LibrarySetting::Bibtex(file)) = &restored.library
        && !exists(file)
    {
        notes.push(Note::LibraryNotFound);
    }
    let changes = changes(current, &restored);
    Plan {
        current: current.clone(),
        restored,
        changes,
        notes,
    }
}

fn changes(current: &Settings, restored: &Settings) -> Vec<Change> {
    let fields = |settings: &Settings| {
        let mut fields = match serde_json::to_value(settings) {
            Ok(Value::Object(fields)) => fields,
            _ => Map::new(),
        };
        fields.remove("schema_version");
        fields.remove("shortcut");
        for (system, shortcut) in settings.shortcut.by_system() {
            fields.insert(
                format!("shortcut.{}", system_name(*system)),
                Value::String(shortcut.clone()),
            );
        }
        fields
    };
    let (mut before, mut after) = (fields(current), fields(restored));
    let mut names: Vec<String> = before.keys().chain(after.keys()).cloned().collect();
    names.sort();
    names.dedup();
    names
        .into_iter()
        .filter_map(|setting| {
            let (from, to) = (before.remove(&setting), after.remove(&setting));
            (from != to).then_some(Change { setting, from, to })
        })
        .collect()
}

/// Whether a restore must write the settings file even when the settings
/// recovered for it already match the backup: it is damaged, or missing with
/// only `settings.json.bak` left.
pub fn needs_repair(loaded: &super::Loaded) -> bool {
    is_damaged(loaded.problem.as_ref()) || (loaded.primary_read == PrimaryRead::Missing && loaded.from_backup)
}

fn is_damaged(problem: Option<&SettingsError>) -> bool {
    match problem {
        Some(SettingsError::Invalid(_) | SettingsError::InvalidZoteroLibraryEntry { .. }) => true,
        Some(SettingsError::Unreadable(error)) => error.kind() == io::ErrorKind::InvalidData,
        _ => false,
    }
}

/// Save `plan.restored`, if the settings file still holds `plan.current`. The
/// settings it replaces become `settings.json.bak`. A damaged settings file is
/// kept as `settings.json.invalid-N` and replaced, since a backup is how people
/// recover from one; a file from a newer Ibis or one that cannot be read is
/// left alone.
pub fn restore(path: &Path, plan: &Plan) -> Result<Settings, BackupError> {
    let mut loaded = super::load(path);
    let missing = loaded.primary_read == PrimaryRead::Missing;
    let repair = needs_repair(&loaded);
    let damaged = match loaded.problem.take() {
        None => false,
        Some(problem) if is_damaged(Some(&problem)) => true,
        // With no settings file, nothing is replaced; an unusable old backup is kept.
        Some(SettingsError::Unreadable(_)) if missing => false,
        Some(SettingsError::Backup(error)) if missing && !matches!(*error, SettingsError::NewerVersion(_)) => false,
        Some(problem) => return Err(problem.into()),
    };
    if loaded.settings != plan.current {
        return Err(BackupError::Changed);
    }
    if plan.restored != loaded.settings || repair {
        super::write_settings(path, &plan.restored, damaged)?;
    }
    Ok(plan.restored.clone())
}
