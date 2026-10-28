//! Benchmark, not a gate: a 100,000-reference library (#1251) in release. It
//! prints first open, a one-record refresh, the delay a concurrent copy sees,
//! search p95, and asserts only that the refresh landed. The companion
//! `refresh_budget.py` runner also reports this process's peak RSS on macOS/Linux.
//! Run with `cargo test -p ibis-engine --release --test refresh_budget -- --ignored --nocapture`.
#![cfg(not(debug_assertions))]
#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use ibis_engine::{Cancellation, Engine, Method};
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

#[test]
#[ignore = "100k release measurement; run alone with --ignored --nocapture"]
fn a_hundred_thousand_references_refresh_without_stalling_copy() {
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

    let searches: Vec<_> = [
        "researcher42 2001",
        "synthetic signals 99999",
        "morgan7",
        "invented letters",
        "avery",
    ]
    .iter()
    .cycle()
    .take(50)
    .map(|query| {
        let start = Instant::now();
        engine
            .dispatch(Method::Search, &json!({"query":query,"limit":50}), &cancel)
            .unwrap();
        start.elapsed()
    })
    .collect();
    let search_p95 = percentile(searches, 95);

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
}
