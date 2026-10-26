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
    first
        .csl
        .insert("author".into(), json!([{"family":"Researcher","given":"Synthetic"}]));
    first.csl.insert(
        "translator".into(),
        json!([{"family":"Translator","given":"Synthetic","non-dropping-particle":"van"}]),
    );
    first
        .csl
        .insert("container-title".into(), json!("<i>Journal</i> &amp; Review"));
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
    for (query, field) in [("translator", MatchField::Author), ("journal", MatchField::Container)] {
        let hits = index.search(query, 2, None, &|| false).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].matched, [field]);
    }
    assert!(
        index
            .search("amp", 2, None, &|| false)
            .unwrap()
            .iter()
            .all(|hit| !hit.matched.contains(&MatchField::Container))
    );
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

/// Manual synthetic scale evidence; no machine-dependent timing assertion in CI.
#[test]
#[ignore = "manual 100000-record snapshot measurement"]
fn snapshot_index_100000_record_measurement() {
    use std::time::{Duration, Instant};
    let case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap();
    let LibraryObject::Record(base) = case.library.objects()[0].object.clone() else {
        panic!("record")
    };
    let records = (0..100_000)
        .map(|number| {
            let mut record = base.clone();
            record.id.key = format!("record:{number:05}");
            record.citation_keys[0].value = format!("ref{number:05}");
            record.csl.insert("title".into(), json!("Synthetic research 研究"));
            record
                .csl
                .insert("author".into(), json!([{"given":"Synthetic","family":"García"}]));
            LibraryObject::Record(record)
        })
        .collect();
    let began = Instant::now();
    let snapshot = Arc::new(Snapshot::new(records).unwrap());
    let snapshot_time = began.elapsed();
    let began = Instant::now();
    let index = SnapshotIndex::new(snapshot);
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    eprintln!(
        "synthetic records=100000 profile={profile} snapshot_us={} index_us={}",
        snapshot_time.as_micros(),
        began.elapsed().as_micros()
    );
    for (query, expected) in [
        ("r", 20),
        ("研", 20),
        ("research", 20),
        ("garcia", 20),
        ("ref99999", 1),
        ("missing-synthetic-term", 0),
    ] {
        let mut elapsed = Duration::ZERO;
        for _ in 0..10 {
            let began = Instant::now();
            let hits = index.search(query, 20, None, &|| false).unwrap();
            elapsed += began.elapsed();
            assert_eq!(hits.len(), expected, "{query}");
            if expected == 1 {
                assert_eq!(hits[0].item.object.id().key, "record:99999");
            }
        }
        eprintln!("query={query:?} samples=10 mean_us={}", elapsed.as_micros() / 10);
    }
}
