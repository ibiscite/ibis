//! Explicit desktop launch utility; tested with constructed commands/fake binaries.
use ibis_core::contract::{EngineError, ErrorCode, ErrorReason};
use ibis_core::toggle::{RECEIPT, RECEIPT_ARG};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
/// How long a starting app may take to confirm the toggle.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(10);

pub fn run(app: Option<&Path>) -> Result<(), EngineError> {
    let sibling = std::env::current_exe()
        .ok()
        .map(|exe| exe.with_file_name(format!("ibis-desktop{}", std::env::consts::EXE_SUFFIX)))
        .filter(|path| path.is_file());
    // Per-user (NSIS) first, then machine-wide (MSI or NSIS for all users).
    let install_roots: Vec<PathBuf> = ["LOCALAPPDATA", "ProgramFiles"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .collect();
    let receipt = Receipt::create()?;
    let mut command = desktop_command(
        std::env::consts::OS,
        sibling.as_deref(),
        app,
        &install_roots,
        Path::is_file,
    );
    command
        .args([RECEIPT_ARG.as_ref(), receipt.path.as_os_str()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            EngineError::because(ErrorCode::NotFound, ErrorReason::AppMissing)
        } else {
            EngineError::because(ErrorCode::BadInput, ErrorReason::NotIbis)
        }
    })?;
    // macOS starts the app through `open`, which returns once Launch Services has it.
    let launcher = cfg!(target_os = "macos") && command.get_program() == "open";
    wait_for_answer(&mut child, launcher, &receipt.path, ANSWER_TIMEOUT)
}

/// Succeed only on Ibis's receipt. A program that exits without one is not
/// Ibis; one that neither answers nor exits in time did not answer.
fn wait_for_answer(
    child: &mut std::process::Child,
    launcher: bool,
    receipt: &Path,
    timeout: Duration,
) -> Result<(), EngineError> {
    let answered = || std::fs::read(receipt).is_ok_and(|bytes| bytes == RECEIPT);
    let deadline = Instant::now() + timeout;
    let mut exited = None;
    loop {
        if answered() {
            return Ok(());
        }
        if exited.is_none() {
            exited = child.try_wait().ok().flatten();
        }
        match exited {
            Some(status) if launcher && !status.success() => {
                return Err(EngineError::because(ErrorCode::NotFound, ErrorReason::AppMissing));
            }
            // Ibis signs before it exits, so a direct child that exited without
            // signing is some other program.
            Some(_) if !launcher => {
                return if answered() {
                    Ok(())
                } else {
                    Err(EngineError::because(ErrorCode::BadInput, ErrorReason::NotIbis))
                };
            }
            _ => {}
        }
        if Instant::now() >= deadline {
            return Err(EngineError::because(
                ErrorCode::SourceUnavailable,
                ErrorReason::NoAnswer,
            ));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// An empty file only this run knows, removed when the toggle is settled.
struct Receipt {
    path: PathBuf,
}
impl Receipt {
    fn create() -> Result<Self, EngineError> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |time| time.as_nanos());
        let path = std::env::temp_dir().join(format!("ibis-toggle-{}-{nonce}", std::process::id()));
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| EngineError::new(ErrorCode::Internal))?;
        Ok(Self { path })
    }
}
impl Drop for Receipt {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Launch the same desktop binary again; its single-instance plugin forwards
/// `toggle` to the running copy. Overrides win over sibling, Windows install
/// folders (in the order given) and PATH; macOS app selection uses Launch Services.
fn desktop_command(
    platform: &str,
    sibling: Option<&Path>,
    app: Option<&Path>,
    install_roots: &[PathBuf],
    mut is_file: impl FnMut(&Path) -> bool,
) -> std::process::Command {
    // Tauri installers: <root>\<productName>\<binary>, productName Ibis, binary ibis-desktop.
    let installed = install_roots
        .iter()
        .filter(|_| platform == "windows" && app.is_none() && sibling.is_none())
        .filter(|root| !root.as_os_str().is_empty())
        .map(|root| root.join("Ibis").join("ibis-desktop.exe"))
        .find(|path| is_file(path));
    let mut command = if platform == "macos" && (app.is_some() || sibling.is_none()) {
        let mut open = std::process::Command::new("open");
        // A new process is required for single-instance arguments to arrive.
        open.args(["-n", "-a"])
            .arg(app.unwrap_or(Path::new("Ibis")))
            .arg("--args");
        open
    } else {
        let binary = format!("ibis-desktop{}", if platform == "windows" { ".exe" } else { "" });
        std::process::Command::new(app.or(sibling).or(installed.as_deref()).unwrap_or(Path::new(&binary)))
    };
    command.arg("toggle");
    command
}

#[cfg(test)]
mod toggle_tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_program_that_neither_answers_nor_exits_did_not_answer() {
        let receipt = Receipt::create().unwrap_or_else(|error| panic!("{error:?}"));
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .unwrap_or_else(|error| panic!("{error}"));
        let waited = Instant::now();
        let outcome = wait_for_answer(&mut child, false, &receipt.path, Duration::from_millis(200));
        assert_eq!(
            outcome,
            Err(EngineError::because(
                ErrorCode::SourceUnavailable,
                ErrorReason::NoAnswer
            ))
        );
        assert!(waited.elapsed() < Duration::from_secs(2));
        let _ = (child.kill(), child.wait());
        let path = receipt.path.clone();
        drop(receipt);
        assert!(!path.exists(), "the receipt file is removed");
    }

    #[cfg(unix)]
    #[test]
    fn a_launcher_that_fails_means_the_app_is_missing() {
        let receipt = Receipt::create().unwrap_or_else(|error| panic!("{error:?}"));
        let mut child = std::process::Command::new("false")
            .spawn()
            .unwrap_or_else(|error| panic!("{error}"));
        let outcome = wait_for_answer(&mut child, true, &receipt.path, Duration::from_secs(5));
        assert_eq!(
            outcome,
            Err(EngineError::because(ErrorCode::NotFound, ErrorReason::AppMissing))
        );
    }

    #[test]
    fn toggle_accepts_an_explicit_app() {
        let sibling = Some(Path::new("/synthetic/sibling"));
        for app in ["Ibis Nightly", "/synthetic/文献 $(x).app"] {
            let command = desktop_command("macos", sibling, Some(Path::new(app)), &[], |_| false);
            assert_eq!(command.get_program(), "open");
            let expected = ["-n", "-a", app, "--args", "toggle"];
            assert_eq!(command.get_args().collect::<Vec<_>>(), expected);
        }
    }

    #[test]
    fn toggle_launches_the_desktop_with_one_literal_argument() {
        let path = Path::new("/synthetic/Ibis app/ibis-desktop");
        let command = desktop_command("linux", Some(path), None, &[], |_| false);
        assert_eq!(command.get_program(), path);
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["toggle"]);
        let command = desktop_command("macos", None, None, &[], |_| false);
        let expected = ["-n", "-a", "Ibis", "--args", "toggle"];
        assert_eq!(command.get_args().collect::<Vec<_>>(), expected);
    }

    #[test]
    fn linux_nightly_is_selected_explicitly_instead_of_the_stable_default() {
        let nightly = Path::new("/synthetic/bin/ibis-nightly");
        let stable = Path::new("/synthetic/bin/ibis-desktop");
        for sibling in [None, Some(stable)] {
            let command = desktop_command("linux", sibling, Some(nightly), &[], |_| false);
            assert_eq!(command.get_program(), nightly);
            assert_eq!(command.get_args().collect::<Vec<_>>(), ["toggle"]);
        }
        let command = desktop_command("linux", None, None, &[], |_| false);
        assert_eq!(command.get_program(), "ibis-desktop");
    }

    #[test]
    fn toggle_windows_lookup_prefers_override_then_sibling_then_install_folders_then_path() {
        let user = PathBuf::from("/synthetic/Local 文献");
        let machine = PathBuf::from("/synthetic/Program Files");
        let in_user = user.join("Ibis").join("ibis-desktop.exe");
        let in_machine = machine.join("Ibis").join("ibis-desktop.exe");
        let explicit = Path::new("/synthetic/Nightly app.exe");
        let sibling = Path::new("/synthetic/sibling.exe");
        let fallback = Path::new("ibis-desktop.exe");
        let roots = [user.clone(), machine.clone()];
        for (sibling, app, roots, existing, expected) in [
            (None, None, &roots[..], vec![&in_user, &in_machine], in_user.as_path()),
            (None, None, &roots[..], vec![&in_machine], in_machine.as_path()),
            (None, None, &roots[..], vec![], fallback),
            (None, None, &[][..], vec![&in_user], fallback),
            (None, None, &[PathBuf::new()][..], vec![], fallback),
            (Some(sibling), None, &roots[..], vec![&in_user], sibling),
            (Some(sibling), Some(explicit), &roots[..], vec![&in_user], explicit),
        ] {
            let command = desktop_command("windows", sibling, app, roots, |path| {
                assert!(sibling.is_none() && app.is_none());
                existing.iter().any(|p| p.as_path() == path)
            });
            assert_eq!(command.get_program(), expected);
            assert_eq!(command.get_args().collect::<Vec<_>>(), ["toggle"]);
        }
    }
}
