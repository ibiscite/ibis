//! Offline DOI resolution through the production public engine module.
#![allow(clippy::unwrap_used)]

use ibis_core::{contract::*, identifier::Identifier};
use ibis_engine::{
    Cancellation, Engine,
    lookup::{Refusal, Resolution, resolve},
    request::SearchRequest,
};
use serde_json::{Value, json};
use std::sync::Arc;

const DOI: &str = "10.5555/invented-river";
const OTHER: &str = "10.5555/invented-lantern";

fn record(library: &str, key: &str, fields: Value, identifiers: Vec<Identifier>) -> LibraryObject {
    let mut csl = json!({
        "id": key,
        "type": "article-journal",
        "title": "Invented Rivers",
        "author": [{"family": "Vale", "given": "Ada"}],
        "note": "Invented document text",
        "URL": "file:///invented/river.pdf"
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
            value: "vale2026".into(),
            source: CitationKeySource::Bibtex,
        }],
        aliases: vec!["invented-alias".into()],
        warnings: vec![],
        identifiers,
    })
}

fn local<'a>(query: &str, snapshot: &'a Snapshot) -> Vec<&'a SnapshotObject> {
    let Resolution::Local(matches) = resolve(query, snapshot).unwrap() else {
        panic!("a held DOI must resolve locally");
    };
    matches
}

fn missing(query: &str, snapshot: &Snapshot) -> String {
    let Resolution::Missing { doi } = resolve(query, snapshot).unwrap() else {
        panic!("an absent DOI must identify what was requested");
    };
    doi
}

#[test]
fn a_doi_held_beyond_a_search_result_cap_is_found() {
    let engine = Engine::from_snapshot(Arc::new(
        Snapshot::new(
            (0..80)
                .map(|i| record("invented", &format!("held-{i:02}"), json!({"DOI": DOI}), vec![]))
                .collect(),
        )
        .unwrap(),
    ));
    let page = engine
        .search(
            &SearchRequest {
                query: "Invented Rivers".into(),
                limit: 10,
                libraries: None,
                interpret_locator: false,
            },
            &Cancellation::default(),
        )
        .unwrap();
    assert_eq!(page.hits.len(), 10);
    assert!(!page.complete);

    let snapshot = engine.snapshot().unwrap();
    let beyond_cap = snapshot
        .objects()
        .iter()
        .find(|item| !page.hits.iter().any(|hit| hit.item.object.id() == item.object.id()))
        .unwrap();
    let matches = local(DOI, &snapshot);
    assert_eq!(matches.len(), 80);
    assert!(matches.iter().any(|item| std::ptr::eq(*item, beyond_cap.as_ref())));
}

#[test]
fn every_exact_match_across_libraries_keeps_its_identity_and_fingerprints() {
    let snapshot = Snapshot::new(vec![
        record(
            "invented-a",
            "shared-key",
            json!({"DOI": DOI, "URL": DOI}),
            vec![Identifier::Doi(DOI.into())],
        ),
        record("invented-b", "shared-key", json!({"DOI": DOI}), vec![]),
        record(
            "invented-c",
            "another-key",
            json!({}),
            vec![Identifier::Doi(DOI.into())],
        ),
        record("invented-d", "unrelated", json!({"DOI": OTHER}), vec![]),
    ])
    .unwrap();
    let matches = local(DOI, &snapshot);
    assert_eq!(matches.len(), 3);
    for (found, held) in matches.iter().zip(&snapshot.objects()[..3]) {
        assert!(std::ptr::eq(*found, held.as_ref()));
        assert_eq!(found.object.id(), held.object.id());
        assert_eq!(found.fingerprints, held.fingerprints);
    }
}

#[test]
fn normalized_doi_queries_match_csl_fields_urls_and_source_identifiers() {
    let snapshot = Snapshot::new(vec![
        record(
            "invented",
            "doi-field",
            json!({"DOI": " DOI: 10.5555/INVENTED-RIVER "}),
            vec![],
        ),
        record(
            "invented",
            "url-field",
            json!({"URL": "https://doi.org/10.5555/INVENTED%2Driver"}),
            vec![],
        ),
        record(
            "invented",
            "legacy-url",
            json!({"URL": "http://dx.doi.org/10.5555/INVENTED-RIVER"}),
            vec![],
        ),
        record(
            "invented",
            "source-id",
            json!({}),
            vec![Identifier::Doi("DOI: 10.5555/INVENTED-RIVER".into())],
        ),
    ])
    .unwrap();
    for query in [
        DOI,
        " doi: 10.5555/INVENTED-RIVER ",
        "https://doi.org/10.5555/INVENTED%2Driver",
        "http://dx.doi.org/10.5555/INVENTED-RIVER",
    ] {
        let matches = local(query, &snapshot);
        assert_eq!(matches.len(), 4, "{query}");
    }
}

#[test]
fn existing_arxiv_doi_equivalence_matches_versioned_ids_and_urls() {
    let snapshot = Snapshot::new(vec![
        record(
            "invented",
            "preprint-id",
            json!({}),
            vec![Identifier::Arxiv("arXiv:2101.00001v2".into())],
        ),
        record(
            "invented",
            "preprint-url",
            json!({"URL": "https://arxiv.org/abs/2101.00001v3"}),
            vec![],
        ),
        record(
            "invented",
            "other-preprint",
            json!({}),
            vec![Identifier::Arxiv("2101.00002".into())],
        ),
    ])
    .unwrap();
    let matches = local("https://doi.org/10.48550/ARXIV.2101.00001", &snapshot);
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].object.id().key, "preprint-id");
    assert_eq!(matches[1].object.id().key, "preprint-url");
}

#[test]
fn title_only_decoys_do_not_resolve() {
    let snapshot = Snapshot::new(vec![
        record("invented", "title-only", json!({"title": DOI}), vec![]),
        record("invented", "wrong-doi", json!({"title": DOI, "DOI": OTHER}), vec![]),
    ])
    .unwrap();
    assert_eq!(missing(DOI, &snapshot), DOI);
}

#[test]
fn a_miss_identifies_only_the_requested_normalized_doi() {
    let snapshot = Snapshot::new(vec![record(
        "invented",
        "unrelated",
        json!({"DOI": OTHER}),
        vec![Identifier::Pmid("12345".into())],
    )])
    .unwrap();
    // No record, path, title, service list or approval accompanies a local miss.
    assert_eq!(missing(" https://doi.org/10.5555/INVENTED%2Driver ", &snapshot), DOI);
    assert_eq!(missing(DOI, &Snapshot::new(vec![]).unwrap()), DOI);
}

#[test]
fn unsupported_input_is_refused_even_if_the_identifier_is_held() {
    let snapshot = Snapshot::new(vec![record(
        "invented",
        "held",
        json!({"title": "Invented Rivers", "ISBN": "9780306406157"}),
        vec![Identifier::Pmid("12345".into()), Identifier::Arxiv("2101.00001".into())],
    )])
    .unwrap();
    for query in [
        "",
        "Invented Rivers",
        "pmid:12345",
        "ISBN 9780306406157",
        "arXiv:2101.00001",
        "https://arxiv.org/abs/2101.00001",
        "10.5555/",
        "https://example.org/10.5555/invented-river",
        "10.5555/a 10.5555/b",
    ] {
        assert!(
            matches!(resolve(query, &snapshot), Err(Refusal::UnsupportedQuery)),
            "{query}"
        );
    }
}

#[test]
fn a_zotero_extra_doi_resolves_to_its_existing_reference() {
    let snapshot = Snapshot::new(vec![record(
        "invented",
        "extra",
        json!({"note": format!("DOI: {DOI}")}),
        vec![],
    )])
    .unwrap();
    let matches = local(DOI, &snapshot);
    assert_eq!(matches.len(), 1);
    assert!(std::ptr::eq(matches[0], snapshot.objects()[0].as_ref()));
}

#[test]
fn trailing_doi_periods_keep_distinct_references() {
    let dotted = format!("{DOI}.");
    let snapshot = Snapshot::new(vec![
        record("invented", "bare", json!({"DOI": DOI}), vec![]),
        record("invented", "dotted", json!({"DOI": dotted}), vec![]),
    ])
    .unwrap();
    for (query, held) in [(DOI, 0), (dotted.as_str(), 1)] {
        let matches = local(query, &snapshot);
        assert_eq!(matches.len(), 1);
        assert!(std::ptr::eq(matches[0], snapshot.objects()[held].as_ref()), "{query}");
    }
    assert_eq!(missing(&format!("{DOI},"), &snapshot), format!("{DOI},"));
}

#[test]
fn exported_doi_links_round_trip_to_their_reference() {
    let dois = [
        DOI,
        "10.5555/invented?river#part%2F;:,[x](y)",
        "10.5555/invented-river.",
    ];
    let engine = Engine::from_snapshot(Arc::new(
        Snapshot::new(
            dois.iter()
                .enumerate()
                .map(|(i, doi)| record("invented", &format!("export-{i}"), json!({"DOI": doi}), vec![]))
                .collect(),
        )
        .unwrap(),
    ));
    let snapshot = engine.snapshot().unwrap();
    for held in snapshot.objects() {
        let request: CitationRequest = serde_json::from_value(json!({
            "items": [{"item": held.object.id()}],
            "output": {"kind": "preset", "name": "doi-links"}
        }))
        .unwrap();
        let exported = engine.cite(&request, &Cancellation::default()).unwrap();
        assert!(exported.is_complete());
        let matches = local(&exported.text, &snapshot);
        assert_eq!(matches.len(), 1, "{}", exported.text);
        assert!(std::ptr::eq(matches[0], held.as_ref()), "{}", exported.text);
    }
}
