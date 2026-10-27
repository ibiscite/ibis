//! Manual search scale measurement on synthetic libraries: index memory and
//! build time, and per-keystroke latency while a query is typed.
//! `cargo test --release -p ibis-core --test search_scale -- --ignored --nocapture`
#![allow(clippy::unwrap_used)]
use std::sync::Arc;
use std::time::{Duration, Instant};

use ibis_core::{contract::*, search::SnapshotIndex};
use serde_json::json;

/// Resident memory in bytes (Linux; zero elsewhere). Deltas around one
/// allocation-heavy step are a fair estimate of what that step keeps.
fn resident() -> isize {
    std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|statm| statm.split(' ').nth(1)?.parse::<isize>().ok())
        .map_or(0, |pages| pages * 4096)
}

/// Deterministic pseudo-random numbers (SplitMix64), so runs compare.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Made-up words built from syllables, so word lengths and letters look like text.
fn words(rng: &mut Rng, count: usize, capitalise: bool) -> Vec<String> {
    const SYLLABLES: [&str; 24] = [
        "ra", "ne", "to", "li", "sa", "mor", "ven", "ka", "di", "lu", "ber", "son", "tha", "mi", "gra", "pel", "or",
        "est", "an", "ic", "ul", "te", "vo", "ri",
    ];
    (0..count)
        .map(|_| {
            let mut word: String = (0..2 + rng.below(3)).map(|_| SYLLABLES[rng.below(24)]).collect();
            if capitalise {
                word[..1].make_ascii_uppercase();
            }
            word
        })
        .collect()
}

fn library(records: usize) -> Vec<LibraryObject> {
    let mut rng = Rng(7);
    let vocabulary = words(&mut rng, 4000, false);
    let families = words(&mut rng, 3000, true);
    let given = words(&mut rng, 400, true);
    let journals = words(&mut rng, 300, true);
    let library = LibraryId::file(b"synthetic scale library");
    (0..records)
        .map(|n| {
            let mut pick = |list: &[String], count: usize| -> Vec<String> {
                (0..count).map(|_| list[rng.below(list.len())].clone()).collect()
            };
            let title = pick(&vocabulary, 6 + n % 9).join(" ");
            let abstract_text = pick(&vocabulary, 80 + n % 120).join(" ");
            let keywords = pick(&vocabulary, n % 5).join(", ");
            let authors: Vec<_> = (0..1 + n % 6)
                .map(|_| json!({"family": pick(&families, 1)[0], "given": pick(&given, 1)[0]}))
                .collect();
            let journal = format!("Journal of {}", pick(&journals, 2).join(" "));
            let year = 1950 + (n * 7919) % 77;
            let family = authors[0]["family"].as_str().unwrap().to_lowercase();
            let mut record = Record {
                id: ItemRef {
                    library: library.clone(),
                    key: format!("item{n}"),
                },
                csl: Default::default(),
                citation_keys: vec![CitationKey {
                    value: format!("{family}{year}{n}"),
                    source: CitationKeySource::Bibtex,
                }],
                aliases: vec![],
                identifiers: vec![],
                raw: None,
            };
            for (field, value) in [
                ("type", json!("article-journal")),
                ("title", json!(title)),
                ("abstract", json!(abstract_text)),
                ("keyword", json!(keywords)),
                ("author", json!(authors)),
                ("container-title", json!(journal)),
                ("issued", json!({"date-parts": [[year]]})),
                ("DOI", json!(format!("10.5555/synthetic.{n}"))),
            ] {
                record.csl.insert(field.into(), value);
            }
            LibraryObject::Record(record)
        })
        .collect()
}

fn mib(bytes: isize) -> String {
    format!("{:.1}", bytes as f64 / 1_048_576.0)
}

#[test]
#[ignore = "manual synthetic scale measurement; no timing assertion"]
fn search_memory_and_typing_latency() {
    for records in [10_000, 100_000] {
        let before = resident();
        let snapshot = Arc::new(Snapshot::new(library(records)).unwrap());
        let with_snapshot = resident();
        let started = Instant::now();
        let index = SnapshotIndex::new(snapshot.clone());
        let build = started.elapsed();
        let with_index = resident();
        println!(
            "records={records} snapshot_rss_mib={} (as JSON {}) index_mib={} (rss {}) index_build_ms={}",
            mib(with_snapshot - before),
            mib(serde_json::to_vec(&*snapshot).unwrap().len() as isize),
            mib(index.heap_bytes() as isize),
            mib(with_index - with_snapshot),
            build.as_millis()
        );
        // Take a real record's words so every query has answers: one whose
        // first author's surname is long enough for a typo to be forgiven.
        let family_of = |object: &SnapshotObject| match &object.object {
            LibraryObject::Record(record) => record.csl["author"][0]["family"].as_str().unwrap().to_lowercase(),
            _ => panic!("record"),
        };
        let target = snapshot.objects()[records / 2..]
            .iter()
            .find(|object| family_of(object).len() >= 6)
            .unwrap();
        let family = family_of(target);
        let LibraryObject::Record(target) = &target.object else {
            panic!("record")
        };
        let title_word = target.csl["title"]
            .as_str()
            .unwrap()
            .split(' ')
            .nth(2)
            .unwrap()
            .to_owned();
        let year = target.csl["issued"]["date-parts"][0][0].to_string();
        let typed = format!("{family} {year} {title_word}");
        let mut samples = Vec::new();
        for _ in 0..5 {
            for end in 1..=typed.len() {
                let started = Instant::now();
                let hits = index.search(&typed[..end], 20, None, &|| false).unwrap();
                samples.push(started.elapsed());
                std::hint::black_box(hits);
            }
        }
        report("typing", &mut samples);
        for probe in [
            "qqqqqq",
            "qqqqqq xxxxxx",
            "zz",
            "ra",
            "orlidi",
            "orlidi qqqqqq",
            "ra ta ka qqqqqq",
        ] {
            let started = Instant::now();
            for _ in 0..5 {
                std::hint::black_box(index.search(probe, 20, None, &|| false).unwrap());
            }
            println!("  probe {probe:?} mean_us={}", started.elapsed().as_micros() / 5);
        }
        // A misspelt surname (two letters swapped), typed letter by letter.
        let mut swapped: Vec<char> = family.chars().collect();
        swapped.swap(3, 4);
        let swapped: String = swapped.into_iter().collect();
        let mut samples = Vec::new();
        for _ in 0..5 {
            for end in 1..=swapped.len() {
                let started = Instant::now();
                std::hint::black_box(index.search(&swapped[..end], 20, None, &|| false).unwrap());
                samples.push(started.elapsed());
            }
        }
        report("typo", &mut samples);
        let found = index.search(&swapped, 20, None, &|| false).unwrap();
        println!(
            "  typo {swapped:?} for {family:?}: {} hits, target found: {}",
            found.len(),
            found.iter().any(|hit| hit.item.object.id() == &target.id)
        );
    }
}

fn report(name: &str, samples: &mut [Duration]) {
    samples.sort_unstable();
    let at = |q: f64| samples[((samples.len() - 1) as f64 * q) as usize].as_micros();
    println!(
        "  {name}: keystrokes={} p50_us={} p95_us={} max_us={}",
        samples.len(),
        at(0.5),
        at(0.95),
        at(1.0)
    );
}
