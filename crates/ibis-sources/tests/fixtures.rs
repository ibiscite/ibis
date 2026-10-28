//! Loads representative BibTeX fixtures in `fixtures/bibtex/` and compares a readable
//! summary with the reviewed snapshot. A changed snapshot means behaviour changed.
#![allow(clippy::unwrap_used)]

use ibis_core::contract::LibraryObject;
use serde_json::{Value, json};

fn summary(file: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/bibtex")
        .join(file);
    let loaded = ibis_sources::load_file(&path).unwrap();
    let mut records: Vec<Value> = loaded
        .snapshot
        .objects()
        .iter()
        .filter_map(|item| match &item.object {
            LibraryObject::Record(record) => Some(json!({
                "key": record.citation_key().unwrap(),
                "csl": record.csl,
                "identifiers": record.identifiers,
            })),
            _ => None,
        })
        .collect();
    records.sort_by_key(|record| record["key"].to_string());
    json!({ "records": records, "warnings": loaded.warnings })
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
    assert_eq!(summary("empty.bib"), json!({"records": [], "warnings": []}));
}
