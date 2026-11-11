//! A receipt describes the citation a client received, even after newer work.
#![allow(clippy::unwrap_used)]
use ibis_core::contract::{CitationResult, ErrorCode, LibraryObject};
use ibis_engine::{Cancellation, DispatchResult, Engine, Method, mcp};
use serde_json::{Value, json};

fn library() -> (tempfile::TempDir, Engine, Value) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invented.bib");
    std::fs::write(
        &path,
        "@book{one,title={Invented Signals},author={Taylor, Avery},year={2026}}\n",
    )
    .unwrap();
    let engine = Engine::new().unwrap().without_file_watches();
    engine.open_file_observed(&path, &Cancellation::default()).unwrap();
    let snapshot = engine.snapshot().unwrap();
    let item = &snapshot.objects()[0];
    let request = json!({"items":[{"item":item.object.id(),"expected_cite":item.fingerprints.cite}],
        "output":{"kind":"preset","name":"pandoc"}});
    (dir, engine, request)
}

fn call(engine: &Engine, request: &Value) -> Result<ibis_engine::DispatchResult, ibis_core::contract::EngineError> {
    engine.dispatch(Method::Cite, request, &Cancellation::default())
}

fn tool(engine: &Engine, arguments: Value) -> Value {
    let mut session = mcp::Session::default();
    mcp::handle(
        engine,
        &mut session,
        &json!({"jsonrpc":"2.0","id":1,"method":"initialize",
        "params":{"protocolVersion":mcp::LEGACY_VERSIONS[0],"capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
    );
    mcp::handle(
        engine,
        &mut session,
        &json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"cite","arguments":arguments}}),
    )
    .unwrap()["result"]
        .clone()
}

fn rpc(engine: &Engine, request: &Value) -> Value {
    let mut input = Vec::new();
    ibis_engine::protocol::write_frame(
        &mut input,
        &json!({"jsonrpc":"2.0","id":1,"method":"cite","params":request}),
    )
    .unwrap();
    let mut output = Vec::new();
    ibis_engine::server::serve(engine.clone(), input.as_slice(), &mut output).unwrap();
    ibis_engine::protocol::read_frame(&mut std::io::Cursor::new(output))
        .unwrap()
        .unwrap()
}

#[test]
fn a_successful_citation_has_the_same_receipt_in_engine_rpc_and_mcp() {
    let (_dir, engine, request) = library();
    let data = call(&engine, &request).unwrap().data;
    let receipt = &data["receipt"];
    assert_eq!(receipt["request"]["items"][0]["item"], request["items"][0]["item"]);
    assert_eq!(
        receipt["request"]["items"][0]["expected_cite"],
        request["items"][0]["expected_cite"]
    );
    assert_eq!(
        receipt["request"]["output"],
        json!({"kind":"key","syntax":"pandoc","command":null})
    );
    assert_eq!(receipt["content_hash"], data["content_hash"]);
    assert_eq!(receipt["sources"][0]["refresh_error"], Value::Null);
    assert!(receipt["sources"][0]["last_successful_read_ms"].as_u64().unwrap() > 0);
    assert!(receipt["ibis_version"].is_string());
    assert!(receipt["meaning_version"].is_number());
    let rpc = rpc(&engine, &request);
    assert_eq!(rpc["result"]["receipt"], *receipt);
    let mcp = tool(&engine, json!({"items":[{"ref":"one"}],"format":"pandoc"}));
    assert_eq!(mcp["isError"], false);
    assert_eq!(mcp["structuredContent"]["receipt"], *receipt);
    let formatted = call(
        &engine,
        &json!({"items":request["items"],"output":{"kind":"csl","style":"apa"}}),
    )
    .unwrap();
    assert!(formatted.data["receipt"]["formatting"]["processor"].is_string());
    assert!(formatted.data["receipt"]["formatting"]["bundle"].is_string());
}

#[test]
fn strict_refuses_losses_and_partial_citations_instead_of_returning_text() {
    let (_dir, engine, mut request) = library();
    request["output"] = json!({"kind":"key","syntax":"keys"});
    request["items"][0]["locator"] = json!({"term":"page","value":"12"});
    assert_eq!(call(&engine, &request).unwrap().data["text"], "one");
    request["strict"] = json!(true);
    assert_eq!(call(&engine, &request).unwrap_err().code, ErrorCode::BadInput);
    let refused = rpc(&engine, &request);
    assert_eq!(refused["error"]["data"]["reason"], "strict-citation");
    assert!(refused.get("result").is_none());
    let mcp = tool(
        &engine,
        json!({"items":[{"ref":"one","locator":"12"}],"format":"keys","strict":true}),
    );
    assert_eq!(mcp["isError"], true);
    assert!(mcp.get("structuredContent").is_none());
    assert!(mcp["content"][0]["text"].as_str().unwrap().contains("Strict"));
    request["items"][0].as_object_mut().unwrap().remove("locator");
    request["items"][0]["expected_cite"] = json!(ibis_core::contract::Fingerprint::of_bytes(b"changed"));
    assert_eq!(call(&engine, &request).unwrap_err().code, ErrorCode::BadInput);
}

#[test]
fn failed_sources_remain_citable_with_evidence_but_strict_refuses_them() {
    let (dir, engine, mut request) = library();
    std::fs::remove_file(dir.path().join("invented.bib")).unwrap();
    assert!(
        engine
            .dispatch(
                Method::Libraries,
                &json!({"action":"reload","library":request["items"][0]["item"]["library"]}),
                &Cancellation::default()
            )
            .is_err()
    );
    let ordinary = call(&engine, &request).unwrap();
    assert_eq!(ordinary.data["text"], "[@one]");
    assert_eq!(
        ordinary.data["receipt"]["sources"][0]["refresh_error"],
        "source-unavailable"
    );
    request["strict"] = json!(true);
    assert_eq!(call(&engine, &request).unwrap_err().code, ErrorCode::SourceUnavailable);
}

#[test]
fn strict_checks_only_the_cited_sources_and_refuses_their_warnings() {
    let (dir, engine, mut request) = library();
    let damaged = dir.path().join("invented-damaged.bib");
    std::fs::write(
        &damaged,
        "@misc{two,title={Other Invented Work}}\n@misc{broken,title={Missing brace}\n",
    )
    .unwrap();
    engine.open_file_observed(&damaged, &Cancellation::default()).unwrap();
    request["strict"] = json!(true);
    let healthy = call(&engine, &request).unwrap();
    assert_eq!(u8::from(healthy.exit_code), 0);
    let item = engine
        .show(&ibis_engine::request::ShowRequest {
            item: ibis_engine::request::ItemSelector::Key("two".into()),
        })
        .unwrap();
    request["items"] = json!([{"item":item.object.id()}]);
    assert_eq!(call(&engine, &request).unwrap_err().code, ErrorCode::BadInput);
    request["strict"] = json!(false);
    assert!(
        call(&engine, &request).unwrap().data["receipt"]["sources"][0]["warnings"]
            .as_u64()
            .unwrap()
            > 0
    );
}

#[test]
fn receipt_size_counts_the_text_copy_structured_copy_and_modern_frame() {
    let (_dir, engine, _request) = library();
    let mut low = 0;
    let mut high = mcp::MAX_ANSWER;
    let mut largest = Value::Null;
    while low < high {
        let length = (low + high).div_ceil(2);
        let reply = mcp::handle(&engine, &mut mcp::Session::default(), &json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":"cite","arguments":{"items":[{"ref":"one","prefix":"x".repeat(length)}],"format":"pandoc"},
                "_meta":{"io.modelcontextprotocol/protocolVersion":mcp::PROTOCOL_VERSION,"io.modelcontextprotocol/clientCapabilities":{}}}
        })).unwrap();
        assert!(serde_json::to_vec(&reply).unwrap().len() < mcp::MAX_ANSWER);
        if reply["result"]["isError"] == false {
            low = length;
            largest = reply;
        } else {
            high = length - 1;
        }
    }
    assert!(low > 0);
    assert!(largest["result"]["structuredContent"]["receipt"].is_object());
    let oversized_id = mcp::handle(
        &engine,
        &mut mcp::Session::default(),
        &json!({
            "jsonrpc":"2.0","id":"x".repeat(300_000),"method":"tools/call",
            "params":{"name":"cite","arguments":{"items":[{"ref":"one"}]}}
        }),
    )
    .unwrap();
    assert_eq!(oversized_id["id"], Value::Null);
    assert!(oversized_id.get("error").is_some());
    assert!(serde_json::to_vec(&oversized_id).unwrap().len() < mcp::MAX_ANSWER);
}

#[test]
fn an_older_prepared_result_keeps_its_own_records_and_checks_after_a_newer_result() {
    let (dir, engine, request) = library();
    let cancel = Cancellation::default();
    let earlier = engine
        .prepare_citation(&serde_json::from_value(request.clone()).unwrap(), &cancel)
        .unwrap();
    let before = serde_json::to_value(earlier.result()).unwrap();
    std::fs::write(
        dir.path().join("invented.bib"),
        "@book{one,title={New Invented Signals},year={2027}}\n",
    )
    .unwrap();
    engine
        .dispatch(Method::Libraries, &json!({"action":"reload"}), &cancel)
        .unwrap();
    let mut current = request;
    current["items"][0].as_object_mut().unwrap().remove("expected_cite");
    let newer = call(&engine, &current).unwrap();
    assert_ne!(
        newer.data["receipt"]["request"]["items"],
        before["receipt"]["request"]["items"]
    );
    std::fs::remove_file(dir.path().join("invented.bib")).unwrap();
    assert!(
        engine
            .dispatch(
                Method::Libraries,
                &json!({"action":"reload","library":current["items"][0]["item"]["library"]}),
                &cancel
            )
            .is_err()
    );
    let delayed = engine
        .finalize(DispatchResult::citation(earlier.into_result(), 1).unwrap())
        .unwrap();
    assert_eq!(delayed.data, before);
    assert!(!delayed.source_findings);
    assert_eq!(u8::from(delayed.exit_code), 0);
    let parsed: CitationResult = serde_json::from_value(delayed.data).unwrap();
    assert!(parsed.copy_content(1).is_some());
    assert!(matches!(
        engine.snapshot().unwrap().objects()[0].object,
        LibraryObject::Record(_)
    ));
}
