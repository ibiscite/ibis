use super::super::state::candidate;
use super::*;

#[test]
fn remembered_choice_uses_the_read_identity_even_if_the_selected_path_disappears()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().canonicalize()?;
    let path = root.join("invented.bib");
    std::fs::write(&path, "@misc{invented}")?;
    let canonical = path.canonicalize()?;
    // Keep Unix's dot-only difference; Windows needs a non-verbatim input to preserve `.`.
    let selected_root = if cfg!(windows) { directory.path() } else { &root };
    let source = Source::File(selected_root.join(".").join("invented.bib"));
    let library = Mutex::new(Library::default());
    let requests = LoadRequests::default();
    let (request, _, _) = begin_open(&library, &requests, source.clone()).ok_or("startup")?;
    let loaded = load_source(
        source.clone(),
        false,
        None,
        &ibis_engine::Cancellation::default(),
        || false,
    );
    std::fs::remove_file(&path)?;
    let chosen = remembered_choice(&source, &loaded).ok_or("prepared source choice")?;
    let LibrarySetting::Bibtex(chosen_path) = &chosen else {
        return Err("prepared file identity".into());
    };
    assert_eq!(chosen_path.as_os_str(), canonical.as_os_str());
    #[cfg(not(windows))]
    assert_eq!(
        source.setting(),
        Some(chosen.clone()),
        "PathBuf equality hides the different spelling"
    );
    assert!(
        OpenIntent::Read.saves_choice(&source, &chosen),
        "startup must retain the prepared identity"
    );
    assert!(OpenIntent::Remember.saves_choice(&source, &chosen));
    assert!(!OpenIntent::Read.saves_choice(&Source::File(canonical), &chosen));
    assert!(!OpenIntent::Refresh.saves_choice(&source, &chosen));
    std::fs::write(&path, "@misc{invented}")?;
    OpenIntent::Read
        .install(&library, &requests, request, loaded)
        .ok_or("accepted")?;
    Ok(())
}

#[test]
fn replacing_a_library_disconnects_its_idle_observer_but_keeps_it_for_the_same_engine()
-> Result<(), Box<dyn std::error::Error>> {
    use super::super::state::install;
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    let directory = tempfile::tempdir()?;
    let path = directory.path().join("synthetic.bib");
    let library = Mutex::new(candidate(Source::File(path), "@misc{synthetic}"));
    let (file, events) = observe_current(&library).ok_or("observer")?;
    let requests = LoadRequests::default();
    install(
        &library,
        &requests,
        0,
        Ok(Library {
            file: Some(Arc::clone(&file)),
            ..Library::default()
        }),
    )
    .ok_or("same engine")?;
    assert!(matches!(events.try_recv(), Err(mpsc::TryRecvError::Empty)));
    assert!(observe_current(&library).is_none(), "same engine has only one worker");

    let (done, finished) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let _retained_engine = file;
        done.send(events.recv().is_err()).unwrap_or_else(|e| panic!("{e}"));
    });
    install(&library, &requests, 0, Ok(Library::default())).ok_or("replace")?;
    assert!(finished.recv_timeout(Duration::from_secs(2))?);
    worker.join().map_err(|_| "observer worker")?;
    Ok(())
}

#[test]
fn the_chooser_accepts_each_library_file_format_the_engine_reads() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    for (extension, contents) in [
        ("bib", "@misc{synthetic,title={Example reference}}"),
        ("bibtex", "@misc{synthetic,title={Example reference}}"),
        (
            "json",
            r#"[{"id":"synthetic","type":"book","title":"Example reference"}]"#,
        ),
        (
            "csljson",
            r#"[{"id":"synthetic","type":"book","title":"Example reference"}]"#,
        ),
    ] {
        assert!(
            LIBRARY_FILE_EXTENSIONS.contains(&extension),
            "chooser hides .{extension}"
        );
        let path = directory.path().join(format!("synthetic.{extension}"));
        std::fs::write(&path, contents).unwrap_or_else(|e| panic!("{e}"));
        let file = super::super::engine_file::FileLibrary::open(&path).unwrap_or_else(|e| panic!("{e}"));
        let reply = file
            .search(
                &super::super::search::SearchQuery {
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
fn three_rapid_choices_run_only_first_and_last_loads_and_the_last_wins() {
    use std::sync::mpsc;
    use std::time::Duration;

    let library = Mutex::new(Library::default());
    let requests = LoadRequests::default();
    let choose = |id| {
        let source = Source::Zotero(ZoteroLibrary::Group(id).into());
        let (request, reload, _) =
            begin_open(&library, &requests, source.clone()).unwrap_or_else(|| panic!("choice starts"));
        OpenRequest {
            cancel: ibis_engine::Cancellation::default(),
            request,
            source,
            reload,
            intent: OpenIntent::Read,
            #[cfg(debug_assertions)]
            started: std::time::Instant::now(),
        }
    };
    assert!(requests.enqueue(choose(1)), "the first choice starts one worker");
    let (started, active) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let ran = std::thread::scope(|scope| {
        let (requests, library) = (&requests, &library);
        let worker = scope.spawn(move || {
            let mut ran = Vec::new();
            while let Some(open) = requests.next_open() {
                ran.push(open.request);
                if ran.len() == 1 {
                    started.send(()).unwrap_or_else(|e| panic!("{e}"));
                    resume
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap_or_else(|e| panic!("{e}"));
                }
                let loaded = candidate(open.source, "@misc{key}");
                let installed = open
                    .intent
                    .install(library, requests, open.request, Ok(Loaded::New(Box::new(loaded))));
                assert_eq!(installed.is_some(), open.request == 3, "stale work cannot install");
            }
            ran
        });
        active
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|e| panic!("{e}"));
        let second = choose(2);
        assert!(
            !requests.enqueue(second.clone()),
            "the second choice waits for the active load"
        );
        assert!(
            !requests.enqueue(choose(3)),
            "a pending choice cannot start another worker"
        );
        assert!(
            !requests.enqueue(second),
            "a delayed older enqueue cannot replace the latest choice"
        );
        release.send(()).unwrap_or_else(|e| panic!("{e}"));
        worker.join().unwrap_or_else(|e| panic!("{e:?}"))
    });
    assert_eq!(ran, [1, 3], "the middle choice must never load");
    assert_eq!(
        library.lock().unwrap_or_else(|e| panic!("{e}")).source,
        Some(Source::Zotero(ZoteroLibrary::Group(3).into()))
    );
    assert!(requests.enqueue(choose(4)), "an idle worker can restart");
}

#[test]
fn automatic_focus_recovery_keeps_the_unsaved_choice_but_explicit_reload_clears_it() {
    use super::super::error::ZOTERO_OFFLINE;
    use super::super::state::install;

    let source = Source::Zotero(ZoteroLibrary::Group(7).into());
    let mut opened = candidate(source.clone(), "@misc{first}");
    let note = "the library opened but could not be saved for next time: synthetic";
    opened.choice_save_warning = Some(note.to_string());
    let library = Mutex::new(opened);
    let requests = LoadRequests::default();
    let (check, _) = begin_check(&library, &requests).unwrap_or_else(|| panic!("quiet check"));
    install(&library, &requests, check, Err(ZOTERO_OFFLINE)).unwrap_or_else(|| panic!("failed check"));

    let (request, retried, reload) = begin_focus_retry(&library, &requests).unwrap_or_else(|| panic!("focus recovery"));
    assert!(reload);
    let changed = || {
        Ok(Loaded::New(Box::new(candidate(
            source.clone(),
            "@misc{first}\n@misc{second}",
        ))))
    };
    let recovered = OpenIntent::retry(reload)
        .install(&library, &requests, request, changed())
        .unwrap_or_else(|| panic!("recovery is current"));
    assert_eq!(retried, source);
    assert_eq!(recovered.references, 2);
    assert!(recovered.choice_not_saved);
    assert_eq!(recovered.note.as_deref(), Some(note));

    let (request, _, _) = begin_open(&library, &requests, source.clone()).unwrap_or_else(|| panic!("reload"));
    let reloaded = OpenIntent::Read
        .install(&library, &requests, request, changed())
        .unwrap_or_else(|| panic!("explicit reload is current"));
    assert_eq!(reloaded.references, 2);
    assert!(!reloaded.choice_not_saved);
    assert!(reloaded.note.is_none(), "explicit Reload retains its existing behavior");
}

#[test]
fn an_explicit_reread_that_finds_nothing_changed_clears_the_unsaved_choice_warning() {
    let source = Source::File(std::path::PathBuf::from("synthetic.bib"));
    for intent in [OpenIntent::Read, OpenIntent::Remember] {
        let mut opened = candidate(source.clone(), "@misc{first}");
        opened.choice_save_warning = Some("the library opened but could not be saved for next time".into());
        let library = Mutex::new(opened);
        let requests = LoadRequests::default();
        let (request, _, _) = begin_open(&library, &requests, source.clone()).unwrap_or_else(|| panic!("reread"));
        let reread = intent
            .install(&library, &requests, request, Ok(Loaded::Unchanged))
            .unwrap_or_else(|| panic!("reread is current"));
        assert!(!reread.choice_not_saved);
        assert!(reread.note.is_none());
    }
    // A quiet check keeps it: it never saves the choice.
    let mut opened = candidate(source.clone(), "@misc{first}");
    opened.choice_save_warning = Some("the library opened but could not be saved for next time".into());
    let library = Mutex::new(opened);
    let requests = LoadRequests::default();
    let (request, _, _) = begin_open(&library, &requests, source).unwrap_or_else(|| panic!("check"));
    let checked = OpenIntent::Refresh
        .install(&library, &requests, request, Ok(Loaded::Unchanged))
        .unwrap_or_else(|| panic!("check is current"));
    assert!(checked.choice_not_saved);
}

#[cfg(debug_assertions)]
#[test]
fn load_phase_json_contains_only_fixed_numeric_fields_for_all_outcomes() {
    for (accepted, load_ok) in [(1, 1), (1, 0), (0, 1), (0, 0)] {
        let phases = super::LoadPhases {
            load_ms: 1.25,
            accepted,
            load_ok,
            ..Default::default()
        };
        let value = serde_json::to_value(phases).unwrap_or_else(|e| panic!("{e}"));
        let fields = value.as_object().unwrap_or_else(|| panic!("numeric object"));
        let mut expected = vec![
            "load_ms",
            "load_finished_ms",
            "install_ms",
            "publish_ms",
            "completed_ms",
            "accepted",
            "load_ok",
        ];
        expected.sort_unstable();
        let mut actual = fields.keys().map(String::as_str).collect::<Vec<_>>();
        actual.sort_unstable();
        assert_eq!(actual, expected);
        assert!(
            fields
                .values()
                .all(|value| value.as_f64().is_some_and(|n| n.is_finite() && n >= 0.0))
        );
        assert_eq!(fields["accepted"].as_u64(), Some(u64::from(accepted)));
        assert_eq!(fields["load_ok"].as_u64(), Some(u64::from(load_ok)));
    }
}

#[test]
fn status_emission_is_serialized_but_native_menu_work_holds_no_library_lock() {
    let library = Mutex::new(Library::default());
    publish_current_with(
        &library,
        |_| assert!(library.try_lock().is_err()),
        || assert!(library.try_lock().is_ok()),
    );
}

#[test]
fn obsolete_unchanged_reload_keeps_the_newer_choices_save_warning() {
    let warning = "the library opened but could not be saved for next time";
    let library = Mutex::new(Library {
        choice_save_warning: Some(warning.into()),
        ..Library::default()
    });
    let requests = LoadRequests::default();
    requests.0.store(2, Ordering::SeqCst); // The newer choice has completed, with a failed save.
    assert!(
        OpenIntent::Read
            .install(&library, &requests, 1, Ok(Loaded::Unchanged))
            .is_none()
    );
    let current = library.lock().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(current.choice_save_warning.as_deref(), Some(warning));
}

#[test]
fn settings_io_releases_the_library_and_discards_obsolete_feedback() {
    let library = Mutex::new(Library::default());
    let requests = LoadRequests::default();
    assert!(!update_notices(
        &library,
        |_| requests.0.load(Ordering::SeqCst) == 0,
        |feedback| {
            assert!(
                library.try_lock().is_ok(),
                "settings reads and flushes must not block library commands"
            );
            feedback.choice_save_warning = Some("obsolete save failure".into());
            requests.0.store(1, Ordering::SeqCst);
        }
    ));
    let current = library.lock().unwrap_or_else(|e| panic!("{e}"));
    assert!(current.choice_save_warning.is_none());
}

#[test]
fn reload_and_engine_event_record_one_update_in_either_completion_order() -> Result<(), Box<dyn std::error::Error>> {
    for event_first in [false, true] {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("synthetic.bib");
        std::fs::write(&path, "@misc{first}")?;
        let library = Mutex::new(super::super::load::load_library(path.clone(), false).map_err(|e| e.to_string())?);
        let requests = LoadRequests::default();
        let (request, _, _) = begin_open(&library, &requests, path.clone()).ok_or("reload")?;
        let file = library.lock().map_err(|_| "library")?.file.clone().ok_or("file")?;
        std::fs::write(&path, "@misc{first}\n@misc{second}")?;
        let loaded = load_source(
            Source::File(path),
            true,
            Some(file.clone()),
            &ibis_engine::Cancellation::default(),
            || false,
        )
        .map_err(|e| e.to_string())?;
        let observed = std::time::UNIX_EPOCH + std::time::Duration::from_secs(42);
        let record_event = || {
            let _subscription = observe_current(&library).unwrap_or_else(|| panic!("subscribe"));
            assert!(
                library.lock().unwrap_or_else(|e| panic!("{e}")).changed_at.is_some(),
                "subscription must reconcile the missed update"
            );
        };
        if event_first {
            record_event();
            library.lock().map_err(|_| "library")?.changed_at = Some(observed);
        }
        install_loaded(&library, &requests, request, Ok(loaded)).ok_or("install")?;
        if !event_first {
            library.lock().map_err(|_| "library")?.changed_at = Some(observed);
            record_event();
        }
        assert_eq!(
            library.lock().map_err(|_| "library")?.changed_at,
            Some(observed),
            "the same snapshot must not be stamped twice (event first: {event_first})"
        );
    }
    Ok(())
}
