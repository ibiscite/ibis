//! Remove only the retired, private flat-file cache; never inspect its payload.
use super::{Control, FileLease, platform};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;

/// Best-effort startup cleanup of the retired source-generations child only.
/// Absent, foreign, insecure or busy storage never becomes a source of records.
/// Keep the control files: unlinking a live lease would allow competing owners.
pub fn remove_obsolete_file_cache(cache: &Path) -> Result<(), PrivateStorageError> {
    let root = cache.join("source-generations");
    match std::fs::symlink_metadata(&root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(PrivateStorageError::Unavailable),
        Ok(_) => (),
    }
    let directory = PrivateDirectory::open(&root)?;
    let mut candidates = Vec::new();
    for (index, entry) in std::fs::read_dir(&directory.root)
        .map_err(|_| PrivateStorageError::Unavailable)?
        .enumerate()
    {
        directory
            .control
            .check()
            .map_err(|_| PrivateStorageError::Unavailable)?;
        if index >= 1024 {
            return Err(PrivateStorageError::Unavailable);
        }
        let entry = entry.map_err(|_| PrivateStorageError::Unavailable)?;
        let name = entry.file_name();
        let Some(name) = name.to_str().filter(|name| obsolete_name(name)) else {
            continue;
        };
        // The old store was flat. Never traverse or delete a directory.
        if entry
            .file_type()
            .map_err(|_| PrivateStorageError::Unavailable)?
            .is_dir()
        {
            continue;
        }
        directory.verify_file(&entry.path())?;
        candidates.push(name.to_owned());
    }
    for name in candidates {
        directory.verify_file(&directory.root.join(&name))?;
        #[cfg(unix)]
        rustix::fs::unlinkat(&directory.root_handle, name.as_str(), rustix::fs::AtFlags::empty())
            .map_err(|_| PrivateStorageError::Unavailable)?;
        #[cfg(windows)]
        std::fs::remove_file(directory.root.join(name)).map_err(|_| PrivateStorageError::Unavailable)?;
    }
    Ok(())
}

fn obsolete_name(name: &str) -> bool {
    let flat = |name: &str| name.len() <= 80 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    ((name.starts_with("generation-") || name.starts_with("stage-") || name.starts_with("manifest-stage-"))
        && flat(name))
        || name
            .strip_prefix("manifest-")
            .and_then(|name| name.strip_suffix(".json"))
            .is_some_and(|name| {
                name.len() == 64 && name.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
}

/// Existing owned cache directory, held only during cleanup.
/// Paths and native errors are never included in Debug or diagnostics.
struct PrivateDirectory {
    root: PathBuf,
    identity: std::fs::Metadata,
    root_handle: std::fs::File,
    control: Control,
    lease: FileLease,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PrivateStorageError {
    #[error("private storage is unavailable")]
    Unavailable,
    #[error("private storage is already in use")]
    Busy,
}

impl PrivateDirectory {
    fn open(path: &Path) -> Result<Self, PrivateStorageError> {
        use std::io::Read;
        let unavailable = |_| PrivateStorageError::Unavailable;
        let control = Control::new(Arc::new(|| false));
        let parent = platform::directory(path.parent().ok_or(PrivateStorageError::Unavailable)?, &control)
            .map_err(unavailable)?;
        let root = parent.join(path.file_name().ok_or(PrivateStorageError::Unavailable)?);
        platform::secure(&root, &control).map_err(unavailable)?;
        const OWNER: &[u8] = b"Ibis prepared source storage\n";
        let marker_path = root.join(".owner");
        platform::secure_file(&marker_path, &control).map_err(unavailable)?;
        let mut marker = Vec::new();
        platform::open_source(&marker_path)
            .map_err(unavailable)?
            .take(64)
            .read_to_end(&mut marker)
            .map_err(|_| PrivateStorageError::Unavailable)?;
        if marker != OWNER {
            return Err(PrivateStorageError::Unavailable);
        }
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(7).custom_flags(0x0020_0000);
        }
        let lock_path = root.join(".lease");
        let lease = options.open(&lock_path).map_err(|_| PrivateStorageError::Unavailable)?;
        platform::secure_file(&lock_path, &control).map_err(unavailable)?;
        let named = std::fs::symlink_metadata(&lock_path).map_err(|_| PrivateStorageError::Unavailable)?;
        if !platform::same(&named, &lease.metadata().map_err(|_| PrivateStorageError::Unavailable)?) {
            return Err(PrivateStorageError::Unavailable);
        }
        let lease = FileLease::acquire(lease).map_err(|_| PrivateStorageError::Busy)?;
        let identity = std::fs::symlink_metadata(&root).map_err(|_| PrivateStorageError::Unavailable)?;
        #[cfg(unix)]
        let root_handle = {
            use rustix::fs::{Mode, OFlags, open};
            std::fs::File::from(
                open(
                    &root,
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )
                .map_err(|_| PrivateStorageError::Unavailable)?,
            )
        };
        #[cfg(windows)]
        let root_handle = {
            use std::os::windows::fs::OpenOptionsExt;
            // No FILE_SHARE_DELETE: the verified directory cannot be moved
            // while path-based Windows deletion is in progress.
            std::fs::OpenOptions::new()
                .read(true)
                .share_mode(3)
                .custom_flags(0x0220_0000)
                .open(&root)
                .map_err(|_| PrivateStorageError::Unavailable)?
        };
        #[cfg(not(any(unix, windows)))]
        return Err(PrivateStorageError::Unavailable);
        let directory = Self {
            root,
            identity,
            root_handle,
            lease,
            control,
        };
        directory.verify()?;
        Ok(directory)
    }

    fn verify(&self) -> Result<(), PrivateStorageError> {
        let control = &self.control;
        platform::secure(&self.root, control).map_err(|_| PrivateStorageError::Unavailable)?;
        let lease_path = self.root.join(".lease");
        platform::secure_file(&lease_path, control).map_err(|_| PrivateStorageError::Unavailable)?;
        let named = std::fs::symlink_metadata(&lease_path).map_err(|_| PrivateStorageError::Unavailable)?;
        if !platform::same(
            &named,
            &self
                .lease
                .file
                .metadata()
                .map_err(|_| PrivateStorageError::Unavailable)?,
        ) {
            return Err(PrivateStorageError::Unavailable);
        }
        // Read identities after the native privacy checks, which can yield.
        let current = std::fs::symlink_metadata(&self.root).map_err(|_| PrivateStorageError::Unavailable)?;
        let held = self
            .root_handle
            .metadata()
            .map_err(|_| PrivateStorageError::Unavailable)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if current.dev() != held.dev()
                || current.ino() != held.ino()
                || current.dev() != self.identity.dev()
                || current.ino() != self.identity.ino()
            {
                return Err(PrivateStorageError::Unavailable);
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if current.creation_time() != held.creation_time()
                || current.file_attributes() != held.file_attributes()
                || current.creation_time() != self.identity.creation_time()
                || current.file_attributes() != self.identity.file_attributes()
            {
                return Err(PrivateStorageError::Unavailable);
            }
        }
        Ok(())
    }

    fn verify_file(&self, path: &Path) -> Result<(), PrivateStorageError> {
        self.verify()?;
        if path.parent() != Some(self.root.as_path()) {
            return Err(PrivateStorageError::Unavailable);
        }
        platform::secure_file(path, &self.control).map_err(|_| PrivateStorageError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let parent = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let root = parent.path().join("source-generations");
        platform::create_private(&root, &Control::new(Arc::new(|| false))).unwrap_or_else(|e| panic!("{e}"));
        write(&root.join(".owner"), b"Ibis prepared source storage\n");
        write(&root.join(".lease"), b"");
        (parent, root)
    }

    fn write(path: &Path, bytes: &[u8]) {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(path)
            .and_then(|mut file| file.write_all(bytes))
            .unwrap_or_else(|e| panic!("{e}"));
    }

    #[test]
    fn absent_cache_is_not_created() {
        let parent = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(remove_obsolete_file_cache(parent.path()), Ok(()));
        assert_eq!(
            std::fs::read_dir(parent.path())
                .unwrap_or_else(|e| panic!("{e}"))
                .count(),
            0
        );
    }

    #[test]
    fn stale_payloads_are_removed_once_but_controls_unknown_entries_and_siblings_survive() {
        let (parent, root) = fixture();
        let names = [
            "generation-invented",
            "stage-invented",
            "manifest-stage-invented",
            "manifest-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.json",
        ];
        for name in names {
            write(&root.join(name), b"untrusted cache");
        }
        write(&root.join("unrecognized"), b"leave this alone");
        write(&root.join("manifest-unrecognized.json"), b"leave this alone");
        write(&parent.path().join("generation-sibling"), b"outside");
        let nested = root.join("generation-directory");
        std::fs::create_dir(&nested).unwrap_or_else(|e| panic!("{e}"));
        write(&nested.join("generation-nested"), b"leave this alone");
        assert_eq!(remove_obsolete_file_cache(parent.path()), Ok(()));
        assert_eq!(remove_obsolete_file_cache(parent.path()), Ok(()));
        for name in names {
            assert!(!root.join(name).exists());
        }
        for path in [
            root.join(".owner"),
            root.join(".lease"),
            root.join("unrecognized"),
            root.join("manifest-unrecognized.json"),
            nested.join("generation-nested"),
            parent.path().join("generation-sibling"),
        ] {
            assert!(path.exists());
        }
    }

    #[test]
    fn busy_lease_defers_cleanup_and_releases_even_with_an_inherited_descriptor() {
        let (parent, root) = fixture();
        write(&root.join("generation-invented"), b"active cache");
        let first = PrivateDirectory::open(&root).unwrap_or_else(|e| panic!("{e}"));
        let inherited = first.lease.file.try_clone().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            remove_obsolete_file_cache(parent.path()),
            Err(PrivateStorageError::Busy)
        );
        assert!(root.join("generation-invented").exists());
        drop(first);
        assert_eq!(remove_obsolete_file_cache(parent.path()), Ok(()));
        drop(inherited);
        assert!(!root.join("generation-invented").exists());
    }

    #[test]
    fn foreign_directory_and_failed_validation_leave_files_intact() {
        let (parent, root) = fixture();
        write(&root.join("generation-invented"), b"keep");
        std::fs::write(root.join(".owner"), b"foreign").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            remove_obsolete_file_cache(parent.path()),
            Err(PrivateStorageError::Unavailable)
        );
        assert_eq!(
            std::fs::read(root.join("generation-invented")).unwrap_or_else(|e| panic!("{e}")),
            b"keep"
        );
        std::fs::remove_file(root.join(".owner")).unwrap_or_else(|e| panic!("{e}"));
        assert!(remove_obsolete_file_cache(parent.path()).is_err());
        assert!(!root.join(".owner").exists());
    }

    #[test]
    fn missing_or_replaced_lease_retires_the_cleanup_owner() {
        let (parent, root) = fixture();
        let first = PrivateDirectory::open(&root).unwrap_or_else(|e| panic!("{e}"));
        std::fs::rename(root.join(".lease"), root.join("retired-lease")).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(first.verify(), Err(PrivateStorageError::Unavailable));
        assert!(remove_obsolete_file_cache(parent.path()).is_err());
        assert!(!root.join(".lease").exists(), "cleanup cannot create a competing lease");
        write(&root.join(".lease"), b"");
        let _second = PrivateDirectory::open(&root).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(first.verify(), Err(PrivateStorageError::Unavailable));
    }

    #[cfg(unix)]
    #[test]
    fn a_replaced_directory_cannot_retain_the_old_cleanup_authority() {
        let (parent, root) = fixture();
        write(&root.join("generation-invented"), b"old private cache");
        let old = PrivateDirectory::open(&root).unwrap_or_else(|e| panic!("{e}"));
        let retired = parent.path().join("retired");
        std::fs::rename(&root, &retired).unwrap_or_else(|e| panic!("{e}"));
        platform::create_private(&root, &Control::new(Arc::new(|| false))).unwrap_or_else(|e| panic!("{e}"));
        write(&root.join(".owner"), b"Ibis prepared source storage\n");
        write(&root.join(".lease"), b"");
        write(&root.join("generation-invented"), b"replacement private cache");
        assert_eq!(
            old.verify_file(&root.join("generation-invented")),
            Err(PrivateStorageError::Unavailable)
        );
        assert_eq!(remove_obsolete_file_cache(parent.path()), Ok(()));
        assert!(!root.join("generation-invented").exists());
        assert_eq!(
            std::fs::read(retired.join("generation-invented")).unwrap_or_else(|e| panic!("{e}")),
            b"old private cache"
        );
    }

    #[cfg(unix)]
    #[test]
    fn linked_directories_and_payloads_never_delete_their_targets() {
        use std::os::unix::fs::symlink;
        let (parent, root) = fixture();
        let outside = parent.path().join("outside");
        write(&outside, b"outside");
        symlink(&outside, root.join("generation-linked")).unwrap_or_else(|e| panic!("{e}"));
        assert!(remove_obsolete_file_cache(parent.path()).is_err());
        std::fs::remove_file(root.join("generation-linked")).unwrap_or_else(|e| panic!("{e}"));
        std::fs::hard_link(&outside, root.join("generation-linked")).unwrap_or_else(|e| panic!("{e}"));
        assert!(remove_obsolete_file_cache(parent.path()).is_err());
        std::fs::remove_file(root.join("generation-linked")).unwrap_or_else(|e| panic!("{e}"));
        std::fs::rename(&root, parent.path().join("saved-root")).unwrap_or_else(|e| panic!("{e}"));
        symlink(parent.path().join("saved-root"), &root).unwrap_or_else(|e| panic!("{e}"));
        assert!(remove_obsolete_file_cache(parent.path()).is_err());
        assert_eq!(std::fs::read(outside).unwrap_or_else(|e| panic!("{e}")), b"outside");
    }
}
