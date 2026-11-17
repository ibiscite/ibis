use ibis_core::contract::{EngineError, ErrorCode};
use ibis_engine::{
    Method,
    rpc::{Message, RequestId, failure, parse_message},
};
use serde_json::json;

#[test]
fn calls_notifications_and_cancellation_are_distinct() {
    let request = json!({"jsonrpc":"2.0","id":1.0,"method":"search","params":{"query":"文字"}});
    let Message::Request(call) = parse_message(&request) else {
        panic!("request expected");
    };
    assert_eq!(call.method, Method::Search);
    assert_eq!(
        call.id,
        Some(RequestId::parse(&json!(1)).unwrap_or_else(|| panic!("id")))
    );
    assert!(matches!(
        parse_message(&json!({"jsonrpc":"2.0","method":"search"})),
        Message::Request(_)
    ));
    assert!(matches!(
        parse_message(&json!({"jsonrpc":"2.0","method":"unknown"})),
        Message::Ignore
    ));
    assert!(matches!(
        parse_message(&json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":1}})),
        Message::Cancel(_)
    ));
}

#[test]
fn escaped_request_ids_fit_reserved_overhead_and_oversized_ids_are_not_echoed() {
    let accepted = json!("\u{1}".repeat(1024));
    let id = RequestId::parse(&accepted).unwrap_or_else(|| panic!("bounded ID"));
    let response = failure(Some(&id), EngineError::new(ErrorCode::ResourceLimit));
    assert!(ibis_engine::wire::encode(&response, ibis_engine::protocol::RESPONSE_OVERHEAD).is_ok());
    let rejected = json!({"jsonrpc":"2.0","id":"\u{1}".repeat(1025),"method":"libraries"});
    let Message::Reply(response) = parse_message(&rejected) else {
        panic!("invalid ID response")
    };
    assert!(response["id"].is_null());
    assert_eq!(response["error"]["code"], -32600);
    assert!(!response.to_string().contains("\\u0001"));
}
