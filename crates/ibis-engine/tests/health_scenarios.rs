//! Health is an engine decision; clients supply native selection facts only.
#![allow(clippy::unwrap_used)]
use ibis_core::contract::{ErrorCode, ExitCode, LibraryId, LibraryObject};
use ibis_engine::{
    Cancellation, Engine, Method,
    health::{self, InspectionPolicy},
};
use serde_json::json;

#[test]
fn healthy_empty_stale_and_failed_switch_keep_the_searchable_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invented.bib");
    std::fs::write(&path, "@misc{kept,title={Invented retained work}}").unwrap();
    let engine = Engine::new().unwrap().without_file_watches();
    engine.open_file_observed(&path, &Cancellation::default()).unwrap();
    let (snapshot, sources) = engine.health().unwrap();
    assert_eq!(health::classify(&sources[0]), ("ok", "source-ok"));
    assert_eq!(health::counts(&snapshot).values().sum::<usize>(), 1);
    // A rejected switch never publishes; the searched snapshot and source stay current.
    let missing = engine
        .open_file_observed(&dir.path().join("missing.bib"), &Cancellation::default())
        .unwrap_err();
    assert!(health::retains(&missing, false));
    let (retained, current) = engine.health().unwrap();
    assert_eq!(retained.fingerprints(), snapshot.fingerprints());
    assert_eq!(current[0].id, sources[0].id);
    assert_eq!(health::counts(&retained).values().sum::<usize>(), 1);
    assert!(
        engine
            .snapshot()
            .unwrap()
            .objects()
            .iter()
            .any(|item| matches!(item.object, LibraryObject::Record(_)))
    );
    std::fs::write(&path, "").unwrap();
    engine.open_file_observed(&path, &Cancellation::default()).unwrap();
    let (empty_snapshot, empty) = engine.health().unwrap();
    assert_eq!(health::classify(&empty[0]), ("ok", "source-ok"));
    assert_eq!(health::counts(&empty_snapshot).values().sum::<usize>(), 0);
}

#[test]
fn search_health_follows_the_consulted_snapshots_across_a_failed_refresh() {
    let dir = tempfile::tempdir().unwrap();
    let healthy = dir.path().join("invented-healthy.bib");
    let failed = dir.path().join("invented-deleted.bib");
    std::fs::write(&healthy, b"@misc{one,title={Invented Lantern}}\n").unwrap();
    std::fs::write(&failed, b"@misc{two,title={Invented Retained}}\n").unwrap();
    let engine = Engine::new().unwrap().without_file_watches();
    let cancel = Cancellation::default();
    engine.open_file_observed(&healthy, &cancel).unwrap();
    let first_id = engine.libraries().unwrap().libraries.remove(0).id;
    engine.open_file_observed(&failed, &cancel).unwrap();
    let second = engine
        .libraries()
        .unwrap()
        .libraries
        .into_iter()
        .find(|library| library.id != first_id)
        .unwrap();

    let search = |params| engine.dispatch(Method::Search, &params, &cancel).unwrap();
    let lantern = json!({"query":"lantern"});
    let earlier = search(lantern.clone());
    std::fs::remove_file(failed).unwrap();
    let refusal = engine
        .dispatch(
            Method::Libraries,
            &json!({"action":"reload","library":second.id}),
            &cancel,
        )
        .unwrap_err();
    assert_eq!(refusal.code, ErrorCode::SourceUnavailable);

    let clean = json!({"status":"ok","findings":[]});
    let stale = json!({"status":"findings","findings":[{
        "code":"refresh-failed", "library":second.id, "error":"source-unavailable",
        "last_successful_read_ms":second.updated_at_ms
    }]});
    assert_eq!(earlier.data["source_health"], clean);
    assert_eq!(earlier.exit_code, ExitCode::Ok);
    assert!(!earlier.source_findings);
    let all = search(lantern);
    assert_eq!(all.data["source_health"], stale, "a consulted source had no hits");
    assert_eq!(all.exit_code, ExitCode::Findings);
    assert!(all.source_findings);
    assert_eq!(all.data["hits"], earlier.data["hits"]);
    assert_eq!(all.data["snapshot"], earlier.data["snapshot"]);

    let scoped = search(json!({"query":"retained","libraries":[second.id,second.id]}));
    assert_eq!(scoped.data["source_health"], stale);
    assert_eq!(
        scoped.data["hits"][0]["item"]["object"]["data"]["csl"]["title"],
        "Invented Retained"
    );
    let healthy = search(json!({"query":"lantern","libraries":[first_id]}));
    assert_eq!(healthy.data["source_health"], clean);
    assert_eq!(healthy.exit_code, ExitCode::Ok);
    let none = search(json!({"query":"lantern","libraries":[]}));
    assert_eq!(none.data["hits"], json!([]));
    assert_eq!(none.data["source_health"], clean);
    assert_eq!(none.exit_code, ExitCode::Ok);
    assert_eq!(
        engine
            .dispatch(
                Method::Search,
                &json!({"query":"lantern","libraries":[first_id,LibraryId::digest(b"invented unknown")]}),
                &cancel,
            )
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
}

#[test]
fn search_reports_source_warnings_as_a_count_without_the_damaged_entry() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invented-damaged.bib");
    std::fs::write(
        &path,
        b"@misc{one,title={Invented Lantern}}\n@misc{privatevalue,title={Missing brace}\n",
    )
    .unwrap();
    let engine = Engine::new().unwrap().without_file_watches();
    let cancel = Cancellation::default();
    engine.open_file_observed(&path, &cancel).unwrap();
    let library = engine.libraries().unwrap().libraries.remove(0);
    let result = engine
        .dispatch(Method::Search, &json!({"query":"lantern"}), &cancel)
        .unwrap();
    assert_eq!(result.exit_code, ExitCode::Findings);
    assert_eq!(result.data["hits"].as_array().unwrap().len(), 1);
    assert_eq!(library.warnings_total, 1);
    assert_eq!(
        result.data["source_health"],
        json!({"status":"findings","findings":[{
            "code":"source-warnings", "library":library.id, "count":1
        }]})
    );
}

#[test]
fn cancelled_health_inspection_does_not_read_or_mutate_a_source() {
    let cancel = Cancellation::default();
    cancel.cancel();
    let engine = Engine::new().unwrap();
    assert_eq!(
        health::inspect_identity(&cancel).unwrap_err().code,
        ErrorCode::Cancelled
    );
    assert_eq!(
        health::inspect_file(
            InspectionPolicy::ReadOnly,
            std::path::Path::new("/invented/never-read.bib"),
            &cancel
        )
        .unwrap_err(),
        "cancelled"
    );
    assert!(engine.libraries().unwrap().libraries.is_empty());
}
