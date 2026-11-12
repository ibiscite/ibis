#![allow(clippy::unwrap_used)]
use ibis_engine::{Engine, mcp};
use serde_json::json;

#[test]
fn readiness_works_without_libraries_and_does_not_forward_startup_errors() {
    let engine = Engine::new().unwrap().without_file_watches();
    let mut session = mcp::Session::without_libraries(mcp::Refusal::new(
        ibis_core::contract::ErrorCode::SourceUnavailable,
        "private /invented/private/settings.json https://private.example note",
    ));
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
        "name":"status","arguments":{},"_meta":{"io.modelcontextprotocol/protocolVersion":mcp::PROTOCOL_VERSION,"io.modelcontextprotocol/clientCapabilities":{}}
    }});
    let result = mcp::handle(&engine, &mut session, &request).unwrap();
    assert_eq!(result["result"]["isError"], false);
    let status = &result["result"]["structuredContent"];
    assert_eq!(status["version"], ibis_core::VERSION);
    assert_eq!(status["libraries_chosen"], false);
    assert_eq!(status["libraries"], json!([]));
    for secret in ["private", "settings.json", "https://", "note"] {
        assert!(!status.to_string().contains(secret));
    }
}
