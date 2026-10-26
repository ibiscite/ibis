//! CLI and stdio share one serialized engine result cap, including exact boundaries.
#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use ibis_engine::{Cancellation, Engine, Method, protocol};
use serde_json::{Value, json};
use std::{io::Cursor, sync::Arc};

fn engine(padding: usize) -> Engine {
    let case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap();
    let LibraryObject::Record(mut record) = case.library.objects()[0].object.clone() else {
        panic!("record")
    };
    record.csl.insert("annote".into(), json!("x".repeat(padding)));
    Engine::from_snapshot(Arc::new(Snapshot::new(vec![LibraryObject::Record(record)]).unwrap()))
}
fn show() -> Value {
    json!({"item":{"by":"key","value":"Example2026"}})
}
fn cli(engine: Engine) -> (u8, Vec<u8>) {
    let mut output = vec![];
    let mut errors = vec![];
    let code = ibis::run_with(
        engine,
        ["ibis", "show", "Example2026", "--json"].map(Into::into).to_vec(),
        Cursor::new(vec![]),
        &mut output,
        &mut errors,
        ibis::OutputMode::Plain,
    );
    assert!(errors.is_empty());
    (code, output)
}
fn rpc(engine: Engine) -> Value {
    // Worst-case JSON escaping still fits the reserved ID/envelope overhead.
    let id = "\u{1}".repeat(1024);
    let mut input = vec![];
    protocol::write_frame(
        &mut input,
        &json!({"jsonrpc":"2.0","id":id,"method":"show","params":show()}),
    )
    .unwrap();
    let mut output = vec![];
    ibis_engine::server::serve(engine, Cursor::new(input), &mut output).unwrap();
    let mut reader = Cursor::new(output);
    let mut result = None;
    while let Some(message) = protocol::read_response_frame(&mut reader).unwrap() {
        if message.get("id").is_some() {
            assert_eq!(message["id"], id);
            result = Some(message);
        }
    }
    result.unwrap()
}
#[test]
fn single_results_match_byte_for_byte_and_overflow_is_a_resource_error() {
    let baseline = engine(0)
        .dispatch(Method::Show, &show(), &Cancellation::default())
        .unwrap();
    let size = ibis_engine::wire::encode(&baseline.data, protocol::MAX_RESULT_BYTES)
        .unwrap()
        .len();
    let padding = protocol::MAX_RESULT_BYTES - size;
    {
        let engine = engine(padding);
        let data = engine
            .dispatch(Method::Show, &show(), &Cancellation::default())
            .unwrap()
            .data;
        let data_bytes = ibis_engine::wire::encode(&data, protocol::MAX_RESULT_BYTES).unwrap();
        assert_eq!(data_bytes.len(), protocol::MAX_RESULT_BYTES);
        let mut expected =
            ibis_engine::wire::encode(&json!({"ibis":1,"ok":true,"data":data}), protocol::MAX_RESPONSE_BYTES).unwrap();
        expected.push(b'\n');
        let (code, output) = cli(engine.clone());
        assert_eq!(code, 0);
        assert!(output == expected, "CLI envelope bytes differ");
        let response = rpc(engine);
        assert!(
            ibis_engine::wire::encode(&response["result"], protocol::MAX_RESULT_BYTES).unwrap() == data_bytes,
            "stdio result bytes differ"
        );
    }
    let engine = engine(padding + 1);
    let Err(error) = engine.dispatch(Method::Show, &show(), &Cancellation::default()) else {
        panic!("oversized result was accepted")
    };
    assert_eq!(error.code, ErrorCode::ResourceLimit);
    let (code, output) = cli(engine.clone());
    assert_eq!(code, 5);
    assert_eq!(
        serde_json::from_slice::<Value>(&output).unwrap(),
        json!({"ibis":1,"ok":false,"error":{"code":"resource-limit"}})
    );
    let response = rpc(engine);
    assert_eq!(response["error"]["code"], -32000);
    assert_eq!(response["error"]["data"]["code"], "resource-limit");
}

fn native_cite(path: &std::path::Path, params: &Value) -> std::process::Output {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .args(["cite", "--request", "--json", "--file"])
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(params).unwrap())
        .unwrap();
    child.wait_with_output().unwrap()
}
fn native_cite_rpc(path: &std::path::Path, params: &Value) -> Value {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut input = vec![];
    protocol::write_frame(
        &mut input,
        &json!({"jsonrpc":"2.0","id":1,"method":"cite","params":params}),
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .args(["serve", "--stdio", "--file"])
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let mut reader = Cursor::new(output.stdout);
    while let Some(message) = protocol::read_response_frame(&mut reader).unwrap() {
        if message.get("id") == Some(&json!(1)) {
            return message;
        }
    }
    panic!("missing synthetic citation response")
}
#[test]
fn renderer_and_transport_quotas_are_resource_errors_on_real_cli_and_stdio() {
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("synthetic-renderer-threshold-{}.json", std::process::id()));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, serde_json::to_vec(&json!([{"id":"Synthetic2026","type":"book","title":"Synthetic size threshold","abstract":"x".repeat(200_000)}])).unwrap()).unwrap();
    let engine = Engine::new().unwrap();
    engine
        .dispatch(
            Method::Libraries,
            &json!({"action":"open-file","path":path}),
            &Cancellation::default(),
        )
        .unwrap();
    let snapshot = engine.snapshot().unwrap();
    let item = json!({"item":snapshot.objects()[0].object.id()});
    for count in [1, 42, 83, 84] {
        let params = json!({"items":vec![item.clone();count],"output":{"kind":"key","syntax":"csl-json"}});
        let typed = serde_json::from_value(params.clone()).unwrap();
        assert_eq!(
            ibis_core::syntax::cite(&snapshot, &typed).is_ok(),
            count < 84,
            "renderer boundary at {count}"
        );
        let output = native_cite(&path, &params);
        let response = native_cite_rpc(&path, &params);
        assert!(output.stderr.is_empty());
        if count == 1 {
            assert!(output.status.success());
            assert!(response.get("result").is_some());
        } else {
            assert_eq!(output.status.code(), Some(5), "CLI repeats: {count}; RPC: {response}");
            assert_eq!(
                serde_json::from_slice::<Value>(&output.stdout).unwrap(),
                json!({"ibis":1,"ok":false,"error":{"code":"resource-limit"}})
            );
            assert_eq!(response["error"]["code"], -32000, "RPC repeats: {count}");
            assert_eq!(response["error"]["data"]["code"], "resource-limit");
        }
    }
    let invalid = json!({"items":[{"item":snapshot.objects()[0].object.id(),"prefix":vec![json!({"text":""});10_001]}],"output":{"kind":"key","syntax":"pandoc"}});
    assert_eq!(
        ibis_core::syntax::cite(&snapshot, &serde_json::from_value(invalid.clone()).unwrap()),
        Err(CitationError::InvalidRichText)
    );
    let output = native_cite(&path, &invalid);
    let response = native_cite_rpc(&path, &invalid);
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["error"]["code"],
        "bad-input"
    );
    assert_eq!(response["error"]["code"], -32602);
    assert_eq!(response["error"]["data"]["code"], "bad-input");
    std::fs::remove_file(path).unwrap();
}
