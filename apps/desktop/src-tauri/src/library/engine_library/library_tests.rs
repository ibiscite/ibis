use super::*;
use std::sync::{Arc, Mutex};
#[test]
fn unrelated_json_reports_a_format_error_in_library_status() -> Result<(), Box<dyn std::error::Error>> {
    use crate::library::state::{begin_open, install};
    use crate::library::{Library, LoadRequests, Source};
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("synthetic.json");
    std::fs::write(&path, r#"[{"theme":"dark"}]"#)?;
    let library = Mutex::new(Library::default());
    let requests = LoadRequests::default();
    let (request, _, _) = begin_open(&library, &requests, Source::File(path.clone())).ok_or("begin")?;
    let error = FileLibrary::open(&path).err().ok_or("unrelated JSON opened")?;
    assert_eq!(
        error,
        super::super::LoadError::File(ibis_core::contract::ErrorCode::BadInput.into())
    );
    let status = install(&library, &requests, request, Err(error)).ok_or("status")?;
    assert!(!status.has_snapshot);
    assert_eq!(
        status.error.as_deref(),
        Some("the library file does not contain valid reference data")
    );
    for text in ["", "[]"] {
        std::fs::write(&path, text)?;
        assert!(FileLibrary::open(&path).is_ok(), "an empty JSON library must open");
    }
    Ok(())
}

#[test]
fn deleted_current_file_retains_four_references_and_reports_not_updated() -> Result<(), Box<dyn std::error::Error>> {
    use crate::library::state::{begin_open, install};
    use crate::library::{Library, LoadRequests, Source};
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("synthetic.bib");
    std::fs::write(&path, "@book{a}\n@book{b}\n@book{c}\n@book{d}")?;
    let file = Arc::new(FileLibrary::open(&path).map_err(|_| "file")?);
    let source = Source::File(path.clone());
    let library = Mutex::new(Library {
        source: Some(source.clone()),
        file: Some(file.clone()),
        ..Library::default()
    });
    let requests = LoadRequests::default();
    let (request, _, _) = begin_open(&library, &requests, source).ok_or("begin")?;
    std::fs::remove_file(&path)?;
    let error = file.reload().err().ok_or("deleted file reload succeeded")?;
    let status = install(&library, &requests, request, Err(error)).ok_or("install")?;
    assert_eq!(status.references, 4);
    assert!(status.stale, "a failed current-source read must report not updated");
    Ok(())
}

#[test]
fn a_coherent_engine_update_clears_only_its_own_failed_reload() -> Result<(), Box<dyn std::error::Error>> {
    use crate::library::state::{begin_open, install};
    use crate::library::{Library, LoadRequests, Source};
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("synthetic.bib");
    std::fs::write(&path, "@book{a}")?;
    // No live watcher: its own accepted read would supersede these reloads.
    let engine = Engine::new().map_err(|_| "engine")?.without_file_watches();
    let file = Arc::new(FileLibrary::open_file_with(&path, engine, || {}).map_err(|_| "file")?);
    let source = Source::File(path.clone());
    let library = Mutex::new(Library {
        source: Some(source.clone()),
        file: Some(file.clone()),
        ..Library::default()
    });
    let requests = LoadRequests::default();
    let (request, _, _) = begin_open(&library, &requests, source.clone()).ok_or("begin")?;
    // Not UTF-8, so the reload fails; a damaged entry would only warn.
    std::fs::write(&path, b"@book{\xff}")?;
    let error = file.reload().err().ok_or("partial reload succeeded")?;
    assert!(
        install(&library, &requests, request, Err(error))
            .ok_or("install")?
            .error
            .is_some()
    );
    std::fs::write(&path, "@book{a}\n@book{b}")?;
    file.reload().map_err(|_| "valid reload")?;
    let mut state = library.lock().map_err(|_| "state")?;
    assert!(crate::library::tasks::engine_changed(&mut state, &file));
    assert!(
        state.error.is_none(),
        "successful publication must clear the current source's reload error"
    );
    state.requested = Some(Source::File(directory.path().join("other-missing.bib")));
    state.error = Some(super::super::LoadError::FILE_UNAVAILABLE);
    assert!(crate::library::tasks::engine_changed(&mut state, &file));
    assert!(
        state.error.is_some(),
        "a coherent retained source cannot clear another failed choice"
    );
    Ok(())
}

#[test]
fn repeated_reloads_and_failed_choices_emit_one_engine_notification() -> Result<(), Box<dyn std::error::Error>> {
    use crate::library::state::{begin_open, install};
    use crate::library::{Library, LoadRequests, Source};
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("synthetic.bib");
    std::fs::write(&path, "@book{a}")?;
    let file = Arc::new(FileLibrary::open(&path).map_err(|_| "file")?);
    let source = Source::File(path.clone());
    let library = Mutex::new(Library {
        source: Some(source),
        file: Some(file.clone()),
        ..Library::default()
    });
    let requests = LoadRequests::default();
    let mut observers = vec![];
    for _ in 0..4 {
        file.reload().map_err(|_| "reload")?;
        if let Some(events) = file.observe_once().map_err(|_| "observer")? {
            observers.push(events);
        }
        let (request, _, _) =
            begin_open(&library, &requests, Source::File(directory.path().join("missing.bib"))).ok_or("begin")?;
        install(
            &library,
            &requests,
            request,
            Err(super::super::LoadError::FILE_UNAVAILABLE),
        )
        .ok_or("failed choice")?;
        if let Some(events) = file.observe_once().map_err(|_| "observer")? {
            observers.push(events);
        }
    }
    for observer in &observers {
        while observer.try_recv().is_ok() {}
    }
    std::fs::write(&path, "@book{a}\n@book{b}")?;
    file.reload().map_err(|_| "update")?;
    let delivered = observers
        .iter()
        .filter(|events| events.recv_timeout(std::time::Duration::from_secs(2)).is_ok())
        .count();
    assert_eq!(
        delivered, 1,
        "one source update must reach exactly one native publisher"
    );
    Ok(())
}

#[test]
fn shelf_recents_resolve_current_records_and_source_added_order_without_old_pages()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("synthetic-recents.json");
    std::fs::write(
        &path,
        r#"[{"id":"a","type":"book","title":"Synthetic A","date-added":"2026-10-01"},{"id":"b","type":"book","title":"Synthetic B","date-added":"2026-10-30"},{"id":"c","type":"book","title":"Synthetic C","date-added":"2026-10-15"}]"#,
    )?;
    let file = FileLibrary::open(&path).map_err(|_| "open")?;
    let rows = file.search(
        &SearchQuery {
            text: "Synthetic A".into(),
            interpret_locators: true,
        },
        10,
        None,
        &Cancellation::default(),
    )?;
    let id: ItemRef = rows.results.first().ok_or("row")?.id.clone();
    let recent = serde_json::to_value(file.recents(&[id], true)?)?;
    let rows = recent["results"].as_array().ok_or("results")?;
    assert_eq!(
        rows.iter()
            .map(|row| row["title"].as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        ["Synthetic A", "Synthetic B", "Synthetic C"]
    );
    assert_eq!(rows[0]["recent"], "cited");
    assert_eq!(rows[1]["recent"], "added");
    assert!(rows.iter().all(|row| row.get("locator").is_none()));
    Ok(())
}

#[cfg(unix)]
#[test]
fn remembered_file_choice_uses_the_observation_that_supplied_the_loaded_bytes() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = tempfile::tempdir()?;
    let a = dir.path().join("a.bib");
    let b = dir.path().join("b.bib");
    let selected = dir.path().join("selected.bib");
    std::fs::write(&a, "@book{a}")?;
    std::fs::write(&b, "@book{b}")?;
    std::os::unix::fs::symlink(&a, &selected)?;
    let file = FileLibrary::open_file_with(
        &selected,
        Engine::new().map_err(|_| "engine")?.without_file_watches(),
        || {
            std::fs::remove_file(&selected).unwrap_or_else(|e| panic!("{e}"));
            std::os::unix::fs::symlink(&b, &selected).unwrap_or_else(|e| panic!("{e}"));
        },
    )
    .map_err(|e| e.to_string())?;
    let source = crate::library::Source::File(selected);
    let loaded = Ok(crate::library::load::Loaded::New(Box::new(crate::library::Library {
        source: Some(source.clone()),
        file: Some(std::sync::Arc::new(file)),
        ..Default::default()
    })));
    let chosen = crate::library::tasks::remembered_choice(&source, &loaded).ok_or("choice")?;
    assert_eq!(chosen, ibis_core::settings::LibrarySetting::Bibtex(b.canonicalize()?));
    Ok(())
}
