use ibis_core::command_install::{CommandInstall, CommandState, PROFILE_LINE};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

fn fixture() -> (TempDir, CommandInstall) {
    fixture_for("macos")
}

fn fixture_for(package: &str) -> (TempDir, CommandInstall) {
    let dir = TempDir::new().unwrap_or_else(|error| panic!("fixture: {error}"));
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap_or_else(|error| panic!("home: {error}"));
    let binary = dir.path().join(match package {
        "macos" => "Ibis.app/Contents/MacOS/ibis",
        "deb" | "rpm" => "usr/lib/Ibis/binaries/ibis",
        "appimage" => ".mount_Ibis/usr/lib/Ibis/binaries/ibis",
        _ => panic!("unknown package"),
    });
    fs::create_dir_all(binary.parent().unwrap_or_else(|| panic!("parent")))
        .unwrap_or_else(|error| panic!("bundle: {error}"));
    fs::write(&binary, b"invented CLI").unwrap_or_else(|error| panic!("CLI: {error}"));
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|error| panic!("executable CLI: {error}"));
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
fn command_install_and_remove_leave_no_command_or_receipt() {
    for package in ["macos", "deb", "rpm"] {
        let (_dir, install) = fixture_for(package);
        assert_eq!(install.destination(), install.home.join(".local/bin/ibis"));
        assert_eq!(
            install.status().unwrap_or_else(|error| panic!("status: {error}")).state,
            CommandState::NotInstalled
        );
        assert_eq!(
            install
                .install()
                .unwrap_or_else(|error| panic!("install: {error}"))
                .state,
            CommandState::Matching
        );
        assert_eq!(
            fs::read_link(install.destination()).unwrap_or_else(|error| panic!("symlink: {error}")),
            install.bundled
        );
        let reopened = install.clone();
        assert_eq!(
            reopened
                .status()
                .unwrap_or_else(|error| panic!("reopen: {error}"))
                .version
                .as_deref(),
            Some("1.2.3")
        );
        assert_eq!(
            reopened
                .remove()
                .unwrap_or_else(|error| panic!("remove: {error}"))
                .state,
            CommandState::NotInstalled
        );
        assert!(!install.destination().exists());
        assert!(!install.receipt().exists());
    }
}

#[test]
fn missing_path_installs_for_the_terminal_and_gives_the_exact_profile_line() {
    for package in ["macos", "deb", "rpm", "appimage"] {
        let (_dir, mut install) = fixture_for(package);
        install.path.clear();
        assert_eq!(
            install
                .install()
                .unwrap_or_else(|error| panic!("guidance: {error}"))
                .profile_line
                .as_deref(),
            Some(PROFILE_LINE)
        );
        assert_eq!(PROFILE_LINE, "export PATH=\"$HOME/.local/bin:$PATH\"");
        assert!(install.destination().exists());
    }
}

#[test]
fn foreign_commands_and_broken_links_are_never_overwritten_or_removed() {
    for package in ["macos", "deb", "rpm", "appimage"] {
        for broken in [false, true] {
            let (_dir, install) = fixture_for(package);
            fs::create_dir_all(install.destination().parent().unwrap_or_else(|| panic!("bin")))
                .unwrap_or_else(|error| panic!("mkdir: {error}"));
            if broken {
                std::os::unix::fs::symlink("missing-foreign", install.destination())
                    .unwrap_or_else(|error| panic!("foreign link: {error}"));
            } else {
                fs::write(install.destination(), b"foreign ibis")
                    .unwrap_or_else(|error| panic!("foreign command: {error}"));
            }
            assert_eq!(
                install
                    .install()
                    .unwrap_or_else(|error| panic!("refusal: {error}"))
                    .state,
                CommandState::Foreign
            );
            assert!(install.remove().is_err());
            assert!(fs::symlink_metadata(install.destination()).is_ok());
        }
    }
}

#[test]
fn another_ibis_on_path_wins_and_is_never_executed() {
    let (dir, mut install) = fixture();
    let foreign = dir.path().join("homebrew/bin");
    fs::create_dir_all(&foreign).unwrap_or_else(|error| panic!("brew: {error}"));
    fs::write(foreign.join("ibis"), b"foreign executable").unwrap_or_else(|error| panic!("ibis: {error}"));
    fs::set_permissions(foreign.join("ibis"), fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|error| panic!("executable foreign command: {error}"));
    install.path.insert(0, foreign.clone());
    assert_eq!(
        install
            .install()
            .unwrap_or_else(|error| panic!("refusal: {error}"))
            .used_path,
        Some(foreign.join("ibis"))
    );
    assert!(!install.destination().exists());
}

#[test]
fn appimage_copy_survives_unmount_and_reports_an_older_owned_command() {
    let (_dir, mut install) = fixture_for("appimage");
    install.install().unwrap_or_else(|error| panic!("copy: {error}"));
    assert_eq!(install.destination(), install.home.join(".local/bin/ibis"));
    assert!(fs::symlink_metadata(install.destination()).is_ok_and(|metadata| metadata.is_file()));
    fs::remove_file(&install.bundled).unwrap_or_else(|error| panic!("unmount: {error}"));
    assert_eq!(
        install
            .installed_app()
            .unwrap_or_else(|error| panic!("persistent launcher: {error}")),
        install.app
    );
    assert_eq!(
        fs::read(install.destination()).unwrap_or_else(|error| panic!("durable CLI: {error}")),
        b"invented CLI"
    );
    install.version = "1.3.0".into();
    fs::write(&install.bundled, b"new invented CLI").unwrap_or_else(|error| panic!("new mount: {error}"));
    let status = install.status().unwrap_or_else(|error| panic!("older: {error}"));
    assert_eq!(status.state, CommandState::Different);
    assert_eq!(status.version.as_deref(), Some("1.2.3"));
    install
        .remove()
        .unwrap_or_else(|error| panic!("remove owned older CLI: {error}"));
    assert!(!install.destination().exists());
}

#[test]
fn failed_io_leaves_no_command_and_removal_preserves_a_replaced_command() {
    let (_dir, install) = fixture();
    fs::write(install.home.join(".local"), b"blocked directory").unwrap_or_else(|error| panic!("block: {error}"));
    assert!(install.install().is_err());
    assert!(!install.destination().exists());
    fs::remove_file(install.home.join(".local")).unwrap_or_else(|error| panic!("unblock: {error}"));
    install.install().unwrap_or_else(|error| panic!("install: {error}"));
    fs::remove_file(install.destination()).unwrap_or_else(|error| panic!("replace link: {error}"));
    fs::write(install.destination(), b"replacement").unwrap_or_else(|error| panic!("foreign: {error}"));
    assert!(install.remove().is_err());
    assert_eq!(
        fs::read(install.destination()).unwrap_or_else(|error| panic!("kept: {error}")),
        b"replacement"
    );
}

#[test]
fn an_interrupted_install_can_be_completed_after_reopening() {
    for corrupt in [false, true] {
        let (_dir, install) = fixture();
        install
            .install()
            .unwrap_or_else(|error| panic!("first install: {error}"));
        // The receipt is durable, but publication never happened.
        fs::remove_file(install.destination()).unwrap_or_else(|error| panic!("interrupted publication: {error}"));
        if corrupt {
            fs::write(install.receipt(), b"{incomplete").unwrap_or_else(|error| panic!("interrupted receipt: {error}"));
        }
        let reopened = install.clone();
        assert_eq!(
            reopened
                .status()
                .unwrap_or_else(|error| panic!("reopen: {error}"))
                .state,
            CommandState::NotInstalled
        );
        assert_eq!(
            reopened
                .install()
                .unwrap_or_else(|error| panic!("recover: {error}"))
                .state,
            CommandState::Matching
        );
        reopened.remove().unwrap_or_else(|error| panic!("remove: {error}"));
        assert!(!reopened.receipt().exists());
    }
}

#[test]
fn a_delayed_install_finishes_before_a_later_remove_including_after_reopen() {
    use std::future::Future;
    use std::task::{Context, Poll, Waker};
    let (_dir, install) = fixture();
    let earlier = install.clone();
    let later = install.clone();
    tauri::async_runtime::block_on(async {
        let owner = tauri::async_runtime::Mutex::new(());
        let busy = owner.lock().await;
        let mut first = Box::pin(super::with_owner(&owner, move || {
            earlier.install().map_err(|e| e.to_string())
        }));
        let mut second = Box::pin(super::with_owner(&owner, move || {
            later.remove().map_err(|e| e.to_string())
        }));
        let mut context = Context::from_waker(Waker::noop());
        assert!(matches!(first.as_mut().poll(&mut context), Poll::Pending));
        assert!(matches!(second.as_mut().poll(&mut context), Poll::Pending));
        drop(busy);
        // Even if the newer worker gets CPU first, FIFO keeps its removal last.
        assert!(matches!(second.as_mut().poll(&mut context), Poll::Pending));
        assert_eq!(
            first.await.unwrap_or_else(|error| panic!("install: {error}")).state,
            CommandState::Matching
        );
        assert_eq!(
            second.await.unwrap_or_else(|error| panic!("remove: {error}")).state,
            CommandState::NotInstalled
        );
    });
    assert!(!install.destination().exists());
    assert!(!install.receipt().exists());
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
    let mut config: serde_json::Value =
        serde_json::from_str(include_str!("../tauri.conf.json")).unwrap_or_else(|error| panic!("base config: {error}"));
    for overlay in [
        include_str!("../tauri.bundle.conf.json"),
        include_str!("../tauri.linux.bundle.conf.json"),
    ] {
        merge(
            &mut config,
            serde_json::from_str(overlay).unwrap_or_else(|error| panic!("overlay: {error}")),
        );
        let _: tauri::Config = serde_json::from_value(config.clone())
            .unwrap_or_else(|error| panic!("valid release configuration: {error}"));
    }
}

#[test]
fn settings_sees_the_installer_path_before_the_apps_environment_is_restarted() {
    let (_dir, mut install) = fixture();
    install.path.clear();
    assert_eq!(
        install.status().unwrap_or_else(|e| panic!("status: {e}")).state,
        CommandState::NotInstalled
    );
    let registered = std::env::join_paths([install.bundled.parent().unwrap_or_else(|| panic!("bundle folder"))])
        .unwrap_or_else(|e| panic!("registered PATH: {e}"));
    install.path = super::with_registered_path(install.path, Some(&registered));
    let status = install.status().unwrap_or_else(|e| panic!("new installation: {e}"));
    assert_eq!(status.state, CommandState::Matching);
    assert_eq!(status.version.as_deref(), Some("1.2.3"));
}
