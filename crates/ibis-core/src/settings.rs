//! User settings, stored as versioned JSON.
//!
//! Settings are small and human-editable. Saving is atomic and keeps the
//! previous file as a backup, so a crash or a bad edit never loses the last
//! working settings. A file written by a newer version of Ibis is refused
//! rather than overwritten.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::cite::KeyFormat;

/// Version of the settings format this build reads and writes.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    /// Global shortcut, e.g. `"Control+Command+R"`. `None` uses the default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
    /// BibTeX/BibLaTeX file to open at start.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library: Option<PathBuf>,
    /// Syntax the picker copies citations in; remembered from the last copy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copy_format: Option<KeyFormat>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            schema_version: SCHEMA_VERSION,
            shortcut: None,
            library: None,
            copy_format: None,
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
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

fn parse(text: &str) -> Result<Settings, SettingsError> {
    let invalid = |e: serde_json::Error| SettingsError::Invalid(e.to_string());
    let value: serde_json::Value = serde_json::from_str(text).map_err(invalid)?;
    // The version is checked before any other field: a newer format may
    // change field types, and must still be recognised as newer.
    if let Some(version) = value.get("schema_version").and_then(serde_json::Value::as_u64)
        && version > u64::from(SCHEMA_VERSION)
    {
        return Err(SettingsError::NewerVersion(u32::try_from(version).unwrap_or(u32::MAX)));
    }
    serde_json::from_value(value).map_err(invalid)
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
            };
        }
        Err(e) => Err(SettingsError::Unreadable(e)),
    };
    match primary {
        Ok(settings) => Loaded {
            settings,
            problem: None,
        },
        Err(problem) => {
            let settings = fs::read_to_string(backup_path(path))
                .ok()
                .and_then(|text| parse(&text).ok())
                .unwrap_or_default();
            Loaded {
                settings,
                problem: Some(problem),
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
            ..Settings::default()
        };
        save(&path, &settings).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(load(&path).settings, settings);
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
}
