//! Clients receive an actionable diagnostic, without interpreting English.
use ibis_core::contract::*;
use serde_json::Value;

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
