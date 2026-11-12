//! Settings writes are confined to a chosen home. Unix walks and mutations use
//! directory handles with NOFOLLOW, including backups and temporary files.
use super::{Error, Result};
#[cfg(not(unix))]
use std::fs;
use std::{
    fs::{File, Metadata},
    io::Read,
    path::{Component, Path, PathBuf},
};
#[cfg(unix)]
use std::{
    io::Write,
    sync::atomic::{AtomicU64, Ordering},
};

const LIMIT: u64 = 4 * 1024 * 1024;
#[cfg(unix)]
static NEXT: AtomicU64 = AtomicU64::new(0);
type Directory = (File, Vec<(PathBuf, Identity)>);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Identity {
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
    #[cfg(unix)]
    mode: u32,
    #[cfg(unix)]
    links: u64,
    len: u64,
    modified: Option<std::time::SystemTime>,
    #[cfg(unix)]
    changed: (i64, i64),
}

fn identity(meta: &Metadata, directory: bool) -> Identity {
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    Identity {
        #[cfg(unix)]
        dev: meta.dev(),
        #[cfg(unix)]
        ino: meta.ino(),
        #[cfg(unix)]
        mode: meta.mode(),
        #[cfg(unix)]
        links: if directory { 0 } else { meta.nlink() },
        len: if directory { 0 } else { meta.len() },
        modified: if directory { None } else { meta.modified().ok() },
        #[cfg(unix)]
        changed: if directory {
            (0, 0)
        } else {
            (meta.ctime(), meta.ctime_nsec())
        },
    }
}

fn safe(meta: &Metadata, path: &Path, directory: bool) -> Result<()> {
    if (directory && !meta.is_dir()) || (!directory && !meta.is_file()) || meta.file_type().is_symlink() {
        return Err(Error::at(path, "refusing a symlink or a special file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.mode() & 0o002 != 0 {
            return Err(Error::at(path, "refusing a world-writable location"));
        }
        if !directory && meta.nlink() != 1 {
            return Err(Error::at(path, "refusing a file with multiple hard links"));
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub(super) struct Snapshot {
    pub bytes: Option<Vec<u8>>,
    file: Option<Identity>,
    directories: Vec<(PathBuf, Identity)>,
}

/// Paths are constructed by adapters, never by receipts or client config text.
#[derive(Debug, Clone)]
pub(super) struct Target {
    pub home: PathBuf,
    pub path: PathBuf,
}

impl Target {
    fn relative(&self) -> Result<&Path> {
        let path = self
            .path
            .strip_prefix(&self.home)
            .map_err(|_| Error::at(&self.path, "location must be inside the home directory"))?;
        if path.components().any(|c| !matches!(c, Component::Normal(_))) || path.file_name().is_none() {
            return Err(Error::at(&self.path, "invalid settings location"));
        }
        Ok(path)
    }

    #[cfg(unix)]
    fn directory(&self, create: bool) -> Result<Option<Directory>> {
        use rustix::fs::{Mode, OFlags, mkdirat, open, openat};
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let mut directory =
            File::from(open(&self.home, flags, Mode::empty()).map_err(|e| Error::io(&self.home, e.into()))?);
        let mut path = self.home.clone();
        let mut identities = vec![];
        let mut parts: Vec<_> = self.relative()?.components().collect();
        parts.pop();
        for part in std::iter::once(None).chain(parts.iter().map(Some)) {
            if let Some(part) = part {
                let name = part.as_os_str();
                path.push(name);
                let fd = match openat(&directory, name, flags, Mode::empty()) {
                    Ok(fd) => fd,
                    Err(rustix::io::Errno::NOENT) if create => {
                        match mkdirat(&directory, name, Mode::from_raw_mode(0o700)) {
                            Ok(()) | Err(rustix::io::Errno::EXIST) => {}
                            Err(e) => return Err(Error::io(&path, e.into())),
                        }
                        openat(&directory, name, flags, Mode::empty()).map_err(|e| Error::io(&path, e.into()))?
                    }
                    Err(rustix::io::Errno::NOENT) => return Ok(None),
                    Err(e) => return Err(Error::io(&path, e.into())),
                };
                directory = File::from(fd);
            }
            let meta = directory.metadata().map_err(|e| Error::io(&path, e))?;
            safe(&meta, &path, true)?;
            identities.push((path.clone(), identity(&meta, true)));
        }
        Ok(Some((directory, identities)))
    }

    #[cfg(not(unix))]
    fn directory(&self, create: bool) -> Result<Option<Directory>> {
        let mut path = self.home.clone();
        let mut identities = vec![];
        let mut parts: Vec<_> = self.relative()?.components().collect();
        parts.pop();
        for part in std::iter::once(None).chain(parts.iter().map(Some)) {
            if let Some(part) = part {
                path.push(part.as_os_str());
            }
            let meta = match fs::symlink_metadata(&path) {
                Ok(meta) => meta,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound && create => {
                    fs::create_dir(&path).map_err(|e| Error::io(&path, e))?;
                    fs::symlink_metadata(&path).map_err(|e| Error::io(&path, e))?
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(Error::io(&path, e)),
            };
            safe(&meta, &path, true)?;
            identities.push((path.clone(), identity(&meta, true)));
        }
        // Directory-relative, no-follow operations are required for this boundary.
        // Refuse rather than make a weaker promise on an unsupported platform.
        Err(Error::at(
            &self.path,
            "safe settings writes are not available on this platform yet",
        ))
    }

    pub fn read(&self) -> Result<Snapshot> {
        let directory = self.directory(false)?;
        let Some((directory, directories)) = directory else {
            // Capture the nearest existing parent too, for ancestor-swap checks.
            let mut parent = self.clone();
            while !parent.path.parent().is_some_and(|p| p == self.home) {
                parent.path.pop();
                if let Some((_, directories)) = parent.directory(false)? {
                    return Ok(Snapshot {
                        bytes: None,
                        file: None,
                        directories,
                    });
                }
            }
            return Ok(Snapshot {
                bytes: None,
                file: None,
                directories: vec![],
            });
        };
        self.read_in(&directory, directories)
    }

    fn read_in(&self, _directory: &File, directories: Vec<(PathBuf, Identity)>) -> Result<Snapshot> {
        #[cfg(unix)]
        let opened = rustix::fs::openat(
            _directory,
            self.path
                .file_name()
                .ok_or_else(|| Error::at(&self.path, "missing filename"))?,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .map(File::from)
        .map_err(std::io::Error::from);
        #[cfg(not(unix))]
        let opened = File::open(&self.path);
        let mut file = match opened {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Snapshot {
                    bytes: None,
                    file: None,
                    directories,
                });
            }
            Err(e) => return Err(Error::io(&self.path, e)),
        };
        let meta = file.metadata().map_err(|e| Error::io(&self.path, e))?;
        safe(&meta, &self.path, false)?;
        if meta.len() > LIMIT {
            return Err(Error::at(&self.path, "settings file is too large"));
        }
        let mut bytes = vec![];
        Read::by_ref(&mut file)
            .take(LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| Error::io(&self.path, e))?;
        if bytes.len() as u64 > LIMIT
            || identity(&meta, false) != identity(&file.metadata().map_err(|e| Error::io(&self.path, e))?, false)
        {
            return Err(Error::at(&self.path, "file changed while being read; try again"));
        }
        Ok(Snapshot {
            bytes: Some(bytes),
            file: Some(identity(&meta, false)),
            directories,
        })
    }

    pub fn check(&self, before: &Snapshot) -> Result<()> {
        let current = self.read()?;
        if current.bytes != before.bytes
            || current.file != before.file
            || before.directories.iter().any(|d| !current.directories.contains(d))
        {
            return Err(Error::at(
                &self.path,
                "file or directory changed since it was read; try again",
            ));
        }
        Ok(())
    }

    pub fn write(&self, before: &Snapshot, bytes: Option<&[u8]>, backup: bool) -> Result<()> {
        self.check(before)?;
        if before.bytes.as_deref() == bytes {
            return Ok(());
        }
        let (directory, _) = self
            .directory(true)?
            .ok_or_else(|| Error::at(&self.path, "missing directory"))?;
        if backup && let Some(original) = &before.bytes {
            let mut backup_path = self.path.as_os_str().to_owned();
            backup_path.push(".ibis-backup");
            let target = Target {
                home: self.home.clone(),
                path: backup_path.into(),
            };
            let saved = target.read()?;
            if saved.bytes.is_none() {
                target.atomic(&directory, &saved, Some(original), before.file.as_ref())?;
            }
        }
        self.check(before)?;
        self.atomic(&directory, before, bytes, before.file.as_ref())
    }

    #[cfg(unix)]
    fn atomic(
        &self,
        directory: &File,
        before: &Snapshot,
        bytes: Option<&[u8]>,
        permissions: Option<&Identity>,
    ) -> Result<()> {
        use rustix::fs::{AtFlags, Mode, OFlags, fchmod, linkat, openat, renameat, unlinkat};
        let name = self
            .path
            .file_name()
            .ok_or_else(|| Error::at(&self.path, "missing filename"))?;
        let Some(bytes) = bytes else {
            self.check(before)?;
            return unlinkat(directory, name, AtFlags::empty()).map_err(|e| Error::io(&self.path, e.into()));
        };
        let temporary = format!(
            ".ibis-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let fd = openat(
            directory,
            &temporary,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )
        .map_err(|e| Error::io(&self.path, e.into()))?;
        let result = (|| {
            let mut file = File::from(fd);
            file.write_all(bytes).map_err(|e| Error::io(&self.path, e))?;
            if let Some(permissions) = permissions {
                fchmod(&file, Mode::from_bits_truncate(permissions.mode as _))
                    .map_err(|e| Error::io(&self.path, e.into()))?;
            }
            file.sync_all().map_err(|e| Error::io(&self.path, e))?;
            self.check(before)?;
            if before.bytes.is_none() {
                // Publishing a previously absent file must never replace a file
                // that another process created after our last check.
                linkat(directory, &temporary, directory, name, AtFlags::empty())
                    .map_err(|e| Error::io(&self.path, e.into()))?;
                unlinkat(directory, &temporary, AtFlags::empty()).map_err(|e| Error::io(&self.path, e.into()))?;
            } else {
                renameat(directory, &temporary, directory, name).map_err(|e| Error::io(&self.path, e.into()))?;
            }
            directory.sync_all().map_err(|e| Error::io(&self.path, e))
        })();
        if result.is_err() {
            let _ = unlinkat(directory, &temporary, AtFlags::empty());
        }
        result
    }

    #[cfg(not(unix))]
    fn atomic(&self, _: &File, _: &Snapshot, _: Option<&[u8]>, _: Option<&Identity>) -> Result<()> {
        Err(Error::at(
            &self.path,
            "safe settings writes are not available on this platform yet",
        ))
    }
}
