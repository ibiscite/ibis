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
            "units",
            json!({"title": "Particles of 10 μm in μ-opioid receptors", "issued": {"date-parts": [[2017]]}}),
        ),
        (
            "accented",
            json!({"title": "Μία μελέτη για τον θεός", "issued": {"date-parts": [[2016]]}}),
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
    // Greek text keeps its letters, accented or not, one-letter words included.
    for query in ["eta", "alpha", "sigma", "iota"] {
        assert!(
            keys(&index, query)
                .iter()
                .all(|(key, _)| key != "greek" && key != "accented"),
            "{query}"
        );
    }
    // Second review: aliases never form phrases the text lacks; Greek prose
    // elsewhere keeps symbols; units in any width; short titles get aliases.
    let more = crate::index(&[
        (
            "phrase",
            json!({"title": "Müller studies β", "title-short": "β-sheets"}),
        ),
        (
            "mixed",
            json!({"title": "β-amyloid: Η γη", "container-title-short": "J β Res"}),
        ),
        ("units", json!({"title": "10 μｍ and 5 μΩ"})),
        ("prose", json!({"title": "Η γη"})),
        ("lone", json!({"title": "β δοκιμή"})),
    ]);
    assert!(keys(&more, "t:\"mueller beta\"").is_empty());
    assert_eq!(keys(&more, "beta amyloid")[0].0, "mixed");
    assert!(keys(&more, "eta").iter().all(|(key, _)| key != "prose"));
    assert_eq!(keys(&more, "beta δοκιμη")[0].0, "lone");
    assert!(keys(&more, "mum").is_empty() && keys(&more, "muomega").is_empty());
    assert_eq!(keys(&more, "t:beta sheets")[0].0, "phrase");
    assert_eq!(keys(&more, "j:beta")[0].0, "mixed");
    // μ before a Latin letter is a unit; standing alone it is mu.
    assert!(keys(&index, "mum").is_empty());
    assert_eq!(keys(&index, "mu opioid"), [("units".to_string(), false)]);
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
    // Hinted words must still match; only the others may be missing.
    assert_eq!(keys(&index, "a:smith memory quantum"), [("plain".to_string(), true)]);
    assert!(keys(&index, "a:nobody memory quantum").is_empty());
    // One- and two-letter words are not counted.
    assert!(keys(&index, "of in quantum memory").iter().all(|(_, partial)| *partial));
    assert!(keys(&index, "of in quantum").is_empty());
    // A repeated word counts each time it is typed, typed out or not.
    let sleep = crate::index(&[("sleep", json!({"title": "Sleep"}))]);
    assert_eq!(
        keys(&sleep, "sleep sleep quantum photon "),
        [("sleep".to_string(), true)]
    );
    assert!(keys(&sleep, "sleep quantum quantum ").is_empty());
    assert!(keys(&sleep, "sleep quantum quantum").is_empty());
    // Repeats score the same either way too: equal matches, newest first.
    let two = crate::index(&[
        ("sleep", json!({"title": "Sleep", "issued": {"date-parts": [[2020]]}})),
        (
            "quantum",
            json!({"title": "Quantum", "issued": {"date-parts": [[2026]]}}),
        ),
    ]);
    for query in [
        "sleep sleep quantum quantum",
        "sleep sleep quantum quantum ",
        "sleep sleep quantum quantum y:2020-2026",
        "y:2020-2026 sleep sleep quantum quantum",
    ] {
        let both = [("quantum".to_string(), true), ("sleep".to_string(), true)];
        assert_eq!(keys(&two, query), both, "{query}");
        let first = two.search(query, 1, None, &|| false).unwrap();
        assert_eq!(first[0].item.object.id().key, "quantum", "{query}");
    }
    assert_eq!(
        keys(&index, "sleep quantum quantum"),
        keys(&index, "sleep quantum quantum ")
    );
    // Long pasted text gets none even when its words repeat.
    assert!(keys(&index, &"sleep quantum ".repeat(1000)).is_empty());
    assert!(keys(&index, &format!("sleep memory {}", "z".repeat(10_000))).is_empty());
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
