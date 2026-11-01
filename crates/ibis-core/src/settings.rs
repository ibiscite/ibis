//! User settings, stored as versioned JSON.
//!
//! Settings are small and human-editable. Saving is atomic, flushed to disk
//! before the old file is replaced, and keeps the previous file as a backup,
//! so a crash, a power cut or a bad edit never loses the last working
//! settings. Fields this build does not know are kept on save, so builds
//! that add fields within one format do not erase each other's (ADR 0022).
//! Other format versions are refused, without migration (ADR 0016).

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::cite::KeyFormat;

/// Version of the settings format this build reads and writes.
pub const SCHEMA_VERSION: u32 = 2;

/// Settings are small; bound both primary and backup reads, including growth
/// after metadata was checked. Recovery uses the same bound for damaged bytes.
const MAX_SETTINGS_BYTES: u64 = 1024 * 1024;

/// The library Ibis opens at start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LibrarySetting {
    /// A BibTeX/BibLaTeX file: `{"bibtex": "/path/refs.bib"}`.
    Bibtex(PathBuf),
    /// Zotero libraries, read from Zotero itself: `{"zotero": "user"}`,
    /// `{"zotero": {"group": 42}}`, or several read together:
    /// `{"zotero": ["user", {"group": 42}]}`.
    Zotero(ZoteroLibraries),
}

/// One or more Zotero libraries, in the order chosen, each at most once.
/// One library is written as itself and several as a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "OneOrMore", into = "OneOrMore")]
pub struct ZoteroLibraries(Vec<ZoteroLibrary>);

impl ZoteroLibraries {
    /// The libraries, or `None` when there are none or one repeats.
    pub fn new(libraries: Vec<ZoteroLibrary>) -> Option<Self> {
        let repeats = libraries.iter().enumerate().any(|(i, l)| libraries[..i].contains(l));
        (!libraries.is_empty() && !repeats).then_some(Self(libraries))
    }

    pub fn libraries(&self) -> &[ZoteroLibrary] {
        &self.0
    }
}

impl From<ZoteroLibrary> for ZoteroLibraries {
    fn from(library: ZoteroLibrary) -> Self {
        Self(vec![library])
    }
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum OneOrMore {
    One(ZoteroLibrary),
    More(Vec<ZoteroLibrary>),
}

impl TryFrom<OneOrMore> for ZoteroLibraries {
    type Error = &'static str;
    fn try_from(value: OneOrMore) -> Result<Self, Self::Error> {
        match value {
            OneOrMore::One(library) => Ok(library.into()),
            OneOrMore::More(libraries) => {
                Self::new(libraries).ok_or("Zotero libraries must be a non-empty list without repeats")
            }
        }
    }
}

impl From<ZoteroLibraries> for OneOrMore {
    fn from(value: ZoteroLibraries) -> Self {
        match value.0.as_slice() {
            [library] => OneOrMore::One(*library),
            _ => OneOrMore::More(value.0),
        }
    }
}

/// Which Zotero library to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "type-export", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum ZoteroLibrary {
    /// The personal library ("My Library").
    User,
    /// A group library, by its numeric group ID.
    Group(u64),
}

/// A bundled citation style the picker can format with (ADR 0020).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "type-export", derive(specta::Type))]
#[serde(rename_all = "kebab-case")]
pub enum CitationStyle {
    Apa,
    ChicagoAuthorDate,
    HarvardCiteThemRight,
    Ieee,
    ModernLanguageAssociation,
}

/// How much each result row shows (ADR 0020).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "type-export", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum Density {
    Compact,
    Comfortable,
    Spacious,
}

/// What leads the search field: the Ibis mark, a magnifier or nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "type-export", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum SearchIcon {
    Ibis,
    Magnifier,
    None,
}

/// Picker content size and position relative to a display's work area, in
/// logical pixels. Display changes clamp these values before presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickerWindow {
    pub width: u32,
    pub height: u32,
    /// Moving the header remembers a place without turning the default size
    /// into a custom size. Only an edge drag sets this flag.
    pub custom_size: bool,
    pub x: i32,
    pub y: i32,
}

/// Quiet, device-local feedback. No reference text or library paths are stored.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Delights {
    pub first_run: [i32; 3],
    pub paste_count: u32,
    pub paste_tip: bool,
    pub welcome: bool,
    pub libraries: BTreeMap<String, u8>,
    pub about_clicks: u8,
    pub fact_index: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delights: Option<Delights>,
    /// Quiet bird facts and milestones; absent means enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small_surprises: Option<bool>,
    /// Explicit global shortcut, e.g. `"CommandOrControl+Shift+Y"`; `None` stays unwritten.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
    /// Library to open at start.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library: Option<LibrarySetting>,
    /// Syntax the picker copies citations in; remembered when chosen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copy_format: Option<KeyFormat>,
    /// Whether Ibis checks GitHub for updates (ADR 0019). `None` means the
    /// person has not been asked yet; nothing is checked until they say yes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_checks: Option<bool>,
    /// Citation style the picker formats with; `None` copies in `copy_format`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citation_style: Option<CitationStyle>,
    /// Result row density. `None` means comfortable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub density: Option<Density>,
    /// Larger text in the picker. `None` means the normal size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub larger_text: Option<bool>,
    /// Frames explicitly dragged by the person, indexed by display.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub picker_windows: BTreeMap<String, PickerWindow>,
    /// Whether to show the citation output. `None` means shown (ADR 0020).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_paste_preview: Option<bool>,
    /// The Ibis mark in place of the search glyph. `None` means the magnifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_icon: Option<SearchIcon>,
    /// Whether Enter pastes into the app the picker was opened from. `None`
    /// means on; `false` only copies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paste: Option<bool>,
    /// The menu bar item; absent means shown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub menu_bar: Option<bool>,
    /// Fields this build does not know, written by another version of Ibis
    /// or by hand. Kept as they are and written back on save.
    #[serde(flatten)]
    pub unknown: serde_json::Map<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            schema_version: SCHEMA_VERSION,
            delights: None,
            small_surprises: None,
            shortcut: None,
            library: None,
            copy_format: None,
            update_checks: None,
            citation_style: None,
            density: None,
            larger_text: None,
            picker_windows: BTreeMap::new(),
            show_paste_preview: None,
            search_icon: None,
            paste: None,
            menu_bar: None,
            unknown: serde_json::Map::new(),
        }
    }
}

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("the settings file could not be read: {0}")]
    Unreadable(#[from] io::Error),
    #[error("the settings file is not valid: {0}")]
    Invalid(String),
    /// An invalid Zotero list member, counted from one; no member value is kept.
    #[error(
        "the settings file is not valid: Zotero library entry {position} must be \"user\" or {{\"group\": <non-negative integer ID>}}"
    )]
    InvalidZoteroLibraryEntry { position: usize },
    #[error("the settings file was written by a newer version of Ibis (format {0}); it was left unchanged")]
    NewerVersion(u32),
    /// A refused backup when the primary entry is absent; retain its error source.
    #[error("settings.json.bak: {0}")]
    Backup(#[source] Box<SettingsError>),
}

/// The outcome of the existing primary-file read, independent of parsing or backup recovery.
/// This is in-memory provenance, never a persisted setting or raw diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimaryRead {
    /// The read and entry metadata returned NotFound; the backup was tried before defaults.
    Missing,
    /// UTF-8 contents were read; `Loaded::problem` says whether they were valid.
    Present,
    /// The read failed without confirming an absent entry; backup recovery was attempted.
    Unreadable,
}

/// Result of loading: the settings to use, and a problem to show the user, if any.
#[derive(Debug)]
pub struct Loaded {
    pub settings: Settings,
    pub problem: Option<SettingsError>,
    /// Captured by the primary read, not inferred from the returned settings.
    pub primary_read: PrimaryRead,
    /// Recovery provenance from the same read, even if the backup equals defaults.
    pub from_backup: bool,
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

fn parse(text: &str) -> Result<Settings, SettingsError> {
    let invalid = |e: serde_json::Error| SettingsError::Invalid(e.to_string());
    let value: Value = serde_json::from_str(text).map_err(invalid)?;
    // The version is checked before any other field: a newer format may
    // change field types, and must still be recognised as newer.
    let version = match value.get("schema_version") {
        None => u64::from(SCHEMA_VERSION),
        Some(v) => v
            .as_u64()
            .ok_or_else(|| SettingsError::Invalid("schema_version must be a whole number".into()))?,
    };
    if version > u64::from(SCHEMA_VERSION) {
        return Err(SettingsError::NewerVersion(u32::try_from(version).unwrap_or(u32::MAX)));
    }
    if version < u64::from(SCHEMA_VERSION) {
        return Err(SettingsError::Invalid(format!("unsupported settings format {version}")));
    }
    Settings::deserialize(&value).map_err(|error| {
        // Untagged enums discard the failing member's context. Inspect the
        // existing JSON only after failure; never include that value in text.
        if let Some(libraries) = value.pointer("/library/zotero").and_then(Value::as_array) {
            for (index, library) in libraries.iter().enumerate() {
                if ZoteroLibrary::deserialize(library).is_err() {
                    return SettingsError::InvalidZoteroLibraryEntry { position: index + 1 };
                }
            }
        }
        invalid(error)
    })
}

/// Load settings, falling back to the backup, then to defaults. Report a missing
/// primary when a backup is recovered, and other primary problems unconditionally.
/// With no primary, a refused backup reports its own error instead of being treated as absent.
pub fn load(path: &Path) -> Loaded {
    let (primary_read, primary) = match read_regular(path) {
        Ok(text) => (PrimaryRead::Present, parse(&text)),
        Err(e)
            if e.kind() == io::ErrorKind::NotFound
                && fs::symlink_metadata(path).is_err_and(|e| e.kind() == io::ErrorKind::NotFound) =>
        {
            (PrimaryRead::Missing, Err(SettingsError::Unreadable(e)))
        }
        Err(e) => (PrimaryRead::Unreadable, Err(SettingsError::Unreadable(e))),
    };
    match primary {
        Ok(settings) => Loaded {
            settings,
            problem: None,
            primary_read,
            from_backup: false,
        },
        Err(problem) => {
            let backup_path = backup_path(path);
            let backup = read_regular(&backup_path)
                .map_err(SettingsError::from)
                .and_then(|text| parse(&text));
            let from_backup = backup.is_ok();
            let mut problem = Some(problem);
            let settings = backup.unwrap_or_else(|error| {
                if primary_read == PrimaryRead::Missing {
                    let absent = matches!(&error, SettingsError::Unreadable(e) if e.kind() == io::ErrorKind::NotFound)
                        && fs::symlink_metadata(&backup_path).is_err_and(|e| e.kind() == io::ErrorKind::NotFound);
                    problem = (!absent).then_some(SettingsError::Backup(Box::new(error)));
                }
                Settings::default()
            });
            Loaded {
                settings,
                problem,
                primary_read,
                from_backup,
            }
        }
    }
}

/// Save settings atomically, keeping the previous file as a backup. Refuses to
/// replace a file written by a newer version.
pub fn save(path: &Path, settings: &Settings) -> Result<(), SettingsError> {
    write_settings(path, settings, false)
}

fn write_settings(path: &Path, settings: &Settings, recovering: bool) -> Result<(), SettingsError> {
    if let Some(problem) = load(path).problem {
        let source = match &problem {
            SettingsError::Backup(error) => error.as_ref(),
            _ => &problem,
        };
        if matches!(source, SettingsError::NewerVersion(_)) {
            return Err(problem);
        }
    }
    let temporary = path.with_extension("json.tmp");
    let backup = backup_path(path);
    for candidate in [path, temporary.as_path(), backup.as_path()] {
        if let Err(error) = require_regular(candidate) {
            // A dangling primary/backup may point to the fresh output name.
            let absent = error.kind() == io::ErrorKind::NotFound
                && (candidate == temporary.as_path()
                    || fs::symlink_metadata(candidate).is_err_and(|e| e.kind() == io::ErrorKind::NotFound));
            if !absent {
                return Err(SettingsError::Unreadable(error));
            }
        }
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| SettingsError::Invalid(e.to_string()))?;
    // Include the trailing newline: a successful save must remain readable.
    if json.len() as u64 >= MAX_SETTINGS_BYTES {
        return Err(settings_too_large().into());
    }
    let permissions = match fs::metadata(path) {
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    if recovering {
        preserve_invalid(path)?;
    }
    // The new contents reach the disk before they replace the old file, so a
    // power cut leaves either file whole, never an empty one.
    // Never remove an interrupted write: the primary or backup may point to it.
    let (temporary, mut file) = create_temporary(path)?;
    file.write_all(format!("{json}\n").as_bytes())?;
    if let Some(permissions) = permissions {
        file.set_permissions(permissions)?;
    }
    file.sync_all()?;
    drop(file);
    if !recovering && path.exists() {
        fs::copy(path, &backup)?;
        // Windows flushes only a handle opened for writing.
        fs::OpenOptions::new().write(true).open(&backup)?.sync_all()?;
    }
    fs::rename(&temporary, path)?;
    sync_directory(path);
    Ok(())
}

fn create_temporary(path: &Path) -> io::Result<(PathBuf, fs::File)> {
    for number in 1..=1000 {
        let temporary = path.with_extension(format!("json.tmp-{number}"));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        match options.open(&temporary) {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "settings temporary names are exhausted",
    ))
}

/// Copy and flush the damaged bytes before opening the temporary output.
/// Exclusive creation never overwrites an occupied name and works on
/// filesystems without hard links.
fn preserve_invalid(path: &Path) -> io::Result<()> {
    let bytes = match read_regular_bytes(path) {
        // A missing primary has no damaged bytes to preserve. Refuse a dangling
        // symlink rather than discarding an existing entry during recovery.
        Err(error)
            if error.kind() == io::ErrorKind::NotFound
                && fs::symlink_metadata(path).is_err_and(|e| e.kind() == io::ErrorKind::NotFound) =>
        {
            return Ok(());
        }
        other => other?,
    };
    for number in 1..=1000 {
        let invalid = path.with_extension(format!("json.invalid-{number}"));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        // Damaged settings may hold private paths: readable by the owner only.
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        match options.open(&invalid) {
            Ok(mut file) => {
                file.write_all(&bytes)?;
                file.sync_all()?;
                sync_directory(&invalid);
                return Ok(());
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "settings recovery names are exhausted",
    ))
}

/// Flush the folder holding `path`, so the rename itself survives a power
/// cut. Unix only: Windows cannot open a folder as a file, and NTFS journals
/// the rename. Best effort: the file is already in place, and some file
/// systems refuse to flush a folder.
fn sync_directory(path: &Path) {
    #[cfg(unix)]
    if let Some(directory) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        let _ = fs::File::open(directory).and_then(|d| d.sync_all());
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// Change settings on disk: read the file as it is now, apply `change` and
/// save the result. Edits made to the file since it was loaded are kept. Invalid
/// settings with a valid backup are preserved as `settings.json.invalid-N` on
/// save; the backup is kept. Missing primaries and invalid UTF-8 can recover too;
/// other read errors, newer versions and invalid files without a valid backup are refused.
pub fn update(path: &Path, change: impl FnOnce(&mut Settings)) -> Result<Settings, SettingsError> {
    let loaded = load(path);
    let recovering = loaded.from_backup
        && match &loaded.problem {
            Some(SettingsError::Invalid(_) | SettingsError::InvalidZoteroLibraryEntry { .. }) => true,
            Some(SettingsError::Unreadable(error)) => {
                matches!(error.kind(), io::ErrorKind::InvalidData | io::ErrorKind::NotFound)
            }
            _ => false,
        };
    if let Some(problem) = loaded.problem
        && !recovering
    {
        return Err(problem);
    }
    let current = loaded.settings;
    let mut changed = current.clone();
    change(&mut changed);
    if changed != current || recovering {
        write_settings(path, &changed, recovering)?;
    }
    Ok(changed)
}

// Metadata follows ordinary file symlinks, preserving their existing behavior.
// This refuses special files before opening them; it is not an external-writer lock.
fn require_regular(path: &Path) -> io::Result<()> {
    if !fs::metadata(path)?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "settings path is not a regular file",
        ));
    }
    Ok(())
}

fn read_regular(path: &Path) -> io::Result<String> {
    String::from_utf8(read_regular_bytes(path)?).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn read_regular_bytes(path: &Path) -> io::Result<Vec<u8>> {
    require_regular(path)?;
    let file = fs::File::open(path)?;
    if file.metadata()?.len() > MAX_SETTINGS_BYTES {
        return Err(settings_too_large());
    }
    read_bounded(file)
}

fn settings_too_large() -> io::Error {
    io::Error::new(io::ErrorKind::FileTooLarge, "settings file exceeds 1 MiB")
}

fn read_bounded(reader: impl Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(MAX_SETTINGS_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_SETTINGS_BYTES {
        return Err(settings_too_large());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(&format!("ibis-settings-{name}-"))
            .tempdir()
            .unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn missing_file_gives_defaults_without_a_problem() {
        let directory = dir("missing");
        let loaded = load(&directory.path().join("missing/settings.json"));
        assert_eq!(loaded.settings, Settings::default());
        assert!(loaded.problem.is_none());
    }

    #[test]
    fn settings_reads_refuse_oversized_primary_and_backup_files() {
        let directory = dir("bounded");
        let path = directory.path().join("settings.json");
        let mut text = String::from(r#"{"paste":false}"#);
        text.extend(std::iter::repeat_n(' ', 1024 * 1024 - text.len()));
        fs::write(&path, &text).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(load(&path).settings.paste, Some(false), "the limit is inclusive");
        text.push(' ');
        fs::write(&path, &text).unwrap_or_else(|e| panic!("{e}"));
        for _ in 0..2 {
            let loaded = load(&path);
            assert_eq!(loaded.primary_read, PrimaryRead::Unreadable);
            assert!(
                matches!(loaded.problem, Some(SettingsError::Unreadable(e)) if e.kind() == io::ErrorKind::FileTooLarge)
            );
            assert!(update(&path, |s| s.paste = Some(true)).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap_or_default(), text);
        }
        fs::rename(&path, backup_path(&path)).unwrap_or_else(|e| panic!("{e}"));
        let loaded = load(&path);
        assert!(!loaded.from_backup);
        assert!(
            matches!(loaded.problem, Some(SettingsError::Backup(error)) if matches!(error.as_ref(), SettingsError::Unreadable(e) if e.kind() == io::ErrorKind::FileTooLarge))
        );
    }

    #[test]
    fn a_growing_settings_read_stops_after_one_byte_beyond_the_limit() {
        let mut reader = io::Cursor::new(vec![b' '; MAX_SETTINGS_BYTES as usize + 100]);
        let error = read_bounded(&mut reader)
            .err()
            .unwrap_or_else(|| panic!("oversized read"));
        assert_eq!(error.kind(), io::ErrorKind::FileTooLarge);
        assert_eq!(reader.position(), MAX_SETTINGS_BYTES + 1);
    }

    #[test]
    fn a_save_cannot_create_settings_that_exceed_the_read_limit() {
        let directory = dir("oversized-save");
        let path = directory.path().join("settings.json");
        let original = r#"{"paste":false}"#;
        fs::write(&path, original).unwrap_or_else(|e| panic!("{e}"));
        let result = update(&path, |s| {
            s.unknown
                .insert("future".into(), Value::String("x".repeat(MAX_SETTINGS_BYTES as usize)));
        });
        assert!(matches!(result, Err(SettingsError::Unreadable(e)) if e.kind() == io::ErrorKind::FileTooLarge));
        assert_eq!(fs::read_to_string(&path).unwrap_or_default(), original);
        assert!(!backup_path(&path).exists());
        assert_eq!(load(&path).settings.paste, Some(false));
    }

    #[test]
    #[cfg(unix)]
    fn saving_and_recovering_settings_keep_the_existing_file_mode() {
        use std::os::unix::fs::PermissionsExt;
        for recovering in [false, true] {
            let directory = dir("save-mode");
            let path = directory.path().join("settings.json");
            fs::write(&path, r#"{"paste":false}"#).unwrap_or_else(|e| panic!("{e}"));
            if recovering {
                fs::copy(&path, backup_path(&path)).unwrap_or_else(|e| panic!("{e}"));
                fs::write(&path, "{").unwrap_or_else(|e| panic!("{e}"));
            }
            fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap_or_else(|e| panic!("{e}"));
            for larger in [true, false] {
                update(&path, |s| s.larger_text = Some(larger)).unwrap_or_else(|e| panic!("{e}"));
                assert_eq!(
                    fs::metadata(&path)
                        .unwrap_or_else(|e| panic!("{e}"))
                        .permissions()
                        .mode()
                        & 0o777,
                    0o640
                );
                assert_eq!(load(&path).settings.paste, Some(false), "reopening keeps copy only");
            }
        }
    }

    #[test]
    fn primary_read_distinguishes_missing_from_present_default_and_shortcut_choices() {
        let directory = dir("primary-read");
        let path = directory.path().join("settings.json");
        let missing = load(&path);
        assert_eq!(missing.primary_read, PrimaryRead::Missing);
        assert_eq!(missing.settings, Settings::default());
        assert!(missing.problem.is_none());
        assert!(!missing.from_backup);
        for (text, shortcut) in [
            ("{}", None),
            (r#"{"schema_version":2}"#, None),
            (r#"{"shortcut":null}"#, None),
            (r#"{"shortcut":"Control+Command+R"}"#, Some("Control+Command+R")),
            (r#"{"shortcut":"Control+Nonsense"}"#, Some("Control+Nonsense")),
            (r#"{"shortcut":""}"#, Some("")),
        ] {
            fs::write(&path, text).unwrap_or_else(|e| panic!("{e}"));
            let present = load(&path);
            assert_eq!(present.primary_read, PrimaryRead::Present);
            assert_eq!(present.settings.shortcut.as_deref(), shortcut);
            assert!(present.problem.is_none());
            assert!(!present.from_backup);
            assert_eq!(fs::read_to_string(&path).unwrap_or_default(), text);
        }
        // Provenance is captured by that load, not recalculated from the current file.
        assert_eq!(missing.primary_read, PrimaryRead::Missing);
    }

    #[test]
    fn a_missing_primary_loads_an_existing_backup_and_reports_recovery() {
        let directory = dir("missing-with-backup");
        let path = directory.path().join("settings.json");
        let backup = r#"{"shortcut":"Alt+Space"}"#;
        fs::write(backup_path(&path), backup).unwrap_or_else(|e| panic!("{e}"));
        let loaded = load(&path);
        assert_eq!(loaded.primary_read, PrimaryRead::Missing);
        assert_eq!(loaded.settings.shortcut.as_deref(), Some("Alt+Space"));
        assert!(matches!(loaded.problem, Some(SettingsError::Unreadable(e)) if e.kind() == io::ErrorKind::NotFound));
        assert!(loaded.from_backup);
        assert!(!path.exists());
        assert_eq!(fs::read_to_string(backup_path(&path)).unwrap_or_default(), backup);
    }

    #[test]
    fn missing_primary_refuses_a_newer_backup_on_two_save_attempts() {
        for direct_save in [false, true] {
            let directory = dir("missing-newer-backup");
            let path = directory.path().join("settings.json");
            let bytes = r#"{"schema_version":99,"shortcut":{"future":"参考"}}"#.as_bytes();
            fs::write(backup_path(&path), bytes).unwrap_or_else(|e| panic!("{e}"));
            let attempts: Vec<_> = (0..2)
                .map(|attempt| {
                    if direct_save {
                        save(&path, &Settings::default())
                    } else {
                        update(&path, |s| {
                            s.copy_format = Some(if attempt == 0 {
                                KeyFormat::Keys
                            } else {
                                KeyFormat::Typst
                            })
                        })
                        .map(|_| ())
                    }
                })
                .collect();
            assert_eq!(fs::read(backup_path(&path)).unwrap_or_else(|e| panic!("{e}")), bytes);
            assert!(!path.exists());
            for result in &attempts {
                let error = result
                    .as_ref()
                    .err()
                    .unwrap_or_else(|| panic!("expected a refused backup"));
                assert!(error.to_string().contains("settings.json.bak"));
            }
            assert!(
                attempts
                    .into_iter()
                    .all(|result| matches!(result, Err(SettingsError::Backup(error)) if matches!(*error, SettingsError::NewerVersion(99))))
            );
            assert!(
                matches!(load(&path).problem, Some(SettingsError::Backup(error)) if matches!(*error, SettingsError::NewerVersion(99)))
            );
        }
    }

    #[test]
    fn missing_primary_distinguishes_absent_backups_from_refused_backups() {
        for bytes in [b"{".as_slice(), b"\xff"] {
            let directory = dir("missing-refused-backup");
            let path = directory.path().join("settings.json");
            assert!(load(&path).problem.is_none());
            fs::write(backup_path(&path), bytes).unwrap_or_else(|e| panic!("{e}"));
            let problem = load(&path)
                .problem
                .unwrap_or_else(|| panic!("expected a refused backup"));
            assert!(problem.to_string().contains("settings.json.bak"));
            let error = update(&path, |s| s.copy_format = Some(KeyFormat::Keys))
                .err()
                .unwrap_or_else(|| panic!("expected a failed save"));
            assert!(error.to_string().contains("settings.json.bak"));
            assert!(!path.exists());
            assert_eq!(fs::read(backup_path(&path)).unwrap_or_default(), bytes);
        }
    }

    #[test]
    #[cfg(unix)]
    fn missing_primary_recovery_keeps_a_backup_symlinked_to_an_interrupted_write() {
        let directory = dir("backup-temporary-alias");
        let path = directory.path().join("settings.json");
        let temporary = path.with_extension("json.tmp");
        let bytes = br#"{"library":{"bibtex":"refs.bib"},"future_choice":[true,null]}"#;
        fs::write(&temporary, bytes).unwrap_or_else(|e| panic!("{e}"));
        std::os::unix::fs::symlink(&temporary, backup_path(&path)).unwrap_or_else(|e| panic!("{e}"));
        // An occupied output name must never be followed or overwritten either.
        let occupied = path.with_extension("json.tmp-1");
        std::os::unix::fs::symlink(&temporary, &occupied).unwrap_or_else(|e| panic!("{e}"));
        assert!(load(&path).from_backup);
        let saved = update(&path, |s| s.copy_format = Some(KeyFormat::Keys)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(fs::read(backup_path(&path)).unwrap_or_else(|e| panic!("{e}")), bytes);
        assert_eq!(fs::read(&temporary).unwrap_or_else(|e| panic!("{e}")), bytes);
        assert_eq!(fs::read_link(&occupied).unwrap_or_else(|e| panic!("{e}")), temporary);
        assert_eq!(saved.unknown["future_choice"], serde_json::json!([true, null]));
        assert_eq!(load(&path).settings, saved);
        assert!(load(&path).problem.is_none());
        assert!(!path.with_extension("json.invalid-1").exists());
    }

    #[test]
    #[cfg(unix)]
    fn a_dangling_backup_cannot_alias_a_fresh_temporary_output() {
        let directory = dir("dangling-backup-output");
        let path = directory.path().join("settings.json");
        let temporary = path.with_extension("json.tmp-1");
        fs::write(&path, b"{}").unwrap_or_else(|e| panic!("{e}"));
        std::os::unix::fs::symlink(&temporary, backup_path(&path)).unwrap_or_else(|e| panic!("{e}"));
        assert!(update(&path, |s| s.copy_format = Some(KeyFormat::Keys)).is_err());
        assert_eq!(fs::read(&path).unwrap_or_default(), b"{}");
        assert_eq!(
            fs::read_link(backup_path(&path)).unwrap_or_else(|e| panic!("{e}")),
            temporary
        );
        assert!(!temporary.exists());
    }

    #[test]
    fn missing_primary_recovery_preserves_hard_links_and_refuses_temporary_name_exhaustion() {
        for exhausted in [false, true] {
            let directory = dir("temporary-collisions");
            let path = directory.path().join("settings.json");
            let temporary = path.with_extension("json.tmp");
            fs::write(&temporary, b"{}").unwrap_or_else(|e| panic!("{e}"));
            fs::hard_link(&temporary, backup_path(&path)).unwrap_or_else(|e| panic!("{e}"));
            let count = if exhausted { 1000 } else { 1 };
            for number in 1..=count {
                fs::write(path.with_extension(format!("json.tmp-{number}")), b"occupied")
                    .unwrap_or_else(|e| panic!("{e}"));
            }
            assert_eq!(update(&path, |_| {}).is_err(), exhausted);
            assert_eq!(path.exists(), !exhausted);
            assert_eq!(fs::read(&temporary).unwrap_or_default(), b"{}");
            assert_eq!(fs::read(backup_path(&path)).unwrap_or_default(), b"{}");
            for number in 1..=count {
                assert_eq!(
                    fs::read(path.with_extension(format!("json.tmp-{number}"))).unwrap_or_default(),
                    b"occupied"
                );
            }
        }
    }

    #[test]
    fn missing_primary_recovery_keeps_real_settings_after_two_saves() {
        for backup in [
            "{}",
            r#"{"library":{"bibtex":"参考.bib"},"shortcut":"Alt+Space","future_choice":{"kept":[true,null,"参考"]}}"#,
        ] {
            for change_first in [false, true] {
                let directory = dir("missing-recover-save");
                let path = directory.path().join("settings.json");
                fs::write(backup_path(&path), backup).unwrap_or_else(|e| panic!("{e}"));
                let mut expected = parse(backup).unwrap_or_else(|e| panic!("{e}"));
                if change_first {
                    expected.copy_format = Some(KeyFormat::Typst);
                }
                let first = update(&path, |s| {
                    if change_first {
                        s.copy_format = Some(KeyFormat::Typst);
                    }
                })
                .unwrap_or_else(|e| panic!("{e}"));
                assert_eq!(first, expected);
                let restored = load(&path);
                assert_eq!(restored.primary_read, PrimaryRead::Present);
                assert!(!restored.from_backup);
                assert!(restored.problem.is_none());
                assert_eq!(restored.settings, expected);
                assert_eq!(fs::read_to_string(backup_path(&path)).unwrap_or_default(), backup);
                assert!(!path.with_extension("json.invalid-1").exists());
                expected.copy_format = Some(KeyFormat::Keys);
                let second = update(&path, |s| s.copy_format = Some(KeyFormat::Keys)).unwrap_or_else(|e| panic!("{e}"));
                assert_eq!(second, expected);
                assert_eq!(load(&path).settings, expected);
                let saved_backup = fs::read_to_string(backup_path(&path)).unwrap_or_else(|e| panic!("{e}"));
                assert_eq!(parse(&saved_backup).unwrap_or_else(|e| panic!("{e}")), first);
            }
        }
    }

    #[test]
    fn missing_primary_recovery_refuses_a_blocked_temporary_without_touching_the_backup() {
        let directory = dir("missing-blocked-save");
        let path = directory.path().join("settings.json");
        let backup = r#"{"shortcut":"Alt+Space"}"#;
        fs::write(backup_path(&path), backup).unwrap_or_else(|e| panic!("{e}"));
        fs::create_dir(path.with_extension("json.tmp")).unwrap_or_else(|e| panic!("{e}"));
        assert!(update(&path, |s| s.larger_text = Some(true)).is_err());
        assert!(!path.exists());
        assert!(path.with_extension("json.tmp").is_dir());
        assert_eq!(fs::read_to_string(backup_path(&path)).unwrap_or_default(), backup);
        assert!(load(&path).from_backup);
        assert!(!path.with_extension("json.invalid-1").exists());
    }

    #[cfg(unix)]
    #[test]
    fn missing_primary_recovery_preserves_a_dangling_primary_symlink() {
        let directory = dir("missing-primary-link");
        let path = directory.path().join("settings.json");
        let target = directory.path().join("absent.json");
        std::os::unix::fs::symlink(&target, &path).unwrap_or_else(|e| panic!("{e}"));
        fs::write(backup_path(&path), "{}").unwrap_or_else(|e| panic!("{e}"));
        let loaded = load(&path);
        assert!(loaded.from_backup);
        assert_eq!(loaded.primary_read, PrimaryRead::Unreadable);
        assert!(update(&path, |_| {}).is_err());
        assert_eq!(fs::read_link(&path).unwrap_or_else(|e| panic!("{e}")), target);
        assert!(!target.exists());
        assert_eq!(fs::read_to_string(backup_path(&path)).unwrap_or_default(), "{}");
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn unreadable_primary_provenance_keeps_problem_and_default_backup_recovery() {
        for invalid_utf8 in [false, true] {
            let directory = dir("unreadable-primary");
            let path = directory.path().join("settings.json");
            if invalid_utf8 {
                fs::write(&path, [0xff, 0xfe]).unwrap_or_else(|e| panic!("{e}"));
            } else {
                fs::create_dir(&path).unwrap_or_else(|e| panic!("{e}"));
            }
            for with_backup in [false, true] {
                if with_backup {
                    fs::write(backup_path(&path), "{}").unwrap_or_else(|e| panic!("{e}"));
                }
                let loaded = load(&path);
                assert_eq!(loaded.primary_read, PrimaryRead::Unreadable);
                assert!(matches!(loaded.problem, Some(SettingsError::Unreadable(_))));
                assert_eq!(loaded.settings, Settings::default());
                assert_eq!(loaded.from_backup, with_backup);
            }
        }
    }

    #[test]
    fn save_and_load_round_trip() {
        let directory = dir("roundtrip");
        let path = directory.path().join("nested/settings.json");
        let settings = Settings {
            shortcut: Some("Control+Command+R".into()),
            copy_format: Some(KeyFormat::Typst),
            update_checks: Some(false),
            ..Settings::default()
        };
        save(&path, &settings).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(load(&path).settings, settings);
    }

    #[test]
    fn picker_preferences_round_trip_and_stay_optional() {
        for text in ["{}", r#"{"show_paste_preview":null}"#] {
            assert_eq!(parse(text).map(|settings| settings.show_paste_preview).ok(), Some(None));
        }
        let saved = serde_json::to_string(&Settings::default()).unwrap_or_default();
        for field in [
            "citation_style",
            "density",
            "larger_text",
            "show_paste_preview",
            "search_icon",
            "paste",
        ] {
            assert!(!saved.contains(field), "{field} is written only once chosen");
        }
        let settings = parse(
            r#"{"schema_version": 2, "citation_style": "chicago-author-date", "density": "compact", "larger_text": true, "show_paste_preview": false, "search_icon": "magnifier", "paste": false}"#,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(settings.paste, Some(false));
        assert_eq!(settings.search_icon, Some(SearchIcon::Magnifier));
        assert_eq!(settings.citation_style, Some(CitationStyle::ChicagoAuthorDate));
        assert_eq!(settings.density, Some(Density::Compact));
        assert_eq!(settings.larger_text, Some(true));
        assert_eq!(settings.show_paste_preview, Some(false));
        let directory = dir("preferences");
        let path = directory.path().join("settings.json");
        save(&path, &settings).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(load(&path).settings, settings);
    }

    #[test]
    fn unknown_picker_preference_values_are_invalid_not_guessed() {
        for text in [
            r#"{"citation_style": "vancouver"}"#,
            r#"{"density": "tiny"}"#,
            r#"{"larger_text": "yes"}"#,
            r#"{"show_paste_preview": "yes"}"#,
            r#"{"show_paste_preview": 0}"#,
        ] {
            assert!(matches!(parse(text), Err(SettingsError::Invalid(_))), "{text}");
        }
    }

    #[test]
    fn update_checks_stay_unasked_until_the_person_answers() {
        assert_eq!(Settings::default().update_checks, None);
        assert_eq!(parse("{}").map(|s| s.update_checks).ok(), Some(None));
        let saved = serde_json::to_string(&Settings::default()).unwrap_or_default();
        assert!(!saved.contains("update_checks"));
        assert_eq!(
            parse(r#"{"schema_version": 2, "update_checks": true}"#)
                .map(|s| s.update_checks)
                .ok(),
            Some(Some(true))
        );
        assert!(parse(r#"{"schema_version": 2, "update_checks": "yes"}"#).is_err());
    }

    #[test]
    fn corrupt_file_falls_back_to_the_backup_and_reports_it() {
        let directory = dir("corrupt");
        let path = directory.path().join("settings.json");
        let good = Settings {
            shortcut: Some("Alt+Space".into()),
            ..Settings::default()
        };
        save(&path, &good).unwrap_or_else(|e| panic!("{e}"));
        save(&path, &good).unwrap_or_else(|e| panic!("{e}"));
        fs::write(&path, "{ not json").unwrap_or_else(|e| panic!("{e}"));
        let loaded = load(&path);
        assert_eq!(loaded.settings, good);
        assert!(matches!(loaded.problem, Some(SettingsError::Invalid(_))));
        assert!(loaded.from_backup);
    }

    #[test]
    fn a_backup_recovery_saves_and_keeps_each_damaged_file() {
        for backup in [
            "{}",
            r#"{"library":{"bibtex":"refs.bib"},"future_choice":{"kept":[true,null,"参考"]}}"#,
        ] {
            let directory = dir("recover-save");
            let path = directory.path().join("settings.json");
            fs::write(backup_path(&path), backup).unwrap_or_else(|e| panic!("{e}"));
            let occupied = path.with_extension("json.invalid-1");
            fs::write(&occupied, "previous damaged settings").unwrap_or_else(|e| panic!("{e}"));
            let damaged = [
                b"".as_slice(),
                b"{ not json",
                br#"{"copy_format":"unknown"}"#,
                br#"{"library":{"zotero":["user",{"group":-1}]}}"#,
                b"\xff",
            ];
            for (index, bad) in damaged.into_iter().enumerate() {
                fs::write(&path, bad).unwrap_or_else(|e| panic!("{e}"));
                let loaded = load(&path);
                assert!(loaded.from_backup);
                // Even a save with no changed values must finish recovery.
                let saved = update(&path, |_| {}).unwrap_or_else(|e| panic!("recovery save failed: {e}"));
                assert_eq!(saved, loaded.settings);
                assert!(load(&path).problem.is_none());
                assert_eq!(load(&path).settings, saved);
                assert_eq!(fs::read_to_string(backup_path(&path)).unwrap_or_default(), backup);
                assert_eq!(
                    fs::read_to_string(&occupied).unwrap_or_default(),
                    "previous damaged settings"
                );
                for (previous, expected) in damaged.into_iter().take(index + 1).enumerate() {
                    assert_eq!(
                        fs::read(path.with_extension(format!("json.invalid-{}", previous + 2)))
                            .unwrap_or_else(|e| panic!("{e}")),
                        expected
                    );
                }
            }
        }
    }

    #[test]
    #[cfg(unix)]
    fn recovery_keeps_the_target_of_a_primary_symlink_to_an_interrupted_write() {
        let directory = dir("symlink-recovery");
        let path = directory.path().join("settings.json");
        let temporary = path.with_extension("json.tmp");
        let damaged = b"{ damaged\xff";
        fs::write(&temporary, damaged).unwrap_or_else(|e| panic!("{e}"));
        std::os::unix::fs::symlink(&temporary, &path).unwrap_or_else(|e| panic!("{e}"));
        fs::write(backup_path(&path), "{}").unwrap_or_else(|e| panic!("{e}"));
        let occupied = path.with_extension("json.invalid-1");
        std::os::unix::fs::symlink("missing-synthetic-target", &occupied).unwrap_or_else(|e| panic!("{e}"));
        fs::create_dir(path.with_extension("json.invalid-2")).unwrap_or_else(|e| panic!("{e}"));

        let saved = update(&path, |s| s.copy_format = Some(KeyFormat::Keys)).unwrap_or_else(|e| panic!("{e}"));
        let archive = path.with_extension("json.invalid-3");
        assert_eq!(fs::read(&archive).unwrap_or_else(|e| panic!("{e}")), damaged);
        assert!(
            fs::symlink_metadata(&archive)
                .unwrap_or_else(|e| panic!("{e}"))
                .is_file()
        );
        assert_eq!(
            fs::read_link(&occupied).unwrap_or_else(|e| panic!("{e}")),
            Path::new("missing-synthetic-target")
        );
        assert!(path.with_extension("json.invalid-2").is_dir());
        assert_eq!(load(&path).settings, saved);
        assert!(load(&path).problem.is_none());
        assert_eq!(fs::read_to_string(backup_path(&path)).unwrap_or_default(), "{}");
        assert_eq!(fs::read(&temporary).unwrap_or_else(|e| panic!("{e}")), damaged);
    }

    #[test]
    fn damaged_settings_archive_is_an_independent_copy() {
        let directory = dir("independent-archive");
        let path = directory.path().join("settings.json");
        let damaged = b"{ damaged\xff";
        fs::write(&path, damaged).unwrap_or_else(|e| panic!("{e}"));
        preserve_invalid(&path).unwrap_or_else(|e| panic!("{e}"));
        // A later edit to the old file must never change the preserved bytes.
        fs::write(&path, b"changed after preservation").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            fs::read(path.with_extension("json.invalid-1")).unwrap_or_else(|e| panic!("{e}")),
            damaged
        );
    }

    #[test]
    fn failed_recovery_preserves_the_primary_and_backup() {
        for blocked_temporary in [false, true] {
            let directory = dir("blocked-recovery");
            let path = directory.path().join("settings.json");
            fs::write(&path, "{ damaged").unwrap_or_else(|e| panic!("{e}"));
            fs::write(backup_path(&path), "{}").unwrap_or_else(|e| panic!("{e}"));
            if blocked_temporary {
                fs::create_dir(path.with_extension("json.tmp")).unwrap_or_else(|e| panic!("{e}"));
            } else {
                fs::write(path.with_extension("json.tmp"), "unfinished save").unwrap_or_else(|e| panic!("{e}"));
                for number in 1..=1000 {
                    fs::write(path.with_extension(format!("json.invalid-{number}")), "kept")
                        .unwrap_or_else(|e| panic!("{e}"));
                }
            }
            assert!(update(&path, |s| s.copy_format = Some(KeyFormat::Keys)).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap_or_default(), "{ damaged");
            assert_eq!(fs::read_to_string(backup_path(&path)).unwrap_or_default(), "{}");
            assert!(load(&path).from_backup);
            if !blocked_temporary {
                assert_eq!(
                    fs::read_to_string(path.with_extension("json.tmp")).unwrap_or_default(),
                    "unfinished save"
                );
                for number in 1..=1000 {
                    assert_eq!(
                        fs::read_to_string(path.with_extension(format!("json.invalid-{number}"))).unwrap_or_default(),
                        "kept"
                    );
                }
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn recovery_never_writes_through_a_planted_temporary_link() {
        let directory = dir("temporary-link");
        let path = directory.path().join("settings.json");
        fs::write(&path, "{ damaged").unwrap_or_else(|e| panic!("{e}"));
        fs::write(backup_path(&path), "{}").unwrap_or_else(|e| panic!("{e}"));
        std::os::unix::fs::symlink(path.with_extension("json.invalid-1"), path.with_extension("json.tmp"))
            .unwrap_or_else(|e| panic!("{e}"));
        update(&path, |s| s.copy_format = Some(KeyFormat::Keys)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            fs::read_to_string(path.with_extension("json.invalid-1")).unwrap_or_default(),
            "{ damaged"
        );
        assert!(load(&path).problem.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn preserved_damaged_settings_are_readable_by_the_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let directory = dir("archive-mode");
        let path = directory.path().join("settings.json");
        fs::write(&path, "{ damaged").unwrap_or_else(|e| panic!("{e}"));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap_or_else(|e| panic!("{e}"));
        fs::write(backup_path(&path), "{}").unwrap_or_else(|e| panic!("{e}"));
        update(&path, |_| {}).unwrap_or_else(|e| panic!("{e}"));
        let mode = fs::metadata(path.with_extension("json.invalid-1"))
            .unwrap_or_else(|e| panic!("{e}"))
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0);
    }

    #[test]
    fn an_invalid_backup_cannot_authorize_recovery() {
        for backup in [
            b"{".as_slice(),
            br#"{"schema_version":1}"#,
            br#"{"schema_version":99}"#,
            b"\xff",
        ] {
            let directory = dir("invalid-backup");
            let path = directory.path().join("settings.json");
            fs::write(&path, "{ damaged").unwrap_or_else(|e| panic!("{e}"));
            fs::write(backup_path(&path), backup).unwrap_or_else(|e| panic!("{e}"));
            assert!(!load(&path).from_backup);
            let mut changed = false;
            assert!(update(&path, |_| changed = true).is_err());
            assert!(!changed);
            assert_eq!(fs::read_to_string(&path).unwrap_or_default(), "{ damaged");
            assert_eq!(fs::read(backup_path(&path)).unwrap_or_default(), backup);
            assert!(!path.with_extension("json.invalid-1").exists());
            assert!(!path.with_extension("json.tmp").exists());
        }
    }

    #[test]
    fn settings_recovery_reports_backup_or_defaults_from_the_same_read() {
        let directory = dir("recovery-source");
        let path = directory.path().join("settings.json");
        fs::create_dir_all(
            path.parent()
                .unwrap_or_else(|| panic!("expected a synthetic settings directory")),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        for primary in ["{ invalid", r#"{"schema_version":3}"#] {
            fs::write(&path, primary).unwrap_or_else(|e| panic!("{e}"));
            let fallback = load(&path);
            assert_eq!(fallback.primary_read, PrimaryRead::Present);
            assert!(fallback.problem.is_some());
            assert!(!fallback.from_backup);
            fs::write(backup_path(&path), "{}").unwrap_or_else(|e| panic!("{e}"));
            let recovered = load(&path);
            assert_eq!(recovered.primary_read, PrimaryRead::Present);
            assert!(recovered.problem.is_some());
            assert!(recovered.from_backup);
            assert_eq!(recovered.settings, Settings::default());
            fs::remove_file(backup_path(&path)).unwrap_or_else(|e| panic!("{e}"));
        }
    }

    #[test]
    fn a_newer_settings_file_is_never_overwritten() {
        let directory = dir("newer");
        let path = directory.path().join("settings.json");
        fs::create_dir_all(path.parent().unwrap_or(Path::new("."))).unwrap_or_else(|e| panic!("{e}"));
        fs::write(&path, r#"{"schema_version": 99, "shortcut": "Alt+X"}"#).unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(load(&path).problem, Some(SettingsError::NewerVersion(99))));
        assert!(matches!(
            save(&path, &Settings::default()),
            Err(SettingsError::NewerVersion(99))
        ));
        assert!(fs::read_to_string(&path).unwrap_or_default().contains("99"));
    }

    #[test]
    fn newer_files_are_refused_even_when_their_fields_changed_type() {
        let directory = dir("newer-types");
        let path = directory.path().join("settings.json");
        fs::create_dir_all(path.parent().unwrap_or_else(|| panic!("no parent"))).unwrap_or_else(|e| panic!("{e}"));
        let newer = r#"{"schema_version": 99, "shortcut": {"key": "Space"}}"#;
        fs::write(&path, newer).unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(load(&path).problem, Some(SettingsError::NewerVersion(99))));
        assert!(matches!(
            save(&path, &Settings::default()),
            Err(SettingsError::NewerVersion(99))
        ));
        assert_eq!(fs::read_to_string(&path).unwrap_or_default(), newer);
        for with_backup in [false, true] {
            if with_backup {
                fs::write(backup_path(&path), "{}").unwrap_or_else(|e| panic!("{e}"));
            }
            assert_eq!(load(&path).from_backup, with_backup);
            let mut changed = false;
            assert!(matches!(
                update(&path, |_| changed = true),
                Err(SettingsError::NewerVersion(99))
            ));
            assert!(!changed);
            assert_eq!(fs::read_to_string(&path).unwrap_or_default(), newer);
            assert!(!path.with_extension("json.invalid-1").exists());
            assert!(!path.with_extension("json.tmp").exists());
            if with_backup {
                assert_eq!(fs::read_to_string(backup_path(&path)).unwrap_or_default(), "{}");
            }
        }
    }

    #[test]
    fn updates_keep_edits_made_since_loading_and_refuse_invalid_files_without_a_backup() {
        let directory = dir("update");
        let path = directory.path().join("settings.json");
        update(&path, |s| s.copy_format = Some(KeyFormat::Typst)).unwrap_or_else(|e| panic!("{e}"));
        // The user edits the file while the app runs.
        let edited = r#"{"schema_version": 2, "shortcut": "Alt+Space", "copy_format": "typst"}"#;
        fs::write(&path, edited).unwrap_or_else(|e| panic!("{e}"));
        let after = update(&path, |s| s.copy_format = Some(KeyFormat::Keys)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(after.shortcut.as_deref(), Some("Alt+Space"));
        assert_eq!(load(&path).settings, after);
        fs::remove_file(backup_path(&path)).unwrap_or_else(|e| panic!("{e}"));
        fs::write(&path, "{ not json").unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(
            update(&path, |s| s.copy_format = Some(KeyFormat::Pandoc)),
            Err(SettingsError::Invalid(_))
        ));
        assert_eq!(fs::read_to_string(&path).unwrap_or_default(), "{ not json");
    }

    #[test]
    fn saves_keep_fields_another_ibis_version_wrote() {
        let directory = dir("unknown");
        let path = directory.path().join("settings.json");
        let newer_build = serde_json::json!({
            "schema_version": 2,
            "citation_style": "apa",
            "window_position": { "display": "synthetic", "x": 10, "y": -20 },
            "later_flag": [true, null, 1.5],
        });
        fs::write(&path, newer_build.to_string()).unwrap_or_else(|e| panic!("{e}"));
        update(&path, |s| s.shortcut = Some("Alt+Space".into())).unwrap_or_else(|e| panic!("{e}"));
        let saved: Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap_or_default()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(saved["shortcut"], "Alt+Space");
        assert_eq!(saved["citation_style"], "apa");
        assert_eq!(saved["window_position"], newer_build["window_position"]);
        assert_eq!(saved["later_flag"], newer_build["later_flag"]);
    }

    #[test]
    fn a_temporary_file_left_by_an_interrupted_save_does_not_leak_into_the_next() {
        let directory = dir("interrupted");
        let path = directory.path().join("settings.json");
        let stale = format!("{{\"schema_version\": 2, \"shortcut\": \"{}\"}}", "x".repeat(4096));
        fs::write(path.with_extension("json.tmp"), &stale).unwrap_or_else(|e| panic!("{e}"));
        save(&path, &Settings::default()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(load(&path).settings, Settings::default());
        assert!(load(&path).problem.is_none());
        assert_eq!(
            fs::read_to_string(path.with_extension("json.tmp")).unwrap_or_default(),
            stale
        );
    }

    #[test]
    fn old_formats_use_invalid_settings_recovery_without_migration() {
        for old in [
            r#"{"schema_version": 1, "library": "/synthetic/refs.bib", "copy_format": "typst"}"#,
            r#"{"library": "/synthetic/refs.bib", "copy_format": "typst"}"#,
            r#"{"schema_version": 1, "shortcut": "Alt+Y"}"#,
            r#"{"schema_version": 0, "library": {"bibtex": "/synthetic/refs.bib"}}"#,
        ] {
            let directory = dir("old-format");
            let path = directory.path().join("settings.json");
            fs::write(&path, old).unwrap_or_else(|e| panic!("{e}"));
            let loaded = load(&path);
            assert!(matches!(loaded.problem, Some(SettingsError::Invalid(_))));
            assert_eq!(loaded.settings, Settings::default());
            assert!(!loaded.from_backup);
            assert!(matches!(
                update(&path, |_| panic!("cannot update an old format without a valid backup")),
                Err(SettingsError::Invalid(_))
            ));
            assert_eq!(fs::read_to_string(&path).unwrap_or_default(), old);

            let backup = r#"{"schema_version":2,"shortcut":"Alt+Space"}"#;
            fs::write(backup_path(&path), backup).unwrap_or_else(|e| panic!("{e}"));
            let loaded = load(&path);
            assert!(matches!(loaded.problem, Some(SettingsError::Invalid(_))));
            assert!(loaded.from_backup);
            assert_eq!(loaded.settings.shortcut.as_deref(), Some("Alt+Space"));
            assert_eq!(loaded.settings.copy_format, None);
            assert_eq!(loaded.settings.library, None);
            assert_eq!(fs::read_to_string(&path).unwrap_or_default(), old);

            let saved = update(&path, |_| {}).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(saved, loaded.settings);
            assert_eq!(load(&path).settings, saved);
            assert!(load(&path).problem.is_none());
            assert_eq!(fs::read_to_string(backup_path(&path)).unwrap_or_default(), backup);
            assert_eq!(
                fs::read_to_string(path.with_extension("json.invalid-1")).unwrap_or_default(),
                old
            );
        }
    }

    #[test]
    fn invalid_format_versions_are_refused() {
        for version in ["\"1\"", "null", "-1", "1.5"] {
            assert!(matches!(
                parse(&format!(r#"{{"schema_version":{version}}}"#)),
                Err(SettingsError::Invalid(_))
            ));
        }
    }

    #[test]
    fn zotero_libraries_are_written_as_documented() {
        let cases = [
            (
                LibrarySetting::Zotero(ZoteroLibrary::User.into()),
                serde_json::json!({ "zotero": "user" }),
            ),
            (
                LibrarySetting::Zotero(ZoteroLibrary::Group(42).into()),
                serde_json::json!({ "zotero": { "group": 42 } }),
            ),
            (
                LibrarySetting::Zotero(
                    ZoteroLibraries::new(vec![ZoteroLibrary::User, ZoteroLibrary::Group(42)])
                        .unwrap_or_else(|| panic!("valid")),
                ),
                serde_json::json!({ "zotero": ["user", { "group": 42 }] }),
            ),
            (
                LibrarySetting::Bibtex(PathBuf::from("/synthetic/refs.bib")),
                serde_json::json!({ "bibtex": "/synthetic/refs.bib" }),
            ),
        ];
        for (setting, json) in cases {
            assert_eq!(serde_json::to_value(&setting).unwrap_or_else(|e| panic!("{e}")), json);
            let text = serde_json::json!({ "schema_version": 2, "library": json }).to_string();
            assert_eq!(parse(&text).unwrap_or_else(|e| panic!("{e}")).library, Some(setting));
        }
        for bad in [
            r#"{"zotero": "everyone"}"#,
            r#"{"zotero": {"group": -1}}"#,
            r#"{"zotero": []}"#,
            r#"{"zotero": ["user", "user"]}"#,
            r#"{"zotero": ["user", "everyone"]}"#,
            r#"{"onedrive": "x"}"#,
            r#""/a/bare/path.bib""#,
        ] {
            let text = format!(r#"{{"schema_version": 2, "library": {bad}}}"#);
            assert!(
                matches!(
                    parse(&text),
                    Err(SettingsError::Invalid(_) | SettingsError::InvalidZoteroLibraryEntry { .. })
                ),
                "{bad}"
            );
        }
    }

    #[test]
    fn invalid_zotero_list_members_name_the_position_without_the_value() {
        for bad in [
            serde_json::json!("synthetic-private-value"),
            serde_json::json!({ "group": "synthetic-private-value" }),
            serde_json::json!({ "group": -1 }),
            serde_json::json!({ "group": 1.5 }),
            serde_json::json!({ "group": null }),
            serde_json::json!({ "unknown": "synthetic-private-value" }),
            serde_json::json!({ "group": 42, "unknown": "synthetic-private-value" }),
            serde_json::json!(["synthetic-private-value"]),
            serde_json::json!(null),
            serde_json::json!(false),
            serde_json::json!(42),
        ] {
            for position in 1..=3 {
                let mut libraries = vec![serde_json::json!("user"), serde_json::json!({ "group": 42 })];
                libraries.insert(position - 1, bad.clone());
                let mut settings = serde_json::json!({ "schema_version": 2, "library": { "zotero": libraries } });
                let Err(error) = parse(&settings.to_string()) else {
                    panic!("expected the invalid-member error");
                };
                assert_eq!(
                    error.to_string(),
                    format!(
                        "the settings file is not valid: Zotero library entry {position} must be \"user\" or {{\"group\": <non-negative integer ID>}}"
                    )
                );
                assert!(
                    matches!(error, SettingsError::InvalidZoteroLibraryEntry { position: actual } if actual == position)
                );
                settings["schema_version"] = serde_json::json!(99);
                assert!(matches!(
                    parse(&settings.to_string()),
                    Err(SettingsError::NewerVersion(99))
                ));
            }
        }
    }

    #[test]
    fn an_invalid_zotero_member_is_preserved_when_saving_from_the_backup() {
        let directory = dir("zotero-member");
        let path = directory.path().join("settings.json");
        let good = Settings {
            library: Some(LibrarySetting::Zotero(ZoteroLibrary::User.into())),
            ..Settings::default()
        };
        save(&path, &good).unwrap_or_else(|e| panic!("{e}"));
        save(&path, &good).unwrap_or_else(|e| panic!("{e}"));
        let backup = fs::read(backup_path(&path)).unwrap_or_else(|e| panic!("{e}"));
        let bad = r#"{"schema_version":2,"library":{"zotero":["user",{"group":"synthetic-private-value"}]}}"#;
        fs::write(&path, bad).unwrap_or_else(|e| panic!("{e}"));

        let loaded = load(&path);
        assert_eq!(loaded.settings, good);
        assert!(loaded.from_backup);
        let error = loaded
            .problem
            .unwrap_or_else(|| panic!("expected the invalid-member error"));
        assert_eq!(
            error.to_string(),
            "the settings file is not valid: Zotero library entry 2 must be \"user\" or {\"group\": <non-negative integer ID>}"
        );
        assert_eq!(fs::read_to_string(&path).unwrap_or_default(), bad);
        let saved = update(&path, |s| s.copy_format = Some(KeyFormat::Keys)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(saved.library, good.library);
        assert_eq!(saved.copy_format, Some(KeyFormat::Keys));
        assert_eq!(load(&path).settings, saved);
        assert!(load(&path).problem.is_none());
        assert_eq!(
            fs::read_to_string(path.with_extension("json.invalid-1")).unwrap_or_default(),
            bad
        );
        assert_eq!(fs::read(backup_path(&path)).unwrap_or_default(), backup);
    }
}
