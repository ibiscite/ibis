use ibis_core::contract::{EngineError, ErrorCode};
use ibis_engine::{
    Method,
    rpc::{Message, RequestId, failure, parse_message, success},
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
fn errors_are_fixed_and_response_ids_stay_separate() {
    let Message::Reply(error) = parse_message(&json!({"jsonrpc":"2.0","id":"safe","method":"secret-request-text"}))
    else {
        panic!("error expected");
    };
    assert_eq!(error["error"]["code"], -32601);
    assert!(!error.to_string().contains("secret-request-text"));
    let id = RequestId::parse(&json!("1")).unwrap_or_else(|| panic!("id"));
    assert_ne!(Some(id.clone()), RequestId::parse(&json!(1)));
    assert_eq!(success(&id, json!([])), json!({"jsonrpc":"2.0","id":"1","result":[]}));
    assert_eq!(
        failure(
            Some(&id),
            EngineError {
                code: ErrorCode::Cancelled
            }
        )["error"]["data"]["code"],
        "cancelled"
    );
    for message in [
        json!({"jsonrpc":2,"id":1,"method":"search"}),
        json!({"jsonrpc":"2.0","id":true,"method":"search"}),
        json!({"jsonrpc":"2.0","id":1,"method":"search","params":"private"}),
    ] {
        assert!(matches!(parse_message(&message), Message::Reply(_)));
    }
}

#[test]
fn explicit_null_id_is_a_request_not_a_notification() {
    let value = json!({"jsonrpc":"2.0","id":null,"method":"search","params":{"query":"synthetic"}});
    let Message::Request(request) = parse_message(&value) else {
        panic!("null-id request expected");
    };
    assert_eq!(request.id.map(|id| id.value()), Some(serde_json::Value::Null));
}

#[test]
fn escaped_request_ids_fit_reserved_overhead_and_oversized_ids_are_not_echoed() {
    let accepted = json!("\u{1}".repeat(1024));
    let id = RequestId::parse(&accepted).unwrap_or_else(|| panic!("bounded ID"));
    let response = failure(
        Some(&id),
        EngineError {
            code: ErrorCode::ResourceLimit,
        },
    );
    assert!(ibis_engine::wire::encode(&response, ibis_engine::protocol::RESPONSE_OVERHEAD).is_ok());
    let rejected = json!({"jsonrpc":"2.0","id":"\u{1}".repeat(1025),"method":"libraries"});
    let Message::Reply(response) = parse_message(&rejected) else {
        panic!("invalid ID response")
    };
    assert!(response["id"].is_null());
    assert_eq!(response["error"]["code"], -32600);
    assert!(!response.to_string().contains("\\u0001"));
}
