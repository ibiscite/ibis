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
    for (bibliography, expected) in [
        (false, "(Taylor, 2026)"),
        (true, "Taylor, A. (2026). Synthetic signals. Imaginary Press."),
    ] {
        let mut args = vec!["ibis", "cite", "Example2026", "--style", "apa", "--json"];
        if bibliography {
            args.push("--bibliography");
        }
        let mut output = vec![];
        let mut errors = vec![];
        let code = ibis::run_with(
            synthetic_engine()?,
            args.into_iter().map(Into::into).collect(),
            Cursor::new(vec![]),
            &mut output,
            &mut errors,
            ibis::OutputMode::Plain,
        );
        assert_eq!(code, 0);
        assert!(errors.is_empty());
        let result: Value = serde_json::from_slice(&output)?;
        assert_eq!(result["data"]["text"], expected);
        assert!(result["data"]["html"].is_string());
        assert!(result["data"]["rtf"].is_string());
    }
    Ok(())
}
