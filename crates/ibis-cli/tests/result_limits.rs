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
