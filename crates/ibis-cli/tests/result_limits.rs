//! CLI and stdio share one serialized engine result cap, including exact boundaries.
#![cfg(test)]
use ibis_core::contract::*;
use ibis_engine::{Cancellation, Engine, Method, protocol};
use serde_json::{Value, json};
use std::io::Cursor;

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
    // Distinct records: CSL-JSON exports a record cited twice only once.
    let records: Vec<_> = (0..84).map(|n| json!({"id":format!("Synthetic{n}"),"citation-key":format!("Synthetic{n}"),"type":"book","title":"Synthetic size threshold","abstract":"x".repeat(200_000)})).collect();
    std::fs::write(&path, serde_json::to_vec(&records).unwrap()).unwrap();
    let engine = Engine::new().unwrap();
    engine
        .dispatch(
            Method::Libraries,
            &json!({"action":"open-file","path":path}),
            &Cancellation::default(),
        )
        .unwrap();
    let snapshot = engine.snapshot().unwrap();
    let items: Vec<_> = snapshot
        .objects()
        .iter()
        .map(|item| json!({"item":item.object.id()}))
        .collect();
    // Library ids belong to a person's key, which the real command may keep apart from this test's.
    let listed = std::process::Command::new(env!("CARGO_BIN_EXE_ibis"))
        .args(["libraries", "--json", "--file"])
        .arg(&path)
        .output()
        .unwrap();
    let listed: Value = serde_json::from_slice(&listed.stdout).unwrap();
    let library = &listed["data"]["libraries"][0]["id"];
    let native_items: Vec<_> = (0..84)
        .map(|n| json!({"item":{"library":library,"key":format!("record:Synthetic{n}")}}))
        .collect();
    for count in [1, 42, 83, 84] {
        let output = json!({"kind":"key","syntax":"csl-json"});
        let params = json!({"items":items[..count],"output":output});
        let typed = serde_json::from_value(params).unwrap();
        let params = json!({"items":native_items[..count],"output":output});
        assert_eq!(
            ibis_core::syntax::cite(&snapshot, &typed).is_ok(),
            count < 84,
            "renderer boundary at {count}"
        );
        let output = native_cite(&path, &params);
        let response = native_cite_rpc(&path, &params);
        assert_eq!(output.stderr.is_empty(), count == 1, "{count}");
        if count == 1 {
            assert!(output.status.success());
            assert!(response.get("result").is_some());
        } else {
            assert_eq!(output.status.code(), Some(5), "CLI repeats: {count}; RPC: {response}");
            let expected = if count == 84 {
                json!({"code":"resource-limit","citation_error":"output-too-large","message":{"id":"problem-output-too-large","args":{}},"recovery":{"action":"reduce-selection"},"focus":"results"})
            } else {
                json!({"code":"resource-limit","message":{"id":"error-resource-limit","args":{}},"recovery":{"action":"reduce-selection"},"focus":"results"})
            };
            assert_eq!(
                serde_json::from_slice::<Value>(&output.stdout).unwrap(),
                json!({"ibis":1,"ok":false,"error":expected})
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
    assert_eq!(response["error"]["data"]["citation_error"], "invalid-rich-text");
    std::fs::remove_file(path).unwrap();
}
