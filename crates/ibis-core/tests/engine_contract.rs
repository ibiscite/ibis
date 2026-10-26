//! Acceptance checks for the engine contract scaffolding (ADR 0028).
use std::path::Path;

#[test]
fn engine_crates_and_golden_cases_exist() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for name in ["ibis-sources", "ibis-csl", "ibis-engine"] {
        assert!(root.join("crates").join(name).join("Cargo.toml").is_file(), "missing {name}");
    }
    let cases = std::fs::read_dir(root.join("fixtures/cases")).expect("golden cases folder");
    assert_eq!(cases.filter_map(Result::ok).filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json")).count(), 10);
}
