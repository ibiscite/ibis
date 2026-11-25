//! Native boundaries used only by the closed reader. No source lock is acquired.
use super::{ClosedError, Control, FileLease, ParentLinks};
use std::fs::{File, Metadata};
use std::io::Read;
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::{Command, Stdio};
#[cfg(unix)]
use std::time::Duration;

/// Conservative: refuse every running Zotero, even a different profile. An
/// incomplete/failed process enumeration is never evidence that Zotero is closed.
pub(super) fn closed(control: &Control) -> Result<(), ClosedError> {
    control.check()?;
    #[cfg(unix)]
    {
        if std::env::var_os("FLATPAK_ID").is_some() || Path::new("/.flatpak-info").exists() {
            return Err(ClosedError::ProcessCheck);
        }
        let bytes = output(Command::new("/bin/ps").args(["-A", "-ww", "-o", "pid=,comm="]), control)?;
        process_list(&bytes, std::process::id())
    }
    #[cfg(windows)]
    {
        windows::closed(control)
    }
    #[cfg(not(any(unix, windows)))]
    Err(ClosedError::ProcessCheck)
}

#[cfg(unix)]
fn process_list(bytes: &[u8], own_pid: u32) -> Result<(), ClosedError> {
    let text = std::str::from_utf8(bytes).map_err(|_| ClosedError::ProcessCheck)?;
    let mut seen_self = false;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let (pid, name) = line
            .trim()
            .split_once(char::is_whitespace)
            .ok_or(ClosedError::ProcessCheck)?;
        let pid: u32 = pid.parse().map_err(|_| ClosedError::ProcessCheck)?;
        if pid == own_pid {
            seen_self = true;
        }
        if name.trim().is_empty() {
            return Err(ClosedError::ProcessCheck);
        }
        if zotero_process(name) {
            return Err(ClosedError::Running);
        }
    }
    if seen_self {
        Ok(())
    } else {
        Err(ClosedError::ProcessCheck)
    }
}

#[cfg(unix)]
fn zotero_process(name: &str) -> bool {
    let name = Path::new(name.trim())
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    matches!(name.as_str(), "zotero" | "zotero-bin")
}

/// Bounded native-helper output stays private. Read concurrently so a full pipe
/// cannot turn process enumeration or locality checking into an unbounded wait.
#[cfg(unix)]
pub(super) fn output(command: &mut Command, control: &Control) -> Result<Vec<u8>, ClosedError> {
    const LIMIT: u64 = 4 * 1024 * 1024;
    control.check()?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| control.check().err().unwrap_or(ClosedError::ProcessCheck))?;
    let stdout = child.stdout.take().ok_or(ClosedError::ProcessCheck)?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.take(LIMIT + 1).read_to_end(&mut bytes).map(|_| bytes)
    });
    let outcome = loop {
        if let Err(error) = control.check() {
            break Err(error);
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status.success()),
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(_) => break Err(ClosedError::ProcessCheck),
        }
    };
    if outcome.is_err() {
        // Reap before returning a stop.
        let _ = child.kill();
        let _ = child.wait();
    }
    let bytes = reader.join().ok().and_then(Result::ok);
    let result = outcome.and_then(|success| {
        control.check()?;
        let bytes = bytes.as_ref().ok_or(ClosedError::ProcessCheck)?;
        if !success || bytes.len() as u64 > LIMIT {
            Err(ClosedError::ProcessCheck)
        } else {
            Ok(())
        }
    });
    result?;
    bytes.ok_or(ClosedError::ProcessCheck)
}

pub(super) fn directory(path: &Path, control: &Control) -> Result<PathBuf, ClosedError> {
    control.check()?;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        super::mounts::resolved(path, control)
    }
    #[cfg(windows)]
    {
        windows::local(path, control).map_err(|e| e.context(ClosedError::UnsafeSource))?;
        Ok(path.to_owned())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = path;
        Err(ClosedError::UnsafeSource)
    }
}

pub(super) fn resident(metadata: &Metadata) -> bool {
    #[cfg(target_os = "macos")]
    {
        use std::os::macos::fs::MetadataExt;
        metadata.st_flags() & 0x4000_0000 == 0
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x0044_1400 == 0
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = metadata;
        true
    }
}

pub(super) fn open_source(path: &Path) -> Result<File, ClosedError> {
    let file = open_regular(path)?.ok_or(ClosedError::Unreadable)?;
    passive(&file)?;
    Ok(file)
}

/// Open each component through a no-follow handle. Directory handles stay
/// pinned until the file is opened; Windows also denies ancestor replacement.
pub(super) fn local_file(path: &Path, parents: ParentLinks, control: &Control) -> Result<Option<File>, ClosedError> {
    use std::path::Component;
    control.check()?;
    if !path.is_absolute() {
        return Err(ClosedError::UnsafeSource);
    }
    // Resolve only the selected parent, classifying each symlink target before
    // probing it. Reopen the resolved path below through no-follow handles.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let resolved;
    let path = match parents {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        ParentLinks::Resolve => {
            let (Some(folder), Some(name)) = (path.parent(), path.file_name()) else {
                return Err(ClosedError::UnsafeSource);
            };
            resolved = directory(folder, control)?.join(name);
            resolved.as_path()
        }
        // Windows always refuses reparse points; its aliases are drive paths.
        _ => path,
    };
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    super::mounts::local(path, control)?;
    #[cfg(unix)]
    {
        use nix::fcntl::{OFlag, open, openat};
        use nix::sys::stat::Mode;
        let flags = OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC;
        let mut dir = File::from(
            open(Path::new("/"), flags | OFlag::O_DIRECTORY, Mode::empty()).map_err(|_| ClosedError::UnsafeSource)?,
        );
        let mut parts = path
            .components()
            .filter_map(|part| match part {
                Component::Normal(name) => Some(name),
                Component::ParentDir => Some(std::ffi::OsStr::new("..")),
                _ => None,
            })
            .peekable();
        while let Some(name) = parts.next() {
            control.check()?;
            let directory = parts.peek().is_some();
            let file = match openat(
                &dir,
                name,
                flags | if directory { OFlag::O_DIRECTORY } else { OFlag::empty() },
                Mode::empty(),
            ) {
                Ok(file) => File::from(file),
                Err(nix::errno::Errno::ENOENT) => return Ok(None),
                Err(_) => return Err(ClosedError::UnsafeSource),
            };
            if !directory {
                return regular(file).map(Some);
            }
            if !resident(&file.metadata().map_err(|_| ClosedError::Unreadable)?) {
                return Err(ClosedError::UnsafeSource);
            }
            dir = file;
        }
        Err(ClosedError::UnsafeSource)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };
        let (Some(folder), Some(name)) = (path.parent(), path.file_name()) else {
            return Err(ClosedError::UnsafeSource);
        };
        // Classify the drive before probing it, then pin each ancestor before
        // opening the next one. Reparse points and placeholders fail closed.
        let mut current = PathBuf::new();
        let mut directories = Vec::new();
        windows::checked_path(path)?;
        for component in folder.components() {
            current.push(component);
            if matches!(component, Component::Prefix(_)) {
                continue;
            }
            control.check()?;
            if matches!(component, Component::RootDir) {
                windows::local(&current, control)?;
            }
            let directory = match std::fs::OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&current)
            {
                Ok(directory) => directory,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(_) => return Err(ClosedError::Unreadable),
            };
            let metadata = directory.metadata().map_err(|_| ClosedError::Unreadable)?;
            if !metadata.is_dir() || !resident(&metadata) {
                return Err(ClosedError::UnsafeSource);
            }
            directories.push(directory);
        }
        open_regular(&current.join(name))
    }
    #[cfg(not(any(unix, windows)))]
    Err(ClosedError::UnsafeSource)
}

/// A regular, resident local file opened without following a final link, or
/// `None` when nothing is there.
pub(super) fn open_regular(path: &Path) -> Result<Option<File>, ClosedError> {
    #[cfg(unix)]
    let file = {
        use nix::fcntl::{OFlag, open};
        use nix::sys::stat::Mode;
        match open(
            path,
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
            Mode::empty(),
        ) {
            Ok(file) => File::from(file),
            Err(nix::errno::Errno::ENOENT) => return Ok(None),
            Err(_) => return Err(ClosedError::Unreadable),
        }
    };
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_SHARE_READ | WRITE | DELETE: never prevent a Zotero launch.
        match std::fs::OpenOptions::new()
            .read(true)
            .share_mode(7)
            .custom_flags(0x0020_0000)
            .open(path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(ClosedError::Unreadable),
        }
    };
    #[cfg(not(any(unix, windows)))]
    return Err(ClosedError::UnsafeSource);
    regular(file).map(Some)
}

fn regular(file: File) -> Result<File, ClosedError> {
    let m = file.metadata().map_err(|_| ClosedError::Unreadable)?;
    if !m.is_file() || m.file_type().is_symlink() || !resident(&m) {
        return Err(ClosedError::UnsafeSource);
    }
    Ok(file)
}

/// Identity and link count from an opened file, without resolving its path again.
pub(super) fn file_identity(file: &File, metadata: &Metadata) -> Option<([u64; 2], u64)> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let _ = file;
        Some(([metadata.dev(), metadata.ino()], metadata.nlink()))
    }
    #[cfg(windows)]
    {
        let _ = metadata;
        windows::file_identity(file)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (file, metadata);
        None
    }
}

/// Query existing SQLite byte-range locks; F_GETLK never obtains a lock.
pub(super) fn passive(file: &File) -> Result<(), ClosedError> {
    #[cfg(unix)]
    {
        use nix::fcntl::{FcntlArg, fcntl};
        let mut lock = nix::libc::flock {
            l_start: 0,
            l_len: 0,
            l_pid: 0,
            l_type: nix::libc::F_WRLCK as _,
            l_whence: nix::libc::SEEK_SET as _,
        };
        fcntl(file, FcntlArg::F_GETLK(&mut lock)).map_err(|_| ClosedError::ProcessCheck)?;
        if lock.l_type != nix::libc::F_UNLCK as nix::libc::c_short {
            return Err(ClosedError::Running);
        }
    }
    #[cfg(not(unix))]
    let _ = file;
    Ok(())
}

pub(super) fn same(a: &Metadata, b: &Metadata) -> bool {
    if !a.is_file() || !b.is_file() || !resident(a) || !resident(b) {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if a.dev() != b.dev() || a.ino() != b.ino() {
            return false;
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if a.creation_time() != b.creation_time() || a.file_attributes() != b.file_attributes() {
            return false;
        }
    }
    a.len() == b.len() && a.modified().ok().is_some() && a.modified().ok() == b.modified().ok()
}

pub(super) fn create_private(path: &Path, control: &Control) -> Result<(), ClosedError> {
    control.check()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        use std::os::unix::fs::MetadataExt;
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(path)
            .map_err(|_| ClosedError::PrivateStorage)?;
        let created = std::fs::symlink_metadata(path).map_err(|_| ClosedError::PrivateStorage)?;
        #[cfg(target_os = "macos")]
        let result = clear_inherited_acl(path, control).and_then(|()| secure(path, control));
        #[cfg(not(target_os = "macos"))]
        let result = secure(path, control);
        if result.is_err()
            && std::fs::symlink_metadata(path).is_ok_and(|current| {
                current.is_dir() && current.dev() == created.dev() && current.ino() == created.ino()
            })
        {
            // Nothing has been written yet. Never recurse or repair an existing
            // root; a cancelled fresh hardening must leave a retry possible.
            let _ = std::fs::remove_dir(path);
        }
        result?;
    }
    #[cfg(windows)]
    {
        // Per-user cache security is inherited; no reader-specific ACL policy.
        directory(path.parent().ok_or(ClosedError::PrivateStorage)?, control)
            .map_err(|e| e.context(ClosedError::PrivateStorage))?;
        std::fs::create_dir(path).map_err(|_| ClosedError::PrivateStorage)?;
        secure(path, control)?;
    }
    #[cfg(not(any(unix, windows)))]
    return Err(ClosedError::PrivateStorage);
    Ok(())
}

/// Acquire the reusable lock, retaining the same file and lease through validation.
pub(super) fn private_lock(path: &Path, control: &Control) -> Result<FileLease, ClosedError> {
    control.check()?;
    let failure = if cfg!(windows) {
        ClosedError::LockPermissions
    } else {
        ClosedError::PrivateStorage
    };
    #[cfg(not(windows))]
    let file = ibis_core::atomic_file::open_private(path).map_err(|_| failure)?;
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
        match ibis_core::atomic_file::create_private(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(path)
                .map_err(|_| failure)?,
            Err(_) => return Err(failure),
        }
    };
    if !file.metadata().is_ok_and(|m| m.is_file() && resident(&m)) {
        return Err(failure);
    }
    let metadata = std::fs::symlink_metadata(path).map_err(|_| failure)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || !resident(&metadata) {
        return Err(failure);
    }
    FileLease::acquire(file).map_err(|error| match error {
        std::fs::TryLockError::WouldBlock => ClosedError::StorageInUse,
        _ if cfg!(windows) => failure,
        _ => ClosedError::StorageInUse,
    })
}

#[cfg(target_os = "macos")]
fn clear_inherited_acl(path: &Path, control: &Control) -> Result<(), ClosedError> {
    output(Command::new("/bin/chmod").args(["-h", "-N"]).arg(path), control)
        .map(|_| ())
        .map_err(|e| e.context(ClosedError::PrivateStorage))
}

pub(super) fn secure(path: &Path, control: &Control) -> Result<(), ClosedError> {
    control.check()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = std::fs::symlink_metadata(path).map_err(|_| ClosedError::PrivateStorage)?;
        if !m.is_dir()
            || m.file_type().is_symlink()
            || m.uid() != nix::unistd::geteuid().as_raw()
            || m.mode() & 0o777 != 0o700
        {
            return Err(ClosedError::PrivateStorage);
        }
    }
    #[cfg(target_os = "macos")]
    {
        // Darwin extended ACLs can grant access despite mode0700. Refuse any
        // existing ACL; only freshly created empty directories are stripped.
        // -b escapes filename newlines, so an ACL-free listing has one line.
        let report = output(Command::new("/bin/ls").args(["-lde", "-b"]).arg(path), control)
            .map_err(|e| e.context(ClosedError::PrivateStorage))?;
        if report.split(|b| *b == b'\n').filter(|line| !line.is_empty()).count() != 1 {
            return Err(ClosedError::PrivateStorage);
        }
    }
    #[cfg(windows)]
    {
        directory(path, control).map_err(|e| e.context(ClosedError::PrivateStorage))?;
        if !std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && resident(&m)) {
            return Err(ClosedError::PrivateStorage);
        }
    }
    Ok(())
}

pub(super) fn exclude_backups(path: &Path, control: &Control) -> Result<(), ClosedError> {
    control.check()?;
    // Standard cachedir signature: recognized by Unix backup tools. macOS also
    // gets Time Machine's native exclusion; Windows uses the per-user app cache.
    use std::io::Write;
    const TAG: &[u8] = b"Signature: 8a477f597d28d172789f06886806bc55\n# Disposable Ibis Zotero copies.\n";
    let tag = path.join("CACHEDIR.TAG");
    match std::fs::symlink_metadata(&tag) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut file = ibis_core::atomic_file::create_private(&tag).map_err(|_| ClosedError::PrivateStorage)?;
            file.write_all(TAG)
                .and_then(|_| file.sync_all())
                .map_err(|_| ClosedError::PrivateStorage)?;
        }
        Ok(_) => {
            // Never overwrite or follow a pre-existing tag, including a link
            // inserted before inherited ACLs were removed from a new directory.
            let file = open_source(&tag).map_err(|_| ClosedError::PrivateStorage)?;
            let mut bytes = Vec::new();
            file.take(TAG.len() as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| ClosedError::PrivateStorage)?;
            if bytes != TAG {
                return Err(ClosedError::PrivateStorage);
            }
        }
        Err(_) => return Err(ClosedError::PrivateStorage),
    }
    #[cfg(target_os = "macos")]
    xattr::set(
        path,
        "com.apple.metadata:com_apple_backup_excludeItem",
        b"com.apple.backupd",
    )
    .map_err(|_| ClosedError::PrivateStorage)?;
    Ok(())
}

pub(super) fn space(path: &Path, needed: u64, control: &Control) -> Result<(), ClosedError> {
    control.check()?;
    #[cfg(unix)]
    {
        let fs = nix::sys::statvfs::statvfs(path).map_err(|_| ClosedError::NoSpace)?;
        // These counts are u64 on Linux but narrower on macOS.
        #[allow(clippy::useless_conversion)]
        let available = u64::from(fs.blocks_available()).saturating_mul(u64::from(fs.fragment_size()));
        if available < needed {
            return Err(ClosedError::NoSpace);
        }
    }
    #[cfg(windows)]
    {
        windows::space(path, needed, control).map_err(|e| e.context(ClosedError::NoSpace))?;
    }
    Ok(())
}

/// The drive-letter offset in a plain or verbatim absolute Windows disk path.
/// Raw UTF-16 keeps this locality gate testable on every host, without letting
/// host-specific Path parsing accept UNC, device or ambiguous components.
#[cfg(any(windows, test))]
fn windows_drive_path(path: &[u16]) -> Option<usize> {
    let disk = if path.starts_with(&[92, 92, 63, 92]) { 4 } else { 0 };
    let plain = path.get(disk..)?;
    if !matches!(plain.first(), Some(65..=90 | 97..=122))
        || plain.get(1) != Some(&58)
        || !matches!(plain.get(2), Some(47 | 92))
        || (disk != 0 && plain.contains(&47))
    {
        return None;
    }
    for part in plain[3..].split(|c| matches!(*c, 47 | 92)).filter(|p| !p.is_empty()) {
        if part == [46]
            || part == [46, 46]
            || part.iter().any(|c| *c == 58 || *c < 32 || (127..=159).contains(c))
            || matches!(part.last(), Some(46 | 32))
        {
            return None;
        }
    }
    Some(disk)
}

#[cfg(test)]
mod path_tests {
    use super::windows_drive_path;

    #[test]
    fn document_windows_disk_paths_accept_plain_verbatim_and_short_names() {
        for (path, disk) in [
            (r"C:\Users\Example\draft.md", 0),
            (r"C:/Users/Example/draft.md", 0),
            (r"\\?\C:\Users\Example\draft.md", 4),
            (r"C:\Users\EXAMPL~1\draft.md", 0),
            (r"\\?\C:\Users\EXAMPL~1\draft.md", 4),
            (r"\\?\C:\", 4),
        ] {
            assert_eq!(windows_drive_path(&path.encode_utf16().collect::<Vec<_>>()), Some(disk));
        }
    }

    #[test]
    fn document_windows_unc_device_and_ambiguous_paths_are_refused() {
        for path in [
            r"\\server\share\draft.md",
            r"\\?\UNC\server\share\draft.md",
            r"\\.\C:\draft.md",
            r"\\?\Volume{invented}\draft.md",
            r"\??\C:\draft.md",
            r"C:relative.md",
            r"\draft.md",
            r"\\?\C:/draft.md",
            r"C:\a\..\draft.md",
            r"\\?\C:\a\..\draft.md",
            r"C:\a\.\draft.md",
            r"\\?\C:\a\.\draft.md",
            r"C:\draft.md:stream",
            r"\\?\C:\draft.md:stream",
            r"C:\a.\draft.md",
            r"C:\a \draft.md",
            "C:\\a\u{1f}\\draft.md",
            "C:\\draft\0.md",
        ] {
            assert_eq!(windows_drive_path(&path.encode_utf16().collect::<Vec<_>>()), None);
        }
    }
}

// SAFETY: Win32 owns the handles and buffers in this boundary. Unsafe calls are
// confined here, with checked lengths, explicit ownership and fixed errors.
#[cfg(windows)]
#[allow(unsafe_code)]
mod windows;

#[cfg(test)]
#[cfg(unix)]
pub(super) mod tests {
    use super::*;
    use crate::zotero_local::closed::tests::control;
    use std::sync::Arc;

    fn synthetic_helper(root: &Path) -> Command {
        helper_command(
            &std::env::current_exe().unwrap_or_else(|error| fixture_error("executable", error)),
            root,
        )
    }

    fn helper_command(executable: &Path, root: &Path) -> Command {
        let mut command = Command::new(executable);
        command
            .args([
                "--exact",
                "zotero_local::closed::platform::tests::synthetic_native_helper",
                "--ignored",
                "--nocapture",
            ])
            .env("IBIS_TEST_CLOSED_NATIVE_ROOT", root);
        command
    }

    fn fixture_error(stage: &str, error: std::io::Error) -> ! {
        panic!(
            "synthetic process {stage}: kind={:?}, errno={:?}",
            error.kind(),
            error.raw_os_error()
        );
    }

    pub(in crate::zotero_local::closed) struct NamedProcess {
        child: std::process::Child,
        executable: PathBuf,
        _root: tempfile::TempDir,
    }

    impl NamedProcess {
        pub(in crate::zotero_local::closed) fn start(name: &str) -> Self {
            let source = std::env::current_exe().unwrap_or_else(|error| fixture_error("executable", error));
            // Reuse the running inode: no executable write descriptor can be
            // inherited by a concurrent fork. Keep the alias on the same FS.
            let root = tempfile::tempdir_in(source.parent().unwrap_or_else(|| panic!("synthetic executable parent")))
                .unwrap_or_else(|error| fixture_error("directory", error));
            let executable = root.path().join(name);
            std::fs::hard_link(&source, &executable).unwrap_or_else(|error| fixture_error("link", error));
            let child = helper_command(&executable, root.path())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap_or_else(|error| fixture_error("spawn", error));
            Self {
                child,
                executable,
                _root: root,
            }
        }

        pub(in crate::zotero_local::closed) fn snapshot(&self) -> Vec<u8> {
            let own = std::process::id();
            let child = self.child.id().to_string();
            // Never enumerate unrelated processes for the fixture-name oracle.
            let bytes = output(
                Command::new("/bin/ps").args(["-ww", "-p", &format!("{own},{child}"), "-o", "pid=,comm="]),
                &control(),
            )
            .unwrap_or_else(|_| panic!("synthetic process snapshot"));
            let text = std::str::from_utf8(&bytes).unwrap_or_else(|_| panic!("synthetic process encoding"));
            let observed = text
                .lines()
                .filter_map(|line| line.trim().split_once(char::is_whitespace))
                .find_map(|(pid, name)| (pid == child).then_some(name))
                .and_then(|name| Path::new(name.trim()).file_name());
            assert!(
                observed == self.executable.file_name(),
                "synthetic child must have the exact native process name"
            );
            bytes
        }
    }

    impl Drop for NamedProcess {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    #[test]
    #[ignore = "subprocess helper; parent creates only invented temporary files"]
    fn synthetic_native_helper() {
        let Some(root) = std::env::var_os("IBIS_TEST_CLOSED_NATIVE_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        let held = File::create(root.join("held")).unwrap_or_else(|_| panic!("synthetic held file"));
        held.try_lock().unwrap_or_else(|_| panic!("synthetic child lock"));
        std::fs::write(root.join("ready"), b"ready").unwrap_or_else(|_| panic!("synthetic child ready"));
        std::thread::sleep(Duration::from_secs(90));
        drop(held);
    }

    #[test]
    fn running_native_child_is_cancelled_and_reaped() {
        let root = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic child root"));
        let ready = root.path().join("ready");
        let cancelled = Control::new(Arc::new(move || ready.exists()));
        assert_eq!(
            output(&mut synthetic_helper(root.path()), &cancelled).err(),
            Some(ClosedError::Cancelled)
        );
        assert!(root.path().join("ready").exists(), "cancellation follows child entry");
        let held = File::open(root.path().join("held")).unwrap_or_else(|_| panic!("synthetic child lock file"));
        assert!(held.try_lock().is_ok(), "return must follow child termination");
    }

    #[test]
    fn running_native_child_uses_the_callers_deadline() {
        let root = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic child root"));
        let mut expired = control();
        expired.deadline = std::time::Instant::now() + Duration::from_millis(100);
        assert_eq!(
            output(&mut synthetic_helper(root.path()), &expired).err(),
            Some(ClosedError::TimedOut)
        );
    }

    #[cfg(unix)]
    #[test]
    fn incomplete_or_malformed_process_lists_are_not_closed() {
        use super::{ClosedError, process_list};
        assert_eq!(process_list(b" 42 /synthetic/ibis\n 43 /bin/ps\n", 42), Ok(()));
        for bytes in [b"".as_slice(), b"43 ps\n", b"invalid\n", b"42 \n"] {
            assert_eq!(process_list(bytes, 42), Err(ClosedError::ProcessCheck));
        }
        assert_eq!(
            process_list(b"42 ibis\n43 /synthetic/zotero\n", 42),
            Err(ClosedError::Running)
        );
    }

    #[cfg(unix)]
    #[test]
    fn zotero_mcp_process_does_not_mean_zotero_is_running() {
        assert_eq!(process_list(b"42 ibis\n43 /synthetic/zotero-mcp\n", 42), Ok(()));
    }
}

#[cfg(test)]
#[cfg(target_os = "macos")]
mod macos_acl_tests {
    use super::*;

    #[test]
    fn owner_only_creation_drops_inherited_extended_acl() {
        let parent = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic ACL parent"));
        let status = Command::new("/bin/chmod")
            .args(["+a", "everyone allow list,search,directory_inherit,file_inherit"])
            .arg(parent.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap_or_else(|_| panic!("synthetic inherited ACL"));
        assert!(status.success());
        let child = parent.path().join("private");
        create_private(&child, &crate::zotero_local::closed::tests::control()).unwrap_or_else(|e| panic!("{e}"));
        let report = output(
            Command::new("/bin/ls").args(["-lde", "-b"]).arg(&child),
            &crate::zotero_local::closed::tests::control(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(report.split(|b| *b == b'\n').filter(|line| !line.is_empty()).count(), 1);
        // Existing roots with an unexpected ACL are refused, never repaired.
        let status = Command::new("/bin/chmod")
            .args(["+a", "everyone allow list,search"])
            .arg(&child)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap_or_else(|_| panic!("synthetic existing ACL"));
        assert!(status.success());
        assert_eq!(
            secure(&child, &crate::zotero_local::closed::tests::control()),
            Err(ClosedError::PrivateStorage)
        );
    }
}

#[cfg(test)]
#[cfg(unix)]
mod cache_tag_tests {
    use super::*;
    #[test]
    fn backup_tag_never_overwrites_an_existing_link_target() {
        let root = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic tag root"));
        let outside = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic sentinel root"));
        let target = outside.path().join("sentinel");
        std::fs::write(&target, b"synthetic sentinel").unwrap_or_else(|_| panic!("synthetic sentinel"));
        std::os::unix::fs::symlink(&target, root.path().join("CACHEDIR.TAG"))
            .unwrap_or_else(|_| panic!("synthetic tag link"));
        assert_eq!(
            exclude_backups(root.path(), &crate::zotero_local::closed::tests::control()),
            Err(ClosedError::PrivateStorage)
        );
        assert_eq!(std::fs::read(target).unwrap_or_default(), b"synthetic sentinel");
    }
}
