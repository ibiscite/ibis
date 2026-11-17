//! Closed-only Zotero reader. Source files are read as bytes; SQLite opens only
//! a private per-user cache. No source lock is taken, including at startup.
//!
//! Unchanged sources reuse one file set across readers and processes. A cache
//! lease excludes concurrent refresh/query; only a completely published stamp
//! permits reuse. Errors
//! and Debug output never include paths or item text.
// This module owns I/O on Zotero's data folder, as `file::io` does for library files.
#![allow(clippy::disallowed_methods)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub use ibis_core::contract::ZoteroTable;
use thiserror::Error;

mod database;
mod database_bounds;
mod keys;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod mounts;
mod platform;
mod private_copy;

#[cfg(any(target_os = "macos", target_os = "linux"))]
use platform::resident;

/// Fixed reasons suitable for diagnostics. Native errors are deliberately lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ClosedError {
    #[error("Zotero's database was not found")]
    NotFound,
    #[error("Zotero is running; use its local API")]
    Running,
    #[error("Ibis could not establish that Zotero is closed")]
    ProcessCheck,
    #[error("this directory cannot be read safely as a local Zotero database")]
    UnsafeSource,
    #[error("Zotero's database could not be read")]
    Unreadable,
    #[error("private Zotero storage could not be secured")]
    PrivateStorage,
    #[error("the private Zotero lock file's permissions are wrong")]
    LockPermissions,
    #[error("private Zotero storage is already in use")]
    StorageInUse,
    #[error("there is not enough free space for a private Zotero copy")]
    NoSpace,
    #[error("Zotero's database kept changing during three copy attempts")]
    Changing,
    #[error("Zotero's database is damaged; open Zotero to repair it")]
    Damaged,
    #[error("this Zotero database is too old; update Zotero or open it")]
    TooOld,
    #[error("this Zotero database is newer than Ibis knows; update Ibis or open Zotero")]
    TooNew { table: ZoteroTable },
    #[error("Zotero's database exceeds the closed-reader limit")]
    TooLarge,
    #[error("Zotero's database needs journal recovery; open Zotero first")]
    NeedsRecovery,
    #[error("reading the closed Zotero database took too long")]
    TimedOut,
    #[error("reading the closed Zotero database was cancelled")]
    Cancelled,
}

impl ClosedError {
    // Native context must not hide the caller's reason for stopping.
    fn context(self, fallback: Self) -> Self {
        match self {
            Self::Cancelled | Self::TimedOut => self,
            _ => fallback,
        }
    }
}

/// Neutral bibliographic data accepted by the single Zotero mapper (#1195).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct RawItem {
    pub key: String,
    pub item_type: String,
    pub creators: Vec<RawCreator>,
    pub fields: BTreeMap<String, String>,
    /// Keys Better BibTeX keeps outside Zotero's own fields (see [`crate::zotero_local::bbt`]).
    pub better_bibtex_keys: Vec<String>,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct RawCreator {
    pub creator_type: String,
    pub first_name: String,
    pub last_name: String,
    pub name: String,
    pub field_mode: u8,
}

/// Engine-private provenance; deliberately has no serialization implementation.
#[derive(Clone, PartialEq, Eq)]
pub struct SourceMetadata {
    pub item_id: i64,
    pub library_id: i64,
    pub version: i64,
    pub date_added: String,
    pub date_modified: String,
    /// Original text only where multipart decoding changed it. Unchanged text
    /// remains in RawItem.fields; combine both to reconstruct stored strings.
    pub stored_fields: BTreeMap<String, String>,
    /// Exact non-text SQLite storage values, before JavaScript string coercion.
    pub stored_values: BTreeMap<String, StoredValue>,
}

#[derive(Clone, PartialEq, Eq)]
pub enum StoredValue {
    Null,
    Integer(i64),
    RealBits(u64),
}

#[derive(Clone, PartialEq, Eq)]
pub struct ClosedItem {
    pub data: RawItem,
    pub metadata: SourceMetadata,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ClosedLibrary {
    pub library_id: i64,
    /// None denotes the personal library. Group IDs are Zotero's public IDs.
    pub group_id: Option<i64>,
    /// The group's name as Zotero stored it; untrusted, and empty for the
    /// personal library or a name past the size limit.
    pub name: String,
    pub items: Vec<ClosedItem>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ClosedSnapshot {
    pub schema_version: i64,
    pub libraries: Vec<ClosedLibrary>,
    pub warnings: Vec<ClosedWarning>,
    /// Better BibTeX keeps keys this read could not get (an unreadable or
    /// older store, or no record of which Zotero last ran). Citation keys are
    /// then unknown: refuse key output rather than publish "no key".
    pub better_bibtex_unknown: bool,
    /// The personal library's owner as Zotero's local API names it: the
    /// account's user ID, or 0 when none is signed in or when the personal
    /// library holds no top-level item (the API reads it from the first one).
    pub owner: u64,
    /// Attachments of the items read, or `None` when this database's
    /// attachment or full-text tables could not be read. Never fails a read.
    pub attachments: Option<Vec<ClosedAttachment>>,
}

/// One attachment of an item, with the page counts Zotero recorded when it
/// indexed the file (decision 0055). Its title is library text.
#[derive(Clone, PartialEq, Eq)]
pub struct ClosedAttachment {
    /// Check of Zotero's item revision and full-text index metadata.
    pub revision: ibis_core::contract::Fingerprint,
    pub library_id: i64,
    /// The key of the item it belongs to.
    pub parent: String,
    pub key: String,
    pub title: String,
    pub content_type: String,
    /// A link to a web page, with no file and no text.
    pub link_only: bool,
    /// Zotero's record of what it indexed, when it indexed the file.
    pub indexed: Option<IndexedCounts>,
    /// Where Zotero keeps the file: `storage:NAME` in the attachment's
    /// storage folder, or the path of a linked file. Empty when unknown.
    pub path: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct IndexedCounts {
    pub indexed_pages: Option<u32>,
    pub total_pages: Option<u32>,
}

#[derive(Clone, PartialEq, Eq)]
pub enum ClosedWarning {
    NewerThanTested,
    CloudSyncedDirectory,
    /// The bounded source key identifies an omitted item without its contents.
    ItemTooLarge {
        library_id: i64,
        key: String,
    },
}

// A panic or a diagnostic must not dump library content/provenance.
macro_rules! private_debug {
    ($($t:ty),+ $(,)?) => {$ (
        impl std::fmt::Debug for $t {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(concat!(stringify!($t), " { private }") )
            }
        }
    )+};
}
private_debug!(
    RawItem,
    RawCreator,
    SourceMetadata,
    StoredValue,
    ClosedItem,
    ClosedLibrary,
    ClosedSnapshot,
    ClosedWarning,
    ClosedAttachment,
    IndexedCounts
);

/// Reads through a private copy root under `cache`, held only while a read runs.
pub struct ClosedReader {
    cache: PathBuf,
}

impl ClosedReader {
    /// `cache` must be the app's per-user cache parent, never library storage.
    /// Ibis creates its own child with backup exclusion. The caller owns OS paths.
    pub fn new(cache: &Path) -> Self {
        Self {
            cache: cache.to_owned(),
        }
    }

    pub fn read(&self, data: &Path) -> Result<ClosedSnapshot, ClosedError> {
        self.read_cancellable(data, Arc::new(|| false))
    }

    /// One 60-second budget covers private-store setup, native helpers, copying
    /// and private SQLite queries. Cancellation also interrupts helper waits.
    pub fn read_cancellable(
        &self,
        data: &Path,
        cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<ClosedSnapshot, ClosedError> {
        let control = Control::new(cancelled);
        self.read_with_control(data, &control)
    }

    fn read_with_control(&self, data: &Path, control: &Control) -> Result<ClosedSnapshot, ClosedError> {
        control.check()?;
        let cache = self.cache.clone();
        let data = data.to_owned();
        control.io(move |control| private_copy::Store::open(&cache, control)?.read(&data, control))
    }
}

/// The folder Zotero or the person chose, with links in its own path resolved
/// once: only what lies below it is opened without following any link. Windows
/// keeps the path as given, since its verbatim form fails the local-file checks.
pub fn trusted_folder(path: &Path) -> PathBuf {
    // Classify every component and symlink target against the cached mounts
    // before probing it; plain canonicalize could touch a remote root first.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if let Ok(resolved) = platform::directory(path, &Control::within(Arc::new(|| false), LOCAL_FILE_BUDGET)) {
        return resolved;
    }
    path.to_owned()
}

/// The text Zotero cached for attachment `key` of the data directory `data`,
/// or `None` when Zotero cached none. Only a regular local file is read, in
/// at most ten seconds, and the text stays in memory (decision 0036).
pub fn read_text_cache(
    data: &Path,
    key: &str,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<Option<String>, ClosedError> {
    if !crate::zotero_local::fulltext::valid_key(key) {
        return Err(ClosedError::UnsafeSource);
    }
    use std::io::Read;
    let path = trusted_folder(data).join("storage").join(key).join(".zotero-ft-cache");
    let limit = crate::zotero_local::fulltext::MAX_TEXT_BYTES as u64;
    let read = with_local_file(&path, cancelled, LOCAL_FILE_BUDGET, move |file| {
        let mut bytes = Vec::new();
        file.take(limit + 1).read_to_end(&mut bytes).map(|_| bytes)
    })?;
    let Some(bytes) = read else { return Ok(None) };
    let bytes = bytes.map_err(|_| ClosedError::Unreadable)?;
    if bytes.len() as u64 > limit {
        return Err(ClosedError::TooLarge);
    }
    String::from_utf8(bytes).map(Some).map_err(|_| ClosedError::Unreadable)
}

const LOCAL_FILE_BUDGET: Duration = Duration::from_secs(10);

/// Use an opened regular, resident local file without following a link at
/// any component. The operation shares a bounded I/O slot and one deadline
/// with opening; cancelled or late results are dropped, never published.
pub fn with_local_file<T: Send + 'static>(
    path: &Path,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
    budget: Duration,
    operation: impl FnOnce(std::fs::File) -> T + Send + 'static,
) -> Result<Option<T>, ClosedError> {
    let control = Control::within(cancelled, budget.min(LOCAL_FILE_BUDGET));
    let path = path.to_owned();
    control.io(move |control| {
        let file = platform::local_file(&path, control)?;
        control.check()?;
        Ok(file.map(operation))
    })
}

#[derive(Clone)]
struct Control {
    deadline: Instant,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
}
impl Control {
    /// Filesystem calls may stall in the kernel. Bound both the caller's wait
    /// and detached jobs; a late result can only be dropped, never published.
    fn io<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&Control) -> Result<T, ClosedError> + Send + 'static,
    ) -> Result<T, ClosedError> {
        use ibis_core::file_io::{Admission, Pool, Scope};
        static POOL: std::sync::OnceLock<Pool> = std::sync::OnceLock::new();
        let check = || (self.cancelled)();
        let scope = Scope {
            until: self.deadline,
            now: Arc::new(Instant::now),
            cancelled: &check,
        };
        let worker = self.clone();
        POOL.get_or_init(|| Pool::new(8, 0))
            .run(&scope, Admission::Wait, move || {
                worker.check().and_then(|()| operation(&worker))
            })
            .map_err(|error| match error {
                ibis_core::file_io::Error::Cancelled => ClosedError::Cancelled,
                ibis_core::file_io::Error::TimedOut => ClosedError::TimedOut,
                _ => ClosedError::Unreadable,
            })?
    }

    fn new(cancelled: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        Self::within(cancelled, Duration::from_secs(60))
    }

    fn within(cancelled: Arc<dyn Fn() -> bool + Send + Sync>, budget: Duration) -> Self {
        Self {
            deadline: Instant::now() + budget,
            cancelled,
        }
    }

    fn check(&self) -> Result<(), ClosedError> {
        if (self.cancelled)() {
            Err(ClosedError::Cancelled)
        } else if Instant::now() >= self.deadline {
            Err(ClosedError::TimedOut)
        } else {
            Ok(())
        }
    }
}

struct FileLease {
    file: std::fs::File,
}

impl FileLease {
    fn acquire(file: std::fs::File) -> Result<Self, std::fs::TryLockError> {
        file.try_lock()?;
        Ok(Self { file })
    }
}

impl Drop for FileLease {
    fn drop(&mut self) {
        // Closing only our descriptor can leave a Unix lock held by a concurrent
        // fork until exec. Release it when this owner ends, including setup errors.
        let _ = self.file.unlock();
    }
}

/// A path heuristic warning, not a claim to detect every sync client.
pub fn cloud_synced(data: &Path) -> bool {
    let parts: Vec<_> = data
        .components()
        .map(|p| p.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    parts.iter().enumerate().any(|(i, p)| {
        [
            "dropbox",
            "onedrive",
            "google drive",
            "googledrive",
            "my drive",
            "icloud drive",
            "iclouddrive",
            "box",
            "box sync",
            "pcloud drive",
            "nextcloud",
            "owncloud",
            "sync",
            "syncthing",
        ]
        .contains(&p.as_str())
            || p.starts_with("dropbox (")
            || p.starts_with("onedrive -")
            || (i > 0 && parts[i - 1] == "library" && ["cloudstorage", "mobile documents"].contains(&p.as_str()))
    })
}

#[cfg(test)]
mod tests;
