//! Byte-only acquisition. This module never gives a source path to SQLite.
use super::{ClosedError, ClosedSnapshot, ClosedWarning, Control, database, keys, platform};
use std::fs::{File, Metadata, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub(super) const MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const CHUNK: usize = 1024 * 1024;
const ROOT: &str = "ibis-zotero-closed";
const MARKER: &[u8] = b"Ibis private disposable Zotero copy v1\n";
/// Source files, relative to the data directory. Only the main database must
/// exist. Better BibTeX's files are copied and checked with Zotero's own, so
/// its keys come from the same quiet moment.
const FILES: [&str; 8] = [
    "zotero.sqlite",
    "zotero.sqlite-wal",
    "zotero.sqlite-journal",
    "zotero.sqlite-shm",
    "better-bibtex.sqlite",
    "better-bibtex.sqlite-wal",
    "better-bibtex.sqlite-journal",
    "better-bibtex/read-only.json",
];
/// Files from here on are Better BibTeX's: optional, and never a reason to
/// refuse the library. One that is present but cannot be copied safely is
/// skipped, which leaves the snapshot's keys unknown.
const BBT: usize = 4;
/// Each Better BibTeX file, on top of Zotero's own [`MAX_BYTES`].
const MAX_BBT_FILE: u64 = 256 * 1024 * 1024;
/// Logs before their databases; SHM (3) is activity evidence, never copied.
const COPY_ORDER: [usize; 7] = [1, 2, 0, 5, 6, 4, 7];

fn source_path(data: &Path, name: &str) -> PathBuf {
    name.split('/').fold(data.to_owned(), |path, part| path.join(part))
}

/// The flat name of a source file in the private copy.
pub(super) fn private_name(name: &str) -> String {
    name.replace('/', "-")
}

pub(super) struct Store {
    root: PathBuf,
    _lock: File,
}
impl Store {
    pub(super) fn open(cache: &Path, control: &Control) -> Result<Self, ClosedError> {
        control.check()?;
        let parent = platform::directory(cache, control).map_err(|e| e.context(ClosedError::PrivateStorage))?;
        let root = parent.join(ROOT);
        match std::fs::symlink_metadata(&root) {
            Ok(_) => platform::secure(&root, control)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => platform::create_private(&root, control)?,
            Err(_) => return Err(ClosedError::PrivateStorage),
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
        }
        let lock = options
            .open(root.join(".lock"))
            .map_err(|_| ClosedError::PrivateStorage)?;
        if !lock.metadata().is_ok_and(|m| m.is_file()) {
            return Err(ClosedError::PrivateStorage);
        }
        lock.try_lock().map_err(|_| ClosedError::StorageInUse)?;
        platform::exclude_backups(&root, control)?;
        // Only marked candidates in our verified root are ours. Prefix
        // lookalikes, foreign files and symlinks are never traversed/deleted.
        for entry in std::fs::read_dir(&root).map_err(|_| ClosedError::PrivateStorage)? {
            control.check()?;
            let entry = entry.map_err(|_| ClosedError::PrivateStorage)?;
            if entry.file_name().to_string_lossy().starts_with("copy-")
                && entry.file_type().is_ok_and(|t| t.is_dir() && !t.is_symlink())
            {
                match platform::secure(&entry.path(), control) {
                    Ok(()) if owned_marker(&entry.path()) => {
                        control.check()?;
                        std::fs::remove_dir_all(entry.path()).map_err(|_| ClosedError::PrivateStorage)?;
                    }
                    Err(error @ (ClosedError::Cancelled | ClosedError::TimedOut)) => return Err(error),
                    _ => (),
                }
            }
        }
        control.check()?;
        Ok(Self { root, _lock: lock })
    }

    pub(super) fn read(&self, data: &Path, control: &Control) -> Result<ClosedSnapshot, ClosedError> {
        self.read_with(data, control, &mut || Ok(()), &mut || platform::closed(control))
    }

    // Hooks are private and used by synthetic tests to force exact race points.
    pub(super) fn read_with(
        &self,
        data: &Path,
        control: &Control,
        after_copy: &mut dyn FnMut() -> Result<(), ClosedError>,
        check_closed: &mut dyn FnMut() -> Result<(), ClosedError>,
    ) -> Result<ClosedSnapshot, ClosedError> {
        control.check()?;
        check_closed()?;
        let directory = platform::directory(data, control)?;
        if directory.starts_with(&self.root) || self.root.starts_with(&directory) {
            return Err(ClosedError::UnsafeSource);
        }
        let mut left = (MAX_BYTES + MAX_BBT_FILE * (FILES.len() - BBT) as u64).saturating_mul(3);
        let mut last = ClosedError::Changing;
        for _ in 0..3 {
            control.check()?;
            check_closed()?;
            let result = self.attempt(data, &directory, control, &mut left, after_copy, check_closed);
            match result {
                Ok(mut snapshot) => {
                    if super::cloud_synced(data) || super::cloud_synced(&directory) {
                        snapshot.warnings.push(ClosedWarning::CloudSyncedDirectory);
                    }
                    return Ok(snapshot);
                }
                Err(error @ (ClosedError::Changing | ClosedError::Damaged)) => last = error,
                Err(error) => return Err(error),
            }
        }
        Err(last)
    }

    fn attempt(
        &self,
        input: &Path,
        data: &Path,
        control: &Control,
        left: &mut u64,
        after_copy: &mut dyn FnMut() -> Result<(), ClosedError>,
        check_closed: &mut dyn FnMut() -> Result<(), ClosedError>,
    ) -> Result<ClosedSnapshot, ClosedError> {
        let mut sources = Sources::open(data)?;
        let size = |files: &[Option<Source>]| {
            files
                .iter()
                .flatten()
                .try_fold(0u64, |sum, f| sum.checked_add(f.metadata.len()))
        };
        size(&sources.files[..BBT])
            .filter(|sum| *sum <= MAX_BYTES)
            .ok_or(ClosedError::TooLarge)?;
        // Better BibTeX files are each at most MAX_BBT_FILE.
        let total = size(&sources.files).ok_or(ClosedError::TooLarge)?;
        // Copy bytes + WAL index/query headroom; no checkpoint writes to main.
        let needed = total
            .checked_mul(2)
            .and_then(|n| n.checked_add(64 * 1024 * 1024))
            .ok_or(ClosedError::TooLarge)?;
        platform::space(&self.root, needed, control)?;
        let copy = Copy::new(&self.root, control)?;
        // WAL is copied before main. We make no claim to concurrent-copy
        // support: process checks and source validation must all remain quiet.
        for index in COPY_ORDER {
            if let Some(source) = &mut sources.files[index] {
                let mut target = private_file(&copy.path.join(private_name(FILES[index])))?;
                source.file.rewind().map_err(|_| ClosedError::Unreadable)?;
                let mut remaining = source.metadata.len();
                let mut buffer = vec![0; CHUNK];
                while remaining > 0 {
                    control.check()?;
                    platform::passive(&source.file)?;
                    let n = remaining.min(CHUNK as u64) as usize;
                    source
                        .file
                        .read_exact(&mut buffer[..n])
                        .map_err(|_| ClosedError::Changing)?;
                    *left = left.checked_sub(n as u64).ok_or(ClosedError::TooLarge)?;
                    target.write_all(&buffer[..n]).map_err(|_| ClosedError::NoSpace)?;
                    remaining -= n as u64;
                }
                if source.file.read(&mut buffer[..1]).map_err(|_| ClosedError::Changing)? != 0 {
                    return Err(ClosedError::Changing);
                }
                target.sync_all().map_err(|_| ClosedError::PrivateStorage)?;
            }
        }
        after_copy()?;
        control.check()?;
        sources.validate(input, data, &copy.path, control)?;
        check_closed()?;
        // Drop *every* source handle before SQLite is opened privately.
        let skipped = sources.skipped.contains(&true);
        drop(sources);
        let (mut snapshot, zotero) = database::read(&copy, control)?;
        keys::attach(&copy, zotero, skipped, &mut snapshot, control)?;
        control.check()?;
        Ok(snapshot)
    }
}

fn owned_marker(path: &Path) -> bool {
    let path = path.join(".ibis-copy");
    if !std::fs::symlink_metadata(&path)
        .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink() && m.len() == MARKER.len() as u64)
    {
        return false;
    }
    let Ok(file) = platform::open_source(&path) else {
        return false;
    };
    let mut bytes = Vec::new();
    file.take(MARKER.len() as u64 + 1).read_to_end(&mut bytes).is_ok() && bytes == MARKER
}

fn private_file(path: &Path) -> Result<File, ClosedError> {
    let mut o = OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(path).map_err(|_| ClosedError::PrivateStorage)
}

pub(super) struct Copy {
    path: PathBuf,
}
impl Copy {
    pub(super) fn database_path(&self) -> PathBuf {
        self.path.join("zotero.sqlite")
    }

    /// The private copy of a source file named in `FILES`.
    pub(super) fn file(&self, name: &str) -> PathBuf {
        self.path.join(private_name(name))
    }

    fn new(root: &Path, control: &Control) -> Result<Self, ClosedError> {
        control.check()?;
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| ClosedError::PrivateStorage)?
            .as_nanos();
        let path = root.join(format!(
            "copy-{}-{stamp:x}-{:x}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        platform::create_private(&path, control)?;
        let result = Self { path };
        platform::exclude_backups(&result.path, control)?;
        control.check()?;
        let mut marker = private_file(&result.path.join(".ibis-copy"))?;
        marker.write_all(MARKER).map_err(|_| ClosedError::PrivateStorage)?;
        // The cleanup proof must reach stable storage before any library file.
        marker.sync_all().map_err(|_| ClosedError::PrivateStorage)?;
        #[cfg(unix)]
        {
            File::open(&result.path)
                .and_then(|d| d.sync_all())
                .map_err(|_| ClosedError::PrivateStorage)?;
            File::open(root)
                .and_then(|d| d.sync_all())
                .map_err(|_| ClosedError::PrivateStorage)?;
        }
        control.check()?;
        Ok(result)
    }
}
impl Drop for Copy {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

struct Source {
    file: File,
    metadata: Metadata,
    header: Vec<u8>,
}
impl Source {
    fn open(path: &Path, seen: &Metadata) -> Result<Self, ClosedError> {
        let mut file = platform::open_source(path)?;
        let metadata = file.metadata().map_err(|_| ClosedError::Unreadable)?;
        if !platform::same(seen, &metadata) {
            return Err(ClosedError::Changing);
        }
        let header = header(&mut file)?;
        Ok(Self { file, metadata, header })
    }
}
struct Sources {
    files: Vec<Option<Source>>,
    /// Better BibTeX files present but not copied.
    skipped: Vec<bool>,
}
impl Sources {
    fn open(data: &Path) -> Result<Self, ClosedError> {
        let mut files = Vec::new();
        let mut skipped = vec![false; FILES.len()];
        for (i, name) in FILES.iter().enumerate() {
            let path = source_path(data, name);
            let source = match std::fs::symlink_metadata(&path) {
                Ok(m)
                    if m.is_file()
                        && !m.file_type().is_symlink()
                        && platform::resident(&m)
                        && (i < BBT || m.len() <= MAX_BBT_FILE) =>
                {
                    Source::open(&path, &m).map(Some)
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound && i != 0 => Ok(None),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(ClosedError::NotFound),
                _ => Err(ClosedError::UnsafeSource),
            };
            match source {
                Ok(source) => files.push(source),
                Err(ClosedError::Changing) => return Err(ClosedError::Changing),
                Err(_) if i >= BBT => {
                    skipped[i] = true;
                    files.push(None);
                }
                Err(error) => return Err(error),
            }
        }
        Ok(Self { files, skipped })
    }

    fn validate(&mut self, input: &Path, data: &Path, copy: &Path, control: &Control) -> Result<(), ClosedError> {
        if platform::directory(input, control)? != data {
            return Err(ClosedError::Changing);
        }
        for (i, source) in self.files.iter_mut().enumerate() {
            control.check()?;
            let current = std::fs::symlink_metadata(source_path(data, FILES[i]));
            match (source, current) {
                (None, _) if self.skipped[i] => (),
                (None, Err(e)) if e.kind() == std::io::ErrorKind::NotFound => (),
                (Some(source), Ok(m)) if !m.file_type().is_symlink() && platform::same(&source.metadata, &m) => {
                    platform::passive(&source.file)?;
                    if !platform::same(
                        &source.metadata,
                        &source.file.metadata().map_err(|_| ClosedError::Changing)?,
                    ) || source.header != header(&mut source.file)?
                    {
                        return Err(ClosedError::Changing);
                    }
                    if i == 3 {
                        continue;
                    } // SHM is activity evidence, never copied.
                    // Full byte comparison defeats coarse/restored mtimes and
                    // detects source WAL changes through the recorded EOF.
                    let mut private =
                        File::open(copy.join(private_name(FILES[i]))).map_err(|_| ClosedError::PrivateStorage)?;
                    source.file.rewind().map_err(|_| ClosedError::Changing)?;
                    let mut a = vec![0; CHUNK];
                    let mut b = vec![0; CHUNK];
                    let mut left = source.metadata.len();
                    while left > 0 {
                        control.check()?;
                        let n = left.min(CHUNK as u64) as usize;
                        source.file.read_exact(&mut a[..n]).map_err(|_| ClosedError::Changing)?;
                        private
                            .read_exact(&mut b[..n])
                            .map_err(|_| ClosedError::PrivateStorage)?;
                        if a[..n] != b[..n] {
                            return Err(ClosedError::Changing);
                        }
                        left -= n as u64;
                    }
                    if source.file.read(&mut a[..1]).map_err(|_| ClosedError::Changing)? != 0 {
                        return Err(ClosedError::Changing);
                    }
                    if !platform::same(
                        &source.metadata,
                        &source.file.metadata().map_err(|_| ClosedError::Changing)?,
                    ) {
                        return Err(ClosedError::Changing);
                    }
                }
                _ => return Err(ClosedError::Changing),
            }
        }
        // A sidecar could appear while another was being compared.
        for (i, (name, source)) in FILES.iter().zip(&self.files).enumerate() {
            match (source, std::fs::symlink_metadata(source_path(data, name))) {
                (None, _) if self.skipped[i] => (),
                (None, Err(e)) if e.kind() == std::io::ErrorKind::NotFound => (),
                (Some(s), Ok(m)) if !m.file_type().is_symlink() && platform::same(&s.metadata, &m) => (),
                _ => return Err(ClosedError::Changing),
            }
        }
        Ok(())
    }
}

fn header(file: &mut File) -> Result<Vec<u8>, ClosedError> {
    file.seek(SeekFrom::Start(0)).map_err(|_| ClosedError::Unreadable)?;
    let mut bytes = Vec::new();
    file.take(100)
        .read_to_end(&mut bytes)
        .map_err(|_| ClosedError::Unreadable)?;
    Ok(bytes)
}

#[cfg(test)]
#[path = "copy_tests.rs"]
mod tests;
