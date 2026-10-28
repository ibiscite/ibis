use ibis_engine::{
    Engine,
    protocol::{read_frame, write_frame},
    server::serve,
};
use serde_json::json;
use std::io::{BufReader, Cursor};

#[test]
fn stdio_drains_requests_at_eof_without_replies_to_notifications() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = vec![];
    for message in [
        json!({"jsonrpc":"2.0","id":"one","method":"libraries"}),
        json!({"jsonrpc":"2.0","id":2,"method":"search","params":{"query":"nothing"}}),
        json!({"jsonrpc":"2.0","method":"unknown"}),
        json!({"jsonrpc":"2.0","id":3,"method":"unknown"}),
    ] {
        write_frame(&mut input, &message)?;
    }
    let mut output = vec![];
    serve(
        Engine::new().map_err(|_| "engine")?,
        BufReader::new(Cursor::new(input)),
        &mut output,
    )?;
    let mut reader = Cursor::new(output);
    let mut responses = vec![];
    while let Some(message) = read_frame(&mut reader)? {
        assert!(message.get("id").is_some(), "unexpected notification: {message}");
        responses.push(message);
    }
    assert_eq!(responses.len(), 3);
    assert!(
        responses
            .iter()
            .any(|response| response["id"] == "one" && response["result"]["libraries"] == json!([]))
    );
    assert!(
        responses
            .iter()
            .any(|response| response["id"] == 2 && response["result"]["hits"] == json!([]))
    );
    assert!(
        responses
            .iter()
            .any(|response| response["id"] == 3 && response["error"]["code"] == -32601)
    );
    Ok(())
}

#[test]
fn batch_frames_get_one_invalid_request_error() -> Result<(), Box<dyn std::error::Error>> {
    for batch in [
        json!([]),
        json!([{"jsonrpc":"2.0","id":1,"method":"libraries"}]),
        json!([{"jsonrpc":"2.0","method":"libraries"}]),
        json!([
            {"jsonrpc":"2.0","id":1,"method":"libraries"},
            {"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":1}},
            false
        ]),
    ] {
        let mut input = vec![];
        write_frame(&mut input, &batch)?;
        let mut output = vec![];
        serve(Engine::new().map_err(|_| "engine")?, Cursor::new(input), &mut output)?;
        let mut reader = Cursor::new(output);
        let response = read_frame(&mut reader)?.ok_or("missing error")?;
        assert_eq!(response["jsonrpc"], "2.0");
        assert!(response.get("id").is_some_and(serde_json::Value::is_null));
        assert_eq!(response["error"]["code"], -32600);
        assert_eq!(response["error"]["message"], "Invalid request");
        assert_eq!(read_frame(&mut reader)?, None);
    }
    Ok(())
}
