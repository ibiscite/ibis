//! Invented source files are compared with fixed, independently generated Zotero output.
use ibis_core::contract::{CitationRequest, CitationResult, LibraryObject};
use ibis_engine::{
    Cancellation, Engine,
    protocol::{read_frame, write_frame},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
    path::Path,
};

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
fn assert_oracle(result: &CitationResult, expected: &Value, context: &str) -> TestResult {
    assert_eq!(result.text, expected["text"], "{context} text");
    assert_eq!(result.html.as_deref(), expected["html"].as_str(), "{context} HTML");
    assert_eq!(result.rtf.as_deref(), expected["rtf"].as_str(), "{context} RTF");
    assert!(result.copy_content(4).is_some(), "{context} lost an item");
    Ok(())
}

#[test]
fn source_files_match_pinned_zotero_through_cli_and_framed_stdio() -> TestResult {
    let fixture: Value = serde_json::from_str(include_str!("../../../fixtures/formatted/source-files/expected.json"))?;
    let cases = fixture["cases"].as_array().ok_or("cases")?;
    let keys = fixture["keys"].as_array().ok_or("keys")?;
    assert_eq!(keys.len(), 4);
    assert_eq!(cases.len(), 10, "five styles, citation and bibliography");
    let styles: BTreeSet<_> = cases.iter().filter_map(|case| case["style"].as_str()).collect();
    assert_eq!(
        styles,
        BTreeSet::from([
            "apa",
            "chicago-author-date",
            "harvard-cite-them-right",
            "ieee",
            "modern-language-association"
        ])
    );
    let pairs: BTreeSet<_> = cases
        .iter()
        .map(|case| (case["style"].as_str(), case["form"].as_str()))
        .collect();
    assert_eq!(pairs.len(), 10, "every style needs both distinct forms");
    assert!(
        cases
            .iter()
            .all(|case| matches!(case["form"].as_str(), Some("normal" | "bibliography-only")))
    );
    for extension in ["bib", "json"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../fixtures/formatted/source-files/synthetic.{extension}"));
        let path = path.to_str().ok_or("fixture path")?.to_owned();
        let engine = Engine::new().map_err(|error| format!("{error:?}"))?;
        // Enter through the real CLI file loader; never install a snapshot fixture.
        run(
            engine.clone(),
            vec![
                "ibis".into(),
                "search".into(),
                "".into(),
                "--file".into(),
                path.clone(),
                "--json".into(),
            ],
            vec![],
        )?;
        let snapshot = engine.snapshot().map_err(|error| format!("{error:?}"))?;
        let mut identities = BTreeMap::new();
        for object in snapshot.objects() {
            if let LibraryObject::Record(record) = &object.object {
                identities.insert(
                    record.citation_key()?.ok_or("missing fixture key")?.to_owned(),
                    record.id.clone(),
                );
            }
        }
        assert_eq!(identities.len(), 4);
        let mut requests = vec![];
        let mut typed = vec![];
        for (index, case) in cases.iter().enumerate() {
            let style = case["style"].as_str().ok_or("style")?;
            let form = case["form"].as_str().ok_or("form")?;
            let context = format!("{extension} {style} {form}");
            let items = keys
                .iter()
                .map(|key| {
                    let key = key.as_str().ok_or("key")?;
                    Ok(json!({"item": identities.get(key).ok_or("source lost a fixture key")?}))
                })
                .collect::<TestResult<Vec<_>>>()?;
            let request: CitationRequest = serde_json::from_value(json!({
                "items": items, "form": form, "output": {"kind":"csl","style":style}, "output_locale":"en-US"
            }))?;
            let result = engine
                .cite(&request, &Cancellation::default())
                .map_err(|error| format!("{error:?}"))?;
            assert_oracle(&result, case, &context)?;
            let mut args = vec!["ibis".into(), "cite".into()];
            args.extend(keys.iter().map(|key| key.as_str().unwrap_or_default().into()));
            args.extend([
                "--style".into(),
                style.into(),
                "--locale".into(),
                "en-US".into(),
                "--file".into(),
                path.clone(),
                "--json".into(),
            ]);
            if form == "bibliography-only" {
                args.push("--bibliography".into());
            }
            let output = run(engine.clone(), args, vec![])?;
            let envelope: Value = serde_json::from_slice(&output)?;
            assert_eq!(envelope["ibis"], 1);
            assert_eq!(envelope["ok"], true);
            let actual: CitationResult = serde_json::from_value(envelope["data"].clone())?;
            assert_oracle(&actual, case, &context)?;
            assert_eq!(actual, result, "{context} human CLI changed the canonical result");
            requests.push(json!({"jsonrpc":"2.0", "id":index, "method":"cite", "params":request}));
            typed.push(result);
        }
        let mut received = BTreeSet::new();
        // Stay below the server's bounded in-flight limit; this is replay,
        // not an overload test. Replies may arrive in any order.
        for chunk in requests.chunks(4) {
            let mut input = vec![];
            for request in chunk {
                write_frame(&mut input, request)?;
            }
            let output = run(
                engine.clone(),
                vec![
                    "ibis".into(),
                    "serve".into(),
                    "--stdio".into(),
                    "--file".into(),
                    path.clone(),
                ],
                input,
            )?;
            let mut frames = Cursor::new(output);
            while let Some(frame) = read_frame(&mut frames)? {
                let Some(id) = frame.get("id").and_then(Value::as_u64) else {
                    continue;
                };
                let index = usize::try_from(id)?;
                assert!(received.insert(index), "duplicate reply");
                assert!(
                    frame.get("error").is_none(),
                    "stdio refused {extension} golden {index}: {:?}",
                    frame.get("error")
                );
                let result: CitationResult = serde_json::from_value(frame["result"].clone())?;
                assert_eq!(
                    result,
                    *typed.get(index).ok_or("unexpected reply")?,
                    "{extension} stdio changed the canonical result"
                );
                assert_oracle(&result, &cases[index], extension)?;
            }
        }
        assert_eq!(received.len(), cases.len());
    }
    Ok(())
}
