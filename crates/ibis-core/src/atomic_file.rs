//! Whole-file writes, with private staging and permission-preserving replacement.

use std::fs::{self, File, Permissions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// A staged write. Dropping it leaves the destination alone and removes only
/// its own temporary file. Callers may check a transaction before committing.
pub struct AtomicFile {
    file: File,
    temporary: Temporary,
    path: PathBuf,
}

impl AtomicFile {
    pub fn new(path: &Path, bytes: &[u8]) -> io::Result<Self> {
        permissions(path)?;
        let (temporary, file) = create_numbered(path, ".tmp")?;
        let mut staged = Self {
            file,
            temporary: Temporary {
                path: temporary,
                placed: false,
            },
            path: path.to_path_buf(),
        };
        staged.file.write_all(bytes)?;
        Ok(staged)
    }

    /// Refuse a read-only target with `PermissionDenied`, even if the folder
    /// permits renaming over it. Keep an existing target's permissions.
    pub fn replace(mut self) -> io::Result<()> {
        if let Some(permissions) = permissions(&self.path)? {
            self.file.set_permissions(permissions)?;
        }
        self.file.sync_all()?;
        drop(self.file);
        fs::rename(&self.temporary.path, &self.path)?;
        self.temporary.placed = true;
        sync_directory(&self.path);
        Ok(())
    }

    /// Take a free name without replacing a file that appears during the write.
    /// Drives without hard links use exclusive creation instead.
    pub fn create(self) -> io::Result<()> {
        self.file.sync_all()?;
        drop(self.file);
        match fs::hard_link(&self.temporary.path, &self.path) {
            Err(error) if error.kind() != io::ErrorKind::AlreadyExists => {
                let mut file = create_private(&self.path)?;
                File::open(&self.temporary.path)
                    .and_then(|mut source| io::copy(&mut source, &mut file))
                    .and_then(|_| file.sync_all())
                    .inspect_err(|_| {
                        let _ = fs::remove_file(&self.path);
                    })?;
            }
            other => other?,
        }
        sync_directory(&self.path);
        Ok(())
    }
}

/// Why [`replace_keeping_backup`] changed nothing.
#[derive(Debug)]
pub enum BackupReplaceError<E> {
    /// The new text could not be staged beside the file.
    Stage(io::Error),
    /// The backup of the old text could not be written.
    Backup,
    /// The caller's check failed after staging: usually, the file changed.
    Check(E),
    /// The check failed and the new backup could not be removed either.
    BackupLeft(E),
    /// The staged file could not take the file's place.
    Write(io::Error),
}

/// Write `after` in place of `before` at `path`, keeping `before` in a private
/// numbered backup `path.ibis-backup-N`, or create `path` when `before` is
/// `None`. `check` runs once everything is staged, just before the rename, so
/// the caller can refuse if another program saved the file meanwhile; a
/// backup is kept only beside a change that was made.
pub fn replace_keeping_backup<E>(
    path: &Path,
    before: Option<&[u8]>,
    after: &[u8],
    check: impl FnOnce() -> Result<(), E>,
) -> Result<Option<PathBuf>, BackupReplaceError<E>> {
    let staged = AtomicFile::new(path, after).map_err(BackupReplaceError::Stage)?;
    let backup = match before {
        Some(before) => {
            let (backup, mut file) = create_numbered(path, ".ibis-backup").map_err(|_| BackupReplaceError::Backup)?;
            file.write_all(before).and_then(|()| file.sync_all()).map_err(|_| {
                let _ = fs::remove_file(&backup);
                BackupReplaceError::Backup
            })?;
            Some(backup)
        }
        None => None,
    };
    if let Err(reason) = check() {
        return Err(match &backup {
            Some(backup) if fs::remove_file(backup).is_err() => BackupReplaceError::BackupLeft(reason),
            _ => BackupReplaceError::Check(reason),
        });
    }
    let written = if before.is_some() {
        staged.replace()
    } else {
        staged.create()
    };
    written.map_err(|error| {
        if let Some(backup) = &backup {
            let _ = fs::remove_file(backup);
        }
        BackupReplaceError::Write(error)
    })?;
    Ok(backup)
}

struct Temporary {
    path: PathBuf,
    placed: bool,
}

impl Drop for Temporary {
    fn drop(&mut self) {
        if !self.placed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn permissions(path: &Path) -> io::Result<Option<Permissions>> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.permissions().readonly() => Err(io::ErrorKind::PermissionDenied.into()),
        Ok(metadata) if metadata.is_file() => Ok(Some(metadata.permissions())),
        Ok(_) => Err(io::ErrorKind::InvalidInput.into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Create a new file with Unix mode 0600 and otherwise default security.
/// Refuse every existing entry.
pub fn create_private(path: &Path) -> io::Result<File> {
    private_options().create_new(true).open(path)
}

/// Open a reusable private lock without changing existing bytes or permissions.
/// Refuse Unix symlinks.
pub fn open_private(path: &Path) -> io::Result<File> {
    let mut options = private_options();
    options.read(true).create(true).truncate(false);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(&mut options, nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
    options.open(path)
}

fn private_options() -> fs::OpenOptions {
    let mut options = fs::OpenOptions::new();
    options.write(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
}

/// Exclusively create `path{suffix}-N`, trying 1 through 1000 in order.
pub fn create_numbered(path: &Path, suffix: &str) -> io::Result<(PathBuf, File)> {
    let name = path.file_name().ok_or(io::ErrorKind::InvalidInput)?;
    for number in 1..=1000 {
        let mut name = name.to_os_string();
        name.push(format!("{suffix}-{number}"));
        let path = path.with_file_name(name);
        match create_private(&path) {
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            file => return Ok((path, file?)),
        }
    }
    Err(io::ErrorKind::AlreadyExists.into())
}

/// Best effort directory flush: the file is already committed. Windows journals
/// the rename and cannot open a directory as a file.
pub fn sync_directory(path: &Path) {
    #[cfg(unix)]
    if let Some(directory) = path.parent().filter(|directory| !directory.as_os_str().is_empty()) {
        let _ = File::open(directory).and_then(|directory| directory.sync_all());
    }
    #[cfg(not(unix))]
    let _ = path;
}
