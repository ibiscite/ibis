//! One owner for the first three plain-copy hints, shared by picker and receipt.

use ibis_core::settings;
use tauri::State;

use super::SettingsFile;

pub(super) struct CopyHints {
    count: u8,
    saved: u8,
}

impl SettingsFile {
    /// Consume once per successful plain-copy outcome, never when rendering it.
    /// Keep the in-memory count on failed I/O and retry the latest value later.
    pub(crate) fn take_copy_hint(&self) -> bool {
        let Ok(mut hints) = self.copy_hints.lock() else {
            return false;
        };
        let hints = hints.get_or_insert_with(|| {
            let loaded = settings::load(&self.path);
            // An uncertain read must not keep replaying first-use feedback.
            let count = if loaded.problem.is_some() {
                3
            } else {
                loaded.settings.copy_hint_count.unwrap_or(0).min(3)
            };
            CopyHints { count, saved: count }
        });
        let show = hints.count < 3;
        hints.count = hints.count.saturating_add(1).min(3);
        if hints.count > hints.saved {
            let count = hints.count;
            if self
                .update(|s| s.copy_hint_count = Some(s.copy_hint_count.unwrap_or(0).max(count).min(3)))
                .is_ok()
            {
                hints.saved = count;
            }
        }
        show
    }
}

/// First-use feedback for a successful copy that stays in the picker.
#[tauri::command]
#[specta::specta]
pub(crate) async fn copy_hint(settings_file: State<'_, SettingsFile>) -> Result<bool, ()> {
    let file = settings_file.inner().clone();
    tauri::async_runtime::spawn_blocking(move || file.take_copy_hint())
        .await
        .map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use crate::preferences::SettingsFile;
    use ibis_core::settings;

    #[test]
    fn copy_hint_first_three_survive_close_reopen_and_restart() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("settings.json");
        let file = SettingsFile::new(path.clone());
        assert!(file.take_copy_hint());
        assert!(file.clone().take_copy_hint());
        let restarted = SettingsFile::new(path.clone());
        assert!(restarted.take_copy_hint());
        assert!(!restarted.take_copy_hint());
        let loaded = settings::load(&path);
        assert_eq!(loaded.settings.copy_hint_count, Some(3));
        assert_eq!(loaded.settings.schema_version, settings::SCHEMA_VERSION);
        assert!(!SettingsFile::new(path).take_copy_hint());
    }

    #[test]
    fn copy_hint_failed_saves_still_exhaust_and_recovery_persists_the_latest_count() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("settings.json");
        let file = SettingsFile::new(path.clone());
        assert!(file.take_copy_hint());
        std::fs::remove_file(&path).unwrap_or_else(|e| panic!("{e}"));
        std::fs::create_dir(&path).unwrap_or_else(|e| panic!("{e}"));
        assert!(file.take_copy_hint());
        assert!(file.take_copy_hint());
        assert!(!file.take_copy_hint());
        std::fs::remove_dir(&path).unwrap_or_else(|e| panic!("{e}"));
        assert!(!file.take_copy_hint());
        assert_eq!(settings::load(&path).settings.copy_hint_count, Some(3));
        assert!(!SettingsFile::new(path).take_copy_hint());
    }

    #[test]
    fn copy_hint_concurrent_owners_share_one_monotonic_count_and_keep_other_settings() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("settings.json");
        let file = SettingsFile::new(path.clone());
        file.update(|s| s.paste = Some(false)).unwrap_or_else(|e| panic!("{e}"));
        let hints: usize = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    let file = file.clone();
                    scope.spawn(move || usize::from(file.take_copy_hint()))
                })
                .collect();
            handles
                .into_iter()
                .map(|thread| thread.join().unwrap_or_else(|e| panic!("{e:?}")))
                .sum()
        });
        assert_eq!(hints, 3);
        let loaded = settings::load(&path);
        assert_eq!(loaded.settings.copy_hint_count, Some(3));
        assert_eq!(loaded.settings.paste, Some(false));
    }
}
