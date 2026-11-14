#![allow(clippy::unwrap_used)]
use std::{cell::Cell, sync::Arc};

use ibis_core::{
    contract::*,
    identifier::Identifier,
    search::{MatchField, PastedList, SnapshotIndex},
};

fn item(library: &str, key: &str) -> ItemRef {
    ItemRef {
        library: LibraryId::new(library).unwrap(),
        key: key.into(),
    }
}

fn record(library: &str, key: &str, identifiers: Vec<Identifier>) -> LibraryObject {
    LibraryObject::Record(Record {
        id: item(library, key),
        added_at: None,
        csl: [("title".into(), serde_json::json!("Quiet memory"))]
            .into_iter()
            .collect(),
        citation_keys: vec![CitationKey {
            value: key.into(),
            source: CitationKeySource::Bibtex,
        }],
        aliases: vec![format!("old-{key}")],
        warnings: vec![],
        identifiers,
    })
}

fn objects() -> Vec<LibraryObject> {
    vec![
        record(
            "one",
            "a",
            vec![
                Identifier::Doi("10.1000/example".into()),
                Identifier::Isbn("9780306406157".into()),
                Identifier::Pmid("12345678".into()),
                Identifier::Arxiv("2101.00001".into()),
            ],
        ),
        record("one", "b", vec![Identifier::Doi("10.1000/other".into())]),
    ]
}

fn index(objects: Vec<LibraryObject>) -> SnapshotIndex {
    SnapshotIndex::new(Arc::new(Snapshot::new(objects).unwrap()))
}

fn resolve<'a>(index: &'a SnapshotIndex, query: &str) -> PastedList<'a> {
    index.pasted_list(query, None, &|| false).unwrap().unwrap()
}

#[test]
fn an_identifier_resolves_exactly() {
    let index = index(objects());
    let list = resolve(&index, "10.1000/example");
    assert_eq!(list.entries.len(), 1);
    let entry = &list.entries[0];
    assert_eq!(entry.text, "10.1000/example");
    assert_eq!(entry.candidates.len(), 1);
    assert_eq!(entry.candidates[0].item.object.id(), &item("one", "a"));
    assert_eq!(entry.candidates[0].matched, MatchField::Doi);
    assert!(entry.candidates[0].attachments.is_empty());
}

#[test]
fn equivalent_identifier_spellings_share_matching_semantics() {
    let index = index(objects());
    for (spellings, field) in [
        (
            vec![
                "10.1000/EXAMPLE",
                "https://doi.org/10.1000/example",
                "doi: 10.1000/example",
            ],
            MatchField::Doi,
        ),
        (vec!["ISBN: 0-306-40615-2", "978-0-306-40615-7"], MatchField::Identifier),
        (
            vec!["PMID:012345678", "https://pubmed.ncbi.nlm.nih.gov/12345678/"],
            MatchField::Identifier,
        ),
        (
            vec!["arXiv:2101.00001v3", "https://arxiv.org/abs/2101.00001"],
            MatchField::Identifier,
        ),
    ] {
        for query in spellings {
            let list = resolve(&index, query);
            assert_eq!(list.entries.len(), 1, "{query}");
            let candidates = &list.entries[0].candidates;
            assert_eq!(candidates.len(), 1, "{query}");
            assert_eq!(candidates[0].item.object.id(), &item("one", "a"), "{query}");
            assert_eq!(candidates[0].matched, field, "{query}");
        }
    }
}

#[test]
fn explicit_lists_keep_input_order_and_repeated_occurrences() {
    let index = index(objects());
    for query in ["[@b; @a; @b]", r"\cite{b,a,b}", "@b @a @b"] {
        let list = resolve(&index, query);
        assert_eq!(
            list.entries.iter().map(|entry| entry.text.as_str()).collect::<Vec<_>>(),
            ["b", "a", "b"]
        );
        assert_eq!(
            list.entries
                .iter()
                .map(|entry| entry.candidates[0].item.object.id().key.as_str())
                .collect::<Vec<_>>(),
            ["b", "a", "b"]
        );
        assert_eq!(
            list.hits
                .iter()
                .map(|hit| hit.item.object.id().key.as_str())
                .collect::<Vec<_>>(),
            ["b", "a"]
        );
    }
    let list = resolve(&index, "a,OLD-A,a");
    assert_eq!(list.entries.len(), 3);
    assert!(list.entries.iter().all(|entry| entry.candidates.len() == 1));
    assert_eq!(list.hits.len(), 1);
}

#[test]
fn every_entry_keeps_all_exact_candidates_even_when_search_already_emitted_them() {
    let mut data = objects();
    data.push(record("two", "a", vec![Identifier::Doi("10.1000/example".into())]));
    let index = index(data);
    let list = resolve(&index, "@a 10.1000/example @old-a");
    for entry in &list.entries {
        assert_eq!(
            entry
                .candidates
                .iter()
                .map(|candidate| candidate.item.object.id().clone())
                .collect::<Vec<_>>(),
            [item("one", "a"), item("two", "a")]
        );
    }
    assert_eq!(list.hits.len(), 2);
    assert!(list.hits.iter().all(|hit| hit.matched == [MatchField::Citekey]));
}

#[test]
fn scope_limits_candidates_and_unmatched_means_only_this_snapshot() {
    let mut data = objects();
    data.push(record("two", "a", vec![Identifier::Doi("10.1000/example".into())]));
    let index = index(data);
    let list = index
        .pasted_list("10.1000/example", Some(&[LibraryId::new("two").unwrap()]), &|| false)
        .unwrap()
        .unwrap();
    assert_eq!(list.entries[0].candidates.len(), 1);
    assert_eq!(list.entries[0].candidates[0].item.object.id(), &item("two", "a"));
    let list = index
        .pasted_list("10.1000/example", Some(&[]), &|| false)
        .unwrap()
        .unwrap();
    assert_eq!(list.entries.len(), 1);
    assert!(list.entries[0].candidates.is_empty());
    let empty = index_with_empty_snapshot();
    assert!(resolve(&empty, "10.1000/example").entries[0].candidates.is_empty());
}

fn index_with_empty_snapshot() -> SnapshotIndex {
    index(vec![])
}

#[test]
fn unmatched_entries_remain_visible_including_equivalent_repeats() {
    let index = index(objects());
    let list = resolve(&index, "@absent @a @ABSENT PMID:999 pmid:0999");
    assert_eq!(
        list.entries.iter().map(|entry| entry.text.as_str()).collect::<Vec<_>>(),
        ["absent", "a", "ABSENT", "PMID:999", "pmid:0999"]
    );
    assert_eq!(
        list.entries
            .iter()
            .map(|entry| entry.candidates.len())
            .collect::<Vec<_>>(),
        [0, 1, 0, 0, 0]
    );
    assert_eq!(list.missing, ["absent", "PMID:999"]);
}

#[test]
fn ordinary_prose_retains_ordinary_search_fallback() {
    let index = index(objects());
    for query in ["quiet memory", "quiet, memory"] {
        assert!(index.pasted_list(query, None, &|| false).unwrap().is_none());
        assert!(!index.search(query, 10, None, &|| false).unwrap().is_empty());
    }
}

#[test]
fn raw_keys_keep_precedence_over_identifier_interpretation() {
    let mut data = objects();
    data.push(record("one", "10.1000/example", vec![]));
    let index = index(data);
    let list = resolve(&index, "10.1000/example, PMID:12345678");
    assert_eq!(
        list.entries[0].candidates[0].item.object.id(),
        &item("one", "10.1000/example")
    );
    assert_eq!(list.entries[0].candidates[0].matched, MatchField::Citekey);
    assert_eq!(list.entries[1].candidates[0].item.object.id(), &item("one", "a"));
    let list = resolve(&index, "@{10.1000/other}");
    assert!(list.entries[0].candidates.is_empty());
}

#[test]
fn attachments_report_existing_pdf_and_html_metadata_without_inferred_downloads() {
    let mut data = objects();
    data.push(record("two", "a", vec![]));
    for (library, key, parent, media_type, availability) in [
        ("one", "0-pdf", "a", "application/pdf", Availability::OnDisk),
        ("one", "z-html", "a", "text/html", Availability::NotDownloaded),
        ("one", "z-xhtml", "a", "application/xhtml+xml", Availability::Unknown),
        ("one", "z-online", "a", "application/pdf", Availability::Online),
        ("one", "z-image", "a", "image/png", Availability::OnDisk),
        ("one", "z-other", "b", "application/pdf", Availability::CloudPlaceholder),
        ("one", "z-orphan", "absent", "application/pdf", Availability::BrokenLink),
        ("two", "z-pdf", "a", "application/pdf", Availability::Unreadable),
    ] {
        data.push(LibraryObject::Attachment(Attachment {
            id: item(library, key),
            parent: Some(item(library, parent)),
            title: "Saved copy".into(),
            media_type: media_type.into(),
            availability,
            content_hash: None,
        }));
    }
    let index = index(data);
    let list = resolve(&index, "@a @b @a");
    let attached = |entry: usize, candidate: usize| {
        list.entries[entry].candidates[candidate]
            .attachments
            .iter()
            .map(|attachment| {
                (
                    attachment.id.clone(),
                    attachment.media_type.as_str(),
                    attachment.availability,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        attached(0, 0),
        [
            (item("one", "0-pdf"), "application/pdf", Availability::OnDisk),
            (item("one", "z-html"), "text/html", Availability::NotDownloaded),
            (item("one", "z-online"), "application/pdf", Availability::Online),
            (item("one", "z-xhtml"), "application/xhtml+xml", Availability::Unknown),
        ]
    );
    assert_eq!(
        attached(0, 1),
        [(item("two", "z-pdf"), "application/pdf", Availability::Unreadable)]
    );
    assert_eq!(
        attached(1, 0),
        [(
            item("one", "z-other"),
            "application/pdf",
            Availability::CloudPlaceholder
        )]
    );
    assert_eq!(attached(0, 0), attached(2, 0));
}

#[test]
fn cancellation_at_any_poll_returns_no_partial_answer() {
    let index = index(objects());
    let query = "@a @absent @b @a";
    let polls = Cell::new(0);
    assert!(
        index
            .pasted_list(query, None, &|| {
                polls.set(polls.get() + 1);
                false
            })
            .unwrap()
            .is_some()
    );
    for cancel_at in 1..=polls.get() {
        let checks = Cell::new(0);
        assert!(
            index
                .pasted_list(query, None, &|| {
                    checks.set(checks.get() + 1);
                    checks.get() >= cancel_at
                })
                .is_none()
        );
    }
}
