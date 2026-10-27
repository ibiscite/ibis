#![allow(clippy::unwrap_used)]
#[path = "support/zotero.rs"]
mod fixture;
use ibis_core::contract::{ErrorCode, GoldenCase};
use ibis_engine::{Cancellation, CitationCommitError, Engine, Method};
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
