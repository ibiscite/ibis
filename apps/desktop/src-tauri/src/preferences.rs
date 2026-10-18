//! Saving choices made in the picker to the settings file.

use std::path::PathBuf;
use std::sync::Mutex;

use ibis_core::settings;

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
        settings::update(&self.path, change).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::cite::KeyFormat;

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
