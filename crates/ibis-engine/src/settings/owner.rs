//! One queue owns every settings operation and every outside-edit report.
use crate::sync::MutexExt;
use crate::sync::attempt;
use std::cell::RefCell;
use std::collections::BTreeSet;
#[cfg(test)]
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use super::{Loaded, Settings, SettingsError};
use crate::settings;

type Work = Box<dyn FnOnce(&Path, u64) -> Completion + Send>;
type Reply = Box<dyn FnOnce() + Send>;
#[derive(Default)]
struct Completion {
    written: Vec<Commit>,
    publish: Option<Box<dyn FnOnce() -> Reply + Send>>,
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
type Published = Arc<dyn Fn(&Loaded) + Send + Sync>;
#[derive(Default)]
struct WatchState {
    edited: Option<Edited>,
    watcher: Option<notify::RecommendedWatcher>,
    /// The file as the queue last saw it; see [`Owner::view`].
    view: Option<Arc<Loaded>>,
    published: Option<Published>,
}
fn publish(path: &Path, watch: &Mutex<WatchState>) {
    let view = Arc::new(settings::load(path));
    let published = {
        let mut state = watch.recover_lock();
        if state.view.as_ref().is_some_and(|old| {
            old.settings == view.settings
                && old.primary_read == view.primary_read
                && old.from_backup == view.from_backup
                && old.problem.as_ref().map(ToString::to_string) == view.problem.as_ref().map(ToString::to_string)
        }) {
            return;
        }
        state.view = Some(Arc::clone(&view));
        state.published.clone()
    };
    // Outside the lock: a listener may read the view again.
    if let Some(published) = published {
        published(&view);
    }
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

// Keep each successful write's input as well as its output: an editor may have
// changed the input after the turn's initial observation. Replay in order so
// several transactions in one turn do not report each other's writes as edits.
struct Commit {
    before: settings::Snapshot,
    bytes: Vec<u8>,
}
thread_local! { static COMMITTED: RefCell<Option<Vec<Commit>>> = const { RefCell::new(None) }; }
pub(super) fn committed(before: settings::Snapshot, bytes: Vec<u8>) {
    COMMITTED.with(|slot| {
        if let Some(written) = slot.borrow_mut().as_mut() {
            written.push(Commit { before, bytes });
        }
    });
}
fn tracking<T>(work: impl FnOnce() -> T) -> (T, Vec<Commit>) {
    struct Restore(Option<Vec<Commit>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            COMMITTED.with(|slot| {
                slot.replace(self.0.take());
            });
        }
    }
    let restore = Restore(COMMITTED.with(|slot| slot.replace(Some(Vec::new()))));
    let result = work();
    let written = COMMITTED.with(|slot| slot.borrow_mut().as_mut().map(std::mem::take).unwrap_or_default());
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
        let mut state = self.state.recover_lock();
        state.edited = None;
        state.watcher = None;
    }
}
fn watch_edits(mut receive: impl FnMut(Option<Duration>) -> bool, mut changed: impl FnMut() -> bool) {
    while receive(None) {
        // Coalesce hints already waiting, but never wait for silence: a person
        // may press a restored shortcut immediately after their editor saves.
        while receive(Some(Duration::ZERO)) {}
        if !changed() {
            return;
        }
    }
}

#[must_use = "a change's outcome says whether it was saved"]
pub struct Pending<T>(Receiver<T>);
impl<T> Pending<T> {
    pub fn wait(self) -> Option<T> {
        self.0.recv().ok()
    }
}

fn report(path: &Path, receipt: u64, watch: &Mutex<WatchState>, primary: std::io::Result<Vec<u8>>) {
    // Windows told of an outside edit read the view, so it shows the edit first.
    publish(path, watch);
    let edited = watch.recover_lock().edited.clone();
    if let Some(edited) = edited {
        edited(receipt, settings::load_bytes(path, primary));
    }
}

fn observe(path: &Path, receipt: u64, watch: &Mutex<WatchState>, seen: &mut Fingerprint) {
    observe_read(path, receipt, watch, seen, settings::read_regular_bytes(path));
}
fn observe_read(
    path: &Path,
    receipt: u64,
    watch: &Mutex<WatchState>,
    seen: &mut Fingerprint,
    primary: std::io::Result<Vec<u8>>,
) {
    match (&*seen, &primary) {
        (Fingerprint::Bytes(previous), Ok(bytes)) if previous == bytes => return,
        (Fingerprint::Failed(previous), Err(error)) if *previous == error.kind() => return,
        _ => {}
    }
    *seen = match &primary {
        Ok(bytes) => Fingerprint::Bytes(bytes.clone()),
        Err(error) => Fingerprint::Failed(error.kind()),
    };
    let _ = attempt(|| report(path, receipt, watch, primary));
}
fn enqueue(queue: &Mutex<Queue>, work: Work) -> Option<u64> {
    let mut queue = queue.recover_lock();
    // Leave a slot before and after each job for reports found in its turn.
    let receipt = queue.next.checked_add(4).filter(|value| *value < (1 << 53) - 1)?;
    queue.sender.as_ref()?.send(Job { receipt, work }).ok()?;
    queue.next = receipt;
    Some(receipt)
}

fn signal_edit(
    event: notify::Result<notify::Event>,
    tracked: &Mutex<BTreeSet<PathBuf>>,
    signal: &mpsc::SyncSender<()>,
) {
    if event.is_ok_and(|event| {
        // Inotify reports our own reads as opens. Treating them as edits would
        // make every fingerprint schedule another fingerprint indefinitely.
        (!matches!(event.kind, notify::EventKind::Access(_))
            || matches!(
                event.kind,
                notify::EventKind::Access(notify::event::AccessKind::Close(notify::event::AccessMode::Write))
            ))
            && (event.need_rescan() || {
                let files = tracked.recover_lock();
                event.paths.iter().any(|path| files.contains(path))
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
        publish(&path, &watch);
        let watching = Arc::clone(&watch);
        let mut seen = fingerprint(&path);
        let worker = std::thread::Builder::new()
            .name("ibis-settings".into())
            .spawn(move || {
                for job in jobs {
                    observe(&file, job.receipt - 1, &watching, &mut seen);
                    let completed = attempt(|| (job.work)(&file, job.receipt)).unwrap_or_default();
                    for commit in completed.written {
                        observe_read(
                            &file,
                            job.receipt - 1,
                            &watching,
                            &mut seen,
                            commit.before.map_err(std::io::Error::from),
                        );
                        seen = Fingerprint::Bytes(commit.bytes);
                    }
                    // Effects that run with the outcome read the saved file.
                    publish(&file, &watching);
                    let reply = completed.publish.and_then(|publish| attempt(publish).ok());
                    observe(&file, job.receipt + 1, &watching, &mut seen);
                    // Finish the whole turn before returning its outcome. The
                    // caller's next edit must not race this turn's final read.
                    if let Some(reply) = reply {
                        let _ = attempt(reply);
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
    /// Watch the settings and backup names, including their symlink targets.
    pub fn watch(&self, edited: impl Fn(u64, Loaded) + Send + Sync + 'static) -> Option<Watching> {
        use notify::Watcher;
        let folder = self.path.parent()?.to_path_buf();
        // A new profile has no file yet. Its folder must exist before native
        // watching starts, so the first editor save is observed too.
        std::fs::create_dir_all(&folder).ok()?;
        // FSEvents reports canonical paths, including when the file is absent.
        // Resolve the directory without following the settings symlink itself.
        let folder = std::fs::canonicalize(folder).ok()?;
        let path = folder.join(self.path.file_name()?);
        let files = [settings::backup_path(&path), path];
        let tracked = Arc::new(Mutex::new(files.iter().cloned().collect()));
        let relevant = Arc::clone(&tracked);
        let (signal, events) = mpsc::sync_channel::<()>(1);
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            signal_edit(event, &relevant, &signal);
        })
        .ok()?;
        watcher.watch(&folder, notify::RecursiveMode::NonRecursive).ok()?;
        let mut target_folders = BTreeSet::new();
        rebind(&files, &folder, &mut target_folders, &mut watcher, &tracked);
        {
            let mut state = self.watch.recover_lock();
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
                watch_edits(
                    |timeout| match timeout {
                        Some(timeout) => events.recv_timeout(timeout).is_ok(),
                        None => events.recv().is_ok(),
                    },
                    || {
                        let mut state = state.recover_lock();
                        let Some(watcher) = state.watcher.as_mut() else {
                            return false;
                        };
                        rebind(&files, &folder, &mut target_folders, watcher, &tracked);
                        enqueue(&jobs, Box::new(|_, _| Completion::default())).is_some()
                    },
                );
            })
            .ok()?;
        Some(Watching {
            state: Arc::clone(&self.watch),
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Call `published` when a turn or an outside edit changes the loaded view.
    /// One listener; a later call replaces it, and closing drops it.
    pub fn on_publish(&self, published: impl Fn(&Loaded) + Send + Sync + 'static) {
        self.watch.recover_lock().published = Some(Arc::new(published));
    }
    /// The settings as the last finished turn left them, without waiting for
    /// queued work. Use it wherever a caller must not stall on a write.
    pub fn view(&self) -> Arc<Loaded> {
        let view = self.watch.recover_lock().view.clone();
        view.unwrap_or_else(|| Arc::new(settings::load(&self.path)))
    }
    pub fn read(&self) -> Loaded {
        self.in_turn(|path| settings::load(path.unwrap_or(&self.path)))
    }
    /// Read and apply read effects in order, without saving or repairing a file.
    pub fn read_with<T>(&self, read: impl FnOnce(Loaded) -> T) -> Option<T> {
        self.in_turn(|path| path.ok().map(|path| read(settings::load(path))))
    }
    pub fn submit<T, E>(
        &self,
        change: impl FnOnce(&mut Settings) -> Result<T, E> + Send + 'static,
    ) -> Pending<Result<(Settings, T), E>>
    where
        T: Send + 'static,
        E: From<SettingsError> + Send + 'static,
    {
        self.submit_with_receipt(change, |_, result| result).1
    }
    /// Publish a save's effects after its input edits and before its final
    /// observation; return the outcome only once the whole turn is finished.
    pub fn submit_with_receipt<T, E, U>(
        &self,
        change: impl FnOnce(&mut Settings) -> Result<T, E> + Send + 'static,
        after: impl FnOnce(u64, Result<(Settings, T), E>) -> U + Send + 'static,
    ) -> (Option<u64>, Pending<U>)
    where
        T: Send + 'static,
        E: From<SettingsError> + Send + 'static,
        U: Send + 'static,
    {
        let (reply, outcome) = mpsc::channel();
        let receipt = enqueue(
            &self.jobs,
            Box::new(move |path, receipt| {
                let (result, written) = tracking(|| settings::transaction(path, change));
                Completion {
                    written,
                    publish: Some(Box::new(move || {
                        let result = after(receipt, result);
                        Box::new(move || {
                            let _ = reply.send(result);
                        })
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
        self.change_with(change, |result| result)
    }
    /// Run effects of a completed edit before observing later outside changes.
    pub fn change_with<T, E: From<SettingsError>, U>(
        &self,
        change: impl FnOnce(&mut Settings) -> Result<T, E>,
        after: impl FnOnce(Result<(Settings, T), E>) -> U,
    ) -> U {
        self.in_turn(|path| {
            after(
                path.map_err(E::from)
                    .and_then(|path| settings::transaction(path, change)),
            )
        })
    }
    /// Prepare a file for the external editor, leaving existing contents alone.
    pub fn ensure_file(&self) -> Result<(), SettingsError> {
        self.in_turn(|path| path.and_then(settings::ensure_file))
    }

    /// Restores exactly the previewed settings, preserving this device's state.
    pub fn restore(&self, plan: &super::backup::Plan) -> Result<Settings, super::backup::BackupError> {
        self.in_turn(|path| {
            path.map_err(super::backup::BackupError::from)
                .and_then(|path| super::backup::restore(path, plan))
        })
        .map_err(|error| match error {
            super::backup::BackupError::Settings(problem) => super::backup::BackupError::Current(problem),
            other => other,
        })
    }

    /// Run `work` in its own turn. One that has not started within
    /// [`TURN_LIMIT`], as behind a read that stopped responding, fails instead:
    /// its work never runs, so nothing it would have written lands later.
    fn in_turn<T>(&self, work: impl FnOnce(Result<&Path, SettingsError>) -> T) -> T {
        let (start, turn) = mpsc::channel::<()>();
        let (finished, done) = mpsc::channel::<Vec<Commit>>();
        let (settled, completion) = mpsc::channel::<()>();
        let queued = enqueue(
            &self.jobs,
            Box::new(move |_, _| {
                let _ = start.send(());
                Completion {
                    written: done.recv().unwrap_or_default(),
                    publish: Some(Box::new(move || {
                        Box::new(move || {
                            let _ = settled.send(());
                        })
                    })),
                }
            }),
        );
        if queued.is_none() {
            return work(Err(closed()));
        }
        if let Err(waited) = turn.recv_timeout(TURN_LIMIT) {
            return work(Err(match waited {
                mpsc::RecvTimeoutError::Timeout => busy(),
                mpsc::RecvTimeoutError::Disconnected => closed(),
            }));
        }
        let (result, written) = tracking(|| work(Ok(&self.path)));
        let _ = finished.send(written);
        let _ = completion.recv();
        result
    }
    pub fn close(&self) {
        {
            let mut jobs = self.jobs.recover_lock();
            jobs.sender.take();
        }
        if let Some(worker) = self.worker.recover_lock().take() {
            let _ = worker.join();
        }
        {
            let mut state = self.watch.recover_lock();
            state.edited = None;
            state.watcher = None;
            state.published = None;
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
/// How long a turn may wait for the ones before it. Settings work takes
/// milliseconds; this only ends a wait that would otherwise never end.
const TURN_LIMIT: Duration = Duration::from_secs(3);
fn busy() -> SettingsError {
    SettingsError::Unreadable(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        "settings did not answer in time",
    ))
}
fn rebind(
    files: &[PathBuf],
    folder: &Path,
    target_folders: &mut BTreeSet<PathBuf>,
    watcher: &mut notify::RecommendedWatcher,
    tracked: &Mutex<BTreeSet<PathBuf>>,
) {
    use notify::Watcher;
    let targets: Vec<_> = files
        .iter()
        .filter_map(|path| {
            std::fs::canonicalize(path).ok().or_else(|| {
                // Keep observing a symlink's target name while an editor replaces it.
                let linked = path.parent()?.join(std::fs::read_link(path).ok()?);
                Some(std::fs::canonicalize(linked.parent()?).ok()?.join(linked.file_name()?))
            })
        })
        .collect();
    let desired: BTreeSet<_> = targets
        .iter()
        .filter_map(|target| target.parent())
        .filter(|parent| *parent != folder)
        .map(Path::to_path_buf)
        .collect();
    // Publish the exact names before registering the watches that can report them.
    *tracked.recover_lock() = files.iter().cloned().chain(targets).collect();
    for previous in target_folders.difference(&desired) {
        let _ = watcher.unwatch(previous);
    }
    target_folders.retain(|previous| desired.contains(previous));
    for added in desired {
        if !target_folders.contains(&added) && watcher.watch(&added, notify::RecursiveMode::NonRecursive).is_ok() {
            target_folders.insert(added);
        }
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
            s.portable.density = Some(density);
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
    fn a_choice_behind_a_stalled_read_fails_in_bounded_time_and_never_lands_later() {
        let (directory, owner) = owner();
        density(&owner, settings::Density::Compact).unwrap_or_else(|error| panic!("{error}"));
        let owner = Arc::new(owner);
        // A read that stops responding holds its turn.
        let (release, released) = mpsc::channel::<()>();
        let (held, holding) = mpsc::channel();
        let stalled = std::thread::spawn({
            let owner = Arc::clone(&owner);
            move || {
                owner.read_with(|_| {
                    let _ = held.send(());
                    let _ = released.recv();
                })
            }
        });
        holding
            .recv_timeout(CHANNEL_LIMIT)
            .unwrap_or_else(|error| panic!("the read never started: {error}"));

        // A format choice made meanwhile is refused instead of waiting.
        let (answer, answered) = mpsc::channel();
        std::thread::spawn({
            let owner = Arc::clone(&owner);
            move || {
                let _ = answer.send(density(&owner, settings::Density::Comfortable).is_err());
            }
        });
        let refused = answered.recv_timeout(CHANNEL_LIMIT);
        let _ = release.send(());
        let _ = stalled.join();
        assert_eq!(refused, Ok(true), "the choice waited for the stalled read");

        // Released, the old turn lets later work run, and the refused choice
        // never lands over what is saved now.
        assert_eq!(owner.read().settings.portable.density, Some(settings::Density::Compact));
        let saved = settings::load(&directory.path().join("settings.json"));
        assert_eq!(saved.settings.portable.density, Some(settings::Density::Compact));
        density(&owner, settings::Density::Comfortable).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            owner.read().settings.portable.density,
            Some(settings::Density::Comfortable)
        );
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
        assert_eq!(
            owner.submit(|_| Ok::<_, SettingsError>(())).wait().map(|_| ()),
            Some(())
        );
        replace(&path, b"{ invented half-typed");
        assert!(next(&edits).problem.is_some());
        assert!(density(&owner, settings::Density::Comfortable).is_err());
        assert_eq!(std::fs::read(&path).unwrap_or_default(), b"{ invented half-typed");
        // Fixing the file is reported too, and changes work again.
        let fixed = Settings {
            portable: crate::settings::Portable {
                density: Some(settings::Density::Comfortable),
                ..Default::default()
            },
            ..Default::default()
        };
        replace(&path, &serde_json::to_vec(&fixed).unwrap_or_default());
        let reported = next(&edits);
        assert!(reported.problem.is_none());
        assert_eq!(reported.settings.portable.density, Some(settings::Density::Comfortable));
        owner.close();
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
    fn only_changed_views_are_published_to_the_listener() {
        let (directory, owner) = owner();
        let (sender, published) = mpsc::channel();
        owner.on_publish(move |loaded| {
            let _ = sender.send(loaded.settings.portable.paste);
        });
        let seen = || {
            published
                .recv_timeout(CHANNEL_LIMIT)
                .unwrap_or_else(|error| panic!("nothing published: {error}"))
        };
        let paste = |value| {
            owner
                .change(move |s| {
                    s.portable.paste = Some(value);
                    Ok::<_, SettingsError>(())
                })
                .unwrap_or_else(|error| panic!("{error}"))
        };
        paste(false);
        assert_eq!(seen(), Some(false), "an own write");
        let _ = owner.read();
        assert!(
            published.try_recv().is_err(),
            "a read must not republish an unchanged view"
        );
        // An outside edit is observed at the start of the next turn.
        replace(&directory.path().join("settings.json"), br#"{"paste":true}"#);
        assert_eq!(owner.read().settings.portable.paste, Some(true));
        let mut views = Vec::new();
        while let Ok(view) = published.try_recv() {
            views.push(view);
        }
        assert!(views.contains(&Some(true)), "the outside edit: {views:?}");
        owner.close();
        let _ = density(&owner, settings::Density::Compact);
        assert!(published.try_recv().is_err(), "a closed owner publishes nothing");
    }

    #[test]
    fn an_outside_edit_during_a_save_is_never_overwritten() {
        for outside in [br#"{"larger_text":true}"#.as_slice(), b"{ still editing"] {
            let (directory, owner) = owner();
            density(&owner, settings::Density::Compact).unwrap_or_else(|error| panic!("{error}"));
            let path = directory.path().join("settings.json");
            let saved: Result<(Settings, ()), SettingsError> = owner.change(|settings| {
                std::fs::write(&path, outside).unwrap_or_else(|error| panic!("{error}"));
                settings.portable.menu_bar = Some(false);
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
        assert_eq!(
            owner.submit(|_| Ok::<_, SettingsError>(())).wait().map(|_| ()),
            Some(())
        );
        // Replacing the inode must keep observing this filename.
        replace(&first, br#"{"larger_text":true}"#);
        assert_eq!(next(&edits).settings.portable.larger_text, Some(true));
        let replacement = directory.path().join("replacement");
        std::os::unix::fs::symlink(&second, &replacement).unwrap_or_else(|error| panic!("{error}"));
        std::fs::rename(&replacement, &path).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(next(&edits).settings.portable.larger_text, None);
        replace(&second, br#"{"menu_bar":false}"#);
        assert_eq!(next(&edits).settings.portable.menu_bar, Some(false));
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
    fn the_view_answers_while_a_change_is_held_and_shows_each_save() {
        let (_directory, owner) = owner();
        density(&owner, settings::Density::Compact).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(owner.view().settings.portable.density, Some(settings::Density::Compact));
        let (started, running) = mpsc::channel();
        let (release, held) = mpsc::channel::<()>();
        let slow = owner.submit(move |s: &mut Settings| -> Result<(), SettingsError> {
            let _ = started.send(());
            let _ = held.recv_timeout(CHANNEL_LIMIT);
            s.portable.density = Some(settings::Density::Comfortable);
            Ok(())
        });
        running
            .recv_timeout(CHANNEL_LIMIT)
            .unwrap_or_else(|error| panic!("the slow change starts: {error}"));
        assert_eq!(
            owner.view().settings.portable.density,
            Some(settings::Density::Compact),
            "a held write never blocks the view"
        );
        let _ = release.send(());
        assert!(slow.wait().is_some_and(|saved| saved.is_ok()));
        assert_eq!(
            owner.view().settings.portable.density,
            Some(settings::Density::Comfortable)
        );
    }

    #[test]
    fn a_change_in_turn_waits_for_earlier_ones_and_holds_back_later_ones() {
        let (_directory, owner) = owner();
        let (release_earlier, earlier_held) = mpsc::channel();
        let earlier = owner.submit(move |s: &mut Settings| -> Result<(), SettingsError> {
            earlier_held
                .recv_timeout(CHANNEL_LIMIT)
                .unwrap_or_else(|error| panic!("the test releases the earlier choice: {error}"));
            s.portable.density = Some(settings::Density::Compact);
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
                        seen.set(s.portable.density);
                        entered
                            .send(())
                            .unwrap_or_else(|error| panic!("the test is waiting for the turn: {error}"));
                        turn_held
                            .recv_timeout(CHANNEL_LIMIT)
                            .unwrap_or_else(|error| panic!("the test releases the turn: {error}"));
                        s.portable.density = Some(settings::Density::Comfortable);
                        Ok::<_, SettingsError>(())
                    })
                    .unwrap_or_else(|error| panic!("{error}"));
                (seen.get(), saved.portable.density)
            });
            release_earlier
                .send(())
                .unwrap_or_else(|error| panic!("the earlier choice is held: {error}"));
            in_turn
                .recv_timeout(CHANNEL_LIMIT)
                .unwrap_or_else(|error| panic!("the next turn starts: {error}"));
            let later = owner.submit(|settings| Ok::<_, SettingsError>(settings.portable.density));
            release_turn
                .send(())
                .unwrap_or_else(|error| panic!("the turn is held: {error}"));
            assert_eq!(
                turn.join().unwrap_or_else(|_| panic!("the turn panicked")),
                (Some(settings::Density::Compact), Some(settings::Density::Comfortable))
            );
            assert_eq!(
                later.wait().and_then(Result::ok).and_then(|(_, density)| density),
                Some(settings::Density::Comfortable)
            );
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
            s.portable.density = Some(settings::Density::Compact);
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
                .portable
                .density,
            Some(settings::Density::Compact)
        );
    }
}
