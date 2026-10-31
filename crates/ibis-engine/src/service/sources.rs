//! Source handles stay outside Shared: callbacks upgrade only publication state.
use super::*;
use ibis_sources::zotero_source::{Library, LiveError, ZoteroError, ZoteroSource};
use ibis_sources::{LoadedLibrary, SourceError, SourceWatch};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
const MAX_FILES: usize = 64;
const MAX_WARNING_DETAILS: usize = 256;
#[derive(Default)]
pub(super) struct Owners {
    operation: Mutex<()>,
    watches: Mutex<BTreeMap<u64, SourceWatch>>,
    zotero: Mutex<ZoteroSource>,
    zotero_port: Option<u16>,
    #[cfg(test)]
    fail_watches: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    before_watch: Mutex<Option<Box<dyn Fn() + Send>>>,
}
impl Owners {
    pub(super) fn on_port(port: u16) -> Self {
        Self {
            zotero_port: Some(port),
            ..Self::default()
        }
    }
}
#[derive(Default)]
pub(super) struct Registry {
    next: u64,
    slots: BTreeMap<u64, Slot>,
}
struct Slot {
    generation: u64,
    path: PathBuf,
    loaded: Arc<LoadedLibrary>,
}
/// A strict read outcome prepared privately, including source-health failures.
/// No publication occurs until the client accepts it under its selection guard.
pub struct PreparedFileReload {
    origin: Weak<Shared>,
    base: Arc<State>,
    slot: u64,
    generation: u64,
    path: PathBuf,
    read: Result<FileReloadRead, EngineError>,
    cancel: Cancellation,
}
struct FileReloadRead {
    loaded: LoadedLibrary,
    observation: ibis_sources::SourceSnapshot,
    next: State,
    watch: Option<SourceWatch>,
    activation: mpsc::Sender<()>,
}
impl PreparedFileReload {
    pub fn source_path(&self) -> Option<&Path> {
        self.read.as_ref().ok().map(|read| read.observation.source_path())
    }
}

pub(super) fn source_error(error: SourceError) -> EngineError {
    super::error(match error {
        SourceError::TooLarge | SourceError::LoadLimit => ErrorCode::ResourceLimit,
        SourceError::NotRegular | SourceError::Unreadable | SourceError::Incomplete | SourceError::InvalidData => {
            ErrorCode::BadInput
        }
        SourceError::Unavailable | SourceError::Changing | SourceError::WatchUnavailable => {
            ErrorCode::SourceUnavailable
        }
    })
}
fn info(loaded: &LoadedLibrary) -> LibraryInfo {
    LibraryInfo {
        id: loaded.library.clone(),
        objects: loaded.snapshot.objects().len(),
        warnings: loaded.warnings.iter().take(MAX_WARNING_DETAILS).cloned().collect(),
        warnings_total: loaded.warnings.len(),
        refresh_error: None,
        updated_at_ms: now_ms(),
        watch_warning: None,
        zotero: None,
    }
}
impl Engine {
    pub(super) fn open_file(&self, path: &Path, cancel: &Cancellation) -> Result<(), EngineError> {
        self.open_file_observed(path, cancel).map(|_| ())
    }

    /// Return the canonical identity from the observation that supplied the bytes.
    pub fn open_file_observed(&self, path: &Path, cancel: &Cancellation) -> Result<PathBuf, EngineError> {
        cancel.check()?;
        let operation = self
            .resources
            .operation
            .lock()
            .map_err(|_| error(ErrorCode::Internal))?;
        let path = std::path::absolute(path).map_err(|_| error(ErrorCode::BadInput))?;
        let (loaded, observation) = ibis_sources::load_file_snapshot(&path).map_err(source_error)?;
        let source = observation.source_path().to_path_buf();
        cancel.check()?;
        self.install_file(path, loaded, Some(observation), cancel)?;
        drop(operation);
        Ok(source)
    }
    /// Prepare the read and watch without publishing. Source failures are also
    /// carried to acceptance, so superseded requests cannot change source health.
    pub fn prepare_file_reload(&self, path: &Path, cancel: &Cancellation) -> Result<PreparedFileReload, EngineError> {
        self.prepare_file_reload_expected(path, None, cancel)
    }

    fn prepare_file_reload_expected(
        &self,
        path: &Path,
        expected: Option<(u64, u64)>,
        cancel: &Cancellation,
    ) -> Result<PreparedFileReload, EngineError> {
        cancel.check()?;
        let path = std::path::absolute(path).map_err(|_| error(ErrorCode::BadInput))?;
        let (slot, generation, base, previous, other_owners) = {
            let registry = self.shared.sources.lock().map_err(|_| error(ErrorCode::Internal))?;
            let (&id, slot) = registry
                .slots
                .iter()
                .find(|(_, slot)| slot.path == path)
                .ok_or_else(|| error(ErrorCode::NotFound))?;
            if expected.is_some_and(|expected| expected != (id, slot.generation)) {
                return Err(error(ErrorCode::Cancelled));
            }
            (
                id,
                slot.generation,
                self.state()?,
                slot.loaded.library.clone(),
                registry
                    .slots
                    .iter()
                    .any(|(other, other_slot)| *other != id && other_slot.loaded.library == slot.loaded.library),
            )
        };
        let read = (|| {
            let (loaded, observation) =
                ibis_sources::reload_file_snapshot_reusing(&path, base.index.snapshot()).map_err(source_error)?;
            cancel.check()?;
            let library = loaded.library.clone();
            let mut next = State::replacement(&base, vec![(info(&loaded), &loaded.snapshot)], |id| {
                *id == library || (!other_owners && *id == previous)
            })?;
            // Canonicalization and native registration can stall. Keep them off
            // the selection lock; callbacks wait for this exact acceptance.
            let (activation, callback) = self.reload_watch_callback(slot, generation.wrapping_add(1));
            let (watch, warning) = if self.watch_files {
                match self.watch(&path, Some(observation.clone()), loaded.snapshot.clone(), callback) {
                    Ok(watch) => (Some(watch), None),
                    Err(_) => (None, Some(WatchWarning::LiveRefreshOff)),
                }
            } else {
                (None, None)
            };
            next.libraries
                .get_mut(&library)
                .ok_or_else(|| error(ErrorCode::Internal))?
                .watch_warning = warning;
            Ok(FileReloadRead {
                loaded,
                observation,
                next,
                watch,
                activation,
            })
        })();
        cancel.check()?;
        Ok(PreparedFileReload {
            origin: Arc::downgrade(&self.shared),
            base,
            slot,
            generation,
            path,
            read,
            cancel: cancel.clone(),
        })
    }

    /// The caller holds its selection guard. Parsing/index/watch setup is done;
    /// any newer accepted publication retires this candidate, including watcher R2.
    pub fn accept_file_reload(&self, prepared: PreparedFileReload) -> Result<(), EngineError> {
        prepared.cancel.check()?;
        if !Weak::ptr_eq(&prepared.origin, &Arc::downgrade(&self.shared)) {
            return Err(error(ErrorCode::BadInput));
        }
        let _operation = self
            .resources
            .operation
            .lock()
            .map_err(|_| error(ErrorCode::Internal))?;
        let mut registry = self.shared.sources.lock().map_err(|_| error(ErrorCode::Internal))?;
        let slot = registry
            .slots
            .get(&prepared.slot)
            .ok_or_else(|| error(ErrorCode::Cancelled))?;
        if slot.generation != prepared.generation || slot.path != prepared.path {
            return Err(error(ErrorCode::Cancelled));
        }
        let _publication = self.shared.publication.lock().map_err(|_| error(ErrorCode::Internal))?;
        let generation = slot.generation.wrapping_add(1);
        let id = prepared.slot;
        prepared.cancel.check()?;
        // Recheck under the state write barrier: direct publishers need no source lock.
        let mut state = self.shared.state.write().map_err(|_| error(ErrorCode::Internal))?;
        if !prepared.base.same_publication(&state) {
            return Err(error(ErrorCode::Cancelled));
        }
        let mut read = match prepared.read {
            Ok(read) => read,
            Err(failure) => {
                *state = Arc::new(state.with_failure(&slot.loaded.library, failure.code)?);
                drop(state);
                self.shared.notify()?;
                return Err(failure);
            }
        };
        // Failed reads may update health while parsing/indexing. A fresh repair
        // clears its own error, while unrelated libraries keep their latest one.
        for (id, info) in &mut read.next.libraries {
            if *id != read.loaded.library
                && let Some(current) = state.libraries.get(id)
            {
                info.refresh_error = current.refresh_error;
            }
        }
        *state = Arc::new(read.next);
        registry.slots.insert(
            id,
            Slot {
                generation,
                path: prepared.path,
                loaded: Arc::new(read.loaded),
            },
        );
        drop(state);
        drop(registry);
        let old = {
            let mut watches = self.resources.watches.lock().map_err(|_| error(ErrorCode::Internal))?;
            match read.watch {
                Some(watch) => watches.insert(id, watch),
                None => watches.remove(&id),
            }
        };
        // Each candidate has its own one-shot gate, so an abandoned watch with
        // the same next slot generation can never publish another read's data.
        let _ = read.activation.send(());
        drop(old);
        self.shared.notify()?;
        Ok(())
    }

    fn reload_watch_callback(
        &self,
        id: u64,
        generation: u64,
    ) -> (
        mpsc::Sender<()>,
        impl Fn(Result<LoadedLibrary, SourceError>) -> Option<Snapshot> + Send + Sync + 'static,
    ) {
        let (activation, accepted) = mpsc::channel();
        let accepted = Mutex::new(accepted);
        let active = std::sync::OnceLock::new();
        let weak = Arc::downgrade(&self.shared);
        let callback = move |candidate| {
            if !*active.get_or_init(|| accepted.lock().is_ok_and(|accepted| accepted.recv().is_ok())) {
                return None;
            }
            weak.upgrade()
                .and_then(|shared| shared.file_changed(id, generation, candidate))
        };
        (activation, callback)
    }

    pub(super) fn install_file(
        &self,
        path: PathBuf,
        mut loaded: LoadedLibrary,
        observation: Option<ibis_sources::SourceSnapshot>,
        cancel: &Cancellation,
    ) -> Result<(), EngineError> {
        loaded.snapshot.reuse_unchanged(self.snapshot()?.as_ref());
        let mut registry = self.shared.sources.lock().map_err(|_| error(ErrorCode::Internal))?;
        let existing = registry
            .slots
            .iter()
            .find(|(_, slot)| slot.path == path)
            .or_else(|| {
                registry
                    .slots
                    .iter()
                    .find(|(_, slot)| slot.loaded.library == loaded.library)
            })
            .map(|(id, slot)| (*id, slot.generation, slot.loaded.library.clone()));
        if existing.is_none() && registry.slots.len() >= MAX_FILES {
            return Err(error(ErrorCode::ResourceLimit));
        }
        let id = existing.as_ref().map(|entry| entry.0).unwrap_or(registry.next);
        let generation = existing.as_ref().map_or(0, |entry| entry.1.wrapping_add(1));
        let weak = Arc::downgrade(&self.shared);
        let callback = move |candidate| {
            weak.upgrade()
                .and_then(|shared| shared.file_changed(id, generation, candidate))
        };
        let (watch, watch_warning) = if self.watch_files {
            match self.watch(&path, observation, loaded.snapshot.clone(), callback) {
                Ok(watch) => (Some(watch), None),
                Err(_) => (None, Some(WatchWarning::LiveRefreshOff)),
            }
        } else {
            (None, None)
        };
        let previous = existing.as_ref().map(|entry| entry.2.clone()).filter(|old| {
            *old != loaded.library
                && !registry
                    .slots
                    .iter()
                    .any(|(other, slot)| *other != id && slot.loaded.library == *old)
        });
        let mut library_info = info(&loaded);
        library_info.watch_warning = watch_warning;
        cancel.check()?;
        if let Err(error) = self.shared.install(library_info, &loaded.snapshot, previous) {
            drop(registry);
            drop(watch);
            return Err(error);
        }
        registry.next = registry.next.max(id.wrapping_add(1));
        registry.slots.insert(
            id,
            Slot {
                generation,
                path,
                loaded: Arc::new(loaded),
            },
        );
        drop(registry);
        let old = {
            let mut watches = self.resources.watches.lock().map_err(|_| error(ErrorCode::Internal))?;
            match watch {
                Some(watch) => watches.insert(id, watch),
                None => watches.remove(&id),
            }
        };
        drop(old);
        cancel.check()
    }
    fn watch(
        &self,
        path: &Path,
        observation: Option<ibis_sources::SourceSnapshot>,
        model: Snapshot,
        callback: impl Fn(Result<LoadedLibrary, SourceError>) -> Option<Snapshot> + Send + Sync + 'static,
    ) -> Result<SourceWatch, SourceError> {
        #[cfg(test)]
        if let Some(before_watch) = self
            .resources
            .before_watch
            .lock()
            .map_err(|_| SourceError::WatchUnavailable)?
            .as_ref()
        {
            before_watch();
        }
        #[cfg(test)]
        if self.resources.fail_watches.load(Ordering::Relaxed) {
            return Err(SourceError::WatchUnavailable);
        }
        SourceWatch::with_library(path, observation, model, callback)
    }
    pub(super) fn reload_files(&self, library: Option<&LibraryId>, cancel: &Cancellation) -> Result<(), EngineError> {
        let loaded_zotero = self
            .resources
            .zotero
            .lock()
            .map_err(|_| error(ErrorCode::Internal))?
            .current()
            .iter()
            .any(|loaded| library.is_none_or(|id| *id == loaded.library));
        if loaded_zotero {
            let result = self.read_zotero(true, None, cancel);
            if library.is_some() {
                return result;
            }
            if let Err(failure) = result
                && failure.code == ErrorCode::Cancelled
            {
                return Err(failure);
            }
        }
        let state = self.state()?;
        if library.is_some_and(|id| !state.libraries.contains_key(id)) {
            return Err(error(ErrorCode::NotFound));
        }
        let paths: Vec<_> = self
            .shared
            .sources
            .lock()
            .map_err(|_| error(ErrorCode::Internal))?
            .slots
            .iter()
            .filter(|(_, slot)| library.is_none_or(|id| slot.loaded.library == *id))
            .map(|(slot, source)| (*slot, source.generation, source.path.clone()))
            .collect();
        if library.is_some() && paths.is_empty() {
            return Err(error(ErrorCode::SourceUnavailable));
        }
        for (slot, generation, path) in paths {
            cancel.check()?;
            let reload = self
                .prepare_file_reload_expected(&path, Some((slot, generation)), cancel)
                .and_then(|prepared| self.accept_file_reload(prepared));
            if let Err(failure) = reload {
                if failure.code == ErrorCode::Cancelled {
                    return Err(failure);
                }
                if library.is_some() {
                    return Err(failure);
                }
            }
        }
        Ok(())
    }
}
impl Engine {
    /// Read the chosen Zotero libraries; `libraries` replaces the choice. A
    /// new choice takes effect only once all its libraries are read, and the
    /// libraries no longer chosen leave in the same publication.
    pub(super) fn read_zotero(
        &self,
        reload: bool,
        libraries: Option<&[ZoteroLibrary]>,
        cancel: &Cancellation,
    ) -> Result<(), EngineError> {
        cancel.check()?;
        let _operation = self
            .resources
            .operation
            .lock()
            .map_err(|_| error(ErrorCode::Internal))?;
        let mut source = self.resources.zotero.lock().map_err(|_| error(ErrorCode::Internal))?;
        let mut chosen = match libraries {
            Some(libraries) => {
                let libraries: Vec<Library> = libraries.iter().copied().map(Into::into).collect();
                (source.libraries() != libraries)
                    .then(|| ZoteroSource::new(&libraries).map_err(|_| error(ErrorCode::BadInput)))
                    .transpose()?
            }
            None => None,
        };
        let reload = reload || !self.state()?.zotero_ready;
        let mut guard_error = None;
        let mut began = false;
        let mut on_full = || {
            if !began {
                if let Err(failure) = self.shared.begin_zotero_read() {
                    guard_error = Some(failure);
                    return Err(LiveError::Cancelled);
                }
                began = true;
            }
            Ok(())
        };
        let target = chosen.as_mut().unwrap_or(&mut *source);
        let result = if let Some(port) = self.resources.zotero_port {
            target.refresh_observed(port, reload, &|| cancel.is_cancelled(), &mut on_full)
        } else {
            let cancel = cancel.clone();
            target.refresh_native(reload, Arc::new(move || cancel.is_cancelled()), &mut on_full)
        };
        let read = target.libraries();
        if let Some(failure) = guard_error {
            return Err(failure);
        }
        match result {
            Ok(loaded) => {
                cancel.check()?;
                let entries = loaded
                    .iter()
                    .zip(read)
                    .map(|(loaded, library)| {
                        let mut info = info(loaded);
                        info.zotero = Some(library.into());
                        (info, &loaded.snapshot)
                    })
                    .collect();
                self.shared.publish(entries, ibis_sources::zotero_source::is_zotero)?;
                if let Some(chosen) = chosen {
                    *source = chosen;
                }
                Ok(())
            }
            Err(failure) => {
                let code = if failure.cancelled() {
                    ErrorCode::Cancelled
                } else if failure.limited() {
                    ErrorCode::ResourceLimit
                } else if failure.newer() {
                    ErrorCode::NewerFormat
                } else if failure == ZoteroError::InvalidData || failure == ZoteroError::Selection {
                    ErrorCode::BadInput
                } else if failure == ZoteroError::LibraryNotFound {
                    ErrorCode::NotFound
                } else {
                    ErrorCode::SourceUnavailable
                };
                // Any failed read retires prepared Zotero copies until a read
                // succeeds, even when only a new choice failed: its full read
                // already began, and the copy guard does not tell them apart.
                if code != ErrorCode::Cancelled {
                    if !began {
                        self.shared.begin_zotero_read()?;
                    }
                    // A new choice that fails says nothing about the old one.
                    if chosen.is_none() {
                        for previous in source.current() {
                            let _ = self.shared.failed(&previous.library, code);
                        }
                    }
                }
                Err(error(code))
            }
        }
    }
}
impl Shared {
    fn file_changed(
        &self,
        id: u64,
        generation: u64,
        candidate: Result<LoadedLibrary, SourceError>,
    ) -> Option<Snapshot> {
        let mut registry = self.sources.lock().ok()?;
        let slot = registry.slots.get(&id)?;
        if slot.generation != generation {
            return None;
        }
        let previous = slot.loaded.library.clone();
        match candidate {
            Err(failure) => {
                if failure == SourceError::WatchUnavailable {
                    let _ = self.file_watch_unavailable(&previous);
                } else {
                    let _ = self.failed(&previous, source_error(failure).code);
                }
                None
            }
            Ok(mut loaded) => {
                if let Ok(state) = self.state_view() {
                    loaded.snapshot.reuse_unchanged(state.index.snapshot());
                }
                let remove = (previous != loaded.library
                    && !registry
                        .slots
                        .iter()
                        .any(|(other, slot)| *other != id && slot.loaded.library == previous))
                .then_some(previous.clone());
                match self.install(info(&loaded), &loaded.snapshot, remove) {
                    Ok(()) => {
                        let installed = loaded.snapshot.clone();
                        registry.slots.get_mut(&id)?.loaded = Arc::new(loaded);
                        Some(installed)
                    }
                    Err(failure) => {
                        let _ = self.failed(&previous, failure.code);
                        None
                    }
                }
            }
        }
    }

    fn file_watch_unavailable(&self, library: &LibraryId) -> Result<(), EngineError> {
        let _publication = self.publication.lock().map_err(|_| error(ErrorCode::Internal))?;
        let old = self.state_view()?;
        // No source observation completed: observers must not advance their
        // successful-check time just because watch health changed.
        let mut next = old.with_failure(library, ErrorCode::SourceUnavailable)?;
        next.libraries
            .get_mut(library)
            .ok_or_else(|| error(ErrorCode::NotFound))?
            .watch_warning = Some(WatchWarning::LiveRefreshOff);
        *self.state.write().map_err(|_| error(ErrorCode::Internal))? = Arc::new(next);
        self.notify()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn lost_registration_warns_until_a_new_watch_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthetic.bib");
        std::fs::write(&path, "@misc{kept}").unwrap();
        let engine = Engine::new().unwrap().without_file_watches();
        engine.open_file(&path, &Cancellation::default()).unwrap();
        let (id, generation) = {
            let registry = engine.shared.sources.lock().unwrap();
            let (id, slot) = registry.slots.iter().next().unwrap();
            (*id, slot.generation)
        };
        engine
            .shared
            .file_changed(id, generation, Err(SourceError::WatchUnavailable));
        let info = engine.libraries().unwrap();
        assert_eq!(info.libraries[0].objects, 1);
        assert_eq!(info.libraries[0].watch_warning, Some(WatchWarning::LiveRefreshOff));
        assert_eq!(info.libraries[0].refresh_error, Some(ErrorCode::SourceUnavailable));
        // A delayed success belonging to an older registration cannot clear it.
        engine.shared.file_changed(
            id,
            generation.wrapping_sub(1),
            Ok(ibis_sources::load_file(&path).unwrap()),
        );
        assert_eq!(
            engine.libraries().unwrap().libraries[0].watch_warning,
            Some(WatchWarning::LiveRefreshOff)
        );
        engine
            .shared
            .file_changed(id, generation, Ok(ibis_sources::load_file(&path).unwrap()));
        assert_eq!(engine.libraries().unwrap().libraries[0].watch_warning, None);
    }

    #[test]
    fn an_unaccepted_watch_cannot_activate_with_another_reloads_slot_generation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthetic.bib");
        std::fs::write(&path, "@book{original}").unwrap();
        let engine = Engine::new().unwrap().without_file_watches();
        let cancel = Cancellation::default();
        engine.open_file(&path, &cancel).unwrap();
        let (id, generation) = engine
            .shared
            .sources
            .lock()
            .unwrap()
            .slots
            .iter()
            .map(|(id, slot)| (*id, slot.generation.wrapping_add(1)))
            .next()
            .unwrap();
        let (abandoned, callback) = engine.reload_watch_callback(id, generation);
        let old = ibis_sources::load_file(&path).unwrap();
        // The worker can already have read a candidate during native setup.
        let worker = std::thread::spawn(move || callback(Ok(old)));
        std::fs::write(&path, "@book{accepted}").unwrap();
        let prepared = engine.prepare_file_reload(&path, &cancel).unwrap();
        engine.accept_file_reload(prepared).unwrap();
        let accepted = engine.snapshot().unwrap();
        // Its generation now exists, but this particular watch was not accepted.
        drop(abandoned);
        assert!(worker.join().unwrap().is_none());
        assert_eq!(engine.snapshot().unwrap().fingerprints(), accepted.fingerprints());

        let (activation, callback) = engine.reload_watch_callback(id, generation);
        std::fs::write(&path, "@book{watched}").unwrap();
        let watched = ibis_sources::load_file(&path).unwrap();
        let worker = std::thread::spawn(move || callback(Ok(watched)));
        activation.send(()).unwrap();
        assert!(worker.join().unwrap().is_some());
        assert_ne!(engine.snapshot().unwrap().fingerprints(), accepted.fingerprints());
        // A previously activated watcher still retires when the source reopens.
        let (activation, callback) = engine.reload_watch_callback(id, generation);
        activation.send(()).unwrap();
        let late = ibis_sources::load_file(&path).unwrap();
        engine.open_file(&path, &cancel).unwrap();
        assert!(callback(Ok(late)).is_none());
    }

    #[test]
    fn failed_reload_stays_private_and_cannot_mark_a_newer_accepted_read_stale() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthetic.bib");
        std::fs::write(&path, "@book{original}").unwrap();
        let engine = Engine::new().unwrap().without_file_watches();
        let cancel = Cancellation::default();
        engine.open_file(&path, &cancel).unwrap();
        std::fs::remove_file(&path).unwrap();
        let failed = engine.prepare_file_reload(&path, &cancel).unwrap();
        assert_eq!(engine.libraries().unwrap().libraries[0].refresh_error, None);
        std::fs::write(&path, "@book{repaired}").unwrap();
        let repair = engine.prepare_file_reload(&path, &cancel).unwrap();
        engine.accept_file_reload(repair).unwrap();
        assert_eq!(
            engine.accept_file_reload(failed).unwrap_err().code,
            ErrorCode::Cancelled
        );
        assert_eq!(engine.libraries().unwrap().libraries[0].refresh_error, None);
        std::fs::remove_file(&path).unwrap();
        let cancelled = Cancellation::default();
        let failed = engine.prepare_file_reload(&path, &cancelled).unwrap();
        cancelled.cancel();
        assert_eq!(
            engine.accept_file_reload(failed).unwrap_err().code,
            ErrorCode::Cancelled
        );
        assert_eq!(engine.libraries().unwrap().libraries[0].refresh_error, None);
        let failed = engine.prepare_file_reload(&path, &cancel).unwrap();
        assert_eq!(
            engine.accept_file_reload(failed).unwrap_err().code,
            ErrorCode::SourceUnavailable
        );
        assert_eq!(
            engine.libraries().unwrap().libraries[0].refresh_error,
            Some(ErrorCode::SourceUnavailable)
        );
    }

    #[test]
    fn reload_watch_setup_leaves_the_selection_guard_available_for_supersession() {
        use std::{sync::mpsc, time::Duration};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthetic.bib");
        std::fs::write(&path, "@book{original}").unwrap();
        let engine = Engine::new().unwrap();
        engine.resources.fail_watches.store(true, Ordering::Relaxed);
        engine.open_file(&path, &Cancellation::default()).unwrap();
        let original = engine.snapshot().unwrap();
        std::fs::write(&path, "@book{replacement}").unwrap();
        let (entered, setup_started) = mpsc::channel();
        let (release, released) = mpsc::channel();
        *engine.resources.before_watch.lock().unwrap() = Some(Box::new(move || {
            entered.send(()).unwrap();
            released.recv().unwrap();
        }));
        let selection = Arc::new(Mutex::new(()));
        let cancel = Cancellation::default();
        let worker = {
            let engine = engine.clone();
            let selection = selection.clone();
            let cancel = cancel.clone();
            std::thread::spawn(move || {
                let prepared = engine.prepare_file_reload(&path, &cancel)?;
                let _selection = selection.lock().unwrap();
                engine.accept_file_reload(prepared)
            })
        };
        setup_started.recv_timeout(Duration::from_secs(2)).unwrap();
        let can_supersede = selection.try_lock().is_ok();
        cancel.cancel();
        release.send(()).unwrap();
        assert_eq!(worker.join().unwrap().unwrap_err().code, ErrorCode::Cancelled);
        assert!(can_supersede, "native watch setup must never hold the selection guard");
        assert_eq!(engine.snapshot().unwrap().fingerprints(), original.fingerprints());
    }

    #[test]
    fn prepared_file_reload_cannot_replace_a_newer_watcher_publication() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthetic.bib");
        std::fs::write(&path, "@book{r0}").unwrap();
        let engine = Engine::new().unwrap().without_file_watches();
        let cancel = Cancellation::default();
        engine.open_file(&path, &cancel).unwrap();
        let before = engine.snapshot().unwrap();
        std::fs::write(&path, "@book{r1}").unwrap();
        let prepared = engine.prepare_file_reload(&path, &cancel).unwrap();
        assert_eq!(engine.snapshot().unwrap().fingerprints(), before.fingerprints());
        let (id, generation) = engine
            .shared
            .sources
            .lock()
            .unwrap()
            .slots
            .iter()
            .map(|(id, slot)| (*id, slot.generation))
            .next()
            .unwrap();
        std::fs::write(&path, "@book{r2}").unwrap();
        let watcher = ibis_sources::load_file(&path).unwrap();
        let newer = watcher.snapshot.fingerprints().clone();
        engine.shared.file_changed(id, generation, Ok(watcher)).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(
            engine.accept_file_reload(prepared),
            Err(EngineError {
                code: ErrorCode::Cancelled
            })
        ));
        assert_eq!(engine.snapshot().unwrap().fingerprints(), &newer);
        // A later fresh open has a new owner; a captured reload cannot consume it.
        std::fs::write(&path, "@book{reopened}").unwrap();
        let prepared = engine.prepare_file_reload(&path, &cancel).unwrap();
        engine.open_file(&path, &cancel).unwrap();
        let reopened = engine.snapshot().unwrap();
        assert!(engine.accept_file_reload(prepared).is_err());
        assert_eq!(engine.snapshot().unwrap().fingerprints(), reopened.fingerprints());
    }

    #[test]
    fn prepared_reload_accepts_repair_after_health_only_updates_and_keeps_other_errors() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.bib");
        let b = dir.path().join("b.bib");
        std::fs::write(&a, "@book{original}").unwrap();
        std::fs::write(&b, "@book{other}").unwrap();
        let engine = Engine::new().unwrap().without_file_watches();
        let cancel = Cancellation::default();
        engine.open_file(&a, &cancel).unwrap();
        engine.open_file(&b, &cancel).unwrap();
        let a_id = ibis_sources::load_file(&a).unwrap().library;
        let b_id = ibis_sources::load_file(&b).unwrap().library;
        std::fs::write(&a, "@book{repaired}").unwrap();
        let prepared = engine.prepare_file_reload(&a, &cancel).unwrap();
        engine.shared.failed(&a_id, ErrorCode::BadInput).unwrap();
        engine.shared.file_watch_unavailable(&a_id).unwrap();
        engine.shared.failed(&b_id, ErrorCode::SourceUnavailable).unwrap();
        engine.accept_file_reload(prepared).unwrap();
        let info = engine.libraries().unwrap().libraries;
        assert_eq!(info.iter().find(|info| info.id == a_id).unwrap().refresh_error, None);
        assert_eq!(
            info.iter().find(|info| info.id == b_id).unwrap().refresh_error,
            Some(ErrorCode::SourceUnavailable)
        );
        let prepared = engine.prepare_file_reload(&a, &cancel).unwrap();
        let same = ibis_sources::load_file(&a).unwrap();
        // Equal payloads are still a newer accepted observation/diagnostic owner.
        engine.publish_library(a_id, Arc::new(same.snapshot)).unwrap();
        assert!(matches!(
            engine.accept_file_reload(prepared),
            Err(EngineError {
                code: ErrorCode::Cancelled
            })
        ));
    }

    #[test]
    fn prepared_file_reload_is_cancellable_and_accepts_only_its_origin() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthetic.bib");
        std::fs::write(&path, "@book{original}").unwrap();
        let engine = Engine::new().unwrap().without_file_watches();
        engine.open_file(&path, &Cancellation::default()).unwrap();
        let original = engine.snapshot().unwrap();
        std::fs::write(&path, "@book{replacement}").unwrap();
        let cancel = Cancellation::default();
        let prepared = engine.prepare_file_reload(&path, &cancel).unwrap();
        cancel.cancel();
        assert!(matches!(
            engine.accept_file_reload(prepared),
            Err(EngineError {
                code: ErrorCode::Cancelled
            })
        ));
        assert_eq!(engine.snapshot().unwrap().fingerprints(), original.fingerprints());
        let prepared = engine.prepare_file_reload(&path, &Cancellation::default()).unwrap();
        let other = Engine::new().unwrap().without_file_watches();
        assert!(matches!(
            other.accept_file_reload(prepared),
            Err(EngineError {
                code: ErrorCode::BadInput
            })
        ));
        assert_eq!(engine.snapshot().unwrap().fingerprints(), original.fingerprints());
        for text in ["@book{partial,", "@book{complete}", ""] {
            std::fs::write(&path, text).unwrap();
            let prepared = engine.prepare_file_reload(&path, &Cancellation::default()).unwrap();
            if let Err(error) = engine.accept_file_reload(prepared) {
                assert_eq!(error.code, ErrorCode::BadInput);
                assert_eq!(engine.snapshot().unwrap().fingerprints(), original.fingerprints());
            }
        }
        assert!(engine.snapshot().unwrap().objects().is_empty());
        let accepted = engine.snapshot().unwrap();
        std::fs::remove_file(&path).unwrap();
        let failed = engine.prepare_file_reload(&path, &Cancellation::default()).unwrap();
        assert!(engine.accept_file_reload(failed).is_err());
        assert_eq!(engine.snapshot().unwrap().fingerprints(), accepted.fingerprints());
    }

    #[test]
    fn zotero_full_reads_retire_only_zotero_citations_until_coherent_publication() {
        let case: GoldenCase = serde_json::from_str(include_str!("../../../../fixtures/cases/02-pandoc.json")).unwrap();
        let engine = Engine::from_snapshot(Arc::new(case.library.clone()));
        let cancel = Cancellation::default();
        let file = engine.prepare_citation(&case.request, &cancel).unwrap();
        let LibraryObject::Record(mut record) = case.library.objects()[0].object.clone() else {
            panic!("invented record")
        };
        record.id.library = LibraryId::zotero(b"synthetic");
        record.citation_keys[0].value = "Zotero2026".into();
        let mut request = case.request.clone();
        request.items[0].item = record.id.clone();
        let id = record.id.library.clone();
        let snapshot = Arc::new(Snapshot::new(vec![LibraryObject::Record(record)]).unwrap());
        engine.publish_library(id.clone(), snapshot.clone()).unwrap();
        let old = engine.prepare_citation(&request, &cancel).unwrap();
        engine.shared.begin_zotero_read().unwrap();
        assert_eq!(
            engine.prepare_citation(&request, &cancel).unwrap_err().code,
            ErrorCode::SourceUnavailable
        );
        assert!(matches!(
            engine.commit_citation(&old, &old.result().content_hash, |_| panic!("uncertain Zotero")),
            Err(CitationCommitError::Engine(EngineError {
                code: ErrorCode::SourceUnavailable
            }))
        ));
        engine
            .commit_citation(&file, &file.result().content_hash, |content| {
                assert_eq!(content.text, "[@Example2026]")
            })
            .unwrap();
        // Neither a failed read nor a successful file publication restores Zotero.
        engine.shared.failed(&id, ErrorCode::SourceUnavailable).unwrap();
        engine
            .publish_library(case.request.items[0].item.library.clone(), Arc::new(case.library))
            .unwrap();
        assert_eq!(
            engine.prepare_citation(&request, &cancel).unwrap_err().code,
            ErrorCode::SourceUnavailable
        );
        engine.publish_library(id.clone(), snapshot.clone()).unwrap();
        assert!(matches!(
            engine.commit_citation(&old, &old.result().content_hash, |_| panic!("retired preview")),
            Err(CitationCommitError::PreviewChanged)
        ));
        let current = engine.prepare_citation(&request, &cancel).unwrap();
        engine
            .commit_citation(&current, &current.result().content_hash, |content| {
                assert_eq!(content.text, "[@Zotero2026]")
            })
            .unwrap();
        // A second full read retires the newer preview too, even with equal data.
        engine.shared.begin_zotero_read().unwrap();
        engine.publish_library(id, snapshot).unwrap();
        assert!(matches!(
            engine.commit_citation(&current, &current.result().content_hash, |_| panic!("old generation")),
            Err(CitationCommitError::PreviewChanged)
        ));
    }

    #[test]
    fn readable_file_still_returns_results_when_watch_creation_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthetic.bib");
        std::fs::write(&path, "@article{demo,title={Synthetic widgets},year={2026}}\n").unwrap();
        let engine = Engine::new().unwrap();
        engine.resources.fail_watches.store(true, Ordering::Relaxed);
        let cancel = Cancellation::default();
        engine.open_file(&path, &cancel).unwrap();
        let result = engine
            .dispatch(Method::Search, &json!({"query":"widgets"}), &cancel)
            .unwrap();
        assert_eq!(result.exit_code, ExitCode::Ok);
        assert_eq!(result.data["hits"].as_array().unwrap().len(), 1);
        assert_eq!(
            engine.libraries().unwrap().libraries[0].watch_warning,
            Some(WatchWarning::LiveRefreshOff)
        );
        assert!(engine.resources.watches.lock().unwrap().is_empty());
        let listed = engine
            .dispatch(Method::Libraries, &json!({"action":"list"}), &cancel)
            .unwrap();
        assert_eq!(listed.exit_code, ExitCode::Ok);
        assert_eq!(listed.data["libraries"][0]["watch_warning"], "live-refresh-off");
        let cited = engine
            .cite_keys(
                &KeyCitationRequest {
                    keys: vec!["demo".into()],
                    output: OutputSpec::Key {
                        syntax: KeySyntax::Pandoc,
                        command: None,
                    },
                    form: CitationForm::Normal,
                    output_locale: None,
                },
                &cancel,
            )
            .unwrap();
        assert_eq!(
            engine
                .finalize(DispatchResult::citation(cited, 1).unwrap())
                .unwrap()
                .text
                .as_deref(),
            Some("[@demo]")
        );
        engine.reload_files(None, &cancel).unwrap();
        assert_eq!(
            engine.libraries().unwrap().libraries[0].watch_warning,
            Some(WatchWarning::LiveRefreshOff)
        );
    }
    #[test]
    fn one_shot_file_load_does_not_attempt_a_watch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthetic.bib");
        std::fs::write(&path, "@article{demo,title={Synthetic widgets}}\n").unwrap();
        let engine = Engine::new().unwrap().without_file_watches();
        engine.resources.fail_watches.store(true, Ordering::Relaxed);
        engine.open_file(&path, &Cancellation::default()).unwrap();
        assert_eq!(engine.libraries().unwrap().libraries[0].watch_warning, None);
        assert!(engine.resources.watches.lock().unwrap().is_empty());
    }
    #[test]
    fn old_callbacks_cannot_replace_new_data_and_equal_hash_updates_still_replace_owner() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthetic.bib");
        let write = |title: &str| std::fs::write(&path, format!("@article{{demo,title={{{title}}}}}\n")).unwrap();
        write("Original");
        let old = ibis_sources::load_file(&path).unwrap();
        let engine = Engine::new().unwrap();
        let cancel = Cancellation::default();
        engine.open_file(&path, &cancel).unwrap();
        let (id, generation) = engine
            .shared
            .sources
            .lock()
            .unwrap()
            .slots
            .iter()
            .map(|(id, slot)| (*id, slot.generation))
            .next()
            .unwrap();
        write("Replacement");
        engine.open_file(&path, &cancel).unwrap();
        let fresh = engine.snapshot().unwrap().fingerprints().clone();
        assert!(engine.shared.file_changed(id, generation, Ok(old)).is_none());
        assert_eq!(engine.snapshot().unwrap().fingerprints(), &fresh);
        let (generation, prior) = {
            let registry = engine.shared.sources.lock().unwrap();
            let slot = registry.slots.get(&id).unwrap();
            (slot.generation, slot.loaded.clone())
        };
        let mut same = (*prior).clone();
        same.warnings = vec![
            ibis_sources::SourceWarning {
                code: ibis_sources::WarningCode::Field,
                line: None
            };
            MAX_WARNING_DETAILS + 1
        ];
        let prepared_index = engine.state().unwrap().index.clone();
        assert!(engine.shared.file_changed(id, generation, Ok(same)).is_some());
        assert!(Arc::ptr_eq(&prepared_index, &engine.state().unwrap().index));
        let libraries = engine.libraries().unwrap();
        assert_eq!(libraries.libraries[0].warnings.len(), MAX_WARNING_DETAILS);
        assert_eq!(libraries.libraries[0].warnings_total, MAX_WARNING_DETAILS + 1);
        assert_eq!(engine.snapshot().unwrap().fingerprints(), &fresh);
        let registry = engine.shared.sources.lock().unwrap();
        assert!(!Arc::ptr_eq(&registry.slots[&id].loaded, &prior));
        drop(registry);
        let handles = Arc::downgrade(&engine.resources);
        let publication = engine.shared.clone();
        drop(engine);
        assert!(
            handles.upgrade().is_none(),
            "publication readers cannot retain source handles"
        );
        assert!(publication.state_view().is_ok());
    }
}
