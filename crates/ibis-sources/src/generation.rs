//! Private immutable source generations. Storage never grants citation readiness.
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

use ibis_core::contract::{Fingerprint, Fingerprints, ItemRef, LibraryId, LibraryObject, SnapshotObject};
use ibis_zotero_local::closed::{PrivateDirectory, PrivateStorageError};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use thiserror::Error;

const FORMAT: u32 = 1;
const MAGIC: &[u8; 8] = b"IBISGEN1";
const MAX_OBJECT: u64 = 2 * 1024 * 1024;
const MAX_INDEX: u64 = 64 * 1024 * 1024;
// Bound decoded String slots separately from their serialized byte lengths.
const MAX_METADATA_SLOTS: usize = 1_000_000;
const MAX_GENERATION: u64 = 256 * 1024 * 1024;
const MAX_MANIFEST: u64 = 8 * 1024;
const MAX_FILES: usize = 512;
const MAX_STORE: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Copy)]
struct StoreLimits {
    files: usize,
    bytes: u64,
}
impl Default for StoreLimits {
    fn default() -> Self {
        Self {
            files: MAX_FILES,
            bytes: MAX_STORE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum StoreError {
    #[error("prepared storage is unavailable")]
    Unavailable,
    #[error("prepared storage is already in use")]
    Busy,
    #[error("prepared storage is invalid")]
    Invalid,
    #[error("prepared storage exceeded its limit")]
    Limit,
    #[error("prepared source operation was superseded")]
    Stale,
    #[error("prepared source session is closed")]
    Closed,
}
impl From<PrivateStorageError> for StoreError {
    fn from(error: PrivateStorageError) -> Self {
        match error {
            PrivateStorageError::Busy => Self::Busy,
            PrivateStorageError::Unavailable => Self::Unavailable,
        }
    }
}

/// Display metadata only; canonical citation data remains in the object section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchSummary<'a> {
    pub id: &'a ItemRef,
    pub fingerprints: &'a Fingerprints,
    pub title: &'a str,
    pub authors: &'a [String],
    pub year: Option<i32>,
    pub container: Option<&'a str>,
    pub item_type: &'a str,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DisplayFields {
    title: String,
    #[serde(deserialize_with = "bounded_names")]
    authors: Vec<String>,
    year: Option<i32>,
    container: Option<String>,
    item_type: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    id: ItemRef,
    fingerprints: Fingerprints,
    #[serde(deserialize_with = "bounded_names")]
    keys: Vec<String>,
    summary: Option<DisplayFields>,
    offset: u64,
    len: u64,
    hash: Fingerprint,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    format: u32,
    library: LibraryId,
    observation: Fingerprint,
    #[serde(deserialize_with = "bounded_rows")]
    rows: Vec<Row>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: u32,
    library: LibraryId,
    file: String,
    index_start: u64,
    index_len: u64,
    index_hash: Fingerprint,
}

#[derive(Default)]
struct State {
    closed: bool,
    next: u64,
    operations: BTreeMap<LibraryId, u64>,
    readers: Vec<Weak<PreparedLibrary>>,
    stages: BTreeMap<u64, StageReservation>,
    usage: Option<Usage>,
}
#[derive(Clone, Copy, Default)]
struct Usage {
    files: usize,
    bytes: u64,
}
impl Usage {
    fn plus(self, other: Self) -> Result<Self, StoreError> {
        Ok(Self {
            files: self.files.checked_add(other.files).ok_or(StoreError::Limit)?,
            bytes: self.bytes.checked_add(other.bytes).ok_or(StoreError::Limit)?,
        })
    }
    fn fits(self, limits: StoreLimits) -> bool {
        self.files <= limits.files && self.bytes <= limits.bytes
    }
}
struct StageReservation {
    path: PathBuf,
    bytes: u64,
}
struct Inner {
    directory: Arc<PrivateDirectory>,
    state: Mutex<State>,
    limits: StoreLimits,
    #[cfg(test)]
    fail_manifest: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    fail_write: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    fail_reclaim: std::sync::atomic::AtomicBool,
}

/// One publication authority/session. Writers hold only a weak reference to it.
pub struct GenerationStore {
    inner: Arc<Inner>,
}
pub struct GenerationWriter {
    stage: OwnedStage,
    index: Index,
    ids: BTreeSet<ItemRef>,
    metadata_slots: usize,
    bytes: u64,
    failed: bool,
}
pub struct StagedGeneration {
    stage: OwnedStage,
    index: Index,
    index_start: u64,
    index_len: u64,
    index_hash: Fingerprint,
}
// The file and its reservation move together from writer to staged generation.
// Only this handle may release its stage; collection preserves every live handle,
// including handles whose publication operation has since been superseded.
struct OwnedStage {
    owner: Weak<Inner>,
    operation: u64,
    file: Option<NamedTempFile>,
}
/// Readers hold immutable metadata and a generation-specific file, not an authority.
pub struct PreparedLibrary {
    _directory: Arc<PrivateDirectory>,
    path: PathBuf,
    file: Mutex<File>,
    index: Index,
}

fn manifest_name(library: &LibraryId) -> String {
    format!(
        "manifest-{}.json",
        Fingerprint::of_bytes(library.as_str().as_bytes()).as_str()
    )
}
fn generation_name(name: &str) -> bool {
    name.starts_with("generation-") && name.len() <= 80 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
fn stage_name(name: &str) -> bool {
    (name.starts_with("stage-") || name.starts_with("manifest-stage-"))
        && name.len() <= 80
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
fn authority(owner: &Weak<Inner>, library: &LibraryId, operation: u64) -> Result<Arc<Inner>, StoreError> {
    let inner = owner.upgrade().ok_or(StoreError::Closed)?;
    {
        let state = inner.state.lock().map_err(|_| StoreError::Unavailable)?;
        active(&state, library, operation)?;
    }
    Ok(inner)
}
fn active(state: &State, library: &LibraryId, operation: u64) -> Result<(), StoreError> {
    if state.closed {
        return Err(StoreError::Closed);
    }
    if state.operations.get(library) != Some(&operation) {
        return Err(StoreError::Stale);
    }
    Ok(())
}

impl GenerationStore {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        Ok(Self {
            inner: Arc::new(Inner {
                directory: Arc::new(PrivateDirectory::open(path)?),
                state: Mutex::new(State::default()),
                limits: StoreLimits::default(),
                #[cfg(test)]
                fail_manifest: std::sync::atomic::AtomicBool::new(false),
                #[cfg(test)]
                fail_write: std::sync::atomic::AtomicBool::new(false),
                #[cfg(test)]
                fail_reclaim: std::sync::atomic::AtomicBool::new(false),
            }),
        })
    }
    pub fn close(&self) -> Result<(), StoreError> {
        self.inner.state.lock().map_err(|_| StoreError::Unavailable)?.closed = true;
        Ok(())
    }
    pub fn begin(&self, library: LibraryId, observation: Fingerprint) -> Result<GenerationWriter, StoreError> {
        let mut state = self.inner.state.lock().map_err(|_| StoreError::Unavailable)?;
        if state.closed {
            return Err(StoreError::Closed);
        }
        if !state.operations.contains_key(&library) && state.operations.len() >= 64 {
            return Err(StoreError::Limit);
        }
        self.inner.collect(&mut state)?;
        state.next = state.next.checked_add(1).ok_or(StoreError::Limit)?;
        let operation = state.next;
        self.inner.reserve(&mut state, Usage { files: 1, bytes: 0 })?;
        let file = tempfile::Builder::new()
            .prefix("stage-")
            .disable_cleanup(true)
            .tempfile_in(self.inner.directory.path())
            .map_err(|_| {
                state.usage = None;
                StoreError::Unavailable
            })?;
        state.stages.insert(
            operation,
            StageReservation {
                path: file.path().to_path_buf(),
                bytes: 0,
            },
        );
        state.operations.insert(library.clone(), operation);
        let mut stage = OwnedStage {
            owner: Arc::downgrade(&self.inner),
            operation,
            file: Some(file),
        };
        drop(state);
        self.inner.directory.verify_file(stage.file()?.path())?;
        stage.write(&library, MAGIC, false)?;
        Ok(GenerationWriter {
            stage,
            index: Index {
                format: FORMAT,
                library,
                observation,
                rows: vec![],
            },
            ids: BTreeSet::new(),
            metadata_slots: 0,
            bytes: MAGIC.len() as u64,
            failed: false,
        })
    }

    pub fn publish(&self, mut staged: StagedGeneration) -> Result<Arc<PreparedLibrary>, StoreError> {
        if !Weak::ptr_eq(&staged.stage.owner, &Arc::downgrade(&self.inner)) {
            return Err(StoreError::Stale);
        }
        let mut state = self.inner.state.lock().map_err(|_| StoreError::Unavailable)?;
        active(&state, &staged.index.library, staged.stage.operation)?;
        self.inner.directory.verify_file(staged.stage.file()?.path())?;
        let suffix = staged
            .stage
            .file()?
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_prefix("stage-"))
            .ok_or(StoreError::Invalid)?;
        let name = format!("generation-{suffix}");
        if !generation_name(&name) {
            return Err(StoreError::Invalid);
        }
        let path = self.inner.directory.path().join(&name);
        let manifest = Manifest {
            format: FORMAT,
            library: staged.index.library.clone(),
            file: name,
            index_start: staged.index_start,
            index_len: staged.index_len,
            index_hash: staged.index_hash,
        };
        let bytes = encoded(&manifest, MAX_MANIFEST)?;
        // One scratch slot covers both persist_noclobber's possible link/unlink
        // fallback and the later manifest temporary. Replacing an existing
        // manifest still needs room for its complete replacement before commit.
        self.inner.reserve(
            &mut state,
            Usage {
                files: 1,
                bytes: bytes.len() as u64,
            },
        )?;
        state.usage = None; // Any I/O failure below is remeasured before another reservation.
        let temporary = staged.stage.file.take().ok_or(StoreError::Invalid)?;
        let file = match temporary.persist_noclobber(&path) {
            Ok(file) => file,
            Err(error) => {
                staged.stage.file = Some(error.file);
                return Err(StoreError::Unavailable);
            }
        };
        state.stages.remove(&staged.stage.operation);
        // The link/unlink fallback must have removed the old stage name before
        // its scratch slot can be reused by the manifest temporary.
        let result = self
            .inner
            .directory
            .verify_file(&path)
            .map_err(StoreError::from)
            .and_then(|_| self.write_manifest(&manifest, &bytes));
        if let Err(error) = result {
            // This unique file has never been published or handed to a reader.
            drop(file);
            if self.inner.directory.verify_file(&path).is_ok() {
                let _ = std::fs::remove_file(&path);
            }
            return Err(error);
        }
        let generation = Arc::new(PreparedLibrary {
            _directory: self.inner.directory.clone(),
            path,
            file: Mutex::new(file),
            index: staged.index,
        });
        state.readers.push(Arc::downgrade(&generation));
        Ok(generation)
    }

    fn write_manifest(&self, manifest: &Manifest, bytes: &[u8]) -> Result<(), StoreError> {
        #[cfg(test)]
        if self.inner.fail_manifest.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(StoreError::Unavailable);
        }
        let name = manifest_name(&manifest.library);
        // Refuse links, directories or unsafe files before replacing our manifest.
        self.inner.directory.open_file(&name)?;
        let mut temporary = tempfile::Builder::new()
            .prefix("manifest-stage-")
            .tempfile_in(self.inner.directory.path())
            .map_err(|_| StoreError::Unavailable)?;
        self.inner.directory.verify_file(temporary.path())?;
        temporary
            .write_all(bytes)
            .and_then(|_| temporary.as_file().sync_all())
            .map_err(|_| StoreError::Unavailable)?;
        self.inner.directory.verify()?;
        temporary
            .persist(self.inner.directory.path().join(name))
            .map_err(|_| StoreError::Unavailable)?;
        // The rename is the commit point. Directory flushing is best effort as
        // in settings: losing this reconstructible cache after a power cut is a miss.
        #[cfg(unix)]
        {
            let _ = File::open(self.inner.directory.path()).and_then(|directory| directory.sync_all());
        }
        Ok(())
    }

    pub fn open_library(&self, library: &LibraryId) -> Result<Option<Arc<PreparedLibrary>>, StoreError> {
        let mut state = self.inner.state.lock().map_err(|_| StoreError::Unavailable)?;
        if state.closed {
            return Err(StoreError::Closed);
        }
        let Some(manifest_file) = self.inner.directory.open_file(&manifest_name(library))? else {
            return Ok(None);
        };
        let manifest: Manifest =
            serde_json::from_slice(&read_bounded(manifest_file, MAX_MANIFEST)?).map_err(|_| StoreError::Invalid)?;
        if manifest.format != FORMAT {
            return Ok(None);
        }
        if &manifest.library != library || !generation_name(&manifest.file) || manifest.index_len > MAX_INDEX {
            return Err(StoreError::Invalid);
        }
        let mut file = self
            .inner
            .directory
            .open_file(&manifest.file)?
            .ok_or(StoreError::Invalid)?;
        let size = file.metadata().map_err(|_| StoreError::Unavailable)?.len();
        if size > MAX_GENERATION
            || manifest.index_start < MAGIC.len() as u64
            || manifest.index_start.checked_add(manifest.index_len) != Some(size)
        {
            return Err(StoreError::Invalid);
        }
        let mut magic = [0; 8];
        file.read_exact(&mut magic).map_err(|_| StoreError::Invalid)?;
        if &magic != MAGIC {
            return Err(StoreError::Invalid);
        }
        file.seek(SeekFrom::Start(manifest.index_start))
            .map_err(|_| StoreError::Unavailable)?;
        let bytes = read_bounded(&mut file, manifest.index_len)?;
        if bytes.len() as u64 != manifest.index_len || Fingerprint::of_bytes(&bytes) != manifest.index_hash {
            return Err(StoreError::Invalid);
        }
        let index: Index = serde_json::from_slice(&bytes).map_err(|_| StoreError::Invalid)?;
        if index.format != FORMAT {
            return Ok(None);
        }
        validate_index(&index, &manifest)?;
        let generation = Arc::new(PreparedLibrary {
            _directory: self.inner.directory.clone(),
            path: self.inner.directory.path().join(&manifest.file),
            file: Mutex::new(file),
            index,
        });
        state.readers.push(Arc::downgrade(&generation));
        Ok(Some(generation))
    }
}

impl Inner {
    // Caller holds the session lock and has checked its operation (or is begin).
    // Successful reservations remain charged until their file is removed or the
    // next bounded collection measures it. No file write runs outside this lock.
    fn reserve(&self, state: &mut State, extra: Usage) -> Result<(), StoreError> {
        if state.usage.is_none() {
            self.collect(state)?;
        }
        let mut total = state.usage.ok_or(StoreError::Unavailable)?.plus(extra)?;
        if !total.fits(self.limits) {
            self.collect(state)?;
            total = state.usage.ok_or(StoreError::Unavailable)?.plus(extra)?;
        }
        if !total.fits(self.limits) {
            return Err(StoreError::Limit);
        }
        state.usage = Some(total);
        Ok(())
    }

    /// Bound traversal even when over capacity. Never delete a generation without
    /// seeing every manifest, but abandoned stages need no manifest references.
    fn collect(&self, state: &mut State) -> Result<(), StoreError> {
        state.usage = None;
        self.directory.verify()?;
        state.readers.retain(|reader| reader.strong_count() != 0);
        let mut retained: BTreeSet<PathBuf> = state
            .readers
            .iter()
            .filter_map(Weak::upgrade)
            .map(|reader| reader.path.clone())
            .collect();
        let scan_limit = self.limits.files.saturating_mul(2);
        let entries: Vec<_> = std::fs::read_dir(self.directory.path())
            .map_err(|_| StoreError::Unavailable)?
            .take(scan_limit.saturating_add(1))
            .collect::<Result<_, _>>()
            .map_err(|_| StoreError::Unavailable)?;
        // An unrecognized manifest prevents cleanup: it may belong to a newer store.
        let complete = entries.len() <= scan_limit;
        let mut cleanup = complete;
        for entry in &entries {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("manifest-") && name.ends_with(".json") {
                let parsed = self
                    .directory
                    .open_file(&name)?
                    .map(|file| read_bounded(file, MAX_MANIFEST))
                    .transpose()?
                    .and_then(|bytes| serde_json::from_slice::<Manifest>(&bytes).ok());
                if let Some(manifest) = parsed.filter(|m| m.format == FORMAT && generation_name(&m.file)) {
                    retained.insert(self.directory.path().join(manifest.file));
                } else {
                    cleanup = false;
                }
            }
        }
        let mut usage = Usage::default();
        let mut seen_stages = BTreeSet::new();
        for entry in entries {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let live_stage = state.stages.iter_mut().find(|(_, stage)| stage.path == path);
            if let Some((operation, stage)) = live_stage {
                // Include capacity already reserved for a not-yet-complete write.
                stage.bytes = stage.bytes.max(
                    std::fs::symlink_metadata(&path)
                        .map_err(|_| StoreError::Unavailable)?
                        .len(),
                );
                usage = usage.plus(Usage {
                    files: 1,
                    bytes: stage.bytes,
                })?;
                seen_stages.insert(*operation);
                continue;
            }
            let abandoned = stage_name(&name);
            if (abandoned || (cleanup && generation_name(&name) && !retained.contains(&path)))
                && let Some(mut file) = self.directory.open_file(&name)?
            {
                let mut magic = [0; 8];
                let size = file.metadata().map_err(|_| StoreError::Unavailable)?.len();
                let len = size.min(MAGIC.len() as u64) as usize;
                file.read_exact(&mut magic[..len])
                    .map_err(|_| StoreError::Unavailable)?;
                // A crash can precede or interrupt the header. Manifest stages
                // have no header/readers and are private to the held lease.
                let owned = if name.starts_with("manifest-stage-") {
                    size <= MAX_MANIFEST
                } else if abandoned {
                    magic[..len] == MAGIC[..len]
                } else {
                    len == MAGIC.len() && &magic == MAGIC
                };
                if owned {
                    drop(file);
                    self.remove(&path)?;
                    continue;
                }
            }
            usage = usage.plus(Usage {
                files: 1,
                bytes: std::fs::symlink_metadata(&path)
                    .map_err(|_| StoreError::Unavailable)?
                    .len(),
            })?;
        }
        if !complete {
            state.usage = None;
            return Err(StoreError::Limit);
        }
        for (operation, stage) in &state.stages {
            if !seen_stages.contains(operation) {
                usage = usage.plus(Usage {
                    files: 1,
                    bytes: stage.bytes,
                })?;
            }
        }
        state.usage = Some(usage);
        if !usage.fits(self.limits) {
            return Err(StoreError::Limit);
        }
        Ok(())
    }

    fn remove(&self, path: &Path) -> Result<(), StoreError> {
        self.directory.verify_file(path)?;
        #[cfg(test)]
        if self.fail_reclaim.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(StoreError::Unavailable);
        }
        std::fs::remove_file(path).map_err(|_| StoreError::Unavailable)
    }
}

impl OwnedStage {
    fn file(&self) -> Result<&NamedTempFile, StoreError> {
        self.file.as_ref().ok_or(StoreError::Invalid)
    }

    fn write(&mut self, library: &LibraryId, bytes: &[u8], sync: bool) -> Result<(), StoreError> {
        let inner = self.owner.upgrade().ok_or(StoreError::Closed)?;
        let mut state = inner.state.lock().map_err(|_| StoreError::Unavailable)?;
        active(&state, library, self.operation)?;
        inner.reserve(
            &mut state,
            Usage {
                files: 0,
                bytes: bytes.len() as u64,
            },
        )?;
        let reservation = state.stages.get_mut(&self.operation).ok_or(StoreError::Invalid)?;
        reservation.bytes = reservation
            .bytes
            .checked_add(bytes.len() as u64)
            .ok_or(StoreError::Limit)?;
        let file = self.file.as_mut().ok_or(StoreError::Invalid)?;
        #[cfg(test)]
        if inner.fail_write.load(std::sync::atomic::Ordering::Relaxed) {
            file.write_all(&bytes[..bytes.len().min(3)])
                .map_err(|_| StoreError::Unavailable)?;
            return Err(StoreError::Unavailable);
        }
        file.write_all(bytes).map_err(|_| StoreError::Unavailable)?;
        if sync {
            file.as_file().sync_all().map_err(|_| StoreError::Unavailable)?;
        }
        Ok(())
    }

    fn cancel(&mut self) {
        let Some(file) = self.file.take() else { return };
        // Do not keep a dead authority/lease alive. Its next session will reclaim
        // the abandoned file. Cleanup here only releases this handle's own stage;
        // stale/closed handles never run collection or change other reservations.
        if let Some(inner) = self.owner.upgrade()
            && let Ok(mut state) = inner.state.lock()
        {
            state.stages.remove(&self.operation);
            state.usage = None;
            if inner.directory.verify_file(file.path()).is_ok() {
                let _ = file.close();
            }
        }
    }
}
impl Drop for OwnedStage {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl GenerationWriter {
    pub fn push(&mut self, object: LibraryObject) -> Result<(), StoreError> {
        let result = self.push_inner(object);
        self.failed |= result.is_err();
        if result.is_err() {
            self.stage.cancel();
        }
        result
    }
    fn push_inner(&mut self, object: LibraryObject) -> Result<(), StoreError> {
        // Validate recursive payloads before any rejection can drop them.
        let object = SnapshotObject::new(object).map_err(|_| StoreError::Invalid)?;
        authority(&self.stage.owner, &self.index.library, self.stage.operation)?;
        if self.failed
            || self.index.rows.len() >= crate::MAX_LIBRARY_RECORDS
            || object.object.id().library != self.index.library
            || !self.ids.insert(object.object.id().clone())
        {
            return Err(StoreError::Invalid);
        }
        let bytes = encoded(&object, MAX_OBJECT)?;
        let end = self
            .bytes
            .checked_add(bytes.len() as u64)
            .filter(|end| *end <= MAX_GENERATION)
            .ok_or(StoreError::Limit)?;
        let (keys, summary) = metadata(&object);
        if keys.len() > 256 || summary.as_ref().is_some_and(|summary| summary.authors.len() > 256) {
            return Err(StoreError::Limit);
        }
        let row = Row {
            id: object.object.id().clone(),
            fingerprints: object.fingerprints.clone(),
            keys,
            summary,
            offset: self.bytes,
            len: bytes.len() as u64,
            hash: Fingerprint::of_bytes(&bytes),
        };
        charge_metadata(&mut self.metadata_slots, &row)?;
        self.index.rows.push(row);
        self.stage.write(&self.index.library, &bytes, false)?;
        self.bytes = end;
        Ok(())
    }
    pub fn finish(mut self) -> Result<StagedGeneration, StoreError> {
        if self.failed {
            return Err(StoreError::Invalid);
        }
        authority(&self.stage.owner, &self.index.library, self.stage.operation)?;
        self.index.rows.sort_by(|a, b| a.id.cmp(&b.id));
        let bytes = encoded(&self.index, MAX_INDEX)?;
        if self
            .bytes
            .checked_add(bytes.len() as u64)
            .is_none_or(|size| size > MAX_GENERATION)
        {
            return Err(StoreError::Limit);
        }
        self.stage.write(&self.index.library, &bytes, true)?;
        authority(&self.stage.owner, &self.index.library, self.stage.operation)?;
        Ok(StagedGeneration {
            stage: self.stage,
            index: self.index,
            index_start: self.bytes,
            index_len: bytes.len() as u64,
            index_hash: Fingerprint::of_bytes(&bytes),
        })
    }
}

impl PreparedLibrary {
    pub fn library(&self) -> &LibraryId {
        &self.index.library
    }
    /// An observation is source metadata, never proof that a source is ready.
    pub fn observation(&self) -> &Fingerprint {
        &self.index.observation
    }
    /// Stored metadata is not citation readiness; selected objects are verified by get.
    pub fn object_fingerprints(&self) -> impl Iterator<Item = (&ItemRef, &Fingerprints)> {
        self.index.rows.iter().map(|row| (&row.id, &row.fingerprints))
    }
    /// Exact keys and aliases, including conflicts, without loading object payloads.
    pub fn key_owners(&self) -> impl Iterator<Item = (&str, &ItemRef)> {
        self.index
            .rows
            .iter()
            .flat_map(|row| row.keys.iter().map(move |key| (key.as_str(), &row.id)))
    }
    pub fn summaries(&self) -> impl Iterator<Item = SearchSummary<'_>> {
        self.index.rows.iter().filter_map(|row| {
            row.summary.as_ref().map(|display| SearchSummary {
                id: &row.id,
                fingerprints: &row.fingerprints,
                title: &display.title,
                authors: &display.authors,
                year: display.year,
                container: display.container.as_deref(),
                item_type: &display.item_type,
            })
        })
    }
    pub fn get(&self, id: &ItemRef) -> Result<Option<SnapshotObject>, StoreError> {
        let Ok(position) = self.index.rows.binary_search_by(|row| row.id.cmp(id)) else {
            return Ok(None);
        };
        let row = &self.index.rows[position];
        let mut file = self.file.lock().map_err(|_| StoreError::Unavailable)?;
        file.seek(SeekFrom::Start(row.offset))
            .map_err(|_| StoreError::Unavailable)?;
        let mut bytes = vec![0; row.len as usize];
        file.read_exact(&mut bytes).map_err(|_| StoreError::Invalid)?;
        if Fingerprint::of_bytes(&bytes) != row.hash {
            return Err(StoreError::Invalid);
        }
        let stored: SnapshotObject = serde_json::from_slice(&bytes).map_err(|_| StoreError::Invalid)?;
        let canonical = SnapshotObject::new(stored.object).map_err(|_| StoreError::Invalid)?;
        if canonical.object.id() != id
            || canonical.fingerprints != stored.fingerprints
            || canonical.fingerprints != row.fingerprints
            || metadata(&canonical) != (row.keys.clone(), row.summary.clone())
        {
            return Err(StoreError::Invalid);
        }
        Ok(Some(canonical))
    }
}

fn validate_index(index: &Index, manifest: &Manifest) -> Result<(), StoreError> {
    if index.library != manifest.library
        || index.rows.len() > crate::MAX_LIBRARY_RECORDS
        || index.rows.windows(2).any(|rows| rows[0].id >= rows[1].id)
    {
        return Err(StoreError::Invalid);
    }
    let mut spans = Vec::with_capacity(index.rows.len());
    for row in &index.rows {
        if row.id.library != index.library
            || row.id.key.trim().is_empty()
            || row.id.key.chars().any(char::is_control)
            || row.len == 0
            || row.len > MAX_OBJECT
            || row.offset < MAGIC.len() as u64
            || row
                .offset
                .checked_add(row.len)
                .is_none_or(|end| end > manifest.index_start)
        {
            return Err(StoreError::Invalid);
        }
        spans.push((row.offset, row.offset + row.len));
    }
    spans.sort_unstable();
    if spans.windows(2).any(|spans| spans[0].1 > spans[1].0) {
        return Err(StoreError::Invalid);
    }
    Ok(())
}

fn metadata(object: &SnapshotObject) -> (Vec<String>, Option<DisplayFields>) {
    let LibraryObject::Record(record) = &object.object else {
        return (vec![], None);
    };
    let text = |field: &str| {
        ibis_core::model::strip_markup(record.csl.get(field).and_then(serde_json::Value::as_str).unwrap_or(""))
    };
    let authors = record
        .csl
        .get("author")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|author| author.get("literal").or_else(|| author.get("family")))
        .filter_map(serde_json::Value::as_str)
        .map(ibis_core::model::strip_markup)
        .collect();
    let year = record
        .csl
        .get("issued")
        .and_then(|date| date.get("date-parts"))
        .and_then(|parts| parts.get(0))
        .and_then(|parts| parts.get(0))
        .and_then(serde_json::Value::as_i64)
        .and_then(|year| i32::try_from(year).ok());
    let keys = record
        .citation_keys
        .iter()
        .map(|key| key.value.clone())
        .chain(record.aliases.iter().cloned())
        .collect();
    let summary = DisplayFields {
        title: text("title"),
        authors,
        year,
        container: ["container-title", "publisher"]
            .into_iter()
            .map(text)
            .find(|text| !text.trim().is_empty()),
        item_type: text("type"),
    };
    (keys, Some(summary))
}

fn bounded_names<'de, D: serde::Deserializer<'de>>(input: D) -> Result<Vec<String>, D::Error> {
    bounded_sequence(input, 256)
}
fn bounded_rows<'de, D: serde::Deserializer<'de>>(input: D) -> Result<Vec<Row>, D::Error> {
    let mut slots = 0;
    bounded_sequence_checked(input, crate::MAX_LIBRARY_RECORDS, |row| {
        charge_metadata(&mut slots, row).is_ok()
    })
}
fn charge_metadata(slots: &mut usize, row: &Row) -> Result<(), StoreError> {
    *slots = slots
        .checked_add(row.keys.len())
        .and_then(|total| total.checked_add(row.summary.as_ref().map_or(0, |summary| summary.authors.len())))
        .filter(|total| *total <= MAX_METADATA_SLOTS)
        .ok_or(StoreError::Limit)?;
    Ok(())
}
fn bounded_sequence<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    input: D,
    limit: usize,
) -> Result<Vec<T>, D::Error> {
    bounded_sequence_checked(input, limit, |_| true)
}
fn bounded_sequence_checked<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    input: D,
    limit: usize,
    check: impl FnMut(&T) -> bool,
) -> Result<Vec<T>, D::Error> {
    struct Visitor<T, F> {
        limit: usize,
        check: F,
        marker: std::marker::PhantomData<T>,
    }
    impl<'de, T: Deserialize<'de>, F: FnMut(&T) -> bool> serde::de::Visitor<'de> for Visitor<T, F> {
        type Value = Vec<T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("bounded prepared metadata")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(mut self, mut sequence: A) -> Result<Vec<T>, A::Error> {
            if sequence.size_hint().is_some_and(|size| size > self.limit) {
                return Err(serde::de::Error::custom("prepared metadata limit"));
            }
            let mut values = Vec::new();
            while let Some(value) = sequence.next_element()? {
                if values.len() == self.limit || !(self.check)(&value) {
                    return Err(serde::de::Error::custom("prepared metadata limit"));
                }
                values.push(value);
            }
            Ok(values)
        }
    }
    input.deserialize_seq(Visitor {
        limit,
        check,
        marker: std::marker::PhantomData,
    })
}

fn read_bounded(file: impl Read, limit: u64) -> Result<Vec<u8>, StoreError> {
    let mut bytes = vec![];
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| StoreError::Unavailable)?;
    if bytes.len() as u64 > limit {
        return Err(StoreError::Limit);
    }
    Ok(bytes)
}
fn encoded(value: &impl Serialize, limit: u64) -> Result<Vec<u8>, StoreError> {
    struct Bounded {
        bytes: Vec<u8>,
        limit: u64,
    }
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.bytes.len() as u64 + bytes.len() as u64 > self.limit {
                return Err(std::io::ErrorKind::FileTooLarge.into());
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut out = Bounded { bytes: vec![], limit };
    serde_json::to_writer(&mut out, value).map_err(|_| StoreError::Limit)?;
    Ok(out.bytes)
}

#[cfg(test)]
mod tests;
