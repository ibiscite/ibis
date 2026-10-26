//! Explicit desktop launch utility; tested with constructed commands/fake binaries.
use ibis_core::contract::{EngineError, ErrorCode};
use std::path::{Path, PathBuf};
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
    let mut command = desktop_command(
        std::env::consts::OS,
        sibling.as_deref(),
        app,
        &install_roots,
        Path::is_file,
    );
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let failed = || EngineError {
        code: ErrorCode::SourceUnavailable,
    };
    if cfg!(target_os = "macos") && (app.is_some() || sibling.is_none()) {
        if !command.status().map_err(|_| failed())?.success() {
            return Err(failed());
        }
    } else {
        command.spawn().map_err(|_| failed())?;
    }
    Ok(())
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
