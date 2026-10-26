//! Raw source handles, held together while copying. SQLite never opens them.

use std::fs::{File, Metadata};
use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use super::{Control, DATABASES, Fingerprint, MAX_DATABASE_BYTES, SnapshotError, copy_bounded, names, resident};

// POSIX record locks belong to the process. Another in-process acquisition
// must not close a descriptor for an inode whose lock this read still owns.
static ACQUISITION: Mutex<()> = Mutex::new(());

pub(super) struct Sources {
    files: Vec<(String, File)>,
    #[cfg(windows)]
    _directories: Vec<File>,
    _owner: MutexGuard<'static, ()>,
}

impl Sources {
    pub(super) fn acquire(data: &Path, seen: &Fingerprint, control: &Control) -> Result<Self, SnapshotError> {
        let owner = loop {
            control.check()?;
            match ACQUISITION.try_lock() {
                Ok(owner) => break owner,
                Err(std::sync::TryLockError::Poisoned(_)) => return Err(SnapshotError::Changing),
                Err(std::sync::TryLockError::WouldBlock) => std::thread::sleep(std::time::Duration::from_millis(5)),
            }
        };
        #[cfg(windows)]
        let directories = directory_guards(data)?;
        let total = seen
            .0
            .iter()
            .flatten()
            .try_fold(0u64, |total, (len, _)| total.checked_add(*len));
        if total.is_none_or(|total| total > MAX_DATABASE_BYTES) {
            return Err(SnapshotError::TooLarge);
        }
        let present: Vec<_> = names()
            .zip(&seen.0)
            .filter(|(_, state)| state.is_some())
            .map(|(name, _)| name)
            .collect();
        let mut files = Vec::new();
        // Lock both main DBs before inspecting sidecars. An exclusive lock on
        // the main inode excludes WAL startup even when no -shm exists yet.
        for main in DATABASES {
            if present.iter().any(|name| name == main) {
                let file = open(&data.join(main), true)?;
                #[cfg(unix)]
                rustix::fs::fcntl_lock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive)
                    .map_err(|_| SnapshotError::Changing)?;
                files.push((main.to_string(), file));
            }
        }
        for name in present {
            if !DATABASES.contains(&name.as_str()) {
                files.push((name.clone(), open(&data.join(name), false)?));
            }
        }
        Ok(Self {
            files,
            _owner: owner,
            #[cfg(windows)]
            _directories: directories,
        })
    }

    pub(super) fn same_files(&self, data: &Path) -> bool {
        self.files.iter().all(|(name, file)| {
            file.metadata()
                .ok()
                .zip(std::fs::symlink_metadata(data.join(name)).ok())
                .is_some_and(|(opened, current)| same(&opened, &current))
        })
    }

    pub(super) fn copy_to(&mut self, target: &Path, control: &Control) -> Result<Vec<u64>, SnapshotError> {
        let mut hashes = Vec::new();
        for (name, source) in &mut self.files {
            control.check()?;
            if name.ends_with("-shm") {
                continue;
            }
            if name.contains('/') {
                std::fs::create_dir(target.join("better-bibtex")).map_err(|_| SnapshotError::CopyFailed)?;
            }
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
            let output = options.open(target.join(name)).map_err(|_| SnapshotError::CopyFailed)?;
            hashes.push(copy_bounded(source, Some(output), MAX_DATABASE_BYTES, control)?);
        }
        Ok(hashes)
    }
}

fn open(path: &Path, main: bool) -> Result<File, SnapshotError> {
    #[cfg(unix)]
    let file = {
        use rustix::fs::{Mode, OFlags};
        // A writable descriptor is required for F_WRLCK; it is used only for
        // that lock and reads. No source write or chmod is ever attempted.
        let access = if main { OFlags::RDWR } else { OFlags::RDONLY };
        File::from(
            rustix::fs::open(
                path,
                access | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|_| SnapshotError::Unreadable)?,
        )
    };
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        let _ = main;
        // A read-only handle that shares only reads excludes existing/new
        // writers and deletion for its whole lifetime, including WAL startup.
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(0x0020_0000)
            .open(path)
            .map_err(|_| SnapshotError::Changing)?
    };
    #[cfg(not(any(unix, windows)))]
    let file: File = {
        let _ = (path, main);
        return Err(SnapshotError::NotLocal);
    };
    let metadata = file.metadata().map_err(|_| SnapshotError::Unreadable)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(SnapshotError::Unreadable);
    }
    if !resident(&metadata) {
        return Err(SnapshotError::NotLocal);
    }
    Ok(file)
}

fn same(opened: &Metadata, current: &Metadata) -> bool {
    if !current.is_file() || current.file_type().is_symlink() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        opened.dev() == current.dev() && opened.ino() == current.ino()
    }
    #[cfg(not(unix))]
    {
        opened.len() == current.len() && opened.modified().ok() == current.modified().ok()
    }
}

#[cfg(windows)]
pub(super) fn directory_guard(path: &Path) -> Result<File, SnapshotError> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    let file = std::fs::OpenOptions::new()
        .access_mode(0)
        .share_mode(1)
        .custom_flags(0x0200_0000 | 0x0020_0000)
        .open(path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                SnapshotError::NotFound
            } else {
                SnapshotError::NotLocal
            }
        })?;
    let metadata = file.metadata().map_err(|_| SnapshotError::NotLocal)?;
    if !metadata.is_dir() || metadata.file_attributes() & 0x0044_1400 != 0 {
        return Err(SnapshotError::NotLocal);
    }
    Ok(file)
}

#[cfg(windows)]
pub(super) fn directory_guards(path: &Path) -> Result<Vec<File>, SnapshotError> {
    if !super::windows::drive(path) {
        return Err(SnapshotError::NotLocal);
    }
    let mut guarded = Vec::new();
    let mut part = PathBuf::new();
    for component in path.components() {
        part.push(component);
        if part.has_root() {
            guarded.push(directory_guard(&part)?);
        }
    }
    Ok(guarded)
}
