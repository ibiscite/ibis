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
    library("synthetic.bib", &BIB.replace("ABSTRACT", &"long ".repeat(400)))
}

fn library(name: &str, contents: &str) -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    std::fs::write(&path, contents).unwrap();
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
    let reply = handle(
        engine,
        &modern(9, "tools/call", json!({"name":name,"arguments":arguments})),
    )
    .unwrap();
    reply["result"].clone()
}

/// One stateless request, as a 2026-07-28 client sends it.
fn handle(engine: &Engine, message: &Value) -> Option<Value> {
    mcp::handle(engine, &mut mcp::Session::default(), message)
}

/// A process an older client opened with `initialize`.
fn initialized(engine: &Engine) -> mcp::Session {
    let mut session = mcp::Session::default();
    let opened = mcp::handle(
        engine,
        &mut session,
        &request(
            0,
            "initialize",
            json!({"protocolVersion":mcp::LEGACY_VERSIONS[0],"capabilities":{}}),
        ),
    )
    .unwrap();
    assert_eq!(opened["result"]["protocolVersion"], mcp::LEGACY_VERSIONS[0]);
    session
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
    // An unknown requested revision is answered with the newest handshake revision.
    assert_eq!(replies[0]["result"]["protocolVersion"], mcp::LEGACY_VERSIONS[0]);
    assert!(replies[0]["result"].get("resultType").is_none());
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

    // CSL-JSON output would carry the whole record, notes included.
    let exported = tool(
        &engine,
        "cite",
        json!({"items":[{"key":"demo2026"}],"syntax":"csl-json"}),
    );
    assert_eq!(exported["isError"], true);
    assert!(!exported.to_string().contains("private"));
    let tools = handle(&engine, &modern(3, "tools/list", json!({}))).unwrap();
    let syntaxes = &tools["result"]["tools"][2]["inputSchema"]["properties"]["syntax"]["enum"];
    assert_eq!(
        syntaxes,
        &json!(["pandoc", "biblatex", "natbib", "latex", "typst", "keys"])
    );
}

#[test]
fn cite_formats_by_key_or_by_id() {
    let (_dir, engine) = engine();
    let pandoc = tool(&engine, "cite", json!({"items":[{"key":"demo2026"}],"syntax":"pandoc"}));
    assert_eq!(pandoc["structuredContent"]["text"], "[@demo2026]");
    let apa = tool(&engine, "cite", json!({"items":[{"key":"demo2026"}]}));
    assert_eq!(apa["structuredContent"]["text"], "(Researcher & Writer, 2026)");
    // Notices travel with the text, as `ibis cite --json` reports; one work needs no year letters.
    assert_eq!(apa["structuredContent"]["notices"], json!([]));
    assert!(
        apa["structuredContent"].get("warnings").is_none(),
        "clean responses retain their existing shape"
    );

    let hits = tool(&engine, "search", json!({"query":"second"}));
    let id = hits["structuredContent"]["hits"][0]["id"].clone();
    assert_eq!(hits["structuredContent"]["hits"][0]["key"], "nokey");
    let by_id = tool(&engine, "cite", json!({"items":[{"id":id}],"style":"apa"}));
    assert_eq!(by_id["structuredContent"]["text"], "(Author, 2025)");
}

#[test]
fn cite_preserves_both_item_warning_codes_by_key_and_by_id() {
    use ibis_core::contract::*;
    use ibis_engine::request::KeyCitationRequest;
    let (_dir, engine) = library(
        "invented.bib",
        r"@article{protected2020,title={Using {iPhone} for NASA},author={Okafor, Ada},year={2020},journal={Invented Journal},publisher={\unknown{PRIVATE_SYNTHETIC_FIELD}}}",
    );
    let expected = engine
        .cite_keys(
            &KeyCitationRequest {
                keys: vec!["protected2020".into()],
                output: OutputSpec::Csl { style: "apa".into() },
                form: CitationForm::BibliographyOnly,
                output_locale: None,
            },
            &Cancellation::default(),
        )
        .unwrap();
    assert_eq!(
        expected.warnings,
        vec![
            CitationWarning {
                item: 0,
                code: CitationWarningCode::SourceField
            },
            CitationWarning {
                item: 0,
                code: CitationWarningCode::SentenceCaseExpected
            }
        ]
    );
    let shown = engine
        .show(&ibis_engine::request::ShowRequest {
            item: ibis_engine::request::ItemSelector::Key("protected2020".into()),
        })
        .unwrap();
    for selector in [json!({"key":"protected2020"}), json!({"id":shown.object.id()})] {
        let answer = tool(
            &engine,
            "cite",
            json!({"items":[selector],"style":"apa","form":"bibliography-only"}),
        );
        assert_eq!(answer["isError"], false);
        assert_eq!(answer["structuredContent"]["text"], expected.text);
        assert_eq!(answer["structuredContent"]["warnings"], json!(expected.warnings));
        let text: Value = serde_json::from_str(answer["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(text["warnings"], json!(expected.warnings));
        assert!(!answer.to_string().contains("PRIVATE_SYNTHETIC_FIELD"));
    }
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
    let unknown = handle(
        &engine,
        &modern(5, "tools/call", json!({"name":"delete","arguments":{}})),
    )
    .unwrap();
    assert_eq!(unknown["error"]["code"], -32602);
    let batch = handle(&engine, &json!([request(6, "ping", json!({}))])).unwrap();
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

#[test]
fn note_overrides_print_but_note_text_and_app_links_never_reach_agents() {
    let (_dir, engine) = library(
        "synthetic.json",
        &json!([{
            "id": "untitled",
            "type": "book",
            "author": [{"family": "Invented", "given": "Ada"}],
            "issued": {"date-parts": [[2020]]},
            "publisher": "Invented Press",
            "note": "Invented private memo\ntitle: Invented override title\ntodo: invented private task",
            "annote": "Invented private annotation",
            "URL": "zotero://open-pdf/library/items/INVENTED",
            "x-todo": "Invented private task"
        }])
        .to_string(),
    );
    let id = item_id(&engine);
    let shown = tool(&engine, "show", json!({"item":{"id":id}}));
    assert_eq!(shown["structuredContent"]["title"], "Invented override title");
    let bibliography = tool(&engine, "cite", json!({"items":[{"id":id}],"form":"bibliography-only"}));
    assert_eq!(
        bibliography["structuredContent"]["text"],
        "Invented, A. (2020). Invented override title. Invented Press."
    );
    for answer in [
        shown,
        bibliography,
        tool(&engine, "search", json!({"query":"invented"})),
    ] {
        let text = answer.to_string();
        assert!(
            !text.contains("private") && !text.contains("zotero:") && !text.contains("todo"),
            "{text}"
        );
    }
}

fn item_id(engine: &Engine) -> Value {
    let hits = tool(engine, "search", json!({"query":"invented"}));
    hits["structuredContent"]["hits"][0]["id"].clone()
}

#[test]
fn every_answer_is_bounded() {
    let records: Vec<Value> = (0..mcp::MAX_HITS)
        .map(|index| {
            json!({
                "id": format!("bound{index}"),
                "type": "book",
                "title": "Invented bound",
                // About 6 MB in all, inside the engine's own result budget.
                "issued": {"date-parts": [["9".repeat(200_000)]]},
                "author": [{"family": "x".repeat(20_000)}, {"family": "y".repeat(20_000)}],
            })
        })
        .collect();
    let (_dir, engine) = library("synthetic.json", &json!(records).to_string());
    let hits = tool(&engine, "search", json!({"query":"bound","limit":mcp::MAX_HITS}));
    assert_eq!(hits["isError"], false);
    assert!(hits["content"][0]["text"].as_str().unwrap().len() <= mcp::MAX_ANSWER);
    let first = &hits["structuredContent"]["hits"][0];
    assert_eq!(first["year"].as_str().unwrap().chars().count(), mcp::MAX_TEXT + 1);
    assert_eq!(
        hits["structuredContent"]["hits"].as_array().unwrap().len(),
        mcp::MAX_HITS
    );
    let shown = tool(&engine, "show", json!({"item":{"key":"bound0"}}));
    assert!(shown["content"][0]["text"].as_str().unwrap().len() <= mcp::MAX_ANSWER);
}

#[test]
fn malformed_messages_are_answered() {
    let (_dir, engine) = engine();
    let bad_method = handle(&engine, &json!({"jsonrpc":"2.0","id":7,"method":123})).unwrap();
    assert_eq!(
        bad_method,
        json!({"jsonrpc":"2.0","id":7,"error":{"code":-32600,"message":"Invalid Request"}})
    );
    let no_method = handle(&engine, &json!({"jsonrpc":"2.0","id":"eight"})).unwrap();
    assert_eq!(no_method["id"], "eight");
    assert_eq!(no_method["error"]["code"], -32600);
    let bad_id = handle(&engine, &json!({"jsonrpc":"2.0","id":[1],"method":7})).unwrap();
    assert_eq!(bad_id["id"], Value::Null);
    let old_version = handle(&engine, &json!({"id":{"invented":1},"method":"ping"})).unwrap();
    assert_eq!(old_version["id"], Value::Null);
    // A response to a request the server never sent needs no answer.
    assert_eq!(handle(&engine, &json!({"jsonrpc":"2.0","id":1,"result":{}})), None);
}

#[test]
fn an_override_cannot_slip_past_the_formatter_checks() {
    // An "others" marker is refused whether it is a field or a note override.
    let (_dir, engine) = library(
        "synthetic.json",
        &json!([{"id":"others","type":"book","title":"Invented","issued":{"date-parts":[[2020]]},"note":"author: others"}])
            .to_string(),
    );
    let cited = tool(&engine, "cite", json!({"items":[{"id":item_id(&engine)}]}));
    assert_eq!(cited["isError"], true);
}

fn modern(id: u64, method: &str, mut params: Value) -> Value {
    params["_meta"] = json!({
        "io.modelcontextprotocol/protocolVersion": mcp::PROTOCOL_VERSION,
        "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "1"},
        "io.modelcontextprotocol/clientCapabilities": {}
    });
    request(id, method, params)
}

#[test]
fn a_2026_client_works_without_a_handshake() {
    let (_dir, engine) = engine();
    let discover = handle(&engine, &modern(1, "server/discover", json!({}))).unwrap();
    let result = &discover["result"];
    assert_eq!(result["resultType"], "complete");
    assert_eq!(
        result["supportedVersions"],
        json!([mcp::PROTOCOL_VERSION, mcp::LEGACY_VERSIONS[0], mcp::LEGACY_VERSIONS[1]])
    );
    assert_eq!(result["capabilities"], json!({"tools": {}}));
    assert_eq!(result["_meta"]["io.modelcontextprotocol/serverInfo"]["name"], "ibis");
    assert_eq!(result["cacheScope"], "public");
    assert!(result["ttlMs"].as_u64().is_some());

    let list = handle(&engine, &modern(2, "tools/list", json!({}))).unwrap();
    assert_eq!(list["result"]["resultType"], "complete");
    assert!(list["result"]["ttlMs"].as_u64().is_some());
    assert_eq!(list["result"]["cacheScope"], "public");
    // The same tools in the same order as for older clients, so caches agree.
    let legacy = mcp::handle(&engine, &mut initialized(&engine), &request(3, "tools/list", json!({}))).unwrap();
    assert_eq!(list["result"]["tools"], legacy["result"]["tools"]);
    assert!(legacy["result"].get("ttlMs").is_none());

    let call = handle(
        &engine,
        &modern(4, "tools/call", json!({"name":"search","arguments":{"query":"sleep"}})),
    )
    .unwrap();
    assert_eq!(call["result"]["resultType"], "complete");
    assert_eq!(call["result"]["structuredContent"]["hits"][0]["key"], "demo2026");
    assert_eq!(
        call["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "ibis"
    );

    // Handshake-only methods do not exist for a stateless request.
    for method in ["initialize", "ping"] {
        let reply = handle(&engine, &modern(5, method, json!({}))).unwrap();
        assert_eq!(reply["error"]["code"], -32601, "{method}");
    }
}

#[test]
fn an_unknown_revision_is_refused_with_the_supported_list() {
    let (_dir, engine) = engine();
    for requested in [json!("1900-01-01"), json!("2025-06-18")] {
        let mut message = modern(1, "tools/list", json!({}));
        message["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"] = requested.clone();
        let reply = handle(&engine, &message).unwrap();
        assert_eq!(reply["id"], 1);
        assert_eq!(reply["error"]["code"], -32022);
        assert_eq!(reply["error"]["data"]["requested"], requested);
        assert_eq!(reply["error"]["data"]["supported"][0], mcp::PROTOCOL_VERSION);
        assert!(reply.get("result").is_none());
    }
    // A notification with an unknown revision still gets no answer.
    let mut notification = modern(1, "notifications/cancelled", json!({}));
    notification.as_object_mut().unwrap().remove("id");
    notification["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"] = json!("1900-01-01");
    assert_eq!(handle(&engine, &notification), None);
}

#[test]
fn a_request_without_the_fields_its_revision_requires_is_refused() {
    let (_dir, engine) = engine();
    let mut no_capabilities = modern(1, "tools/list", json!({}));
    no_capabilities["params"]["_meta"]
        .as_object_mut()
        .unwrap()
        .remove("io.modelcontextprotocol/clientCapabilities");
    let mut numeric = modern(2, "tools/list", json!({}));
    numeric["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"] = json!(20260728);
    // Without a version, a request belongs to an older client, which must open with initialize.
    let unversioned = request(3, "tools/call", json!({"name":"styles","arguments":{}}));
    for message in [no_capabilities, numeric, unversioned.clone()] {
        let reply = handle(&engine, &message).unwrap();
        assert_eq!(reply["error"]["code"], -32602, "{message}");
        assert!(reply.get("result").is_none());
    }
    let reply = mcp::handle(&engine, &mut initialized(&engine), &unversioned).unwrap();
    assert!(reply["result"]["content"].is_array(), "{reply}");
}

#[test]
fn older_clients_negotiate_a_revision_they_asked_for() {
    let (_dir, engine) = engine();
    for version in mcp::LEGACY_VERSIONS {
        let reply = handle(
            &engine,
            &request(
                1,
                "initialize",
                json!({"protocolVersion":version,"capabilities":{},"clientInfo":{"name":"t","version":"1"}}),
            ),
        )
        .unwrap();
        assert_eq!(reply["result"]["protocolVersion"], version);
        assert_eq!(reply["result"]["serverInfo"]["name"], "ibis");
    }
    // The 2026 revision has no handshake, so asking for it there gets the newest handshake revision.
    let mut session = mcp::Session::default();
    let reply = mcp::handle(
        &engine,
        &mut session,
        &request(
            2,
            "initialize",
            json!({"protocolVersion":mcp::PROTOCOL_VERSION,"capabilities":{}}),
        ),
    )
    .unwrap();
    assert_eq!(reply["result"]["protocolVersion"], mcp::LEGACY_VERSIONS[0]);
    assert_eq!(
        mcp::handle(&engine, &mut session, &request(3, "ping", json!({}))).unwrap()["result"],
        json!({})
    );
}

#[test]
fn every_tool_answer_has_the_fields_its_output_schema_requires() {
    let (_dir, engine) = engine();
    let tools = handle(&engine, &modern(1, "tools/list", json!({}))).unwrap()["result"]["tools"].clone();
    let id = tool(&engine, "search", json!({"query":"sleep"}))["structuredContent"]["hits"][0]["id"].clone();
    let calls = [
        ("search", json!({"query":"sleep"})),
        ("show", json!({"item":{"key":"demo2026"}})),
        ("show", json!({"item":{"id":id}})),
        ("cite", json!({"items":[{"key":"demo2026"}]})),
        ("cite", json!({"items":[{"key":"demo2026"}],"syntax":"pandoc"})),
        ("styles", json!({})),
    ];
    for (name, arguments) in calls {
        let schema = tools
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap()["outputSchema"]
            .clone();
        assert_eq!(schema["type"], "object", "{name}");
        let answer = tool(&engine, name, arguments);
        assert_eq!(answer["isError"], false, "{name}: {answer}");
        let content = &answer["structuredContent"];
        for field in schema["required"].as_array().unwrap() {
            assert!(content.get(field.as_str().unwrap()).is_some(), "{name} lacks {field}");
        }
        if name == "search" {
            for field in schema["properties"]["hits"]["items"]["required"].as_array().unwrap() {
                assert!(
                    content["hits"][0].get(field.as_str().unwrap()).is_some(),
                    "hit lacks {field}"
                );
            }
        }
    }
}
