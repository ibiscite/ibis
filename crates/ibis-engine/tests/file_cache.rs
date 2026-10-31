#![allow(clippy::unwrap_used)]
use ibis_core::contract::ErrorCode::{Cancelled, SourceUnavailable};
use ibis_core::contract::*;
use ibis_engine::{Cancellation, Engine, FileCacheSession, Method};
use serde_json::{Value, json};
use std::path::Path;

const BIB: &str = "@book{beta,title={Synthetic stars},author={Invented, B},year={2025}}\n@book{alpha,title={Synthetic stars},author={Invented, A},year={2026}}\n";
fn engine() -> Engine {
    Engine::new().unwrap().without_file_watches()
}
fn prepare(
    reader: &Engine,
    cache: &FileCacheSession,
    path: &Path,
    strict: bool,
    id: u64,
) -> Result<ibis_engine::PreparedFileLoad, EngineError> {
    reader.prepare_file_load(path, strict, cache.operation(id), &Cancellation::default())
}
fn accept(engine: &mut Engine, cache: &FileCacheSession, path: &Path, id: u64) -> bool {
    cache.invalidate(id);
    let prepared = prepare(engine, cache, path, false, id).unwrap();
    let hit = prepared.cache_hit();
    install(engine, prepared);
    if hit {
        let refreshed = prepare(engine, cache, path, false, id).unwrap();
        install(engine, refreshed);
    }
    hit
}
fn install(engine: &mut Engine, prepared: ibis_engine::PreparedFileLoad) {
    let candidate = prepared.engine().unwrap();
    candidate.accept_file_load(prepared).unwrap();
    *engine = candidate;
}
fn call(engine: &Engine, method: Method, params: Value) -> Value {
    engine.dispatch(method, &params, &Cancellation::default()).unwrap().data
}
fn manifest(storage: &Path) -> std::path::PathBuf {
    std::fs::read_dir(storage)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "json"))
        .unwrap()
}
fn fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invented.bib");
    let storage = dir.path().join("cache");
    std::fs::write(&path, BIB).unwrap();
    (dir, path, storage)
}

#[test]
fn unchanged_cached_verification_shares_records_but_never_readiness() {
    let (_dir, path, storage) = fixture();
    let cache = FileCacheSession::open(&storage);
    let mut reader = engine();
    accept(&mut reader, &cache, &path, 1);
    let before = visible(&reader);
    let snapshot = reader.snapshot().unwrap();
    let mut request: CitationRequest = serde_json::from_value(json!({
        "items":[{"item":snapshot.objects()[0].object.id(),"expected_cite":snapshot.objects()[0].fingerprints.cite}],
        "output":{"kind":"csl","style":"apa"}
    }))
    .unwrap();
    let formatted = reader
        .prepare_citation(&request, &Cancellation::default())
        .unwrap()
        .into_result();
    cache.invalidate(2);
    let pending = prepare(&reader, &cache, &path, false, 2).unwrap();
    assert!(pending.cache_hit());
    let cached = pending.engine().unwrap();
    cached.accept_file_load(pending).unwrap();
    assert_eq!(
        call(&cached, Method::Search, json!({"query":"synthetic"})),
        before["search"]
    );
    let show = json!({"item":{"by":"id","value":snapshot.objects()[0].object.id()}});
    assert_eq!(
        call(&cached, Method::Show, show.clone()),
        call(&reader, Method::Show, show)
    );
    expect_error(
        cached.prepare_citation(&request, &Cancellation::default()),
        SourceUnavailable,
    );
    let previous = cached.snapshot().unwrap();
    let verified = prepare(&cached, &cache, &path, false, 2).unwrap();
    assert!(!verified.cache_hit());
    install(&mut reader, verified);
    for (old, new) in previous.objects().iter().zip(reader.snapshot().unwrap().objects()) {
        assert!(
            std::sync::Arc::ptr_eq(old, new),
            "verification duplicated a canonical payload"
        );
    }
    assert_eq!(visible(&reader), before);
    let prepared = reader.prepare_citation(&request, &Cancellation::default()).unwrap();
    assert_eq!(prepared.result(), &formatted);
    assert_eq!(
        reader
            .commit_citation(&prepared, &formatted.content_hash, |content| content.text.to_owned())
            .unwrap(),
        formatted.text
    );
    request.output = OutputSpec::Key {
        syntax: KeySyntax::Pandoc,
        command: None,
    };
    expect_error(
        cached.prepare_citation(&request, &Cancellation::default()),
        SourceUnavailable,
    );
}

#[test]
fn verification_rereads_changed_deleted_or_unavailable_sources_and_refuses_old_picks() {
    for change in ["changed", "deleted", "unavailable"] {
        let (_dir, path, storage) = fixture();
        let cache = FileCacheSession::open(&storage);
        let mut reader = engine();
        accept(&mut reader, &cache, &path, 1);
        cache.invalidate(2);
        let pending = prepare(&reader, &cache, &path, false, 2).unwrap();
        assert!(pending.cache_hit());
        install(&mut reader, pending);
        let old = reader.snapshot().unwrap();
        let request = serde_json::from_value(json!({
            "items":[{"item":old.objects()[0].object.id(),"expected_cite":old.objects()[0].fingerprints.cite}],
            "output":{"kind":"key","syntax":"pandoc","command":null}
        }))
        .unwrap();
        let accepted = std::fs::read(manifest(&storage)).unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        if change == "unavailable" {
            std::fs::remove_file(&path).unwrap();
        } else {
            let text = if change == "changed" {
                BIB.replace("stars", "moons")
            } else {
                BIB.lines().next().unwrap().to_owned()
            };
            std::fs::write(&path, text).unwrap();
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(modified)
                .unwrap();
        }
        let fresh = prepare(&reader, &cache, &path, false, 2).unwrap();
        if change == "unavailable" {
            expect_error(reader.accept_file_load(fresh), SourceUnavailable);
            assert_eq!(*reader.snapshot().unwrap(), *old);
            expect_error(
                reader.prepare_citation(&request, &Cancellation::default()),
                SourceUnavailable,
            );
            assert_eq!(std::fs::read(manifest(&storage)).unwrap(), accepted);
        } else {
            install(&mut reader, fresh);
            let cited = reader.prepare_citation(&request, &Cancellation::default()).unwrap();
            let expected = if change == "changed" {
                CitationError::ChangedItem
            } else {
                CitationError::MissingItem
            };
            assert_eq!(
                cited.result().outcomes,
                [ItemOutcome::Refused {
                    item: 0,
                    error: expected
                }]
            );
            assert!(cited.result().copy_content(1).is_none());
            assert_eq!(
                call(&reader, Method::Search, json!({"query":"synthetic"}))["hits"]
                    .as_array()
                    .unwrap()
                    .len(),
                if change == "changed" { 2 } else { 1 }
            );
        }
    }
}

#[track_caller]
fn expect_error<T>(result: Result<T, EngineError>, code: ErrorCode) {
    assert_eq!(result.err().unwrap().code, code);
}
fn rewrite_cache(storage: &Path, change: impl FnOnce(&mut Value, &mut Value, &mut Vec<u8>)) {
    let path = manifest(storage);
    let mut manifest: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let payload = storage.join(manifest["file"].as_str().unwrap());
    let mut bytes = std::fs::read(&payload).unwrap();
    let start = manifest["index_start"].as_u64().unwrap() as usize;
    let mut index: Value = serde_json::from_slice(&bytes[start..]).unwrap();
    change(&mut manifest, &mut index, &mut bytes);
    let encoded = serde_json::to_vec(&index).unwrap();
    bytes.truncate(start);
    bytes.extend(&encoded);
    manifest["index_len"] = json!(encoded.len());
    manifest["index_hash"] = json!(Fingerprint::of_bytes(&encoded));
    std::fs::write(payload, bytes).unwrap();
    std::fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
}

#[test]
fn regression_forged_observation_never_grants_citation_readiness() {
    for failed_save in [false, true] {
        let (_dir, path, storage) = fixture();
        let cache = FileCacheSession::open(&storage);
        let mut reader = engine();
        accept(&mut reader, &cache, &path, 1);
        let current = BIB.replace("stars", "moons");
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::fs::write(&path, &current).unwrap();
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(modified).unwrap();
        rewrite_cache(&storage, |_, index, _| {
            index["observation"] = json!(Fingerprint::of_bytes(current.as_bytes()))
        });
        cache.invalidate(2);
        let pending = prepare(&reader, &cache, &path, false, 2).unwrap();
        assert!(pending.cache_hit());
        let unready = pending.engine().unwrap();
        assert!(
            call(&unready, Method::Search, json!({"query":"synthetic"}))
                .to_string()
                .contains("Synthetic stars")
        );
        let request = serde_json::from_value(json!({"items":[{"item":unready.snapshot().unwrap().objects()[0].object.id()}],"output":{"kind":"key","syntax":"pandoc","command":null}})).unwrap();
        expect_error(
            unready.prepare_citation(&request, &Cancellation::default()),
            SourceUnavailable,
        );
        let keys = serde_json::from_value(
            json!({"keys":[{"key":"alpha"}],"output":{"kind":"key","syntax":"pandoc","command":null}}),
        )
        .unwrap();
        expect_error(unready.cite_keys(&keys, &Cancellation::default()), SourceUnavailable);
        unready.accept_file_load(pending).unwrap();
        let manifest_path = manifest(&storage);
        if failed_save {
            cache.disable();
            assert!(!manifest_path.exists(), "an unsaved identity must not leave a cache");
        }
        std::fs::write(&path, format!("{current}@book{{unfinished,")).unwrap();
        let fresh = prepare(&unready, &cache, &path, false, 2).unwrap();
        assert!(!fresh.cache_hit());
        let verified = fresh.engine().unwrap();
        verified.accept_file_load(fresh).unwrap();
        if failed_save {
            assert!(
                !manifest_path.exists(),
                "verification must stay uncached after a failed save"
            );
        }
        assert!(visible(&verified).to_string().contains("Synthetic moons"));
        assert_eq!(verified.libraries().unwrap().libraries[0].warnings_total, 1);
        if failed_save {
            std::fs::write(&path, &current).unwrap();
            let mut eligible = verified.clone();
            assert!(!accept(&mut eligible, &cache, &path, 3));
            assert!(visible(&eligible).to_string().contains("Synthetic moons"));
            assert!(
                !manifest_path.exists(),
                "even eligible verified imports must stay uncached"
            );
        }
        expect_error(
            unready.prepare_citation(&request, &Cancellation::default()),
            SourceUnavailable,
        );
    }
}

#[test]
fn regression_removal_during_startup_hydration_retires_the_configured_source() {
    for startup in ["before-hydration", "hydrated", "moved", "deleted", "retargeted"] {
        let (_dir, path, storage) = fixture();
        let path = path.canonicalize().unwrap(); // The remembered source identity survives unavailability.
        {
            let cache = FileCacheSession::open(&storage);
            accept(&mut engine(), &cache, &path, 1);
        }
        let manifest_path = manifest(&storage);
        let manifest: Value = serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        let payload = storage.join(manifest["file"].as_str().unwrap());
        match startup {
            "moved" => std::fs::rename(&path, path.with_file_name("retired.bib")).unwrap(),
            "deleted" => std::fs::remove_file(&path).unwrap(),
            #[cfg(unix)]
            "retargeted" => {
                let target = path.with_file_name("target.bib");
                std::fs::write(&target, BIB).unwrap();
                std::fs::remove_file(&path).unwrap();
                std::os::unix::fs::symlink(target, &path).unwrap();
            }
            _ => (),
        }
        let cache = FileCacheSession::default();
        cache.invalidate(1);
        cache.operation(1).select_file(&path).unwrap();
        cache.configure(&storage); // Startup captures selection before the app cache directory is opened.
        let pending = (startup == "hydrated").then(|| prepare(&engine(), &cache, &path, false, 1).unwrap());
        assert!(pending.as_ref().is_none_or(ibis_engine::PreparedFileLoad::cache_hit));
        if matches!(startup, "moved" | "deleted") {
            expect_error(prepare(&engine(), &cache, &path, false, 1), SourceUnavailable);
        }
        cache.remove(); // Choosing Zotero before the startup load can be accepted.
        assert!(!manifest_path.exists(), "{startup}");
        assert!(!payload.exists(), "{startup}");
        if let Some(pending) = pending {
            expect_error(pending.engine().unwrap().accept_file_load(pending), Cancelled);
        }
        if startup == "retargeted" {
            std::fs::remove_file(&path).unwrap();
        }
        std::fs::write(&path, BIB).unwrap();
        assert!(!accept(&mut engine(), &cache, &path, 2));
        let moved = path.with_file_name("moved.bib");
        std::fs::rename(&path, &moved).unwrap();
        assert!(!accept(&mut engine(), &cache, &moved, 3));
        cache.invalidate(4);
        expect_error(prepare(&engine(), &cache, &path, false, 4), SourceUnavailable);
    }
}
fn visible(engine: &Engine) -> Value {
    let search = call(engine, Method::Search, json!({"query":"synthetic"}));
    let shown: Vec<_> = search["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| {
            call(
                engine,
                Method::Show,
                json!({"item":{"by":"id","value":hit["item"]["object"]["data"]["id"]}}),
            )
        })
        .collect();
    let request = serde_json::from_value(
        json!({"keys":[{"key":"beta"},{"key":"alpha"}],"output":{"kind":"key","syntax":"pandoc","command":null}}),
    )
    .unwrap();
    let cited = engine.cite_keys(&request, &Cancellation::default()).unwrap();
    assert_eq!(cited.copy_content(2).unwrap().text, "[@beta; @alpha]");
    let mut info = call(engine, Method::Libraries, json!({"action":"list"}));
    for library in info["libraries"].as_array_mut().unwrap() {
        library.as_object_mut().unwrap().remove("updated_at_ms");
    }
    json!({"search":search,"show":shown,"citation":cited,"diagnostics":info})
}

#[test]
fn unknown_or_corrupt_cache_rebuilds_all_objects_before_citation() {
    for damage in ["format", "index", "object", "fingerprint"] {
        let (_dir, path, storage) = fixture();
        let cache = FileCacheSession::open(&storage);
        let mut engine = engine();
        accept(&mut engine, &cache, &path, 1);
        let before = visible(&engine);
        rewrite_cache(&storage, |manifest, index, bytes| match damage {
            "format" => manifest["format"] = json!(999),
            "index" => index["rows"][0]["offset"] = json!(u64::MAX),
            "object" => bytes[8] ^= 1,
            _ => index["rows"][1]["fingerprints"]["cite"] = json!(Fingerprint::of_bytes(b"forged")),
        });
        assert!(!accept(&mut engine, &cache, &path, 2), "{damage}");
        assert_eq!(visible(&engine), before, "{damage}");
    }
}

#[test]
fn delayed_cancelled_removed_and_closed_loads_publish_neither_engine_nor_manifest() {
    for strict in [false, true] {
        for interruption in ["newer", "cancel", "remove", "close"] {
            let (_dir, path, storage) = fixture();
            let cache = FileCacheSession::open(&storage);
            let mut engine = engine();
            accept(&mut engine, &cache, &path, 1);
            let before = visible(&engine);
            std::fs::write(&path, BIB.replace("stars", "moons")).unwrap();
            cache.invalidate(2);
            let mut verifier = engine.clone();
            if !strict {
                let pending = prepare(&engine, &cache, &path, false, 2).unwrap();
                assert!(pending.cache_hit());
                install(&mut verifier, pending);
            }
            let cancel = Cancellation::default();
            let delayed = verifier
                .prepare_file_load(&path, strict, cache.operation(2), &cancel)
                .unwrap();
            match interruption {
                "newer" => {
                    std::fs::write(&path, BIB).unwrap();
                    accept(&mut engine, &cache, &path, 3);
                }
                "cancel" => cancel.cancel(),
                "remove" => cache.remove(),
                _ => cache.close(),
            }
            let mut delayed = Some(delayed);
            if interruption != "close" {
                let delayed = delayed.take().unwrap();
                expect_error(delayed.engine().unwrap().accept_file_load(delayed), Cancelled);
            }
            assert_eq!(visible(&engine), before);
            std::fs::write(&path, BIB).unwrap();
            drop(cache);
            let reopened = FileCacheSession::open(&storage);
            assert_eq!(accept(&mut engine, &reopened, &path, 1), interruption != "remove");
            if let Some(delayed) = delayed {
                expect_error(delayed.engine().unwrap().accept_file_load(delayed), Cancelled);
            }
            assert_eq!(visible(&engine), before);
            assert!(
                !std::fs::read_dir(storage).unwrap().any(|e| e
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("stage-"))
            );
        }
    }
}

#[test]
fn lease_contention_and_ineligible_imports_keep_ordinary_diagnostics_and_citation() {
    for (extension, suffix, busy) in [
        ("bib", "", true),
        ("bib", "@book{unfinished,", false),
        (
            "bib",
            "@book{attachment,title={Synthetic file},file={invented.pdf}}",
            false,
        ),
        ("bibtex", "", false),
    ] {
        let (dir, _, storage) = fixture();
        std::fs::write(dir.path().join("invented.pdf"), "%PDF-1.4\ninvented").unwrap();
        let path = dir.path().join(format!("fallback.{extension}"));
        let text = format!("{BIB}{suffix}");
        std::fs::write(&path, text).unwrap();
        let ordinary = engine();
        call(&ordinary, Method::Libraries, json!({"action":"open-file","path":path}));
        let _lease = busy.then(|| FileCacheSession::open(&storage));
        let cache = FileCacheSession::open(&storage);
        let mut cached = engine();
        for id in [1, 2] {
            assert!(!accept(&mut cached, &cache, &path, id));
            assert_eq!(visible(&cached), visible(&ordinary));
            assert_eq!(
                serde_json::to_value(cached.snapshot().unwrap()).unwrap(),
                serde_json::to_value(ordinary.snapshot().unwrap()).unwrap()
            );
        }
    }
}

#[test]
fn failed_payload_or_manifest_write_keeps_previous_manifest_and_fresh_visible_results() {
    for failure in ["payload", "manifest"] {
        let (_dir, path, storage) = fixture();
        let cache = FileCacheSession::open(&storage);
        let mut engine = engine();
        accept(&mut engine, &cache, &path, 1);
        let manifest = manifest(&storage);
        let accepted = std::fs::read(&manifest).unwrap();
        if failure == "payload" {
            let used: u64 = std::fs::read_dir(&storage)
                .unwrap()
                .map(|e| e.unwrap().metadata().unwrap().len())
                .sum();
            let file = std::fs::File::create(storage.join("invented-reservation")).unwrap();
            file.set_len(1024 * 1024 * 1024 - used - 8).unwrap();
        } else {
            let used = std::fs::read_dir(&storage).unwrap().count();
            for slot in used..511 {
                std::fs::File::create(storage.join(format!("invented-{slot}"))).unwrap();
            }
        }
        std::fs::write(&path, BIB.replace("stars", "moons")).unwrap();
        assert!(accept(&mut engine, &cache, &path, 2));
        assert!(visible(&engine).to_string().contains("Synthetic moons"));
        assert_eq!(std::fs::read(manifest).unwrap(), accepted);
    }
}

#[test]
#[ignore = "fresh-process cached verification measurement; refresh_budget.py --cached"]
fn cached_verification_measurement() {
    use std::time::{Duration, Instant};
    let source = std::path::PathBuf::from(std::env::var_os("IBIS_CACHE_SOURCE").unwrap());
    let storage = std::path::PathBuf::from(std::env::var_os("IBIS_CACHE_STORE").unwrap());
    let records: usize = std::env::var("IBIS_CACHE_RECORDS").unwrap().parse().unwrap();
    let seed = std::env::var("IBIS_CACHE_MODE").unwrap() == "seed";
    let start = Instant::now();
    let cache = FileCacheSession::open(&storage);
    let mut reader = engine();
    cache.invalidate(1);
    let pending = prepare(&reader, &cache, &source, false, 1).unwrap();
    assert_eq!(pending.cache_hit(), !seed, "measurement must exercise a seeded cache");
    install(&mut reader, pending);
    assert_eq!(reader.snapshot().unwrap().objects().len(), records);
    if seed {
        return;
    }
    let result = call(&reader, Method::Search, json!({"query":"synthetic signals","limit":5}));
    assert!(!result["hits"].as_array().unwrap().is_empty());
    let first_search = start.elapsed();
    let request = serde_json::from_value(json!({
        "keys":[{"key":"synthetic0"}],"output":{"kind":"key","syntax":"pandoc","command":null}
    }))
    .unwrap();
    expect_error(reader.cite_keys(&request, &Cancellation::default()), SourceUnavailable);
    let verified = prepare(&reader, &cache, &source, false, 1).unwrap();
    assert!(!verified.cache_hit());
    install(&mut reader, verified);
    let cited = reader.cite_keys(&request, &Cancellation::default()).unwrap();
    assert_eq!(cited.copy_content(1).unwrap().text, "[@synthetic0]");
    println!(
        "CACHE_BUDGET {}",
        json!({
            "records":records, "release":!cfg!(debug_assertions), "cached_first_search_ms":first_search.as_secs_f64()*1000.0,
            "verified_ready_ms":start.elapsed().as_secs_f64()*1000.0
        })
    );
    // The external sampler records settled RSS with only the accepted engine alive.
    std::thread::sleep(Duration::from_secs(1));
    std::hint::black_box(&reader);
}
