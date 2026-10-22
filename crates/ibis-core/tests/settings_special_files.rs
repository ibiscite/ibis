#![cfg(unix)]

use std::fs;
use std::os::unix::fs::FileTypeExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use ibis_core::settings::{self, Settings, SettingsError};

#[test]
fn special_settings_files_never_block_or_replace_a_recovery_file() {
    if let Some(root) = std::env::var_os("IBIS_SPECIAL_SETTINGS_CHILD") {
        let root = std::path::PathBuf::from(root);
        let path = root.join("settings.json");
        let backup = path.with_extension("json.bak");
        let temporary = path.with_extension("json.tmp");
        let pipe = |path: &std::path::Path| {
            assert!(
                Command::new("mkfifo")
                    .arg(path)
                    .status()
                    .unwrap_or_else(|e| panic!("{e}"))
                    .success()
            );
        };
        let recovery = br#"{"shortcut":"Alt+Space"}"#;
        pipe(&path);
        fs::write(&backup, recovery).unwrap_or_else(|e| panic!("{e}"));
        let loaded = settings::load(&path);
        assert!(loaded.from_backup);
        assert_eq!(loaded.settings.shortcut.as_deref(), Some("Alt+Space"));
        assert!(matches!(loaded.problem, Some(SettingsError::Unreadable(_))));
        assert_eq!(loaded.primary_read, settings::PrimaryRead::Unreadable);
        assert!(settings::update(&path, |s| s.shortcut = None).is_err());
        assert!(settings::save(&path, &Settings::default()).is_err());
        assert!(
            fs::symlink_metadata(&path)
                .unwrap_or_else(|e| panic!("{e}"))
                .file_type()
                .is_fifo()
        );
        assert_eq!(fs::read(&backup).unwrap_or_default(), recovery);

        fs::remove_file(&path).unwrap_or_else(|e| panic!("{e}"));
        fs::write(&path, b"{").unwrap_or_else(|e| panic!("{e}"));
        fs::remove_file(&backup).unwrap_or_else(|e| panic!("{e}"));
        pipe(&backup);
        let loaded = settings::load(&path);
        assert!(!loaded.from_backup);
        assert!(matches!(loaded.problem, Some(SettingsError::Invalid(_))));
        fs::write(&path, br#"{"schema_version":99}"#).unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(
            settings::save(&path, &Settings::default()),
            Err(SettingsError::NewerVersion(99))
        ));
        fs::write(&path, b"{}").unwrap_or_else(|e| panic!("{e}"));
        assert!(settings::update(&path, |s| s.shortcut = Some("Alt+Space".into())).is_err());
        assert!(settings::save(&path, &Settings::default()).is_err());
        assert_eq!(fs::read(&path).unwrap_or_default(), b"{}");
        assert!(!temporary.exists());
        assert!(
            fs::symlink_metadata(&backup)
                .unwrap_or_else(|e| panic!("{e}"))
                .file_type()
                .is_fifo()
        );

        fs::remove_file(&backup).unwrap_or_else(|e| panic!("{e}"));
        fs::write(&backup, recovery).unwrap_or_else(|e| panic!("{e}"));
        pipe(&temporary);
        assert!(settings::update(&path, |s| s.shortcut = Some("Alt+Space".into())).is_err());
        assert_eq!(fs::read(&path).unwrap_or_default(), b"{}");
        assert_eq!(fs::read(&backup).unwrap_or_default(), recovery);
        assert!(
            fs::symlink_metadata(&temporary)
                .unwrap_or_else(|e| panic!("{e}"))
                .file_type()
                .is_fifo()
        );
        fs::remove_file(&temporary).unwrap_or_else(|e| panic!("{e}"));
        fs::remove_file(&path).unwrap_or_else(|e| panic!("{e}"));
        let ordinary = root.join("ordinary.json");
        fs::write(&ordinary, recovery).unwrap_or_else(|e| panic!("{e}"));
        std::os::unix::fs::symlink(&ordinary, &path).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(settings::load(&path).settings.shortcut.as_deref(), Some("Alt+Space"));
        return;
    }

    // Keep the deadline outside the loader process, as in the existing budgets.
    let root = tempfile::Builder::new()
        .prefix("ibis-special-settings-")
        .tempdir()
        .unwrap_or_else(|e| panic!("{e}"));
    let mut child = Command::new(std::env::current_exe().unwrap_or_else(|e| panic!("{e}")))
        .args([
            "--exact",
            "special_settings_files_never_block_or_replace_a_recovery_file",
            "--nocapture",
        ])
        .env("IBIS_SPECIAL_SETTINGS_CHILD", root.path())
        .stdin(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| panic!("{e}"));
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap_or_else(|e| panic!("{e}")) {
            assert!(status.success(), "special settings subprocess failed");
            break;
        }
        if start.elapsed() > Duration::from_secs(5) {
            child.kill().unwrap_or_else(|e| panic!("{e}"));
            let _ = child.wait();
            panic!("synthetic special settings load exceeded the five second deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
