//! Lookup consent uses the settings owner's existing transactions and observations.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::sync::mpsc;
use std::time::Duration;

use ibis_engine::settings::{self, Device, Owner, PrimaryRead, SettingsError};
use ibis_engine::settings_backup;

fn profile() -> (tempfile::TempDir, Owner) {
    let directory = tempfile::tempdir().unwrap();
    let owner = Owner::open(directory.path().join("settings.json"));
    (directory, owner)
}

fn choose(owner: &Owner, enabled: bool) -> Result<(), SettingsError> {
    owner
        .change(|settings| {
            settings.device.online_lookup = Some(enabled);
            Ok::<_, SettingsError>(())
        })
        .map(|_| ())
}

#[test]
fn a_fresh_profile_stays_unasked_and_off_without_creating_settings() {
    let (_directory, owner) = profile();
    assert_eq!(owner.read_with(|loaded| loaded.online_lookup()), Some(None));
    assert!(!owner.path().exists());
    owner.close();
    let reopened = Owner::open(owner.path().to_path_buf());
    assert_eq!(reopened.read().online_lookup(), None);
    assert!(!reopened.path().exists());
}

#[test]
fn explicit_enable_persists_and_reopens_without_approving_first_use() {
    let (_directory, owner) = profile();
    choose(&owner, true).unwrap();
    owner.close();
    let reopened = Owner::open(owner.path().to_path_buf());
    let loaded = reopened.read();
    assert_eq!(loaded.online_lookup(), Some(true));
    assert_eq!(
        loaded.cites().unwrap().device,
        Device {
            online_lookup: Some(true),
            ..Device::default()
        },
        "enabling records only the switch, never first-use disclosure approval"
    );
    assert!(loaded.cites().unwrap().unknown.is_empty());
}

#[test]
fn saved_and_external_false_are_observed_by_the_same_owner() {
    let (_directory, owner) = profile();
    choose(&owner, true).unwrap();
    let (send, edits) = mpsc::channel();
    let _watch = owner
        .watch(move |receipt, loaded| {
            let _ = send.send((receipt, loaded.online_lookup()));
        })
        .expect("native settings watching");
    choose(&owner, false).unwrap();
    assert_eq!(owner.read_with(|loaded| loaded.online_lookup()), Some(Some(false)));
    choose(&owner, true).unwrap();

    let replacement = owner.path().with_extension("replacement");
    fs::write(&replacement, r#"{"online_lookup":false}"#).unwrap();
    fs::rename(replacement, owner.path()).unwrap();
    // A queued read also observes edits, without depending on native-event timing.
    assert_eq!(owner.read_with(|loaded| loaded.online_lookup()), Some(Some(false)));
    let (_, choice) = edits.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(choice, Some(false));
}

#[test]
fn a_queued_disable_wins_over_a_delayed_enable() {
    let (_directory, owner) = profile();
    let (started, start) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let enable = owner.submit(move |settings| {
        started.send(()).unwrap();
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
        settings.device.online_lookup = Some(true);
        Ok::<_, SettingsError>(())
    });
    start.recv_timeout(Duration::from_secs(5)).unwrap();
    let disable = owner.submit(|settings| {
        settings.device.online_lookup = Some(false);
        Ok::<_, SettingsError>(())
    });
    release.send(()).unwrap();
    enable.wait().unwrap().unwrap();
    disable.wait().unwrap().unwrap();
    owner.close();
    assert_eq!(
        Owner::open(owner.path().to_path_buf()).read().online_lookup(),
        Some(false)
    );
}

#[test]
fn unusable_primary_and_backup_recovery_never_grant_lookup() {
    let (_directory, owner) = profile();
    let backup = owner.path().with_extension("json.bak");
    for recovery in [false, true] {
        if recovery {
            fs::write(&backup, r#"{"online_lookup":true}"#).unwrap();
        }
        for primary in [
            r#"{"online_lookup":true,"density":"invalid"}"#,
            r#"{"online_lookup":"true"}"#,
            r#"{"online_lookup":1}"#,
            r#"{"schema_version":999,"online_lookup":true}"#,
        ] {
            fs::write(owner.path(), primary).unwrap();
            let loaded = owner.read();
            assert!(loaded.problem.is_some());
            assert_eq!(loaded.from_backup, recovery);
            assert_eq!(loaded.online_lookup(), None);
            assert!(choose(&owner, true).is_err());
            assert_eq!(fs::read_to_string(owner.path()).unwrap(), primary);
        }
        // A directory is deterministically unreadable as settings, even as root.
        fs::remove_file(owner.path()).unwrap();
        fs::create_dir(owner.path()).unwrap();
        let loaded = owner.read();
        assert_eq!(loaded.primary_read, PrimaryRead::Unreadable);
        assert_eq!(loaded.online_lookup(), None);
        assert!(choose(&owner, true).is_err());
        fs::remove_dir(owner.path()).unwrap();
        let missing = owner.read();
        assert_eq!(missing.primary_read, PrimaryRead::Missing);
        assert_eq!(missing.from_backup, recovery);
        assert_eq!(missing.online_lookup(), None);
    }
}

#[test]
fn portable_backups_cannot_change_this_devices_lookup_choice() {
    let (directory, owner) = profile();
    choose(&owner, true).unwrap();
    let text = settings_backup::export_file(owner.path().to_path_buf(), "0.3.6").unwrap();
    let mut exported: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(exported["settings"].get("online_lookup").is_none());
    // Even a hand-edited backup cannot carry consent to another profile.
    exported["settings"]["online_lookup"] = true.into();
    exported["settings"]["larger_text"] = true.into();
    let source = directory.path().join("portable.json");
    fs::write(&source, exported.to_string()).unwrap();
    for choice in [None, Some(false), Some(true)] {
        let (_target_directory, target) = profile();
        if let Some(choice) = choice {
            choose(&target, choice).unwrap();
        }
        let (_, mut plan) = settings_backup::prepare(target.path().to_path_buf(), source.clone()).unwrap();
        // The public native plan is not an alternate enable operation either.
        plan.restored.device.online_lookup = Some(true);
        target.restore(&plan).unwrap();
        let loaded = target.read();
        assert_eq!(loaded.online_lookup(), choice);
        assert_eq!(loaded.cites().unwrap().portable.larger_text, Some(true));
    }
}

#[test]
fn repairing_settings_cannot_promote_consent_from_the_automatic_backup() {
    let (directory, owner) = profile();
    let source = directory.path().join("portable.json");
    fs::write(
        &source,
        settings::backup::export(&settings::Settings::default(), "0.3.6").unwrap(),
    )
    .unwrap();
    for missing in [false, true] {
        for automatic in [false, true] {
            if !missing {
                fs::write(owner.path(), "{ damaged").unwrap();
            }
            let backup = owner.path().with_extension("json.bak");
            fs::write(&backup, r#"{"online_lookup":true,"update_checks":false}"#).unwrap();
            assert_eq!(owner.read().online_lookup(), None);
            let source = if automatic { &backup } else { &source };
            let (_, plan) = settings_backup::prepare(owner.path().to_path_buf(), source.clone()).unwrap();
            owner.restore(&plan).unwrap();
            let repaired = owner.read();
            assert!(repaired.problem.is_none());
            assert_eq!(repaired.online_lookup(), None);
            assert_eq!(repaired.cites().unwrap().device.update_checks, Some(false));
            fs::remove_file(owner.path()).unwrap();
        }
    }
}

#[test]
fn failed_saves_preserve_the_previous_choice_and_unrelated_fields() {
    let (_directory, owner) = profile();
    for previous in [false, true] {
        fs::write(
            owner.path(),
            format!(r#"{{"online_lookup":{previous},"larger_text":true,"update_checks":false,"future_choice":42}}"#),
        )
        .unwrap();
        let before = fs::read(owner.path()).unwrap();
        let backup = owner.path().with_extension("json.bak");
        fs::create_dir(&backup).unwrap();
        assert!(choose(&owner, !previous).is_err());
        assert_eq!(fs::read(owner.path()).unwrap(), before);
        assert_eq!(owner.read().online_lookup(), Some(previous));
        fs::remove_dir(&backup).unwrap();
        choose(&owner, !previous).unwrap();
        let loaded = owner.read();
        assert_eq!(loaded.online_lookup(), Some(!previous));
        let saved = loaded.cites().unwrap();
        assert_eq!(saved.portable.larger_text, Some(true));
        assert_eq!(saved.device.update_checks, Some(false));
        assert_eq!(saved.unknown["future_choice"], 42);
        fs::remove_file(backup).unwrap();
    }
}
