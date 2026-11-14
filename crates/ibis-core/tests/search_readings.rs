#![allow(clippy::unwrap_used)]
//! Deterministic, search-only kana readings, exercised through SnapshotIndex.
use std::sync::Arc;

use ibis_core::{
    contract::*,
    search::{MatchField, SnapshotIndex},
};
use serde_json::{Value, json};

fn record(key: &str, csl: Value) -> LibraryObject {
    LibraryObject::Record(Record {
        id: ItemRef {
            library: LibraryId::digest(b"invented kana library"),
            key: key.into(),
        },
        added_at: None,
        csl: serde_json::from_value(csl).unwrap(),
        citation_keys: vec![CitationKey {
            value: key.into(),
            source: CitationKeySource::Bibtex,
        }],
        aliases: vec![],
        warnings: vec![],
        identifiers: vec![],
    })
}

fn person(key: &str, name: &str) -> LibraryObject {
    record(key, json!({"title": "Invented signals", "author": [{"literal": name}]}))
}

fn snapshot(objects: Vec<LibraryObject>) -> Arc<Snapshot> {
    Arc::new(Snapshot::new(objects).unwrap())
}

fn keys(index: &SnapshotIndex, query: &str) -> Vec<String> {
    index
        .search(query, 20, None, &|| false)
        .unwrap()
        .iter()
        .map(|hit| hit.item.object.id().key.clone())
        .collect()
}

#[test]
fn kana_width_script_and_composed_voicing_share_author_readings() {
    // Family first, one space between name parts; kana-only literal names too.
    for (family, given) in [
        ("かざね", "しゅう"),
        ("カザネ", "シュウ"),
        ("ｶｻﾞﾈ", "ｼｭｳ"),
        ("かさ\u{3099}ね", "しゅう"),
    ] {
        let source = snapshot(vec![record(
            "source",
            json!({
                "title": "Invented observations", "author": [{"family": family, "given": given}]
            }),
        )]);
        let original = serde_json::to_value(source.as_ref()).unwrap();
        let index = SnapshotIndex::new(source.clone());
        for query in [
            "kazane shuu",
            "a:kazane a:shuu",
            "a:\"kazane shuu\"",
            "かざね",
            "カザネ",
            "ｶｻﾞﾈ",
        ] {
            let hits = index.search(query, 10, None, &|| false).unwrap();
            assert_eq!(hits.len(), 1, "{family}: {query}");
            assert_eq!(hits[0].matched, [MatchField::Author]);
            assert!(!hits[0].typo && !hits[0].partial, "{query}");
        }
        assert!(keys(&index, "t:kazane").is_empty());
        assert!(keys(&index, "k:kazane").is_empty());
        // Search cannot alter original names, CSL, keys, aliases or fingerprints.
        assert_eq!(serde_json::to_value(index.snapshot().as_ref()).unwrap(), original);
    }
}

#[test]
fn supported_spelling_is_orthographic_hepburn_with_explicit_vowels() {
    // shi/chi/tsu/fu/ji, wo/wi/we/vu; ぢ=じ and づ=ず. No surname rules.
    // Small ya/yu/yo contract only the i-row; っ doubles k/s/t/p, ch -> tch.
    // ー repeats the previous vowel; おう stays ou, おお stays oo (no macrons).
    // ん is n, with an apostrophe before a vowel or y: shin'you != shinyou.
    let cases = [
        ("かざね", "kazane"),
        ("がざね", "gazane"),
        ("ぱざね", "pazane"),
        ("ばざね", "bazane"),
        ("はざね", "hazane"),
        ("きゃの", "kyano"),
        ("きやの", "kiyano"),
        ("ぎゅの", "gyuno"),
        ("しゃの", "shano"),
        ("ちゅの", "chuno"),
        ("じょの", "jono"),
        ("にゃの", "nyano"),
        ("ひゅの", "hyuno"),
        ("びょの", "byono"),
        ("ぴゃの", "pyano"),
        ("みゅの", "myuno"),
        ("りょの", "ryono"),
        ("かっこ", "kakko"),
        ("かこ", "kako"),
        ("さっし", "sasshi"),
        ("まっちゃ", "matcha"),
        ("はっぱ", "happa"),
        ("きって", "kitte"),
        ("しんよう", "shin'you"),
        ("しにょう", "shinyou"),
        ("かんあ", "kan'a"),
        ("かな", "kana"),
        ("かんな", "kanna"),
        ("らん", "ran"),
        ("コーナ", "koona"),
        ("こうな", "kouna"),
        ("しちつふじ", "shichitsufuji"),
        ("ぢづをゐゑヴ", "jizuwowiwevu"),
    ];
    let index = SnapshotIndex::new(snapshot(
        cases
            .iter()
            .enumerate()
            .map(|(n, (kana, _))| person(&format!("case{n}"), kana))
            .collect(),
    ));
    // Quotes disable the existing typo fallback, so nearby readings must differ.
    for (n, (_, reading)) in cases.iter().enumerate() {
        assert_eq!(
            keys(&index, &format!("a:\"{reading}\"")),
            [format!("case{n}")],
            "{reading}"
        );
    }
}

#[test]
fn aliases_keep_person_boundaries_and_field_hints() {
    let index = SnapshotIndex::new(snapshot(vec![record(
        "people",
        json!({
            "title": "Invented observations", "container-title": "Invented journal",
            "author": [{"family": "かざね", "given": "しゅう"}, {"family": "みやの", "given": "りか"}]
        }),
    )]));
    for query in ["a:kazane a:rika", "a:\"kazane shuu\"", "a:\"miyano rika\""] {
        assert_eq!(keys(&index, query), ["people"], "{query}");
    }
    for query in [
        "a:\"shuu miyano\"",
        "a:\"みやのりか kazane\"",
        "t:kazane",
        "j:rika",
        "k:kazane",
    ] {
        assert!(keys(&index, query).is_empty(), "{query}");
    }
}

#[test]
fn original_keys_and_exact_titles_outrank_readings() {
    let index = SnapshotIndex::new(snapshot(vec![
        person("author", "かざね"),
        person("kazane", "りか"),
        record("title", json!({"title": "Kazane"})),
        record("かな", json!({"title": "Invented", "author": [{"literal": "りか"}]})),
    ]));
    assert_eq!(keys(&index, "kazane"), ["kazane", "title", "author"]);
    assert_eq!(keys(&index, "a:kazane"), ["author"]);
    assert_eq!(keys(&index, "かな")[0], "かな");
    // A kana citation key never acquires a romanised alias or an exact-key score.
    assert!(keys(&index, "kana").is_empty());
    let hits = index.search("kazane", 10, None, &|| false).unwrap();
    assert_eq!(hits[0].matched, [MatchField::Citekey]);
    assert_eq!(hits[1].matched, [MatchField::Title]);
    assert_eq!(hits[2].matched, [MatchField::Author]);
}

#[test]
fn refreshed_and_rebuilt_readings_agree_after_insert_edit_and_delete() {
    let before = SnapshotIndex::new(snapshot(vec![
        person("a", "かざね"),
        person("b", "きゃの"),
        person("d", "りか"),
    ]));
    let after = snapshot(vec![
        person("b", "きゃの"),
        person("d", "ぎゅの"),
        person("e", "かっこ"),
    ]);
    let updated = SnapshotIndex::updated(after.clone(), &before);
    let rebuilt = SnapshotIndex::new(after);
    for (query, expected) in [
        ("a:kazane", vec![]),
        ("a:kyano", vec!["b"]),
        ("a:rika", vec![]),
        ("a:gyuno", vec!["d"]),
        ("a:kakko", vec!["e"]),
    ] {
        let read = |index: &SnapshotIndex| {
            index
                .search(query, 10, None, &|| false)
                .unwrap()
                .into_iter()
                .map(|hit| (hit.item.clone(), hit.score, hit.matched, hit.typo, hit.partial))
                .collect::<Vec<_>>()
        };
        assert_eq!(keys(&updated, query), expected, "{query}");
        assert_eq!(read(&updated), read(&rebuilt), "{query}");
    }
}

#[test]
fn unsupported_scripts_spellings_and_oversized_names_get_no_reading() {
    // No Han-name inference, Chinese/Korean readings, mixed-script names,
    // iteration marks, loanword small-vowel combinations or malformed small kana.
    for name in [
        "山田",
        "王",
        "김",
        "山田かな",
        "かなゝ",
        "ティナ",
        "ゃの",
        "かっ",
        "っあ",
        "ーかな",
        &"か".repeat(257),
    ] {
        let index = SnapshotIndex::new(snapshot(vec![person("unsupported", name)]));
        for query in [
            "a:yamada", "a:wang", "a:gim", "a:kana", "a:tina", "a:yano", "a:ka", "a:a",
        ] {
            assert!(keys(&index, query).is_empty(), "{name}: {query}");
        }
    }
    let index = SnapshotIndex::new(snapshot(vec![person("bounded", &"か".repeat(256))]));
    assert_eq!(keys(&index, &format!("a:\"{}\"", "ka".repeat(256))), ["bounded"]);
    let index = SnapshotIndex::new(snapshot(vec![record(
        "fields",
        json!({"title": "かざね", "keyword": "かざね"}),
    )]));
    assert!(keys(&index, "kazane").is_empty());
}
