//! The new renderer consumes the shared contract without front-end formatting.
use ibis_core::contract::{GoldenCase, OutputSpec};

#[test]
fn key_golden_cases_render_exactly() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cases");
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let case: GoldenCase = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        if !matches!(case.request.output, OutputSpec::Key { .. }) || case.request.items.is_empty() {
            continue;
        }
        let actual = ibis_core::syntax::cite(&case.library, &case.request)
            .map_err(|error| format!("{}: {error:?}", case.name))?;
        assert_eq!(Some(&actual), case.expected.data(), "{}", case.name);
    }
    Ok(())
}
