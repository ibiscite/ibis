use super::*;
use std::sync::{Arc, Barrier};

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
