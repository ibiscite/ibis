//! Source handles stay outside Shared: callbacks upgrade only publication state.
use super::*;
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
    #[cfg(test)]
    fail_watches: std::sync::atomic::AtomicBool,
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
fn source_error(error: SourceError) -> EngineError {
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
            (ibis_sources::reload_file(&path).map_err(source_error)?, None)
        } else {
            let (loaded, observation) = ibis_sources::load_file_snapshot(&path).map_err(source_error)?;
            (loaded, Some(observation))
        };
        cancel.check()?;
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
            if let Some(shared) = weak.upgrade() {
                shared.file_changed(id, generation, candidate);
            }
        };
        let watch = self.watch(&path, observation, callback).map_err(source_error)?;
        let previous = existing.as_ref().map(|entry| entry.2.clone()).filter(|old| {
            *old != loaded.library
                && !registry
                    .slots
                    .iter()
                    .any(|(other, slot)| *other != id && slot.loaded.library == *old)
        });
        if let Err(error) = self.shared.install(info(&loaded), &loaded.snapshot, previous) {
            drop(registry);
            drop(operation);
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
        let old = self
            .resources
            .watches
            .lock()
            .map_err(|_| error(ErrorCode::Internal))?
            .insert(id, watch);
        drop(operation);
        drop(old);
        cancel.check()
    }
    fn watch(
        &self,
        path: &Path,
        observation: Option<ibis_sources::SourceSnapshot>,
        callback: impl Fn(Result<LoadedLibrary, SourceError>) + Send + Sync + 'static,
    ) -> Result<SourceWatch, SourceError> {
        #[cfg(test)]
        if self.resources.fail_watches.load(Ordering::Relaxed) {
            return Err(SourceError::WatchUnavailable);
        }
        match observation {
            Some(observation) => SourceWatch::with_snapshot(path, observation, callback),
            None => SourceWatch::new(path, callback),
        }
    }
    pub(super) fn reload_files(&self, library: Option<&LibraryId>, cancel: &Cancellation) -> Result<(), EngineError> {
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
impl Shared {
    fn file_changed(&self, id: u64, generation: u64, candidate: Result<LoadedLibrary, SourceError>) {
        let Ok(mut registry) = self.sources.lock() else { return };
        let Some(slot) = registry.slots.get(&id) else { return };
        if slot.generation != generation {
            return;
        }
        let previous = slot.loaded.library.clone();
        match candidate {
            Err(failure) => {
                let _ = self.failed(&previous, source_error(failure).code);
            }
            Ok(loaded) => {
                let remove = (previous != loaded.library
                    && !registry
                        .slots
                        .iter()
                        .any(|(other, slot)| *other != id && slot.loaded.library == previous))
                .then_some(previous.clone());
                match self.install(info(&loaded), &loaded.snapshot, remove) {
                    Ok(()) => {
                        if let Some(slot) = registry.slots.get_mut(&id) {
                            slot.loaded = Arc::new(loaded);
                        }
                    }
                    Err(failure) => {
                        let _ = self.failed(&previous, failure.code);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
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
        engine.shared.file_changed(id, generation, Ok(old));
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
        engine.shared.file_changed(id, generation, Ok(same));
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
