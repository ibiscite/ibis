//! Native boundaries used only by the closed reader. No source lock is acquired.
use super::{ClosedError, Control};
use std::fs::{File, Metadata};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

#[cfg(windows)]
const SCRIPT: &str = include_str!("windows.ps1");

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
        match windows("closed", Path::new(""), 0, control)?.as_slice() {
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
    matches!(name.as_str(), "zotero" | "zotero-bin")
}

/// Bounded native-helper output stays private. Read concurrently so a full pipe
/// cannot turn process enumeration or locality checking into an unbounded wait.
pub(super) fn output(command: &mut Command, control: &Control) -> Result<Vec<u8>, ClosedError> {
    const LIMIT: u64 = 4 * 1024 * 1024;
    control.check()?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| {
            let error = control.check().err().unwrap_or(ClosedError::ProcessCheck);
            #[cfg(test)]
            report_failure(error, "spawn");
            error
        })?;
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
        // Reap before releasing the single-flight permit or returning a stop.
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
    #[cfg(test)]
    if let Err(error) = result {
        let phase = match bytes.as_deref().unwrap_or_default() {
            [1, 2, ..] => "action",
            [1, ..] => "script-entry",
            _ => "spawn",
        };
        report_failure(error, phase);
    }
    result?;
    bytes.ok_or(ClosedError::ProcessCheck)
}

#[cfg(test)]
fn report_failure(error: ClosedError, phase: &str) {
    let reason = match error {
        ClosedError::Cancelled => "cancelled",
        ClosedError::TimedOut => "deadline",
        _ => "native",
    };
    eprintln!("closed native helper stopped: phase={phase} reason={reason}");
}

#[cfg(any(windows, test))]
fn gated_output(command: &mut Command, control: &Control, gate: &std::sync::Mutex<()>) -> Result<Vec<u8>, ClosedError> {
    let _permit = control.lock(gate, ClosedError::ProcessCheck).inspect_err(|_error| {
        #[cfg(test)]
        report_failure(*_error, "queue");
    })?;
    output(command, control)
}

pub(super) fn directory(path: &Path, control: &Control) -> Result<PathBuf, ClosedError> {
    control.check()?;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        super::mounts::resolved(path, control)
    }
    #[cfg(windows)]
    {
        windows("local", path, 0, control).map_err(|e| e.context(ClosedError::UnsafeSource))?;
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
        windows("create", path, 0, control).map_err(|e| e.context(ClosedError::PrivateStorage))?;
    }
    #[cfg(not(any(unix, windows)))]
    return Err(ClosedError::PrivateStorage);
    Ok(())
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
            || m.uid() != rustix::process::geteuid().as_raw()
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
        windows("private", path, 0, control).map_err(|e| e.context(ClosedError::PrivateStorage))?;
    }
    Ok(())
}

pub(super) fn exclude_backups(path: &Path, control: &Control) -> Result<(), ClosedError> {
    control.check()?;
    // Standard cachedir signature: recognized by Unix backup tools. macOS also
    // gets Time Machine's native exclusion; Windows root is restricted to TEMP.
    use std::io::Write;
    const TAG: &[u8] = b"Signature: 8a477f597d28d172789f06886806bc55\n# Disposable Ibis Zotero copies.\n";
    let tag = path.join("CACHEDIR.TAG");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(&tag) {
        Ok(mut file) => {
            file.write_all(TAG)
                .and_then(|_| file.sync_all())
                .map_err(|_| ClosedError::PrivateStorage)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
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
    #[cfg(windows)]
    windows("backup", path, 0, control).map_err(|e| e.context(ClosedError::PrivateStorage))?;
    Ok(())
}

pub(super) fn space(path: &Path, needed: u64, control: &Control) -> Result<(), ClosedError> {
    control.check()?;
    #[cfg(unix)]
    {
        let fs = rustix::fs::statvfs(path).map_err(|_| ClosedError::NoSpace)?;
        if fs.f_bavail.saturating_mul(fs.f_frsize) < needed {
            return Err(ClosedError::NoSpace);
        }
    }
    #[cfg(windows)]
    {
        windows("space", path, needed, control).map_err(|e| e.context(ClosedError::NoSpace))?;
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
fn windows(action: &str, path: &Path, needed: u64, control: &Control) -> Result<Vec<u8>, ClosedError> {
    use std::os::windows::process::CommandExt;
    static GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
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
    #[cfg(test)]
    let started = std::time::Instant::now();
    let result = gated_output(&mut cmd, control, &GATE).and_then(|bytes| {
        bytes
            .strip_prefix(&[1, 2])
            .map(<[u8]>::to_vec)
            .ok_or(ClosedError::ProcessCheck)
    });
    #[cfg(test)]
    if result.is_err() {
        eprintln!(
            "closed Windows helper action {action} failed after {}ms",
            started.elapsed().as_millis()
        );
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::closed::tests::control;
    use std::sync::{Arc, Mutex};

    fn synthetic_helper(root: &Path) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap_or_else(|_| panic!("synthetic executable")));
        command
            .args([
                "--exact",
                "closed::platform::tests::synthetic_native_helper",
                "--ignored",
                "--nocapture",
            ])
            .env("IBIS_TEST_CLOSED_NATIVE_ROOT", root);
        command
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
    fn expired_native_call_does_not_spawn_and_releases_its_permit() {
        let root = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic child root"));
        let gate = Mutex::new(());
        let mut expired = control();
        expired.deadline = std::time::Instant::now();
        assert_eq!(
            gated_output(&mut synthetic_helper(root.path()), &expired, &gate).err(),
            Some(ClosedError::TimedOut)
        );
        assert!(!root.path().join("ready").exists());
        assert!(gate.try_lock().is_ok());
    }

    #[test]
    fn cancelled_native_permit_wait_never_spawns_a_second_child() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let root = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic child root"));
        let gate = Mutex::new(());
        let held = gate.lock().unwrap_or_else(|_| panic!("native permit"));
        let polls = Arc::new(AtomicUsize::new(0));
        let observed = polls.clone();
        let cancelled = Control::new(Arc::new(move || observed.fetch_add(1, Ordering::SeqCst) >= 2));
        assert_eq!(
            gated_output(&mut synthetic_helper(root.path()), &cancelled, &gate).err(),
            Some(ClosedError::Cancelled)
        );
        assert!(polls.load(Ordering::SeqCst) >= 3);
        assert!(!root.path().join("ready").exists());
        assert!(gate.try_lock().is_err());
        drop(held);
        assert!(gate.try_lock().is_ok());
    }

    #[test]
    fn queued_native_wait_spends_the_callers_deadline_without_spawning() {
        let root = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic child root"));
        let gate = Mutex::new(());
        let held = gate.lock().unwrap_or_else(|_| panic!("native permit"));
        let mut expired = control();
        expired.deadline = std::time::Instant::now() + Duration::from_millis(20);
        assert_eq!(
            gated_output(&mut synthetic_helper(root.path()), &expired, &gate).err(),
            Some(ClosedError::TimedOut)
        );
        assert!(!root.path().join("ready").exists());
        assert!(gate.try_lock().is_err());
        drop(held);
        assert!(gate.try_lock().is_ok());
    }

    #[test]
    fn running_native_child_is_cancelled_reaped_and_releases_its_permit() {
        let root = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic child root"));
        let ready = root.path().join("ready");
        let cancelled = Control::new(Arc::new(move || ready.exists()));
        let gate = Mutex::new(());
        assert_eq!(
            gated_output(&mut synthetic_helper(root.path()), &cancelled, &gate).err(),
            Some(ClosedError::Cancelled)
        );
        assert!(root.path().join("ready").exists(), "cancellation follows child entry");
        let held = File::open(root.path().join("held")).unwrap_or_else(|_| panic!("synthetic child lock file"));
        assert!(held.try_lock().is_ok(), "return must follow child termination");
        assert!(gate.try_lock().is_ok());
    }

    #[test]
    fn running_native_child_uses_the_callers_deadline_and_releases_its_permit() {
        let root = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic child root"));
        let gate = Mutex::new(());
        let mut expired = control();
        expired.deadline = std::time::Instant::now() + Duration::from_millis(100);
        assert_eq!(
            gated_output(&mut synthetic_helper(root.path()), &expired, &gate).err(),
            Some(ClosedError::TimedOut)
        );
        assert!(gate.try_lock().is_ok());
    }

    #[test]
    fn native_permit_is_released_after_spawn_failure_and_success() {
        let root = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic child root"));
        let gate = Mutex::new(());
        assert_eq!(
            gated_output(
                &mut Command::new(root.path().join("missing-executable")),
                &control(),
                &gate
            )
            .err(),
            Some(ClosedError::ProcessCheck)
        );
        assert!(gate.try_lock().is_ok());
        let mut success = Command::new(std::env::current_exe().unwrap_or_else(|_| panic!("synthetic executable")));
        success.arg("--list");
        assert!(gated_output(&mut success, &control(), &gate).is_ok());
        assert!(gate.try_lock().is_ok());
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
    fn recognizes_application_names_without_matching_test_executable() {
        for name in [
            "zotero",
            "/Applications/Zotero.app/Contents/MacOS/zotero",
            "zotero-bin",
            "ZoTeRo",
        ] {
            assert!(super::zotero_process(name));
        }
        for name in [
            "ibis_zotero_local-test",
            "zotero_reader_test",
            "zotero-mcp",
            "Zotero.Beta",
            "zotero-sync-helper",
            "ps",
            "/tmp/zotero/test",
        ] {
            assert!(!super::zotero_process(name));
        }
    }

    #[cfg(unix)]
    #[test]
    fn zotero_mcp_process_does_not_mean_zotero_is_running() {
        assert_eq!(process_list(b"42 ibis\n43 /synthetic/zotero-mcp\n", 42), Ok(()));
    }

    #[cfg(unix)]
    #[test]
    fn a_synthetic_zotero_mcp_executable_is_not_the_zotero_process() {
        let root = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic process directory"));
        let executable = root.path().join("zotero-mcp");
        std::fs::copy("/bin/sleep", &executable).unwrap_or_else(|_| panic!("synthetic executable"));
        let mut child = Command::new(executable)
            .arg("10")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|_| panic!("synthetic process"));
        let own = std::process::id();
        // Inspect only our test and its invented helper, never other processes.
        let bytes = output(
            Command::new("/bin/ps").args(["-ww", "-p", &format!("{own},{}", child.id()), "-o", "pid=,comm="]),
            &control(),
        );
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(bytes.and_then(|bytes| process_list(&bytes, own)), Ok(()));
    }
}

#[cfg(all(test, target_os = "macos"))]
mod macos_acl_tests {
    use super::*;
    #[test]
    fn acl_removal_does_not_follow_a_replaced_final_symlink() {
        let root = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic ACL root"));
        let target = root.path().join("outside");
        std::fs::create_dir(&target).unwrap_or_else(|_| panic!("synthetic ACL target"));
        let status = Command::new("/bin/chmod")
            .args(["+a", "everyone allow list,search"])
            .arg(&target)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap_or_else(|_| panic!("synthetic target ACL"));
        assert!(status.success());
        let link = root.path().join("replaced");
        std::os::unix::fs::symlink(&target, &link).unwrap_or_else(|_| panic!("synthetic ACL link"));
        let _ = clear_inherited_acl(&link, &crate::closed::tests::control());
        let report = output(
            Command::new("/bin/ls").args(["-lde", "-b"]).arg(&target),
            &crate::closed::tests::control(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(report.split(|b| *b == b'\n').filter(|l| !l.is_empty()).count(), 2);
    }

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
        create_private(&child, &crate::closed::tests::control()).unwrap_or_else(|e| panic!("{e}"));
        let report = output(
            Command::new("/bin/ls").args(["-lde", "-b"]).arg(&child),
            &crate::closed::tests::control(),
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
            secure(&child, &crate::closed::tests::control()),
            Err(ClosedError::PrivateStorage)
        );
    }
}

#[cfg(all(test, unix))]
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
            exclude_backups(root.path(), &crate::closed::tests::control()),
            Err(ClosedError::PrivateStorage)
        );
        assert_eq!(std::fs::read(target).unwrap_or_default(), b"synthetic sentinel");
    }
}
