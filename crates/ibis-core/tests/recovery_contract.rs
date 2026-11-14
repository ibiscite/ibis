//! Clients receive an actionable diagnostic, without interpreting English.
use ibis_core::contract::*;
use serde_json::{Value, json};

#[test]
fn every_engine_failure_and_citation_refusal_has_one_recovery_and_focus() {
    for code in ErrorCode::ALL {
        let error = serde_json::to_value(EngineError::new(code)).unwrap_or_default();
        actionable(&error);
    }
    for error in CitationError::ALL {
        let outcome = serde_json::to_value(ItemOutcome::Refused { item: 0, error }).unwrap_or_default();
        actionable(&outcome);
    }
}

fn actionable(value: &Value) {
    assert!(value["message"]["id"].as_str().is_some(), "{value}");
    assert!(value["message"]["args"].is_object(), "{value}");
    assert!(value["recovery"]["action"].is_string(), "{value}");
    assert!(value["focus"].is_string(), "{value}");
}

#[test]
fn keyless_refusal_tells_every_client_to_choose_a_style() {
    let refused = serde_json::to_value(ItemOutcome::Refused {
        item: 0,
        error: CitationError::MissingKey,
    })
    .unwrap_or_default();
    assert_eq!(refused["error"], "missing-key");
    assert_eq!(refused["message"], json!({"id":"problem-missing-key", "args":{}}));
    assert_eq!(refused["recovery"], json!({"action":"choose-style"}));
    assert_eq!(refused["focus"], "format-picker");
}

#[test]
fn an_omitted_reference_requires_reviewing_the_source_library() {
    let value = serde_json::to_value(EngineError::because(ErrorCode::NotFound, ErrorReason::LibraryCutShort))
        .unwrap_or_default();
    assert_eq!(value["recovery"]["action"], "review-library");
    assert_eq!(value["focus"], "source-file");
}

#[test]
fn a_bibliography_refusal_keeps_its_diagnostic_and_recovers_inside_the_sheet() {
    let refusal = CitationError::UnsupportedForm.problem();
    let bibliography = refusal.clone().on(CitationSurface::Bibliography);
    assert_eq!(bibliography.code, refusal.code);
    assert_eq!(bibliography.info.message.id, refusal.info.message.id);
    let presented = serde_json::to_value(bibliography).unwrap_or_default();
    assert_eq!(presented["recovery"]["action"], "remove-references");
    assert_eq!(presented["focus"], "session-references");
    assert_eq!(presented["message"]["context"], "bibliography");
}
