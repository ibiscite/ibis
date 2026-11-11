//! The one owner of a settings file (decision 0040). Every change is submitted
//! whole, in the order it was asked for, and one worker applies them one at a
//! time, each as the core transaction: the cross-process lock, a fresh read,
//! the change, validation and an atomic write.
//!
//! An edit to the file from outside, by hand or by the command line, is read
//! in the same queue and reported once, so it never races a change here
//! (decision 0042). A file that cannot be read is reported as it is and never
//! written (0016): every transaction refuses it.
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use ibis_core::settings;
pub use ibis_core::settings::{Loaded, Settings, SettingsError};

type Job = Box<dyn FnOnce(&Path) + Send>;

/// Owns the writes to one settings file in this process.
pub struct Owner {
    path: PathBuf,
    /// Shared with a watch, which sends into the same queue until it closes.
    jobs: Arc<Mutex<Option<Sender<Job>>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    /// The file's bytes after the last job, so an event for a write made here
    /// is recognised and not reported as an outside edit.
    seen: Arc<Mutex<Option<Vec<u8>>>>,
}

/// Watching a settings file; dropping it stops.
pub struct Watching {
    _watcher: notify::RecommendedWatcher,
}

/// Events from one save arrive together; one read follows them.
const SETTLE: Duration = Duration::from_millis(150);

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
        let seen = Arc::new(Mutex::new(std::fs::read(&path).ok()));
        let after = Arc::clone(&seen);
        let worker = std::thread::Builder::new()
            .name("ibis-settings".into())
            .spawn(move || {
                for job in jobs {
                    // A change that panics fails alone; the ones after it still run.
                    let _ = catch_unwind(AssertUnwindSafe(|| job(&file)));
                    if let Ok(mut seen) = after.lock() {
                        *seen = std::fs::read(&file).ok();
                    }
                }
            })
            .ok();
        Owner {
            path,
            jobs: Arc::new(Mutex::new(worker.as_ref().map(|_| sender))),
            worker: Mutex::new(worker),
            seen,
        }
    }

    /// Report each edit made to the file from outside this owner, read in turn
    /// with the changes made here: what was read, with any problem, so an
    /// unreadable file is shown rather than replaced. Writes made here, and a
    /// save that leaves the bytes as they were, report nothing. `None` when
    /// the folder cannot be watched; the file is then read when it is used.
    pub fn watch(&self, edited: impl Fn(Loaded) + Send + Sync + 'static) -> Option<Watching> {
        use notify::Watcher;
        let folder = self.path.parent()?.to_path_buf();
        let name = self.path.file_name()?.to_owned();
        let jobs = Arc::clone(&self.jobs);
        let (signal, events) = mpsc::sync_channel::<()>(1);
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            // Atomic saves rename a sibling over the file, so any event naming it counts.
            if event.is_ok_and(|event| event.paths.iter().any(|path| path.file_name() == Some(&name))) {
                let _ = signal.try_send(());
            }
        })
        .ok()?;
        watcher.watch(&folder, notify::RecursiveMode::NonRecursive).ok()?;
        let seen = Arc::clone(&self.seen);
        let edited = Arc::new(edited);
        std::thread::Builder::new()
            .name("ibis-settings-watch".into())
            .spawn(move || {
                while events.recv().is_ok() {
                    // Let the rest of this save land, then read once.
                    while events.recv_timeout(SETTLE).is_ok() {}
                    let (seen, edited) = (Arc::clone(&seen), Arc::clone(&edited));
                    let job: Job = Box::new(move |path| {
                        let bytes = std::fs::read(path).ok();
                        let new = seen.lock().is_ok_and(|seen| *seen != bytes);
                        if new {
                            edited(settings::load(path));
                        }
                    });
                    // Once the owner closed, nothing more is read.
                    let sent = jobs
                        .lock()
                        .ok()
                        .and_then(|jobs| jobs.as_ref().map(|jobs| jobs.send(job).is_ok()));
                    if sent != Some(true) {
                        return;
                    }
                }
            })
            .ok()?;
        Some(Watching { _watcher: watcher })
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

    /// Wait for this change's turn, then run it as one transaction on the
    /// calling thread, so it may read borrowed state. For callers that would
    /// wait anyway; never call it from inside another change.
    pub fn change<T, E: From<SettingsError>>(
        &self,
        change: impl FnOnce(&mut Settings) -> Result<T, E>,
    ) -> Result<(Settings, T), E> {
        self.in_turn(|path| settings::transaction(path, change))
            .unwrap_or_else(|| Err(closed().into()))
    }

    /// Run `work` on the calling thread when its turn in the queue comes; the
    /// changes after it wait until it returns. `None` once the owner closed.
    pub fn in_turn<T>(&self, work: impl FnOnce(&Path) -> T) -> Option<T> {
        let (start, turn) = mpsc::channel::<()>();
        let (finished, done) = mpsc::channel::<()>();
        // Its own outcome is the turn itself; nothing waits on the reply.
        let _ = self.submit_on_file(move |_| {
            let _ = start.send(());
            // Returns when `finished` is dropped: after `work`, or as it unwinds.
            let _ = done.recv();
        });
        turn.recv().ok()?;
        let result = work(&self.path);
        drop(finished);
        Some(result)
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
    use std::time::Duration;

    const CHANNEL_LIMIT: Duration = Duration::from_secs(5);

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

    /// The next edit the watch reports, within a bound so a lost event fails.
    fn next(edits: &mpsc::Receiver<Loaded>) -> Loaded {
        edits
            .recv_timeout(CHANNEL_LIMIT)
            .unwrap_or_else(|error| panic!("no edit reported: {error}"))
    }

    #[test]
    fn an_outside_edit_is_reported_once_and_a_write_here_is_not() {
        let (directory, owner) = owner();
        let path = directory.path().join("settings.json");
        density(&owner, settings::Density::Compact).unwrap_or_else(|error| panic!("{error}"));
        let (sender, edits) = mpsc::channel();
        let _watching = owner
            .watch(move |loaded| {
                let _ = sender.send(loaded);
            })
            .unwrap_or_else(|| panic!("the temporary folder can be watched"));
        // A write here is followed by an outside edit; only the edit is reported.
        density(&owner, settings::Density::Comfortable).unwrap_or_else(|error| panic!("{error}"));
        let mut edited = owner.read().settings;
        edited.larger_text = Some(true);
        std::fs::write(&path, serde_json::to_vec(&edited).unwrap_or_default())
            .unwrap_or_else(|error| panic!("{error}"));
        let reported = next(&edits);
        assert!(reported.problem.is_none());
        assert_eq!(
            (reported.settings.larger_text, reported.settings.density),
            (Some(true), Some(settings::Density::Comfortable))
        );
        // A later change here starts from the edit, so neither is lost.
        density(&owner, settings::Density::Compact).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(owner.read().settings.larger_text, Some(true));
    }

    #[test]
    fn an_unreadable_edit_is_reported_and_the_file_is_never_written() {
        let (directory, owner) = owner();
        let path = directory.path().join("settings.json");
        density(&owner, settings::Density::Compact).unwrap_or_else(|error| panic!("{error}"));
        let (sender, edits) = mpsc::channel();
        let _watching = owner
            .watch(move |loaded| {
                let _ = sender.send(loaded);
            })
            .unwrap_or_else(|| panic!("the temporary folder can be watched"));
        std::fs::write(&path, b"{ invented half-typed").unwrap_or_else(|error| panic!("{error}"));
        assert!(next(&edits).problem.is_some());
        assert!(density(&owner, settings::Density::Comfortable).is_err());
        assert_eq!(std::fs::read(&path).unwrap_or_default(), b"{ invented half-typed");
        // Fixing the file is reported too, and changes work again.
        let fixed = Settings {
            density: Some(settings::Density::Comfortable),
            ..Settings::default()
        };
        std::fs::write(&path, serde_json::to_vec(&fixed).unwrap_or_default()).unwrap_or_else(|error| panic!("{error}"));
        let reported = next(&edits);
        assert!(reported.problem.is_none());
        assert_eq!(reported.settings.density, Some(settings::Density::Comfortable));
        owner.close();
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
        let (started, running) = mpsc::channel();
        let (release, held) = mpsc::channel();
        // The earlier choice is still being applied when the later one is asked for.
        let earlier = owner.submit(move |s: &mut Settings| -> Result<(), SettingsError> {
            started
                .send(())
                .unwrap_or_else(|error| panic!("the test is waiting for the earlier choice: {error}"));
            held.recv_timeout(CHANNEL_LIMIT)
                .unwrap_or_else(|error| panic!("the test releases the earlier choice: {error}"));
            s.density = Some(settings::Density::Compact);
            Ok(())
        });
        running
            .recv_timeout(CHANNEL_LIMIT)
            .unwrap_or_else(|error| panic!("the earlier choice starts: {error}"));
        let later = owner.submit(|s: &mut Settings| -> Result<(), SettingsError> {
            s.density = Some(settings::Density::Comfortable);
            Ok(())
        });
        release
            .send(())
            .unwrap_or_else(|error| panic!("the earlier choice is held: {error}"));
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
    fn a_change_in_turn_waits_for_earlier_ones_and_holds_back_later_ones() {
        let (_directory, owner) = owner();
        let (release_earlier, earlier_held) = mpsc::channel();
        let earlier = owner.submit(move |s: &mut Settings| -> Result<(), SettingsError> {
            earlier_held
                .recv_timeout(CHANNEL_LIMIT)
                .unwrap_or_else(|error| panic!("the test releases the earlier choice: {error}"));
            s.density = Some(settings::Density::Compact);
            Ok(())
        });
        std::thread::scope(|scope| {
            let (entered, in_turn) = mpsc::channel();
            let (release_turn, turn_held) = mpsc::channel();
            let owner = &owner;
            let turn = scope.spawn(move || {
                // Borrowed state, read in its turn: the earlier change is already saved.
                let seen = std::cell::Cell::new(None);
                let (saved, ()) = owner
                    .change(|s| {
                        seen.set(s.density);
                        entered
                            .send(())
                            .unwrap_or_else(|error| panic!("the test is waiting for the turn: {error}"));
                        turn_held
                            .recv_timeout(CHANNEL_LIMIT)
                            .unwrap_or_else(|error| panic!("the test releases the turn: {error}"));
                        s.density = Some(settings::Density::Comfortable);
                        Ok::<_, SettingsError>(())
                    })
                    .unwrap_or_else(|error| panic!("{error}"));
                (seen.get(), saved.density)
            });
            release_earlier
                .send(())
                .unwrap_or_else(|error| panic!("the earlier choice is held: {error}"));
            in_turn
                .recv_timeout(CHANNEL_LIMIT)
                .unwrap_or_else(|error| panic!("the next turn starts: {error}"));
            let later = owner.submit_on_file(|path| settings::load(path).settings.density);
            release_turn
                .send(())
                .unwrap_or_else(|error| panic!("the turn is held: {error}"));
            assert_eq!(
                turn.join().unwrap_or_else(|_| panic!("the turn panicked")),
                (Some(settings::Density::Compact), Some(settings::Density::Comfortable))
            );
            assert_eq!(later.wait().flatten(), Some(settings::Density::Comfortable));
        });
        assert!(earlier.wait().is_some_and(|saved| saved.is_ok()));
        // A turn that panics releases the queue.
        let unwound = std::panic::catch_unwind(AssertUnwindSafe(|| owner.in_turn(|_| panic!("a broken change"))));
        assert!(unwound.is_err());
        assert!(density(&owner, settings::Density::Compact).is_ok());
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
