//! Closed-only Zotero reader. Source files are read as bytes; SQLite opens only
//! an owner-only disposable copy. No source lock is taken, including at startup.
//!
//! Keep one reader for subsequent loads. Its first read removes stale private
//! copies under the same cancellation/deadline control as acquisition; a
//! private-root lock prevents another reader cleaning an active copy. Errors
//! and Debug output never include paths or item text.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, TryLockError};
use std::time::{Duration, Instant};

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
    TooNew,
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
/// Adapters do not normalize these fields or produce CSL/legacy references.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct RawItem {
    pub key: String,
    pub item_type: String,
    pub creators: Vec<RawCreator>,
    pub fields: BTreeMap<String, String>,
    /// Keys Better BibTeX keeps outside Zotero's own fields (see [`crate::bbt`]).
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
}

#[derive(Clone, PartialEq, Eq)]
pub enum ClosedWarning {
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
    ClosedWarning
);

/// One owner of the private copy root, initialized during the first read.
pub struct ClosedReader {
    cache: PathBuf,
    store: OnceLock<private_copy::Store>,
    initializing: Mutex<()>,
}

impl ClosedReader {
    /// `cache` must be an application cache/temporary parent, never library
    /// storage. On Windows it must be inside the current user Temp folder.
    /// Ibis creates and verifies its own child with backup exclusion.
    pub fn new(cache: &Path) -> Result<Self, ClosedError> {
        Ok(Self {
            cache: cache.to_owned(),
            store: OnceLock::new(),
            initializing: Mutex::new(()),
        })
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
        if self.store.get().is_none() {
            let _permit = control.lock(&self.initializing, ClosedError::PrivateStorage)?;
            if self.store.get().is_none() {
                let store = private_copy::Store::open(&self.cache, control)?;
                self.store.set(store).map_err(|_| ClosedError::PrivateStorage)?;
            }
        }
        self.store.get().ok_or(ClosedError::PrivateStorage)?.read(data, control)
    }
}

#[derive(Clone)]
struct Control {
    deadline: Instant,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
}
impl Control {
    fn new(cancelled: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        Self {
            deadline: Instant::now() + Duration::from_secs(60),
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

    fn lock<'a, T>(&self, mutex: &'a Mutex<T>, poisoned: ClosedError) -> Result<MutexGuard<'a, T>, ClosedError> {
        loop {
            self.check()?;
            match mutex.try_lock() {
                Ok(permit) => return Ok(permit),
                Err(TryLockError::Poisoned(_)) => return Err(poisoned),
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    }
}

/// A persistent private namespace, independent of disposable Zotero copies.
/// Paths and native errors are never included in Debug or diagnostics.
pub struct PrivateDirectory {
    root: PathBuf,
    identity: std::fs::Metadata,
    lease: std::fs::File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PrivateStorageError {
    #[error("private storage is unavailable")]
    Unavailable,
    #[error("private storage is already in use")]
    Busy,
}

impl PrivateDirectory {
    pub fn open(path: &Path) -> Result<Self, PrivateStorageError> {
        use std::io::{Read, Write};
        let unavailable = |_| PrivateStorageError::Unavailable;
        let control = Control::new(Arc::new(|| false));
        let parent = platform::directory(path.parent().ok_or(PrivateStorageError::Unavailable)?, &control)
            .map_err(unavailable)?;
        let root = parent.join(path.file_name().ok_or(PrivateStorageError::Unavailable)?);
        let fresh = match std::fs::symlink_metadata(&root) {
            Ok(_) => {
                platform::secure(&root, &control).map_err(unavailable)?;
                false
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                platform::create_private(&root, &control).map_err(unavailable)?;
                true
            }
            Err(_) => return Err(PrivateStorageError::Unavailable),
        };
        const OWNER: &[u8] = b"Ibis prepared source storage\n";
        let marker_path = root.join(".owner");
        if fresh {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut marker = options
                .open(&marker_path)
                .map_err(|_| PrivateStorageError::Unavailable)?;
            marker
                .write_all(OWNER)
                .and_then(|_| marker.sync_all())
                .map_err(|_| PrivateStorageError::Unavailable)?;
        }
        platform::secure_file(&marker_path, &control).map_err(unavailable)?;
        let mut marker = Vec::new();
        platform::open_source(&marker_path)
            .map_err(unavailable)?
            .take(64)
            .read_to_end(&mut marker)
            .map_err(|_| PrivateStorageError::Unavailable)?;
        if marker != OWNER {
            return Err(PrivateStorageError::Unavailable);
        }
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(7).custom_flags(0x0020_0000);
        }
        let lock_path = root.join(".lease");
        let lease = options.open(&lock_path).map_err(|_| PrivateStorageError::Unavailable)?;
        platform::secure_file(&lock_path, &control).map_err(unavailable)?;
        let named = std::fs::symlink_metadata(&lock_path).map_err(|_| PrivateStorageError::Unavailable)?;
        if !platform::same(&named, &lease.metadata().map_err(|_| PrivateStorageError::Unavailable)?) {
            return Err(PrivateStorageError::Unavailable);
        }
        lease.try_lock().map_err(|_| PrivateStorageError::Busy)?;
        platform::exclude_backups(&root, &control).map_err(unavailable)?;
        let identity = std::fs::symlink_metadata(&root).map_err(|_| PrivateStorageError::Unavailable)?;
        let directory = Self { root, identity, lease };
        directory.verify()?;
        Ok(directory)
    }

    /// Only private-storage code should use this path; it must never serialize.
    pub fn path(&self) -> &Path {
        &self.root
    }

    pub fn verify(&self) -> Result<(), PrivateStorageError> {
        let control = Control::new(Arc::new(|| false));
        platform::secure(&self.root, &control).map_err(|_| PrivateStorageError::Unavailable)?;
        let lease_path = self.root.join(".lease");
        platform::secure_file(&lease_path, &control).map_err(|_| PrivateStorageError::Unavailable)?;
        let named = std::fs::symlink_metadata(&lease_path).map_err(|_| PrivateStorageError::Unavailable)?;
        if !platform::same(
            &named,
            &self.lease.metadata().map_err(|_| PrivateStorageError::Unavailable)?,
        ) {
            return Err(PrivateStorageError::Unavailable);
        }
        // Read identities after the native privacy checks, which can yield.
        let current = std::fs::symlink_metadata(&self.root).map_err(|_| PrivateStorageError::Unavailable)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if current.dev() != self.identity.dev() || current.ino() != self.identity.ino() {
                return Err(PrivateStorageError::Unavailable);
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if current.creation_time() != self.identity.creation_time()
                || current.file_attributes() != self.identity.file_attributes()
            {
                return Err(PrivateStorageError::Unavailable);
            }
        }
        Ok(())
    }

    pub fn verify_file(&self, path: &Path) -> Result<(), PrivateStorageError> {
        self.verify()?;
        if path.parent() != Some(self.path()) {
            return Err(PrivateStorageError::Unavailable);
        }
        platform::secure_file(path, &Control::new(Arc::new(|| false))).map_err(|_| PrivateStorageError::Unavailable)
    }

    pub fn open_file(&self, name: &str) -> Result<Option<std::fs::File>, PrivateStorageError> {
        if name.is_empty()
            || name.len() > 128
            || name == "."
            || name == ".."
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        {
            return Err(PrivateStorageError::Unavailable);
        }
        self.verify()?;
        let path = self.root.join(name);
        let named = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(PrivateStorageError::Unavailable),
        };
        self.verify_file(&path)?;
        let file = platform::open_source(&path).map_err(|_| PrivateStorageError::Unavailable)?;
        if !platform::same(&named, &file.metadata().map_err(|_| PrivateStorageError::Unavailable)?) {
            return Err(PrivateStorageError::Unavailable);
        }
        Ok(Some(file))
    }
}

#[cfg(test)]
mod persistent_directory_tests {
    use super::*;
    #[test]
    fn a_foreign_private_directory_is_not_adopted_or_modified() {
        let parent = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let directory = parent.path().join("foreign");
        platform::create_private(&directory, &Control::new(Arc::new(|| false))).unwrap_or_else(|e| panic!("{e}"));
        let sentinel = directory.join("generation-invented");
        std::fs::write(&sentinel, b"IBISGEN1foreign").unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(
            PrivateDirectory::open(&directory),
            Err(PrivateStorageError::Unavailable)
        ));
        assert_eq!(
            std::fs::read_dir(&directory).unwrap_or_else(|e| panic!("{e}")).count(),
            1
        );
        assert_eq!(
            std::fs::read(sentinel).unwrap_or_else(|e| panic!("{e}")),
            b"IBISGEN1foreign"
        );
    }

    #[test]
    fn lease_and_private_namespace_are_independent_of_disposable_copies() {
        let parent = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = parent.path().join("generations");
        let directory = PrivateDirectory::open(&path).unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(PrivateDirectory::open(&path), Err(PrivateStorageError::Busy)));
        assert!(directory.open_file("../outside").is_err());
        assert!(
            directory
                .open_file("absent")
                .unwrap_or_else(|e| panic!("{e}"))
                .is_none()
        );
        drop(directory);
        assert!(PrivateDirectory::open(&path).is_ok());
    }

    #[test]
    fn a_missing_or_replaced_named_lease_retires_its_previous_owner() {
        for replace in [false, true] {
            let parent = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
            let path = parent.path().join("generations");
            let first = PrivateDirectory::open(&path).unwrap_or_else(|e| panic!("{e}"));
            let lease = path.join(".lease");
            // Keep the old handle open while replacing its directory entry.
            std::fs::rename(&lease, path.join("retired-lease")).unwrap_or_else(|e| panic!("{e}"));
            let second = replace.then(|| PrivateDirectory::open(&path).unwrap_or_else(|e| panic!("{e}")));
            assert!(matches!(first.verify(), Err(PrivateStorageError::Unavailable)));
            assert!(matches!(
                first.open_file("absent"),
                Err(PrivateStorageError::Unavailable)
            ));
            drop(second);
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_namespace_is_refused_without_touching_its_destination() {
        let parent = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let destination = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = parent.path().join("generations");
        std::os::unix::fs::symlink(destination.path(), &path).unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(
            PrivateDirectory::open(&path),
            Err(PrivateStorageError::Unavailable)
        ));
        assert_eq!(
            std::fs::read_dir(destination.path())
                .unwrap_or_else(|e| panic!("{e}"))
                .count(),
            0
        );
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
