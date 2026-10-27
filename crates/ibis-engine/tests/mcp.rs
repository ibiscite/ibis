#![allow(clippy::unwrap_used)]
//! `ibis mcp` from a synthetic `.bib` file to the text an agent receives.
use ibis_engine::{Cancellation, Engine, Method, mcp};
use serde_json::{Value, json};

const BIB: &str = r"@article{demo2026,
  author = {Researcher, Synthetic and Writer, Example},
  title = {Synthetic research on sleep},
  journal = {Journal of Examples},
  year = {2026},
  note = {private reading note},
  annote = {another private note},
  abstract = {ABSTRACT}
}
@book{nokey,
  title = {A second synthetic book},
  author = {Author, Second},
  year = {2025}
}
";

fn engine() -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.bib");
    std::fs::write(&path, BIB.replace("ABSTRACT", &"long ".repeat(400))).unwrap();
    let engine = Engine::new().unwrap().without_file_watches();
    engine
        .dispatch(
            Method::Libraries,
            &json!({"action":"open-file","path":path}),
            &Cancellation::default(),
        )
        .unwrap();
    (dir, engine)
}

fn request(id: u64, method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
}

fn tool(engine: &Engine, name: &str, arguments: Value) -> Value {
    let reply = mcp::handle(
        engine,
        &request(9, "tools/call", json!({"name":name,"arguments":arguments})),
    )
    .unwrap();
    reply["result"].clone()
}

#[test]
fn a_session_runs_over_newline_delimited_stdio() {
    let (_dir, engine) = engine();
    let input = [
        request(
            1,
            "initialize",
            json!({"protocolVersion":"2099-01-01","capabilities":{},"clientInfo":{"name":"test","version":"1"}}),
        ),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        request(2, "tools/list", json!({})),
        request(3, "tools/call", json!({"name":"search","arguments":{"query":"sleep"}})),
        request(4, "nonsense", json!({})),
    ]
    .iter()
    .map(|message| format!("{message}\n"))
    .collect::<String>()
        + "not json\n";
    let mut output = Vec::new();
    mcp::serve(&engine, input.as_bytes(), &mut output).unwrap();
    let replies: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    // The notification gets no answer; everything else gets exactly one.
    assert_eq!(replies.len(), 5);
    assert_eq!(replies[0]["result"]["protocolVersion"], mcp::PROTOCOL_VERSION);
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "ibis");
    let names: Vec<&str> = replies[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["search", "show", "cite", "styles"]);
    for tool in replies[1]["result"]["tools"].as_array().unwrap() {
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
        assert_eq!(tool["annotations"]["openWorldHint"], false);
    }
    assert_eq!(replies[2]["result"]["structuredContent"]["hits"][0]["key"], "demo2026");
    assert_eq!(replies[3]["error"]["code"], -32601);
    assert_eq!(replies[4]["error"]["code"], -32700);
}

#[test]
fn agents_never_see_private_notes_and_long_text_is_cut() {
    let (_dir, engine) = engine();
    let shown = tool(&engine, "show", json!({"item":{"key":"demo2026"}}));
    assert_eq!(shown["isError"], false);
    let data = &shown["structuredContent"];
    assert_eq!(data["title"], "Synthetic research on sleep");
    assert_eq!(data["authors"], json!(["Researcher, Synthetic", "Writer, Example"]));
    assert_eq!(data["year"], 2026);
    let text = shown.to_string();
    assert!(!text.contains("private"));
    let summary = data["csl"]["abstract"].as_str().unwrap();
    assert_eq!(summary.chars().count(), mcp::MAX_TEXT + 1);
    assert!(summary.ends_with('…'));
}

#[test]
fn cite_formats_by_key_or_by_id() {
    let (_dir, engine) = engine();
    let pandoc = tool(&engine, "cite", json!({"items":[{"key":"demo2026"}],"syntax":"pandoc"}));
    assert_eq!(pandoc["content"][0]["text"], "[@demo2026]");
    let apa = tool(&engine, "cite", json!({"items":[{"key":"demo2026"}]}));
    assert_eq!(apa["content"][0]["text"], "(Researcher & Writer, 2026)");

    let hits = tool(&engine, "search", json!({"query":"second"}));
    let id = hits["structuredContent"]["hits"][0]["id"].clone();
    assert_eq!(hits["structuredContent"]["hits"][0]["key"], "nokey");
    let by_id = tool(&engine, "cite", json!({"items":[{"id":id}],"style":"apa"}));
    assert_eq!(by_id["content"][0]["text"], "(Author, 2025)");
}

#[test]
fn bad_calls_are_tool_errors_without_library_text() {
    let (_dir, engine) = engine();
    let missing = tool(&engine, "show", json!({"item":{"key":"absent"}}));
    assert_eq!(missing["isError"], true);
    assert_eq!(missing["content"][0]["text"], "error: not-found");
    let both = tool(
        &engine,
        "cite",
        json!({"items":[{"key":"demo2026"}],"style":"apa","syntax":"pandoc"}),
    );
    assert_eq!(both["isError"], true);
    let limit = tool(&engine, "search", json!({"query":"sleep","limit":1000}));
    assert_eq!(limit["isError"], true);
    let unknown = mcp::handle(
        &engine,
        &request(5, "tools/call", json!({"name":"delete","arguments":{}})),
    )
    .unwrap();
    assert_eq!(unknown["error"]["code"], -32602);
    let batch = mcp::handle(&engine, &json!([request(6, "ping", json!({}))])).unwrap();
    assert_eq!(batch["error"]["code"], -32600);
}

#[test]
fn an_oversized_line_is_refused_once_and_the_session_continues() {
    let (_dir, engine) = engine();
    let mut input = "x".repeat(ibis_engine::protocol::MAX_REQUEST_BYTES + 10);
    input.push('\n');
    input.push_str(&format!("{}\n", request(7, "ping", json!({}))));
    let mut output = Vec::new();
    mcp::serve(&engine, input.as_bytes(), &mut output).unwrap();
    let replies: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(replies.len(), 2);
    assert_eq!(replies[0]["error"]["code"], -32600);
    assert_eq!(replies[1]["id"], 7);
}
