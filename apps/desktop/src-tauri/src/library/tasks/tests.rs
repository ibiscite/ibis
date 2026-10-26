use super::*;

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
                let loaded = candidate(open.source, ibis_core::bibtex::load_str("synthetic", "@misc{key}"));
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
    let mut opened = candidate(source.clone(), ibis_core::bibtex::load_str("synthetic", "@misc{first}"));
    let note = "the library opened but could not be saved for next time: synthetic";
    opened.choice_save_warning = Some(note.to_string());
    let library = Mutex::new(opened);
    let requests = LoadRequests::default();
    let (check, _, _) = begin_check(&library, &requests).unwrap_or_else(|| panic!("quiet check"));
    install(&library, &requests, check, Err(ZOTERO_OFFLINE)).unwrap_or_else(|| panic!("failed check"));

    let (request, retried, reload) = begin_focus_retry(&library, &requests).unwrap_or_else(|| panic!("focus recovery"));
    assert!(reload);
    let changed = || {
        Ok(Loaded::New(Box::new(candidate(
            source.clone(),
            ibis_core::bibtex::load_str("synthetic", "@misc{first}\n@misc{second}"),
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
