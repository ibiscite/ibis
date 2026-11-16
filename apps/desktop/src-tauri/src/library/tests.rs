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
fn an_unrelated_live_settings_edit_keeps_the_shown_citation_copyable() -> Result<(), Box<dyn std::error::Error>> {
    use tauri::{Listener, Manager};
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("settings.json");
    std::fs::write(&path, br#"{"output":"apa"}"#)?;
    let settings_file = SettingsFile::new(path.clone());
    let app = tauri::test::mock_builder()
        .manage(settings_file.clone())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))?;
    let handle = app.handle().clone();
    let notices = settings_file.clone();
    app.manage(Owner::new(
        Arc::new(settings_file),
        || {
            Ok(Engine::new()?
                .without_file_watches()
                .with_formatting(engine_library::FORMATTING.clone()))
        },
        move |mut status| {
            use tauri::Emitter;
            status.settings_notice = notices.notice();
            let _ = handle.emit("library:changed", status);
        },
    ));
    let bib = directory.path().join("synthetic.bib");
    std::fs::write(
        &bib,
        "@book{example,title={Synthetic reference},author={Example, Ada},year={2026}}",
    )?;
    open(&app.state::<Owner>(), bib);
    let file = app.state::<Owner>().current().ok_or("library")?;
    let rows = file
        .search(
            &serde_json::from_value(serde_json::json!({"text":"example","interpret_locators":false}))?,
            10,
            None,
            &ibis_engine::Cancellation::default(),
        )
        .map_err(|error| error.to_string())?;
    let preview = crate::test_citations::preview(
        &file.engine,
        false,
        vec![serde_json::from_value(
            serde_json::json!({"id":rows.results[0].id,"cite":rows.results[0].cite}),
        )?],
        "apa",
    )?;
    // A library notification makes the picker search/preview again. Even
    // reserving that replacement retires the ticket Enter is about to copy.
    let refreshed = file.clone();
    let listener = app.listen("library:changed", move |_| {
        refreshed
            .engine
            .citations()
            .invalidate(ibis_engine::citations::SessionId::PICKER);
    });
    let edited = br#"{"shortcut":"Control+Alt+F12"}"#;
    std::fs::write(&path, edited)?;
    refresh_settings_notice(app.handle(), &app.state::<SettingsFile>());
    let mut clipboard = String::new();
    crate::test_citations::copy(&file.engine, &preview.ticket, &preview.content_hash, |content| {
        clipboard = content.text.to_owned();
        Ok(())
    })
    .map_err(|error| error.to_string())?;
    assert_eq!(clipboard, "(Example, 2026)");
    assert_eq!(std::fs::read(&path)?, edited);
    app.unlisten(listener);

    let (sent, notices) = std::sync::mpsc::channel();
    app.listen("library:changed", move |event| {
        let _ = sent.send(event.payload().to_owned());
    });
    std::fs::write(&path, b"{ still editing")?;
    refresh_settings_notice(app.handle(), &app.state::<SettingsFile>());
    let notice: serde_json::Value = serde_json::from_str(&notices.try_recv()?)?;
    assert!(
        notice["settings_notice"]
            .as_str()
            .is_some_and(|notice| notice.contains("not being saved"))
    );
    refresh_settings_notice(app.handle(), &app.state::<SettingsFile>());
    assert!(notices.try_recv().is_err());
    std::fs::write(&path, edited)?;
    refresh_settings_notice(app.handle(), &app.state::<SettingsFile>());
    let notice: serde_json::Value = serde_json::from_str(&notices.try_recv()?)?;
    assert!(notice["settings_notice"].is_null());
    Ok(())
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
fn a_preview_refused_during_a_quiet_check_waits_and_refuses_again_after_it() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let server = zotero_fixture::Server::new();
    let port = server.port;
    let library = Owner::new(
        Arc::new(SettingsFile::new(directory.path().join("settings.json"))),
        move || Engine::with_zotero_port(port),
        |_| {},
    );
    open(
        &library,
        Source::Zotero(ibis_engine::settings::ZoteroLibrary::User.into()),
    );
    let app = tauri::test::mock_builder()
        .manage(library)
        .manage(crate::project::ProjectBibliography::default())
        .manage(crate::editor_endpoint::Pending::default())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))?;
    let preview = |request| {
        tauri::async_runtime::block_on(crate::copy::preview_file_citation(
            Vec::new(),
            "ieee".into(),
            String::new(),
            PreviewIntent {
                keep_order: false,
                page: 1,
                request,
                cancel: false,
                context: Some(app.state::<crate::project::ProjectBibliography>().version()),
            },
            app.state(),
            app.state(),
            app.state(),
        ))
        .err()
    };
    server.instance.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    server.pause.store(true, std::sync::atomic::Ordering::SeqCst);
    app.state::<Owner>().submit(Intent::Check);
    server.wait_for_payload();
    assert_eq!(preview(1), Some(CommandError::LibraryLoading));
    server.pause.store(false, std::sync::atomic::Ordering::SeqCst);
    assert!(app.state::<Owner>().wait_idle(Duration::from_secs(10)));
    assert_eq!(
        preview(2).map(|error| error.problem()),
        Some(ibis_core::contract::CitationError::EmptySelection.problem())
    );
    Ok(())
}

#[test]
fn settings_notice_belongs_to_settings_and_a_successful_choice_save_refreshes_it()
-> Result<(), Box<dyn std::error::Error>> {
    use ibis_engine::library::ChoiceStore;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("settings.json");
    std::fs::write(&path, "{ still editing")?;
    let settings = SettingsFile::new(path.clone());
    assert!(settings.notice().is_some());
    let owner = owner(settings.clone());
    let bib = directory.path().join("invented.bib");
    std::fs::write(&bib, "@book{invented}")?;
    open(&owner, bib);
    assert!(settings.notice().is_some());
    owner.submit(Intent::Close);
    assert!(settings.notice().is_some());
    std::fs::write(&path, "{}")?;
    ChoiceStore::save(&settings, &mut |saved| saved.portable.larger_text = Some(true))?;
    assert!(settings.notice().is_none());
    assert_eq!(crate::settings_fixture::read(&path).portable.larger_text, Some(true));
    Ok(())
}

#[test]
fn reference_counts_exclude_attachments_in_library_status() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("invented.bib");
    std::fs::write(directory.path().join("invented.pdf"), b"invented attachment")?;
    std::fs::write(&path, "@book{one,title={Invented rivers},file={invented.pdf}}")?;
    let owner = owner(SettingsFile::new(directory.path().join("settings.json")));
    open(&owner, path.clone());
    let current = owner.current().ok_or("current")?;
    assert_eq!(
        current.engine.libraries().map_err(|_| "libraries")?.libraries[0].objects,
        2
    );
    let loaded = owner.status();
    assert_eq!(loaded.references, 1);
    assert_eq!(loaded.source_counts.values().copied().collect::<Vec<_>>(), [1]);
    assert_eq!(crate::tray::library_line(&loaded), "invented.bib · 1 reference");
    assert_eq!(serde_json::to_value(&loaded)?["references"], 1);
    std::fs::remove_file(&path)?;
    owner.submit(Intent::Reload);
    assert!(owner.wait_idle(Duration::from_secs(10)));
    let retained = owner.status();
    assert_eq!(retained.references, 1);
    assert_eq!(retained.source_counts, loaded.source_counts);
    assert!(retained.stale && retained.kept_references);
    Ok(())
}

#[test]
fn native_presentation_facts_do_not_change_the_picker_payload() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("invented.bib");
    std::fs::write(&path, "@book{kept}")?;
    let owner = owner(SettingsFile::new(directory.path().join("settings.json")));
    open(&owner, path.clone());
    std::fs::remove_file(&path)?;
    owner.submit(Intent::Reload);
    assert!(owner.wait_idle(Duration::from_secs(10)));
    let current = owner.status();
    assert!(current.error_reason.is_some() && current.kept_references);
    let payload = serde_json::to_value(&current)?;
    let expected = [
        "state",
        "revision",
        "source_generation",
        "source_is_requested",
        "zotero",
        "zotero_libraries",
        "has_snapshot",
        "name",
        "snapshot_name",
        "references",
        "source_counts",
        "warnings",
        "reading",
        "live_updates_off",
        "error",
        "problem",
        "failure",
        "note",
        "settings_notice",
        "snapshot_zotero",
        "stale",
        "updated_at",
        "checked_at",
    ];
    let keys = payload.as_object().ok_or("status object")?;
    assert_eq!(keys.len(), expected.len());
    assert!(expected.iter().all(|key| keys.contains_key(*key)));
    assert_eq!(payload["source_generation"], owner.view().generation.to_string());
    Ok(())
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
