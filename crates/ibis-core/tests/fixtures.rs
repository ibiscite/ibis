//! Loads every fixture in `fixtures/` and compares a readable summary with
//! the reviewed snapshot. A changed snapshot means behaviour changed.

use ibis_core::bibtex::load_str;
use serde_json::{Value, json};

fn summary(file: &str) -> Value {
    let path = format!("{}/../../fixtures/bibtex/{file}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let result = load_str(file, &text);
    json!({
        "references": result.references.iter().map(|r| json!({
            "key": r.id.native_key,
            "type": r.item_type,
            "title": r.title,
            "authors": r.authors.iter().map(|n| n.display()).collect::<Vec<_>>(),
            "issued": r.issued,
            "variables": r.variables,
            "container": r.container_title,
            "doi": r.doi,
            "url": r.url,
            "keywords": r.keywords,
            "language": r.language,
        })).collect::<Vec<_>>(),
        "warnings": result.warnings,
    })
}

#[test]
fn real_world_quirks() {
    insta::assert_json_snapshot!(summary("real-world.bib"));
}

#[test]
fn damaged_file_keeps_good_entries() {
    insta::assert_json_snapshot!(summary("damaged.bib"));
}

#[test]
fn empty_file_is_empty_not_an_error() {
    assert_eq!(summary("empty.bib"), json!({"references": [], "warnings": []}));
}

#[test]
fn csl_export_keeps_every_citation_detail() {
    let path = format!("{}/../../fixtures/bibtex/real-world.bib", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let result = load_str("real-world.bib", &text);
    let csl: Vec<Value> = result
        .references
        .iter()
        .filter(|r| matches!(r.id.native_key.as_str(), "complete2024" | "edition2017" | "mueller2021"))
        .map(|r| r.to_csl_json())
        .collect();
    insta::assert_json_snapshot!(csl);
}
