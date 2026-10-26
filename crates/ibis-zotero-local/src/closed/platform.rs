//! Native boundaries used only by the closed reader. No source lock is acquired.
use super::ClosedError;
use std::fs::{File, Metadata};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(windows)]
const SCRIPT: &str = include_str!("windows.ps1");

/// Conservative: refuse every running Zotero, even a different profile. An
/// incomplete/failed process enumeration is never evidence that Zotero is closed.
pub(super) fn closed() -> Result<(), ClosedError> {
    #[cfg(unix)]
    {
        if std::env::var_os("FLATPAK_ID").is_some() || Path::new("/.flatpak-info").exists() {
            return Err(ClosedError::ProcessCheck);
        }
        let bytes = output(Command::new("/bin/ps").args(["-A", "-ww", "-o", "pid=,comm="]))?;
        process_list(&bytes, std::process::id())
    }
    #[cfg(windows)]
    {
        match windows("closed", Path::new(""), 0)?.as_slice() {
            b"closed" => Ok(()),
            b"running" => Err(ClosedError::Running),
            _ => Err(ClosedError::ProcessCheck),
        }
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
    name == "zotero" || name.starts_with("zotero-") || name.starts_with("zotero.")
}

/// Bounded native-helper output stays private. Read concurrently so a full pipe
/// cannot turn process enumeration or locality checking into an unbounded wait.
fn output(command: &mut Command) -> Result<Vec<u8>, ClosedError> {
    const LIMIT: u64 = 4 * 1024 * 1024;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| ClosedError::ProcessCheck)?;
    let stdout = child.stdout.take().ok_or(ClosedError::ProcessCheck)?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.take(LIMIT + 1).read_to_end(&mut bytes).map(|_| bytes)
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let bytes = reader
        .join()
        .map_err(|_| ClosedError::ProcessCheck)?
        .map_err(|_| ClosedError::ProcessCheck)?;
    if !success || bytes.len() as u64 > LIMIT {
        return Err(ClosedError::ProcessCheck);
    }
    Ok(bytes)
}

pub(super) fn directory(path: &Path) -> Result<PathBuf, ClosedError> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        super::mounts::resolved(path)
    }
    #[cfg(windows)]
    {
        windows("local", path, 0).map_err(|_| ClosedError::UnsafeSource)?;
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
    #[cfg(unix)]
    let file = {
        use rustix::fs::{Mode, OFlags, open};
        File::from(
            open(
                path,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|_| ClosedError::Unreadable)?,
        )
    };
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_SHARE_READ | WRITE | DELETE: never prevent a Zotero launch.
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(7)
            .custom_flags(0x0020_0000)
            .open(path)
            .map_err(|_| ClosedError::Unreadable)?
    };
    #[cfg(not(any(unix, windows)))]
    return Err(ClosedError::UnsafeSource);
    let m = file.metadata().map_err(|_| ClosedError::Unreadable)?;
    if !m.is_file() || m.file_type().is_symlink() || !resident(&m) {
        return Err(ClosedError::UnsafeSource);
    }
    passive(&file)?;
    Ok(file)
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

pub(super) fn create_private(path: &Path) -> Result<(), ClosedError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(path)
            .map_err(|_| ClosedError::PrivateStorage)?;
        secure(path)?;
    }
    #[cfg(windows)]
    {
        windows("create", path, 0).map_err(|_| ClosedError::PrivateStorage)?;
    }
    #[cfg(not(any(unix, windows)))]
    return Err(ClosedError::PrivateStorage);
    Ok(())
}

pub(super) fn secure(path: &Path) -> Result<(), ClosedError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = std::fs::symlink_metadata(path).map_err(|_| ClosedError::PrivateStorage)?;
        if !m.is_dir()
            || m.file_type().is_symlink()
            || m.uid() != rustix::process::geteuid().as_raw()
            || m.mode() & 0o777 != 0o700
        {
            return Err(ClosedError::PrivateStorage);
        }
    }
    #[cfg(windows)]
    {
        windows("private", path, 0).map_err(|_| ClosedError::PrivateStorage)?;
    }
    Ok(())
}

pub(super) fn exclude_backups(path: &Path) -> Result<(), ClosedError> {
    // Standard cachedir signature: recognized by Unix backup tools. macOS also
    // gets Time Machine's native exclusion; Windows root is restricted to TEMP.
    std::fs::write(
        path.join("CACHEDIR.TAG"),
        b"Signature: 8a477f597d28d172789f06886806bc55\n# Disposable Ibis Zotero copies.\n",
    )
    .map_err(|_| ClosedError::PrivateStorage)?;
    #[cfg(target_os = "macos")]
    xattr::set(
        path,
        "com.apple.metadata:com_apple_backup_excludeItem",
        b"com.apple.backupd",
    )
    .map_err(|_| ClosedError::PrivateStorage)?;
    #[cfg(windows)]
    windows("backup", path, 0).map_err(|_| ClosedError::PrivateStorage)?;
    Ok(())
}

pub(super) fn space(path: &Path, needed: u64) -> Result<(), ClosedError> {
    #[cfg(unix)]
    {
        let fs = rustix::fs::statvfs(path).map_err(|_| ClosedError::NoSpace)?;
        if fs.f_bavail.saturating_mul(fs.f_frsize) < needed {
            return Err(ClosedError::NoSpace);
        }
    }
    #[cfg(windows)]
    {
        windows("space", path, needed).map_err(|_| ClosedError::NoSpace)?;
    }
    Ok(())
}

#[cfg(windows)]
fn drive_path(path: &Path) -> bool {
    use std::path::{Component, Prefix};
    let mut parts = path.components();
    matches!(parts.next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_)))
        && matches!(parts.next(), Some(Component::RootDir))
        && parts.all(|p| matches!(p, Component::Normal(_)))
}

#[cfg(windows)]
fn windows(action: &str, path: &Path, needed: u64) -> Result<Vec<u8>, ClosedError> {
    use std::os::windows::process::CommandExt;
    let system = std::env::var_os("SystemRoot").ok_or(ClosedError::ProcessCheck)?;
    if !drive_path(Path::new(&system)) {
        return Err(ClosedError::ProcessCheck);
    }
    let mut cmd = Command::new(PathBuf::from(system).join(r"System32\WindowsPowerShell\v1.0\powershell.exe"));
    cmd.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .env("IBIS_CLOSED_ACTION", action)
        .env("IBIS_CLOSED_PATH", path)
        .env("IBIS_CLOSED_SPACE", needed.to_string())
        .creation_flags(0x0800_0000);
    output(&mut cmd)
}

#[cfg(test)]
mod tests {
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
    fn recognizes_application_names_without_matching_test_executable() {
        for name in [
            "zotero",
            "/Applications/Zotero.app/Contents/MacOS/zotero",
            "zotero-bin",
            "Zotero.Beta",
        ] {
            assert!(super::zotero_process(name));
        }
        for name in ["ibis_zotero_local-test", "zotero_reader_test", "ps", "/tmp/zotero/test"] {
            assert!(!super::zotero_process(name));
        }
    }
}
