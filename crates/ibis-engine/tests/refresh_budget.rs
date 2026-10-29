//! Release budgets at 100,000 references (#1369). PR CI selects search_budget;
//! Preview's nightly/release builds select open_refresh_budget, which also
//! measures refresh and concurrent copy latency. Both assert search; only
//! open_refresh_budget asserts open/refresh timing. The companion
//! `refresh_budget.py` runner also reports this process's peak RSS on macOS/Linux.
//! Run with `cargo test -p ibis-engine --release --test refresh_budget -- open_refresh_budget --ignored --nocapture`.
#![cfg(not(debug_assertions))]
#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use ibis_engine::{Cancellation, Engine, Method, request::SearchRequest};
use serde_json::json;
use std::{
    sync::{
        Barrier,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const RECORDS: usize = 100_000;

fn library(edited: usize) -> String {
    (0..RECORDS)
        .map(|index| {
            let title = if index == edited { "Edited synthetic signals" } else { "Synthetic signals" };
            format!(
                "@article{{synthetic{index},title={{{title} {index}}},author={{Researcher{}, Avery and Morgan{}, Ann}},year={{{}}},journal={{Invented Letters}},pages={{1--20}},doi={{10.1000/synthetic{index}}},volume={{{}}},number={{{}}}}}\n",
                index % 5_000,
                index % 701,
                1950 + index % 75,
                index % 12 + 1,
                index % 4 + 1,
            )
        })
        .collect()
}

fn percentile(mut samples: Vec<Duration>, percent: usize) -> Duration {
    samples.sort_unstable();
    samples[(samples.len() * percent / 100).min(samples.len() - 1)]
}

// Captured from the pre-optimization scorer on this exact generated 100k
// library. Each digest covers ordered (native key, score, matched, typo, partial)
// tuples, excluding the temporary file's library ID. Never regenerate these
// from the optimized implementation to make a mismatch pass.
fn assert_original_ranking(engine: &Engine, cancel: &Cancellation) {
    let cases = [
        (
            "researcher42 2001",
            "b566fbfb106d01991f6cadb6e155b6cb363aa2d3a2ad1602c435acf546be2c12",
            34,
            false,
        ),
        (
            "synthetic signals 99999",
            "0609a507dbaf9318f6f5e7f85d894cb903674a6f059ee2291c8a6e2f2e490425",
            1,
            false,
        ),
        (
            "morgan7",
            "5f0870a0915f0df82cf5ddd936c767094e566dfd295a112bae3b737fc757bb45",
            50,
            false,
        ),
        (
            "invented letters",
            "073a5546d87f167b5c592401ed177f4ddad5a2e999455c9ea5921a844a6dc061",
            50,
            false,
        ),
        (
            "avery",
            "85fd5e6c7f758903399aa5a3e6798575a3e64ccdaa01dbcd59b3a25ba368af20",
            50,
            false,
        ),
        (
            "synthetic quasar",
            "6caa73cb874196c899b9cb163cb3b0ffae98d53897097ce6e109a5e438c18480",
            50,
            true,
        ),
        (
            "synthetci signals",
            "508c7c0422fb46be536552859bded63a985fa4fd9ad955208af4b5879bf0ef01",
            50,
            false,
        ),
        (
            "SYNTHETIC42",
            "127cf44a7f7b513456242978c16e46c752662077c97f54bf59a8cee827f914a7",
            50,
            false,
        ),
        (
            "a:researcher42",
            "c2cf137df0fbc484fabbceded0b7f20d2c75dbef5eaf83f97fb6ca532e432e47",
            50,
            false,
        ),
        (
            "j:invented y:2001",
            "ef4416e58861455d1633e3edee41257b7ad597692528275d15a9e53f0622a27f",
            50,
            false,
        ),
        (
            "10.1000/synthetic42",
            "9059acff4fa2c7752593197b8f8fe21006ea0488a222af66e09006cac66bd252",
            1,
            false,
        ),
    ];
    for (query, expected, count, partial) in cases {
        let result = engine
            .search(
                &SearchRequest {
                    query: query.into(),
                    limit: 50,
                    libraries: None,
                    interpret_locator: false,
                },
                cancel,
            )
            .unwrap();
        let ordered: Vec<_> = result
            .hits
            .iter()
            .map(|hit| json!([hit.item.object.id().key, hit.score, hit.matched, hit.typo, hit.partial]))
            .collect();
        assert_eq!(result.hits.len(), count, "{query}");
        assert_eq!(result.hits.iter().any(|hit| hit.partial), partial, "{query}");
        assert_eq!(
            Fingerprint::of_bytes(&serde_json::to_vec(&ordered).unwrap()).as_str(),
            expected,
            "{query}: result order or match metadata changed"
        );
    }
}

#[test]
#[ignore = "100k search gate; PR CI runs explicitly in release mode"]
fn search_budget() {
    measure(false);
}

#[test]
#[ignore = "100k open/refresh gate; Preview runs explicitly in release mode"]
fn open_refresh_budget() {
    measure(true);
}

fn measure(enforce_open_refresh: bool) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("synthetic.bib");
    std::fs::write(&path, library(usize::MAX)).unwrap();
    let input_bytes = std::fs::metadata(&path).unwrap().len();
    let engine = Engine::new().unwrap().without_file_watches();
    let cancel = Cancellation::default();

    let start = Instant::now();
    engine
        .dispatch(Method::Libraries, &json!({"action":"open-file","path":path}), &cancel)
        .unwrap();
    let open = start.elapsed();
    assert_eq!(engine.libraries().unwrap().libraries[0].objects, RECORDS);

    const QUERIES: [&str; 5] = [
        "researcher42 2001",
        "synthetic signals 99999",
        "morgan7",
        "invented letters",
        "avery",
    ];
    const SAMPLES_PER_QUERY: usize = 50;
    let mut cases: [Vec<Duration>; QUERIES.len()] = std::array::from_fn(|_| Vec::with_capacity(SAMPLES_PER_QUERY));
    // Include the first call; no discarded warmup, retries or best-of runs.
    for _ in 0..SAMPLES_PER_QUERY {
        for (query, samples) in QUERIES.iter().zip(&mut cases) {
            let start = Instant::now();
            let result = engine
                .search(
                    &SearchRequest {
                        query: (*query).into(),
                        limit: 50,
                        libraries: None,
                        interpret_locator: false,
                    },
                    &cancel,
                )
                .unwrap();
            assert!(!result.hits.is_empty());
            samples.push(start.elapsed());
        }
    }
    for (query, samples) in QUERIES.iter().zip(&cases) {
        println!(
            "100k search case={query:?} n={} first={:?} p50={:?} p95={:?} p99={:?}",
            samples.len(),
            samples[0],
            percentile(samples.clone(), 50),
            percentile(samples.clone(), 95),
            percentile(samples.clone(), 99)
        );
    }
    let searches: Vec<_> = cases.iter().flatten().copied().collect();
    let search_p95 = percentile(searches.clone(), 95);
    println!(
        "100k search: n={} p50={:?} p95={:?} p99={:?}",
        searches.len(),
        percentile(searches.clone(), 50),
        search_p95,
        percentile(searches, 99)
    );

    assert_original_ranking(&engine, &cancel);

    // Each query must meet the limit; cheap queries cannot mask a slow case.
    for (query, samples) in QUERIES.iter().zip(cases) {
        let p95 = percentile(samples, 95);
        assert!(
            p95 <= Duration::from_millis(20),
            "100k {query:?} p95 {p95:?} exceeds 20 ms"
        );
    }
    assert!(
        search_p95 <= Duration::from_millis(20),
        "100k search p95 {search_p95:?} exceeds 20 ms"
    );
    if !enforce_open_refresh {
        return;
    }

    let cited: Vec<_> = ["synthetic1", "synthetic2"]
        .iter()
        .map(|key| {
            let shown = engine
                .dispatch(Method::Show, &json!({"item":{"by":"key","value":key}}), &cancel)
                .unwrap();
            json!({"item":shown.data["object"]["data"]["id"],"expected_cite":shown.data["fingerprints"]["cite"]})
        })
        .collect();
    let request: CitationRequest = serde_json::from_value(json!({
        "items": cited,
        "output": {"kind":"key","syntax":"pandoc","command":null},
        "keep_order": true,
    }))
    .unwrap();
    let prepared = engine.prepare_citation(&request, &cancel).unwrap();
    let hash = prepared.result().content_hash.clone();

    std::fs::write(&path, library(RECORDS / 2)).unwrap();
    let refreshing = AtomicBool::new(true);
    let ready = Barrier::new(2);
    let (refresh, commits) = std::thread::scope(|scope| {
        let copier = scope.spawn(|| {
            let mut samples = vec![];
            ready.wait();
            while refreshing.load(Ordering::Relaxed) {
                let start = Instant::now();
                engine
                    .commit_citation(&prepared, &hash, |content| content.text.len())
                    .unwrap();
                samples.push(start.elapsed());
                std::thread::sleep(Duration::from_millis(1));
            }
            samples
        });
        ready.wait();
        let start = Instant::now();
        let result = engine.dispatch(Method::Libraries, &json!({"action":"reload"}), &cancel);
        let refresh = start.elapsed();
        refreshing.store(false, Ordering::Relaxed);
        result.unwrap();
        (refresh, copier.join().unwrap())
    });
    let edited = engine
        .dispatch(Method::Search, &json!({"query":"edited synthetic","limit":5}), &cancel)
        .unwrap();
    assert_eq!(edited.data["hits"].as_array().unwrap().len(), 1);
    let copy_max = commits.iter().max().copied().unwrap_or_default();
    let copy_p99 = percentile(commits.clone(), 99);
    println!(
        "100k budget: input={}bytes open={}ms refresh={}ms copy-during-refresh p99={}µs max={}µs (n={}) search-p95={}ms",
        input_bytes,
        open.as_millis(),
        refresh.as_millis(),
        copy_p99.as_micros(),
        copy_max.as_micros(),
        commits.len(),
        search_p95.as_millis(),
    );
    assert!(open < Duration::from_secs(3), "100k open {open:?} exceeds 3 s");
    assert!(refresh < Duration::from_secs(3), "100k refresh {refresh:?} exceeds 3 s");
}
