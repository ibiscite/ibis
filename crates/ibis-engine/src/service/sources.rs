//! Source handles stay outside Shared: callbacks upgrade only publication state.
use super::*;
use crate::sync::MutexExt;
use ibis_core::zotero::api::TransportError;
use ibis_sources::zotero_source::{Library, LiveError, ZoteroError, ZoteroSource};
use ibis_sources::adapter::{AdapterError, SourceAdapter};
use ibis_sources::{LoadedLibrary, SourceError, SourceWatch};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
const MAX_FILES: usize = 64;
const MAX_RELOAD_ATTEMPTS: usize = 3;
const MAX_WARNING_DETAILS: usize = 256;
#[cfg(test)]
#[path = "../../tests/support/zotero.rs"]
#[allow(dead_code)]
pub(crate) mod zotero_fixture;
#[derive(Default)]
pub(super) struct Owners {
    operation: Mutex<()>,
    watches: Mutex<BTreeMap<u64, SourceWatch>>,
    /// Each open adapter library with the adapter and library name that read it.
    adapters: Mutex<BTreeMap<LibraryId, (SourceAdapter, Option<String>)>>,
    zotero: Mutex<ZoteroSource>,
    zotero_port: Option<u16>,
    /// What the last closed read listed. Apart from `zotero`, which a read
    /// holds throughout, so listing never waits for one to finish.
    closed_groups: Mutex<Option<Vec<ibis_sources::zotero_source::Group>>>,
    #[cfg(test)]
    fail_watches: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    before_watch: Mutex<Option<Box<dyn Fn() + Send>>>,
}
impl Owners {
    pub(super) fn on_port(port: u16, profile: &[u8]) -> Self {
        Self {
            zotero: Mutex::new(ZoteroSource::default().on_profile(profile)),
            zotero_port: Some(port),
            ..Self::default()
        }
    }
    pub(super) fn read_closed(&self) -> bool {
        self.closed_groups.recover_lock().is_some()
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
impl Registry {
    pub(super) fn file_generation(&self, library: &LibraryId) -> Option<u64> {
        self.slots
            .values()
            .find(|slot| slot.loaded.library == *library)
            .map(|slot| slot.generation)
    }

    /// Where a file library was read from; other sources have no file.
    pub(super) fn file_path(&self, library: &LibraryId) -> Option<PathBuf> {
        self.slots
            .values()
            .find(|slot| slot.loaded.library == *library)
            .map(|slot| slot.path.clone())
    }

    /// The path a proposal for `library` may be written to: that of a file
    /// read as this library whose reader declared proposals. The permission
    /// and the destination always come from the same read.
    pub(super) fn proposal_path(&self, library: &LibraryId) -> Option<PathBuf> {
        self.slots
            .values()
            .find(|slot| {
                slot.loaded.library == *library && slot.loaded.capabilities.contains(&SourceCapability::Proposals)
            })
            .map(|slot| slot.path.clone())
    }
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

/// Zotero data and health kept private until the client accepts them under its
/// current selection guard. A guarded full read retires citations immediately.
pub struct PreparedZoteroReload {
    origin: Weak<Shared>,
    base: Arc<State>,
    next: State,
    source: ZoteroSource,
    closed_groups: Option<Vec<ibis_sources::zotero_source::Group>>,
    outcome: Result<(), EngineError>,
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

pub(super) fn zotero_error(failure: ZoteroError) -> EngineError {
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
    EngineError {
        code,
        reason: failure.reason(),
        detail: failure.schema_table().map(|table| ErrorDetail { table }),
        strict_cause: None,
        citation_error: None,
    }
}

pub(super) fn adapter_error(error: AdapterError) -> EngineError {
    use ibis_core::contract::ErrorReason;
    let reason = match error {
        AdapterError::Cancelled => return super::error(ErrorCode::Cancelled),
        AdapterError::NotFound => ErrorReason::SourceNotFound,
        AdapterError::ManagerClosed => ErrorReason::SourceManagerClosed,
        AdapterError::LibraryNeeded => ErrorReason::SourceLibraryNeeded,
        AdapterError::LibraryNotFound => ErrorReason::SourceLibraryNotFound,
        AdapterError::PermissionDenied => ErrorReason::SourcePermission,
        AdapterError::Failed => ErrorReason::SourceFailed,
    };
    EngineError::because(ErrorCode::SourceUnavailable, reason)
}

pub(super) fn source_error(error: SourceError) -> EngineError {
    use ibis_core::contract::ErrorReason;
    super::error(match error {
        SourceError::Missing => return EngineError::because(ErrorCode::SourceUnavailable, ErrorReason::MissingFile),
        SourceError::UnknownFormat => return EngineError::because(ErrorCode::BadInput, ErrorReason::UnknownFormat),
        SourceError::CsvTemplate => return EngineError::because(ErrorCode::BadInput, ErrorReason::CsvTemplate),
        SourceError::AttachmentOutsideLibrary => {
            return EngineError::because(ErrorCode::SourceUnavailable, ErrorReason::AttachmentOutsideLibrary);
        }
        SourceError::Cancelled => ErrorCode::Cancelled,
        SourceError::TooLarge => ErrorCode::ResourceLimit,
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
        kind: loaded.kind,
        capabilities: loaded.capabilities.to_vec(),
        revision: loaded.snapshot.fingerprints().full.clone(),
        objects: loaded.snapshot.objects().len(),
        warnings: loaded.warnings.iter().take(MAX_WARNING_DETAILS).cloned().collect(),
        warnings_total: loaded.warnings.len(),
        refresh_error: None,
        refresh_reason: None,
        updated_at_ms: now_ms(),
        watch_warning: None,
        zotero: None,
        duplicate_keys: loaded.duplicate_keys.clone(),
        damaged_keys: loaded.damaged_keys.clone(),
        cut_short: loaded.cut_short,
    }
}
impl Engine {
    /// Read on a private source clone. No source or operation lock is held
    /// during acquisition, so an abandoned read cannot block the next choice.
    pub fn prepare_zotero_reload(
        &self,
        reload: bool,
        libraries: Option<&[ZoteroLibrary]>,
        profile: Option<&str>,
        cancel: &Cancellation,
    ) -> Result<PreparedZoteroReload, EngineError> {
        self.prepare_zotero_reload_guarded(reload, libraries, profile, cancel, |retire| retire())
    }

    /// The client invokes `retire` while holding its current request/source
    /// guard. Only citation readiness changes during acquisition; data and
    /// health remain private until acceptance. An obsolete read must refuse
    /// retirement, including when it fails before its full read begins.
    pub fn prepare_zotero_reload_guarded(
        &self,
        reload: bool,
        libraries: Option<&[ZoteroLibrary]>,
        profile: Option<&str>,
        cancel: &Cancellation,
        mut guard: impl FnMut(&mut dyn FnMut() -> Result<(), EngineError>) -> Result<(), EngineError>,
    ) -> Result<PreparedZoteroReload, EngineError> {
        cancel.check()?;
        let (base, resources) = {
            let _operation = self.resources.operation.recover_lock();
            let source = self.resources.zotero.recover_lock().clone();
            let groups = self.resources.closed_groups.recover_lock().clone();
            (
                self.state()?,
                Owners {
                    zotero: Mutex::new(source),
                    zotero_port: self.resources.zotero_port,
                    closed_groups: Mutex::new(groups),
                    ..Owners::default()
                },
            )
        };
        // Reuse the complete read policy against isolated publication state.
        // Retire the active engine only while the client still owns this read.
        let candidate = Engine {
            shared: Arc::new(Shared::new(base.clone())),
            formatting: self.formatting.clone(),
            resources: Arc::new(resources),
            watch_files: false,
            pdf: self.pdf.clone(),
            built_in_sources: self.built_in_sources.clone(),
            attachment_text_outside_library: self.attachment_text_outside_library,
        };
        let mut expected = base;
        let mut retirement_error = None;
        let outcome = candidate.read_zotero_observed(reload, libraries, profile, cancel, || {
            let retirement = (|| {
                let mut retired = None;
                guard(&mut || {
                    if retired.is_some() {
                        return Ok(());
                    }
                    let _publication = self.shared.publication.recover_lock();
                    let mut state = self.shared.state.write().map_err(|_| error(ErrorCode::Internal))?;
                    cancel.check()?;
                    if !expected.same_publication(&state) {
                        return Err(error(ErrorCode::Cancelled));
                    }
                    let mut next = (**state).clone();
                    next.zotero_generation = next
                        .zotero_generation
                        .checked_add(1)
                        .ok_or_else(|| error(ErrorCode::ResourceLimit))?;
                    next.zotero_ready = false;
                    let next = Arc::new(next);
                    *state = next.clone();
                    retired = Some(next);
                    Ok(())
                })?;
                // A guard that does not perform retirement cannot authorize
                // acquisition to make a known-uncertain source copyable.
                expected = retired.ok_or_else(|| error(ErrorCode::Cancelled))?;
                Ok(())
            })();
            if let Err(failure) = retirement {
                retirement_error = Some(failure);
            }
            retirement
        });
        if let Some(failure) = retirement_error {
            return Err(failure);
        }
        cancel.check()?;
        let next = (*candidate.state()?).clone();
        let resources = Arc::try_unwrap(candidate.resources).map_err(|_| error(ErrorCode::Internal))?;
        Ok(PreparedZoteroReload {
            origin: Arc::downgrade(&self.shared),
            base: expected,
            next,
            source: crate::sync::recover(resources.zotero.into_inner()),
            closed_groups: crate::sync::recover(resources.closed_groups.into_inner()),
            outcome,
            cancel: cancel.clone(),
        })
    }

    /// Accept data and health together. Another accepted publication or a
    /// cancellation retires this candidate, even when its fingerprints match.
    pub fn accept_zotero_reload(&self, mut prepared: PreparedZoteroReload) -> Result<(), EngineError> {
        prepared.cancel.check()?;
        if !Weak::ptr_eq(&prepared.origin, &Arc::downgrade(&self.shared)) {
            return Err(error(ErrorCode::BadInput));
        }
        let _operation = self.resources.operation.recover_lock();
        let mut source = self.resources.zotero.recover_lock();
        let mut groups = self.resources.closed_groups.recover_lock();
        let _publication = self.shared.publication.recover_lock();
        let mut state = self.shared.state.write().map_err(|_| error(ErrorCode::Internal))?;
        prepared.cancel.check()?;
        if !prepared.base.same_publication(&state) {
            return Err(error(ErrorCode::Cancelled));
        }
        // A concurrent file-health update does not retire this Zotero read.
        for (id, info) in &mut prepared.next.libraries {
            if info.kind != LibraryKind::Zotero
                && let Some(current) = state.libraries.get(id)
            {
                *info = current.clone();
            }
        }
        *state = Arc::new(prepared.next);
        *source = prepared.source;
        *groups = prepared.closed_groups;
        drop(state);
        self.shared.notify()?;
        prepared.outcome
    }

    pub(crate) fn open_file(&self, path: &Path, cancel: &Cancellation) -> Result<(), EngineError> {
        self.open_file_observed(path, cancel).map(|_| ())
    }

    /// Return the canonical identity from the observation that supplied the bytes.
    pub fn open_file_observed(&self, path: &Path, cancel: &Cancellation) -> Result<PathBuf, EngineError> {
        cancel.check()?;
        let operation = self.resources.operation.recover_lock();
        let path = std::path::absolute(path).map_err(|_| error(ErrorCode::BadInput))?;
        let (loaded, observation) =
            ibis_sources::load_file_snapshot(&path, &|| cancel.is_cancelled()).map_err(source_error)?;
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
            let registry = self.shared.sources.recover_lock();
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
                ibis_sources::reload_file_snapshot(&path, Some(base.index.snapshot()), &|| cancel.is_cancelled())
                    .map_err(source_error)?;
            cancel.check()?;
            let library = loaded.library.clone();
            let mut next = State::replacement(&base, vec![(info(&loaded), &loaded.snapshot)], |old| {
                !other_owners && old.id == previous
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
        let _operation = self.resources.operation.recover_lock();
        let mut registry = self.shared.sources.recover_lock();
        let slot = registry
            .slots
            .get(&prepared.slot)
            .ok_or_else(|| error(ErrorCode::Cancelled))?;
        if slot.generation != prepared.generation || slot.path != prepared.path {
            return Err(error(ErrorCode::Cancelled));
        }
        let _publication = self.shared.publication.recover_lock();
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
                *state = Arc::new(state.with_failure(&slot.loaded.library, failure)?);
                drop(state);
                self.shared.notify()?;
                return Err(failure);
            }
        };
        // Failed reads may update health while parsing/indexing. A fresh repair
        // clears its own error, while unrelated libraries keep their latest health.
        for (id, info) in &mut read.next.libraries {
            if *id != read.loaded.library
                && let Some(current) = state.libraries.get(id)
            {
                info.refresh_error = current.refresh_error;
                info.refresh_reason = current.refresh_reason;
                info.watch_warning = current.watch_warning;
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
            let mut watches = self.resources.watches.recover_lock();
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
            if !*active.get_or_init(|| accepted.recover_lock().recv().is_ok()) {
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
        let mut registry = self.shared.sources.recover_lock();
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
            let mut watches = self.resources.watches.recover_lock();
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
        if let Some(before_watch) = self.resources.before_watch.recover_lock().as_ref() {
            before_watch();
        }
        #[cfg(test)]
        if self.resources.fail_watches.load(Ordering::Relaxed) {
            return Err(SourceError::WatchUnavailable);
        }
        SourceWatch::with_library(path, observation, model, callback)
    }
    /// Read a library through the adapter called `name`.
    pub(crate) fn open_source(&self, name: &str, library: Option<&str>, cancel: &Cancellation) -> Result<(), EngineError> {
        cancel.check()?;
        let search = std::env::var_os("PATH");
        let adapter =
            SourceAdapter::find(name, self.built_in_sources.as_deref(), search.as_deref()).map_err(adapter_error)?;
        self.open_adapter(adapter, library, cancel).map(|_| ())
    }

    /// Read `library` through `adapter` and publish it; its id and the
    /// adapter's own change token. A read whose scope changed replaces the library it read
    /// before, so no earlier handle finds a record in it. A failure publishes
    /// nothing.
    pub fn open_adapter(
        &self,
        adapter: SourceAdapter,
        library: Option<&str>,
        cancel: &Cancellation,
    ) -> Result<(LibraryId, String), EngineError> {
        cancel.check()?;
        let load = adapter
            .read(library, &|| cancel.is_cancelled())
            .map_err(adapter_error)?;
        cancel.check()?;
        let _operation = self.resources.operation.recover_lock();
        let mut loaded = load.loaded;
        loaded.snapshot.reuse_unchanged(self.snapshot()?.as_ref());
        let mut adapters = self.resources.adapters.recover_lock();
        let previous = adapters
            .iter()
            .find(|(id, (old, name))| **id != loaded.library && *old == adapter && name.as_deref() == library)
            .map(|(id, _)| id.clone());
        if previous.is_none() && !adapters.contains_key(&loaded.library) && adapters.len() >= MAX_FILES {
            return Err(error(ErrorCode::ResourceLimit));
        }
        self.shared.install(info(&loaded), &loaded.snapshot, previous.clone())?;
        if let Some(previous) = previous {
            adapters.remove(&previous);
        }
        adapters.insert(loaded.library.clone(), (adapter, library.map(str::to_owned)));
        Ok((loaded.library, load.token))
    }

    /// Read every adapter library again, or only `library`: a failure keeps
    /// the last good read and reports it in that library's health.
    fn reload_adapters(&self, library: Option<&LibraryId>, cancel: &Cancellation) -> Result<bool, EngineError> {
        let chosen: Vec<_> = self
            .resources
            .adapters
            .recover_lock()
            .iter()
            .filter(|(id, _)| library.is_none_or(|wanted| wanted == *id))
            .map(|(id, (adapter, name))| (id.clone(), adapter.clone(), name.clone()))
            .collect();
        let found = !chosen.is_empty();
        for (id, adapter, name) in chosen {
            if let Err(failure) = self.open_adapter(adapter, name.as_deref(), cancel) {
                if failure.code == ErrorCode::Cancelled {
                    return Err(failure);
                }
                self.shared.failed(&id, failure.clone())?;
                if library.is_some() {
                    return Err(failure);
                }
            }
        }
        Ok(found)
    }

    pub(crate) fn reload_files(&self, library: Option<&LibraryId>, cancel: &Cancellation) -> Result<(), EngineError> {
        if self.reload_adapters(library, cancel)? && library.is_some() {
            return Ok(());
        }
        let loaded_zotero = self
            .resources
            .zotero
            .recover_lock()
            .current()
            .iter()
            .any(|loaded| library.is_none_or(|id| *id == loaded.library));
        if loaded_zotero {
            let result = self.read_zotero(true, None, None, cancel);
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
            .recover_lock()
            .slots
            .iter()
            .filter(|(_, slot)| library.is_none_or(|id| slot.loaded.library == *id))
            .map(|(slot, source)| (*slot, source.generation, source.path.clone()))
            .collect();
        if library.is_some() && paths.is_empty() {
            return Err(error(ErrorCode::SourceUnavailable));
        }
        for (slot, generation, path) in paths {
            let mut reload = Err(error(ErrorCode::Cancelled));
            // A watcher publishing first retires this read, not the person's
            // request: read again, so Reload still ends on what the file holds.
            for _ in 0..MAX_RELOAD_ATTEMPTS {
                cancel.check()?;
                reload = self
                    .prepare_file_reload_expected(&path, Some((slot, generation)), cancel)
                    .and_then(|prepared| self.accept_file_reload(prepared));
                if !matches!(&reload, Err(failure) if failure.code == ErrorCode::Cancelled) {
                    break;
                }
            }
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
        profile: Option<&str>,
        cancel: &Cancellation,
    ) -> Result<(), EngineError> {
        self.read_zotero_observed(reload, libraries, profile, cancel, || Ok(()))
    }

    fn read_zotero_observed(
        &self,
        reload: bool,
        libraries: Option<&[ZoteroLibrary]>,
        profile: Option<&str>,
        cancel: &Cancellation,
        mut on_begin: impl FnMut() -> Result<(), EngineError>,
    ) -> Result<(), EngineError> {
        cancel.check()?;
        let _operation = self.resources.operation.recover_lock();
        let mut source = self.resources.zotero.recover_lock();
        let wanted: Vec<Library> = match libraries {
            Some(libraries) => libraries.to_vec(),
            None => source.libraries(),
        };
        let profile = profile.or(source.profile()).map(str::to_owned);
        let mut chosen = (source.libraries() != wanted || source.profile() != profile.as_deref())
            .then(|| {
                source
                    .choosing(&wanted)
                    .map(|new| new.with_profile(profile))
                    .map_err(|_| error(ErrorCode::BadInput))
            })
            .transpose()?;
        let reload = reload || !self.state()?.zotero_ready;
        let mut guard_error = None;
        let mut began = false;
        let mut on_full = || {
            if !began {
                if let Err(failure) = on_begin().and_then(|()| self.shared.begin_zotero_read()) {
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
                        info.zotero = Some(library);
                        (info, &loaded.snapshot)
                    })
                    .collect();
                self.shared.publish(entries, |old| old.kind == LibraryKind::Zotero)?;
                if let Some(chosen) = chosen {
                    *source = chosen;
                }
                *self.resources.closed_groups.recover_lock() = source.closed_groups().map(<[_]>::to_vec);
                Ok(())
            }
            Err(failure) => {
                let failure = zotero_error(failure);
                let code = failure.code;
                // Any failed read retires prepared Zotero copies until a read
                // succeeds, even when only a new choice failed: its full read
                // already began, and the copy guard does not tell them apart.
                if code != ErrorCode::Cancelled {
                    if !began {
                        on_begin()?;
                        self.shared.begin_zotero_read()?;
                    }
                    // A new choice that fails says nothing about the old one.
                    if chosen.is_none() {
                        for previous in source.current() {
                            let _ = self.shared.failed(&previous.library, failure);
                        }
                    }
                }
                Err(failure)
            }
        }
    }
    /// What reads the chosen Zotero libraries' attachments, and the port
    /// Zotero answers on while it runs.
    pub(super) fn zotero_reading(&self) -> Result<(ibis_sources::zotero_source::ZoteroReading, u16), EngineError> {
        let reading = self.resources.zotero.recover_lock().reading();
        let port = self
            .resources
            .zotero_port
            .unwrap_or(ibis_sources::zotero_source::DEFAULT_PORT);
        Ok((reading, port))
    }
    /// The Zotero profiles here by name, for a choice while Zotero is closed.
    pub(super) fn zotero_profiles(&self) -> Result<Vec<String>, EngineError> {
        ibis_sources::zotero_source::profiles().map_err(|_| error(ErrorCode::SourceUnavailable))
    }
    /// Group libraries by name. Zotero answers while it runs; a closed Zotero
    /// has only what its last read listed, which never starts another read.
    pub(super) fn zotero_groups(&self) -> Result<Vec<ZoteroGroup>, EngineError> {
        let port = self
            .resources
            .zotero_port
            .unwrap_or(ibis_sources::zotero_source::DEFAULT_PORT);
        let groups = match ibis_sources::zotero_source::live_groups(port) {
            Ok(groups) => groups,
            Err(ZoteroError::Live(LiveError::Transport(TransportError::Unreachable))) => {
                let closed = self.resources.closed_groups.recover_lock();
                closed.clone().ok_or_else(|| error(ErrorCode::SourceUnavailable))?
            }
            Err(failure) => {
                return Err(EngineError {
                    code: ErrorCode::SourceUnavailable,
                    reason: failure.reason(),
                    detail: None,
                    strict_cause: None,
                    citation_error: None,
                });
            }
        };
        Ok(groups
            .into_iter()
            .map(|group| ZoteroGroup {
                id: group.id,
                name: group.name,
            })
            .collect())
    }
}
impl Shared {
    fn file_changed(
        &self,
        id: u64,
        generation: u64,
        candidate: Result<LoadedLibrary, SourceError>,
    ) -> Option<Snapshot> {
        let mut registry = self.sources.recover_lock();
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
                    // The whole error, so a refresh says why as an open would.
                    let _ = self.failed(&previous, source_error(failure));
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
        let _publication = self.publication.recover_lock();
        let old = self.state_view()?;
        // No source observation completed: observers must not advance their
        // successful-check time just because watch health changed.
        let mut next = old.with_failure(library, error(ErrorCode::SourceUnavailable))?;
        next.libraries
            .get_mut(library)
            .ok_or_else(|| error(ErrorCode::NotFound))?
            .watch_warning = Some(WatchWarning::LiveRefreshOff);
        *self.state.write().map_err(|_| error(ErrorCode::Internal))? = Arc::new(next);
        self.notify()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn incompatible_newer_zotero_schema_names_only_the_fixed_table_in_json() {
        let failure = zotero_error(ZoteroError::Closed(ibis_sources::zotero_source::ClosedError::TooNew {
            table: ZoteroTable::FieldsCombined,
        }));
        assert_eq!(
            serde_json::to_value(failure).unwrap(),
            serde_json::json!({
                "code":"newer-format", "detail":{"table":"fieldsCombined"},
                "message":{"id":"error-newer-format","args":{}},
                "recovery":{"action":"upgrade-ibis"},"focus":"recovery-action"
            })
        );
    }

    use super::*;

    #[test]
    fn owner_probe_refusal_names_sharing_off_and_the_same_engine_recovers() {
        use ibis_sources::zotero_source::discovery::{DiscoveryRoots, Platform, discover};
        let home = tempfile::tempdir().unwrap();
        let roots = DiscoveryRoots::new(Platform::Linux, home.path().into(), None);
        let registry = home.path().join(".zotero/zotero");
        std::fs::create_dir_all(&registry).unwrap();
        std::fs::write(registry.join("profiles.ini"), "invalid registry").unwrap();
        let unresolved = discover(&roots);
        let mut ini = String::new();
        for n in 0..2 {
            let profile = registry.join(format!("p{n}"));
            std::fs::create_dir_all(&profile).unwrap();
            std::fs::write(profile.join("prefs.js"), format!("user_pref(\"extensions.zotero.useDataDir\", true);\nuser_pref(\"extensions.zotero.dataDir\", \"/invented/directory{n}\");")).unwrap();
            ini.push_str(&format!("[Profile{n}]\nIsRelative=1\nPath=p{n}\n"));
        }
        std::fs::write(registry.join("profiles.ini"), ini).unwrap();
        for found in [unresolved, discover(&roots)] {
            let server = zotero_fixture::Server::new();
            let engine = Engine::with_zotero_port(server.port).unwrap();
            engine.resources.zotero.recover_lock().use_discovery(&found);
            let cancel = Cancellation::default();
            let open = serde_json::json!({"action":"open-zotero"});
            server.sharing_off.store(true, Ordering::SeqCst);
            let failure = engine.dispatch(Method::Libraries, &open, &cancel).unwrap_err();
            assert_eq!(failure.reason, Some(ibis_core::contract::ErrorReason::ZoteroSharingOff));
            server.sharing_off.store(false, Ordering::SeqCst);
            engine.dispatch(Method::Libraries, &open, &cancel).unwrap();
            let libraries = engine.libraries().unwrap();
            assert_eq!(libraries.libraries[0].objects, 1);
            assert!(libraries.libraries[0].refresh_error.is_none());
            assert!(libraries.libraries[0].refresh_reason.is_none());
        }
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
        assert!(engine.resources.watches.recover_lock().is_empty());
        let listed = engine
            .dispatch(Method::Libraries, &json!({"action":"list"}), &cancel)
            .unwrap();
        assert_eq!(listed.exit_code, ExitCode::Ok);
        assert_eq!(listed.data["libraries"][0]["watch_warning"], "live-refresh-off");
        let cited = engine
            .cite_keys_checked(
                &KeyCitationRequest {
                    keys: vec!["demo".into()],
                    output: OutputSpec::Key {
                        syntax: KeySyntax::Pandoc,
                        command: None,
                    },
                    form: CitationForm::Normal,
                    strict: false,
                    output_locale: None,
                    keep_order: false,
                },
                &cancel,
            )
            .unwrap();
        assert_eq!(
            DispatchResult::citation(cited, 1, &OutputSpec::Preset { name: "pandoc".into() })
                .unwrap()
                .wire()
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
            .recover_lock()
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
            let registry = engine.shared.sources.recover_lock();
            let slot = registry.slots.get(&id).unwrap();
            (slot.generation, slot.loaded.clone())
        };
        let mut same = (*prior).clone();
        same.warnings = vec![
            ibis_sources::SourceWarning {
                code: ibis_sources::WarningCode::Field,
                line: None,
                key: None,
                entry: None,
                cause: None,
                field: None,
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
        let registry = engine.shared.sources.recover_lock();
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
