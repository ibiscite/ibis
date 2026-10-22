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
        settings::SettingsError::InvalidZoteroLibraryEntry { .. } => {
            "settings.json has an invalid Zotero library entry"
        }
        settings::SettingsError::NewerVersion(_) => "settings.json was written by a newer version of Ibis",
    }
}

/// Interface and failed-save text may include a typed position, never raw
/// serde or operating-system details. Diagnostics keep the fixed description.
fn settings_message(problem: &settings::SettingsError) -> String {
    match problem {
        settings::SettingsError::InvalidZoteroLibraryEntry { .. } => problem.to_string(),
        _ => settings_problem(problem).to_string(),
    }
}

/// This notice is for the interface only: its settings path must not be logged.
pub(crate) fn settings_notice(path: &Path, loaded: &settings::Loaded) -> Option<String> {
    loaded.problem.as_ref().map(|problem| {
        let recovery = if loaded.from_backup { "the backup" } else { "defaults" };
        format!(
            "{}; using {recovery}. Changes are not being saved while this problem remains. Settings file: {}",
            settings_message(problem),
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
        settings::update(&self.path, change).map_err(|error| settings_message(&error))
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
    pub(crate) show_paste_preview: bool,
}

impl PickerPreferences {
    fn from_settings(settings: &settings::Settings) -> Self {
        PickerPreferences {
            citation_style: settings.citation_style,
            density: settings.density.unwrap_or(Density::Comfortable),
            larger_text: settings.larger_text.unwrap_or(false),
            show_paste_preview: settings.show_paste_preview.unwrap_or(true),
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
            s.show_paste_preview = Some(preferences.show_paste_preview).filter(|shown| !shown);
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
            (format!(r#"{{"copy_format":"{private}"}}"#), "not valid"),
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
            let Err(failed) = SettingsFile::new(path.clone()).update(|s| s.copy_format = Some(KeyFormat::Keys)) else {
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
            std::fs::remove_file(path.with_extension("json.bak")).unwrap_or_else(|e| panic!("{e}"));
        }
        std::fs::remove_file(&path).unwrap_or_else(|e| panic!("{e}"));
        assert!(settings_notice(&path, &settings::load(&path)).is_none());
    }

    #[test]
    fn settings_recovery_and_preference_changes_leave_damaged_files_untouched() {
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
                let backup_contents = br#"{"schema_version":2,"copy_format":"typst"}"#;
                std::fs::write(&file.path, contents).unwrap_or_else(|e| panic!("{e}"));
                std::fs::write(&temporary, b"synthetic unfinished save").unwrap_or_else(|e| panic!("{e}"));
                if has_backup {
                    std::fs::write(&backup, backup_contents).unwrap_or_else(|e| panic!("{e}"));
                }
                let loaded = settings::load(&file.path);
                assert!(loaded.problem.is_some());
                assert_eq!(loaded.from_backup, has_backup);
                assert_eq!(loaded.settings.copy_format, has_backup.then_some(KeyFormat::Typst));
                assert_eq!(std::fs::read(&file.path).unwrap_or_else(|e| panic!("{e}")), contents);
                let chosen = PickerPreferences::from_settings(&loaded.settings);
                assert!(file.save_preferences(chosen, Some(KeyFormat::Cite)).is_err());
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
            show_paste_preview: false,
        };
        let saved = file.save_preferences(chosen, None).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(saved.copy_format, Some(KeyFormat::Typst));
        assert_eq!(PickerPreferences::from_settings(&saved), chosen);
        let defaults = PickerPreferences {
            citation_style: None,
            density: Density::Comfortable,
            larger_text: false,
            show_paste_preview: true,
        };
        let saved = file.save_preferences(defaults, None).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(PickerPreferences::from_settings(&saved), defaults);
        let text = std::fs::read_to_string(&file.path).unwrap_or_default();
        for field in ["citation_style", "density", "larger_text", "show_paste_preview"] {
            assert!(!text.contains(field), "{field} is not written for the default");
        }
    }

    #[test]
    fn paste_preview_defaults_on_and_off_survives_restart_without_losing_settings() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        assert!(PickerPreferences::from_settings(&settings::Settings::default()).show_paste_preview);

        let original = serde_json::json!({
            "schema_version": 2, "shortcut": "Alt+Space", "library": { "bibtex": "refs.bib" },
            "copy_format": "typst", "update_checks": false, "citation_style": "ieee",
            "density": "compact", "larger_text": true, "future_choice": { "kept": [true, null] },
        });
        std::fs::write(&file.path, original.to_string()).unwrap_or_else(|e| panic!("{e}"));
        let mut chosen = PickerPreferences::from_settings(&settings::load(&file.path).settings);
        chosen.show_paste_preview = false;
        file.save_preferences(chosen, None).unwrap_or_else(|e| panic!("{e}"));
        let restarted = SettingsFile::new(file.path.clone());
        let loaded = settings::load(&restarted.path);
        assert!(loaded.problem.is_none());
        assert_eq!(PickerPreferences::from_settings(&loaded.settings), chosen);
        let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file.path).unwrap_or_default())
            .unwrap_or_else(|e| panic!("{e}"));
        for (field, value) in original.as_object().unwrap_or_else(|| panic!("expected an object")) {
            assert_eq!(&saved[field], value, "{field} survives the preview choice");
        }
        assert_eq!(saved["show_paste_preview"], false);

        chosen.show_paste_preview = true;
        restarted
            .save_preferences(chosen, None)
            .unwrap_or_else(|e| panic!("{e}"));
        let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file.path).unwrap_or_default())
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(
            saved.get("show_paste_preview").is_none(),
            "the on default stays unwritten"
        );
        assert_eq!(saved["future_choice"], original["future_choice"]);
    }

    #[test]
    fn key_choices_are_saved_with_style_removal_before_any_copy() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        let styled = PickerPreferences {
            citation_style: Some(CitationStyle::Apa),
            density: Density::Compact,
            larger_text: true,
            show_paste_preview: false,
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
            show_paste_preview: false,
        };
        for text in [
            r#"{"copy_format":"synthetic-private-value"}"#,
            r#"{"show_paste_preview":"synthetic-private-value"}"#,
            r#"{"schema_version":3,"show_paste_preview":false,"future_choice":{"kept":true}}"#,
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
