//! Acceptance checks for the engine contract scaffolding (ADR 0028).
use std::path::Path;

#[test]
fn engine_crates_and_golden_cases_exist() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for name in ["ibis-sources", "ibis-csl", "ibis-engine"] {
        assert!(
            root.join("crates").join(name).join("Cargo.toml").is_file(),
            "missing {name}"
        );
    }
    let cases = std::fs::read_dir(root.join("fixtures/cases"))?;
    assert_eq!(
        cases
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            .count(),
        10
    );
    Ok(())
}

#[test]
fn generated_schema_matches_checked_in_contract() -> Result<(), Box<dyn std::error::Error>> {
    let checked: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/engine-contract.schema.json"))?;
    assert_eq!(
        serde_json::to_value(ibis_core::contract::schema())?,
        checked,
        "regenerate schema after an intentional contract change"
    );
    Ok(())
}

#[test]
fn golden_cases_parse_and_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    use ibis_core::contract::GoldenCase;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cases");
    let mut names = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let wire = std::fs::read_to_string(path)?;
        let case: GoldenCase = serde_json::from_str(&wire)?;
        assert!(names.insert(case.name.clone()));
        if let Some(result) = case.expected.data() {
            assert!(result.content_matches_hash(), "{}: hash", case.name);
            assert_eq!(
                result.outcomes.len(),
                case.request.items.len(),
                "{}: outcomes",
                case.name
            );
        }
        assert_eq!(
            serde_json::from_str::<GoldenCase>(&serde_json::to_string(&case)?)?,
            case
        );
    }
    assert_eq!(names.len(), 10);
    Ok(())
}
