use ibis_core::command_install::{CommandInstall, CommandState, PROFILE_LINE};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

fn fixture() -> (TempDir, CommandInstall) {
    fixture_for("macos")
}

fn fixture_for(package: &str) -> (TempDir, CommandInstall) {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap();
    let binary = dir.path().join(match package {
        "macos" => "Ibis.app/Contents/MacOS/ibis",
        "deb" | "rpm" => "usr/lib/Ibis/binaries/ibis",
        "appimage" => ".mount_Ibis/usr/lib/Ibis/binaries/ibis",
        _ => panic!("unknown package"),
    });
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(&binary, b"invented CLI").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    let install = CommandInstall::new(
        home.clone(),
        vec![home.join(".local/bin")],
        binary,
        (package == "appimage").then(|| home.join("Ibis.AppImage")),
        "1.2.3".into(),
    );
    (dir, install)
}

#[test]
fn agent_full_path_is_the_cli_in_each_package_never_the_desktop_executable() {
    for package in ["macos", "deb", "rpm", "appimage"] {
        let (dir, mut install) = fixture_for(package);
        let desktop = dir.path().join("Ibis.app/Contents/MacOS/ibis-desktop");
        fs::create_dir_all(desktop.parent().unwrap()).unwrap();
        fs::write(&desktop, b"invented desktop").unwrap();
        install.path.clear();
        assert_eq!(
            super::agent_environment(install.clone(), dir.path().join("config"), None)
                .unwrap()
                .ibis,
            (package != "appimage").then(|| install.bundled.clone())
        );
        assert_ne!(
            super::agent_environment(install.clone(), dir.path().join("config"), None)
                .unwrap()
                .ibis,
            Some(desktop)
        );
        install.install().unwrap();
        assert_eq!(
            super::agent_environment(install.clone(), dir.path().join("config"), None)
                .unwrap()
                .ibis
                .unwrap(),
            install.destination()
        );
        if package == "appimage" {
            fs::set_permissions(install.destination(), fs::Permissions::from_mode(0o600)).unwrap();
            assert_eq!(
                super::agent_environment(install.clone(), dir.path().join("config"), None)
                    .unwrap()
                    .ibis,
                None
            );
        }
        install.remove().unwrap();
        let foreign = dir.path().join("other-bin");
        fs::create_dir_all(&foreign).unwrap();
        fs::write(foreign.join("ibis"), b"invented command").unwrap();
        fs::set_permissions(foreign.join("ibis"), fs::Permissions::from_mode(0o755)).unwrap();
        install.path = vec![foreign.clone()];
        assert_eq!(
            super::agent_environment(install.clone(), dir.path().join("config"), None)
                .unwrap()
                .ibis
                .unwrap(),
            foreign.join("ibis")
        );
    }
}

#[test]
fn command_install_and_remove_leave_no_command_or_receipt() {
    for package in ["macos", "deb", "rpm"] {
        let (_dir, install) = fixture_for(package);
        assert_eq!(install.destination(), install.home.join(".local/bin/ibis"));
        assert_eq!(install.status().unwrap().state, CommandState::NotInstalled);
        assert_eq!(install.install().unwrap().state, CommandState::Matching);
        assert_eq!(fs::read_link(install.destination()).unwrap(), install.bundled);
        let reopened = install.clone();
        assert_eq!(reopened.status().unwrap().version.as_deref(), Some("1.2.3"));
        assert_eq!(reopened.remove().unwrap().state, CommandState::NotInstalled);
        assert!(!install.destination().exists());
        assert!(!install.receipt().exists());
    }
}

#[test]
fn missing_path_installs_for_the_terminal_and_gives_the_exact_profile_line() {
    for package in ["macos", "deb", "rpm", "appimage"] {
        let (_dir, mut install) = fixture_for(package);
        install.path.clear();
        assert_eq!(install.install().unwrap().profile_line.as_deref(), Some(PROFILE_LINE));
        assert_eq!(PROFILE_LINE, "export PATH=\"$HOME/.local/bin:$PATH\"");
        assert!(install.destination().exists());
    }
}

#[test]
fn foreign_commands_and_broken_links_are_never_overwritten_or_removed() {
    for package in ["macos", "deb", "rpm", "appimage"] {
        for broken in [false, true] {
            let (_dir, install) = fixture_for(package);
            fs::create_dir_all(install.destination().parent().unwrap()).unwrap();
            if broken {
                std::os::unix::fs::symlink("missing-foreign", install.destination()).unwrap();
            } else {
                fs::write(install.destination(), b"foreign ibis").unwrap();
            }
            assert_eq!(install.install().unwrap().state, CommandState::Foreign);
            assert!(install.remove().is_err());
            assert!(fs::symlink_metadata(install.destination()).is_ok());
        }
    }
}

#[test]
fn another_ibis_on_path_wins_and_is_never_executed() {
    let (dir, mut install) = fixture();
    let foreign = dir.path().join("homebrew/bin");
    fs::create_dir_all(&foreign).unwrap();
    fs::write(foreign.join("ibis"), b"foreign executable").unwrap();
    fs::set_permissions(foreign.join("ibis"), fs::Permissions::from_mode(0o755)).unwrap();
    install.path.insert(0, foreign.clone());
    assert_eq!(install.install().unwrap().used_path, Some(foreign.join("ibis")));
    assert!(!install.destination().exists());
}

#[test]
fn failed_io_leaves_no_command_and_removal_preserves_a_replaced_command() {
    let (_dir, install) = fixture();
    fs::write(install.home.join(".local"), b"blocked directory").unwrap();
    assert!(install.install().is_err());
    assert!(!install.destination().exists());
    fs::remove_file(install.home.join(".local")).unwrap();
    install.install().unwrap();
    fs::remove_file(install.destination()).unwrap();
    fs::write(install.destination(), b"replacement").unwrap();
    assert!(install.remove().is_err());
    assert_eq!(fs::read(install.destination()).unwrap(), b"replacement");
}

#[test]
fn release_bundle_overlays_are_valid_tauri_configuration_on_every_platform() {
    fn merge(base: &mut serde_json::Value, overlay: serde_json::Value) {
        if let (Some(base), Some(overlay)) = (base.as_object_mut(), overlay.as_object()) {
            for (key, value) in overlay {
                merge(base.entry(key).or_insert(serde_json::Value::Null), value.clone());
            }
        } else {
            *base = overlay;
        }
    }
    let mut config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    for overlay in [
        include_str!("../tauri.bundle.conf.json"),
        include_str!("../tauri.linux.bundle.conf.json"),
    ] {
        merge(&mut config, serde_json::from_str(overlay).unwrap());
        let _: tauri::Config = serde_json::from_value(config.clone()).unwrap();
    }
}

#[test]
fn settings_sees_the_installer_path_before_the_apps_environment_is_restarted() {
    let (_dir, mut install) = fixture();
    install.path.clear();
    assert_eq!(install.status().unwrap().state, CommandState::NotInstalled);
    let registered = std::env::join_paths([install.bundled.parent().unwrap()]).unwrap();
    install.path = super::with_registered_path(install.path, Some(&registered));
    let status = install.status().unwrap();
    assert_eq!(status.state, CommandState::Matching);
    assert_eq!(status.version.as_deref(), Some("1.2.3"));
}
