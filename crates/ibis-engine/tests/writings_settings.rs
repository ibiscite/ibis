#![allow(clippy::unwrap_used)]
use ibis_core::contract::{KeySyntax, OutputSpec};
use ibis_engine::settings::Purpose;
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
    let writing = writings::find(loaded.usable(Purpose::Interface).unwrap(), "Atlas").unwrap();
    assert_eq!(writing.format, output());
    assert!(writing.bibliography.is_none());
    let renamed = settings::Owner::open(path.clone())
        .change(|s| writings::rename(s, "Atlas", "Revised Atlas".into()))
        .map(|(_, writing)| writing)
        .unwrap();
    assert_eq!(renamed.name, "Revised Atlas");
    let reopened = settings::load(&path).usable(Purpose::Interface).unwrap().clone();
    assert_eq!(
        reopened.device.writing_apps["macos:com.microsoft.word"],
        "Revised Atlas"
    );
    assert_eq!(writings::find(&reopened, "Revised Atlas").unwrap(), &renamed);
    settings::Owner::open(path.clone())
        .change(|s| writings::remove(s, "Revised Atlas"))
        .map(|(_, writing)| writing)
        .unwrap();
    assert!(
        settings::load(&path)
            .usable(Purpose::Interface)
            .unwrap()
            .clone()
            .device
            .writing_apps
            .is_empty()
    );
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
    let first = settings::Owner::open(path.clone())
        .change(|s| writings::create(s, "Atlas".into(), output(), None))
        .map(|(_, writing)| writing)
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(
        settings::Owner::open(path.clone())
            .change(|s| writings::rename(s, &first.name, "".into()))
            .map(|(_, writing)| writing)
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let blocked = directory.path().join("blocked");
    std::fs::write(&blocked, "a regular file").unwrap();
    assert!(
        settings::Owner::open(blocked.join("settings.json"))
            .change(|s| writings::create(s, "Other".into(), output(), None))
            .map(|(_, writing)| writing)
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn another_process_refuses_immediately_with_recovery_and_retry_preserves_its_changes() {
    use std::io::{BufRead, Read, Write};
    use std::process::{Child, Command, Stdio};
    use std::time::Duration;
    const CHILD: &str = "IBIS_SETTINGS_EXTERNAL_WRITER";
    if let Some(path) = std::env::var_os(CHILD) {
        let lock = std::fs::OpenOptions::new().read(true).write(true).open(path).unwrap();
        lock.try_lock().unwrap();
        println!("ibis-writer-ready");
        std::io::stdout().flush().unwrap();
        std::io::stdin().read_to_end(&mut Vec::new()).unwrap();
        return;
    }
    struct Running(Child);
    impl Drop for Running {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    ibis_engine::settings::Owner::open(path.clone())
        .change(|s| {
            s.portable.recent_citations = Some(false);
            Ok::<_, ibis_engine::settings::SettingsError>(())
        })
        .map(|(settings, ())| settings)
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    let mut child = Running(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "another_process_refuses_immediately_with_recovery_and_retry_preserves_its_changes",
                "--nocapture",
            ])
            .env(CHILD, path.with_extension("json.lock"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let child_output = child.0.stdout.take().unwrap();
    let (ready, waiting) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(child_output).lines().map_while(Result::ok) {
            if line == "ibis-writer-ready" {
                ready.send(()).unwrap();
            }
        }
    });
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    let (finished, result) = std::sync::mpsc::channel();
    let saving = path.clone();
    let save = std::thread::spawn(move || {
        finished
            .send(
                ibis_engine::settings::Owner::open(saving.clone())
                    .change(|current| {
                        *current = Settings::default();
                        Ok::<_, ibis_engine::settings::SettingsError>(())
                    })
                    .map(|_| ()),
            )
            .unwrap();
    });
    // The child still holds the lock: a blocking cross-process lock cannot pass.
    let error = result.recv_timeout(Duration::from_secs(5)).unwrap().unwrap_err();
    save.join().unwrap();
    assert!(matches!(error, settings::SettingsError::Busy));
    assert_eq!(
        ibis_engine::settings_backup::settings_problem(&error),
        "settings are being saved by another Ibis"
    );
    assert_eq!(
        ibis_engine::settings_backup::settings_message(&error),
        "settings are being saved by another Ibis; wait for it to finish, then try again"
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let backup = settings::backup::parse(&settings::backup::export(&Settings::default(), "0.0.0").unwrap()).unwrap();
    let plan = settings::backup::plan(
        &settings::load(&path).usable(Purpose::Interface).unwrap().clone(),
        &backup,
        |_| true,
    );
    let refused = ibis_engine::settings::Owner::open(path.clone())
        .restore(&plan)
        .unwrap_err();
    assert_eq!(
        ibis_engine::settings_backup::problem(&refused),
        settings::backup::BackupProblem::Busy
    );
    assert_eq!(
        ibis_engine::settings_backup::message(&refused),
        "settings are being saved by another Ibis; wait for it to finish, then try again. Nothing was changed."
    );
    assert!(
        settings::Owner::open(path.clone())
            .change(|s| writings::create(s, "Atlas".into(), output(), None))
            .map(|(_, writing)| writing)
            .is_err()
    );
    drop(child.0.stdin.take());
    assert!(child.0.wait().unwrap().success());
    // Once the external writer finishes, the same caller can save normally.
    settings::Owner::open(path.clone())
        .change(|s| writings::create(s, "Atlas".into(), output(), None))
        .map(|(_, writing)| writing)
        .unwrap();
    let saved = settings::load(&path).usable(Purpose::Interface).unwrap().clone();
    assert_eq!(saved.portable.recent_citations, Some(false));
    assert_eq!(saved.device.writings.len(), 1);
}

#[test]
fn an_app_binding_limit_cannot_publish_settings_that_the_next_launch_refuses() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    ibis_engine::settings::Owner::open(path.clone())
        .change(|settings| {
            settings.portable.density = Some(settings::Density::Compact);
            Ok::<_, ibis_engine::settings::SettingsError>(())
        })
        .map(|(settings, ())| settings)
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(
        ibis_engine::settings::Owner::open(path.clone())
            .change(|settings| {
                {
                    settings.device.writing_apps = (0..201)
                        .map(|index| (format!("macos:org.example.writer{index}"), "a".repeat(32)))
                        .collect();
                };
                Ok::<_, ibis_engine::settings::SettingsError>(())
            })
            .map(|(settings, ())| settings)
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
    assert!(
        ibis_engine::settings::Owner::open(path.clone())
            .change(|settings| {
                settings.portable.larger_text = Some(true);
                Ok::<_, ibis_engine::settings::SettingsError>(())
            })
            .map(|(settings, ())| settings)
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
