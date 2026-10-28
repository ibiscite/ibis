use super::*;
use std::sync::{Arc, Barrier};

#[test]
fn replacing_or_invalidating_a_preview_cancels_its_engine_work() -> Result<(), Box<dyn std::error::Error>> {
    let file = FileLibrary::from_engine(Engine::new().map_err(|_| "engine")?);
    let first = file.reserve_preview();
    let running = file.preview_cancellation(first).map_err(|_| "token")?;
    let replacement = file.reserve_preview();
    let current = file
        .preview_cancellation(replacement)
        .map_err(|_| "replacement token")?;
    assert!(running.is_cancelled());
    assert!(!current.is_cancelled());
    let late = file.reserve_preview_with(running.clone());
    assert!(file.preview_cancellation(late).is_err());
    assert!(
        !current.is_cancelled(),
        "a late cancelled command cannot retire its replacement"
    );
    let request = serde_json::from_value(json!({
        "items": [], "output": {"kind":"csl", "style":"apa"}
    }))?;
    assert_eq!(
        file.engine
            .prepare_citation(&request, &running)
            .err()
            .ok_or("cancelled preview ran")?
            .code,
        ibis_core::contract::ErrorCode::Cancelled
    );
    file.invalidate();
    assert!(current.is_cancelled());
    Ok(())
}

#[test]
fn a_superseded_preview_stops_before_reading_its_request() -> Result<(), Box<dyn std::error::Error>> {
    let file = FileLibrary::from_engine(Engine::new().map_err(|_| "engine")?);
    let older = file.reserve_preview();
    file.reserve_preview();
    let error = file
        .preview(
            older,
            vec![FileSelection {
                id: Value::Null,
                cite: String::new(),
                locator: None,
            }],
            KeyFormat::Pandoc,
            Some(CitationStyle::Apa),
        )
        .err()
        .ok_or("obsolete preview ran")?;
    assert_eq!(
        error,
        CommandError::from("A newer citation preview replaced this request.")
    );
    Ok(())
}

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
    assert_eq!(error, super::super::LoadError::InvalidData);
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
fn an_older_blocking_worker_finishing_last_cannot_retire_the_newer_preview() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("synthetic.bib");
    std::fs::write(
        &path,
        "@book{a,title={Synthetic},author={Researcher, Synthetic},year={2026}}",
    )?;
    let file = Arc::new(FileLibrary::open(&path).map_err(|_| "file")?);
    let rows = serde_json::to_value(
        file.search(
            &serde_json::from_value(json!({"text":"synthetic","interpret_locators":false}))?,
            10,
            None,
            &ibis_engine::Cancellation::default(),
        )
        .map_err(|_| "search")?,
    )?;
    let selected = json!({"id":rows["results"][0]["id"],"cite":rows["results"][0]["cite"],"locator":null});
    // Native command entry reserves first; its blocking worker deliberately starts last.
    let older = file.reserve_preview();
    let started = Barrier::new(2);
    let release = Barrier::new(2);
    std::thread::scope(|scope| -> Result<(), Box<dyn std::error::Error>> {
        let old = scope.spawn(|| {
            started.wait();
            release.wait();
            file.preview(
                older,
                vec![serde_json::from_value(selected.clone()).unwrap_or_else(|e| panic!("{e}"))],
                KeyFormat::Pandoc,
                Some(CitationStyle::Apa),
            )
        });
        started.wait();
        let newer = file.reserve_preview();
        let ready = file
            .preview(
                newer,
                vec![serde_json::from_value(selected.clone())?],
                KeyFormat::Pandoc,
                None,
            )
            .map_err(|_| "newer preview")?;
        release.wait();
        assert!(
            old.join().map_err(|_| "old worker")?.is_err(),
            "older request must not replace the current native slot"
        );
        let copied = file
            .copy(&ready.ticket, &ready.content_hash, |content| {
                assert_eq!(content.text, "[@a]");
                Ok(())
            })
            .map_err(|_| "newer copy")?;
        assert_eq!(copied.text, "[@a]");
        Ok(())
    })?;
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
    let file = Arc::new(FileLibrary::open(&path).map_err(|_| "file")?);
    let source = Source::File(path.clone());
    let library = Mutex::new(Library {
        source: Some(source.clone()),
        file: Some(file.clone()),
        ..Library::default()
    });
    let requests = LoadRequests::default();
    let (request, _, _) = begin_open(&library, &requests, source.clone()).ok_or("begin")?;
    std::fs::write(&path, "@book{partial,")?;
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
    state.error = Some(ibis_core::source::SourceError::Unavailable.into());
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
            Err(ibis_core::source::SourceError::Unavailable.into()),
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
fn same_author_year_notice_survives_native_preview_and_copy() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("synthetic.bib");
    std::fs::write(
        &path,
        "@book{a,title={Synthetic first},author={Researcher, Synthetic},year={2026}}\n@book{b,title={Synthetic second},author={Researcher, Synthetic},year={2026}}",
    )?;
    let file = FileLibrary::open(&path).map_err(|_| "file")?;
    let rows = file
        .search(
            &serde_json::from_value(json!({"text":"synthetic","interpret_locators":false}))?,
            10,
            None,
            &ibis_engine::Cancellation::default(),
        )
        .map_err(|_| "search")?;
    let rows = serde_json::to_value(rows)?;
    let selected = rows["results"]
        .as_array()
        .ok_or("results")?
        .iter()
        .map(|row| serde_json::from_value(json!({"id":row["id"],"cite":row["cite"],"locator":null})))
        .collect::<Result<Vec<FileSelection>, _>>()?;
    assert_eq!(selected.len(), 2);
    let preview = file
        .preview(
            file.reserve_preview(),
            selected,
            KeyFormat::Pandoc,
            Some(CitationStyle::Apa),
        )
        .map_err(|e| e.to_string())?;
    assert_eq!(
        preview.warnings,
        ["Year suffixes require the whole document; Ibis leaves them out."]
    );
    let copied = file
        .copy(&preview.ticket, &preview.content_hash, |content| {
            assert_eq!(content.text, preview.text);
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    assert_eq!(copied.warnings, preview.warnings);
    Ok(())
}

#[test]
fn conflicting_keys_explain_refusal_without_a_copy_receipt() -> Result<(), Box<dyn std::error::Error>> {
    use ibis_core::contract::{CitationKey, CitationKeySource, GoldenCase, Snapshot};
    let case: GoldenCase = serde_json::from_str(include_str!("../../../../../../fixtures/cases/02-pandoc.json"))?;
    let mut object = case.library.objects()[0].object.clone();
    let LibraryObject::Record(record) = &mut object else {
        return Err("record".into());
    };
    record.citation_keys = vec![
        CitationKey {
            value: "First2026".into(),
            source: CitationKeySource::Zotero,
        },
        CitationKey {
            value: "Other2026".into(),
            source: CitationKeySource::Extra,
        },
    ];
    let file = FileLibrary::from_engine(Engine::from_snapshot(Arc::new(Snapshot::new(vec![object])?)));
    let snapshot = file.engine.snapshot().map_err(|_| "snapshot")?;
    let record = &snapshot.objects()[0];
    let selected = json!({"id":record.object.id(),"cite":record.fingerprints.cite,"locator":null});
    let error = match file.preview(
        file.reserve_preview(),
        vec![serde_json::from_value(selected.clone())?],
        KeyFormat::Pandoc,
        None,
    ) {
        Err(error) => error,
        Ok(_) => return Err("conflicting keys produced a preview receipt".into()),
    };
    let error = serde_json::to_value(error)?;
    assert_eq!(error["code"], "failed");
    assert_eq!(
        error["message"],
        "These references have conflicting citation keys. Resolve them in your library or choose a formatted style."
    );
    let writes = std::cell::Cell::new(0);
    assert!(
        file.copy("refused", "refused", |_| {
            writes.set(writes.get() + 1);
            Ok(())
        })
        .is_err()
    );
    assert_eq!(writes.get(), 0);
    // Conflicting keys do not prevent a formatted citation of this record.
    assert!(
        file.preview(
            file.reserve_preview(),
            vec![serde_json::from_value(selected)?],
            KeyFormat::Pandoc,
            Some(CitationStyle::Apa)
        )
        .is_ok()
    );
    Ok(())
}

#[test]
fn a_key_another_record_also_has_is_named_and_not_copied() -> Result<(), Box<dyn std::error::Error>> {
    use ibis_core::contract::{GoldenCase, Snapshot};
    let case: GoldenCase = serde_json::from_str(include_str!("../../../../../../fixtures/cases/02-pandoc.json"))?;
    let first = case.library.objects()[0].object.clone();
    let mut second = first.clone();
    let LibraryObject::Record(record) = &mut second else {
        return Err("record".into());
    };
    record.id.key = "work-2".into();
    let file = FileLibrary::from_engine(Engine::from_snapshot(Arc::new(Snapshot::new(vec![first, second])?)));
    let snapshot = file.engine.snapshot().map_err(|_| "snapshot")?;
    let record = &snapshot.objects()[0];
    let selected = json!({"id":record.object.id(),"cite":record.fingerprints.cite,"locator":null});
    for format in [KeyFormat::Pandoc, KeyFormat::Cite, KeyFormat::Keys] {
        let error = file
            .preview(
                file.reserve_preview(),
                vec![serde_json::from_value(selected.clone())?],
                format,
                None,
            )
            .err()
            .ok_or("a shared key produced a preview receipt")?;
        // The picker names the key from the code, never from the message text.
        assert_eq!(
            error,
            CommandError::SharedCitationKey {
                key: "Example2026".into()
            }
        );
    }
    Ok(())
}
