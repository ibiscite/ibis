//! Passive source enrollment: retained descriptors and mutation evidence, no locks.
use std::ffi::OsString;
use std::fs::{File, Metadata};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

use rustix::fs::{Mode, OFlags};

use super::{
    CHUNK, Control, DATABASES, Fingerprint, ScratchBudget, SnapshotError, copy_bounded, names, resident, witness,
};

pub(super) struct Sources {
    files: Vec<(String, File)>,
    directories: Vec<File>,
    links: Vec<(usize, OsString, usize)>,
    root: usize,
    shadow: Option<usize>,
    input: PathBuf,
    resolved: PathBuf,
    witness: witness::Witness,
}

impl Sources {
    pub(super) fn acquire(data: &Path, seen: &Fingerprint, control: &Control) -> Result<Self, SnapshotError> {
        let resolved = super::mounts::resolved(data)?;
        let mut directories = vec![directory(None, Path::new("/"))?];
        let mut links = Vec::new();
        for component in resolved.components() {
            control.check()?;
            if let Component::Normal(name) = component {
                let parent = directories.len() - 1;
                let child = directory(Some(&directories[parent]), Path::new(name))?;
                links.push((parent, name.to_os_string(), directories.len()));
                directories.push(child);
            }
        }
        let root = directories.len() - 1;
        let mut witness = witness::Witness::new()?;
        witness.watch(&directories[root])?;
        let shadow = match directory(Some(&directories[root]), Path::new("better-bibtex")) {
            Ok(dir) => {
                witness.watch(&dir)?;
                let id = directories.len();
                links.push((root, OsString::from("better-bibtex"), id));
                directories.push(dir);
                Some(id)
            }
            Err(SnapshotError::NotFound) => None,
            Err(error) => return Err(error),
        };
        let mut files = Vec::new();
        for (name, state) in names().zip(&seen.0) {
            control.check()?;
            let parent = if name.contains('/') { shadow } else { Some(root) };
            let file = if let Some(parent) = parent {
                open(
                    &directories[parent],
                    Path::new(&name).file_name().ok_or(SnapshotError::Changing)?.as_ref(),
                )?
            } else {
                None
            };
            if file.is_some() != state.is_some() {
                return Err(SnapshotError::Changing);
            }
            if let Some(file) = file {
                witness.watch(&file)?;
                files.push((name, file));
            }
        }
        let mut result = Self {
            files,
            directories,
            links,
            root,
            shadow,
            input: data.to_owned(),
            resolved,
            witness,
        };
        result.check(control)?;
        if !result.same_files(data) {
            return Err(SnapshotError::Changing);
        }
        Ok(result)
    }

    fn check(&mut self, control: &Control) -> Result<(), SnapshotError> {
        control.check()?;
        self.witness.unchanged()?;
        for (name, file) in &self.files {
            if DATABASES.contains(&name.as_str()) {
                passive(file)?;
            }
        }
        self.witness.unchanged()
    }

    pub(super) fn same_files(&mut self, _: &Path) -> bool {
        let checked = || -> Result<(), SnapshotError> {
            if super::mounts::resolved(&self.input)? != self.resolved {
                return Err(SnapshotError::Changing);
            }
            for (parent, name, child) in &self.links {
                let current = directory(Some(&self.directories[*parent]), Path::new(name))?;
                if !same(
                    &current.metadata().map_err(|_| SnapshotError::Changing)?,
                    &self.directories[*child]
                        .metadata()
                        .map_err(|_| SnapshotError::Changing)?,
                ) {
                    return Err(SnapshotError::Changing);
                }
            }
            if self.shadow.is_none()
                && directory(Some(&self.directories[self.root]), Path::new("better-bibtex")).err()
                    != Some(SnapshotError::NotFound)
            {
                return Err(SnapshotError::Changing);
            }
            for name in names() {
                let parent = if name.contains('/') {
                    self.shadow
                } else {
                    Some(self.root)
                };
                let current = if let Some(parent) = parent {
                    open(
                        &self.directories[parent],
                        Path::new(&name).file_name().ok_or(SnapshotError::Changing)?.as_ref(),
                    )?
                } else {
                    None
                };
                let old = self.files.iter().find(|(old, _)| old == &name);
                match (current, old) {
                    (None, None) => {}
                    (Some(current), Some((_, old)))
                        if same(
                            &current.metadata().map_err(|_| SnapshotError::Changing)?,
                            &old.metadata().map_err(|_| SnapshotError::Changing)?,
                        ) => {}
                    _ => return Err(SnapshotError::Changing),
                }
            }
            Ok(())
        };
        self.witness.unchanged().is_ok() && checked().is_ok() && self.witness.unchanged().is_ok()
    }

    #[cfg(test)]
    pub(super) fn copy_to(&mut self, target: &Path, control: &Control) -> Result<Vec<u64>, SnapshotError> {
        self.copy_to_budget(
            target,
            control,
            &mut ScratchBudget {
                remaining: super::MAX_DATABASE_BYTES,
            },
        )
    }

    pub(super) fn copy_to_budget(
        &mut self,
        target: &Path,
        control: &Control,
        budget: &mut ScratchBudget,
    ) -> Result<Vec<u64>, SnapshotError> {
        self.check(control)?;
        // Every WAL reaches its actual EOF before any main database is copied.
        self.files.sort_by_key(|(name, _)| {
            if name.ends_with("-wal") {
                0
            } else if DATABASES.contains(&name.as_str()) {
                1
            } else {
                2
            }
        });
        let mut hashes = Vec::new();
        let mut wals = Vec::new();
        for (name, source) in &mut self.files {
            control.check()?;
            self.witness.unchanged()?;
            if name.ends_with("-shm") {
                continue;
            }
            if name.contains('/') {
                std::fs::create_dir(target.join("better-bibtex")).map_err(|_| SnapshotError::CopyFailed)?;
            }
            use std::os::unix::fs::OpenOptionsExt;
            let output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(target.join(&name))
                .map_err(|_| SnapshotError::CopyFailed)?;
            let copied = copy_bounded(source, output, budget, control, &mut || self.witness.unchanged())?;
            hashes.push(copied.hash);
            if name.ends_with("-wal") {
                wals.push((name.clone(), copied.bytes));
            }
        }
        for (name, length) in wals {
            let source = &mut self
                .files
                .iter_mut()
                .find(|(file, _)| file == &name)
                .ok_or(SnapshotError::Changing)?
                .1;
            let mut private = File::open(target.join(name)).map_err(|_| SnapshotError::CopyFailed)?;
            source.seek(SeekFrom::Start(0)).map_err(|_| SnapshotError::Changing)?;
            let mut left = vec![0; CHUNK];
            let mut right = vec![0; CHUNK];
            let mut remaining = length;
            while remaining > 0 {
                control.check()?;
                self.witness.unchanged()?;
                let count = remaining.min(CHUNK as u64) as usize;
                source
                    .read_exact(&mut left[..count])
                    .map_err(|_| SnapshotError::Changing)?;
                private
                    .read_exact(&mut right[..count])
                    .map_err(|_| SnapshotError::CopyFailed)?;
                if left[..count] != right[..count] {
                    return Err(SnapshotError::Changing);
                }
                remaining -= count as u64;
            }
            control.check()?;
            self.witness.unchanged()?;
            if source.read(&mut left[..1]).map_err(|_| SnapshotError::Changing)? != 0
                || private.read(&mut right[..1]).map_err(|_| SnapshotError::CopyFailed)? != 0
            {
                return Err(SnapshotError::Changing);
            }
        }
        self.check(control)?;
        if !self.same_files(&self.input.clone()) {
            return Err(SnapshotError::Changing);
        }
        Ok(hashes)
    }
}

fn directory(parent: Option<&File>, path: &Path) -> Result<File, SnapshotError> {
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
    let fd = match parent {
        Some(parent) => rustix::fs::openat(parent, path, flags, Mode::empty()),
        None => rustix::fs::open(path, flags, Mode::empty()),
    };
    fd.map(File::from).map_err(|error| {
        if error == rustix::io::Errno::NOENT {
            SnapshotError::NotFound
        } else {
            SnapshotError::Changing
        }
    })
}

fn open(parent: &File, name: &Path) -> Result<Option<File>, SnapshotError> {
    match rustix::fs::openat(
        parent,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => {
            let file = File::from(fd);
            let metadata = file.metadata().map_err(|_| SnapshotError::Changing)?;
            if !metadata.is_file() || !resident(&metadata) {
                return Err(SnapshotError::NotLocal);
            }
            Ok(Some(file))
        }
        Err(error) if error == rustix::io::Errno::NOENT => Ok(None),
        Err(_) => Err(SnapshotError::Changing),
    }
}

fn same(a: &Metadata, b: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino() && a.file_type() == b.file_type()
}

fn passive(file: &File) -> Result<(), SnapshotError> {
    use nix::fcntl::{FcntlArg, fcntl};
    let mut request = nix::libc::flock {
        l_start: 0,
        l_len: 0,
        l_pid: 0,
        l_type: nix::libc::F_WRLCK as _,
        l_whence: nix::libc::SEEK_SET as _,
    };
    fcntl(file, FcntlArg::F_GETLK(&mut request)).map_err(|_| SnapshotError::Changing)?;
    if request.l_type != nix::libc::F_UNLCK as nix::libc::c_short {
        return Err(SnapshotError::Changing);
    }
    Ok(())
}
