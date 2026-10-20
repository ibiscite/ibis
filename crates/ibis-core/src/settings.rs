//! User settings, stored as versioned JSON.
//!
//! Settings are small and human-editable. Saving is atomic and keeps the
//! previous file as a backup, so a crash or a bad edit never loses the last
//! working settings. A file written by a newer version of Ibis is refused
//! rather than overwritten; a file written by an older version is migrated
//! when it is read, one version step at a time (ADR 0018).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::cite::KeyFormat;

/// Version of the settings format this build reads and writes.
pub const SCHEMA_VERSION: u32 = 2;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    /// Global shortcut, e.g. `"Control+Command+R"`. `None` uses the default.
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
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            schema_version: SCHEMA_VERSION,
            shortcut: None,
            library: None,
            copy_format: None,
            update_checks: None,
            citation_style: None,
            density: None,
            larger_text: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("the settings file could not be read: {0}")]
    Unreadable(#[from] io::Error),
    #[error("the settings file is not valid: {0}")]
    Invalid(String),
    #[error("the settings file was written by a newer version of Ibis (format {0}); it was left unchanged")]
    NewerVersion(u32),
}

/// Result of loading: the settings to use, and a problem to show the user, if any.
#[derive(Debug)]
pub struct Loaded {
    pub settings: Settings,
    pub problem: Option<SettingsError>,
    /// Recovery provenance from the same read, even if the backup equals defaults.
    pub from_backup: bool,
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

fn parse(text: &str) -> Result<Settings, SettingsError> {
    let invalid = |e: serde_json::Error| SettingsError::Invalid(e.to_string());
    let mut value: Value = serde_json::from_str(text).map_err(invalid)?;
    // The version is checked before any other field: a newer format may
    // change field types, and must still be recognised as newer.
    let version = match value.get("schema_version") {
        None | Some(Value::Null) => 1,
        Some(v) => v
            .as_u64()
            .ok_or_else(|| SettingsError::Invalid("schema_version must be a whole number".into()))?,
    };
    if version > u64::from(SCHEMA_VERSION) {
        return Err(SettingsError::NewerVersion(u32::try_from(version).unwrap_or(u32::MAX)));
    }
    if version < 2 {
        value = v1_to_v2(value);
    }
    serde_json::from_value(value).map_err(invalid)
}

/// Version 1 named the library by a bare path, always a BibTeX file.
/// Version 2 says which kind of library it is. Other fields are unchanged.
fn v1_to_v2(mut value: Value) -> Value {
    if let Some(object) = value.as_object_mut() {
        if let Some(path) = object.get("library").filter(|l| l.is_string()).cloned() {
            object.insert("library".into(), serde_json::json!({ "bibtex": path }));
        }
        object.insert("schema_version".into(), Value::from(2));
    }
    value
}

/// Load settings. A missing file gives defaults. An unreadable or invalid file
/// falls back to the backup, then to defaults, and reports the problem.
pub fn load(path: &Path) -> Loaded {
    let primary = match fs::read_to_string(path) {
        Ok(text) => parse(&text),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Loaded {
                settings: Settings::default(),
                problem: None,
                from_backup: false,
            };
        }
        Err(e) => Err(SettingsError::Unreadable(e)),
    };
    match primary {
        Ok(settings) => Loaded {
            settings,
            problem: None,
            from_backup: false,
        },
        Err(problem) => {
            let backup = fs::read_to_string(backup_path(path))
                .ok()
                .and_then(|text| parse(&text).ok());
            let from_backup = backup.is_some();
            Loaded {
                settings: backup.unwrap_or_default(),
                problem: Some(problem),
                from_backup,
            }
        }
    }
}

/// Save settings atomically, keeping the previous file as a backup. Refuses to
/// replace a file written by a newer version.
pub fn save(path: &Path, settings: &Settings) -> Result<(), SettingsError> {
    if let Ok(text) = fs::read_to_string(path)
        && let Err(SettingsError::NewerVersion(v)) = parse(&text)
    {
        return Err(SettingsError::NewerVersion(v));
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| SettingsError::Invalid(e.to_string()))?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, format!("{json}\n"))?;
    if path.exists() {
        fs::copy(path, backup_path(path))?;
    }
    fs::rename(&temporary, path)?;
    Ok(())
}

/// Change settings on disk: read the file as it is now, apply `change` and
/// save the result. Edits made to the file since it was loaded are kept. A file
/// that cannot be read or is invalid is left untouched and the problem returned.
pub fn update(path: &Path, change: impl FnOnce(&mut Settings)) -> Result<Settings, SettingsError> {
    let current = match fs::read_to_string(path) {
        Ok(text) => parse(&text)?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => Settings::default(),
        Err(e) => return Err(SettingsError::Unreadable(e)),
    };
    let mut changed = current.clone();
    change(&mut changed);
    if changed != current {
        save(path, &changed)?;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ibis-settings-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn missing_file_gives_defaults_without_a_problem() {
        let loaded = load(&dir("missing").join("settings.json"));
        assert_eq!(loaded.settings, Settings::default());
        assert!(loaded.problem.is_none());
    }

    #[test]
    fn save_and_load_round_trip() {
        let path = dir("roundtrip").join("settings.json");
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
        let saved = serde_json::to_string(&Settings::default()).unwrap_or_default();
        for field in ["citation_style", "density", "larger_text"] {
            assert!(!saved.contains(field), "{field} is written only once chosen");
        }
        let settings = parse(
            r#"{"schema_version": 2, "citation_style": "chicago-author-date", "density": "compact", "larger_text": true}"#,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(settings.citation_style, Some(CitationStyle::ChicagoAuthorDate));
        assert_eq!(settings.density, Some(Density::Compact));
        assert_eq!(settings.larger_text, Some(true));
        let path = dir("preferences").join("settings.json");
        save(&path, &settings).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(load(&path).settings, settings);
    }

    #[test]
    fn unknown_style_or_density_is_invalid_not_guessed() {
        for text in [
            r#"{"citation_style": "vancouver"}"#,
            r#"{"density": "tiny"}"#,
            r#"{"larger_text": "yes"}"#,
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
        let path = dir("corrupt").join("settings.json");
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
    fn settings_recovery_reports_backup_or_defaults_from_the_same_read() {
        let path = dir("recovery-source").join("settings.json");
        fs::create_dir_all(
            path.parent()
                .unwrap_or_else(|| panic!("expected a synthetic settings directory")),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        for primary in ["{ invalid", r#"{"schema_version":3}"#] {
            fs::write(&path, primary).unwrap_or_else(|e| panic!("{e}"));
            let fallback = load(&path);
            assert!(fallback.problem.is_some());
            assert!(!fallback.from_backup);
            fs::write(backup_path(&path), "{}").unwrap_or_else(|e| panic!("{e}"));
            let recovered = load(&path);
            assert!(recovered.problem.is_some());
            assert!(recovered.from_backup);
            assert_eq!(recovered.settings, Settings::default());
            fs::remove_file(backup_path(&path)).unwrap_or_else(|e| panic!("{e}"));
        }
    }

    #[test]
    fn a_newer_settings_file_is_never_overwritten() {
        let path = dir("newer").join("settings.json");
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
        let path = dir("newer-types").join("settings.json");
        fs::create_dir_all(path.parent().unwrap_or_else(|| panic!("no parent"))).unwrap_or_else(|e| panic!("{e}"));
        let newer = r#"{"schema_version": 99, "shortcut": {"key": "Space"}}"#;
        fs::write(&path, newer).unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(load(&path).problem, Some(SettingsError::NewerVersion(99))));
        assert!(matches!(
            save(&path, &Settings::default()),
            Err(SettingsError::NewerVersion(99))
        ));
        assert_eq!(fs::read_to_string(&path).unwrap_or_default(), newer);
    }

    #[test]
    fn updates_keep_edits_made_since_loading_and_never_replace_invalid_files() {
        let path = dir("update").join("settings.json");
        update(&path, |s| s.copy_format = Some(KeyFormat::Typst)).unwrap_or_else(|e| panic!("{e}"));
        // The user edits the file while the app runs.
        let edited = r#"{"schema_version": 1, "shortcut": "Alt+Space", "copy_format": "typst"}"#;
        fs::write(&path, edited).unwrap_or_else(|e| panic!("{e}"));
        let after = update(&path, |s| s.copy_format = Some(KeyFormat::Keys)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(after.shortcut.as_deref(), Some("Alt+Space"));
        assert_eq!(load(&path).settings, after);
        fs::write(&path, "{ not json").unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(
            update(&path, |s| s.copy_format = Some(KeyFormat::Pandoc)),
            Err(SettingsError::Invalid(_))
        ));
        assert_eq!(fs::read_to_string(&path).unwrap_or_default(), "{ not json");
    }

    #[test]
    fn version_1_files_are_migrated_when_read() {
        let v1 = serde_json::json!({ "schema_version": 1, "library": "/synthetic/refs.bib", "shortcut": "Alt+Y" });
        assert_eq!(
            v1_to_v2(v1),
            serde_json::json!({ "schema_version": 2, "library": { "bibtex": "/synthetic/refs.bib" }, "shortcut": "Alt+Y" })
        );
        // No library, no version, and already-migrated values pass through.
        assert_eq!(
            v1_to_v2(serde_json::json!({})),
            serde_json::json!({ "schema_version": 2 })
        );
        let migrated = serde_json::json!({ "schema_version": 2, "library": { "bibtex": "/x.bib" } });
        assert_eq!(v1_to_v2(migrated.clone()), migrated);

        for text in [
            r#"{"schema_version": 1, "library": "/synthetic/refs.bib", "copy_format": "typst"}"#,
            r#"{"library": "/synthetic/refs.bib", "copy_format": "typst"}"#,
            r#"{"schema_version": 0, "library": "/synthetic/refs.bib", "copy_format": "typst"}"#,
        ] {
            let settings = parse(text).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(settings.schema_version, SCHEMA_VERSION);
            assert_eq!(
                settings.library,
                Some(LibrarySetting::Bibtex(PathBuf::from("/synthetic/refs.bib")))
            );
            assert_eq!(settings.copy_format, Some(KeyFormat::Typst));
        }
        assert!(matches!(
            parse(r#"{"schema_version": "1"}"#),
            Err(SettingsError::Invalid(_))
        ));
    }

    #[test]
    fn a_migrated_file_is_written_in_the_new_format_and_the_old_one_is_kept_as_backup() {
        let path = dir("migrate").join("settings.json");
        fs::create_dir_all(path.parent().unwrap_or_else(|| panic!("no parent"))).unwrap_or_else(|e| panic!("{e}"));
        let v1 = r#"{"schema_version": 1, "library": "/synthetic/refs.bib"}"#;
        fs::write(&path, v1).unwrap_or_else(|e| panic!("{e}"));
        assert!(load(&path).problem.is_none());
        update(&path, |s| s.copy_format = Some(KeyFormat::Keys)).unwrap_or_else(|e| panic!("{e}"));
        let written: Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap_or_default()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(written["schema_version"], 2);
        assert_eq!(
            written["library"],
            serde_json::json!({ "bibtex": "/synthetic/refs.bib" })
        );
        assert_eq!(fs::read_to_string(backup_path(&path)).unwrap_or_default(), v1);
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
            assert!(matches!(parse(&text), Err(SettingsError::Invalid(_))), "{bad}");
        }
    }
}
