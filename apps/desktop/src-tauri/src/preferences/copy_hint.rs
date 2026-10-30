//! Decide copy hints from memory. Saving cannot delay or publish feedback.

use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use ibis_core::settings;

use super::SettingsFile;

struct Count {
    used: AtomicU8,
    saved: AtomicU8,
}

enum Save {
    Count,
    #[cfg(test)]
    Flush(mpsc::Sender<()>),
}

pub(super) struct CopyHints {
    count: Arc<Count>,
    save: Option<mpsc::SyncSender<Save>>,
}

impl CopyHints {
    pub(super) fn new(path: &Path, writing: Arc<Mutex<()>>) -> Self {
        let loaded = settings::load(path);
        // Read once at startup. Uncertain storage never replays first-use hints.
        let used = if loaded.problem.is_some() {
            3
        } else {
            loaded.settings.copy_hint_count.unwrap_or(0).min(3)
        };
        let count = Arc::new(Count {
            used: AtomicU8::new(used),
            saved: AtomicU8::new(used),
        });
        let (save, pending) = mpsc::sync_channel(1);
        let worker_count = count.clone();
        let path = path.to_path_buf();
        // One sleeping writer and one queued signal. Each save takes the latest
        // count; neither a slow disk nor repeated copies can grow a worker queue.
        let worker = std::thread::Builder::new()
            .name("ibis-copy-hints".into())
            .spawn(move || {
                for _request in pending {
                    if worker_count.used.load(Ordering::Acquire) > worker_count.saved.load(Ordering::Acquire)
                        && let Ok(_writing) = writing.lock()
                    {
                        let used = worker_count.used.load(Ordering::Acquire);
                        if settings::update(&path, |s| {
                            s.copy_hint_count = Some(s.copy_hint_count.unwrap_or(0).max(used).min(3))
                        })
                        .is_ok()
                        {
                            worker_count.saved.store(used, Ordering::Release);
                        }
                    }
                    #[cfg(test)]
                    if let Save::Flush(done) = _request {
                        let _ = done.send(());
                    }
                }
            });
        Self {
            count,
            save: worker.ok().map(|_| save),
        }
    }
}

impl SettingsFile {
    /// Called once by a successful clipboard write. No file I/O or reply.
    /// A failed save leaves the bounded memory count intact; a later copy retries.
    pub(crate) fn take_copy_hint(&self) -> bool {
        let hints = &self.copy_hints;
        let mut previous = hints.count.used.load(Ordering::Acquire);
        while previous < 3 {
            match hints
                .count
                .used
                .compare_exchange_weak(previous, previous + 1, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => break,
                Err(current) => previous = current,
            }
        }
        if hints.count.used.load(Ordering::Acquire) > hints.count.saved.load(Ordering::Acquire)
            && let Some(save) = &hints.save
        {
            let _ = save.try_send(Save::Count);
        }
        previous < 3
    }
}

#[cfg(test)]
mod tests {
    use crate::preferences::SettingsFile;
    use ibis_core::settings;

    fn saved(file: &SettingsFile) {
        let (done, completed) = std::sync::mpsc::channel();
        file.copy_hints
            .save
            .as_ref()
            .unwrap_or_else(|| panic!("writer unavailable"))
            .send(super::Save::Flush(done))
            .unwrap_or_else(|e| panic!("{e}"));
        completed
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap_or_else(|e| panic!("{e}"));
    }

    #[test]
    fn a_blocked_hint_save_cannot_publish_a_after_b_was_pasted() {
        use std::sync::mpsc;
        use std::time::Duration;

        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        let writing = file.writing.lock().unwrap_or_else(|e| panic!("{e}"));
        std::thread::scope(|scope| {
            let (published, receipts) = mpsc::channel();
            let file = &file;
            let a = scope.spawn(move || {
                assert!(file.take_copy_hint());
                published
                    .send("Copied A. Press Ctrl+V to paste")
                    .unwrap_or_else(|e| panic!("{e}"));
            });
            // A has copied; storage is blocked. B then replaces the clipboard
            // and pastes. Releasing A's save must never publish an older receipt.
            let first = receipts.recv_timeout(Duration::from_millis(500));
            let mut shown: Vec<_> = first.iter().copied().collect();
            shown.push("Pasted B into Example Writer");
            drop(writing);
            a.join().unwrap_or_else(|e| panic!("{e:?}"));
            if first.is_err() {
                shown.push(receipts.recv().unwrap_or_else(|e| panic!("{e}")));
            }
            assert_eq!(shown.last(), Some(&"Pasted B into Example Writer"));
        });
        saved(&file);
    }

    #[test]
    fn copy_hint_first_three_survive_close_reopen_and_restart() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("settings.json");
        let file = SettingsFile::new(path.clone());
        assert!(file.take_copy_hint());
        assert!(file.clone().take_copy_hint());
        saved(&file);
        let restarted = SettingsFile::new(path.clone());
        assert!(restarted.take_copy_hint());
        assert!(!restarted.take_copy_hint());
        saved(&restarted);
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
        saved(&file);
        std::fs::remove_file(&path).unwrap_or_else(|e| panic!("{e}"));
        std::fs::create_dir(&path).unwrap_or_else(|e| panic!("{e}"));
        assert!(file.take_copy_hint());
        assert!(file.take_copy_hint());
        assert!(!file.take_copy_hint());
        saved(&file);
        std::fs::remove_dir(&path).unwrap_or_else(|e| panic!("{e}"));
        assert!(!file.take_copy_hint());
        saved(&file);
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
        saved(&file);
        let loaded = settings::load(&path);
        assert_eq!(loaded.settings.copy_hint_count, Some(3));
        assert_eq!(loaded.settings.paste, Some(false));
    }
}
