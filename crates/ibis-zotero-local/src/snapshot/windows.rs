//! Windows checks run in the system's Windows PowerShell/.NET Framework.
//! Paths are environment values, never script text; no output leaves the child.

use std::path::{Component, Path, Prefix};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::SnapshotError;

const SCRIPT: &str = include_str!("windows.ps1");
const HELPER_BUDGET: Duration = Duration::from_secs(5);

// Refuse UNC, device namespaces and drive-relative paths without starting a
// helper or probing the path. The helper checks mapped drives before files.
fn drive_path(path: &Path) -> bool {
    let mut parts = path.components();
    matches!(parts.next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_)))
        && matches!(parts.next(), Some(Component::RootDir))
        && parts.all(|part| matches!(part, Component::Normal(_)))
}

pub(super) fn drive(path: &Path) -> bool {
    drive_path(path) && run("drive", path)
}

pub(super) fn local(path: &Path) -> bool {
    super::locks::directory_guards(path).is_ok()
}

pub(super) fn create_private(path: &Path) -> Result<(), SnapshotError> {
    let Some(parent) = path.parent() else {
        return Err(SnapshotError::CopyFailed);
    };
    let Ok(_directories) = super::locks::directory_guards(parent) else {
        return Err(SnapshotError::CopyFailed);
    };
    if drive_path(path) && run("create", path) {
        Ok(())
    } else {
        Err(SnapshotError::CopyFailed)
    }
}

/// Recheck after the caller has opened its no-delete directory guard.
pub(super) fn private(path: &Path) -> bool {
    let Ok(_directories) = super::locks::directory_guards(path) else {
        return false;
    };
    drive_path(path) && run("private", path)
}

fn run(action: &str, path: &Path) -> bool {
    use std::os::windows::process::CommandExt;

    let Some(system) = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .filter(|p| drive_path(p))
    else {
        return false;
    };
    let executable = system.join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let deadline = Instant::now() + HELPER_BUDGET;
    let Ok(mut child) = Command::new(executable)
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .env("IBIS_SNAPSHOT_ACTION", action)
        .env("IBIS_SNAPSHOT_PATH", path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
    else {
        return false;
    };
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                // We own only this helper; reap it on timeouts and wait errors.
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_and_device_paths_are_rejected_lexically() {
        for path in [
            r"\\server\share\Zotero",
            r"\\?\UNC\server\share",
            r"\\.\pipe\database",
            r"C:Zotero",
            r"\Zotero",
        ] {
            assert!(!drive_path(Path::new(path)));
            assert!(!local(Path::new(path)));
            assert!(matches!(
                crate::snapshot::configuration_guards(&Path::new(path).join("prefs.js")),
                Err(SnapshotError::NotLocal)
            ));
        }
    }

    #[test]
    fn a_private_directory_and_its_files_exclude_inherited_readers() {
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let dir = root.path().join("private '$() library");
        create_private(&dir).unwrap_or_else(|e| panic!("{e}"));
        assert!(private(&dir));
        let file = dir.join("synthetic.sqlite");
        std::fs::write(&file, b"synthetic").unwrap_or_else(|e| panic!("{e}"));
        assert!(run("verify", &file));
        assert!(create_private(&dir).is_err());
        assert_eq!(std::fs::read(&file).unwrap_or_else(|e| panic!("{e}")), b"synthetic");
    }

    #[test]
    fn junctions_are_rejected_before_descendants_are_probed() {
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let target = root.path().join("target");
        let junction = root.path().join("junction");
        std::fs::create_dir(&target).unwrap_or_else(|e| panic!("{e}"));
        let status = Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/j"])
            .arg(&junction)
            .arg(&target)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(status.success());
        assert!(!local(&junction));
        assert!(!local(&junction.join("unprobed")));
        assert!(create_private(&junction.join("uncreated")).is_err());
        assert!(!target.join("uncreated").exists());
        std::fs::remove_dir(junction).unwrap_or_else(|e| panic!("{e}"));
    }
}
