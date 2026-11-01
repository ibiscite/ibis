//! Golden cases exercise the same parser/dispatcher used by the native binary.
use ibis_core::contract::*;
use ibis_engine::Engine;
use std::{collections::BTreeSet, io::Cursor, sync::Arc};

#[test]
fn canonical_citation_golden_cases_replay_through_the_cli() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cases");
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let case: GoldenCase = serde_json::from_slice(&std::fs::read(path)?)?;
        if !matches!(case.request.output, OutputSpec::Key { .. }) {
            continue;
        }
        let engine = Engine::new().map_err(|_| "engine")?;
        let libraries: BTreeSet<_> = case
            .library
            .objects()
            .iter()
            .map(|item| item.object.id().library.clone())
            .collect();
        for library in libraries {
            let objects = case
                .library
                .objects()
                .iter()
                .filter(|item| item.object.id().library == library)
                .map(|item| item.object.clone())
                .collect();
            engine
                .publish_library(library, Arc::new(Snapshot::new(objects)?))
                .map_err(|_| "publish")?;
        }
        let mut output = vec![];
        let mut errors = vec![];
        let code = ibis::run_with(
            engine,
            ["ibis", "cite", "--request", "--json"].map(Into::into).to_vec(),
            Cursor::new(serde_json::to_vec(&case.request)?),
            &mut output,
            &mut errors,
            ibis::OutputMode::Plain,
        );
        let expected_code =
            case.expected
                .error()
                .map(|error| error.code.exit_code().value())
                .unwrap_or_else(|| {
                    if case.expected.data().is_some_and(|data| {
                        data.losses.is_empty() && data.copy_content(case.request.items.len()).is_some()
                    }) {
                        0
                    } else {
                        1
                    }
                });
        assert_eq!(code, expected_code, "{}", case.name);
        let mut expected = ibis_engine::wire::encode(&serde_json::to_value(&case.expected)?, 16 * 1024 * 1024)?;
        expected.push(b'\n');
        assert_eq!(output, expected, "{}", case.name);
        // Errors also say what went wrong on stderr, in one line; successes stay silent there.
        if case.expected.error().is_some() {
            let errors = String::from_utf8(errors)?;
            assert!(
                errors.starts_with("ibis: ") && errors.ends_with('\n') && errors.lines().count() == 1,
                "{}: {errors}",
                case.name
            );
        } else {
            assert!(errors.is_empty(), "{}", case.name);
        }
    }
    Ok(())
}
