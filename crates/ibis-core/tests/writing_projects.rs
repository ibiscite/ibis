#![allow(clippy::unwrap_used)]
use ibis_core::contract::{KeySyntax, OutputSpec};
use ibis_core::settings::{self, Settings, writing_projects as projects};

fn output() -> OutputSpec {
    OutputSpec::Key {
        syntax: KeySyntax::Pandoc,
        command: None,
    }
}

#[test]
fn renaming_keeps_identity_and_old_context_is_not_the_new_definition() {
    let mut settings = Settings::default();
    let first = projects::create(&mut settings, "Field Guide".into(), output(), None).unwrap();
    let revision = first.revision();
    let renamed = projects::rename(&mut settings, &first.id, "Atlas".into()).unwrap();
    assert_eq!(renamed.id, first.id);
    assert_ne!(renamed.revision(), revision);
    let back = projects::rename(&mut settings, &first.id, "Field Guide".into()).unwrap();
    assert_ne!(
        back.revision(),
        revision,
        "returning to old fields cannot revive an old edit owner"
    );
    projects::rename(&mut settings, &first.id, "Atlas".into()).unwrap();
    assert_eq!(projects::find(&settings, &first.id).unwrap().name, "Atlas");
    let removed = projects::remove(&mut settings, &first.id).unwrap();
    assert_eq!(removed.id, first.id);
    assert!(projects::find(&settings, &first.id).is_err());
}

#[test]
fn invalid_or_duplicate_names_leave_all_definitions_unchanged() {
    let mut settings = Settings::default();
    projects::create(&mut settings, "Atlas".into(), output(), None).unwrap();
    let before = settings.clone();
    for name in ["ATLAS", "", "   ", "Atlas\nChapter", "Atlas\u{202e}Chapter"] {
        assert!(projects::create(&mut settings, name.into(), output(), None).is_err());
        assert_eq!(settings, before);
    }
}

#[test]
fn failed_transaction_and_failed_io_keep_the_old_project() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let first = projects::change(&path, |s| projects::create(s, "Atlas".into(), output(), None)).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(projects::change(&path, |s| projects::rename(s, &first.id, "".into())).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let blocked = directory.path().join("blocked");
    std::fs::write(&blocked, "a regular file").unwrap();
    assert!(
        projects::change(&blocked.join("settings.json"), |s| projects::create(
            s,
            "Other".into(),
            output(),
            None
        ))
        .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn reopen_and_backup_keep_definitions_on_their_computer() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let project = projects::change(&path, |s| {
        projects::create(s, "Atlas".into(), output(), Some(directory.path().join("refs.bib")))
    })
    .unwrap();
    let saved = settings::load(&path).settings;
    assert_eq!(projects::find(&saved, &project.id).unwrap(), &project);
    let backup = settings::backup::parse(&settings::backup::export(&saved, "0.0.0").unwrap()).unwrap();
    assert!(backup.settings.writing_projects.is_empty());
    let plan = settings::backup::plan(&saved, &backup, |_| false);
    assert_eq!(plan.restored.writing_projects, saved.writing_projects);
}

#[test]
fn another_writer_refuses_immediately_and_retry_preserves_its_changes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    settings::update(&path, |s| s.recent_citations = Some(false)).unwrap();
    let locked = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path.with_extension("json.lock"))
        .unwrap();
    locked.try_lock().unwrap();
    assert!(projects::change(&path, |s| projects::create(s, "Atlas".into(), output(), None)).is_err());
    locked.unlock().unwrap();
    projects::change(&path, |s| projects::create(s, "Atlas".into(), output(), None)).unwrap();
    let saved = settings::load(&path).settings;
    assert_eq!(saved.recent_citations, Some(false));
    assert_eq!(saved.writing_projects.len(), 1);
}

#[test]
fn an_app_binding_limit_cannot_publish_settings_that_the_next_launch_refuses() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    settings::update(&path, |settings| settings.density = Some(settings::Density::Compact)).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(
        settings::update(&path, |settings| {
            settings.writing_project_apps = (0..201)
                .map(|index| (format!("macos:org.example.writer{index}"), "a".repeat(32)))
                .collect();
        })
        .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(settings::load(&path).problem.is_none());
}
