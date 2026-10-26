//! Typed loading diagnostics and citation export agree about rejected fields.
use serde_json::Value;
use std::{path::Path, process::Command};
#[test]
fn unbalanced_values_are_reported_and_never_export_injected_metadata() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/bibtex/unbalanced-values.bib");
    let report = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .args(["libraries", "--file"])
        .arg(&path)
        .arg("--json")
        .output()
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(report.status.code(), Some(1));
    let value: Value = serde_json::from_slice(&report.stdout).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(value["data"]["libraries"][0]["objects"], 6);
    assert_eq!(
        value["data"]["libraries"][0]["warnings"].as_array().map(Vec::len),
        Some(4)
    );
    let shown = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .args(["show", "paren-quoted", "--file"])
        .arg(&path)
        .arg("--json")
        .output()
        .unwrap_or_else(|error| panic!("{error}"));
    let value: Value = serde_json::from_slice(&shown.stdout).unwrap_or_else(|error| panic!("{error}"));
    let csl = &value["data"]["object"]["data"]["csl"];
    assert_eq!(csl["publisher"], "Synthetic Press");
    for field in ["title", "author", "issued"] {
        assert!(csl.get(field).is_none(), "{field}");
    }
}
