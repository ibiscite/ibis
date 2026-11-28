//! Health is an engine decision; clients supply native selection facts only.
use ibis_core::contract::{ExitCode, LibraryObject};
use ibis_engine::{
    Cancellation, Engine, Method,
    health::{self},
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
fn a_library_read_outside_the_engine_answers_lookups_as_one_it_opened() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invented-damaged.bib");
    std::fs::write(
        &path,
        b"@misc{one,title={Invented Lantern}}\n@misc{privatevalue,title={Missing brace}\n",
    )
    .unwrap();
    let cancel = Cancellation::default();
    let opened = Engine::new().unwrap().without_file_watches();
    opened.open_file_observed(&path, &cancel).unwrap();
    let handed = Engine::new().unwrap().without_file_watches();
    handed
        .publish_library(&ibis_sources::load_file(&path).unwrap())
        .unwrap();
    let answer =
        |engine: &Engine, method: Method, params: serde_json::Value| match engine.dispatch(method, &params, &cancel) {
            Ok(result) => (result.exit_code, result.data["source_health"].clone()),
            Err(error) => (error.code.exit_code(), json!(error.reason)),
        };
    for (method, params) in [
        (Method::Search, json!({"query": "lantern"})),
        (Method::Show, json!({"item": {"by": "key", "value": "privatevalue"}})),
    ] {
        assert_eq!(answer(&opened, method, params.clone()), answer(&handed, method, params));
    }
    assert_eq!(
        answer(
            &handed,
            Method::Show,
            json!({"item": {"by": "key", "value": "privatevalue"}})
        )
        .1,
        json!("damaged-entry")
    );
}
