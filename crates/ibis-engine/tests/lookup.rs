//! Local identity and outbound intent only: these scenarios make no network requests.
#![allow(clippy::unwrap_used)]

#[path = "../src/lookup.rs"]
mod lookup;

use ibis_core::{contract::*, identifier::Identifier};
use ibis_engine::Engine;
use lookup::{Disclosure, Refusal, Resolution, resolve};
use serde_json::{Value, json};
use std::sync::Arc;

const DOI: &str = "10.5555/invented-river";
const OTHER: &str = "10.5555/invented-lantern";

fn record(library: &str, key: &str, fields: Value, identifiers: Vec<Identifier>) -> LibraryObject {
    let mut csl = json!({
        "id": key,
        "type": "article-journal",
        "title": "PRIVATE_TITLE_SENTINEL",
        "author": [{"family": "PRIVATE_AUTHOR_SENTINEL"}],
        "note": "PRIVATE_DOCUMENT_SENTINEL",
        "URL": "file:///invented/PRIVATE_PATH_SENTINEL.pdf"
    });
    csl.as_object_mut().unwrap().extend(fields.as_object().unwrap().clone());
    LibraryObject::Record(Record {
        id: ItemRef {
            library: LibraryId::new(library).unwrap(),
            key: key.into(),
        },
        added_at: None,
        csl: serde_json::from_value(csl).unwrap(),
        citation_keys: vec![CitationKey {
            value: "PRIVATE_CITATION_KEY_SENTINEL".into(),
            source: CitationKeySource::Bibtex,
        }],
        aliases: vec!["PRIVATE_ALIAS_SENTINEL".into()],
        warnings: vec![],
        identifiers,
    })
}

fn disclose(query: &str, snapshot: &Snapshot) -> Disclosure {
    let Resolution::NeedsApproval(disclosure) = resolve(query, snapshot, Some(true)).unwrap() else {
        panic!("an enabled local miss must require disclosure");
    };
    disclosure
}

#[test]
fn absent_default_and_off_refuse_outbound_intent() {
    let snapshot = Snapshot::new(vec![]).unwrap();
    for permission in [Option::<bool>::default(), Some(false)] {
        assert!(matches!(resolve(DOI, &snapshot, permission), Err(Refusal::Disabled)));
    }
}

#[test]
fn local_resolution_returns_every_exact_match_across_open_libraries_even_while_off() {
    let mut objects: Vec<_> = (0..80)
        .map(|i| record("a", &format!("decoy-{i}"), json!({"title": DOI}), vec![]))
        .collect();
    // More matches than a picker page, after all the title-only decoys.
    objects.extend((0..70).map(|i| record("b", &format!("held-{i:02}"), json!({"DOI": DOI}), vec![])));
    objects.push(record(
        "c",
        "url",
        json!({"URL": "https://doi.org/10.5555/INVENTED-RIVER"}),
        vec![],
    ));
    objects.push(record("d", "search-only", json!({}), vec![Identifier::Doi(DOI.into())]));
    let snapshot = Snapshot::new(objects).unwrap();
    let before = snapshot.clone();
    for permission in [None, Some(false), Some(true)] {
        let Resolution::Local(matches) = resolve("doi: 10.5555/INVENTED-RIVER", &snapshot, permission).unwrap() else {
            panic!("held DOI must resolve locally before permission is considered");
        };
        let ids: Vec<_> = matches.iter().map(|item| item.object.id().clone()).collect();
        let expected: Vec<_> = snapshot.objects()[80..]
            .iter()
            .map(|item| item.object.id().clone())
            .collect();
        assert_eq!(ids, expected);
        assert_eq!(matches.len(), 72);
    }
    assert_eq!(snapshot, before);
}

#[test]
fn local_identity_includes_the_core_arxiv_doi_equivalence() {
    let snapshot = Snapshot::new(vec![record(
        "invented",
        "preprint",
        json!({}),
        vec![Identifier::Arxiv("arXiv:2101.00001v2".into())],
    )])
    .unwrap();
    let Resolution::Local(matches) = resolve("10.48550/arxiv.2101.00001", &snapshot, None).unwrap() else {
        panic!("core identifier equivalence must be used");
    };
    assert_eq!(matches[0].object.id().key, "preprint");
}

#[test]
fn enabling_alone_requires_disclosure_of_exact_doi_bytes_and_all_services() {
    let snapshot = Snapshot::new(vec![]).unwrap();
    let disclosure = disclose(" https://doi.org/10.5555/INVENTED%2Friver ", &snapshot);
    assert_eq!(disclosure.doi().as_bytes(), b"10.5555/invented/river");
    assert_eq!(
        disclosure.services(),
        ["doi.org", "api.crossref.org", "data.crosscite.org"]
    );
    let shown = disclosure.doi().as_bytes().to_vec();
    let intent = disclosure.decide(true).unwrap().prepare(Some(true)).unwrap();
    assert_eq!(intent.doi().as_bytes(), shown);
}

#[test]
fn declining_produces_no_approval_or_outbound_intent() {
    let snapshot = Snapshot::new(vec![]).unwrap();
    assert!(disclose(DOI, &snapshot).decide(false).is_none());
    // Declining does not persist disclosure or approve a later attempt.
    assert!(matches!(
        resolve(DOI, &snapshot, Some(true)),
        Ok(Resolution::NeedsApproval(_))
    ));
}

#[test]
fn approval_of_a_cannot_be_retargeted_by_disclosing_b() {
    let snapshot = Snapshot::new(vec![]).unwrap();
    let approval_a = disclose(DOI, &snapshot).decide(true).unwrap();
    let disclosure_b = disclose(OTHER, &snapshot);
    assert_eq!(disclosure_b.doi(), OTHER);
    assert!(disclosure_b.decide(false).is_none());
    // Preparing consumes approval and accepts no replacement query or DOI.
    let intent = approval_a.prepare(Some(true)).unwrap();
    assert_eq!(intent.doi(), DOI);
    assert!(matches!(
        resolve(OTHER, &snapshot, Some(true)),
        Ok(Resolution::NeedsApproval(_))
    ));
}

#[test]
fn switching_off_or_losing_permission_before_consumption_refuses_intent() {
    let snapshot = Snapshot::new(vec![]).unwrap();
    for current_permission in [Some(false), None] {
        let approval = disclose(DOI, &snapshot).decide(true).unwrap();
        assert!(matches!(approval.prepare(current_permission), Err(Refusal::Disabled)));
    }
}

#[test]
fn outbound_intent_contains_only_the_approved_doi_and_leaves_the_library_unchanged() {
    let engine = Engine::from_snapshot(Arc::new(
        Snapshot::new(vec![record(
            "PRIVATE_LIBRARY_SENTINEL",
            "PRIVATE_RECORD_SENTINEL",
            json!({"DOI": OTHER}),
            vec![Identifier::Pmid("12345".into())],
        )])
        .unwrap(),
    ));
    let before = engine.snapshot().unwrap();
    let libraries = serde_json::to_value(engine.libraries().unwrap()).unwrap();
    let intent = disclose(DOI, &before)
        .decide(true)
        .unwrap()
        .prepare(Some(true))
        .unwrap();
    // This is the complete outbound-intent payload, not a captured HTTP request.
    assert_eq!(serde_json::to_value(&intent).unwrap(), json!({"doi": DOI}));
    assert_eq!(intent.doi(), DOI);
    assert_eq!(*engine.snapshot().unwrap(), *before);
    assert_eq!(serde_json::to_value(engine.libraries().unwrap()).unwrap(), libraries);
}

#[test]
fn unsupported_identifiers_and_ordinary_search_are_refused_even_if_held_locally() {
    let snapshot = Snapshot::new(vec![record(
        "invented",
        "held",
        json!({"title": "invented rivers", "ISBN": "9780306406157"}),
        vec![Identifier::Pmid("12345".into()), Identifier::Arxiv("2101.00001".into())],
    )])
    .unwrap();
    for query in [
        "",
        "invented rivers",
        "pmid:12345",
        "ISBN 9780306406157",
        "arXiv:2101.00001",
        "10.5555/",
        "https://example.org/10.5555/invented-river",
        "10.5555/a 10.5555/b",
    ] {
        for permission in [None, Some(false), Some(true)] {
            assert!(
                matches!(resolve(query, &snapshot, permission), Err(Refusal::UnsupportedQuery)),
                "{query}"
            );
        }
    }
}
