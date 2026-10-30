//! Native file loading: preparation has no publication authority of its own.
use super::*;
use ibis_sources::{
    SourceSnapshot,
    generation::{GenerationStore, StagedGeneration},
};
use std::{path::Path, sync::OnceLock};

#[derive(Default)]
struct CacheState {
    current: Option<u64>,
    closed: bool,
    accepted: Option<LibraryId>,
}
#[derive(Default)]
struct CacheInner {
    state: Mutex<CacheState>,
    store: OnceLock<Option<GenerationStore>>,
}
/// One process-lived lease. Failure to acquire storage leaves loading uncached.
#[derive(Default)]
pub struct FileCacheSession {
    inner: Arc<CacheInner>,
}
#[derive(Clone)]
pub struct FileCacheOperation {
    owner: Weak<CacheInner>,
    generation: u64,
}
pub struct PreparedFileLoad {
    candidate: Result<(Engine, LibraryId), EngineError>,
    operation: FileCacheOperation,
    staged: Option<StagedGeneration>,
    cancel: Cancellation,
    hit: bool,
}
impl PreparedFileLoad {
    /// The candidate is private until the caller accepts its captured selection.
    pub fn engine(&self) -> Option<Engine> {
        self.candidate.as_ref().ok().map(|(engine, _)| engine.clone())
    }
    pub fn cache_hit(&self) -> bool {
        self.hit
    }
}
impl FileCacheSession {
    pub fn open(path: &Path) -> Self {
        let session = Self::default();
        session.configure(path);
        session
    }
    pub fn configure(&self, path: &Path) {
        self.inner.store.get_or_init(|| GenerationStore::open(path).ok());
    }
    /// Call under the source-selection lock, before scheduling any work.
    pub fn invalidate(&self, generation: u64) {
        let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        state.current = Some(generation);
        if let Some(store) = self.inner.store.get().and_then(Option::as_ref) {
            let _ = store.invalidate();
        }
    }
    pub fn operation(&self, generation: u64) -> FileCacheOperation {
        FileCacheOperation {
            owner: Arc::downgrade(&self.inner),
            generation,
        }
    }
    pub fn remove(&self) {
        let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        state.current = None;
        if let Some(store) = self.inner.store.get().and_then(Option::as_ref) {
            let _ = store.invalidate();
            if let Some(library) = state.accepted.take() {
                let _ = store.remove_library(&library);
            }
        }
    }
    pub fn close(&self) {
        let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        state.closed = true;
        if let Some(store) = self.inner.store.get().and_then(Option::as_ref) {
            let _ = store.close();
        }
    }
}
impl Drop for FileCacheSession {
    fn drop(&mut self) {
        self.close();
    }
}
impl FileCacheOperation {
    fn current<T>(
        &self,
        apply: impl FnOnce(&mut CacheState, Option<&GenerationStore>) -> Result<T, EngineError>,
    ) -> Result<T, EngineError> {
        let owner = self.owner.upgrade().ok_or_else(|| error(ErrorCode::Cancelled))?;
        let mut state = owner.state.lock().map_err(|_| error(ErrorCode::Internal))?;
        if state.closed || state.current != Some(self.generation) {
            return Err(error(ErrorCode::Cancelled));
        }
        apply(&mut state, owner.store.get().and_then(Option::as_ref))
    }
}
impl Engine {
    /// Read and validate privately. A prepared load cannot change engine state
    /// or its manifest until the app accepts its captured selection.
    pub fn prepare_file_load(
        &self,
        path: &Path,
        strict: bool,
        operation: FileCacheOperation,
        cancel: &Cancellation,
    ) -> Result<PreparedFileLoad, EngineError> {
        cancel.check()?;
        operation.current(|_, _| Ok(()))?;
        let path = std::path::absolute(path).map_err(|_| error(ErrorCode::BadInput))?;
        let (loaded, observation, hit) = if strict {
            let (loaded, observation) = match SourceSnapshot::reload(&path, self.snapshot()?.as_ref()) {
                Ok(loaded) => loaded,
                Err(failure) => {
                    return Ok(PreparedFileLoad {
                        candidate: Err(sources::source_error(failure)),
                        operation,
                        staged: None,
                        cancel: cancel.clone(),
                        hit: false,
                    });
                }
            };
            (loaded, observation, false)
        } else {
            let observation = SourceSnapshot::read(&path).map_err(sources::source_error)?;
            let generation = operation
                .current(|_, store| Ok(store.and_then(|store| observation.cached_generation(store).ok().flatten())))?;
            let cached = generation
                .as_ref()
                .and_then(|generation| observation.hydrate(generation).ok());
            let hit = cached.is_some();
            let loaded = match cached {
                Some(loaded) => loaded,
                None => observation.load().map_err(sources::source_error)?,
            };
            (loaded, observation, hit)
        };
        cancel.check()?;
        // Allocate the writer while selection ownership is locked; invalidation
        // then prevents even a delayed first payload from charging or writing.
        let writer = operation.current(|_, store| {
            Ok(store
                .filter(|_| !hit)
                .and_then(|store| observation.writer(store, &loaded).ok().flatten()))
        })?;
        let staged = writer.and_then(|mut writer| {
            for object in loaded.snapshot.objects() {
                if cancel.is_cancelled() {
                    return None;
                }
                writer.push(object.object.clone()).ok()?;
            }
            writer.finish().ok()
        });
        cancel.check()?;
        let library = loaded.library.clone();
        let mut candidate = Engine::from_snapshot(Arc::new(loaded.snapshot.clone()));
        candidate.watch_files = self.watch_files;
        #[cfg(test)]
        candidate
            .resources
            .fail_watches
            .store(self.resources.fail_watches.load(Ordering::Relaxed), Ordering::Relaxed);
        candidate.install_file(path, loaded, Some(observation), cancel)?;
        operation.current(|_, _| cancel.check())?;
        Ok(PreparedFileLoad {
            candidate: Ok((candidate, library)),
            operation,
            staged,
            cancel: cancel.clone(),
            hit,
        })
    }
    /// Caller holds its source-selection guard, before entering the existing
    /// engine publication/commit barrier. Storage errors never refuse a source.
    pub fn accept_file_load(&self, load: PreparedFileLoad) -> Result<(), EngineError> {
        load.operation.current(|state, store| {
            load.cancel.check()?;
            let (candidate, library) = match load.candidate {
                Ok(candidate) => candidate,
                Err(failure) => {
                    for id in self.state()?.libraries.keys() {
                        self.shared.failed(id, failure.code)?;
                    }
                    state.current = None;
                    return Err(failure);
                }
            };
            if !Arc::ptr_eq(&self.shared, &candidate.shared) {
                return Err(error(ErrorCode::BadInput));
            }
            if let Some(store) = store {
                if let Some(staged) = load.staged {
                    let _ = store.publish(staged);
                }
                if let Some(previous) = state.accepted.as_ref().filter(|id| **id != library) {
                    let _ = store.remove_library(previous);
                }
            }
            state.accepted = Some(library);
            state.current = None;
            Ok(())
        })
    }
}
