//! Byte-only acquisition and one persistent private cache. SQLite never sees a source path.
use super::{ClosedError, ClosedSnapshot, ClosedWarning, Control, FileLease, database, keys, platform};
use ibis_core::atomic_file::{create_numbered, create_private, sync_directory};
use ibis_core::contract::Fingerprint;
use std::fs::{File, Metadata};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub(super) const MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const CHUNK: usize = 1024 * 1024;
const ROOT: &str = "ibis-zotero-closed";
const STAMP: &str = "stamp";
/// Only the main database must exist. Better BibTeX is acquired at the same
/// quiet moment; SHM is activity evidence, never copied.
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
const BBT: usize = 4;
const MAX_BBT_FILE: u64 = 256 * 1024 * 1024;
const COPY_ORDER: [usize; 7] = [1, 2, 0, 5, 6, 4, 7];

fn source_path(data: &Path, name: &str) -> PathBuf {
    name.split('/').fold(data.to_owned(), |path, part| path.join(part))
}

pub(super) fn private_name(name: &str) -> String {
    name.replace('/', "-")
}

pub(super) struct Store {
    root: PathBuf,
    _lock: FileLease,
}
impl Store {
    pub(super) fn open(cache: &Path, control: &Control) -> Result<Self, ClosedError> {
        control.check()?;
        let parent = cache_directory(cache, control).map_err(|e| e.context(ClosedError::PrivateStorage))?;
        let root = parent.join(ROOT);
        match std::fs::symlink_metadata(&root) {
            Ok(_) => platform::secure(&root, control)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => platform::create_private(&root, control)?,
            Err(_) => return Err(ClosedError::PrivateStorage),
        }
        let lock = platform::private_lock(&root.join(".lock"), control)?;
        platform::exclude_backups(&root, control)?;
        // Stale numbered temp files are never queried. Remove only ordinary
        // files with our exact staging names while holding the cache lease.
        for entry in std::fs::read_dir(&root).map_err(|_| ClosedError::PrivateStorage)? {
            control.check()?;
            let entry = entry.map_err(|_| ClosedError::PrivateStorage)?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let temporary = FILES
                .iter()
                .map(|name| private_name(name))
                .chain([STAMP.to_owned()])
                .any(|base| {
                    name.strip_prefix(&format!("{base}.tmp-"))
                        .is_some_and(|suffix| suffix.parse::<u16>().is_ok_and(|n| (1..=1000).contains(&n)))
                });
            if temporary && entry.file_type().is_ok_and(|t| t.is_file() && !t.is_symlink()) {
                std::fs::remove_file(entry.path()).map_err(|_| ClosedError::PrivateStorage)?;
            }
        }
        control.check()?;
        Ok(Self { root, _lock: lock })
    }

    pub(super) fn read(&self, data: &Path, control: &Control) -> Result<ClosedSnapshot, ClosedError> {
        self.read_with(data, control, &mut || Ok(()), &mut || platform::closed(control))
    }

    // Synthetic tests force exact race points; the copy hook never runs on a hit.
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
        if overlapping(&directory, &self.root)? {
            return Err(ClosedError::UnsafeSource);
        }
        let mut left = (MAX_BYTES + MAX_BBT_FILE * (FILES.len() - BBT) as u64).saturating_mul(3);
        let mut last = ClosedError::Changing;
        for _ in 0..3 {
            control.check()?;
            check_closed()?;
            match self.attempt(data, &directory, control, &mut left, after_copy, check_closed) {
                Ok(mut snapshot) => {
                    if super::cloud_synced(data) || super::cloud_synced(&directory) {
                        snapshot.warnings.push(ClosedWarning::CloudSyncedDirectory);
                    }
                    return Ok(snapshot);
                }
                Err(error @ (ClosedError::Changing | ClosedError::Damaged)) => {
                    if error == ClosedError::Damaged {
                        remove(&self.root.join(STAMP))?;
                    }
                    last = error;
                }
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
        let stamp = sources.stamp(data)?;
        let copy = Copy {
            path: self.root.clone(),
        };
        let hit = self.matches(&stamp, &sources)?;
        if hit {
            sources.validate(input, data, None, control)?;
            check_closed()?;
        } else {
            let total = size(&sources.files).ok_or(ClosedError::TooLarge)?;
            let needed = total
                .checked_mul(2)
                .and_then(|n| n.checked_add(64 * 1024 * 1024))
                .ok_or(ClosedError::TooLarge)?;
            platform::space(&self.root, needed, control)?;
            let mut staged = Staged {
                files: vec![None; FILES.len()],
            };
            for index in COPY_ORDER {
                if let Some(source) = &mut sources.files[index] {
                    let (path, mut target) =
                        create_numbered(&copy.file(FILES[index]), ".tmp").map_err(|_| ClosedError::PrivateStorage)?;
                    staged.files[index] = Some(path);
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
            sources.validate(input, data, Some(&staged), control)?;
            check_closed()?;
            // No committed stamp can describe a partially installed set. A
            // crash from here until stamp publication leaves no usable cache.
            remove(&self.root.join(STAMP))?;
            sync_directory(&self.root.join(STAMP));
            for (index, temporary) in staged.files.iter_mut().enumerate() {
                let destination = copy.file(FILES[index]);
                if let Some(path) = temporary {
                    std::fs::rename(&*path, destination).map_err(|_| ClosedError::PrivateStorage)?;
                    *temporary = None;
                } else {
                    remove(&destination)?;
                }
            }
            remove(&copy.file("better-bibtex.sqlite-shm"))?;
            sync_directory(&self.root.join(STAMP));
        }
        // Supply every sidecar SQLite might create, on all platforms, through
        // the shared private creator. Read-only SQLite never checkpoints WAL.
        for (main, wal) in [(0, 1), (4, 5)] {
            if sources.files[main]
                .as_ref()
                .is_some_and(|s| s.header.get(19) == Some(&2))
                || sources.files[wal].is_some()
            {
                for name in [FILES[wal].to_owned(), format!("{}-shm", FILES[main])] {
                    let path = copy.file(&name);
                    match platform::open_regular(&path).map_err(|_| ClosedError::PrivateStorage)? {
                        Some(_) => (),
                        None => create_private(&path)
                            .and_then(|f| f.sync_all())
                            .map_err(|_| ClosedError::PrivateStorage)?,
                    }
                }
            }
        }
        let skipped = sources.skipped.contains(&true);
        // Drop every source handle before querying the cache.
        drop(sources);
        let (mut snapshot, zotero) = database::read(&copy, control)?;
        keys::attach(&copy, zotero, skipped, &mut snapshot, control)?;
        control.check()?;
        if !hit {
            let path = self.root.join(STAMP);
            let (temporary, mut file) = create_numbered(&path, ".tmp").map_err(|_| ClosedError::PrivateStorage)?;
            let staged = Staged {
                files: vec![Some(temporary.clone())],
            };
            file.write_all(&stamp)
                .and_then(|()| file.sync_all())
                .map_err(|_| ClosedError::PrivateStorage)?;
            drop(file);
            std::fs::rename(temporary, &path).map_err(|_| ClosedError::PrivateStorage)?;
            sync_directory(&path);
            drop(staged);
        }
        Ok(snapshot)
    }

    fn matches(&self, stamp: &[u8], sources: &Sources) -> Result<bool, ClosedError> {
        let Some(file) = platform::open_regular(&self.root.join(STAMP)).map_err(|_| ClosedError::PrivateStorage)?
        else {
            return Ok(false);
        };
        let mut saved = Vec::new();
        file.take(16 * 1024)
            .read_to_end(&mut saved)
            .map_err(|_| ClosedError::PrivateStorage)?;
        if saved != stamp {
            return Ok(false);
        }
        for (index, name) in FILES.iter().enumerate() {
            if index == 3 {
                continue;
            }
            let cached =
                platform::open_regular(&self.root.join(private_name(name))).map_err(|_| ClosedError::PrivateStorage)?;
            match (&sources.files[index], cached) {
                (Some(source), Some(mut cached))
                    if cached.metadata().is_ok_and(|m| m.len() == source.metadata.len()) =>
                {
                    if fingerprint(&mut cached, source.metadata.len())? != source.fingerprint {
                        return Ok(false);
                    }
                }
                (None, None) => (),
                // SQLite can need an empty WAL even when Zotero removed its own.
                (None, Some(cached)) if matches!(index, 1 | 5) && cached.metadata().is_ok_and(|m| m.len() == 0) => (),
                _ => return Ok(false),
            }
        }
        Ok(true)
    }
}

fn overlapping(data: &Path, cache: &Path) -> Result<bool, ClosedError> {
    #[cfg(windows)]
    {
        use crate::zotero_local::discovery::{Platform, path_identity};
        // Both directories have passed locality/reparse checks. Normalize only
        // for comparison: Windows' case and 8.3 aliases must not let a refresh
        // replace the source with its own cache. Keep original paths for opens.
        let identity = |path: &Path| {
            std::fs::canonicalize(path)
                .map(|path| path_identity(&path, Platform::Windows))
                .map_err(|_| ClosedError::UnsafeSource)
        };
        let data = identity(data)?;
        let cache = identity(cache)?;
        let data: Vec<_> = data.split(|byte| *byte == b'/').collect();
        let cache: Vec<_> = cache.split(|byte| *byte == b'/').collect();
        Ok(data.starts_with(&cache) || cache.starts_with(&data))
    }
    #[cfg(not(windows))]
    Ok(data.starts_with(cache) || cache.starts_with(data))
}

fn cache_directory(path: &Path, control: &Control) -> Result<PathBuf, ClosedError> {
    match platform::directory(path, control) {
        Ok(directory) => return Ok(directory),
        Err(ClosedError::NotFound) => (),
        // Windows' attribute boundary deliberately groups missing entries with
        // unsafe paths. Validate the parent before checking a missing child.
        Err(ClosedError::UnsafeSource) if cfg!(windows) => (),
        Err(error) => return Err(error),
    }
    let parent = path.parent().ok_or(ClosedError::PrivateStorage)?;
    let parent = cache_directory(parent, control)?;
    let path = parent.join(path.file_name().ok_or(ClosedError::PrivateStorage)?);
    control.check()?;
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => platform::create_private(&path, control)?,
        _ => return Err(ClosedError::PrivateStorage),
    }
    platform::directory(&path, control)
}

fn remove(path: &Path) -> Result<(), ClosedError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(ClosedError::PrivateStorage),
    }
}

struct Staged {
    files: Vec<Option<PathBuf>>,
}
impl Drop for Staged {
    fn drop(&mut self) {
        for path in self.files.iter().flatten() {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub(super) struct Copy {
    path: PathBuf,
}
impl Copy {
    pub(super) fn database_path(&self) -> PathBuf {
        self.file("zotero.sqlite")
    }
    pub(super) fn file(&self, name: &str) -> PathBuf {
        self.path.join(private_name(name))
    }
}

struct Source {
    file: File,
    metadata: Metadata,
    header: Vec<u8>,
    fingerprint: Fingerprint,
}
impl Source {
    fn open(path: &Path, seen: &Metadata) -> Result<Self, ClosedError> {
        let mut file = platform::open_source(path)?;
        let metadata = file.metadata().map_err(|_| ClosedError::Unreadable)?;
        if !platform::same(seen, &metadata) {
            return Err(ClosedError::Changing);
        }
        let header = header(&mut file)?;
        let fingerprint = fingerprint(&mut file, metadata.len())?;
        Ok(Self {
            file,
            metadata,
            header,
            fingerprint,
        })
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

    fn stamp(&self, data: &Path) -> Result<Vec<u8>, ClosedError> {
        let files: Vec<_> = self
            .files
            .iter()
            .zip(&self.skipped)
            .map(|(source, skipped)| {
                let Some(source) = source else {
                    return Ok(serde_json::json!({"skipped": skipped}));
                };
                let modified = source.metadata.modified().map_err(|_| ClosedError::Unreadable)?;
                let (before_epoch, time) = match modified.duration_since(SystemTime::UNIX_EPOCH) {
                    Ok(time) => (false, time),
                    Err(error) => (true, error.duration()),
                };
                Ok(serde_json::json!({
                    "size": source.metadata.len(),
                    "mtime": [before_epoch, time.as_secs(), time.subsec_nanos()],
                    "fingerprint": source.fingerprint.as_str(),
                }))
            })
            .collect::<Result<_, ClosedError>>()?;
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "source": Fingerprint::of_bytes(data.as_os_str().as_encoded_bytes()).as_str(),
            "files": files,
        }))
        .map_err(|_| ClosedError::PrivateStorage)
    }

    fn validate(
        &mut self,
        input: &Path,
        data: &Path,
        staged: Option<&Staged>,
        control: &Control,
    ) -> Result<(), ClosedError> {
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
                    ) || source.fingerprint != fingerprint(&mut source.file, source.metadata.len())?
                    {
                        return Err(ClosedError::Changing);
                    }
                    // Cache hits need only the quiet-moment recheck. During
                    // refresh retain the full source/staged byte comparison.
                    let Some(staged) = staged.filter(|_| i != 3) else {
                        continue;
                    };
                    // Full byte comparison defeats coarse/restored mtimes and
                    // detects source WAL changes through the recorded EOF.
                    let mut private = File::open(staged.files[i].as_ref().ok_or(ClosedError::PrivateStorage)?)
                        .map_err(|_| ClosedError::PrivateStorage)?;
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

/// Hash the first and last 64 KiB, including all bytes of smaller files.
fn fingerprint(file: &mut File, size: u64) -> Result<Fingerprint, ClosedError> {
    const SAMPLE: u64 = 64 * 1024;
    let first = size.min(SAMPLE);
    let last = (size - first).min(SAMPLE);
    let mut bytes = vec![0; (first + last) as usize];
    file.rewind().map_err(|_| ClosedError::Unreadable)?;
    file.read_exact(&mut bytes[..first as usize])
        .map_err(|_| ClosedError::Changing)?;
    file.seek(SeekFrom::Start(size - last))
        .map_err(|_| ClosedError::Unreadable)?;
    file.read_exact(&mut bytes[first as usize..])
        .map_err(|_| ClosedError::Changing)?;
    Ok(Fingerprint::of_bytes(&bytes))
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
