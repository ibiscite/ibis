//! Citing CSL-JSON records a caller sends, with no library (#1539). The
//! records are untrusted input: nothing they name is opened or followed.
#![allow(clippy::unwrap_used)]
use ibis_core::contract::{CitationError, ErrorCode, ErrorReason, ItemOutcome};
use ibis_engine::{Cancellation, DispatchResult, Engine, Method};
use serde_json::{Value, json};

fn records(engine: &Engine, params: Value) -> Result<DispatchResult, ibis_core::contract::EngineError> {
    engine.dispatch(Method::Records, &params, &Cancellation::default())
}

fn apa(records: Value) -> Value {
    json!({"records": records, "output": {"kind": "preset", "name": "apa"}})
}

fn pandoc(records: Value) -> Value {
    json!({"records": records, "output": {"kind": "preset", "name": "pandoc"}})
}

fn two() -> Value {
    json!([
        {"id": "r1", "type": "book", "title": "Invented Lanterns", "author": [{"family": "Vale", "given": "Ada"}],
         "issued": {"date-parts": [[2024]]}, "citation-key": "vale2024"},
        {"id": 2, "type": "article-journal", "title": "Invented Tides", "author": [{"family": "Fen", "given": "Ola"}],
         "issued": {"date-parts": [[2023]]}}
    ])
}

#[test]
fn two_records_give_an_apa_citation_and_no_library_changes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invented.bib");
    std::fs::write(&path, "@book{kept,title={Invented Kept}}\n").unwrap();
    let engine = Engine::new().unwrap().without_file_watches();
    engine.open_file_observed(&path, &Cancellation::default()).unwrap();
    let before = serde_json::to_value(engine.libraries().unwrap()).unwrap();
    let snapshot = engine.snapshot().unwrap().fingerprints().clone();
    let cited = records(&engine, apa(two())).unwrap();
    assert_eq!(cited.text.as_deref(), Some("(Fen, 2023; Vale, 2024)"));
    assert_eq!(cited.exit_code, ibis_core::contract::ExitCode::Ok);
    assert_eq!(serde_json::to_value(engine.libraries().unwrap()).unwrap(), before);
    assert_eq!(engine.snapshot().unwrap().fingerprints(), &snapshot);
    // Chosen records, in the order asked for, with what goes with each.
    let chosen = records(
        &engine,
        json!({"records": two(), "output": {"kind": "preset", "name": "pandoc"},
               "items": [{"id": "r1", "locator": {"term": "page", "value": "12"}}]}),
    )
    .unwrap();
    assert_eq!(cited.data["outcomes"].as_array().unwrap().len(), 2);
    assert_eq!(chosen.text.as_deref(), Some("[@vale2024, p. 12]"));
    let missing = records(
        &engine,
        json!({"records": two(), "output": {"kind": "preset", "name": "apa"},
        "items": [{"id": "r3"}]}),
    );
    assert_eq!(missing.unwrap_err().code, ErrorCode::NotFound);
}

#[test]
fn a_keyless_record_formats_and_key_outputs_refuse_it() {
    let engine = Engine::new().unwrap();
    let keyless = json!([{"id": "r1", "type": "book", "title": "Invented"}]);
    assert_eq!(
        records(&engine, apa(keyless.clone())).unwrap().text.as_deref(),
        Some("(Invented, n.d.)")
    );
    let refused = records(&engine, pandoc(keyless)).unwrap();
    assert!(refused.text.is_none());
    let outcomes: Vec<ItemOutcome> = serde_json::from_value(refused.data["outcomes"].clone()).unwrap();
    assert_eq!(
        outcomes,
        [ItemOutcome::Refused {
            item: 0,
            error: CitationError::MissingKey
        }]
    );
}

#[test]
fn repeated_ids_and_invalid_records_refuse_the_whole_request() {
    let engine = Engine::new().unwrap();
    for repeated in [
        json!([{"id": "r1", "type": "book", "title": "A"}, {"id": "r1", "type": "book", "title": "B"}]),
        json!([{"id": 1, "type": "book", "title": "A"}, {"id": "1", "type": "book", "title": "B"}]),
    ] {
        let error = records(&engine, apa(repeated)).unwrap_err();
        assert_eq!(
            (error.code, error.reason),
            (ErrorCode::BadInput, Some(ErrorReason::DuplicateRecord))
        );
    }
    // A record with no type cannot be cited, so nothing is: none is silently left out.
    let one_bad = json!([{"id": "r1", "type": "book", "title": "A"}, {"id": "r2", "title": "No type"}]);
    let error = records(&engine, apa(one_bad)).unwrap_err();
    assert_eq!(
        (error.code, error.reason),
        (ErrorCode::BadInput, Some(ErrorReason::RejectedRecord))
    );
    for empty in [
        json!({"records": [], "output": {"kind": "preset", "name": "apa"}}),
        json!({"records": [1]}),
    ] {
        assert_eq!(records(&engine, empty).unwrap_err().code, ErrorCode::BadInput);
    }
    let unknown = json!({"records": two(), "output": {"kind": "preset", "name": "apa"}, "library": "x"});
    assert_eq!(records(&engine, unknown).unwrap_err().code, ErrorCode::BadInput);
}

#[test]
fn a_file_field_is_never_followed_and_private_links_are_removed() {
    let dir = tempfile::tempdir().unwrap();
    let secret = dir.path().join("invented-secret.pdf");
    std::fs::write(&secret, b"invented").unwrap();
    let engine = Engine::new().unwrap();
    let sent = json!([{"id": "r1", "type": "book", "title": "Invented", "citation-key": "one",
        "file": secret.to_string_lossy(), "URL": "https://person:secret@example.com/invented"},
        {"id": "r2", "type": "book", "title": "Invented Two", "citation-key": "two",
        "file": format!("file://{}", secret.to_string_lossy()), "URL": "file:///invented/private.pdf"}]);
    let exported = records(
        &engine,
        json!({"records": sent, "output": {"kind": "preset", "name": "csl-json"}}),
    )
    .unwrap();
    let text = exported.text.unwrap();
    assert!(!text.contains("invented-secret"), "{text}");
    assert!(!text.contains("secret@") && !text.contains("file:"), "{text}");
    let exported: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(exported[0]["title"], "Invented");
    assert!(exported[0].get("file").is_none() && exported[0].get("URL").is_none());
}

#[test]
fn too_many_or_too_deep_records_hit_a_resource_limit() {
    let engine = Engine::new().unwrap();
    let many: Vec<Value> = (0..1001)
        .map(|n| json!({"id": format!("r{n}"), "type": "book", "title": "Invented"}))
        .collect();
    assert_eq!(
        records(&engine, apa(json!(many))).unwrap_err().code,
        ErrorCode::ResourceLimit
    );
    let mut deep = json!("bottom");
    for _ in 0..20 {
        deep = json!([deep]);
    }
    let nested = json!([{"id": "r1", "type": "book", "title": "Invented", "custom": deep}]);
    assert_eq!(
        records(&engine, apa(nested)).unwrap_err().code,
        ErrorCode::ResourceLimit
    );
    let large = json!([{"id": "r1", "type": "book", "title": "x".repeat(600 * 1024)}]);
    assert_eq!(records(&engine, apa(large)).unwrap_err().code, ErrorCode::ResourceLimit);
}

#[test]
fn a_cancelled_request_leaves_the_next_one_unaffected() {
    let engine = Engine::new().unwrap();
    let cancel = Cancellation::default();
    cancel.cancel();
    assert_eq!(
        engine.dispatch(Method::Records, &apa(two()), &cancel).unwrap_err().code,
        ErrorCode::Cancelled
    );
    assert!(records(&engine, apa(two())).unwrap().text.is_some());
}

/// Every CSL field a style can print reaches the formatter: the export gives
/// back each one sent, and APA prints those its reference list uses.
#[test]
fn every_style_field_survives_into_rendering() {
    let engine = Engine::new().unwrap();
    let record = json!({
        "id": "r1", "type": "article-journal", "citation-key": "vale2024",
        "title": "Invented Lanterns", "title-short": "Lanterns",
        "author": [{"family": "Vale", "given": "Ada"}], "editor": [{"family": "Fen", "given": "Ola"}],
        "container-title": "Journal of Invented Light", "container-title-short": "J. Inv. Light",
        "collection-title": "Invented Series", "publisher": "Imaginary Press", "publisher-place": "Nowhere",
        "volume": "7", "issue": "3", "page": "101-119", "number": "e1234", "edition": "2",
        "issued": {"date-parts": [[2024, 5, 2]]}, "accessed": {"date-parts": [[2026, 1, 1]]},
        "DOI": "10.5555/invented.1234", "ISSN": "1234-5678", "URL": "https://example.com/invented",
        "language": "en", "genre": "Research article", "abstract": "Invented abstract.", "note": "Invented note."
    });
    let exported = records(
        &engine,
        json!({"records": [record.clone()], "output": {"kind": "preset", "name": "csl-json"}}),
    )
    .unwrap();
    let back: Value = serde_json::from_str(exported.text.as_deref().unwrap()).unwrap();
    for (field, value) in record.as_object().unwrap() {
        assert_eq!(&back[0][field], value, "{field}");
    }
    let entry = records(
        &engine,
        json!({"records": [record], "output": {"kind": "preset", "name": "apa"},
        "form": "bibliography-only"}),
    )
    .unwrap();
    let text = entry.text.unwrap();
    for printed in [
        "Vale, A.",
        "(2024)",
        "Invented Lanterns",
        "Journal of Invented Light",
        "7",
        "(3)",
        "Article e1234",
        "https://doi.org/10.5555/invented.1234",
    ] {
        assert!(text.contains(printed), "{printed} in {text}");
    }
}
