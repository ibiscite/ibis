#![allow(clippy::unwrap_used)]
#[path = "support/zotero.rs"]
mod fixture;
use ibis_core::contract::{ErrorCode, ErrorReason, GoldenCase};
use ibis_engine::{Cancellation, CitationCommitError, Engine, Method, request::ZoteroLibrary};
use serde_json::json;
use std::sync::atomic::Ordering;

#[test]
fn unaccepted_zotero_reload_keeps_search_and_health_private() {
    let server = fixture::Server::new();
    let engine = Engine::with_zotero_port(server.port).unwrap();
    let cancel = Cancellation::default();
    engine
        .dispatch(Method::Libraries, &json!({"action":"open-zotero"}), &cancel)
        .unwrap();
    let before = engine.snapshot().unwrap();
    let health = serde_json::to_value(engine.libraries().unwrap()).unwrap();
    for fails in [false, true] {
        let cancel = Cancellation::default();
        *server.title.lock().unwrap() = "Obsolete atlas".into();
        server.sharing_off.store(fails, Ordering::SeqCst);
        let prepared = engine.prepare_zotero_reload(true, None, None, &cancel).unwrap();
        assert_eq!(engine.snapshot().unwrap().fingerprints(), before.fingerprints());
        assert_eq!(serde_json::to_value(engine.libraries().unwrap()).unwrap(), health);
        // The newer choice cancels the operation whose candidate arrives late.
        let directory = tempfile::tempdir().unwrap();
        assert!(
            engine
                .open_file_observed(&directory.path().join("unreadable.bib"), &cancel)
                .is_err()
        );
        cancel.cancel();
        assert_eq!(
            engine.accept_zotero_reload(prepared).unwrap_err().code,
            ErrorCode::Cancelled
        );
        assert_eq!(engine.snapshot().unwrap().fingerprints(), before.fingerprints());
        assert_eq!(serde_json::to_value(engine.libraries().unwrap()).unwrap(), health);
    }
    server.sharing_off.store(false, Ordering::SeqCst);
    let prepared = engine.prepare_zotero_reload(true, None, None, &cancel).unwrap();
    engine.accept_zotero_reload(prepared).unwrap();
    assert_ne!(engine.snapshot().unwrap().fingerprints(), before.fingerprints());
    assert!(
        engine
            .libraries()
            .unwrap()
            .libraries
            .iter()
            .all(|info| info.refresh_error.is_none())
    );
}

#[test]
fn private_zotero_reload_refuses_copy_after_a_detected_switch_before_acceptance() {
    let server = fixture::Server::new();
    let engine = Engine::with_zotero_port(server.port).unwrap();
    let cancel = Cancellation::default();
    engine
        .dispatch(Method::Libraries, &json!({"action":"open-zotero"}), &cancel)
        .unwrap();
    let snapshot = engine.snapshot().unwrap();
    let health = serde_json::to_value(engine.libraries().unwrap()).unwrap();
    let mut case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap();
    case.request.items[0].item = snapshot.objects()[0].object.id().clone();
    case.request.items[0].expected_cite = Some(snapshot.objects()[0].fingerprints.cite.clone());
    let citation = engine.prepare_citation(&case.request, &cancel).unwrap();
    server.instance.store(2, Ordering::SeqCst);
    server.pause.store(true, Ordering::SeqCst);
    let refreshing = engine.clone();
    let refresh =
        std::thread::spawn(move || refreshing.prepare_zotero_reload(false, None, None, &Cancellation::default()));
    server.wait_for_payload();
    // Release the endpoint even if the regression assertion below fails.
    let copy = engine.recite(&citation, &cancel);
    let preview = engine.prepare_citation(&case.request, &cancel);
    server.pause.store(false, Ordering::SeqCst);
    let reload = refresh.join().unwrap().unwrap();
    assert!(copy.is_err(), "a detected source switch must not copy old references");
    assert_eq!(preview.unwrap_err().code, ErrorCode::SourceUnavailable);
    assert_eq!(engine.snapshot().unwrap().fingerprints(), snapshot.fingerprints());
    assert_eq!(serde_json::to_value(engine.libraries().unwrap()).unwrap(), health);
    assert!(
        engine.recite(&citation, &cancel).is_err(),
        "preparation alone cannot make copying ready"
    );
    engine.accept_zotero_reload(reload).unwrap();
    assert!(matches!(
        engine.recite(&citation, &cancel),
        Err(CitationCommitError::PreviewChanged)
    ));
    let current = engine.prepare_citation(&case.request, &cancel).unwrap();
    assert_eq!(engine.recite(&current, &cancel).unwrap().text, "[@Synthetic2026]");
}

#[test]
fn private_zotero_failure_retires_copy_without_publishing_health_and_a_retry_recovers() {
    let server = fixture::Server::new();
    let engine = Engine::with_zotero_port(server.port).unwrap();
    let cancel = Cancellation::default();
    engine
        .dispatch(Method::Libraries, &json!({"action":"open-zotero"}), &cancel)
        .unwrap();
    let snapshot = engine.snapshot().unwrap();
    let health = serde_json::to_value(engine.libraries().unwrap()).unwrap();
    let mut case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap();
    case.request.items[0].item = snapshot.objects()[0].object.id().clone();
    let citation = engine.prepare_citation(&case.request, &cancel).unwrap();
    server.sharing_off.store(true, Ordering::SeqCst);
    let reload = engine.prepare_zotero_reload(false, None, None, &cancel).unwrap();
    assert!(
        engine.recite(&citation, &cancel).is_err(),
        "a failed read cannot keep a copy ready"
    );
    assert_eq!(serde_json::to_value(engine.libraries().unwrap()).unwrap(), health);
    assert_eq!(
        engine.accept_zotero_reload(reload).unwrap_err().reason,
        Some(ErrorReason::ZoteroSharingOff)
    );
    server.sharing_off.store(false, Ordering::SeqCst);
    let reload = engine.prepare_zotero_reload(false, None, None, &cancel).unwrap();
    engine.accept_zotero_reload(reload).unwrap();
    let current = engine.prepare_citation(&case.request, &cancel).unwrap();
    assert_eq!(engine.recite(&current, &cancel).unwrap().text, "[@Synthetic2026]");
    assert!(
        engine.recite(&citation, &cancel).is_err(),
        "the earlier receipt stays retired"
    );
}

#[test]
fn an_obsolete_private_zotero_read_cannot_retire_the_current_copy() {
    let server = fixture::Server::new();
    let engine = Engine::with_zotero_port(server.port).unwrap();
    let cancel = Cancellation::default();
    engine
        .dispatch(Method::Libraries, &json!({"action":"open-zotero"}), &cancel)
        .unwrap();
    let snapshot = engine.snapshot().unwrap();
    let mut case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap();
    case.request.items[0].item = snapshot.objects()[0].object.id().clone();
    let citation = engine.prepare_citation(&case.request, &cancel).unwrap();
    // An unchanged check needs no retirement permission and leaves the shown
    // citation available throughout its private preparation and acceptance.
    let unchanged = engine
        .prepare_zotero_reload_guarded(false, None, None, &cancel, |_retire| Err(ErrorCode::Cancelled.into()))
        .unwrap();
    assert_eq!(engine.recite(&citation, &cancel).unwrap().text, "[@Synthetic2026]");
    engine.accept_zotero_reload(unchanged).unwrap();
    let health = serde_json::to_value(engine.libraries().unwrap()).unwrap();
    for sharing_off in [false, true] {
        server.sharing_off.store(sharing_off, Ordering::SeqCst);
        let obsolete =
            engine.prepare_zotero_reload_guarded(true, None, None, &cancel, |_retire| Err(ErrorCode::Cancelled.into()));
        assert!(matches!(obsolete, Err(failure) if failure.code == ErrorCode::Cancelled));
        assert_eq!(engine.recite(&citation, &cancel).unwrap().text, "[@Synthetic2026]");
        assert_eq!(engine.snapshot().unwrap().fingerprints(), snapshot.fingerprints());
        assert_eq!(serde_json::to_value(engine.libraries().unwrap()).unwrap(), health);
    }
}

#[test]
fn copy_waits_for_a_coherent_read_after_a_detected_switch_and_old_previews_stay_retired() {
    let server = fixture::Server::new();
    let engine = Engine::with_zotero_port(server.port).unwrap();
    let cancel = Cancellation::default();
    engine
        .dispatch(Method::Libraries, &json!({"action":"open-zotero"}), &cancel)
        .unwrap();
    let snapshot = engine.snapshot().unwrap();
    let mut case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap();
    case.request.items[0].item = snapshot.objects()[0].object.id().clone();
    case.request.items[0].expected_cite = Some(snapshot.objects()[0].fingerprints.cite.clone());
    let prepared = engine.prepare_citation(&case.request, &cancel).unwrap();
    server.instance.store(2, Ordering::Relaxed);
    server.pause.store(true, Ordering::Relaxed);
    let refreshing = engine.clone();
    let refresh = std::thread::spawn(move || {
        refreshing.dispatch(
            Method::Libraries,
            &json!({"action":"open-zotero"}),
            &Cancellation::default(),
        )
    });
    server.wait_for_payload();
    assert!(engine.recite(&prepared, &cancel).is_err());
    assert_eq!(
        engine.prepare_citation(&case.request, &cancel).unwrap_err().code,
        ErrorCode::SourceUnavailable
    );
    server.pause.store(false, Ordering::Relaxed);
    refresh.join().unwrap().unwrap();
    assert_eq!(engine.snapshot().unwrap().fingerprints(), snapshot.fingerprints());
    assert!(matches!(
        engine.recite(&prepared, &cancel),
        Err(CitationCommitError::PreviewChanged)
    ));
    let current = engine.prepare_citation(&case.request, &cancel).unwrap();
    assert_eq!(engine.recite(&current, &cancel).unwrap().text, "[@Synthetic2026]");
}

#[test]
fn chosen_group_libraries_publish_separately_and_a_new_choice_replaces_the_old() {
    let server = fixture::Server::new();
    let engine = Engine::with_zotero_port(server.port).unwrap();
    let cancel = Cancellation::default();
    let open = |libraries: serde_json::Value| {
        engine.dispatch(
            Method::Libraries,
            &json!({"action":"open-zotero","libraries":libraries}),
            &cancel,
        )
    };
    let chosen = || {
        let mut chosen: Vec<_> = engine
            .libraries()
            .unwrap()
            .libraries
            .into_iter()
            .map(|library| library.zotero)
            .collect();
        chosen.sort_by_key(|library| format!("{library:?}"));
        chosen
    };
    open(json!(["personal", {"group": 7}])).unwrap();
    assert_eq!(
        chosen(),
        vec![Some(ZoteroLibrary::Group(7)), Some(ZoteroLibrary::Personal)]
    );
    // The fixture serves the same item and key in both libraries: two
    // references, and the shared key names neither rather than guessing.
    let snapshot = engine.snapshot().unwrap();
    assert_eq!(snapshot.objects().len(), 2);
    assert_ne!(snapshot.objects()[0].object.id(), snapshot.objects()[1].object.id());
    assert_eq!(
        engine
            .dispatch(
                Method::Show,
                &json!({"item":{"by":"key","value":"Synthetic2026"}}),
                &cancel
            )
            .unwrap_err()
            .code,
        ErrorCode::BadInput
    );
    let mut case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap();
    let mut cite = |object: &ibis_core::contract::SnapshotObject| {
        case.request.items[0].item = object.object.id().clone();
        case.request.items[0].expected_cite = Some(object.fingerprints.cite.clone());
        engine.cite(&case.request, &cancel).unwrap().text
    };
    assert_eq!(cite(&snapshot.objects()[1]), "", "a conflicting key is refused");

    open(json!([{"group": 7}])).unwrap();
    assert_eq!(chosen(), vec![Some(ZoteroLibrary::Group(7))]);
    let only = engine.snapshot().unwrap();
    assert_eq!(only.objects().len(), 1);
    assert_eq!(cite(&only.objects()[0]), "[@Synthetic2026]");
    let group = engine.snapshot().unwrap().fingerprints().clone();

    for invalid in [json!([]), json!([{"group": 0}]), json!(["personal", "personal"])] {
        assert_eq!(open(invalid).unwrap_err().code, ErrorCode::BadInput);
    }
    // A group that does not exist is reported, never read as another library,
    // and the previous choice stays published until it is read again.
    assert_eq!(open(json!([{"group": 99}])).unwrap_err().code, ErrorCode::NotFound);
    assert_eq!(chosen(), vec![Some(ZoteroLibrary::Group(7))]);
    assert_eq!(engine.snapshot().unwrap().fingerprints(), &group);
    engine
        .dispatch(Method::Libraries, &json!({"action":"open-zotero"}), &cancel)
        .unwrap();
    assert_eq!(chosen(), vec![Some(ZoteroLibrary::Group(7))]);
    assert_eq!(engine.snapshot().unwrap().fingerprints(), &group);
}

#[test]
fn a_zotero_that_stops_refusing_is_read_without_any_other_change() {
    let server = fixture::Server::new();
    server.sharing_off.store(true, Ordering::Relaxed);
    let engine = Engine::with_zotero_port(server.port).unwrap();
    let cancel = Cancellation::default();
    let open = json!({"action":"open-zotero","libraries":["personal"]});
    let refused = engine.dispatch(Method::Libraries, &open, &cancel).unwrap_err();
    assert_eq!(refused.code, ErrorCode::SourceUnavailable);
    assert_eq!(refused.reason, Some(ErrorReason::ZoteroSharingOff));
    assert!(
        engine.libraries().unwrap().libraries.is_empty(),
        "a refusal publishes nothing"
    );

    server.sharing_off.store(false, Ordering::Relaxed);
    engine.dispatch(Method::Libraries, &open, &cancel).unwrap();
    assert_eq!(engine.libraries().unwrap().libraries[0].objects, 1);
}

#[test]
fn turning_sharing_off_later_keeps_the_last_read_and_says_why() {
    let server = fixture::Server::new();
    let engine = Engine::with_zotero_port(server.port).unwrap();
    let cancel = Cancellation::default();
    engine
        .dispatch(Method::Libraries, &json!({"action":"open-zotero"}), &cancel)
        .unwrap();
    server.sharing_off.store(true, Ordering::Relaxed);
    // Reloading everything reports each library's own health, not one failure.
    engine
        .dispatch(Method::Libraries, &json!({"action":"reload"}), &cancel)
        .unwrap();
    let info = engine.libraries().unwrap();
    assert_eq!(info.libraries[0].objects, 1, "the last good references stay");
    assert_eq!(info.libraries[0].refresh_error, Some(ErrorCode::SourceUnavailable));
    assert_eq!(info.libraries[0].refresh_reason, Some(ErrorReason::ZoteroSharingOff));

    server.sharing_off.store(false, Ordering::Relaxed);
    engine
        .dispatch(Method::Libraries, &json!({"action":"reload"}), &cancel)
        .unwrap();
    let info = engine.libraries().unwrap();
    assert_eq!(info.libraries[0].refresh_error, None);
    assert_eq!(info.libraries[0].refresh_reason, None, "recovery leaves no old reason");
}

#[test]
fn group_names_come_from_a_running_zotero_and_a_refusal_says_why() {
    let server = fixture::Server::new();
    let engine = Engine::with_zotero_port(server.port).unwrap();
    let cancel = Cancellation::default();
    let ask = json!({"action":"zotero-groups"});
    server.sharing_off.store(true, Ordering::Relaxed);
    assert_eq!(
        engine.dispatch(Method::Libraries, &ask, &cancel).unwrap_err().reason,
        Some(ErrorReason::ZoteroSharingOff)
    );
}

#[test]
fn a_closed_zotero_with_nothing_read_lists_no_groups_and_starts_no_read() {
    let free = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = free.local_addr().unwrap().port();
    drop(free);
    let engine = Engine::with_zotero_port(port).unwrap();
    let failure = engine
        .dispatch(
            Method::Libraries,
            &json!({"action":"zotero-groups"}),
            &Cancellation::default(),
        )
        .unwrap_err();
    assert_eq!(failure.code, ErrorCode::SourceUnavailable);
    assert!(engine.libraries().unwrap().libraries.is_empty());
}

#[test]
fn zotero_markers_name_each_item_in_the_library_it_was_read_from() {
    let server = fixture::Server::new();
    let engine = Engine::with_zotero_port(server.port).unwrap();
    let cancel = Cancellation::default();
    engine
        .dispatch(
            Method::Libraries,
            &json!({"action":"open-zotero","libraries":["personal", {"group": 7}]}),
            &cancel,
        )
        .unwrap();
    let snapshot = engine.snapshot().unwrap();
    let libraries = engine.libraries().unwrap().libraries;
    let marker = |zotero: ZoteroLibrary| {
        let library = libraries.iter().find(|info| info.zotero == Some(zotero)).unwrap();
        let object = snapshot
            .objects()
            .iter()
            .find(|object| object.object.id().library == library.id)
            .unwrap();
        let request = serde_json::from_value(json!({
            "items":[{"item":object.object.id(),"expected_cite":object.fingerprints.cite}],
            "output":{"kind":"key","syntax":"scannable-cite"},
        }))
        .unwrap();
        engine.cite(&request, &cancel).unwrap().text
    };
    // The same item and key in both libraries: each marker names its own.
    assert_eq!(
        marker(ZoteroLibrary::Personal),
        "{ | Invented atlas, 2026 | | | zu:0:SYNTH234 }"
    );
    assert_eq!(
        marker(ZoteroLibrary::Group(7)),
        "{ | Invented atlas, 2026 | | | zg:7:SYNTH234 }"
    );
}
