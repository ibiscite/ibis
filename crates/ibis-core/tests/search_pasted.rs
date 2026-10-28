#![allow(clippy::unwrap_used)]
use ibis_core::{contract::*, identifier::Identifier, search::SnapshotIndex};
use std::sync::Arc;

fn index() -> SnapshotIndex {
    let case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap();
    let LibraryObject::Record(base) = case.library.objects()[0].object.clone() else {
        panic!("record")
    };
    let records = ["a", "b", "日本🌙", "any key", "doi"]
        .into_iter()
        .enumerate()
        .map(|(n, key)| {
            let mut record = base.clone();
            record.id.key = key.into();
            record.citation_keys[0].value = key.into();
            record.aliases = if n == 0 { vec!["old-a".into()] } else { vec![] };
            record.identifiers = vec![Identifier::Doi(if key == "doi" {
                "10.1000/x-@a".into()
            } else {
                format!("10.1000/synthetic{n}")
            })];
            record.csl.remove("DOI");
            LibraryObject::Record(record)
        })
        .collect();
    SnapshotIndex::new(Arc::new(Snapshot::new(records).unwrap()))
}

#[test]
fn citation_syntaxes_ignore_locators_and_preserve_paste_order() {
    let index = index();
    for query in [
        "[@b; -@a, p. 4; @b]",
        "\\cite{b,a,b}",
        "\\parencite[see][pp. 4–8]{b,a,b}",
        "#cite(<b>, <a>, supplement: [p. 4])",
        "@b @a",
        "[@b; @a",
        "\\cite{b,a",
    ] {
        let list = index.pasted_list(query, None, &|| false).unwrap().unwrap();
        let keys: Vec<_> = list.hits.iter().map(|hit| hit.item.object.id().key.as_str()).collect();
        assert_eq!(keys, ["b", "a"], "{query}");
        assert!(list.missing.is_empty(), "{query}");
        assert!(
            list.hits
                .iter()
                .all(|hit| hit.score == 3000 && !hit.typo && !hit.partial)
        );
    }
    let list = index
        .pasted_list("@{any key} @日本🌙", None, &|| false)
        .unwrap()
        .unwrap();
    assert_eq!(list.hits.len(), 2);
    assert_eq!(list.hits[0].item.object.id().key, "any key");
}

#[test]
fn plain_keys_and_identifiers_require_every_entry_to_match() {
    let index = index();
    for query in [
        "b\na",
        "b,a",
        "b; a",
        "b\ta",
        "B,OLD-A",
        "10.1000/synthetic1 https://doi.org/10.1000/synthetic0",
    ] {
        let list = index.pasted_list(query, None, &|| false).unwrap().unwrap();
        assert_eq!(
            list.hits
                .iter()
                .map(|hit| hit.item.object.id().key.as_str())
                .collect::<Vec<_>>(),
            ["b", "a"],
            "{query}"
        );
    }
    for query in ["", "a", "a b", "a, missing", "sleep, memory", "a@example.invalid"] {
        assert!(index.pasted_list(query, None, &|| false).unwrap().is_none(), "{query}");
    }
    let list = index
        .pasted_list("@missing @a @missing", None, &|| false)
        .unwrap()
        .unwrap();
    assert_eq!(list.missing, ["missing"]);
    assert_eq!(list.hits.len(), 1);
    // Explicit citation syntax denotes keys, never a guessed DOI.
    let list = index
        .pasted_list("@{10.1000/synthetic0}", None, &|| false)
        .unwrap()
        .unwrap();
    assert!(list.hits.is_empty());
    assert_eq!(list.missing, ["10.1000/synthetic0"]);
}

#[test]
fn lists_keep_scope_cancellation_and_work_limits() {
    let index = index();
    let list = index.pasted_list("@a @b", Some(&[]), &|| false).unwrap().unwrap();
    assert!(list.hits.is_empty());
    assert_eq!(list.missing, ["a", "b"]);
    assert!(index.pasted_list("@a @b", None, &|| true).is_none());
    // Overflow must never turn into a silently truncated list.
    assert!(
        index
            .pasted_list(&"@a ".repeat(1001), None, &|| false)
            .unwrap()
            .is_none()
    );
    assert!(
        index
            .pasted_list(&format!("@{{{}}}", "z".repeat(65_537)), None, &|| false)
            .unwrap()
            .is_none()
    );
}

#[test]
fn hostile_syntax_and_duplicate_aliases_remain_bounded() {
    let index = index();
    let list = index.pasted_list("a,OLD-A,a", None, &|| false).unwrap().unwrap();
    assert_eq!(list.hits.len(), 1);
    let list = index
        .pasted_list(&"@a ".repeat(1000), None, &|| false)
        .unwrap()
        .unwrap();
    assert_eq!(list.hits.len(), 1);
    for query in [
        "\\cite[".repeat(8000),
        "👩🏽‍💻 @a \\cite{日本🌙".into(),
        "\\cite".repeat(10_000),
    ] {
        let _ = index.pasted_list(&query, None, &|| false).unwrap();
    }
    let checks = std::cell::Cell::new(0);
    assert!(
        index
            .pasted_list("@a @b", None, &|| {
                checks.set(checks.get() + 1);
                checks.get() > 1
            })
            .is_none()
    );
}

#[test]
fn an_at_sign_inside_a_doi_is_not_a_citation_marker() {
    let index = index();
    let list = index
        .pasted_list("10.1000/x-@a 10.1000/synthetic1", None, &|| false)
        .unwrap()
        .unwrap();
    assert_eq!(
        list.hits
            .iter()
            .map(|hit| hit.item.object.id().key.as_str())
            .collect::<Vec<_>>(),
        ["doi", "b"]
    );
}
