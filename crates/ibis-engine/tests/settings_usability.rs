#![allow(clippy::unwrap_used)]

use ibis_engine::settings::{self, Owner, Purpose, SettingsError};

#[test]
fn settings_usability_keeps_recovery_visible_and_never_substitutes_citation_choices() {
    for (primary, backup, interface, cites) in [
        (None, None, true, true),
        (Some("{}"), None, true, true),
        (Some("{ invalid"), None, false, false),
        (Some("{ invalid"), Some(r#"{"output":"ieee"}"#), true, false),
        (None, Some(r#"{"output":"ieee"}"#), true, false),
        (Some(r#"{"schema_version":999}"#), None, false, false),
        (
            Some(r#"{"schema_version":999}"#),
            Some(r#"{"output":"ieee"}"#),
            true,
            false,
        ),
        (None, Some(r#"{"schema_version":999}"#), false, false),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        if let Some(text) = primary {
            std::fs::write(&path, text).unwrap();
        }
        if let Some(text) = backup {
            std::fs::write(path.with_extension("json.bak"), text).unwrap();
        }
        let loaded = settings::load(&path);
        assert_eq!(
            loaded.usable(Purpose::Interface).is_ok(),
            interface,
            "{primary:?}, {backup:?}"
        );
        assert_eq!(loaded.usable(Purpose::Cites).is_ok(), cites, "{primary:?}, {backup:?}");
        if interface && !cites {
            assert!(loaded.problem.is_some(), "a recovered interface has a notice");
            assert_eq!(
                loaded.usable(Purpose::Interface).unwrap().portable.output.as_deref(),
                Some("ieee")
            );
        }
        assert_eq!(std::fs::read_to_string(&path).ok().as_deref(), primary);
        assert_eq!(
            std::fs::read_to_string(path.with_extension("json.bak")).ok().as_deref(),
            backup
        );
        let owner = Owner::open(path.clone());
        let mut applied = false;
        let edited = owner.change(|settings| {
            applied = true;
            settings.portable.output = Some("apa".into());
            Ok::<_, SettingsError>(())
        });
        assert_eq!(edited.is_ok(), cites, "ordinary edits need citation settings");
        assert_eq!(applied, cites);
        if !cites {
            assert_eq!(std::fs::read_to_string(&path).ok().as_deref(), primary);
            assert_eq!(
                std::fs::read_to_string(path.with_extension("json.bak")).ok().as_deref(),
                backup
            );
            if primary.is_none() {
                assert!(owner.ensure_file().is_err());
                assert!(!path.exists());
            }
        }
    }
}

#[test]
fn an_unreadable_primary_allows_only_interface_recovery_and_is_left_alone() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    std::fs::create_dir(&path).unwrap();
    let backup = r#"{"output":"ieee"}"#;
    std::fs::write(path.with_extension("json.bak"), backup).unwrap();
    let loaded = settings::load(&path);
    assert!(loaded.usable(Purpose::Cites).is_err());
    assert!(loaded.problem.is_some());
    assert_eq!(
        loaded.usable(Purpose::Interface).unwrap().portable.output.as_deref(),
        Some("ieee")
    );
    assert!(path.is_dir());
    assert_eq!(
        std::fs::read_to_string(path.with_extension("json.bak")).unwrap(),
        backup
    );
}

#[test]
fn an_unknown_setting_stays_on_this_device_through_edits_and_restore() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let future = serde_json::json!({"enabled": true, "choices": ["invented"]});
    std::fs::write(
        &path,
        serde_json::json!({
            "future_setting": future,
            "update_checks": false,
            "editor_endpoint": {"port":23124,"token":"invented-device-token"},
            "zotero_profile": "Invented profile",
            "delights": {"paste_count":7},
            "picker_windows": {"invented-display":{"width":600,"height":400,"custom_size":true}},
            "writings": [{"name":"Atlas","format":"pandoc"}],
            "writing_apps": {"macos:example.writer":"Atlas"},
            "app_outputs": [{"app":{"system":"macos","id":"example.writer"},"output":"ieee"}],
            "suggestion_reports": ["keys"],
            "key_formats_used": true
        })
        .to_string(),
    )
    .unwrap();
    let owner = Owner::open(path.clone());
    let (saved, ()) = owner
        .change(|settings| {
            settings.portable.paste = Some(false);
            Ok::<_, SettingsError>(())
        })
        .unwrap();
    let text = settings::backup::export(&saved, "0.2.8").unwrap();
    let written: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        written["settings"],
        serde_json::json!({"schema_version":2,"paste":false})
    );
    // A foreign backup cannot import device capabilities or unknown fields.
    let mut imported = written;
    imported["settings"]["paste"] = serde_json::json!(true);
    imported["settings"]["editor_endpoint"] = serde_json::json!({"port":9,"token":"foreign-token"});
    imported["settings"]["future_setting"] = serde_json::json!("foreign value");
    let backup = settings::backup::parse(&imported.to_string()).unwrap();
    let plan = settings::backup::plan(&saved, &backup, |_| true);
    owner.restore(&plan).unwrap();
    let reopened = settings::load(&path);
    let settings = reopened.usable(Purpose::Cites).unwrap();
    assert_eq!(settings.unknown["future_setting"], future);
    assert_eq!(settings.portable.paste, Some(true));
    assert_eq!(settings.device, saved.device);
}

#[test]
fn a_queued_read_refuses_missing_citation_settings_without_restoring_the_backup() {
    use std::sync::mpsc;
    use std::time::Duration;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    std::fs::write(&path, r#"{"output":"ieee"}"#).unwrap();
    let owner = Owner::open(path.clone());
    owner
        .change(|settings| {
            settings.portable.larger_text = Some(true);
            Ok::<_, SettingsError>(())
        })
        .unwrap();
    let backup = std::fs::read(path.with_extension("json.bak")).unwrap();
    let (started, entering) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let pending = owner.submit(move |_| {
        started.send(()).unwrap();
        held.recv_timeout(Duration::from_secs(5)).unwrap();
        Ok::<_, SettingsError>(())
    });
    entering.recv_timeout(Duration::from_secs(5)).unwrap();
    std::thread::scope(|scope| {
        let read = scope.spawn(|| {
            owner.read_with(|loaded| {
                assert!(loaded.usable(Purpose::Cites).is_err());
                assert_eq!(
                    loaded.usable(Purpose::Interface).unwrap().portable.output.as_deref(),
                    Some("ieee")
                );
            })
        });
        std::fs::remove_file(&path).unwrap();
        release.send(()).unwrap();
        assert!(pending.wait().unwrap().is_err());
        assert_eq!(read.join().unwrap(), Some(()));
    });
    owner.close();
    assert!(!path.exists());
    assert_eq!(std::fs::read(path.with_extension("json.bak")).unwrap(), backup);
    assert!(settings::load(&path).usable(Purpose::Cites).is_err());
}
