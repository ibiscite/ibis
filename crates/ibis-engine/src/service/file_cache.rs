//! Native file loading: preparation has no publication authority of its own.
use super::*;
use ibis_sources::{
    SourceSnapshot,
    generation::{GenerationStore, StagedGeneration},
};
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

#[derive(Default)]
struct CacheState {
    current: Option<u64>,
    closed: bool,
    configured: Option<LibraryId>,
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
            if let Some(library) = state.configured.take() {
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
    /// Capture selection before startup work, including before hydration begins.
    pub fn select_file(&self, path: &Path) -> Result<Option<PathBuf>, EngineError> {
        let source = std::fs::canonicalize(path).ok();
        if let Some(source) = &source {
            let library = LibraryId::file(source.as_os_str().as_encoded_bytes());
            self.current(|state, store| {
                if let Some(previous) = state.configured.replace(library.clone()).filter(|id| *id != library)
                    && let Some(store) = store
                {
                    let _ = store.remove_library(&previous);
                }
                Ok(())
            })?;
        }
        Ok(source)
    }
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
        let verifying = self
            .state()?
            .readiness
            .iter()
            .any(|(source, entry)| source.is_some() && !entry.1);
        if let Some(source) = operation.select_file(&path)? {
            let library = LibraryId::file(source.as_os_str().as_encoded_bytes());
            if !strict
                && !verifying
                && source
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("bib"))
            {
                let generation = operation
                    .current(|_, store| Ok(store.and_then(|store| store.open_library(&library).ok().flatten())))?;
                if let Some(loaded) = generation
                    .as_ref()
                    .and_then(|generation| SourceSnapshot::hydrate(generation).ok())
                {
                    let mut candidate = Engine::from_snapshot(Arc::new(loaded.snapshot.clone()));
                    candidate.watch_files = self.watch_files;
                    let info = LibraryInfo::clean(library.clone(), loaded.snapshot.objects().len());
                    candidate.shared.install(info, &loaded.snapshot, None)?;
                    candidate.shared.begin_read(Some(library.clone()))?;
                    operation.current(|_, _| cancel.check())?;
                    return Ok(PreparedFileLoad {
                        candidate: Ok((candidate, library)),
                        operation,
                        staged: None,
                        cancel: cancel.clone(),
                        hit: true,
                    });
                }
            }
        }
        let read = if strict {
            SourceSnapshot::reload(&path, self.snapshot()?.as_ref())
        } else {
            SourceSnapshot::read(&path).and_then(|observation| Ok((observation.load()?, observation)))
        };
        let (loaded, observation) = match read {
            Ok(loaded) => loaded,
            Err(failure) if strict || verifying => {
                return Ok(PreparedFileLoad {
                    candidate: Err(sources::source_error(failure)),
                    operation,
                    staged: None,
                    cancel: cancel.clone(),
                    hit: false,
                });
            }
            Err(failure) => return Err(sources::source_error(failure)),
        };
        cancel.check()?;
        // Allocate the writer while selection ownership is locked; invalidation
        // then prevents even a delayed first payload from charging or writing.
        let writer = operation
            .current(|_, store| Ok(store.and_then(|store| observation.writer(store, &loaded).ok().flatten())))?;
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
        candidate.install_file(path, loaded, Some(observation), cancel)?;
        operation.current(|_, _| cancel.check())?;
        Ok(PreparedFileLoad {
            candidate: Ok((candidate, library)),
            operation,
            staged,
            cancel: cancel.clone(),
            hit: false,
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
                if let Some(previous) = state.configured.as_ref().filter(|id| **id != library) {
                    let _ = store.remove_library(previous);
                }
            }
            state.configured = Some(library);
            if !load.hit {
                state.current = None;
            }
            Ok(())
        })
    }
}
