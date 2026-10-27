#![allow(clippy::unwrap_used)]
#[path = "support/zotero.rs"]
mod fixture;
use ibis_core::contract::{ErrorCode, GoldenCase};
use ibis_engine::{Cancellation, CitationCommitError, Engine, Method, request::ZoteroLibrary};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
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
    let wrote = AtomicBool::new(false);
    assert!(
        engine
            .commit_citation(&prepared, &prepared.result().content_hash, |_| wrote
                .store(true, Ordering::Relaxed))
            .is_err()
    );
    assert!(!wrote.load(Ordering::Relaxed));
    assert_eq!(
        engine.prepare_citation(&case.request, &cancel).unwrap_err().code,
        ErrorCode::SourceUnavailable
    );
    server.pause.store(false, Ordering::Relaxed);
    refresh.join().unwrap().unwrap();
    assert_eq!(engine.snapshot().unwrap().fingerprints(), snapshot.fingerprints());
    assert!(matches!(
        engine.commit_citation(&prepared, &prepared.result().content_hash, |_| ()),
        Err(CitationCommitError::PreviewChanged)
    ));
    let current = engine.prepare_citation(&case.request, &cancel).unwrap();
    assert_eq!(
        engine
            .commit_citation(&current, &current.result().content_hash, |content| content
                .text
                .to_owned())
            .unwrap(),
        "[@Synthetic2026]"
    );
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
