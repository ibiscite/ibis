#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use ibis_engine::{Cancellation, Engine, Method, request::*};
use serde_json::json;
use std::sync::Arc;

fn case() -> GoldenCase {
    serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap()
}
#[test]
fn typed_methods_and_dispatch_share_one_snapshot() {
    let case = case();
    let engine = Engine::from_snapshot(Arc::new(case.library.clone()));
    let cancel = Cancellation::default();
    let search = SearchRequest {
        query: "signals".into(),
        limit: 10,
        libraries: None,
    };
    let typed = engine.search(&search, &cancel).unwrap();
    let dispatched = engine
        .dispatch(Method::Search, &serde_json::to_value(search).unwrap(), &cancel)
        .unwrap();
    assert_eq!(serde_json::to_value(typed).unwrap(), dispatched.data);
    let shown = engine
        .show(&ShowRequest {
            item: ItemSelector::Key("Example2026".into()),
        })
        .unwrap();
    assert_eq!(shown, *case.library.objects()[0]);
    assert_eq!(
        engine.cite(&case.request, &cancel).unwrap(),
        *case.expected.data().unwrap()
    );
    assert_eq!(engine.libraries().unwrap().libraries.len(), 1);
    assert_eq!(
        engine
            .dispatch(Method::Call, &json!({"method":"call","params":null}), &cancel)
            .unwrap_err()
            .code,
        ErrorCode::BadInput
    );
    assert_eq!(
        engine
            .dispatch(Method::Search, &json!({"query":"x","private":"payload"}), &cancel)
            .unwrap_err()
            .code,
        ErrorCode::BadInput
    );
    cancel.cancel();
    assert_eq!(
        engine
            .search(
                &SearchRequest {
                    query: "x".into(),
                    limit: 1,
                    libraries: None
                },
                &cancel
            )
            .unwrap_err()
            .code,
        ErrorCode::Cancelled
    );
}

#[test]
fn refresh_keeps_old_snapshots_and_refuses_ambiguous_keys() {
    let case = case();
    let engine = Engine::from_snapshot(Arc::new(case.library.clone()));
    let old = engine.snapshot().unwrap();
    let LibraryObject::Record(mut record) = old.objects()[0].object.clone() else {
        panic!("record")
    };
    let library = LibraryId::file(b"another synthetic library");
    record.id.library = library.clone();
    let second = Arc::new(Snapshot::new(vec![LibraryObject::Record(record)]).unwrap());
    engine.publish_library(library.clone(), second).unwrap();
    assert_eq!(old.objects().len(), 1);
    assert_eq!(engine.snapshot().unwrap().objects().len(), 2);
    assert_eq!(
        engine
            .show(&ShowRequest {
                item: ItemSelector::Key("Example2026".into())
            })
            .unwrap_err()
            .code,
        ErrorCode::BadInput
    );
    engine
        .publish_library(library, Arc::new(Snapshot::new(vec![]).unwrap()))
        .unwrap();
    assert!(
        engine
            .show(&ShowRequest {
                item: ItemSelector::Key("Example2026".into())
            })
            .is_ok()
    );
}

#[test]
fn human_key_arguments_are_resolved_only_inside_the_engine() {
    let case = case();
    let engine = Engine::from_snapshot(Arc::new(case.library));
    let request = KeyCitationRequest {
        keys: vec!["Example2026".into()],
        output: case.request.output,
        form: CitationForm::Normal,
        output_locale: None,
    };
    let result = engine.cite_keys(&request, &Cancellation::default()).unwrap();
    assert_eq!(result.text, "[@Example2026]");
    let mut missing = request;
    missing.keys = vec!["missing".into()];
    assert_eq!(
        engine.cite_keys(&missing, &Cancellation::default()).unwrap_err().code,
        ErrorCode::NotFound
    );
}

#[test]
fn library_changes_invalidate_subscribers_without_mutating_previous_snapshots() {
    let case = case();
    let engine = Engine::new().unwrap();
    let events = engine.subscribe().unwrap();
    let old = engine.snapshot().unwrap();
    let snapshot = Arc::new(case.library);
    let library = snapshot.objects()[0].object.id().library.clone();
    engine.publish_library(library.clone(), snapshot.clone()).unwrap();
    events.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
    assert_eq!(engine.libraries().unwrap().libraries[0].id, library);
    assert_eq!(engine.snapshot().unwrap().fingerprints(), snapshot.fingerprints());
    assert!(old.objects().is_empty());
    drop(events);
    assert!(engine.subscribe().is_ok());
}

#[test]
fn a_named_missing_library_is_not_silently_an_empty_search() {
    let engine = Engine::new().unwrap();
    let mut request = SearchRequest {
        query: "synthetic".into(),
        limit: 10,
        libraries: Some(vec![LibraryId::file(b"missing synthetic scope")]),
    };
    assert_eq!(
        engine.search(&request, &Cancellation::default()).unwrap_err().code,
        ErrorCode::NotFound
    );
    request.libraries = Some(vec![]);
    assert!(
        engine
            .search(&request, &Cancellation::default())
            .unwrap()
            .hits
            .is_empty()
    );
}

#[test]
fn a_slow_subscriber_wakes_to_current_state_without_blocking_publications() {
    let engine = Engine::new().unwrap();
    let events = engine.subscribe().unwrap();
    let newest = Arc::new(case().library);
    let library = newest.objects()[0].object.id().library.clone();
    let publishing = engine.clone();
    let latest = newest.clone();
    let (completed, done) = std::sync::mpsc::channel();
    let publisher = std::thread::spawn(move || {
        for _ in 0..40 {
            publishing
                .publish_library(library.clone(), Arc::new(Snapshot::new(vec![]).unwrap()))
                .unwrap();
        }
        publishing.publish_library(library, latest).unwrap();
        completed.send(()).unwrap();
    });
    // The publisher must finish while the subscriber leaves its single wake queued.
    done.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
    publisher.join().unwrap();
    events.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
    assert_eq!(engine.snapshot().unwrap().fingerprints(), newest.fingerprints());
    assert!(matches!(events.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)));
    engine
        .publish_library(
            newest.objects()[0].object.id().library.clone(),
            Arc::new(Snapshot::new(vec![]).unwrap()),
        )
        .unwrap();
    events.try_recv().unwrap();
    assert!(engine.snapshot().unwrap().objects().is_empty());
}

#[test]
fn library_subscriptions_are_limited_and_drop_releases_a_slot() {
    let engine = Engine::new().unwrap();
    let mut events: Vec<_> = (0..64).map(|_| engine.subscribe().unwrap()).collect();
    assert!(matches!(engine.subscribe(), Err(error) if error.code == ErrorCode::ResourceLimit));
    drop(events.pop());
    events.push(engine.subscribe().unwrap());
    assert!(matches!(engine.subscribe(), Err(error) if error.code == ErrorCode::ResourceLimit));
}

#[test]
fn typed_search_refuses_oversized_results_before_public_object_cloning() {
    let case = case();
    let LibraryObject::Record(mut first) = case.library.objects()[0].object.clone() else {
        panic!("record")
    };
    first.csl.insert(
        "annote".into(),
        json!("x".repeat(ibis_engine::protocol::MAX_RESULT_BYTES / 2 + 1024)),
    );
    let mut second = first.clone();
    second.id.key = "record:second-large".into();
    second.citation_keys[0].value = "Other2026".into();
    let engine = Engine::from_snapshot(Arc::new(
        Snapshot::new(vec![LibraryObject::Record(first), LibraryObject::Record(second)]).unwrap(),
    ));
    let mut request = SearchRequest {
        query: "synthetic".into(),
        limit: 2,
        libraries: None,
    };
    let Err(error) = engine.search(&request, &Cancellation::default()) else {
        panic!("oversized typed search was accepted")
    };
    assert_eq!(error.code, ErrorCode::ResourceLimit);
    request.limit = 1;
    assert_eq!(engine.search(&request, &Cancellation::default()).unwrap().hits.len(), 1);
}

#[test]
fn bundled_styles_are_available_and_unknown_styles_are_explicit_refusals() {
    let mut case = case();
    let engine = Engine::from_snapshot(Arc::new(case.library));
    let cancel = Cancellation::default();
    let catalog = engine.dispatch(Method::Styles, &json!({}), &cancel).unwrap().data;
    assert_eq!(catalog["styles"].as_array().unwrap().len(), 5);
    assert_eq!(catalog["styles"][0]["id"], "apa");
    case.request.output = OutputSpec::Csl {
        style: "unknown-style".into(),
    };
    let result = engine
        .dispatch(Method::Cite, &serde_json::to_value(&case.request).unwrap(), &cancel)
        .unwrap();
    assert_eq!(result.exit_code, ExitCode::Findings);
    let citation: CitationResult = serde_json::from_value(result.data).unwrap();
    assert_eq!(
        citation.outcomes,
        vec![ItemOutcome::Refused {
            item: 0,
            error: CitationError::UnsupportedOutput
        }]
    );
    assert!(citation.copy_content(1).is_none());
}
