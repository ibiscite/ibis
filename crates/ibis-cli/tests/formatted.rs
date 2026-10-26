use ibis_core::contract::{GoldenCase, Snapshot};
use ibis_engine::Engine;
use serde_json::Value;
use std::{io::Cursor, sync::Arc};

fn synthetic_engine() -> Result<Engine, Box<dyn std::error::Error>> {
    let case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json"))?;
    let snapshot: Snapshot = case.library;
    Ok(Engine::from_snapshot(Arc::new(snapshot)))
}

#[test]
fn human_style_option_formats_with_the_shared_engine() -> Result<(), Box<dyn std::error::Error>> {
    let mut output = vec![];
    let mut errors = vec![];
    let code = ibis::run_with(
        synthetic_engine()?,
        ["ibis", "cite", "Example2026", "--style", "apa", "--json"]
            .map(Into::into)
            .to_vec(),
        Cursor::new(vec![]),
        &mut output,
        &mut errors,
        ibis::OutputMode::Plain,
    );
    assert_eq!(code, 0);
    assert!(errors.is_empty());
    let result: Value = serde_json::from_slice(&output)?;
    assert_eq!(result["data"]["text"], "(Taylor, 2026)");
    assert!(result["data"]["html"].is_string());
    assert!(result["data"]["rtf"].is_string());
    Ok(())
}
