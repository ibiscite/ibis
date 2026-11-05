//! Heap held after opening 10,000 invented, realistic records (decision 0032,
//! #1536): long author lists, abstracts, and Chinese, Japanese and Korean text.
//! A counting allocator gives the exact bytes requested, so the limit is the
//! same on every runner and needs no timing margin.
//! Run with `cargo test -p ibis-engine --test library_memory -- --nocapture`.
#![allow(clippy::unwrap_used)]
use std::alloc::System;

use ibis_engine::{Cancellation, Engine, Method};
use serde_json::json;
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

const RECORDS: usize = 10_000;
const MIB: usize = 1024 * 1024;

const FAMILY: [&str; 14] = [
    "Vale",
    "Okonkwo",
    "Lindqvist",
    "Marchetti",
    "Haddad",
    "Nakamura",
    "Kowalczyk",
    "Ferreira",
    "Abernathy",
    "Quill",
    "Rasmussen",
    "Delacroix",
    "Oyelaran",
    "Brannigan",
];
const GIVEN: [&str; 10] = [
    "Ada", "Tomas", "Ingrid", "Kofi", "Mirela", "Yusuf", "Hana", "Pavel", "Lena", "Ravi",
];
const CJK_FAMILY: [&str; 7] = ["王", "李", "張", "山田", "佐藤", "김", "박"];
const CJK_GIVEN: [&str; 6] = ["小明", "美玲", "太郎", "花子", "민준", "서연"];
const CJK_TITLE: [&str; 5] = [
    "灯台の研究",
    "潮汐与苔藓",
    "조용한 신호",
    "铜质子午线",
    "群島のアーカイブ",
];
const WORDS: [&str; 20] = [
    "lantern", "harbour", "tidal", "moss", "quiet", "signal", "lattice", "ember", "orchard", "river", "glass",
    "archive", "meridian", "copper", "fern", "atlas", "salt", "beacon", "drift", "hollow",
];
/// Most records have a few authors; some have many, as real libraries do.
const AUTHOR_COUNTS: [usize; 7] = [1, 1, 2, 3, 4, 8, 30];

/// Deterministic pseudo-random numbers (SplitMix64), so every run has the same library.
struct Rng(u64);
impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) % n as u64) as usize
    }
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

struct Invented {
    key: String,
    title: String,
    names: Vec<(&'static str, &'static str)>,
    journal: String,
    year: usize,
    volume: usize,
    first_page: usize,
    abstract_text: String,
}

fn records() -> Vec<Invented> {
    let mut rng = Rng(1536);
    (0..RECORDS)
        .map(|index| {
            let cjk = index % 10 == 0;
            let count = if index % 97 == 0 {
                120
            } else {
                AUTHOR_COUNTS[rng.below(AUTHOR_COUNTS.len())]
            };
            let names = (0..count)
                .map(|_| {
                    if cjk {
                        (rng.pick(&CJK_FAMILY), rng.pick(&CJK_GIVEN))
                    } else {
                        (rng.pick(&FAMILY), rng.pick(&GIVEN))
                    }
                })
                .collect();
            let title = if cjk {
                format!("{} {index}", rng.pick(&CJK_TITLE))
            } else {
                let words: Vec<_> = (0..3 + rng.below(7)).map(|_| rng.pick(&WORDS)).collect();
                format!("{} {index}", words.join(" "))
            };
            let sentence = format!(
                "An invented abstract about {} and {}, written only for testing. ",
                rng.pick(&WORDS),
                rng.pick(&WORDS)
            );
            Invented {
                key: format!("k{index}"),
                title,
                names,
                journal: format!("Journal of {}", rng.pick(&WORDS)),
                year: 1950 + index * 7 % 75,
                volume: index % 40 + 1,
                first_page: index % 300,
                abstract_text: sentence.repeat(6),
            }
        })
        .collect()
}

fn bibtex(records: &[Invented]) -> String {
    records
        .iter()
        .map(|record| {
            let authors: Vec<_> = record
                .names
                .iter()
                .map(|(family, given)| format!("{family}, {given}"))
                .collect();
            format!(
                "@article{{{},\n  author = {{{}}},\n  title = {{{}}},\n  journal = {{{}}},\n  year = {{{}}},\n  volume = {{{}}},\n  pages = {{{}--{}}},\n  doi = {{10.0000/invented.{}}},\n  abstract = {{{}}}\n}}\n\n",
                record.key,
                authors.join(" and "),
                record.title,
                record.journal,
                record.year,
                record.volume,
                record.first_page,
                record.first_page + 12,
                record.key,
                record.abstract_text,
            )
        })
        .collect()
}

fn csl_json(records: &[Invented]) -> String {
    let items: Vec<_> = records
        .iter()
        .map(|record| {
            json!({
                "id": record.key,
                "type": "article-journal",
                "title": record.title,
                "author": record.names.iter().map(|(family, given)| json!({"family": family, "given": given})).collect::<Vec<_>>(),
                "container-title": record.journal,
                "issued": {"date-parts": [[record.year]]},
                "volume": record.volume.to_string(),
                "page": format!("{}-{}", record.first_page, record.first_page + 12),
                "DOI": format!("10.0000/invented.{}", record.key),
                "abstract": record.abstract_text,
            })
        })
        .collect();
    serde_json::to_string(&items).unwrap()
}

/// Bytes still allocated after the library is open and searchable.
fn held_after_open(name: &str, text: String) -> usize {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(name);
    std::fs::write(&path, text).unwrap();
    let cancel = Cancellation::default();
    let region = Region::new(GLOBAL);
    let engine = Engine::new().unwrap().without_file_watches();
    engine
        .dispatch(
            Method::Libraries,
            &json!({"action": "open-file", "path": path}),
            &cancel,
        )
        .unwrap();
    assert_eq!(engine.libraries().unwrap().libraries[0].objects, RECORDS);
    let change = region.change();
    drop(engine);
    change.bytes_allocated.saturating_sub(change.bytes_deallocated)
}

// Measured at 44.0 MiB (.bib) and 44.9 MiB (CSL-JSON) once names were held as
// compact typed names (#1675), down from 86.3 and 87.8 MiB; the same in debug
// and release. The limits leave about a tenth for ordinary changes.
const BIBTEX_LIMIT: usize = 48 * MIB;
const CSL_JSON_LIMIT: usize = 49 * MIB;

#[test]
fn ten_thousand_realistic_records_fit_the_memory_budget() {
    let records = records();
    let (bib, json) = (bibtex(&records), csl_json(&records));
    drop(records);
    let bib_bytes = held_after_open("library.bib", bib);
    let json_bytes = held_after_open("library.json", json);
    println!(
        "10k records held: bibtex={:.1}MiB csl-json={:.1}MiB",
        bib_bytes as f64 / MIB as f64,
        json_bytes as f64 / MIB as f64
    );
    assert!(bib_bytes <= BIBTEX_LIMIT, "BibTeX library holds {bib_bytes} bytes");
    assert!(
        json_bytes <= CSL_JSON_LIMIT,
        "CSL-JSON library holds {json_bytes} bytes"
    );
}
