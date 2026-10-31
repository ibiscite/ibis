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
        self.load_file(path, false, None, cancel)
    }
    fn load_file(
        &self,
        path: &Path,
        strict: bool,
        expected: Option<(u64, u64)>,
        cancel: &Cancellation,
    ) -> Result<(), EngineError> {
        cancel.check()?;
        let operation = self
            .resources
            .operation
            .lock()
            .map_err(|_| error(ErrorCode::Internal))?;
        let path = std::path::absolute(path).map_err(|_| error(ErrorCode::BadInput))?;
        if let Some((id, generation)) = expected {
            let registry = self.shared.sources.lock().map_err(|_| error(ErrorCode::Internal))?;
            if !registry
                .slots
                .get(&id)
                .is_some_and(|slot| slot.generation == generation && slot.path == path)
            {
                return Err(error(ErrorCode::Cancelled));
            }
        }
        let (loaded, observation) = if strict {
            (
                ibis_sources::reload_file_reusing(&path, self.snapshot()?.as_ref()).map_err(source_error)?,
                None,
            )
        } else {
            let (loaded, observation) = ibis_sources::load_file_snapshot(&path).map_err(source_error)?;
            (loaded, Some(observation))
        };
        cancel.check()?;
        self.install_file(path, loaded, observation, cancel)?;
        drop(operation);
        Ok(())
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
            .map(|(slot, source)| {
                (
                    *slot,
                    source.generation,
                    source.path.clone(),
                    source.loaded.library.clone(),
                )
            })
            .collect();
        if library.is_some() && paths.is_empty() {
            return Err(error(ErrorCode::SourceUnavailable));
        }
        for (slot, generation, path, id) in paths {
            cancel.check()?;
            if let Err(failure) = self.load_file(&path, true, Some((slot, generation)), cancel) {
                if failure.code == ErrorCode::Cancelled {
                    return Err(failure);
                }
                let _ = self.shared.failed(&id, failure.code);
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
        let reload = reload || self.state()?.readiness.get(&None).is_some_and(|entry| !entry.1);
        let mut guard_error = None;
        let mut began = false;
        let mut on_full = || {
            if !began {
                if let Err(failure) = self.shared.begin_read(None) {
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
                        self.shared.begin_read(None)?;
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
        let mut libraries = old.libraries.clone();
        let info = libraries.get_mut(library).ok_or_else(|| error(ErrorCode::NotFound))?;
        info.watch_warning = Some(WatchWarning::LiveRefreshOff);
        // No source observation completed: observers must not advance their
        // successful-check time just because watch health changed.
        info.refresh_error = Some(ErrorCode::SourceUnavailable);
        *self.state.write().map_err(|_| error(ErrorCode::Internal))? = Arc::new(State {
            readiness: old.readiness.clone(),
            index: old.index.clone(),
            libraries,
        });
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
