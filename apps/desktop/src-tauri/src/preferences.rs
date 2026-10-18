//! Saving choices made in the picker to the settings file.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ibis_core::settings;

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

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::cite::KeyFormat;

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
            (r#"{"schema_version":2}"#.to_string(), "newer version"),
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
                            file.update(|s| s.library = Some(PathBuf::from("refs.bib")))
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
        assert_eq!(loaded.settings.library, Some(PathBuf::from("refs.bib")));
        assert_eq!(loaded.settings.copy_format, Some(KeyFormat::Typst));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
