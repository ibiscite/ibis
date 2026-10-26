//! Reading Zotero's libraries while Zotero is closed (#14, ADR 0027).
//!
//! SQLite opens only a private copy. Raw source handles briefly exclude
//! SQLite writers while the database and its WAL/journal are copied; they
//! never write, create or remove source files. On Unix an exclusive POSIX
//! record lock requires a writable descriptor, so read-only sources fail
//! closed. On Windows read-only sharing excludes writes and deletion.
//! A simultaneous Zotero launch may wait or fail until copying finishes.
//!
//! Source identity and metadata are checked around acquisition. The private
//! copy is validated and read after source locks are released, then removed.
//! Callers use this only when the local API does not answer and check again
//! afterwards, so a running Zotero is read through its API.
//!
//! Only local files are read: network paths, and cloud files that are not on
//! this computer (which Windows and macOS would download first), are refused.
//! Errors are fixed text, never paths or library content.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use ibis_core::model::LoadResult;
use ibis_core::zotero::Library;
use thiserror::Error;

mod locks;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod mounts;
mod read;
#[cfg(windows)]
mod windows;

/// Largest database copied: far beyond any real library, small enough to
/// leave room on the disk.
pub const MAX_DATABASE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
/// Copies made before the database is reported as changing.
const MAX_ATTEMPTS: usize = 3;
/// The whole read, copying included, gives up after this.
const DEADLINE: Duration = Duration::from_secs(60);
const CHUNK: usize = 1024 * 1024;

/// Zotero's database and Better BibTeX's, with the SQLite files beside them
/// that hold committed data. `-shm` is watched for activity but not copied.
const DATABASES: [&str; 2] = ["zotero.sqlite", "better-bibtex.sqlite"];
const SIDECARS: [&str; 3] = ["-wal", "-journal", "-shm"];

/// Why a closed Zotero library could not be read. Fixed text for users and
/// logs, without paths or content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SnapshotError {
    #[error("Zotero's database was not found in its data folder")]
    NotFound,
    #[error("Zotero's data folder is on a network drive or not downloaded; open Zotero instead")]
    NotLocal,
    #[error("Zotero's database could not be read")]
    Unreadable,
    #[error("Zotero's database could not be copied; check that the disk has free space")]
    CopyFailed,
    #[error("Zotero's database kept changing while it was copied; Ibis will try again")]
    Changing,
    #[error("Zotero's database is damaged; open Zotero to repair it")]
    Damaged,
    #[error("this Zotero is too old to read while it is closed; update Zotero, or open it")]
    TooOld,
    #[error("this Zotero is newer than Ibis knows how to read while it is closed; update Ibis, or open Zotero")]
    TooNew,
    #[error("Zotero's database is larger than Ibis reads")]
    TooLarge,
    #[error("Zotero's database needs rollback recovery; open Zotero first")]
    NeedsRecovery,
    #[error("reading Zotero's database took too long")]
    TimedOut,
    #[error("a chosen library uses Better BibTeX's separate read-only keys; open Zotero to read it")]
    UnsupportedKeys,
    #[error("reading Zotero's database was cancelled")]
    Cancelled,
    #[error("a chosen Zotero library is not in Zotero's database")]
    NoLibrary,
}

/// Source metadata plus content digests, compared only after coordinated
/// acquisition; coarse timestamps alone never suppress an update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint(Vec<Option<(u64, SystemTime)>>, Vec<u64>);

/// The outcome of [`read`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refresh {
    /// The files are as they were at `previous`; keep the references loaded.
    Unchanged,
    /// The chosen libraries, read from one consistent copy. Missing selected
    /// libraries fail the whole candidate.
    Loaded {
        result: LoadResult,
        fingerprint: Fingerprint,
    },
}

/// Read `libraries` from the Zotero data folder `data`, copying the files
/// into a fresh directory inside `scratch`. `previous` avoids conversion
/// only after the acquired content also matches. The caller makes sure Zotero's local API did not answer.
pub fn read(
    data: &Path,
    scratch: &Path,
    libraries: &[Library],
    previous: Option<&Fingerprint>,
) -> Result<Refresh, SnapshotError> {
    read_cancellable(data, scratch, libraries, previous, Arc::new(|| false))
}

/// As [`read`], stopping obsolete loads during acquisition and conversion.
pub fn read_cancellable(
    data: &Path,
    scratch: &Path,
    libraries: &[Library],
    previous: Option<&Fingerprint>,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<Refresh, SnapshotError> {
    let control = Control {
        deadline: Instant::now() + DEADLINE,
        cancelled,
    };
    control.check()?;
    if !local(data) || !local(scratch) {
        return Err(SnapshotError::NotLocal);
    }
    #[cfg(windows)]
    let _source_directories = locks::directory_guards(data)?;
    #[cfg(windows)]
    let _scratch_directories = locks::directory_guards(scratch)?;
    // A nested cache directory is opened without following its final link
    // before any descendant is probed. If absent, never probe its descendants
    // during this acquisition; a newly appearing directory fails the read.
    let shadow = data.join("better-bibtex");
    let has_shadow = shadow_present(data)?;
    #[cfg(windows)]
    let _shadow_directory = if has_shadow {
        Some(locks::directory_guard(&shadow)?)
    } else {
        None
    };
    #[cfg(not(windows))]
    let _ = shadow;
    for _ in 0..MAX_ATTEMPTS {
        control.check()?;
        if shadow_present(data)? != has_shadow {
            return Err(SnapshotError::Changing);
        }
        let before = fingerprint_with_shadow(data, has_shadow)?;
        if before.0.first().is_none_or(Option::is_none) {
            return Err(SnapshotError::NotFound);
        }
        // A rollback journal can contain an external super-journal pathname.
        // Never let SQLite recovery interpret that untrusted VFS instruction.
        if names()
            .zip(&before.0)
            .any(|(name, state)| name.ends_with("-journal") && state.is_some_and(|(len, _)| len > 0))
        {
            return Err(SnapshotError::NeedsRecovery);
        }
        // Hold SQLite-conflicting locks before opening any sidecar, including
        // when -shm does not exist yet. Only raw handles touch the originals.
        let mut sources = locks::Sources::acquire(data, &before, &control)?;
        if shadow_present(data)? != has_shadow
            || fingerprint_with_shadow(data, has_shadow)? != before
            || !sources.same_files(data)
        {
            continue;
        }
        let copy = Copy::new(scratch)?;
        let hashes = sources.copy_to(&copy.path, &control)?;
        if shadow_present(data)? != has_shadow
            || fingerprint_with_shadow(data, has_shadow)? != before
            || !sources.same_files(data)
        {
            continue;
        }
        // Lock ownership ends before SQLite checks/conversion. Never re-open
        // a main DB while a POSIX record lock is owned: closing it drops locks.
        drop(sources);
        #[cfg(windows)]
        drop(_source_directories);
        #[cfg(windows)]
        drop(_shadow_directory);
        let fingerprint = Fingerprint(before.0, hashes);
        if previous == Some(&fingerprint) {
            return Ok(Refresh::Unchanged);
        }
        let result = read::libraries(&copy.path, libraries, control.clone());
        control.check()?;
        let result = result?;
        control.check()?;
        return Ok(Refresh::Loaded { result, fingerprint });
    }
    Err(SnapshotError::Changing)
}

#[derive(Clone)]
struct Control {
    deadline: Instant,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl Control {
    fn check(&self) -> Result<(), SnapshotError> {
        if (self.cancelled)() {
            Err(SnapshotError::Cancelled)
        } else if Instant::now() > self.deadline {
            Err(SnapshotError::TimedOut)
        } else {
            Ok(())
        }
    }
}

/// Whether the folder holding Zotero's data is in a folder a cloud service
/// syncs. Zotero warns against this: a sync can copy the database while
/// Zotero writes it, and damage it.
pub fn cloud_synced(data: &Path) -> bool {
    let parts: Vec<String> = data
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    const SYNCED: [&str; 14] = [
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
    ];
    parts.iter().enumerate().any(|(i, part)| {
        let inside_library = i > 0 && parts[i - 1] == "library";
        SYNCED.contains(&part.as_str())
            || part.starts_with("dropbox (")
            || part.starts_with("onedrive -")
            || (inside_library && (part == "cloudstorage" || part == "mobile documents"))
    })
}

/// A fresh directory only this user can read, removed when dropped.
struct Copy {
    path: PathBuf,
    _guard: Option<File>,
}

const PREFIX: &str = "ibis-zotero-";

impl Copy {
    fn new(scratch: &Path) -> Result<Copy, SnapshotError> {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = scratch.join(format!("{PREFIX}{}-{nanos:x}", std::process::id()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&dir)
                .map_err(|_| SnapshotError::CopyFailed)?;
        }
        #[cfg(windows)]
        windows::create_private(&dir)?;
        #[cfg(not(any(unix, windows)))]
        return Err(SnapshotError::CopyFailed);
        #[cfg(windows)]
        let guard = {
            let guard = locks::directory_guard(&dir)?;
            if !windows::private(&dir) {
                return Err(SnapshotError::CopyFailed);
            }
            Some(guard)
        };
        #[cfg(not(windows))]
        let guard = None;
        Ok(Copy {
            path: dir,
            _guard: guard,
        })
    }
}

impl Drop for Copy {
    fn drop(&mut self) {
        drop(self._guard.take());
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn names() -> impl Iterator<Item = String> {
    DATABASES
        .iter()
        .flat_map(|db| std::iter::once(db.to_string()).chain(SIDECARS.iter().map(move |s| format!("{db}{s}"))))
        .chain(std::iter::once("better-bibtex/read-only.json".to_string()))
}

#[cfg(test)]
fn fingerprint(data: &Path) -> Result<Fingerprint, SnapshotError> {
    fingerprint_with_shadow(data, shadow_present(data)?)
}

fn shadow_present(data: &Path) -> Result<bool, SnapshotError> {
    let path = data.join("better-bibtex");
    match std::fs::symlink_metadata(&path) {
        Ok(metadata)
            if metadata.is_dir() && !metadata.file_type().is_symlink() && resident(&metadata) && local(&path) =>
        {
            Ok(true)
        }
        Ok(_) => Err(SnapshotError::NotLocal),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(SnapshotError::Unreadable),
    }
}

fn fingerprint_with_shadow(data: &Path, has_shadow: bool) -> Result<Fingerprint, SnapshotError> {
    names()
        .map(|name| {
            if name.contains('/') && !has_shadow {
                return Ok(None);
            }
            match std::fs::symlink_metadata(data.join(&name)) {
                Ok(m) if !m.is_file() => Err(SnapshotError::Unreadable),
                Ok(m) if !resident(&m) => Err(SnapshotError::NotLocal),
                Ok(m) => Ok(Some((m.len(), m.modified().map_err(|_| SnapshotError::Unreadable)?))),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(_) => Err(SnapshotError::Unreadable),
            }
        })
        .collect::<Result<_, _>>()
        .map(|metadata| Fingerprint(metadata, Vec::new()))
}

fn copy_bounded(
    source: &mut File,
    mut target: Option<File>,
    limit: u64,
    control: &Control,
) -> Result<u64, SnapshotError> {
    use std::hash::Hasher;
    let mut hash = std::hash::DefaultHasher::new();
    let mut source = source.take(limit + 1);
    let mut buffer = vec![0u8; CHUNK];
    let mut copied = 0u64;
    loop {
        control.check()?;
        let n = match source.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(SnapshotError::Unreadable),
        };
        copied += n as u64;
        if copied > limit {
            return Err(SnapshotError::TooLarge);
        }
        hash.write(&buffer[..n]);
        if let Some(target) = &mut target {
            target.write_all(&buffer[..n]).map_err(|_| SnapshotError::CopyFailed)?;
        }
    }
    if let Some(target) = target {
        target.sync_all().map_err(|_| SnapshotError::CopyFailed)?;
    }
    Ok(hash.finish())
}

/// Whether a discovered candidate holds a regular, resident database. Windows
/// ancestors remain guarded through the metadata probe; acquisition rechecks it.
pub fn has_database(data: &Path) -> bool {
    if !local(data) {
        return false;
    }
    #[cfg(windows)]
    let Ok(_directories) = locks::directory_guards(data) else {
        return false;
    };
    std::fs::symlink_metadata(data.join("zotero.sqlite"))
        .is_ok_and(|metadata| metadata.is_file() && resident(&metadata))
}

/// Guard a profile configuration's parent before metadata or content probes.
/// Missing ancestors are a missing optional configuration, never a reason to
/// probe a remote or unguarded descendant.
pub(crate) fn configuration_guards(path: &Path) -> Result<Option<Vec<File>>, SnapshotError> {
    let parent = path.parent().ok_or(SnapshotError::NotLocal)?;
    #[cfg(windows)]
    {
        match locks::directory_guards(parent) {
            Ok(guards) => Ok(Some(guards)),
            Err(SnapshotError::NotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    match mounts::check(parent) {
        Ok(()) => Ok(Some(Vec::new())),
        Err(SnapshotError::NotFound) => Ok(None),
        Err(error) => Err(error),
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    Err(SnapshotError::NotLocal)
}

/// Whether a folder is on this computer's own disks. Windows network shares
/// (`\\server\share`) are refused from the path alone, before anything is
/// asked of them; on macOS and Linux the file system says whether it is
/// local. Callers check this before looking for files in the folder.
pub fn local(path: &Path) -> bool {
    let text = path.to_string_lossy();
    let unc = text.strip_prefix(r"\\?\").map_or_else(
        || text.starts_with(r"\\") || text.starts_with("//"),
        |rest| rest.get(..4).is_some_and(|p| p.eq_ignore_ascii_case(r"UNC\")),
    );
    if cfg!(windows) && unc {
        return false;
    }
    local_file_system(path)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn local_file_system(path: &Path) -> bool {
    mounts::check(path).is_ok()
}

#[cfg(windows)]
fn local_file_system(path: &Path) -> bool {
    windows::local(path)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn local_file_system(_: &Path) -> bool {
    false
}

/// Whether a file's contents are on this computer, so reading it does not
/// make a cloud service download it first (or hang until it has).
fn resident(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const OFFLINE: u32 = 0x1000;
        const RECALL_ON_OPEN: u32 = 0x0004_0000;
        const RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
        metadata.file_attributes() & (OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) == 0
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::macos::fs::MetadataExt;
        const SF_DATALESS: u32 = 0x4000_0000;
        metadata.st_flags() & SF_DATALESS == 0
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = metadata;
        true
    }
}

#[cfg(test)]
mod tests;
