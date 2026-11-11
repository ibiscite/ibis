#![allow(clippy::unwrap_used)]
use ibis_core::contract::{KeySyntax, OutputSpec};
use ibis_engine::settings::{self, Settings, writings};

fn output() -> OutputSpec {
    OutputSpec::Key {
        syntax: KeySyntax::Pandoc,
        command: None,
    }
}

#[test]
fn handwritten_writings_need_only_name_format_and_optional_bibliography() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    std::fs::write(
        &path,
        serde_json::json!({
            "schema_version": 2,
            "writings": [{"name":"Atlas", "format":"pandoc"}],
            "writing_apps": {"macos:com.microsoft.word":"Atlas"}
        })
        .to_string(),
    )
    .unwrap();
    let loaded = settings::load(&path);
    assert!(loaded.problem.is_none(), "{:?}", loaded.problem);
    let writing = writings::find(&loaded.settings, "Atlas").unwrap();
    assert_eq!(writing.format, output());
    assert!(writing.bibliography.is_none());
    let renamed = writings::change(&path, |s| writings::rename(s, "Atlas", "Revised Atlas".into())).unwrap();
    assert_eq!(renamed.name, "Revised Atlas");
    let reopened = settings::load(&path).settings;
    assert_eq!(reopened.writing_apps["macos:com.microsoft.word"], "Revised Atlas");
    assert_eq!(writings::find(&reopened, "Revised Atlas").unwrap(), &renamed);
    writings::change(&path, |s| writings::remove(s, "Revised Atlas")).unwrap();
    assert!(settings::load(&path).settings.writing_apps.is_empty());
}

#[test]
fn invalid_or_duplicate_names_leave_all_definitions_unchanged() {
    let mut settings = Settings::default();
    writings::create(&mut settings, "Atlas".into(), output(), None).unwrap();
    let before = settings.clone();
    for name in ["ATLAS", "", "   ", "Atlas\nChapter", "Atlas\u{202e}Chapter"] {
        assert!(writings::create(&mut settings, name.into(), output(), None).is_err());
        assert_eq!(settings, before);
    }
    assert!(
        writings::create(
            &mut settings,
            "Bad format".into(),
            OutputSpec::Csl {
                style: "invented-unavailable-style".into()
            },
            None
        )
        .is_err()
    );
    assert_eq!(settings, before);
}

#[test]
fn failed_transaction_and_failed_io_keep_the_old_writing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let first = writings::change(&path, |s| writings::create(s, "Atlas".into(), output(), None)).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(writings::change(&path, |s| writings::rename(s, &first.name, "".into())).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let blocked = directory.path().join("blocked");
    std::fs::write(&blocked, "a regular file").unwrap();
    assert!(
        writings::change(&blocked.join("settings.json"), |s| writings::create(
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
    let writing = writings::change(&path, |s| {
        writings::create(s, "Atlas".into(), output(), Some(directory.path().join("refs.bib")))
    })
    .unwrap();
    let saved = settings::load(&path).settings;
    assert_eq!(writings::find(&saved, &writing.name).unwrap(), &writing);
    let backup = settings::backup::parse(&settings::backup::export(&saved, "0.0.0").unwrap()).unwrap();
    assert!(backup.settings.writings.is_empty());
    let plan = settings::backup::plan(&saved, &backup, |_| false);
    assert_eq!(plan.restored.writings, saved.writings);
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
    assert!(writings::change(&path, |s| writings::create(s, "Atlas".into(), output(), None)).is_err());
    locked.unlock().unwrap();
    writings::change(&path, |s| writings::create(s, "Atlas".into(), output(), None)).unwrap();
    let saved = settings::load(&path).settings;
    assert_eq!(saved.recent_citations, Some(false));
    assert_eq!(saved.writings.len(), 1);
}

#[test]
fn an_app_binding_limit_cannot_publish_settings_that_the_next_launch_refuses() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    settings::update(&path, |settings| settings.density = Some(settings::Density::Compact)).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(
        settings::update(&path, |settings| {
            settings.writing_apps = (0..201)
                .map(|index| (format!("macos:org.example.writer{index}"), "a".repeat(32)))
                .collect();
        })
        .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(settings::load(&path).problem.is_none());
}

#[test]
fn a_dangling_writing_binding_is_reported_and_never_silently_saved_as_another_format() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    std::fs::write(
        &path,
        serde_json::json!({
            "schema_version": 2,
            "output": "apa",
            "writings": [{"name":"Thesis", "format":"pandoc"}],
            "writing_apps": {"macos:com.microsoft.word":"Missing writing"}
        })
        .to_string(),
    )
    .unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(settings::load(&path).problem.is_some());
    assert!(settings::update(&path, |settings| settings.larger_text = Some(true)).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
