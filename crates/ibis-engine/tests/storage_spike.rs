//! Throwaway stage A. Correctness runs without probes; the runner builds a
//! source-identical disposable workspace with timing probes at private boundaries.
#![allow(clippy::unwrap_used, unexpected_cfgs)]
use ibis_core::contract::*;
use ibis_engine::{Cancellation, Engine, FileCacheSession, Method};
use serde_json::{Value, json};
use std::{path::Path, process::Command, time::Instant};

const PYTHON: &str = "/Users/diyanko/miniconda3/bin/python3";
const QUERIES: [&str; 3] = ["signals", "星図", "signlas"];

fn fixture(directory: &Path, count: usize, seed: u64) {
    assert!(
        Command::new(PYTHON)
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/storage_spike/fixtures.py"))
            .arg(directory)
            .args(["--records", &count.to_string(), "--seed", &seed.to_string()])
            .status()
            .unwrap()
            .success()
    );
}
fn call(engine: &Engine, method: Method, params: Value) -> Value {
    engine.dispatch(method, &params, &Cancellation::default()).unwrap().data
}
fn empty() -> Engine {
    Engine::new().unwrap().without_file_watches()
}
fn uncached(path: &Path) -> Engine {
    let engine = empty();
    call(&engine, Method::Libraries, json!({"action":"open-file","path":path}));
    engine
}
fn cached(reader: &Engine, cache: &FileCacheSession, path: &Path, hit: bool) -> Engine {
    let load = reader
        .prepare_file_load(path, false, cache.operation(1), &Cancellation::default())
        .unwrap();
    assert_eq!(load.cache_hit(), hit);
    let engine = load.engine().unwrap();
    engine.accept_file_load(load).unwrap();
    engine
}
fn check_records(engine: &Engine, records: usize, seed: u64, zotero: bool) -> Result<(), String> {
    let snapshot = engine.snapshot().map_err(|_| "snapshot")?;
    if snapshot.objects().len() != records {
        return Err("wrong record count".into());
    }
    let marker = format!("seed{seed} ");
    for object in snapshot.objects() {
        let LibraryObject::Record(record) = &object.object else {
            return Err("non-record".into());
        };
        if !record.csl["title"]
            .as_str()
            .is_some_and(|title| title.contains(&marker))
        {
            return Err("stale fixture seed".into());
        }
        if !record.id.key.starts_with(if zotero { "S" } else { "record:sp" }) {
            return Err("wrong identity".into());
        }
    }
    if engine
        .libraries()
        .unwrap()
        .libraries
        .iter()
        .any(|library| library.warnings_total != 0)
    {
        return Err("fixture parsed with warnings".into());
    }
    Ok(())
}
fn searches(engine: &Engine, records: usize) -> Vec<Value> {
    QUERIES
        .iter()
        .map(|query| {
            let result = call(engine, Method::Search, json!({"query":query,"limit":50}));
            if *query == "signlas" {
                assert!(result["hits"].as_array().unwrap().iter().any(|hit| hit["typo"] == true));
            }
            assert!(valid_hits(
                &result,
                if *query == "星図" {
                    records.div_ceil(10).min(50)
                } else {
                    records.min(50)
                }
            ));
            result
        })
        .collect()
}
fn valid_hits(result: &Value, expected: usize) -> bool {
    result["hits"]
        .as_array()
        .is_some_and(|hits| hits.len() == expected && !hits.is_empty())
}
fn cite(engine: &Engine, seed: u64, index: usize, csl: bool) -> Vec<u8> {
    let request = serde_json::from_value(json!({"keys":[{"key":format!("sp{seed}r{index}")}],
        "output":if csl { json!({"kind":"csl","style":"apa"}) } else { json!({"kind":"key","syntax":"pandoc","command":null}) }})).unwrap();
    let cited = engine.cite_keys(&request, &Cancellation::default()).unwrap();
    cited.copy_content(1).unwrap().text.as_bytes().to_vec()
}

#[test]
fn deterministic_generator_and_byte_identical_cache_oracle() {
    let root = tempfile::tempdir().unwrap();
    let first = root.path().join("first");
    let second = root.path().join("second");
    fixture(&first, 100, 1534);
    fixture(&second, 100, 1534);
    for file in ["library.bib", "api.jsonl", "lean/zotero.sqlite", "manifest.json"] {
        assert_eq!(
            std::fs::read(first.join(file)).unwrap(),
            std::fs::read(second.join(file)).unwrap()
        );
    }
    let path = first.join("library.bib");
    let no_cache = uncached(&path);
    check_records(&no_cache, 100, 1534, false).unwrap();
    let cache_path = root.path().join("cache");
    {
        let cache = FileCacheSession::open(&cache_path);
        cache.invalidate(1);
        cached(&empty(), &cache, &path, false);
    }
    let cache = FileCacheSession::open(&cache_path);
    cache.invalidate(1);
    let hit = cached(&empty(), &cache, &path, true);
    assert_eq!(
        serde_json::to_vec(&searches(&no_cache, 100)).unwrap(),
        serde_json::to_vec(&searches(&hit, 100)).unwrap()
    );
    let request = serde_json::from_value(
        json!({"keys":[{"key":"sp1534r0"}],"output":{"kind":"key","syntax":"pandoc","command":null}}),
    )
    .unwrap();
    assert_eq!(
        hit.cite_keys(&request, &Cancellation::default()).err().unwrap().code,
        ErrorCode::SourceUnavailable
    );
    let verified = cached(&hit, &cache, &path, false);
    for index in [0, 1, 10, 99] {
        for csl in [false, true] {
            assert_eq!(cite(&no_cache, 1534, index, csl), cite(&verified, 1534, index, csl));
        }
    }
    assert_eq!(searches(&no_cache, 100), searches(&verified, 100));
    assert!(check_records(&verified, 99, 1534, false).is_err());
    assert!(check_records(&verified, 100, 1535, false).is_err());
    assert!(!valid_hits(&json!({"hits":[]}), 50));
    assert!(!valid_hits(&searches(&verified, 100)[0], 49));
}

#[test]
fn failed_io_refused_and_malformed_fixtures_cannot_be_timed_as_success() {
    let root = tempfile::tempdir().unwrap();
    for file in ["missing.bib", "invalid.bib"] {
        if file == "invalid.bib" {
            std::fs::write(root.path().join(file), [0xff]).unwrap();
        }
        assert!(
            empty()
                .dispatch(
                    Method::Libraries,
                    &json!({"action":"open-file","path":root.path().join(file)}),
                    &Cancellation::default()
                )
                .is_err()
        );
    }
    let path = root.path().join("malformed.bib");
    std::fs::write(&path, "@article{unfinished,").unwrap();
    let engine = uncached(&path);
    assert!(check_records(&engine, 1, 1534, false).is_err());
    assert!(!valid_hits(
        &call(&engine, Method::Search, json!({"query":"signals"})),
        1
    ));
}

#[test]
#[ignore = "fresh process; storage_spike.py owns fixtures, ordering and RSS sampling"]
fn storage_spike_measurement() {
    let directory = std::path::PathBuf::from(std::env::var_os("IBIS_SPIKE_FIXTURE").unwrap());
    let case = std::env::var("IBIS_SPIKE_CASE").unwrap();
    let records: usize = std::env::var("IBIS_SPIKE_RECORDS").unwrap().parse().unwrap();
    let seed: u64 = std::env::var("IBIS_SPIKE_SEED").unwrap().parse().unwrap();
    let manifest: Value = serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["seed"], seed);
    assert_eq!(manifest["records"], records);
    let path = directory.join("library.bib");
    let storage = directory.join("cache");
    let first_query = std::env::var("IBIS_SPIKE_FIRST_QUERY").unwrap_or_else(|_| QUERIES[0].into());
    assert!(QUERIES.contains(&first_query.as_str()));
    #[cfg(storage_spike_probe)]
    let mut closed_owner = None;
    let start = Instant::now();
    let mut cache = None;
    let mut engine = match case.as_str() {
        "bib-none" => uncached(&path),
        "bib-cache" | "seed" => {
            cache = Some(FileCacheSession::open(&storage));
            cache.as_ref().unwrap().invalidate(1);
            cached(&empty(), cache.as_ref().unwrap(), &path, case == "bib-cache")
        }
        "zotero-live" => {
            let port = std::env::var("IBIS_SPIKE_PORT").unwrap().parse().unwrap();
            let engine = Engine::with_zotero_port(port).unwrap().without_file_watches();
            call(&engine, Method::Libraries, json!({"action":"open-zotero"}));
            engine
        }
        #[cfg(storage_spike_probe)]
        "zotero-lean" | "zotero-fulltext" => {
            let scratch = directory.join("private-copy");
            std::fs::create_dir_all(&scratch).unwrap();
            let mut source = ibis_sources::zotero_source::ZoteroSource::default();
            let loaded = source
                .storage_spike_closed(&directory.join(case.trim_start_matches("zotero-")), &scratch)
                .unwrap();
            let engine = empty();
            for library in loaded {
                engine
                    .publish_library(library.library.clone(), std::sync::Arc::new(library.snapshot.clone()))
                    .unwrap();
            }
            // Production keeps the raw live state, but releases the raw closed read.
            // Keep mapped source ownership just as Engine.resources does.
            closed_owner = Some(source);
            engine
        }
        _ => panic!("unsupported case"),
    };
    let loaded_ms = start.elapsed().as_secs_f64() * 1000.0;
    if case == "seed" {
        check_records(&engine, records, seed, false).unwrap();
        return;
    }
    // Predeclared per-round rotation includes common, CJK and typo first searches.
    let search_start = Instant::now();
    let first = call(&engine, Method::Search, json!({"query":first_query,"limit":50}));
    let first_results_ms = start.elapsed().as_secs_f64() * 1000.0;
    let first_search_ms = search_start.elapsed().as_secs_f64() * 1000.0;
    assert!(valid_hits(&first, records.min(50)));
    println!("STORAGE_FIRST {first_results_ms}");
    #[cfg(storage_spike_probe)]
    let first_stages = ibis_core::storage_spike_probe::take();
    #[cfg(not(storage_spike_probe))]
    let first_stages = json!({});
    let validation_start = Instant::now();
    check_records(&engine, records, seed, case.starts_with("zotero")).unwrap();
    let cached_hit = case == "bib-cache";
    if cached_hit {
        let request = serde_json::from_value(
            json!({"keys":[{"key":format!("sp{seed}r0")}],"output":{"kind":"key","syntax":"pandoc","command":null}}),
        )
        .unwrap();
        assert_eq!(
            engine.cite_keys(&request, &Cancellation::default()).err().unwrap().code,
            ErrorCode::SourceUnavailable
        );
    }
    let validation_ms = validation_start.elapsed().as_secs_f64() * 1000.0;
    if cached_hit {
        engine = cached(&engine, cache.as_ref().unwrap(), &path, false);
    }
    let cited = cite(&engine, seed, 0, false);
    assert_eq!(cited, format!("[@sp{seed}r0]").as_bytes());
    let verified_ready_ms = start.elapsed().as_secs_f64() * 1000.0;
    #[cfg(storage_spike_probe)]
    let verification_stages = ibis_core::storage_spike_probe::take();
    #[cfg(not(storage_spike_probe))]
    let verification_stages = json!({});
    let mut query_timings = Vec::new();
    for query in QUERIES {
        let began = Instant::now();
        let result = call(&engine, Method::Search, json!({"query":query,"limit":50}));
        let elapsed = began.elapsed().as_secs_f64() * 1000.0;
        let expected = if query == "星図" {
            records.div_ceil(10).min(50)
        } else {
            records.min(50)
        };
        assert!(valid_hits(&result, expected));
        query_timings.push(json!({"query":query,"ms":elapsed,"hits":expected}));
    }
    println!(
        "STORAGE_SPIKE {}",
        json!({"case":case,"records":records,"seed":seed,"release":!cfg!(debug_assertions),
        "loaded_ms":loaded_ms,"first_query":first_query,"first_results_ms":first_results_ms,"first_search_ms":first_search_ms,
        "verified_ready_ms":verified_ready_ms,"oracle_validation_ms":validation_ms,
        "stages_ns":first_stages,"verification_stages_ns":verification_stages,"queries":query_timings,
        "record_count":engine.snapshot().unwrap().objects().len(),"first_hits":first["hits"].as_array().unwrap().len()})
    );
    std::thread::sleep(std::time::Duration::from_secs(1));
    std::hint::black_box(&engine);
    #[cfg(storage_spike_probe)]
    std::hint::black_box(&closed_owner);
}
