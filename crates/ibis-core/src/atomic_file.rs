//! Whole-file writes, with private staging and permission-preserving replacement.

use std::fs::{self, File, Permissions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[cfg(windows)]
#[allow(unsafe_code)]
mod windows;

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

/// Create an owner-only file, refusing every existing entry.
pub fn create_private(path: &Path) -> io::Result<File> {
    #[cfg(windows)]
    return windows::open(path, true);
    #[cfg(not(windows))]
    private_options().create_new(true).open(path)
}

/// Open a reusable private lock without changing existing bytes or permissions.
/// Refuse Unix symlinks; Windows keeps the lock's name in place while it is open.
pub fn open_private(path: &Path) -> io::Result<File> {
    #[cfg(windows)]
    return windows::open(path, false);
    #[cfg(not(windows))]
    {
        let mut options = private_options();
        options.create(true).truncate(false);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::custom_flags(&mut options, nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
        options.open(path)
    }
}

#[cfg(not(windows))]
fn private_options() -> fs::OpenOptions {
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true);
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
