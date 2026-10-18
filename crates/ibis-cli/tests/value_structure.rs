//! Loading diagnostics and CSL export agree about rejected field structure.

use std::path::Path;
use std::process::Command;

#[test]
fn unbalanced_values_are_reported_and_never_export_injected_metadata() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/bibtex/unbalanced-values.bib");
    let check = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .arg("check")
        .arg(&path)
        .args(["--json"])
        .output()
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(check.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(report[0]["loaded"], 6);
    assert_eq!(report[0]["warnings"].as_array().map(Vec::len), Some(4));
    assert!(
        report[0]["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|w| w["kind"] == "field"
                && w["key"] == "paren-quoted"
                && w["message"] == "field `title` has unbalanced braces; it is omitted")
    );
    let export = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .args(["csl", "paren-quoted", "--bib"])
        .arg(&path)
        .output()
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(export.status.success());
    let csl: serde_json::Value = serde_json::from_slice(&export.stdout).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(csl[0]["publisher"], "Synthetic Press");
    for field in ["title", "author", "issued"] {
        assert!(csl[0].get(field).is_none(), "{field}: {csl}");
    }
}
