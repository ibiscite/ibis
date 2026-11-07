//! The one owner of a settings file (decision 0040). Every change is submitted
//! whole, in the order it was asked for, and one worker applies them one at a
//! time, each as the core transaction: the cross-process lock, a fresh read,
//! the change, validation and an atomic write.
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

use ibis_core::settings;
pub use ibis_core::settings::{Loaded, Settings, SettingsError};

type Job = Box<dyn FnOnce(&Path) + Send>;

/// Owns the writes to one settings file in this process.
pub struct Owner {
    path: PathBuf,
    jobs: Mutex<Option<Sender<Job>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

/// The outcome of a submitted change, once it has run.
#[must_use = "a change's outcome says whether it was saved"]
pub struct Pending<T>(Receiver<T>);

impl<T> Pending<T> {
    /// Wait for the change to run. `None` only when the owner closed first.
    pub fn wait(self) -> Option<T> {
        self.0.recv().ok()
    }
}

impl Owner {
    pub fn open(path: PathBuf) -> Self {
        let (sender, jobs) = mpsc::channel::<Job>();
        let file = path.clone();
        let worker = std::thread::Builder::new()
            .name("ibis-settings".into())
            .spawn(move || {
                for job in jobs {
                    // A change that panics fails alone; the ones after it still run.
                    let _ = catch_unwind(AssertUnwindSafe(|| job(&file)));
                }
            })
            .ok();
        Owner {
            path,
            jobs: Mutex::new(worker.as_ref().map(|_| sender)),
            worker: Mutex::new(worker),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The settings as they are on disk now, with how they were read: a
    /// recovered copy is never mistaken for healthy settings.
    pub fn read(&self) -> Loaded {
        settings::load(&self.path)
    }

    /// Change settings after every change asked for before this one. Returns
    /// at once; the change runs as one transaction on the current file, and
    /// a refused change saves nothing. A change that depends on something
    /// else being current checks it here and leaves the settings alone.
    pub fn submit<T, E>(
        &self,
        change: impl FnOnce(&mut Settings) -> Result<T, E> + Send + 'static,
    ) -> Pending<Result<(Settings, T), E>>
    where
        T: Send + 'static,
        E: From<SettingsError> + Send + 'static,
    {
        self.submit_on_file(move |path| settings::transaction(path, change))
    }

    /// Work on the file itself in the same order, for a commit with its own
    /// checks, such as restoring a backup.
    pub fn submit_on_file<T: Send + 'static>(&self, work: impl FnOnce(&Path) -> T + Send + 'static) -> Pending<T> {
        let (reply, outcome) = mpsc::channel();
        let job: Job = Box::new(move |path| {
            let _ = reply.send(work(path));
        });
        if let Ok(jobs) = self.jobs.lock()
            && let Some(jobs) = jobs.as_ref()
        {
            let _ = jobs.send(job);
        }
        Pending(outcome)
    }

    /// Submit and wait, for callers with nothing else to do meanwhile.
    pub fn change<T, E>(
        &self,
        change: impl FnOnce(&mut Settings) -> Result<T, E> + Send + 'static,
    ) -> Result<(Settings, T), E>
    where
        T: Send + 'static,
        E: From<SettingsError> + Send + 'static,
    {
        self.submit(change).wait().unwrap_or_else(|| Err(closed().into()))
    }

    /// Finish every change already asked for, then refuse new ones.
    pub fn close(&self) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.take();
        }
        if let Some(worker) = self.worker.lock().ok().and_then(|mut worker| worker.take()) {
            let _ = worker.join();
        }
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        self.close();
    }
}

fn closed() -> SettingsError {
    SettingsError::Unreadable(std::io::Error::other("settings are closing"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    fn owner() -> (tempfile::TempDir, Owner) {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary folder: {error}"));
        let owner = Owner::open(directory.path().join("settings.json"));
        (directory, owner)
    }

    fn density(owner: &Owner, density: settings::Density) -> Result<(Settings, ()), SettingsError> {
        owner.change(move |s| {
            s.density = Some(density);
            Ok(())
        })
    }

    #[test]
    fn a_change_is_saved_and_reopening_reads_it() {
        let (directory, owner) = owner();
        let (saved, ()) = density(&owner, settings::Density::Compact).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(saved.density, Some(settings::Density::Compact));
        owner.close();
        let reopened = Owner::open(directory.path().join("settings.json"));
        assert_eq!(reopened.read().settings.density, Some(settings::Density::Compact));
    }

    #[test]
    fn a_refused_save_writes_nothing_and_later_changes_still_run() {
        let (directory, owner) = owner();
        let path = directory.path().join("settings.json");
        std::fs::write(&path, r#"{"schema_version": 99, "density": "compact"}"#)
            .unwrap_or_else(|error| panic!("{error}"));
        let before = std::fs::read(&path).unwrap_or_default();
        assert!(matches!(
            density(&owner, settings::Density::Comfortable),
            Err(SettingsError::NewerVersion(99))
        ));
        assert_eq!(
            std::fs::read(&path).unwrap_or_default(),
            before,
            "a newer file is left as it is"
        );
        std::fs::remove_file(&path).unwrap_or_else(|error| panic!("{error}"));
        assert!(
            density(&owner, settings::Density::Compact).is_ok(),
            "the owner keeps working"
        );
    }

    #[test]
    fn a_failed_change_publishes_nothing_and_the_next_one_runs() {
        let (_directory, owner) = owner();
        density(&owner, settings::Density::Compact).unwrap_or_else(|error| panic!("{error}"));
        let failed: Result<(Settings, ()), SettingsError> = owner.change(|s| {
            s.density = Some(settings::Density::Comfortable);
            Err(SettingsError::Invalid("refused by its own check".into()))
        });
        assert!(failed.is_err());
        assert_eq!(owner.read().settings.density, Some(settings::Density::Compact));
        let panicked = owner.submit(|_: &mut Settings| -> Result<(), SettingsError> { panic!("a broken change") });
        assert!(panicked.wait().is_none(), "a change that panics fails alone");
        assert!(density(&owner, settings::Density::Comfortable).is_ok());
    }

    #[test]
    fn a_later_choice_lands_after_a_slow_earlier_one() {
        let (_directory, owner) = owner();
        let started = Arc::new(AtomicBool::new(false));
        let running = started.clone();
        // The earlier choice is still being applied when the later one is asked for.
        let earlier = owner.submit(move |s: &mut Settings| -> Result<(), SettingsError> {
            running.store(true, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(50));
            s.density = Some(settings::Density::Compact);
            Ok(())
        });
        while !started.load(Ordering::SeqCst) {
            std::thread::yield_now();
        }
        let later = owner.submit(|s: &mut Settings| -> Result<(), SettingsError> {
            s.density = Some(settings::Density::Comfortable);
            Ok(())
        });
        assert!(earlier.wait().is_some_and(|saved| saved.is_ok()));
        assert!(later.wait().is_some_and(|saved| saved.is_ok()));
        assert_eq!(
            owner.read().settings.density,
            Some(settings::Density::Comfortable),
            "the stale choice never wins"
        );
    }

    #[test]
    fn changes_from_two_threads_are_all_kept() {
        let (_directory, owner) = owner();
        let owner = Arc::new(owner);
        let threads: Vec<_> = [true, false]
            .into_iter()
            .map(|larger| {
                let owner = owner.clone();
                std::thread::spawn(move || {
                    owner.change(move |s| {
                        if larger {
                            s.larger_text = Some(true);
                        } else {
                            s.menu_bar = Some(false);
                        }
                        Ok::<_, SettingsError>(())
                    })
                })
            })
            .collect();
        for thread in threads {
            assert!(thread.join().is_ok_and(|saved| saved.is_ok()));
        }
        let saved = owner.read().settings;
        assert_eq!((saved.larger_text, saved.menu_bar), (Some(true), Some(false)));
    }

    #[test]
    fn closing_finishes_what_was_asked_and_refuses_the_rest() {
        let (directory, owner) = owner();
        let pending = owner.submit(|s: &mut Settings| -> Result<(), SettingsError> {
            s.density = Some(settings::Density::Compact);
            Ok(())
        });
        owner.close();
        assert!(
            pending.wait().is_some_and(|saved| saved.is_ok()),
            "asked before closing, so saved"
        );
        assert!(
            density(&owner, settings::Density::Comfortable).is_err(),
            "asked after closing, so refused"
        );
        assert_eq!(
            Owner::open(directory.path().join("settings.json"))
                .read()
                .settings
                .density,
            Some(settings::Density::Compact)
        );
    }
}
