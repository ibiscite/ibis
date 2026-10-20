//! Saving choices made in the picker to the settings file.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ibis_core::cite::KeyFormat;
use ibis_core::settings::{self, CitationStyle, Density};
use serde::{Deserialize, Serialize};
use tauri::State;

/// Settings errors can contain user-entered values or OS details. Only these
/// fixed descriptions reach diagnostics or messages about failed saves.
pub(crate) fn settings_problem(problem: &settings::SettingsError) -> &'static str {
    match problem {
        settings::SettingsError::Unreadable(_) => "settings.json could not be read",
        settings::SettingsError::Invalid(_) => "settings.json is not valid",
        settings::SettingsError::NewerVersion(_) => "settings.json was written by a newer version of Ibis",
    }
}

/// This notice is for the interface only: its settings path must not be logged.
pub(crate) fn settings_notice(path: &Path, loaded: &settings::Loaded) -> Option<String> {
    loaded.problem.as_ref().map(|problem| {
        let recovery = if loaded.from_backup { "the backup" } else { "defaults" };
        format!(
            "{}; using {recovery}. Changes are not being saved while this problem remains. Settings file: {}",
            settings_problem(problem),
            path.display()
        )
    })
}

/// Where the settings file is, for saving choices made in the picker.
pub(crate) struct SettingsFile {
    pub(crate) path: PathBuf,
    /// Held for each whole read, change and save, so two saves never interleave.
    writing: Mutex<()>,
}

impl SettingsFile {
    pub(crate) fn new(path: PathBuf) -> Self {
        SettingsFile {
            path,
            writing: Mutex::new(()),
        }
    }

    pub(crate) fn update(&self, change: impl FnOnce(&mut settings::Settings)) -> Result<settings::Settings, String> {
        let _writing = self
            .writing
            .lock()
            .map_err(|_| "settings are unavailable".to_string())?;
        settings::update(&self.path, change).map_err(|error| settings_problem(&error).to_string())
    }
}

/// How the picker looks and what it formats with, remembered across restarts
/// (ADR 0020). A new key format is saved with the same preference change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub(crate) struct PickerPreferences {
    /// A style chosen in the format menu, or `null` to copy citation keys.
    pub(crate) citation_style: Option<CitationStyle>,
    pub(crate) density: Density,
    pub(crate) larger_text: bool,
}

impl PickerPreferences {
    fn from_settings(settings: &settings::Settings) -> Self {
        PickerPreferences {
            citation_style: settings.citation_style,
            density: settings.density.unwrap_or(Density::Comfortable),
            larger_text: settings.larger_text.unwrap_or(false),
        }
    }
}

/// The preferences the picker starts with.
#[tauri::command]
#[specta::specta]
pub(crate) fn picker_preferences(settings_file: State<'_, SettingsFile>) -> PickerPreferences {
    PickerPreferences::from_settings(&settings::load(&settings_file.path).settings)
}

/// Remember the picker preferences; other settings in the file are kept.
#[tauri::command]
#[specta::specta]
pub(crate) fn set_picker_preferences(
    preferences: PickerPreferences,
    copy_format: Option<KeyFormat>,
    settings_file: State<'_, SettingsFile>,
) -> Result<(), String> {
    settings_file.save_preferences(preferences, copy_format).map(|_| ())
}

impl SettingsFile {
    fn save_preferences(
        &self,
        preferences: PickerPreferences,
        copy_format: Option<KeyFormat>,
    ) -> Result<settings::Settings, String> {
        self.update(|s| {
            if let Some(format) = copy_format {
                s.copy_format = Some(format);
            }
            s.citation_style = preferences.citation_style;
            // Defaults stay unwritten, so the file only names real choices.
            s.density = Some(preferences.density).filter(|d| *d != Density::Comfortable);
            s.larger_text = Some(preferences.larger_text).filter(|larger| *larger);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::settings::LibrarySetting;

    #[test]
    fn unreadable_settings_report_no_operating_system_payload() {
        let loaded = settings::Loaded {
            settings: settings::Settings::default(),
            problem: Some(settings::SettingsError::Unreadable(std::io::Error::other(
                "synthetic-private-detail",
            ))),
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
            (format!(r#"{{"copy_format":"{private}"}}"#), "not valid"),
            (r#"{"schema_version":3}"#.to_string(), "newer version"),
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
            let failed = SettingsFile::new(path.clone()).update(|s| s.copy_format = Some(KeyFormat::Keys));
            assert!(failed.is_err());
            assert!(
                !failed
                    .err()
                    .unwrap_or_else(|| panic!("expected the invalid settings save to fail"))
                    .contains(private)
            );
            assert_eq!(
                std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{e}")),
                contents
            );
            std::fs::write(path.with_extension("json.bak"), "{}").unwrap_or_else(|e| panic!("{e}"));
            let recovered = settings::load(&path);
            assert!(
                settings_notice(&path, &recovered)
                    .unwrap_or_else(|| panic!("expected a settings recovery notice"))
                    .contains("backup")
            );
            std::fs::remove_file(path.with_extension("json.bak")).unwrap_or_else(|e| panic!("{e}"));
        }
        std::fs::remove_file(&path).unwrap_or_else(|e| panic!("{e}"));
        assert!(settings_notice(&path, &settings::load(&path)).is_none());
    }

    #[test]
    fn settings_saves_from_several_threads_never_lose_each_other() {
        let dir = std::env::temp_dir().join(format!("ibis-concurrent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = std::sync::Arc::new(SettingsFile::new(dir.join("settings.json")));
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let file = file.clone();
                std::thread::spawn(move || {
                    for _ in 0..20 {
                        let saved = if i % 2 == 0 {
                            file.update(|s| s.library = Some(LibrarySetting::Bibtex(PathBuf::from("refs.bib"))))
                        } else {
                            file.update(|s| s.copy_format = Some(KeyFormat::Typst))
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
        assert_eq!(loaded.settings.copy_format, Some(KeyFormat::Typst));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn picker_preferences_save_only_real_choices_and_keep_other_settings() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        file.update(|s| s.copy_format = Some(KeyFormat::Typst))
            .unwrap_or_else(|e| panic!("{e}"));
        let chosen = PickerPreferences {
            citation_style: Some(CitationStyle::Apa),
            density: Density::Spacious,
            larger_text: true,
        };
        let saved = file.save_preferences(chosen, None).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(saved.copy_format, Some(KeyFormat::Typst));
        assert_eq!(PickerPreferences::from_settings(&saved), chosen);
        let defaults = PickerPreferences {
            citation_style: None,
            density: Density::Comfortable,
            larger_text: false,
        };
        let saved = file.save_preferences(defaults, None).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(PickerPreferences::from_settings(&saved), defaults);
        let text = std::fs::read_to_string(&file.path).unwrap_or_default();
        for field in ["citation_style", "density", "larger_text"] {
            assert!(!text.contains(field), "{field} is not written for the default");
        }
    }

    #[test]
    fn key_choices_are_saved_with_style_removal_before_any_copy() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        let styled = PickerPreferences {
            citation_style: Some(CitationStyle::Apa),
            density: Density::Compact,
            larger_text: true,
        };
        for format in [
            KeyFormat::Pandoc,
            KeyFormat::Autocite,
            KeyFormat::Parencite,
            KeyFormat::Textcite,
            KeyFormat::Cite,
            KeyFormat::Typst,
            KeyFormat::Keys,
        ] {
            file.save_preferences(styled, None).unwrap_or_else(|e| panic!("{e}"));
            let chosen = PickerPreferences {
                citation_style: None,
                ..styled
            };
            file.save_preferences(chosen, Some(format))
                .unwrap_or_else(|e| panic!("{e}"));
            let loaded = settings::load(&file.path);
            assert_eq!(loaded.settings.copy_format, Some(format));
            assert_eq!(PickerPreferences::from_settings(&loaded.settings), chosen);
        }
        // A style/density save keeps a key changed in the file meanwhile.
        file.update(|s| s.copy_format = Some(KeyFormat::Keys))
            .unwrap_or_else(|e| panic!("{e}"));
        let saved = file.save_preferences(styled, None).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(saved.copy_format, Some(KeyFormat::Keys));
        assert_eq!(saved.citation_style, Some(CitationStyle::Apa));
    }

    #[test]
    fn choosing_a_key_never_overwrites_invalid_newer_or_unreadable_settings() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        let chosen = PickerPreferences {
            citation_style: None,
            density: Density::Comfortable,
            larger_text: false,
        };
        for text in [
            r#"{"copy_format":"synthetic-private-value"}"#,
            r#"{"schema_version":3}"#,
        ] {
            std::fs::write(&file.path, text).unwrap_or_else(|e| panic!("{e}"));
            let failed = file.save_preferences(chosen, Some(KeyFormat::Cite));
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
            file.save_preferences(chosen, Some(KeyFormat::Cite))
                .err()
                .unwrap_or_else(|| panic!("expected an unreadable settings save to fail")),
            "settings.json could not be read"
        );
        assert!(file.path.is_dir());
    }
}
