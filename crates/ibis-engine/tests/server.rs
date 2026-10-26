use ibis_engine::{
    Engine,
    protocol::{read_frame, write_frame},
    server::serve,
};
use serde_json::{Value, json};
use std::io::{BufReader, Cursor};

#[test]
fn stdio_drains_requests_at_eof_and_batches_only_responses() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = vec![];
    for message in [
        json!({"jsonrpc":"2.0","id":"one","method":"libraries"}),
        json!([
            {"jsonrpc":"2.0","id":2,"method":"search","params":{"query":"nothing"}},
            {"jsonrpc":"2.0","method":"unknown"},
            {"jsonrpc":"2.0","id":3,"method":"unknown"}
        ]),
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
        match message {
            Value::Array(batch) => {
                assert_eq!(batch.len(), 2);
                responses.extend(batch);
            }
            message if message.get("id").is_some() => responses.push(message),
            message => assert_eq!(message["method"], "$/progress"),
        }
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
