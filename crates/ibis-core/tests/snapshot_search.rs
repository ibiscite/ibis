#![allow(clippy::unwrap_used)]
use std::sync::Arc;

use ibis_core::{
    contract::*,
    identifier::Identifier,
    search::{MatchField, SnapshotIndex},
};
use serde_json::json;

#[test]
fn record_search_preserves_ranking_scripts_identifiers_scope_and_cancellation() {
    let case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap();
    let LibraryObject::Record(mut first) = case.library.objects()[0].object.clone() else {
        panic!("record")
    };
    first.csl.insert("title".into(), json!("García 研究 خواب がっこう"));
    first.csl.insert("issued".into(), json!({"date-parts":[[2026]]}));
    first.identifiers.push(Identifier::Arxiv("2101.00001".into()));
    first.aliases.push("older-key".into());
    let mut other = first.clone();
    other.id.library = LibraryId::file(b"second synthetic library");
    other.id.key = "second".into();
    other.citation_keys[0].value = "other".into();
    other.aliases.clear();
    other.identifiers.clear();
    other.csl.insert("issued".into(), json!({"date-parts":[[2020]]}));
    let scope = [other.id.library.clone()];
    let snapshot =
        Arc::new(Snapshot::new(vec![LibraryObject::Record(first.clone()), LibraryObject::Record(other)]).unwrap());
    let index = SnapshotIndex::new(snapshot);
    for query in ["garcia", "研究", "خواب", "ガッコウ"] {
        let hits = index.search(query, 2, None, &|| false).unwrap();
        assert_eq!(hits.len(), 2, "{query}");
        assert_eq!(hits[0].item.object.id(), &first.id);
        assert_eq!(hits[0].matched, [MatchField::Title]);
        assert_eq!(
            index.search(query, 1, Some(&scope), &|| false).unwrap()[0]
                .item
                .object
                .id()
                .library,
            scope[0]
        );
    }
    for query in ["arXiv:2101.00001", "older-key", "Example2026"] {
        assert_eq!(
            index.search(query, 2, None, &|| false).unwrap()[0].item.object.id(),
            &first.id
        );
    }
    assert!(index.search("研究", 2, Some(&[]), &|| false).unwrap().is_empty());
    assert!(index.search("", 2, None, &|| false).unwrap().is_empty());
    assert!(index.search("研究", 0, None, &|| false).unwrap().is_empty());
    assert!(index.search("研究", 2, None, &|| true).is_none());
}
