//! Same results from every front end (#1200): each golden case runs through
//! every front end that exposes a method, and the envelopes must be
//! byte-identical. The picker's row (its search rows, preview and copy) is
//! `apps/desktop/src-tauri/src/library/engine_file/parity_tests.rs`.
use ibis_core::contract::*;
use ibis_engine::protocol::{read_frame, write_frame};
use ibis_engine::{Cancellation, Engine, Method};
use serde_json::{Value, json};
use std::{io::Cursor, sync::Arc};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Clone, Copy, PartialEq)]
enum FrontEnd {
    /// Typed engine dispatch, as the app calls it.
    Engine,
    /// `ibis <method> ... --json` with human arguments.
    Human,
    /// `ibis cite --request --json`.
    Request,
    /// `ibis call <method> --stdin --json`.
    Call,
    /// `ibis serve --stdio`, one JSON-RPC frame.
    Serve,
}

/// The methods each front end exposes. `schema` and `call` describe the
/// front ends themselves and are compared through `call` and `serve` only.
const EXPOSES: [(FrontEnd, &[Method]); 5] = [
    (
        FrontEnd::Engine,
        &[
            Method::Libraries,
            Method::Search,
            Method::Show,
            Method::Cite,
            Method::Styles,
            Method::Schema,
        ],
    ),
    (FrontEnd::Human, &[Method::Libraries, Method::Search, Method::Styles]),
    (FrontEnd::Request, &[Method::Cite]),
    (
        FrontEnd::Call,
        &[
            Method::Libraries,
            Method::Search,
            Method::Show,
            Method::Cite,
            Method::Styles,
            Method::Schema,
        ],
    ),
    (
        FrontEnd::Serve,
        &[
            Method::Libraries,
            Method::Search,
            Method::Show,
            Method::Cite,
            Method::Styles,
            Method::Schema,
        ],
    ),
];

fn cases() -> Result<Vec<GoldenCase>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cases");
    let mut cases = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "json") {
            cases.push(serde_json::from_slice(&std::fs::read(path)?)?);
        }
    }
    cases.sort_by(|a: &GoldenCase, b| a.name.cmp(&b.name));
    assert!(cases.len() >= 10, "golden cases found");
    Ok(cases)
}

/// One engine per case, with each library published as its own source.
fn engine(case: &GoldenCase) -> Result<Engine> {
    let engine = Engine::new().map_err(|_| "engine")?.without_file_watches();
    let mut libraries: Vec<_> = case
        .library
        .objects()
        .iter()
        .map(|item| item.object.id().library.clone())
        .collect();
    libraries.dedup();
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
    Ok(engine)
}

/// The human arguments for a method, where the front end has them.
fn human(method: Method, params: &Value) -> Option<Vec<String>> {
    let mut args = vec!["ibis".to_owned(), method.as_str().to_owned()];
    match method {
        Method::Libraries | Method::Styles => (),
        Method::Search => {
            args.push(params["query"].as_str()?.to_owned());
            args.extend(["--limit".to_owned(), params["limit"].to_string()]);
        }
        _ => return None,
    }
    args.push("--json".to_owned());
    Some(args)
}

fn envelope(result: std::result::Result<Value, Value>) -> Value {
    match result {
        Ok(data) => json!({"ibis": 1, "ok": true, "data": data}),
        Err(error) => json!({"ibis": 1, "ok": false, "error": error}),
    }
}

fn run(engine: &Engine, args: Vec<String>, input: Vec<u8>) -> Result<Value> {
    let mut output = vec![];
    let mut errors = vec![];
    ibis::run_with(
        engine.clone(),
        args.into_iter().map(Into::into).collect(),
        Cursor::new(input),
        &mut output,
        &mut errors,
        ibis::OutputMode::Plain,
    );
    Ok(serde_json::from_slice(&output)?)
}

/// The envelope a front end gives for one call, or `None` when it does not
/// expose the method.
fn call(front_end: FrontEnd, engine: &Engine, method: Method, params: &Value) -> Result<Option<Value>> {
    if !EXPOSES
        .iter()
        .any(|(end, methods)| *end == front_end && methods.contains(&method))
    {
        return Ok(None);
    }
    let body = serde_json::to_vec(params)?;
    Ok(Some(match front_end {
        FrontEnd::Engine => envelope(
            engine
                .dispatch(method, params, &Cancellation::default())
                .map(|result| result.data)
                .map_err(|error| json!({"code": error.code})),
        ),
        FrontEnd::Human => match human(method, params) {
            Some(args) => run(engine, args, vec![])?,
            None => return Ok(None),
        },
        FrontEnd::Request => run(
            engine,
            ["ibis", "cite", "--request", "--json"].map(Into::into).to_vec(),
            body,
        )?,
        FrontEnd::Call => run(
            engine,
            vec![
                "ibis".into(),
                "call".into(),
                method.as_str().into(),
                "--stdin".into(),
                "--json".into(),
            ],
            body,
        )?,
        FrontEnd::Serve => {
            let mut input = vec![];
            write_frame(
                &mut input,
                &json!({"jsonrpc": "2.0", "id": 7, "method": method.as_str(), "params": params}),
            )?;
            let mut output = vec![];
            ibis::run_with(
                engine.clone(),
                ["ibis", "serve", "--stdio"].map(Into::into).to_vec(),
                Cursor::new(input),
                &mut output,
                &mut vec![],
                ibis::OutputMode::Plain,
            );
            let mut frames = Cursor::new(output);
            let mut reply = None;
            while let Some(frame) = read_frame(&mut frames)? {
                if frame["id"] == json!(7) {
                    assert!(reply.is_none(), "one reply");
                    reply = Some(frame);
                }
            }
            let reply = reply.ok_or("serve reply")?;
            match reply.get("result") {
                Some(result) => envelope(Ok(result.clone())),
                None => envelope(Err(reply["error"]["data"].clone())),
            }
        }
    }))
}

/// Every exposing front end gives exactly `expected`, byte for byte.
fn same(case: &str, engine: &Engine, method: Method, params: &Value, expected: &Value) -> Result {
    let expected = ibis_engine::wire::encode(expected, 16 * 1024 * 1024)?;
    for (front_end, _) in EXPOSES {
        if let Some(actual) = call(front_end, engine, method, params)? {
            assert_eq!(
                String::from_utf8(ibis_engine::wire::encode(&actual, 16 * 1024 * 1024)?)?,
                String::from_utf8(expected.clone())?,
                "{case}: {} through {front_end:?}",
                method.as_str()
            );
        }
    }
    Ok(())
}

#[test]
fn golden_citations_are_identical_through_every_front_end() -> Result {
    for case in cases()? {
        let engine = engine(&case)?;
        let request = serde_json::to_value(&case.request)?;
        // Key output must equal the seed. Formatted seeds are checked against
        // Zotero by `formatted.rs` and the source goldens; here every front
        // end must give what the engine gives.
        let expected = match case.request.output {
            OutputSpec::Key { .. } => serde_json::to_value(&case.expected)?,
            _ => call(FrontEnd::Engine, &engine, Method::Cite, &request)?.ok_or("engine")?,
        };
        same(&case.name, &engine, Method::Cite, &request, &expected)?;
    }
    Ok(())
}

#[test]
fn reading_methods_are_identical_through_every_front_end() -> Result {
    for case in cases()? {
        let engine = engine(&case)?;
        let item = case.library.objects().first().map(|item| item.object.id().clone());
        let title = case
            .library
            .objects()
            .iter()
            .find_map(|item| match &item.object {
                LibraryObject::Record(record) => record.csl.get("title")?.as_str().map(str::to_owned),
                _ => None,
            })
            .unwrap_or_default();
        let word = title.split_whitespace().next().unwrap_or("synthetic").to_owned();
        let mut calls = vec![
            (Method::Libraries, Value::Null),
            (Method::Search, json!({"query": word, "limit": 5})),
            (Method::Search, json!({"query": "no such synthetic words", "limit": 5})),
            (Method::Styles, json!({})),
            (Method::Schema, json!({"method": "cite"})),
        ];
        if let Some(item) = item {
            calls.push((Method::Show, json!({"item": {"by": "id", "value": item}})));
        }
        calls.push((
            Method::Show,
            json!({"item": {"by": "key", "value": "no-such-synthetic-key"}}),
        ));
        for (method, params) in calls {
            let expected = call(FrontEnd::Engine, &engine, method, &params)?.ok_or("engine")?;
            same(&case.name, &engine, method, &params, &expected)?;
        }
    }
    Ok(())
}
