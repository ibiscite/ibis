//! A host's normal write permission cannot widen a read-only health request.
#![allow(clippy::unwrap_used)]
use ibis_engine::{
    Cancellation, Engine,
    health::{self, InspectionPolicy},
};

#[test]
fn read_only_inspection_never_creates_a_key_even_when_the_host_allows_one() {
    let directory = tempfile::tempdir().unwrap();
    let key = directory.path().join("config/library-ids.key");
    ibis_engine::identity::use_key_file(key.clone(), true);
    let file = directory.path().join("invented.bib");
    std::fs::write(&file, "@misc{invented,title={Invented}}").unwrap();
    let engine = Engine::new().unwrap();
    let first = health::inspect_file(InspectionPolicy::ReadOnly, &file, &Cancellation::default()).unwrap();
    assert_eq!(health::classify(&first), ("ok", "source-ok"));
    assert!(!ibis_engine::identity::ids_last());
    let second = health::inspect_file(InspectionPolicy::ReadOnly, &file, &Cancellation::default()).unwrap();
    assert_eq!(first.id, second.id);
    assert!(!key.exists());
    assert!(!key.parent().unwrap().exists());
    assert_eq!(
        std::fs::read_to_string(file).unwrap(),
        "@misc{invented,title={Invented}}"
    );
    assert!(
        engine.libraries().unwrap().libraries.is_empty(),
        "inspection never installs a source in its host"
    );
}
