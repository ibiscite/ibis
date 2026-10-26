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
    assert_eq!(shown, case.library.objects()[0]);
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
    let library = case.library.objects()[0].object.id().library.clone();
    engine.publish_library(library.clone(), Arc::new(case.library)).unwrap();
    let event = events.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
    assert_eq!(event.libraries, [library]);
    assert_eq!(event.snapshot, engine.snapshot().unwrap().fingerprints().clone());
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
fn a_slow_subscriber_gets_latest_state_and_an_explicit_full_resync() {
    let engine = Engine::new().unwrap();
    let events = engine.subscribe().unwrap();
    for number in 0..40 {
        engine
            .publish_library(
                LibraryId::file(format!("synthetic-slow-{number}").as_bytes()),
                Arc::new(Snapshot::new(vec![]).unwrap()),
            )
            .unwrap();
    }
    let event = events.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
    assert_eq!(event.sequence, 40);
    assert_eq!(serde_json::to_value(&event).unwrap()["resync"], true);
    assert!(event.libraries.is_empty());
    assert_eq!(engine.libraries().unwrap().libraries.len(), 40);
    assert!(events.try_recv().is_err());
    let library = LibraryId::file(b"synthetic-after-resync");
    engine
        .publish_library(library.clone(), Arc::new(Snapshot::new(vec![]).unwrap()))
        .unwrap();
    let event = events.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
    assert_eq!(event.sequence, 41);
    assert_eq!(event.libraries, [library]);
    assert_eq!(serde_json::to_value(&event).unwrap()["resync"], false);
}

#[test]
fn public_typed_views_do_not_clone_or_return_raw_source_payloads() {
    let case = case();
    let LibraryObject::Record(mut record) = case.library.objects()[0].object.clone() else {
        panic!("record")
    };
    record.raw = Some(RawProvenance {
        format: SourceFormat::Bibtex,
        data: json!({"synthetic":"x".repeat(64*1024)}),
    });
    let engine = Engine::from_snapshot(Arc::new(Snapshot::new(vec![LibraryObject::Record(record)]).unwrap()));
    let shown = engine
        .show(&ShowRequest {
            item: ItemSelector::Key("Example2026".into()),
        })
        .unwrap();
    let LibraryObject::Record(shown) = shown.object else {
        panic!("record")
    };
    assert!(shown.raw.is_none());
    let hits = engine
        .search(
            &SearchRequest {
                query: "synthetic".into(),
                limit: 10,
                libraries: None,
            },
            &Cancellation::default(),
        )
        .unwrap();
    for hit in hits.hits {
        let LibraryObject::Record(record) = hit.item.object else {
            panic!("record")
        };
        assert!(record.raw.is_none());
    }
    let snapshot = engine.snapshot().unwrap();
    let LibraryObject::Record(record) = &snapshot.objects()[0].object else {
        panic!("record")
    };
    assert!(record.raw.is_some());
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
fn unsupported_formatting_is_an_honest_per_item_refusal() {
    let mut case = case();
    let engine = Engine::from_snapshot(Arc::new(case.library));
    let cancel = Cancellation::default();
    assert_eq!(
        engine.dispatch(Method::Styles, &json!({}), &cancel).unwrap().data,
        json!({"styles":[]})
    );
    case.request.output = OutputSpec::Csl { style: "apa".into() };
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
