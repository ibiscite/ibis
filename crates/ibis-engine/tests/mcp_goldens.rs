#![allow(clippy::unwrap_used)]
//! Every canonical golden case an agent can ask for gives the same text,
//! warnings, notices and losses through `ibis mcp` as through the command line.
use ibis_core::contract::*;
use ibis_engine::{Cancellation, Engine, mcp};
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc};

#[test]
fn golden_cases_replay_through_mcp() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cases");
    let mut replayed = 0;
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let case: GoldenCase = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let Some(arguments) = arguments(&case.request) else {
            continue;
        };
        let engine = Engine::new().unwrap();
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
                .publish_library(library, Arc::new(Snapshot::new(objects).unwrap()))
                .unwrap();
        }
        let reply = mcp::handle(
            &engine,
            &json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"cite","arguments":arguments}}),
        )
        .unwrap();
        let answer = &reply["result"];
        // The command line answers `cite --request` with this same call; key
        // cases also match their stored golden there (citation_input.rs).
        let cli = engine.cite(&case.request, &Cancellation::default());
        // Only a complete citation is handed to an agent; anything less is
        // a tool error, as the command line exits non-zero.
        match cli.as_ref().ok().and_then(|data| {
            data.copy_content(case.request.items.len())
                .map(|content| (data, content))
        }) {
            Some((data, content)) => {
                assert_eq!(answer["isError"], false, "{}", case.name);
                let mut expected = json!({"text": content.text, "notices": data.notices, "losses": data.losses});
                // Nonempty warnings are part of citation feedback, including
                // APA's sentence-case reminder for the bibliography case.
                // Keep the established response shape for clean citations.
                if !data.warnings.is_empty() {
                    expected["warnings"] = json!(data.warnings);
                }
                assert_eq!(answer["structuredContent"], expected, "{}", case.name);
                let text: Value = serde_json::from_str(answer["content"][0]["text"].as_str().unwrap()).unwrap();
                assert_eq!(text, expected, "{}: text response", case.name);
            }
            None => assert_eq!(answer["isError"], true, "{}", case.name),
        }
        replayed += 1;
    }
    assert!(replayed >= 6, "only {replayed} golden cases fit the cite tool");
}

/// The `cite` tool arguments for a request, when the tool can express it:
/// whole items, one form, and an output the tool offers. The tool keeps the
/// order of key citations and sorts formatted ones.
fn arguments(request: &CitationRequest) -> Option<Value> {
    // The tool always reads English input and writes the style's default
    // en-US locale, which the cases spell out.
    if request
        .input_language
        .as_deref()
        .is_some_and(|language| language != "en")
        || request.output_locale.as_deref().is_some_and(|locale| locale != "en-US")
        || request.items.iter().any(|item| {
            item.form.is_some()
                || item.locator.is_some()
                || !item.prefix.is_empty()
                || !item.suffix.is_empty()
                || item.suppress_author
                || item.annotation.is_some()
        })
    {
        return None;
    }
    let mut arguments = json!({
        "items": request.items.iter().map(|item| json!({"id": item.item})).collect::<Vec<_>>(),
        "form": request.form,
    });
    match &request.output {
        OutputSpec::Key { syntax, command: None } if request.keep_order || request.items.len() < 2 => {
            let syntax = serde_json::to_value(syntax).unwrap();
            let offered = mcp::handle(
                &Engine::new().unwrap(),
                &json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
            )
            .unwrap()["result"]["tools"][2]["inputSchema"]["properties"]["syntax"]["enum"]
                .as_array()
                .unwrap()
                .contains(&syntax);
            if !offered {
                return None;
            }
            arguments["syntax"] = syntax;
        }
        OutputSpec::Csl { style } if !request.keep_order || request.items.len() < 2 => {
            arguments["style"] = json!(style)
        }
        _ => return None,
    }
    Some(arguments)
}
