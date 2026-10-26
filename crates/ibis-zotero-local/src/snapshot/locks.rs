//! Raw source handles, held together while copying. SQLite never opens them.

use std::fs::{File, Metadata};
use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use super::{
    Control, DATABASES, Fingerprint, MAX_DATABASE_BYTES, ScratchBudget, SnapshotError, copy_bounded, names, resident,
};

// Windows remains a draft implementation: its sharing mode still interferes
// with startup. Unix uses the separate passive witness implementation.
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

    #[cfg(test)]
    pub(super) fn copy_to(&mut self, target: &Path, control: &Control) -> Result<Vec<u64>, SnapshotError> {
        self.copy_to_budget(
            target,
            control,
            &mut ScratchBudget {
                remaining: MAX_DATABASE_BYTES,
            },
        )
    }

    pub(super) fn copy_to_budget(
        &mut self,
        target: &Path,
        control: &Control,
        budget: &mut ScratchBudget,
    ) -> Result<Vec<u64>, SnapshotError> {
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
            hashes.push(copy_bounded(source, output, budget, control, &mut || Ok(()))?.hash);
        }
        Ok(hashes)
    }
}

fn open(path: &Path, main: bool) -> Result<File, SnapshotError> {
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
    #[cfg(not(windows))]
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
