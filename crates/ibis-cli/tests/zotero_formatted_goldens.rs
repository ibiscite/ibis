//! Invented Zotero items, mapped by Ibis, must format exactly as pinned Zotero
//! code formats them (fixtures/formatted/zotero), through every front end.
use ibis_core::contract::{CitationRequest, CitationResult, LibraryId, LibraryObject, Snapshot};
use ibis_engine::{
    Cancellation, Engine,
    protocol::{read_frame, write_frame},
};
use ibis_sources::zotero::{ZoteroItem, map_item};
use serde_json::{Value, json};
use std::{collections::BTreeSet, io::Cursor, sync::Arc};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn run(engine: Engine, args: Vec<String>, input: Vec<u8>) -> TestResult<Vec<u8>> {
    let mut output = vec![];
    let mut errors = vec![];
    let code = ibis::run_with(
        engine,
        args.into_iter().map(Into::into).collect(),
        Cursor::new(input),
        &mut output,
        &mut errors,
        ibis::OutputMode::Plain,
    );
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&errors));
    assert!(errors.is_empty());
    Ok(output)
}

fn assert_oracle(result: &CitationResult, expected: &Value, context: &str) {
    assert_eq!(result.text, expected["text"], "{context} text");
    assert_eq!(result.html.as_deref(), expected["html"].as_str(), "{context} HTML");
    assert_eq!(result.rtf.as_deref(), expected["rtf"].as_str(), "{context} RTF");
    assert!(result.copy_content(4).is_some(), "{context} lost an item");
}

#[test]
fn zotero_items_match_pinned_zotero_through_engine_cli_and_framed_stdio() -> TestResult {
    let fixture: Value = serde_json::from_str(include_str!("../../../fixtures/formatted/zotero/expected.json"))?;
    let cases = fixture["cases"].as_array().ok_or("cases")?;
    assert_eq!(cases.len(), 10, "five styles, citation and bibliography");
    let library = LibraryId::zotero(b"default:user");
    let mut objects = vec![];
    let mut keys = vec![];
    let mut items = vec![];
    for item in fixture["items"].as_array().ok_or("items")? {
        let item: ZoteroItem = serde_json::from_value(item.clone())?;
        let mapped = map_item(library.clone(), &item).map_err(|error| format!("{error:?}"))?;
        assert!(mapped.warnings.is_empty());
        keys.push(mapped.record.citation_key()?.ok_or("missing fixture key")?.to_owned());
        items.push(json!({"item": mapped.record.id}));
        objects.push(LibraryObject::Record(mapped.record));
    }
    assert_eq!(keys.len(), 4);
    let engine = Engine::from_snapshot(Arc::new(Snapshot::new(objects)?));
    let mut frames = vec![];
    let mut typed = vec![];
    let mut batch_start = 0;
    for (index, case) in cases.iter().enumerate() {
        let style = case["style"].as_str().ok_or("style")?;
        let form = case["form"].as_str().ok_or("form")?;
        let context = format!("{style} {form}");
        let request: CitationRequest = serde_json::from_value(json!({
            "items": items, "form": form, "output": {"kind":"csl","style":style}, "output_locale":"en-US"
        }))?;
        let result = engine
            .cite(&request, &Cancellation::default())
            .map_err(|error| format!("{error:?}"))?;
        assert_oracle(&result, case, &context);
        let mut args = vec!["ibis".into(), "cite".into()];
        args.extend(keys.iter().cloned());
        args.extend(["--style", style, "--locale", "en-US", "--json"].map(Into::into));
        if form == "bibliography-only" {
            args.push("--bibliography".into());
        }
        let envelope: Value = serde_json::from_slice(&run(engine.clone(), args, vec![])?)?;
        assert_eq!(envelope["ok"], true, "{context}");
        let human: CitationResult = serde_json::from_value(envelope["data"].clone())?;
        assert_eq!(human, result, "{context} human CLI changed the canonical result");
        write_frame(
            &mut frames,
            &json!({"jsonrpc":"2.0", "id":index, "method":"cite", "params":request}),
        )?;
        typed.push(result);
        // Stay below the server's bounded in-flight limit.
        if typed.len() % 4 != 0 && index + 1 != cases.len() {
            continue;
        }
        let output = run(
            engine.clone(),
            vec!["ibis".into(), "serve".into(), "--stdio".into()],
            std::mem::take(&mut frames),
        )?;
        let mut replies = Cursor::new(output);
        let mut received = BTreeSet::new();
        while let Some(frame) = read_frame(&mut replies)? {
            let Some(id) = frame.get("id").and_then(Value::as_u64) else {
                continue;
            };
            let id = usize::try_from(id)?;
            assert!(frame.get("error").is_none(), "stdio refused golden {id}");
            let result: CitationResult = serde_json::from_value(frame["result"].clone())?;
            assert_eq!(result, typed[id], "stdio changed golden {id}");
            received.insert(id);
        }
        assert_eq!(received, (batch_start..=index).collect(), "stdio replies");
        batch_start = index + 1;
    }
    Ok(())
}
