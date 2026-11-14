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
        let name = path.file_name().ok_or(io::ErrorKind::InvalidInput)?;
        for number in 1..=1000 {
            let mut name = name.to_os_string();
            name.push(format!(".tmp-{number}"));
            let temporary = path.with_file_name(name);
            let file = match create_private(&temporary) {
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                other => other?,
            };
            let mut staged = Self {
                file,
                temporary: Temporary {
                    path: temporary,
                    placed: false,
                },
                path: path.to_path_buf(),
            };
            staged.file.write_all(bytes)?;
            return Ok(staged);
        }
        Err(io::ErrorKind::AlreadyExists.into())
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

fn create_private(path: &Path) -> io::Result<File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
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
