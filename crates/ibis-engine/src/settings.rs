//! User settings, stored as versioned JSON.
//!
//! Settings are small and human-editable. Saving is atomic, flushed to disk
//! before the old file is replaced, and keeps the previous file as a backup,
//! so a crash, a power cut or a bad edit never loses the last working
//! settings. Fields this build does not know are kept on save, so builds
//! that add fields within one format do not erase each other's (ADR 0022).
//! Other format versions are refused, without migration (ADR 0016).
//!
//! The file is meant to work on another computer too (#1561): bibliography
//! files in the home folder are written as `~/…`, and the opening shortcut is
//! kept per system. [`backup`] makes and restores a backup file from it.

pub mod backup;
mod owner;
pub use owner::{Owner, Pending, Watching};
pub mod writings;

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use ibis_core::atomic_file::{AtomicFile, sync_directory};
use ibis_core::destination::System;

/// Version of the settings format this build reads and writes.
pub const SCHEMA_VERSION: u32 = 2;

/// How many apps keep a format of their own; the least recently chosen goes first.
pub const MAX_APP_OUTPUTS: usize = 200;

/// One app's format: an engine output id. Never a document, window or path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppOutput {
    pub app: ibis_core::destination::AppId,
    pub output: String,
}

/// Settings are small; bound both primary and backup reads, including growth
/// after metadata was checked. Recovery uses the same bound for damaged bytes.
const MAX_SETTINGS_BYTES: u64 = 1024 * 1024;

/// The library Ibis opens at start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LibrarySetting {
    /// A BibTeX/BibLaTeX file: `{"bibtex": "/path/refs.bib"}`, or
    /// `{"bibtex": "~/Documents/refs.bib"}` inside the home folder.
    Bibtex(#[serde(with = "home_path")] PathBuf),
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

/// The opening shortcut chosen on each system, so one settings file or backup
/// serves a Mac and a Windows PC alike (#1561). Ibis always writes one entry
/// per system, `"shortcut": {"macos": "Control+Command+R", "windows": "Alt+Space"}`,
/// so a file shared between systems never hands one system's shortcut to
/// another. A plain string, as people write it by hand, is this system's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Shortcuts(BTreeMap<System, String>);

impl Shortcuts {
    /// Only this system's shortcut.
    pub fn only_here(shortcut: impl Into<String>) -> Self {
        Self(BTreeMap::from([(System::HERE, shortcut.into())]))
    }

    /// The explicit shortcut for this system; `None` means the default.
    pub fn here(&self) -> Option<&str> {
        self.0.get(&System::HERE).map(String::as_str)
    }

    /// Choose this system's shortcut, or `None` for the default. Other
    /// systems' shortcuts are kept.
    pub fn set_here(&mut self, shortcut: Option<String>) {
        match shortcut {
            Some(shortcut) => self.0.insert(System::HERE, shortcut),
            None => self.0.remove(&System::HERE),
        };
    }

    /// Every system's explicit shortcut.
    pub fn by_system(&self) -> &BTreeMap<System, String> {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<BTreeMap<System, String>> for Shortcuts {
    fn from(shortcuts: BTreeMap<System, String>) -> Self {
        Self(shortcuts)
    }
}

impl Serialize for Shortcuts {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Shortcuts {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Written {
            Here(String),
            BySystem(BTreeMap<System, String>),
        }
        Ok(match Option::<Written>::deserialize(deserializer)? {
            None => Self::default(),
            Some(Written::Here(shortcut)) => Self(BTreeMap::from([(System::HERE, shortcut)])),
            Some(Written::BySystem(shortcuts)) => Self(shortcuts),
        })
    }
}

/// A bibliography path inside the home folder is written as `~/` and the
/// rest of the path with `/` between names, so the same setting finds the same
/// file on another computer with another user name or system. Other paths are
/// written as they are.
mod home_path {
    use std::path::{Path, PathBuf};

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub(super) fn serialize<S: Serializer>(path: &Path, serializer: S) -> Result<S::Ok, S::Error> {
        match super::portable_path(path, std::env::home_dir().as_deref()) {
            Some(portable) => portable.serialize(serializer),
            None => path.serialize(serializer),
        }
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<PathBuf, D::Error> {
        let written = String::deserialize(deserializer)?;
        Ok(super::resolve_path(&written, std::env::home_dir().as_deref()))
    }
}

/// `path` as `~/…` when it is a file inside `home`; `None` keeps it as it is.
pub fn portable_path(path: &Path, home: Option<&Path>) -> Option<String> {
    let home = home.filter(|home| home.is_absolute())?;
    let names = path
        .strip_prefix(home)
        .ok()?
        .components()
        .map(|component| match component {
            std::path::Component::Normal(name) => name.to_str().filter(|name| !name.contains(['/', '\\'])),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    (!names.is_empty()).then(|| format!("~/{}", names.join("/")))
}

/// A written path, with a leading `~/` (or `~\` on Windows) read as `home`.
/// Without a home folder the text stays as written, so it is reported as not
/// found rather than read somewhere else.
pub fn resolve_path(written: &str, home: Option<&Path>) -> PathBuf {
    let rest = written
        .strip_prefix("~/")
        .or_else(|| written.strip_prefix("~\\").filter(|_| cfg!(windows)));
    match (rest, home.filter(|home| home.is_absolute())) {
        (Some(rest), Some(home)) => {
            let separators: &[char] = if cfg!(windows) { &['/', '\\'] } else { &['/'] };
            let mut path = home.to_path_buf();
            path.extend(rest.split(separators).filter(|name| !name.is_empty()));
            path
        }
        _ => PathBuf::from(written),
    }
}

/// Which Zotero library to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ZoteroLibrary {
    /// The personal library ("My Library").
    User,
    /// A group library, by its numeric group ID.
    Group(u64),
}

/// Where the editor endpoint listens, and the token that opens it. The token
/// is a capability, so it never prints and never leaves in a backup.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorEndpoint {
    pub port: u16,
    pub token: String,
}

impl std::fmt::Debug for EditorEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditorEndpoint")
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

/// How much each result row shows (ADR 0020).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    Compact,
    Comfortable,
}

/// Picker content size in logical pixels and optional top-left position as
/// fractions of the display's usable width and height. Keys are native stable
/// display IDs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PickerWindow {
    pub width: u32,
    pub height: u32,
    pub custom_size: bool,
    #[serde(default)]
    pub position: Option<(f64, f64)>,
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
    pub fact_index: Option<u8>,
    /// How often the picker has taught what can be typed with the search.
    pub shorthand_hints: u8,
}

/// Settings owns these choices until the picker consumes the same event fields.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Accent {
    #[default]
    System,
    Ibis,
}

impl Appearance {
    pub fn is_system(&self) -> bool {
        *self == Self::System
    }
}

/// Settings retain unknown fields locally, without carrying them in backups.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(flatten)]
    pub portable: Portable,
    #[serde(flatten)]
    pub device: Device,
    #[serde(flatten)]
    pub unknown: serde_json::Map<String, Value>,
}

/// Choices that may travel to another computer in a backup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Portable {
    pub schema_version: u32,
    /// Quiet bird facts and milestones; absent means enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small_surprises: Option<bool>,
    /// Explicit global shortcut per system; none stays unwritten.
    #[serde(skip_serializing_if = "Shortcuts::is_empty")]
    pub shortcut: Shortcuts,
    /// Library to open at start.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library: Option<LibrarySetting>,
    /// The default output: an id from the engine's `outputs` list, written
    /// once chosen. An id this Ibis does not list means the engine's default;
    /// anything but text reads as no choice, so the rest of the file is kept.
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "text_or_none")]
    pub output: Option<String>,
    /// Your format in Markdown, LaTeX and Typst apps: a key output's id
    /// (decision 0042). `output` is your format everywhere else.
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "text_or_none")]
    pub key_output: Option<String>,
    /// Result row density. `None` means comfortable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub density: Option<Density>,
    /// Larger text in the picker. `None` means the normal size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub larger_text: Option<bool>,
    /// Whether Enter pastes into the app the picker was opened from. `None`
    /// means on; `false` only copies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paste: Option<bool>,
    /// The menu bar item; absent means shown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub menu_bar: Option<bool>,
    /// Whether the picker remembers what was cited in each app; absent means on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent_citations: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub appearance: Option<Appearance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent: Option<Accent>,
}

/// Choices and capabilities that belong to this computer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Device {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delights: Option<Delights>,
    /// Explicit writing contexts; bibliography locations stay on this computer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub writings: Vec<writings::Writing>,
    /// Normalized system/app identifiers mapped to writing names.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub writing_apps: BTreeMap<String, String>,
    /// The format chosen for each app, the most recently chosen last, at most
    /// [`MAX_APP_OUTPUTS`]. It stays on this computer (decision 0040).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub app_outputs: Vec<AppOutput>,
    /// Kinds whose first successful suggested paste has been explained.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggestion_reports: Vec<ibis_core::destination::FormatKind>,
    /// The Zotero profile read while Zotero is closed, by its safe name;
    /// `None` until a computer with several profiles needs a choice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zotero_profile: Option<String>,
    /// The editor endpoint; absent means off (decision 0008).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor_endpoint: Option<EditorEndpoint>,
    /// Keep the key-format row visible after a key citation, including copy-only use.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub key_formats_used: bool,
    /// Whether Ibis checks GitHub for updates (ADR 0019). `None` means the
    /// person has not been asked yet; nothing is checked until they say yes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_checks: Option<bool>,
    /// Frames explicitly dragged by the person, indexed by display.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub picker_windows: BTreeMap<String, PickerWindow>,
}

impl Settings {
    /// The format chosen for `app`, compared as the platform compares app ids.
    pub fn app_output(&self, app: &ibis_core::destination::AppId) -> Option<&str> {
        let key = app.normalized();
        self.device
            .app_outputs
            .iter()
            .find(|entry| entry.app.system == app.system && entry.app.id == key)
            .map(|entry| entry.output.as_str())
    }

    /// Remember `output` for `app` as the most recent choice.
    pub fn remember_app_output(&mut self, app: &ibis_core::destination::AppId, output: String) {
        let app = ibis_core::destination::AppId {
            system: app.system,
            id: app.normalized(),
        };
        self.device.app_outputs.retain(|entry| entry.app != app);
        self.device.app_outputs.push(AppOutput { app, output });
        let excess = self.device.app_outputs.len().saturating_sub(MAX_APP_OUTPUTS);
        self.device.app_outputs.drain(..excess);
    }

    /// Forget the normalized app identity, just as it was remembered.
    pub fn forget_app_output(&mut self, app: &ibis_core::destination::AppId) {
        let key = app.normalized();
        self.device
            .app_outputs
            .retain(|entry| entry.app.system != app.system || entry.app.normalized() != key);
    }
}

impl Default for Portable {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            appearance: None,
            accent: None,
            small_surprises: None,
            shortcut: Shortcuts::default(),
            library: None,
            output: None,
            key_output: None,
            density: None,
            larger_text: None,
            paste: None,
            menu_bar: None,
            recent_citations: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("settings.json changed while saving; try again")]
    Changed,
    #[error("settings are being saved by another Ibis; wait for it to finish, then try again")]
    Busy,
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
    pub(crate) settings: Settings,
    pub problem: Option<SettingsError>,
    /// Captured by the primary read, not inferred from the returned settings.
    pub primary_read: PrimaryRead,
    /// Recovery provenance from the same read, even if the backup equals defaults.
    pub from_backup: bool,
}

/// Interface preferences can be recovered without granting citation choices.
#[derive(Debug, Clone, Default)]
pub struct Interface {
    pub shortcut: Option<String>,
    pub update_checks: Option<bool>,
    pub output: Option<String>,
    pub key_output: Option<String>,
    pub appearance: Option<Appearance>,
    pub accent: Option<Accent>,
    pub density: Option<Density>,
    pub larger_text: Option<bool>,
    pub paste: Option<bool>,
    pub menu_bar: Option<bool>,
    pub recent_citations: Option<bool>,
    pub small_surprises: Option<bool>,
    pub delights: Option<Delights>,
    pub picker_windows: BTreeMap<String, PickerWindow>,
}
impl From<&Settings> for Interface {
    fn from(settings: &Settings) -> Self {
        let portable = &settings.portable;
        Self {
            shortcut: portable.shortcut.here().map(str::to_owned),
            update_checks: settings.device.update_checks,
            output: crate::format::usual(settings, None).id,
            key_output: portable.key_output.clone(),
            appearance: portable.appearance,
            accent: portable.accent,
            density: portable.density,
            larger_text: portable.larger_text,
            paste: portable.paste,
            menu_bar: portable.menu_bar,
            recent_citations: portable.recent_citations,
            small_surprises: portable.small_surprises,
            delights: settings.device.delights.clone(),
            picker_windows: settings.device.picker_windows.clone(),
        }
    }
}
impl Loaded {
    /// Only a primary read may supply settings to citation and native effects.
    pub fn cites(&self) -> Result<&Settings, &SettingsError> {
        self.problem.as_ref().map_or(Ok(&self.settings), Err)
    }

    /// Recovery supplies presentation only; it cannot be passed to format selection.
    /// ```compile_fail
    /// let loaded = ibis_engine::settings::load(std::path::Path::new("settings.json"));
    /// ibis_engine::format::choose(&loaded.interface().unwrap(), None);
    /// ```
    pub fn interface(&self) -> Result<Interface, &SettingsError> {
        if self.from_backup {
            Ok((&self.settings).into())
        } else {
            self.cites().map(Interface::from)
        }
    }
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

/// A stored choice read leniently: anything but text, as another version may
/// have written, is no choice at all.
fn text_or_none<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    Ok(match Value::deserialize(deserializer)? {
        Value::String(text) => Some(text),
        _ => None,
    })
}

pub(crate) fn parse(text: &str) -> Result<Settings, SettingsError> {
    parse_value(serde_json::from_str(text).map_err(|e| SettingsError::Invalid(e.to_string()))?)
}

/// Choices removed before 1.0. Each reads as its default, so a file that still
/// names one stays valid and keeps the library and shortcut beside it.
const REMOVED_CHOICES: [(&str, &str); 9] = [
    ("density", "spacious"),
    ("accent", "blue"),
    ("accent", "purple"),
    ("accent", "pink"),
    ("accent", "red"),
    ("accent", "orange"),
    ("accent", "yellow"),
    ("accent", "green"),
    ("accent", "graphite"),
];

fn parse_value(value: Value) -> Result<Settings, SettingsError> {
    let settings = parse_fields(value)?;
    validate_settings(&settings)?;
    Ok(settings)
}

fn parse_fields<T: serde::de::DeserializeOwned>(mut value: Value) -> Result<T, SettingsError> {
    let invalid = |e: serde_json::Error| SettingsError::Invalid(e.to_string());
    if let Value::Object(fields) = &mut value {
        for (key, choice) in REMOVED_CHOICES {
            if fields.get(key).and_then(Value::as_str) == Some(choice) {
                fields.remove(key);
            }
        }
    }
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
    let settings = T::deserialize(&value).map_err(|error| {
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
    })?;
    Ok(settings)
}

fn validate_settings(settings: &Settings) -> Result<(), SettingsError> {
    writings::validate(&settings.device.writings).map_err(|error| SettingsError::Invalid(error.to_string()))?;
    if settings.device.writing_apps.len() > 200
        || settings.device.writing_apps.iter().any(|(key, name)| {
            let Some((system, id)) = key.split_once(':') else {
                return true;
            };
            let system = match system {
                "macos" => System::Macos,
                "windows" => System::Windows,
                "linux" => System::Linux,
                _ => return true,
            };
            key.len() > 512
                || id.is_empty()
                || key.chars().any(char::is_control)
                || ibis_core::destination::AppId { system, id: id.into() }.normalized() != id
                || writings::find(settings, name).is_err()
        })
    {
        return Err(SettingsError::Invalid("invalid writing app choices".into()));
    }
    Ok(())
}

/// Load settings, falling back to the backup, then to defaults. Report a missing
/// primary when a backup is recovered, and other primary problems unconditionally.
/// With no primary, a refused backup reports its own error instead of being treated as absent.
pub fn load(path: &Path) -> Loaded {
    load_bytes(path, read_regular_bytes(path))
}

/// Load the observation already captured by the owner, without rereading a
/// primary that an editor may have changed since its fingerprint was taken.
fn load_bytes(path: &Path, primary: io::Result<Vec<u8>>) -> Loaded {
    let primary = primary
        .and_then(|bytes| String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)));
    let (primary_read, primary) = match primary {
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

/// Create an absent settings file for its editor without replacing any entry
/// another writer created meanwhile. Existing regular files remain editable,
/// including half-written JSON; this operation never repairs their contents.
fn ensure_file(path: &Path) -> Result<(), SettingsError> {
    match fs::symlink_metadata(path) {
        Ok(_) => return require_regular(path).map_err(Into::into),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    let loaded = load(path);
    if let Some(problem) = loaded.problem {
        return Err(problem);
    }
    let json =
        serde_json::to_vec_pretty(&loaded.settings).map_err(|error| SettingsError::Invalid(error.to_string()))?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(&json)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            owner::committed(Err(io::ErrorKind::NotFound), [json, b"\n".to_vec()].concat());
            sync_directory(path);
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => require_regular(path).map_err(Into::into),
        Err(error) => Err(error.into()),
    }
}

type Snapshot = Result<Vec<u8>, io::ErrorKind>;
fn snapshot(path: &Path) -> Snapshot {
    read_regular_bytes(path).map_err(|error| error.kind())
}

/// One process owns the writer section, including direct saves outside a queue.
/// Only another process can make the stable side-file lock refuse immediately.
/// The empty lock file stays in place: removing it would split the lock domain.
fn writer_lock(path: &Path) -> Result<WriterLock, SettingsError> {
    static WRITER: Mutex<()> = Mutex::new(());
    // A save started while this thread already holds the writer would wait on
    // itself forever: refuse it at once instead, and fail loudly in tests.
    if HOLDING.get() {
        debug_assert!(false, "a settings writer was started while this thread holds one");
        return Err(SettingsError::Busy);
    }
    #[cfg(test)]
    if WRITER.try_lock().is_err() {
        writer_attempted();
    }
    let local = WRITER.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let target = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let linked = fs::canonicalize(path)?;
            if is_side_file(path, &linked) {
                path.to_path_buf()
            } else {
                linked
            }
        }
        _ => path.to_path_buf(),
    };
    let lock = target.with_extension("json.lock");
    if let Some(parent) = lock.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    match fs::symlink_metadata(&lock) {
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "settings lock is not a regular file").into());
        }
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error.into()),
        _ => {}
    }
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let file = options.open(lock)?;
    let acquired = file.try_lock();
    #[cfg(test)]
    writer_attempted();
    acquired.map_err(|error| match error {
        fs::TryLockError::WouldBlock => SettingsError::Busy,
        fs::TryLockError::Error(error) => SettingsError::Unreadable(error),
    })?;
    HOLDING.set(true);
    Ok(WriterLock { file, _local: local })
}

thread_local! {
    static HOLDING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

struct WriterLock {
    file: fs::File,
    _local: MutexGuard<'static, ()>,
}

#[cfg(test)]
thread_local! {
    static WRITER_ATTEMPT: std::cell::RefCell<Option<std::sync::mpsc::Sender<()>>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
fn writer_attempted() {
    WRITER_ATTEMPT.with(|attempt| {
        if let Some(attempt) = attempt.take() {
            let _ = attempt.send(());
        }
    });
}

impl Drop for WriterLock {
    fn drop(&mut self) {
        HOLDING.set(false);
        // Closing this descriptor alone is insufficient: a concurrently
        // spawned child may share its open file description until exec.
        // End the transaction's lock explicitly before releasing the file.
        while self
            .file
            .unlock()
            .is_err_and(|error| error.kind() == io::ErrorKind::Interrupted)
        {}
    }
}

fn write_settings_checked(
    path: &Path,
    settings: &Settings,
    recovering: bool,
    expected: &Snapshot,
) -> Result<(), SettingsError> {
    validate_settings(settings)?;
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
    if recovering {
        preserve_invalid(path)?;
    }
    // A settings file linked into a synced folder stays linked: the file it
    // points to is replaced, never the link (#1561). A link to one of Ibis's
    // own side files (an interrupted write, the backup) is replaced instead.
    let target = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let linked = fs::canonicalize(path)?;
            if is_side_file(path, &linked) {
                path.to_path_buf()
            } else {
                linked
            }
        }
        _ => path.to_path_buf(),
    };
    let replacement = AtomicFile::new(&target, format!("{json}\n").as_bytes())?;
    if *expected != snapshot(path) {
        return Err(SettingsError::Changed);
    }
    if !recovering && path.exists() {
        AtomicFile::new(&backup, &read_regular_bytes(path)?)?.replace()?;
    }
    if *expected != snapshot(path) {
        return Err(SettingsError::Changed);
    }
    replacement.replace()?;
    owner::committed(expected.clone(), format!("{json}\n").into_bytes());
    Ok(())
}

/// Whether `linked` is a file Ibis keeps beside `path`, such as
/// `settings.json.tmp`, `settings.json.bak` or `settings.json.invalid-1`.
fn is_side_file(path: &Path, linked: &Path) -> bool {
    let same_folder = match (path.parent().map(fs::canonicalize), linked.parent()) {
        (Some(Ok(folder)), Some(linked_folder)) => folder == linked_folder,
        _ => false,
    };
    let (Some(file), Some(name)) = (
        path.file_name().and_then(|s| s.to_str()),
        linked.file_name().and_then(|s| s.to_str()),
    ) else {
        return false;
    };
    let numbered = |suffix: &str, prefix: &str| {
        suffix
            .strip_prefix(prefix)
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    };
    same_folder
        && name
            .strip_prefix(file)
            .and_then(|rest| rest.strip_prefix('.'))
            .is_some_and(|suffix| {
                matches!(suffix, "tmp" | "bak") || numbered(suffix, "tmp-") || numbered(suffix, "invalid-")
            })
}

/// Copies that a sync service made of the settings file when two computers
/// changed it at once, such as `settings 2.json` (iCloud Drive),
/// `settings-PC.json` (OneDrive), `settings (conflicted copy).json` (Dropbox),
/// `settings (1).json` (Google Drive) or `settings.sync-conflict-….json`
/// (Syncthing). Ibis never merges them; it names them so the person can choose.
/// Only names are returned, from the folder of the file and, for a linked file,
/// the folder it points to; at most [`MAX_CONFLICTS`], sorted.
pub fn sync_conflicts(path: &Path) -> Vec<String> {
    let (Some(stem), Some(folder)) = (path.file_stem().and_then(|s| s.to_str()), path.parent()) else {
        return Vec::new();
    };
    let mut folders = vec![folder.to_path_buf()];
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
        && let Some(linked) = fs::canonicalize(path)
            .ok()
            .and_then(|target| target.parent().map(Path::to_path_buf))
        && !folders.contains(&linked)
    {
        folders.push(linked);
    }
    let mut names: Vec<String> = folders
        .iter()
        .filter_map(|folder| fs::read_dir(folder).ok())
        .flat_map(|entries| entries.take(MAX_FOLDER_ENTRIES).flatten())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| is_sync_conflict(stem, name))
        .collect();
    names.sort();
    names.dedup();
    names.truncate(MAX_CONFLICTS);
    names
}

/// At most this many conflict copies are named.
pub const MAX_CONFLICTS: usize = 5;
/// A settings folder is small; a large synced folder is not read to its end.
const MAX_FOLDER_ENTRIES: usize = 4096;

fn is_sync_conflict(stem: &str, name: &str) -> bool {
    let Some(middle) = name.strip_prefix(stem).and_then(|rest| rest.strip_suffix(".json")) else {
        return false;
    };
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    middle.strip_prefix(' ').is_some_and(digits)
        || middle
            .strip_prefix(" (")
            .and_then(|rest| rest.strip_suffix(')'))
            .is_some_and(|inside| digits(inside) || inside.contains("conflicted copy"))
        || middle
            .strip_prefix('-')
            .is_some_and(|host| !host.is_empty() && !host.contains('.'))
        || middle.starts_with(".sync-conflict-")
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

/// Change settings on disk: read the file as it is now, apply `change` and
/// save the result. Edits made since loading are kept. Recovery always needs
/// an explicit restore, so an ordinary edit cannot promote older cited choices.
#[cfg(test)]
fn update(path: &Path, change: impl FnOnce(&mut Settings)) -> Result<Settings, SettingsError> {
    transaction(path, |settings| {
        change(settings);
        Ok::<_, SettingsError>(())
    })
    .map(|(settings, ())| settings)
}

/// One checked read/change/write transaction, shared by CLI and app commands.
/// Refused edits publish nothing, and concurrent Ibis processes cannot lose edits.
fn transaction<T, E: From<SettingsError>>(
    path: &Path,
    change: impl FnOnce(&mut Settings) -> Result<T, E>,
) -> Result<(Settings, T), E> {
    let _writer = writer_lock(path).map_err(E::from)?;
    let before = snapshot(path);
    let loaded = load(path);
    if let Some(problem) = loaded.problem {
        return Err(problem.into());
    }
    let current = loaded.settings;
    let mut changed = current.clone();
    let result = change(&mut changed)?;
    if before != snapshot(path) {
        return Err(SettingsError::Changed.into());
    }
    validate_settings(&changed).map_err(E::from)?;
    if changed != current {
        write_settings_checked(path, &changed, false, &before).map_err(E::from)?;
    }
    Ok((changed, result))
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

pub(crate) fn read_regular_bytes(path: &Path) -> io::Result<Vec<u8>> {
    require_regular(path)?;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "settings path is not a regular file",
        ));
    }
    if metadata.len() > MAX_SETTINGS_BYTES {
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

    #[test]
    #[cfg_attr(debug_assertions, should_panic(expected = "while this thread holds one"))]
    fn a_save_inside_a_settings_change_refuses_instead_of_waiting_forever() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("settings.json");
        let inner = path.clone();
        let nested = update(&path, move |_| {
            assert!(matches!(update(&inner, |_| {}), Err(SettingsError::Busy)));
        });
        assert!(nested.is_ok());
    }

    #[test]
    fn overlapping_in_process_saves_wait_and_the_last_save_survives_reopen() -> Result<(), Box<dyn std::error::Error>> {
        use std::sync::mpsc;
        use std::time::Duration;
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("settings.json");
        let (started, holding) = mpsc::channel();
        let (release, held) = mpsc::channel();
        let first_path = path.clone();
        let first = std::thread::spawn(move || {
            update(&first_path, |settings| {
                started.send(()).unwrap_or_else(|error| panic!("{error}"));
                held.recv_timeout(Duration::from_secs(5))
                    .unwrap_or_else(|error| panic!("{error}"));
                settings.portable.menu_bar = Some(false);
            })
        });
        holding.recv_timeout(Duration::from_secs(5))?;
        let (attempt, attempted) = mpsc::channel();
        let last_path = path.clone();
        let last_settings = Settings {
            portable: crate::settings::Portable {
                larger_text: Some(true),
                ..Default::default()
            },
            ..Default::default()
        };
        let expected = last_settings.clone();
        let last = std::thread::spawn(move || {
            WRITER_ATTEMPT.with(|slot| slot.replace(Some(attempt)));
            update(&last_path, |s| *s = last_settings.clone())
        });
        // The second save has reached the contended writer while the first
        // still holds it. No scheduling delay or sleep creates the overlap.
        let overlapping = attempted.recv_timeout(Duration::from_secs(5));
        let released = release.send(());
        drop(release);
        overlapping?;
        released?;
        first.join().map_err(|_| "first save panicked")??;
        last.join().map_err(|_| "last save panicked")??;
        let reopened = load(&path);
        assert!(reopened.problem.is_none(), "{:?}", reopened.problem);
        assert_eq!(reopened.settings, expected);
        assert_eq!(load(&backup_path(&path)).settings.portable.menu_bar, Some(false));
        Ok(())
    }

    #[test]
    fn a_child_inheriting_a_lock_descriptor_does_not_hold_up_the_next_save() {
        use std::io::BufRead;
        use std::process::{Child, Command, Stdio};
        const CHILD: &str = "IBIS_SETTINGS_INHERITED_LOCK_CHILD";
        if std::env::var_os(CHILD).is_some() {
            println!("ibis-lock-child-ready");
            std::io::stdout().flush().unwrap_or_else(|error| panic!("{error}"));
            loop {
                std::thread::park();
            }
        }
        struct Running(Child);
        impl Drop for Running {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("settings.json");
        let locked = writer_lock(&path).unwrap_or_else(|error| panic!("{error}"));
        let mut child = Running(
            Command::new(std::env::current_exe().unwrap_or_else(|error| panic!("{error}")))
                .args([
                    "--exact",
                    "settings::tests::a_child_inheriting_a_lock_descriptor_does_not_hold_up_the_next_save",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .stdin(Stdio::from(
                    locked.file.try_clone().unwrap_or_else(|error| panic!("{error}")),
                ))
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap_or_else(|error| panic!("{error}")),
        );
        let output = child.0.stdout.take().unwrap_or_else(|| panic!("child stdout"));
        let (sent, ready) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(output).lines().map_while(Result::ok) {
                if line == "ibis-lock-child-ready" {
                    let _ = sent.send(());
                    break;
                }
            }
        });
        ready
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap_or_else(|error| panic!("{error}"));
        drop(locked);
        let saved = update(&path, |settings| settings.portable.larger_text = Some(true))
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(saved.portable.larger_text, Some(true));
        assert_eq!(load(&path).settings.portable.larger_text, Some(true));
    }

    #[test]
    fn opening_the_settings_file_creates_defaults_once_and_keeps_existing_edits() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("settings.json");
        ensure_file(&path).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(load(&path).settings, Settings::default());
        std::fs::write(&path, b"{ still editing").unwrap_or_else(|error| panic!("{error}"));
        ensure_file(&path).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(std::fs::read(&path).unwrap_or_default(), b"{ still editing");
        std::fs::remove_file(&path).unwrap_or_else(|error| panic!("{error}"));
        std::fs::write(backup_path(&path), br#"{"larger_text":true}"#).unwrap_or_else(|error| panic!("{error}"));
        assert!(ensure_file(&path).is_err());
        assert!(!path.exists(), "opening the editor cannot promote a recovery copy");
        assert_eq!(load(&path).settings.portable.larger_text, Some(true));
    }

    #[test]
    fn invalid_appearance_or_accent_refuses_every_writer_and_keeps_the_file() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("settings.json");
        for contents in [br#"{"appearance":"invented"}"#.as_slice(), br#"{"accent":42}"#] {
            fs::write(&path, contents).unwrap_or_else(|error| panic!("{error}"));
            assert!(load(&path).problem.is_some());
            assert!(update(&path, |settings| settings.portable.menu_bar = Some(false)).is_err());
            assert!(update(&path, |_| {}).is_err());
            assert_eq!(fs::read(&path).unwrap_or_default(), contents);
        }
    }

    #[test]
    fn each_app_keeps_its_latest_format_and_the_oldest_choice_goes_first() {
        use ibis_core::destination::{AppId, System};
        let word = AppId {
            system: System::Macos,
            id: "com.microsoft.Word".into(),
        };
        let mut settings = Settings::default();
        settings.remember_app_output(&word, "apa".into());
        settings.remember_app_output(&word, "typst".into());
        // Compared as macOS compares bundle ids, and saved in that form.
        assert_eq!(
            settings.app_output(&AppId {
                system: System::Macos,
                id: "COM.MICROSOFT.WORD".into()
            }),
            Some("typst")
        );
        assert_eq!(settings.device.app_outputs.len(), 1);
        let other = |n: usize| AppId {
            system: System::Macos,
            id: format!("org.example.app{n}"),
        };
        for n in 0..MAX_APP_OUTPUTS {
            settings.remember_app_output(&other(n), "pandoc".into());
        }
        assert_eq!(settings.device.app_outputs.len(), MAX_APP_OUTPUTS);
        assert_eq!(
            settings.app_output(&word),
            None,
            "the least recently chosen app went first"
        );
        assert_eq!(settings.app_output(&other(0)), Some("pandoc"));
        // Only the system, the app id and the output are written.
        let saved = serde_json::to_value(&settings).unwrap_or_default();
        assert_eq!(
            saved["app_outputs"][0],
            serde_json::json!({ "app": { "system": "macos", "id": "org.example.app0" }, "output": "pandoc" })
        );
    }

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
        assert_eq!(
            load(&path).settings.portable.paste,
            Some(false),
            "the limit is inclusive"
        );
        text.push(' ');
        fs::write(&path, &text).unwrap_or_else(|e| panic!("{e}"));
        for _ in 0..2 {
            let loaded = load(&path);
            assert_eq!(loaded.primary_read, PrimaryRead::Unreadable);
            assert!(
                matches!(loaded.problem, Some(SettingsError::Unreadable(e)) if e.kind() == io::ErrorKind::FileTooLarge)
            );
            assert!(update(&path, |s| s.portable.paste = Some(true)).is_err());
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
        assert_eq!(load(&path).settings.portable.paste, Some(false));
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
                if recovering && load(&path).problem.is_some() {
                    restore_backup(&path).map(|_| ()).map_err(|error| error.to_string())
                } else {
                    update(&path, |s| s.portable.larger_text = Some(larger))
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                }
                .unwrap_or_else(|e| panic!("{e}"));
                assert_eq!(
                    fs::metadata(&path)
                        .unwrap_or_else(|e| panic!("{e}"))
                        .permissions()
                        .mode()
                        & 0o777,
                    0o640
                );
                assert_eq!(
                    load(&path).settings.portable.paste,
                    Some(false),
                    "reopening keeps copy only"
                );
            }
        }
    }

    fn restore_backup(path: &Path) -> Result<Settings, backup::BackupError> {
        let (_, plan) = crate::settings_backup::prepare(path.to_path_buf(), backup_path(path))?;
        Owner::open(path.to_path_buf()).restore(&plan)
    }

    #[test]
    fn primary_provenance_orders_interface_recovery_and_refuses_citation_edits() {
        for (primary, provenance, valid, shortcut) in [
            (None, PrimaryRead::Missing, false, None),
            (Some(b"{}".as_slice()), PrimaryRead::Present, true, None),
            (Some(br#"{"schema_version":2}"#), PrimaryRead::Present, true, None),
            (Some(br#"{"shortcut":null}"#), PrimaryRead::Present, true, None),
            (
                Some(br#"{"shortcut":"Control+Command+R"}"#),
                PrimaryRead::Present,
                true,
                Some("Control+Command+R"),
            ),
            (
                Some(br#"{"shortcut":"Control+Nonsense"}"#),
                PrimaryRead::Present,
                true,
                Some("Control+Nonsense"),
            ),
            (Some(br#"{"shortcut":""}"#), PrimaryRead::Present, true, Some("")),
            (Some(b"{ invalid"), PrimaryRead::Present, false, None),
            (Some(br#"{"schema_version":999}"#), PrimaryRead::Present, false, None),
            (Some(b"\xff"), PrimaryRead::Unreadable, false, None),
            (Some(b"directory"), PrimaryRead::Unreadable, false, None),
        ] {
            for (backup, recovered) in [
                (None, false),
                (
                    Some(br#"{"shortcut":"Alt+Space","density":"compact"}"#.as_slice()),
                    true,
                ),
                (Some(b"{}"), true),
                (Some(b"{"), false),
                (Some(b"\xff"), false),
                (Some(br#"{"schema_version":1}"#), false),
                (Some(br#"{"schema_version":999}"#), false),
            ] {
                let directory = dir("provenance");
                let path = directory.path().join("settings.json");
                match primary {
                    Some(b"directory") => fs::create_dir(&path),
                    Some(bytes) => fs::write(&path, bytes),
                    None => Ok(()),
                }
                .unwrap_or_else(|error| panic!("{error}"));
                if let Some(bytes) = backup {
                    fs::write(backup_path(&path), bytes).unwrap_or_else(|error| panic!("{error}"));
                }
                let before = (snapshot(&path), snapshot(&backup_path(&path)));
                let loaded = load(&path);
                let usable = valid || primary.is_none() && backup.is_none();
                let recovery = !valid && recovered;
                assert_eq!(loaded.primary_read, provenance);
                assert_eq!(loaded.problem.is_none(), usable);
                assert_eq!(loaded.from_backup, recovery);
                assert_eq!(loaded.cites().is_ok(), usable);
                assert_eq!(loaded.interface().is_ok(), usable || recovery);
                let compact = recovery && backup.is_some_and(|bytes| bytes != b"{}");
                if let Ok(interface) = loaded.interface() {
                    assert_eq!(interface.density, compact.then_some(Density::Compact));
                    assert_eq!(
                        loaded.settings.portable.shortcut.here(),
                        if compact { Some("Alt+Space") } else { shortcut }
                    );
                }
                if primary.is_none() && backup.is_some() && !recovered {
                    assert!(matches!(&loaded.problem, Some(SettingsError::Backup(_))));
                    assert!(
                        loaded
                            .problem
                            .as_ref()
                            .is_some_and(|error| error.to_string().contains("settings.json.bak"))
                    );
                }
                let owner = Owner::open(path.clone());
                for _ in 0..2 {
                    let mut applied = false;
                    assert_eq!(
                        owner
                            .change(|_| {
                                applied = true;
                                Ok::<_, SettingsError>(())
                            })
                            .is_ok(),
                        usable
                    );
                    assert_eq!(applied, usable);
                }
                if !usable && primary.is_none() {
                    assert!(owner.ensure_file().is_err());
                }
                assert_eq!((snapshot(&path), snapshot(&backup_path(&path))), before);
                assert!(!path.with_extension("json.invalid-1").exists());
                assert!(!path.with_extension("json.tmp").exists());
            }
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
        let saved = restore_backup(&path).unwrap_or_else(|e| panic!("{e}"));
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
        assert!(update(&path, |s| s.portable.output = Some("keys".into())).is_err());
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
            assert_eq!(restore_backup(&path).is_err(), exhausted);
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
    fn missing_primary_recovery_refuses_a_blocked_temporary_without_touching_the_backup() {
        let directory = dir("missing-blocked-save");
        let path = directory.path().join("settings.json");
        let backup = r#"{"shortcut":"Alt+Space"}"#;
        fs::write(backup_path(&path), backup).unwrap_or_else(|e| panic!("{e}"));
        fs::create_dir(path.with_extension("json.tmp")).unwrap_or_else(|e| panic!("{e}"));
        assert!(restore_backup(&path).is_err());
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
    fn picker_preferences_round_trip_and_stay_optional() {
        for text in ["{}", r#"{"larger_text":null}"#] {
            assert_eq!(
                parse(text).map(|settings| settings.portable.larger_text).ok(),
                Some(None)
            );
        }
        let saved = serde_json::to_string(&Settings::default()).unwrap_or_default();
        for field in ["output", "density", "larger_text", "paste"] {
            assert!(!saved.contains(field), "{field} is written only once chosen");
        }
        let settings = parse(
            r#"{"schema_version": 2, "output": "chicago-author-date", "density": "compact", "larger_text": true, "paste": false, "shortcut":"Control+Command+R", "update_checks":false}"#,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(settings.portable.paste, Some(false));
        assert_eq!(settings.portable.output.as_deref(), Some("chicago-author-date"));
        assert_eq!(settings.portable.density, Some(Density::Compact));
        assert_eq!(settings.portable.larger_text, Some(true));
        let directory = dir("preferences");
        let path = directory.path().join("nested/settings.json");
        update(&path, |s| *s = settings.clone()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(load(&path).settings, settings);
    }

    #[test]
    fn a_removed_choice_keeps_the_file_loadable_and_the_rest_of_its_settings() {
        let settings =
            parse(r#"{"density": "spacious", "accent": "purple", "larger_text": true, "search_icon": "none"}"#)
                .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(settings.portable.density, None);
        assert!(!settings.unknown.contains_key("accent"));
        assert_eq!(settings.portable.larger_text, Some(true));
    }

    #[test]
    fn unknown_picker_preference_values_are_refused_or_read_as_no_choice() {
        for text in [r#"{"density": "tiny"}"#, r#"{"larger_text": "yes"}"#] {
            assert!(matches!(parse(text), Err(SettingsError::Invalid(_))), "{text}");
        }
        // An output id is the engine's to read: one this Ibis does not list
        // keeps the file valid and means the default where it is used.
        let later = parse(r#"{"output": "a-later-output"}"#).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(later.portable.output.as_deref(), Some("a-later-output"));
        // An output in another shape is no choice, and the settings beside it stay.
        for shape in [r#"7"#, r#"{"key": "typst"}"#, "null"] {
            let text = format!(r#"{{"output": {shape}, "larger_text": true}}"#);
            let kept = parse(&text).unwrap_or_else(|e| panic!("{shape}: {e}"));
            assert_eq!(kept.portable.output, None, "{shape}");
            assert_eq!(kept.portable.larger_text, Some(true), "{shape}");
        }
    }

    #[test]
    fn update_checks_stay_unasked_until_the_person_answers() {
        assert_eq!(Settings::default().device.update_checks, None);
        assert_eq!(parse("{}").map(|s| s.device.update_checks).ok(), Some(None));
        let saved = serde_json::to_string(&Settings::default()).unwrap_or_default();
        assert!(!saved.contains("update_checks"));
        assert_eq!(
            parse(r#"{"schema_version": 2, "update_checks": true}"#)
                .map(|s| s.device.update_checks)
                .ok(),
            Some(Some(true))
        );
        assert!(parse(r#"{"schema_version": 2, "update_checks": "yes"}"#).is_err());
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
                br#"{"density":"tiny"}"#,
                br#"{"library":{"zotero":["user",{"group":-1}]}}"#,
                b"\xff",
            ];
            for (index, bad) in damaged.into_iter().enumerate() {
                fs::write(&path, bad).unwrap_or_else(|e| panic!("{e}"));
                let loaded = load(&path);
                assert!(loaded.from_backup);
                // Explicit Restore repairs the primary even when values match the backup.
                let saved = restore_backup(&path).unwrap_or_else(|e| panic!("recovery save failed: {e}"));
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

        let saved = restore_backup(&path).unwrap_or_else(|e| panic!("{e}"));
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
            assert!(restore_backup(&path).is_err());
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
        restore_backup(&path).unwrap_or_else(|e| panic!("{e}"));
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
        restore_backup(&path).unwrap_or_else(|e| panic!("{e}"));
        let mode = fs::metadata(path.with_extension("json.invalid-1"))
            .unwrap_or_else(|e| panic!("{e}"))
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0);
    }

    #[test]
    fn newer_files_are_refused_even_when_their_fields_changed_type() {
        let directory = dir("newer-types");
        let path = directory.path().join("settings.json");
        fs::create_dir_all(path.parent().unwrap_or_else(|| panic!("no parent"))).unwrap_or_else(|e| panic!("{e}"));
        let newer = r#"{"schema_version": 99, "shortcut": {"key": "Space"}}"#;
        fs::write(&path, newer).unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(load(&path).problem, Some(SettingsError::NewerVersion(99))));
        assert!(matches!(update(&path, |_| {}), Err(SettingsError::NewerVersion(99))));
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
        update(&path, |s| s.portable.output = Some("typst".into())).unwrap_or_else(|e| panic!("{e}"));
        // The user edits the file while the app runs.
        let edited = r#"{"schema_version": 2, "shortcut": "Alt+Space", "output": "typst"}"#;
        fs::write(&path, edited).unwrap_or_else(|e| panic!("{e}"));
        let after = update(&path, |s| s.portable.output = Some("keys".into())).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(after.portable.shortcut.here(), Some("Alt+Space"));
        assert_eq!(load(&path).settings, after);
        fs::remove_file(backup_path(&path)).unwrap_or_else(|e| panic!("{e}"));
        fs::write(&path, "{ not json").unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(
            update(&path, |s| s.portable.output = Some("pandoc".into())),
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
            "output": "apa",
            "window_position": { "display": "synthetic", "x": 10, "y": -20 },
            "later_flag": [true, null, 1.5],
        });
        fs::write(&path, newer_build.to_string()).unwrap_or_else(|e| panic!("{e}"));
        update(&path, |s| s.portable.shortcut.set_here(Some("Alt+Space".into()))).unwrap_or_else(|e| panic!("{e}"));
        let saved: Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap_or_default()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            saved["shortcut"][serde_json::to_value(System::HERE)
                .unwrap_or_default()
                .as_str()
                .unwrap_or_default()],
            "Alt+Space"
        );
        assert_eq!(saved["output"], "apa");
        assert_eq!(saved["window_position"], newer_build["window_position"]);
        assert_eq!(saved["later_flag"], newer_build["later_flag"]);
    }

    #[test]
    fn a_temporary_file_left_by_an_interrupted_save_does_not_leak_into_the_next() {
        let directory = dir("interrupted");
        let path = directory.path().join("settings.json");
        let stale = format!("{{\"schema_version\": 2, \"shortcut\": \"{}\"}}", "x".repeat(4096));
        fs::write(path.with_extension("json.tmp"), &stale).unwrap_or_else(|e| panic!("{e}"));
        update(&path, |_| {}).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(load(&path).settings, Settings::default());
        assert!(load(&path).problem.is_none());
        assert_eq!(
            fs::read_to_string(path.with_extension("json.tmp")).unwrap_or_default(),
            stale
        );
    }

    #[test]
    fn old_formats_need_explicit_restore_without_migration() {
        for old in [
            r#"{"schema_version": 1, "library": "/synthetic/refs.bib", "output": "typst"}"#,
            r#"{"library": "/synthetic/refs.bib", "output": "typst"}"#,
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
            assert_eq!(loaded.settings.portable.shortcut.here(), Some("Alt+Space"));
            assert_eq!(loaded.settings.portable.output, None);
            assert_eq!(loaded.settings.portable.library, None);
            assert_eq!(fs::read_to_string(&path).unwrap_or_default(), old);

            assert!(update(&path, |_| {}).is_err());
            let saved = restore_backup(&path).unwrap_or_else(|e| panic!("{e}"));
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
            assert_eq!(
                parse(&text).unwrap_or_else(|e| panic!("{e}")).portable.library,
                Some(setting)
            );
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
    fn an_invalid_zotero_member_requires_explicit_restore_and_keeps_the_bad_bytes() {
        let directory = dir("zotero-member");
        let path = directory.path().join("settings.json");
        let good = Settings {
            portable: crate::settings::Portable {
                library: Some(LibrarySetting::Zotero(ZoteroLibrary::User.into())),
                ..Default::default()
            },
            ..Default::default()
        };
        update(&path, |s| *s = good.clone()).unwrap_or_else(|e| panic!("{e}"));
        update(&path, |s| s.portable.larger_text = Some(true)).unwrap_or_else(|e| panic!("{e}"));
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
        assert!(update(&path, |s| s.portable.output = Some("keys".into())).is_err());
        let saved = restore_backup(&path).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(saved.portable.library, good.portable.library);
        assert_eq!(saved.portable.output, None);
        assert_eq!(load(&path).settings, saved);
        assert!(load(&path).problem.is_none());
        assert_eq!(
            fs::read_to_string(path.with_extension("json.invalid-1")).unwrap_or_default(),
            bad
        );
        assert_eq!(fs::read(backup_path(&path)).unwrap_or_default(), backup);
    }
}
