use super::engine_library::FileLibrary;
use super::*;
use ibis_engine::Engine;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(crate) fn owner(settings: SettingsFile) -> Owner {
    Owner::new(
        Arc::new(settings),
        || {
            Ok(Engine::new()?
                .without_file_watches()
                .with_formatting(engine_library::FORMATTING.clone()))
        },
        |_| {},
    )
}

pub(crate) fn open(owner: &Owner, source: impl Into<Source>) {
    // Independent test owners share the engine's process-wide read budget.
    static OPEN: Mutex<()> = Mutex::new(());
    let _open = OPEN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    owner.submit(Intent::Open {
        source: source.into(),
        save: Save::Never,
        profile: None,
    });
    assert!(owner.wait_idle(Duration::from_secs(10)), "library read finished");
}

#[test]
fn a_dropped_file_of_any_other_kind_is_not_a_library() {
    for name in ["notes.md", "paper.pdf", "image.png", "archive.zip", "script.command"] {
        assert!(!accepts_library_file(std::path::Path::new(name)), "{name}");
    }
}

#[test]
fn the_chooser_accepts_each_library_file_format_the_engine_reads() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let bibtex = "@misc{synthetic,title={Example reference}}";
    let csl = r#"[{"id":"synthetic","type":"book","title":"Example reference"}]"#;
    let ris = "TY  - BOOK\nTI  - Example reference\nER  - \n";
    for extension in ibis_engine::library_file_extensions() {
        let contents = match extension {
            "bib" | "bibtex" | "biblatex" => bibtex,
            "json" | "csljson" => csl,
            "ris" | "txt" => ris,
            "csv" => "type,title\nbook,Example reference\n",
            other => panic!("give .{other} a sample library here"),
        };
        let path = directory.path().join(format!("synthetic.{extension}"));
        // A dropped `.txt` is too likely to be notes to replace the library.
        assert_eq!(accepts_library_file(&path), extension != "txt", "drop and .{extension}");
        std::fs::write(&path, contents).unwrap_or_else(|e| panic!("{e}"));
        let file = ibis_engine::library::Current::open(
            Engine::new().unwrap_or_else(|_| panic!("engine")),
            Source::File(path.clone()),
            None,
            &ibis_engine::Cancellation::default(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let reply = file
            .search(
                &super::search::SearchQuery {
                    text: "Example".into(),
                    interpret_locators: false,
                },
                20,
                None,
                &ibis_engine::Cancellation::default(),
            )
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(reply.results.len(), 1, ".{extension} must load a reference");
    }
}

#[test]
fn startup_parses_fresh_and_can_cite_immediately() {
    use tauri::Manager;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invented.bib");
    for key in ["first", "reopened"] {
        std::fs::write(&path, format!("@book{{{key},title={{Synthetic stars}}}}")).unwrap();
        let library = owner(SettingsFile::new(dir.path().join("settings.json")));
        let app = tauri::test::mock_builder()
            .manage(library)
            .manage(crate::project::ProjectBibliography::default())
            .manage(crate::editor_endpoint::Pending::default())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let library = app.state::<Owner>();
        open(&library, path.clone());
        let ready = library.status();
        assert_eq!(ready.state, LibraryState::Loaded);
        assert!(ready.checked_at.is_some() && ready.has_snapshot && ready.error.is_none());
        let active = library.current().unwrap();
        let query = serde_json::from_value(serde_json::json!({"text":"synthetic","interpret_locators":false})).unwrap();
        let rows = serde_json::to_value(
            active
                .search(&query, 10, None, &ibis_engine::Cancellation::default())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(rows["results"].as_array().unwrap().len(), 1);
        let selection = serde_json::from_value(serde_json::json!({
            "id":rows["results"][0]["id"],"cite":rows["results"][0]["cite"]
        }))
        .unwrap();
        let preview = tauri::async_runtime::block_on(crate::copy::preview_file_citation(
            vec![selection],
            "pandoc".into(),
            active.revision(),
            PreviewIntent {
                keep_order: false,
                page: 1,
                request: 1,
                cancel: false,
                context: Some(app.state::<crate::project::ProjectBibliography>().version()),
            },
            app.state(),
            app.state(),
            app.state(),
        ))
        .unwrap();
        let expected = format!("[@{key}]");
        assert_eq!(preview.text, expected);
        let mut written = String::new();
        crate::test_citations::copy(&active.engine, &preview.ticket, &preview.content_hash, |content| {
            written = content.text.to_owned();
            Ok(())
        })
        .unwrap();
        assert_eq!(written, expected);
    }
}

#[test]
fn a_failed_choice_does_not_hide_failure_or_recovery_of_the_retained_file() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("retained.json");
    let original = r#"[{"id":"retained","type":"book","title":"Invented almanac"}]"#;
    std::fs::write(&path, original)?;
    let owner = owner(SettingsFile::new(directory.path().join("settings.json")));
    open(&owner, path.clone());
    open(&owner, directory.path().join("missing.bib"));
    let rejected = owner.status();
    let current = owner.current().ok_or("current")?;
    let reload = || {
        let _ = current.engine.dispatch(
            ibis_engine::Method::Libraries,
            &serde_json::json!({"action":"reload"}),
            &ibis_engine::Cancellation::default(),
        );
    };
    std::fs::write(&path, "[{")?;
    reload();
    let failed = owner.status();
    assert!(failed.stale, "the references being searched are now stale too");
    assert_eq!(failed.name.as_deref(), Some("missing.bib"));
    assert_eq!(failed.error, rejected.error, "keep the failed choice visible");
    assert!(failed.note.is_some(), "also report the retained source's failure");
    assert_eq!(
        crate::tray::library_line(&failed),
        "missing.bib was moved or is unavailable"
    );
    std::fs::write(&path, original)?;
    reload();
    let recovered = owner.status();
    assert!(!recovered.stale);
    assert!(recovered.note.is_none());
    assert_eq!(recovered.error, rejected.error);
    Ok(())
}
