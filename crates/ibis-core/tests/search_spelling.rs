#![allow(clippy::unwrap_used)]
//! Other spellings (Mueller for Müller, beta for β) and the closest matches
//! given when nothing matches every term.
use std::sync::Arc;

use ibis_core::{
    contract::*,
    search::{MatchField, SnapshotIndex},
};
use serde_json::{Value, json};

fn index(records: &[(&str, Value)]) -> SnapshotIndex {
    let case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap();
    let LibraryObject::Record(template) = case.library.objects()[0].object.clone() else {
        panic!("record")
    };
    let objects = records
        .iter()
        .map(|(key, csl)| {
            let mut record = template.clone();
            record.id.key = (*key).into();
            record.citation_keys[0].value = (*key).into();
            record.aliases.clear();
            record.identifiers.clear();
            record.csl = csl.as_object().unwrap().clone().into_iter().collect();
            LibraryObject::Record(record)
        })
        .collect();
    SnapshotIndex::new(Arc::new(Snapshot::new(objects).unwrap()))
}

fn keys(index: &SnapshotIndex, query: &str) -> Vec<(String, bool)> {
    index
        .search(query, 10, None, &|| false)
        .unwrap()
        .iter()
        .map(|hit| (hit.item.object.id().key.clone(), hit.partial))
        .collect()
}

fn library() -> SnapshotIndex {
    index(&[
        (
            "muller",
            json!({"title": "Sleep and memory", "author": [{"family": "Müller", "given": "Jürgen"}],
                   "issued": {"date-parts": [[2020]]}, "container-title": "Zeitschrift für Psychologie"}),
        ),
        (
            "angstrom",
            json!({"title": "Spectra", "author": [{"family": "Ångström", "given": "Anders"}],
                   "issued": {"date-parts": [[1868]]}}),
        ),
        (
            "amyloid",
            json!({"title": "β-amyloid and NF-κB in <i>Alzheimer</i> disease", "keyword": "TNF-α, sleep",
                   "author": [{"family": "Okafor", "given": "Ada"}], "issued": {"date-parts": [[2021]]}}),
        ),
        (
            "greek",
            json!({"title": "Η μνήμη του ύπνου", "author": [{"family": "Papadopoulos", "given": "Eleni"}],
                   "issued": {"date-parts": [[2019]]}}),
        ),
        (
            "plain",
            json!({"title": "Memory consolidation during sleep", "author": [{"family": "Smith", "given": "Jane"}],
                   "issued": {"date-parts": [[2018]]}}),
        ),
    ])
}

#[test]
fn letters_written_another_way_find_the_record() {
    let index = library();
    for query in [
        "mueller",
        "muller",
        "müller",
        "juergen mueller",
        "aangstroem",
        "angstrom",
        "fuer psychologie",
    ] {
        let hits = keys(&index, query);
        assert!(hits.first().is_some_and(|(_, partial)| !partial), "{query}: {hits:?}");
    }
    assert_eq!(keys(&index, "mueller")[0].0, "muller");
    assert_eq!(keys(&index, "aangstroem")[0].0, "angstrom");
    // Accents are folded for everyone: "muller" finds Müller with no alias.
    assert_eq!(keys(&index, "muller sleep")[0].0, "muller");
    let hit = &index.search("mueller", 1, None, &|| false).unwrap()[0];
    assert_eq!(hit.matched, [MatchField::Author]);
}

#[test]
fn a_greek_letter_standing_alone_is_found_by_its_name() {
    let index = library();
    for query in [
        "beta amyloid",
        "β-amyloid",
        "nf kappab",
        "kappab",
        "tnf alpha",
        "t:beta",
    ] {
        assert_eq!(keys(&index, query), [("amyloid".to_string(), false)], "{query}");
    }
    // The title proper is unchanged, so an exact title still ranks as one.
    let exact = &index
        .search("β-amyloid and NF-κB in Alzheimer disease", 1, None, &|| false)
        .unwrap()[0];
    assert_eq!(exact.item.object.id().key, "amyloid");
    // Words of Greek text are not spelt letter by letter.
    assert!(keys(&index, "etaomicron").is_empty());
    assert!(keys(&index, "mu").iter().all(|(key, _)| key != "greek"));
    assert_eq!(keys(&index, "μνημη")[0].0, "greek");
}

#[test]
fn when_nothing_matches_every_term_the_closest_matches_are_marked() {
    let index = library();
    // Every term matches somewhere: no closest matches.
    assert!(keys(&index, "smith sleep").iter().all(|(_, partial)| !partial));
    // "quantum" matches nothing: records with the other terms, most first.
    let closest = keys(&index, "smith memory quantum");
    assert_eq!(closest[0], ("plain".to_string(), true));
    assert!(closest.iter().all(|(_, partial)| *partial));
    assert_eq!(closest.len(), 1, "Müller matches one of three terms: {closest:?}");
    // At least half the terms must match.
    assert!(keys(&index, "sleep quantum photon").is_empty());
    assert_eq!(
        keys(&index, "memory sleep quantum photon smith")[0],
        ("plain".to_string(), true)
    );
    // One word, or a long pasted text, gets no closest matches.
    assert!(keys(&index, "quantum").is_empty());
    assert!(keys(&index, "sleep a b c d e f g h quantum").is_empty());
    // Numbers are not counted, so a missing DOI or page finds nothing.
    assert!(keys(&index, "10.2020/sleep.memory").is_empty());
    assert!(keys(&index, "10.9999/2018").is_empty());
    assert_eq!(keys(&index, "smith 2019 quantum"), [("plain".to_string(), true)]);
    // Years and scope still apply.
    assert!(keys(&index, "smith quantum y:2020").is_empty());
    assert_eq!(keys(&index, "sleep quantum y:2020"), [("muller".to_string(), true)]);
    // A smaller limit is the start of a larger one.
    let all = index.search("sleep memory quantum", 10, None, &|| false).unwrap();
    for limit in 1..all.len() {
        let some = index.search("sleep memory quantum", limit, None, &|| false).unwrap();
        let ids = |hits: &[ibis_core::search::RecordHit]| {
            hits.iter().map(|hit| hit.item.object.id().clone()).collect::<Vec<_>>()
        };
        assert_eq!(ids(&some), ids(&all[..limit]));
    }
    assert!(index.search("sleep memory quantum", 3, None, &|| true).is_none());
}
