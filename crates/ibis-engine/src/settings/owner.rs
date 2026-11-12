//! One queue owns every settings operation and every outside-edit report.
use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use super::{Loaded, Settings, SettingsError};
use crate::settings;

type Work = Box<dyn FnOnce(&Path, u64) -> Completion + Send>;
#[derive(Default)]
struct Completion {
    written: Option<Vec<u8>>,
    reply: Option<Box<dyn FnOnce() + Send>>,
}
struct Job {
    receipt: u64,
    work: Work,
}
struct Queue {
    sender: Option<Sender<Job>>,
    next: u64,
}
type Edited = Arc<dyn Fn(u64, Loaded) + Send + Sync>;
#[derive(Default)]
struct WatchState {
    edited: Option<Edited>,
    watcher: Option<notify::RecommendedWatcher>,
}

/// Bytes are bounded and regular-file checked, including fingerprints.
#[derive(PartialEq, Eq)]
enum Fingerprint {
    Bytes(Vec<u8>),
    Failed(std::io::ErrorKind),
}
fn fingerprint(path: &Path) -> Fingerprint {
    match settings::read_regular_bytes(path) {
        Ok(bytes) => Fingerprint::Bytes(bytes),
        Err(error) => Fingerprint::Failed(error.kind()),
    }
}

// Record bytes actually committed by this job. Read-only and failed jobs never
// adopt an outside edit as their own last write.
thread_local! { static COMMITTED: RefCell<Option<Option<Vec<u8>>>> = const { RefCell::new(None) }; }
pub(super) fn committed(bytes: Vec<u8>) {
    COMMITTED.with(|slot| {
        if let Some(written) = slot.borrow_mut().as_mut() {
            *written = Some(bytes);
        }
    });
}
fn tracking<T>(work: impl FnOnce() -> T) -> (T, Option<Vec<u8>>) {
    struct Restore(Option<Option<Vec<u8>>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            COMMITTED.with(|slot| {
                slot.replace(self.0.take());
            });
        }
    }
    let restore = Restore(COMMITTED.with(|slot| slot.replace(Some(None))));
    let result = work();
    let written = COMMITTED.with(|slot| slot.borrow_mut().as_mut().and_then(Option::take));
    drop(restore);
    (result, written)
}

/// Owns one settings file. Queue receipts also order native and UI effects.
pub struct Owner {
    path: PathBuf,
    jobs: Arc<Mutex<Queue>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    watch: Arc<Mutex<WatchState>>,
}

/// Dropping this guard or closing its owner stops native watching.
pub struct Watching {
    state: Arc<Mutex<WatchState>>,
}
impl Drop for Watching {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            state.edited = None;
            state.watcher = None;
        }
    }
}
const SETTLE: Duration = Duration::from_millis(150);

#[must_use = "a change's outcome says whether it was saved"]
pub struct Pending<T>(Receiver<T>);
impl<T> Pending<T> {
    pub fn wait(self) -> Option<T> {
        self.0.recv().ok()
    }
}

fn report(path: &Path, receipt: u64, watch: &Mutex<WatchState>) {
    let edited = watch.lock().ok().and_then(|state| state.edited.clone());
    if let Some(edited) = edited {
        edited(receipt, settings::load(path));
    }
}
fn enqueue(queue: &Mutex<Queue>, work: Work) -> Option<u64> {
    let mut queue = queue.lock().ok()?;
    // Leave a slot before and after each job for reports found in its turn.
    let receipt = queue.next.checked_add(4).filter(|value| *value < (1 << 53) - 1)?;
    queue.sender.as_ref()?.send(Job { receipt, work }).ok()?;
    queue.next = receipt;
    Some(receipt)
}

fn signal_edit(event: notify::Result<notify::Event>, tracked: &Mutex<Vec<PathBuf>>, signal: &mpsc::SyncSender<()>) {
    if event.is_ok_and(|event| {
        // Inotify reports our own reads as opens. Treating them as edits would
        // make every fingerprint schedule another fingerprint indefinitely.
        !matches!(event.kind, notify::EventKind::Access(_))
            && tracked.lock().is_ok_and(|folders| {
                // Native events are hints: an atomic save can name only its
                // replacement file or directory. The owner compares bytes.
                event.paths.iter().any(|path| {
                    folders
                        .iter()
                        .any(|folder| path == folder || path.parent() == Some(folder))
                })
            })
    }) {
        let _ = signal.try_send(());
    }
}

impl Owner {
    pub fn open(path: PathBuf) -> Self {
        let (sender, jobs) = mpsc::channel::<Job>();
        let file = path.clone();
        let watch = Arc::new(Mutex::new(WatchState::default()));
        let watching = Arc::clone(&watch);
        let mut seen = fingerprint(&path);
        let worker = std::thread::Builder::new()
            .name("ibis-settings".into())
            .spawn(move || {
                for job in jobs {
                    let before = fingerprint(&file);
                    if before != seen {
                        seen = before;
                        let _ = catch_unwind(AssertUnwindSafe(|| report(&file, job.receipt - 1, &watching)));
                    }
                    let completed =
                        catch_unwind(AssertUnwindSafe(|| (job.work)(&file, job.receipt))).unwrap_or_default();
                    if let Some(bytes) = completed.written {
                        seen = Fingerprint::Bytes(bytes);
                    }
                    let after = fingerprint(&file);
                    if after != seen {
                        seen = after;
                        let _ = catch_unwind(AssertUnwindSafe(|| report(&file, job.receipt + 1, &watching)));
                    }
                    // Finish the whole turn before returning its outcome. The
                    // caller's next edit must not race this turn's final read.
                    if let Some(reply) = completed.reply {
                        reply();
                    }
                }
            })
            .ok();
        Owner {
            path,
            jobs: Arc::new(Mutex::new(Queue {
                sender: worker.as_ref().map(|_| sender),
                next: 0,
            })),
            worker: Mutex::new(worker),
            watch,
        }
    }

    /// Reports carry a queue receipt, allocated before the snapshot is read.
    /// Only the link's directory and its resolved target's directory are watched.
    pub fn watch(&self, edited: impl Fn(u64, Loaded) + Send + Sync + 'static) -> Option<Watching> {
        use notify::Watcher;
        let folder = self.path.parent()?.to_path_buf();
        // A new profile has no file yet. Its folder must exist before native
        // watching starts, so the first editor save is observed too.
        std::fs::create_dir_all(&folder).ok()?;
        // FSEvents reports canonical paths, including when the file is absent.
        // Resolve the directory without following the settings symlink itself.
        let folder = std::fs::canonicalize(folder).ok()?;
        let path = self.path.clone();
        let tracked = Arc::new(Mutex::new(vec![folder.clone()]));
        let relevant = Arc::clone(&tracked);
        let (signal, events) = mpsc::sync_channel::<()>(1);
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            signal_edit(event, &relevant, &signal);
        })
        .ok()?;
        watcher.watch(&folder, notify::RecursiveMode::NonRecursive).ok()?;
        let mut target_folder = None;
        rebind(&path, &folder, &mut target_folder, &mut watcher, &tracked);
        {
            let mut state = self.watch.lock().ok()?;
            state.edited = Some(Arc::new(edited));
            state.watcher = Some(watcher);
        }
        // Compare the opening fingerprint once after registration: an edit
        // during startup may have happened before native events were watched.
        enqueue(&self.jobs, Box::new(|_, _| Completion::default()))?;
        let state = Arc::clone(&self.watch);
        let jobs = Arc::clone(&self.jobs);
        std::thread::Builder::new()
            .name("ibis-settings-watch".into())
            .spawn(move || {
                while events.recv().is_ok() {
                    while events.recv_timeout(SETTLE).is_ok() {}
                    {
                        let Ok(mut state) = state.lock() else { return };
                        let Some(watcher) = state.watcher.as_mut() else { return };
                        rebind(&path, &folder, &mut target_folder, watcher, &tracked);
                    }
                    if enqueue(&jobs, Box::new(|_, _| Completion::default())).is_none() {
                        return;
                    }
                }
            })
            .ok()?;
        Some(Watching {
            state: Arc::clone(&self.watch),
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn read(&self) -> Loaded {
        settings::load(&self.path)
    }
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
    pub fn submit_on_file<T: Send + 'static>(&self, work: impl FnOnce(&Path) -> T + Send + 'static) -> Pending<T> {
        self.submit_on_file_receipt(work).1
    }
    /// The receipt is assigned while enqueuing, never when a delayed save finishes.
    pub fn submit_on_file_receipt<T: Send + 'static>(
        &self,
        work: impl FnOnce(&Path) -> T + Send + 'static,
    ) -> (Option<u64>, Pending<T>) {
        self.submit_on_file_with_receipt(move |_, path| work(path))
    }
    pub fn submit_on_file_with_receipt<T: Send + 'static>(
        &self,
        work: impl FnOnce(u64, &Path) -> T + Send + 'static,
    ) -> (Option<u64>, Pending<T>) {
        let (reply, outcome) = mpsc::channel();
        let receipt = enqueue(
            &self.jobs,
            Box::new(move |path, receipt| {
                let (result, written) = tracking(|| work(receipt, path));
                Completion {
                    written,
                    reply: Some(Box::new(move || {
                        let _ = reply.send(result);
                    })),
                }
            }),
        );
        (receipt, Pending(outcome))
    }
    pub fn change<T, E: From<SettingsError>>(
        &self,
        change: impl FnOnce(&mut Settings) -> Result<T, E>,
    ) -> Result<(Settings, T), E> {
        self.in_turn(|path| settings::transaction(path, change))
            .unwrap_or_else(|| Err(closed().into()))
    }
    pub fn in_turn<T>(&self, work: impl FnOnce(&Path) -> T) -> Option<T> {
        let (start, turn) = mpsc::channel::<()>();
        let (finished, done) = mpsc::channel::<Option<Vec<u8>>>();
        let (settled, completion) = mpsc::channel::<()>();
        enqueue(
            &self.jobs,
            Box::new(move |_, _| {
                let _ = start.send(());
                Completion {
                    written: done.recv().ok().flatten(),
                    reply: Some(Box::new(move || {
                        let _ = settled.send(());
                    })),
                }
            }),
        )?;
        turn.recv().ok()?;
        let (result, written) = tracking(|| work(&self.path));
        let _ = finished.send(written);
        completion.recv().ok()?;
        Some(result)
    }
    pub fn close(&self) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.sender.take();
        }
        if let Some(worker) = self.worker.lock().ok().and_then(|mut worker| worker.take()) {
            let _ = worker.join();
        }
        if let Ok(mut state) = self.watch.lock() {
            state.edited = None;
            state.watcher = None;
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
fn rebind(
    path: &Path,
    folder: &Path,
    target_folder: &mut Option<PathBuf>,
    watcher: &mut notify::RecommendedWatcher,
    tracked: &Mutex<Vec<PathBuf>>,
) {
    use notify::Watcher;
    let target = std::fs::canonicalize(path).ok();
    let desired = target
        .as_ref()
        .and_then(|target| target.parent())
        .filter(|parent| *parent != folder)
        .map(Path::to_path_buf);
    if &desired != target_folder {
        if let Some(previous) = target_folder.take() {
            let _ = watcher.unwatch(&previous);
        }
        if let Some(desired) = desired
            && watcher.watch(&desired, notify::RecursiveMode::NonRecursive).is_ok()
        {
            *target_folder = Some(desired);
        }
    }
    if let Ok(mut folders) = tracked.lock() {
        *folders = std::iter::once(folder.to_path_buf())
            .chain(target_folder.clone())
            .collect();
    }
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

    // Publish one complete editor save, without fs::write's transient truncation.
    fn replace(path: &Path, bytes: &[u8]) {
        let replacement = path.with_extension("replacement");
        std::fs::write(&replacement, bytes).unwrap_or_else(|error| panic!("{error}"));
        std::fs::rename(replacement, path).unwrap_or_else(|error| panic!("{error}"));
    }

    #[test]
    fn reading_settings_does_not_schedule_another_read() {
        use notify::event::{AccessKind, AccessMode};
        let folder = PathBuf::from("synthetic");
        let tracked = Mutex::new(vec![folder.clone()]);
        let (signal, events) = mpsc::sync_channel(1);
        for path in [folder.join("settings.json"), folder] {
            for access in [
                AccessKind::Read,
                AccessKind::Open(AccessMode::Any),
                AccessKind::Close(AccessMode::Read),
            ] {
                signal_edit(
                    Ok(notify::Event::new(notify::EventKind::Access(access)).add_path(path.clone())),
                    &tracked,
                    &signal,
                );
                assert!(matches!(events.try_recv(), Err(mpsc::TryRecvError::Empty)));
            }
        }
    }

    #[test]
    fn replacement_and_directory_events_schedule_a_read_in_both_folders() {
        use notify::EventKind;
        use notify::event::{CreateKind, ModifyKind, RemoveKind, RenameMode};
        let folders = [PathBuf::from("synthetic/profile"), PathBuf::from("synthetic/linked")];
        let tracked = Mutex::new(folders.to_vec());
        let (signal, events) = mpsc::sync_channel(1);
        for folder in &folders {
            for path in [
                folder.clone(),
                folder.join("settings.json"),
                folder.join("settings.replacement"),
            ] {
                for kind in [
                    EventKind::Any,
                    EventKind::Create(CreateKind::Any),
                    EventKind::Modify(ModifyKind::Any),
                    EventKind::Modify(ModifyKind::Name(RenameMode::Any)),
                    EventKind::Remove(RemoveKind::Any),
                    EventKind::Other,
                ] {
                    signal_edit(Ok(notify::Event::new(kind).add_path(path.clone())), &tracked, &signal);
                    assert_eq!(events.try_recv(), Ok(()), "{kind:?}: {path:?}");
                }
            }
            // Backends may report either side first, or only one side at all.
            let settings = folder.join("settings.json");
            let outside = PathBuf::from("synthetic/outside/replacement");
            for paths in [[outside.clone(), settings.clone()], [settings, outside]] {
                signal_edit(
                    Ok(
                        notify::Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
                            .add_path(paths[0].clone())
                            .add_path(paths[1].clone()),
                    ),
                    &tracked,
                    &signal,
                );
                assert_eq!(events.try_recv(), Ok(()));
            }
        }
        signal_edit(
            Ok(notify::Event::new(EventKind::Any).add_path(PathBuf::from("synthetic/unwatched/settings.json"))),
            &tracked,
            &signal,
        );
        assert!(matches!(events.try_recv(), Err(mpsc::TryRecvError::Empty)));
    }

    #[test]
    fn first_run_watches_outside_edits_once_and_ignores_its_own_writes() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary folder: {error}"));
        let path = directory.path().join("new-profile/settings.json");
        let owner = Owner::open(path.clone());
        let (sender, edits) = mpsc::channel();
        let _watching = owner
            .watch(move |_, loaded| {
                let _ = sender.send(loaded);
            })
            .unwrap_or_else(|| panic!("the temporary folder can be watched"));
        assert!(!path.exists(), "watching first run creates no settings file");
        density(&owner, settings::Density::Compact).unwrap_or_else(|error| panic!("{error}"));
        // A write here is followed by an outside edit; only the edit is reported.
        density(&owner, settings::Density::Comfortable).unwrap_or_else(|error| panic!("{error}"));
        let mut edited = owner.read().settings;
        edited.larger_text = Some(true);
        replace(&path, &serde_json::to_vec(&edited).unwrap_or_default());
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
            .watch(move |_, loaded| {
                let _ = sender.send(loaded);
            })
            .unwrap_or_else(|| panic!("the temporary folder can be watched"));
        // Finish the opening comparison before simulating an editor's save.
        assert_eq!(owner.submit_on_file(|_| ()).wait(), Some(()));
        replace(&path, b"{ invented half-typed");
        assert!(next(&edits).problem.is_some());
        assert!(density(&owner, settings::Density::Comfortable).is_err());
        assert_eq!(std::fs::read(&path).unwrap_or_default(), b"{ invented half-typed");
        // Fixing the file is reported too, and changes work again.
        let fixed = Settings {
            density: Some(settings::Density::Comfortable),
            ..Settings::default()
        };
        replace(&path, &serde_json::to_vec(&fixed).unwrap_or_default());
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
    fn an_outside_edit_before_a_queued_save_is_reported_without_waiting_for_the_watch() {
        let (directory, owner) = owner();
        density(&owner, settings::Density::Compact).unwrap_or_else(|error| panic!("{error}"));
        let (sender, edits) = mpsc::channel();
        owner.watch.lock().unwrap_or_else(|error| panic!("{error}")).edited = Some(Arc::new(move |_, loaded| {
            let _ = sender.send(loaded);
        }));
        let path = directory.path().join("settings.json");
        std::fs::write(&path, br#"{"larger_text":true}"#).unwrap_or_else(|error| panic!("{error}"));
        density(&owner, settings::Density::Comfortable).unwrap_or_else(|error| panic!("{error}"));
        let reported = next(&edits);
        assert_eq!(reported.settings.larger_text, Some(true));
        assert_eq!(owner.read().settings.larger_text, Some(true));
    }

    #[test]
    fn an_edit_during_a_read_only_job_is_reported_before_a_later_save() {
        let (directory, owner) = owner();
        density(&owner, settings::Density::Compact).unwrap_or_else(|error| panic!("{error}"));
        let (sender, edits) = mpsc::channel();
        owner.watch.lock().unwrap_or_else(|error| panic!("{error}")).edited = Some(Arc::new(move |receipt, loaded| {
            let _ = sender.send((receipt, loaded));
        }));
        let path = directory.path().join("settings.json");
        owner.in_turn(|_| std::fs::write(&path, br#"{"larger_text":true}"#).unwrap_or_else(|error| panic!("{error}")));
        let (receipt, saved) =
            owner.submit_on_file_receipt(|path| settings::update(path, |s| s.menu_bar = Some(false)));
        assert!(saved.wait().is_some_and(|saved| saved.is_ok()));
        let (reported_receipt, loaded) = edits
            .recv_timeout(CHANNEL_LIMIT)
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(Some(reported_receipt) < receipt);
        assert_eq!(loaded.settings.larger_text, Some(true));
        assert_eq!(owner.read().settings.menu_bar, Some(false));
    }

    #[test]
    fn a_malformed_file_with_a_good_backup_is_reported_and_kept() {
        let (directory, owner) = owner();
        density(&owner, settings::Density::Compact).unwrap_or_else(|error| panic!("{error}"));
        density(&owner, settings::Density::Comfortable).unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("settings.json");
        std::fs::write(&path, b"{ editing").unwrap_or_else(|error| panic!("{error}"));
        assert!(owner.read().from_backup);
        assert!(density(&owner, settings::Density::Compact).is_err());
        assert_eq!(std::fs::read(&path).unwrap_or_default(), b"{ editing");
    }

    #[test]
    fn an_outside_edit_during_a_save_is_never_overwritten() {
        for outside in [br#"{"larger_text":true}"#.as_slice(), b"{ still editing"] {
            let (directory, owner) = owner();
            density(&owner, settings::Density::Compact).unwrap_or_else(|error| panic!("{error}"));
            let path = directory.path().join("settings.json");
            let saved: Result<(Settings, ()), SettingsError> = owner.change(|settings| {
                std::fs::write(&path, outside).unwrap_or_else(|error| panic!("{error}"));
                settings.menu_bar = Some(false);
                Ok(())
            });
            assert!(saved.is_err());
            assert_eq!(std::fs::read(&path).unwrap_or_default(), outside);
        }
    }

    #[test]
    #[cfg(unix)]
    fn a_linked_file_is_live_and_rebinding_keeps_watching_its_new_target() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let first_folder = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let second_folder = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let first = first_folder.path().join("settings.json");
        let second = second_folder.path().join("settings.json");
        std::fs::write(&first, b"{}").unwrap_or_else(|error| panic!("{error}"));
        std::fs::write(&second, b"{}").unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("settings.json");
        std::os::unix::fs::symlink(&first, &path).unwrap_or_else(|error| panic!("{error}"));
        // The pre-existing link is the baseline, rather than an outside edit.
        let owner = Owner::open(path.clone());
        let (sender, edits) = mpsc::channel();
        let _watching = owner
            .watch(move |_, loaded| {
                let _ = sender.send(loaded);
            })
            .unwrap_or_else(|| panic!("watch"));
        assert_eq!(owner.submit_on_file(|_| ()).wait(), Some(()));
        // Replacing the inode must keep observing this filename.
        replace(&first, br#"{"larger_text":true}"#);
        assert_eq!(next(&edits).settings.larger_text, Some(true));
        let replacement = directory.path().join("replacement");
        std::os::unix::fs::symlink(&second, &replacement).unwrap_or_else(|error| panic!("{error}"));
        std::fs::rename(&replacement, &path).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(next(&edits).settings.larger_text, None);
        replace(&second, br#"{"menu_bar":false}"#);
        assert_eq!(next(&edits).settings.menu_bar, Some(false));
        owner.close();
        assert!(density(&owner, settings::Density::Compact).is_err());
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
