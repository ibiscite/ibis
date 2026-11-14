//! The same backup operation serves the command line and native Settings.
#![allow(clippy::unwrap_used)]

#[path = "support/settings.rs"]
mod fixture;

use std::fs;

use ibis_core::contract::ErrorCode;
use ibis_engine::settings::{self, Density, LibrarySetting};
use ibis_engine::settings_backup::{self as backup, BackupProblem};

#[test]
fn neither_client_exports_absent_or_recovered_settings_as_a_valid_backup() {
    let (_directory, target) = fixture::file();
    let absent = backup::export_file(target.clone(), "0.2.8").unwrap_err();
    assert_eq!(backup::code(&absent), ErrorCode::NotFound);
    fs::write(&target, "{ damaged").unwrap();
    fs::write(target.with_extension("json.bak"), "{}").unwrap();
    let recovered = backup::export_file(target.clone(), "0.2.8").unwrap_err();
    assert_eq!(backup::problem(&recovered), BackupProblem::SettingsUnusable);
    assert_eq!(fs::read_to_string(target).unwrap(), "{ damaged");
}

#[test]
fn export_preview_and_restore_agree_and_dry_run_changes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("original.json");
    fs::write(&original, r#"{"density":"compact","output":"ieee","paste":true}"#).unwrap();
    let source = directory.path().join("backup.json");
    fs::write(&source, backup::export_file(original, "0.2.8").unwrap()).unwrap();
    let target = directory.path().join("settings.json");
    let before = r#"{"density":"comfortable","update_checks":false}"#;
    fs::write(&target, before).unwrap();
    let approvals = directory.path().join("apps.json");
    fs::write(&approvals, r#"{"invented.editor":{"paste":false}}"#).unwrap();
    let (_, plan) = backup::prepare(target.clone(), source.clone()).unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), before, "preview is a dry run");
    assert!(!target.with_extension("json.bak").exists());
    assert_eq!(
        plan.changes
            .iter()
            .map(|change| change.setting.as_str())
            .collect::<Vec<_>>(),
        ["density", "output", "paste"]
    );
    ibis_engine::settings::Owner::open(target.clone())
        .restore(&plan)
        .unwrap();
    let reopened = fixture::read(&target);
    assert_eq!(reopened.portable.density, Some(Density::Compact));
    assert_eq!(reopened.portable.output.as_deref(), Some("ieee"));
    assert_eq!(reopened.device.update_checks, Some(false));
    assert_eq!(
        fs::read_to_string(&approvals).unwrap(),
        r#"{"invented.editor":{"paste":false}}"#
    );
    assert_eq!(fs::read_to_string(target.with_extension("json.bak")).unwrap(), before);
    assert!(backup::prepare(target, source).unwrap().1.is_empty());
}

#[test]
fn an_export_at_the_automatic_backup_path_restores_its_library_and_format() {
    let (_directory, target) = fixture::file();
    let library = LibrarySetting::Bibtex("/invented/atlas.bib".into());
    fixture::edit(&target, |settings| {
        settings.portable.library = Some(library.clone());
        settings.portable.output = Some("ieee".into());
    })
    .unwrap();
    let source = target.with_extension("json.bak");
    backup::save(&source, &backup::export_file(target.clone(), "0.3.6").unwrap(), true).unwrap();
    fs::write(&target, "{}").unwrap();
    let (_, plan) = backup::prepare(target.clone(), source).unwrap();
    settings::Owner::open(target.clone()).restore(&plan).unwrap();
    let restored = fixture::read(&target);
    assert_eq!(restored.portable.library, Some(library));
    assert_eq!(restored.portable.output.as_deref(), Some("ieee"));
}

#[test]
fn malformed_newer_and_unreadable_backups_refuse_without_echoing_private_input() {
    let (directory, target) = fixture::file();
    fs::write(&target, "{}").unwrap();
    let source = directory.path().join("backup.json");
    for (text, problem, code) in [
        ("{", BackupProblem::NotABackup, ErrorCode::BadInput),
        (
            r#"{"ibis_settings_backup":2}"#,
            BackupProblem::Newer,
            ErrorCode::NewerFormat,
        ),
        (
            r#"{"ibis_settings_backup":1,"made_on":"linux","made_by":"0.2.8","settings":{"density":"/invented/private"}}"#,
            BackupProblem::Invalid,
            ErrorCode::BadInput,
        ),
    ] {
        for source in [&source, &target.with_extension("json.bak")] {
            fs::write(source, text).unwrap();
            let error = backup::prepare(target.clone(), source.clone()).unwrap_err();
            assert_eq!(backup::problem(&error), problem);
            assert_eq!(backup::code(&error), code);
            assert!(!backup::message(&error).contains("private"));
            assert_eq!(fs::read_to_string(&target).unwrap(), "{}");
        }
    }
    fs::remove_file(&source).unwrap();
    let error = backup::prepare(target, source).unwrap_err();
    assert_eq!(backup::problem(&error), BackupProblem::Unreadable);
    assert_eq!(backup::code(&error), ErrorCode::NotFound);
}

#[test]
fn newer_settings_in_either_primary_or_recovery_file_refuse_preview_and_export() {
    let (directory, target) = fixture::file();
    let source = directory.path().join("backup.json");
    fs::write(&target, "{}").unwrap();
    fs::write(&source, backup::export_file(target.clone(), "0.2.8").unwrap()).unwrap();
    for newer in [&target, &target.with_extension("json.bak")] {
        fs::write(newer, r#"{"schema_version":999}"#).unwrap();
        for error in [
            backup::prepare(target.clone(), source.clone()).unwrap_err(),
            backup::export_file(target.clone(), "0.2.8").unwrap_err(),
        ] {
            assert_eq!(backup::code(&error), ErrorCode::NewerFormat);
            assert_eq!(backup::problem(&error), BackupProblem::SettingsUnusable);
        }
        assert_eq!(fs::read_to_string(newer).unwrap(), r#"{"schema_version":999}"#);
        fs::remove_file(newer).unwrap();
    }
}

#[test]
fn a_change_after_preview_refuses_commit_and_keeps_the_newer_choice() {
    let (directory, target) = fixture::file();
    let source = directory.path().join("backup.json");
    fs::write(&target, r#"{"density":"compact"}"#).unwrap();
    fs::write(&source, backup::export_file(target.clone(), "0.2.8").unwrap()).unwrap();
    fs::write(&target, "{}").unwrap();
    let (_, plan) = backup::prepare(target.clone(), source).unwrap();
    fixture::edit(&target, |settings| {
        settings.portable.larger_text = Some(true);
    })
    .unwrap();
    let before = fs::read(&target).unwrap();
    let error = ibis_engine::settings::Owner::open(target.clone())
        .restore(&plan)
        .unwrap_err();
    assert_eq!(backup::problem(&error), BackupProblem::Changed);
    assert_eq!(fs::read(&target).unwrap(), before);
}

#[test]
fn failed_write_preserves_existing_settings_and_approvals() {
    let (directory, target) = fixture::file();
    let source = directory.path().join("backup.json");
    fs::write(&target, r#"{"density":"compact"}"#).unwrap();
    fs::write(&source, backup::export_file(target.clone(), "0.2.8").unwrap()).unwrap();
    fs::write(&target, "{}").unwrap();
    let approvals = directory.path().join("apps.json");
    let denied = r#"{"invented.editor":{"paste":false}}"#;
    fs::write(&approvals, denied).unwrap();
    let (_, plan) = backup::prepare(target.clone(), source).unwrap();
    // The predecessor cannot be preserved, so the transaction must not replace it.
    fs::create_dir(target.with_extension("json.bak")).unwrap();
    assert!(
        ibis_engine::settings::Owner::open(target.clone())
            .restore(&plan)
            .is_err()
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), "{}");
    assert_eq!(fs::read_to_string(&approvals).unwrap(), denied);
}

#[test]
fn only_a_library_file_name_reaches_the_preview() {
    let (directory, target) = fixture::file();
    let source = directory.path().join("backup.json");
    fixture::edit(&target, |settings| {
        {
            settings.portable.library = Some(LibrarySetting::Bibtex("/invented/private/thesis.bib".into()));
        };
    })
    .unwrap();
    fs::write(&source, backup::export_file(target.clone(), "0.2.8").unwrap()).unwrap();
    fs::write(&target, "{}").unwrap();
    let (_, plan) = backup::prepare(target, source).unwrap();
    let shown: Vec<_> = plan.changes.iter().map(backup::shown).collect();
    assert!(format!("{shown:?}").contains("thesis.bib"));
    assert!(!format!("{shown:?}").contains("private"));
}

#[test]
fn a_missing_settings_file_can_be_restored_from_its_automatic_copy_through_either_client() {
    let (_directory, target) = fixture::file();
    let owner = settings::Owner::open(target.clone());
    owner
        .change(|settings| {
            settings.portable.output = Some("ieee".into());
            settings.device.update_checks = Some(false);
            settings
                .unknown
                .insert("future_choice".into(), serde_json::json!([true, null]));
            Ok::<_, settings::SettingsError>(())
        })
        .unwrap();
    owner
        .change(|settings| {
            settings.portable.larger_text = Some(true);
            Ok::<_, settings::SettingsError>(())
        })
        .unwrap();
    let source = target.with_extension("json.bak");
    let before = fs::read(&source).unwrap();
    fs::remove_file(&target).unwrap();
    assert!(owner.ensure_file().is_err());
    let (_, plan) = backup::prepare(target.clone(), source.clone()).unwrap();
    assert!(plan.notes.contains(&backup::Note::RepairsSettings));
    assert!(!plan.is_empty());
    assert!(!target.exists(), "preview writes nothing");
    owner.restore(&plan).unwrap();
    let loaded = owner.read();
    assert_eq!(loaded.cites().unwrap().portable.output.as_deref(), Some("ieee"));
    assert_eq!(
        loaded.cites().unwrap().unknown["future_choice"],
        serde_json::json!([true, null])
    );
    assert_eq!(fs::read(source).unwrap(), before);
}
