#![allow(clippy::unwrap_used)]
use std::sync::Arc;

use ibis_core::{
    contract::*,
    identifier::Identifier,
    search::{MatchField, SnapshotIndex},
};
use serde_json::json;

#[test]
fn incremental_index_matches_a_rebuild_after_insert_edit_and_delete() {
    let case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap();
    let LibraryObject::Record(mut base) = case.library.objects()[0].object.clone() else {
        panic!("record")
    };
    base.csl.insert("title".into(), json!("García synthetic widgets"));
    base.csl.insert("title-short".into(), json!("Widgets"));
    base.csl.insert("abstract".into(), json!("Invented abstract text"));
    let make = |key: &str| {
        let mut record = base.clone();
        record.id.key = key.into();
        record.citation_keys[0].value = key.into();
        record
    };
    let mut objects = ["b", "d", "f", "h"]
        .map(|key| LibraryObject::Record(make(key)))
        .to_vec();
    objects.push(LibraryObject::Tag(Tag {
        id: ItemRef {
            library: base.id.library.clone(),
            key: "c:tag".into(),
        },
        name: "Invented tag".into(),
        items: vec![],
    }));
    let before = SnapshotIndex::new(Arc::new(Snapshot::new(objects).unwrap()));
    let mut edited = make("f");
    edited.csl.insert("title".into(), json!("Edited synthetic signals"));
    edited.identifiers.push(Identifier::Arxiv("2601.00001".into()));
    edited.aliases.push("older-f".into());
    let snapshot = Arc::new(
        Snapshot::new(vec![
            LibraryObject::Record(make("a")),
            LibraryObject::Record(make("d")),
            LibraryObject::Record(edited),
            LibraryObject::Record(make("z")),
        ])
        .unwrap(),
    );
    let updated = SnapshotIndex::updated(snapshot.clone(), &before);
    let rebuilt = SnapshotIndex::new(snapshot);
    for query in [
        "garcia",
        "widgets",
        "garcia synthetic widgets",
        "signals",
        "invented abstract",
        "arxiv:2601.00001",
        "a",
        "d",
        "f",
        "h",
        "z",
    ] {
        let read = |index: &SnapshotIndex| {
            index
                .search(query, 10, None, &|| false)
                .unwrap()
                .into_iter()
                .map(|hit| (hit.item.clone(), hit.score, hit.matched, hit.typo, hit.partial))
                .collect::<Vec<_>>()
        };
        assert_eq!(read(&updated), read(&rebuilt), "{query}");
    }
    assert!(updated.search("widgets", 10, None, &|| true).is_none());
    for (query, expected, missing) in [
        ("@d @a @f @h", vec!["d", "a", "f"], vec!["h"]),
        ("arxiv:2601.00001 @z", vec!["f", "z"], vec![]),
        ("@older-f @d", vec!["f", "d"], vec![]),
        ("@b @h", vec![], vec!["b", "h"]),
    ] {
        for index in [&updated, &rebuilt] {
            let list = index.pasted_list(query, None, &|| false).unwrap().unwrap();
            assert_eq!(
                list.hits
                    .iter()
                    .map(|hit| hit.item.object.id().key.as_str())
                    .collect::<Vec<_>>(),
                expected,
                "{query}"
            );
            assert_eq!(list.missing, missing, "{query}");
            assert!(list.hits.iter().all(|hit| !hit.partial));
        }
    }
}

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
