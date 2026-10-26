//! Reading Zotero's libraries while Zotero is closed (#14, ADR 0027).
//!
//! Zotero's database is never opened where it lies. Its files are copied
//! into a fresh private directory, the copy is checked and read, and the
//! directory is removed, whatever happens. Opening the original, even read
//! only, would take SQLite locks that make a Zotero starting at that moment
//! fail with "database is locked", and could leave `-wal` or `-shm` files
//! beside it. The `-shm` index is not copied: SQLite rebuilds it from the
//! copied `-wal`, which holds the last transactions of a Zotero that did not
//! shut down cleanly. A rollback `-journal` is copied too, so SQLite can
//! undo the interrupted transaction it records.
//!
//! A copy is only trusted when every file looked the same before and after
//! it was made; otherwise it is made again, a few times at most. Callers use
//! this only when Zotero's local API does not answer, and check again
//! afterwards, so a Zotero that is running is always read through its API.
//!
//! Only local files are read: network paths, and cloud files that are not on
//! this computer (which Windows and macOS would download first), are refused.
//! Errors are fixed text, never paths or library content.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use ibis_core::model::LoadResult;
use ibis_core::zotero::Library;
use thiserror::Error;

mod read;

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
    #[error("reading Zotero's database took too long")]
    TimedOut,
}

/// The state of the database files when they were read: equal fingerprints
/// mean the files did not change in between.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint(Vec<Option<(u64, SystemTime)>>);

/// The outcome of [`read`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refresh {
    /// The files are as they were at `previous`; keep the references loaded.
    Unchanged,
    /// The chosen libraries, read from one consistent copy. A chosen group
    /// the database does not have is a warning naming it.
    Loaded {
        result: LoadResult,
        fingerprint: Fingerprint,
    },
}

/// Read `libraries` from the Zotero data folder `data`, copying the files
/// into a fresh directory inside `scratch`, unless they are unchanged since
/// `previous`. The caller makes sure Zotero's local API did not answer.
pub fn read(
    data: &Path,
    scratch: &Path,
    libraries: &[Library],
    previous: Option<&Fingerprint>,
) -> Result<Refresh, SnapshotError> {
    let deadline = Instant::now() + DEADLINE;
    if !local(data) {
        return Err(SnapshotError::NotLocal);
    }
    for _ in 0..MAX_ATTEMPTS {
        let before = fingerprint(data)?;
        if before.0.first().is_none_or(Option::is_none) {
            return Err(SnapshotError::NotFound);
        }
        if previous == Some(&before) {
            return Ok(Refresh::Unchanged);
        }
        let copy = Copy::new(scratch)?;
        let copied = copy_files(data, &copy.0, &before, deadline);
        // A file that went away or changed while it was copied is a change.
        if fingerprint(data)? != before {
            continue;
        }
        copied?;
        let result = read::libraries(&copy.0, libraries, deadline)?;
        return Ok(Refresh::Loaded {
            result,
            fingerprint: before,
        });
    }
    Err(SnapshotError::Changing)
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
struct Copy(PathBuf);

/// A copy left behind by an Ibis that was stopped mid-read is removed once
/// it is this old; no read lasts this long.
const STALE: Duration = Duration::from_secs(10 * 60);
const PREFIX: &str = "ibis-zotero-";

impl Copy {
    fn new(scratch: &Path) -> Result<Copy, SnapshotError> {
        remove_stale(scratch);
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = scratch.join(format!("{PREFIX}{}-{nanos:x}", std::process::id()));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder.create(&dir).map_err(|_| SnapshotError::CopyFailed)?;
        Ok(Copy(dir))
    }
}

impl Drop for Copy {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Remove old copies, so no copy of a library outlives the read for long.
fn remove_stale(scratch: &Path) {
    let Ok(entries) = std::fs::read_dir(scratch) else {
        return;
    };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .ok()
            .filter(|m| m.is_dir())
            .and_then(|m| m.modified().ok());
        let old = old.and_then(|time| time.elapsed().ok()).is_some_and(|age| age > STALE);
        if old && entry.file_name().to_string_lossy().starts_with(PREFIX) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

fn names() -> impl Iterator<Item = String> {
    DATABASES
        .iter()
        .flat_map(|db| std::iter::once(db.to_string()).chain(SIDECARS.iter().map(move |s| format!("{db}{s}"))))
}

fn fingerprint(data: &Path) -> Result<Fingerprint, SnapshotError> {
    names()
        .map(|name| match std::fs::symlink_metadata(data.join(&name)) {
            Ok(m) if !m.is_file() => Err(SnapshotError::Unreadable),
            Ok(m) if !resident(&m) => Err(SnapshotError::NotLocal),
            Ok(m) => Ok(Some((m.len(), m.modified().map_err(|_| SnapshotError::Unreadable)?))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(SnapshotError::Unreadable),
        })
        .collect::<Result<_, _>>()
        .map(Fingerprint)
}

fn copy_files(data: &Path, copy: &Path, seen: &Fingerprint, deadline: Instant) -> Result<(), SnapshotError> {
    let total: u64 = seen.0.iter().flatten().map(|(len, _)| len).sum();
    if total > MAX_DATABASE_BYTES {
        return Err(SnapshotError::TooLarge);
    }
    for (name, state) in names().zip(&seen.0) {
        if state.is_none() || name.ends_with("-shm") {
            continue;
        }
        let source = crate::discovery::open_regular(&data.join(&name)).map_err(|_| SnapshotError::Unreadable)?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let target = options.open(copy.join(&name)).map_err(|_| SnapshotError::CopyFailed)?;
        copy_bounded(source, target, MAX_DATABASE_BYTES, deadline)?;
    }
    Ok(())
}

fn copy_bounded(source: File, mut target: File, limit: u64, deadline: Instant) -> Result<(), SnapshotError> {
    let mut source = source.take(limit + 1);
    let mut buffer = vec![0u8; CHUNK];
    let mut copied = 0u64;
    loop {
        if Instant::now() > deadline {
            return Err(SnapshotError::TimedOut);
        }
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
        target.write_all(&buffer[..n]).map_err(|_| SnapshotError::CopyFailed)?;
    }
    target.sync_all().map_err(|_| SnapshotError::CopyFailed)
}

/// Whether a path is on this computer's own disks, as far as the path shows:
/// Windows network shares (`\\server\share`) are not.
fn local(path: &Path) -> bool {
    let text = path.to_string_lossy();
    let unc = text.strip_prefix(r"\\?\").map_or_else(
        || text.starts_with(r"\\") || text.starts_with("//"),
        |rest| rest.get(..4).is_some_and(|p| p.eq_ignore_ascii_case(r"UNC\")),
    );
    !(cfg!(windows) && unc)
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
