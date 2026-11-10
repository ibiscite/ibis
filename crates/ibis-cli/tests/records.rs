//! Records a caller sends give the same answer through `cite --records`,
//! `call records`, `serve --stdio` and MCP's `cite` (#1539), and no
//! library is read.
use ibis_engine::protocol::{read_frame, write_frame};
use ibis_engine::{Cancellation, Engine, Method};
use serde_json::{Value, json};
use std::io::Cursor;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn run(args: &[&str], input: &[u8]) -> Result<(u8, Vec<u8>, String)> {
    let (mut output, mut errors) = (vec![], vec![]);
    let code = ibis::run_with(
        Engine::new().map_err(|_| "engine")?,
        args.iter().map(Into::into).collect(),
        Cursor::new(input.to_vec()),
        &mut output,
        &mut errors,
        ibis::OutputMode::Plain,
    );
    Ok((code, output, String::from_utf8(errors)?))
}

fn records() -> Value {
    json!([
        {"id": "r1", "type": "book", "title": "Invented Lanterns", "citation-key": "vale2024",
         "author": [{"family": "Vale", "given": "Ada"}], "issued": {"date-parts": [[2024]]}},
        {"id": 2, "type": "article-journal", "title": "Invented tides", "citation-key": "fen2023",
         "author": [{"family": "Fen", "given": "Ola"}], "issued": {"date-parts": [[2023]]}}
    ])
}

#[test]
fn records_cite_the_same_through_every_front_end() -> Result {
    let params = json!({"records": records(), "output": {"kind": "preset", "name": "apa"}, "form": "normal", "output_locale": null});
    let engine = Engine::new().map_err(|_| "engine")?;
    let expected = json!({"ibis": 1, "ok": true, "data": engine
        .dispatch(Method::Records, &params, &Cancellation::default())
        .map_err(|_| "records")?
        .data});
    let input = serde_json::to_vec(&records())?;
    let (code, cited, errors) = run(&["ibis", "cite", "--records", "--format", "apa", "--json"], &input)?;
    assert_eq!((code, errors.as_str()), (0, ""));
    assert_eq!(serde_json::from_slice::<Value>(&cited)?, expected);
    let (code, called, _) = run(
        &["ibis", "call", "records", "--stdin", "--json"],
        &serde_json::to_vec(&params)?,
    )?;
    assert_eq!(code, 0);
    assert_eq!(serde_json::from_slice::<Value>(&called)?, expected);
    let mut frame = vec![];
    write_frame(
        &mut frame,
        &json!({"jsonrpc": "2.0", "id": 7, "method": "records", "params": params}),
    )?;
    let (_, served, _) = run(&["ibis", "serve", "--stdio"], &frame)?;
    let reply = read_frame(&mut Cursor::new(served))?.ok_or("reply")?;
    assert_eq!(reply["result"], expected["data"]);
    // Plain output is the citation alone.
    let (code, plain, _) = run(&["ibis", "cite", "--records", "--format", "apa"], &input)?;
    assert_eq!(
        (code, String::from_utf8(plain)?),
        (0, "(Fen, 2023; Vale, 2024)\n".to_owned())
    );
    let answer = ibis_engine::mcp::handle(
        &engine,
        &mut ibis_engine::mcp::Session::default(),
        &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {
            "name": "cite", "arguments": {"records": records(), "format": "apa"},
            "_meta": {
                "io.modelcontextprotocol/protocolVersion": ibis_engine::mcp::PROTOCOL_VERSION,
                "io.modelcontextprotocol/clientCapabilities": {}
            }
        }}),
    )
    .ok_or("mcp")?;
    assert_eq!(answer["result"]["isError"], false, "{answer}");
    assert_eq!(answer["result"]["structuredContent"]["text"], expected["data"]["text"]);
    assert_eq!(
        answer["result"]["structuredContent"]["losses"],
        expected["data"]["losses"]
    );
    Ok(())
}

#[test]
fn refused_records_say_which_and_print_nothing_on_stdout() -> Result {
    let input = br#"[{"id":"r1","type":"book","title":"Invented"},{"id":"r2","title":"No type"}]"#;
    let (code, output, errors) = run(&["ibis", "cite", "--records"], input)?;
    assert_eq!(code, 3);
    assert!(output.is_empty());
    assert!(errors.contains("cites none rather than leave it out"), "{errors}");
    assert!(errors.contains("record 2:"), "{errors}");
    let (code, output, _) = run(
        &["ibis", "cite", "--records", "--json"],
        br#"[{"id":1,"type":"book"},{"id":"1","type":"book"}]"#,
    )?;
    assert_eq!(code, 3);
    assert_eq!(
        serde_json::from_slice::<Value>(&output)?,
        json!({"ibis":1,"ok":false,"error":{"code":"bad-input","reason":"duplicate-record"}})
    );
    // A keyless record has no key to write; the style still formats it.
    let keyless = br#"[{"id":"r1","type":"book","title":"Invented"}]"#;
    let (code, output, errors) = run(&["ibis", "cite", "--records", "--format", "pandoc"], keyless)?;
    assert_eq!((code, output.is_empty()), (1, true), "{errors}");
    assert!(errors.contains("no citation key"), "{errors}");
    let (code, _, errors) = run(&["ibis", "cite", "--records", "--file", "invented.bib"], keyless)?;
    assert_eq!(code, 2, "{errors}");
    let (code, _, _) = run(&["ibis", "cite", "--records"], br#"{"id":"r1"}"#)?;
    assert_eq!(code, 3);
    Ok(())
}
