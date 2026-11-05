//! Health is an engine decision; clients supply native selection facts only.
#![allow(clippy::unwrap_used)]
use ibis_core::contract::{ErrorCode, LibraryObject};
use ibis_core::source::SourceError;
use ibis_engine::{
    Cancellation, Engine,
    health::{self, InspectionPolicy},
};

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
    assert!(health::retains(
        Some(SourceError::Unavailable),
        health::counts(&snapshot).values().sum::<usize>() > 0,
        false
    ));
    // A rejected switch never publishes; the searched snapshot and source stay current.
    assert!(
        engine
            .open_file_observed(&dir.path().join("missing.bib"), &Cancellation::default())
            .is_err()
    );
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
    assert!(!health::retains(
        Some(SourceError::Unavailable),
        health::counts(&empty_snapshot).values().sum::<usize>() > 0,
        false
    ));
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
