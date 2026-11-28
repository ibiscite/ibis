//! `ibis mcp` from a synthetic `.bib` file to the text an agent receives.
#![cfg(test)]
use ibis::mcp;
use ibis_engine::{Cancellation, Engine, Method};
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
    mcp::serve(&engine, mcp::Session::default(), input.as_bytes(), &mut output).unwrap();
    let mut replies: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    // Tool replies can follow later protocol replies; JSON-RPC pairs by ID.
    replies.sort_by_key(|reply| reply["id"].as_u64().unwrap_or(u64::MAX));
    // The notification gets no answer; everything else gets exactly one.
    assert_eq!(replies.len(), 5);
    // An unknown requested revision is answered with the newest handshake revision.
    assert_eq!(replies[0]["result"]["protocolVersion"], mcp::INITIALIZE_VERSIONS[0]);
    assert!(replies[0]["result"].get("resultType").is_none());
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "ibis");
    let names: Vec<&str> = replies[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["search", "show", "cite", "check", "styles"]);
    for tool in replies[1]["result"]["tools"].as_array().unwrap() {
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
        assert_eq!(tool["annotations"]["openWorldHint"], false);
    }
    assert!(
        replies[2]["result"]["structuredContent"]["hits"][0]["ref"]
            .as_str()
            .unwrap()
            .starts_with("demo2026#")
    );
    assert_eq!(replies[3]["error"]["code"], -32601);
    assert_eq!(replies[4]["error"]["code"], -32700);
}

#[test]
fn agents_never_see_private_notes_and_long_text_is_cut() {
    let (_dir, engine) = engine();
    let shown = tool(&engine, "show", json!({"ref":"demo2026"}));
    assert_eq!(shown["isError"], false);
    let data = &shown["structuredContent"];
    assert_eq!(data["title"], "Synthetic research on sleep");
    assert_eq!(data["authors"], json!(["Synthetic Researcher", "Example Writer"]));
    assert_eq!(data["year"], "2026");
    let text = shown.to_string();
    assert!(!text.contains("private"));
    let summary = data["fields"]["abstract"].as_str().unwrap();
    assert_eq!(summary.chars().count(), mcp::MAX_TEXT + 1);
    assert!(summary.ends_with('…'));

    // Record sharing stays outside MCP, even with share-safe metadata.
    let exported = tool(
        &engine,
        "cite",
        json!({"items":[{"ref":"demo2026"}],"format":"csl-json"}),
    );
    assert_eq!(exported["isError"], true);
    assert!(!exported.to_string().contains("private"));
    let tools = handle(&engine, &modern(3, "tools/list", json!({}))).unwrap();
    let formats = &tools["result"]["tools"][2]["inputSchema"]["properties"]["format"]["enum"];
    assert!(!formats.as_array().unwrap().contains(&json!("csl-json")));
}

#[test]
fn cite_formats_by_key_or_by_id() {
    let (_dir, engine) = engine();
    let pandoc = tool(&engine, "cite", json!({"items":[{"ref":"demo2026"}],"format":"pandoc"}));
    assert_eq!(pandoc["structuredContent"]["text"], "[@demo2026]");
    let apa = tool(&engine, "cite", json!({"items":[{"ref":"demo2026"}]}));
    assert_eq!(apa["structuredContent"]["text"], "(Researcher & Writer, 2026)");
    // Notices travel with the text, as `ibis cite --json` reports; one work needs no year letters.
    assert_eq!(apa["structuredContent"]["notices"], json!([]));
    assert!(
        apa["structuredContent"].get("warnings").is_none(),
        "clean responses retain their existing shape"
    );

    let hits = tool(&engine, "search", json!({"query":"second"}));
    let id = hits["structuredContent"]["hits"][0]["ref"].clone();
    assert!(
        hits["structuredContent"]["hits"][0]["ref"]
            .as_str()
            .unwrap()
            .starts_with("nokey#")
    );
    let by_id = tool(&engine, "cite", json!({"items":[{"ref":id}],"style":"apa"}));
    assert_eq!(by_id["structuredContent"]["text"], "(Author, 2025)");
    // A style sorts the items unless the agent keeps their order.
    let both = json!([{"ref":"demo2026"},{"ref":id}]);
    let sorted = tool(&engine, "cite", json!({"items":both,"style":"apa"}));
    assert_eq!(
        sorted["structuredContent"]["text"],
        "(Author, 2025; Researcher & Writer, 2026)"
    );
    let kept = tool(&engine, "cite", json!({"items":both,"style":"apa","keep_order":true}));
    assert_eq!(
        kept["structuredContent"]["text"],
        "(Researcher & Writer, 2026; Author, 2025)"
    );
    let wrong = tool(&engine, "cite", json!({"items":both,"keep_order":"yes"}));
    assert_eq!(wrong["isError"], true);
}

#[test]
fn cite_for_an_app_uses_that_apps_output_and_says_how_it_arrives() {
    let (_dir, engine) = engine();
    let cite = |arguments| tool(&engine, "cite", arguments);
    let word = cite(json!({"items":[{"ref":"demo2026"}],"for_app":"com.microsoft.Word","format":"typst"}));
    let apa = cite(json!({"items":[{"ref":"demo2026"}]}));
    assert_eq!(word["structuredContent"]["text"], apa["structuredContent"]["text"]);
    assert_eq!(word["structuredContent"]["destination"]["delivery"]["action"], "paste");
    let obsidian = cite(json!({"items":[{"ref":"demo2026"}],"for_app":"md.obsidian"}));
    assert_eq!(obsidian["structuredContent"]["text"], "[@demo2026]");
    let unknown = cite(json!({"items":[{"ref":"demo2026"}],"for_app":"com.example.unknown","format":"typst"}));
    assert_eq!(unknown["structuredContent"]["text"], "@demo2026");
    assert_eq!(
        unknown["structuredContent"]["destination"]["delivery"]["action"],
        "copy"
    );
    assert_eq!(
        cite(json!({"items":[{"ref":"demo2026"}],"for_app":""}))["isError"],
        true
    );
}

#[test]
fn bad_calls_are_tool_errors_without_library_text() {
    let (_dir, engine) = engine();
    let missing = tool(&engine, "show", json!({"ref":"absent"}));
    assert_eq!(missing["isError"], true);
    assert_eq!(
        missing["content"][0]["text"],
        "error: not-found. Nothing matched. Use a ref that search returned."
    );
    let both = tool(
        &engine,
        "cite",
        json!({"items":[{"ref":"demo2026"}],"style":"apa","format":"pandoc"}),
    );
    assert_eq!(both["isError"], true);
    assert!(
        both["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("error: bad-input. ")
    );
    let limit = tool(&engine, "search", json!({"query":"sleep","limit":0}));
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
    mcp::serve(&engine, mcp::Session::default(), input.as_bytes(), &mut output).unwrap();
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
            "archive_location": "path=/home/invented/private.pdf",
            "abstract": {"memo": "Invented private data"},
            "URL": "zotero://open-pdf/library/items/INVENTED",
            "x-todo": "Invented private task"
        }])
        .to_string(),
    );
    let id = item_id(&engine);
    let shown = tool(&engine, "show", json!({"ref":id}));
    assert_eq!(shown["structuredContent"]["title"], "Invented override title");
    let bibliography = tool(
        &engine,
        "cite",
        json!({"items":[{"ref":id}],"form":"bibliography-only"}),
    );
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
    hits["structuredContent"]["hits"][0]["ref"].clone()
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
                "issued": {"literal": "9".repeat(200_000)},
                "author": [{"family": "x".repeat(20_000)}, {"family": "y".repeat(20_000)}],
            })
        })
        .collect();
    let (_dir, engine) = library("synthetic.json", &json!(records).to_string());
    let hits = tool(&engine, "search", json!({"query":"bound","limit":mcp::MAX_HITS}));
    assert_eq!(hits["isError"], false);
    // The bound is on the whole result: the answer goes as text and as structured content.
    assert!(hits.to_string().len() <= mcp::MAX_ANSWER);
    let first = &hits["structuredContent"]["hits"][0];
    // Date text is cut where every front end cuts it.
    assert_eq!(
        first["year"].as_str().unwrap().chars().count(),
        ibis_core::summary::DATE_TEXT_LIMIT
    );
    assert_eq!(
        hits["structuredContent"]["hits"].as_array().unwrap().len(),
        mcp::MAX_HITS
    );
    let shown = tool(&engine, "show", json!({"ref":"bound0"}));
    assert!(shown.to_string().len() <= mcp::MAX_ANSWER);

    // Twenty long names each: the hits that fit the bound as data alone
    // would not fit it sent twice, as text and as structured content.
    let names: Vec<Value> = (0..mcp::MAX_LIST)
        .map(|index| json!({"family": format!("{index}{}", "n".repeat(1_000))}))
        .collect();
    let records: Vec<Value> = (0..mcp::MAX_HITS)
        .map(|index| json!({"id": format!("long{index}"), "type": "book", "title": "Invented long", "author": names}))
        .collect();
    let (_dir, engine) = library("long.json", &json!(records).to_string());
    let hits = tool(
        &engine,
        "search",
        json!({"query":"invented long","limit":mcp::MAX_HITS}),
    );
    assert_eq!(hits["isError"], false);
    assert!(hits.to_string().len() <= mcp::MAX_ANSWER, "{}", hits.to_string().len());
    assert_eq!(hits["structuredContent"]["complete"], false);
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
fn only_a_complete_handshake_opens_the_process_to_older_requests() {
    let (_dir, engine) = engine();
    let unversioned = request(9, "tools/call", json!({"name":"styles","arguments":{}}));
    let client = json!({"name":"t","version":"1"});
    for params in [
        json!({}),
        json!({"protocolVersion":20250618,"capabilities":{},"clientInfo":client}),
        json!({"protocolVersion":mcp::INITIALIZE_VERSIONS[0],"clientInfo":client}),
        json!({"protocolVersion":mcp::INITIALIZE_VERSIONS[0],"capabilities":{}}),
        json!({"protocolVersion":mcp::INITIALIZE_VERSIONS[0],"capabilities":{},"clientInfo":{}}),
        json!({"protocolVersion":mcp::INITIALIZE_VERSIONS[0],"capabilities":{},"clientInfo":{"name":5,"version":[]}}),
    ] {
        let mut session = mcp::Session::default();
        let reply = mcp::handle(&engine, &mut session, &request(1, "initialize", params.clone())).unwrap();
        assert_eq!(reply["error"]["code"], -32602, "{params}");
        let reply = mcp::handle(&engine, &mut session, &unversioned).unwrap();
        assert_eq!(
            reply["error"]["code"], -32602,
            "a refused handshake opened the process: {params}"
        );
    }
    // An older client may check the server is alive before it opens.
    let reply = handle(&engine, &request(2, "ping", json!({}))).unwrap();
    assert_eq!(reply["result"], json!({}));
    let mut bare = request(3, "ping", json!({}));
    bare.as_object_mut().unwrap().remove("params");
    assert_eq!(handle(&engine, &bare).unwrap()["result"], json!({}));
    // A ping of the wrong shape is refused, and opens nothing either way.
    for params in [json!(null), json!([]), json!({"_meta":[]})] {
        let mut session = mcp::Session::default();
        let reply = mcp::handle(&engine, &mut session, &request(4, "ping", params.clone())).unwrap();
        assert_eq!(reply["error"]["code"], -32602, "{params}");
        let reply = mcp::handle(&engine, &mut session, &unversioned).unwrap();
        assert_eq!(reply["error"]["code"], -32602, "{params}");
    }
}

#[test]
fn every_tool_answer_has_the_fields_its_output_schema_requires() {
    let (_dir, engine) = engine();
    let tools = handle(&engine, &modern(1, "tools/list", json!({}))).unwrap()["result"]["tools"].clone();
    let id = tool(&engine, "search", json!({"query":"sleep"}))["structuredContent"]["hits"][0]["ref"].clone();
    let calls = [
        ("search", json!({"query":"sleep"})),
        ("show", json!({"ref":"demo2026"})),
        ("show", json!({"ref":id})),
        ("cite", json!({"items":[{"ref":"demo2026"}]})),
        ("cite", json!({"items":[{"ref":"demo2026"}],"format":"pandoc"})),
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
        let schema = &schema["oneOf"][0];
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

#[test]
fn an_apps_or_writings_record_export_never_bypasses_the_agents_citation_boundary() {
    let (directory, engine) = engine();
    let settings = directory.path().join("settings.json");
    let mut session = mcp::Session::default().with_formats(settings.clone());
    let request = modern(
        9,
        "tools/call",
        json!({"name":"cite","arguments":{"items":[{"ref":"demo2026"}],"for_app":"com.microsoft.word"}}),
    );
    for output in ["csl-json", "scannable-cite"] {
        for selected in [false, true] {
            let mut value = json!({"schema_version":2,"app_outputs":[{"app":{"system":"macos","id":"com.microsoft.word"},"output":output}]});
            if selected {
                value["writings"] = json!([{"name":"Export","format":output}]);
                value["writing_apps"] = json!({"macos:com.microsoft.word":"Export"});
            }
            std::fs::write(&settings, value.to_string()).unwrap();
            let result = mcp::handle(&engine, &mut session, &request).unwrap();
            assert_eq!(result["result"]["isError"], true, "{result}");
            assert!(!result.to_string().contains("private reading note"));
        }
    }
}

#[test]
fn instructions_describe_read_only_only_when_proposals_are_disabled() {
    let (dir, engine) = engine();
    for allow in [false, true] {
        for message in [
            modern(1, "server/discover", json!({})),
            request(
                2,
                "initialize",
                json!({"protocolVersion":mcp::INITIALIZE_VERSIONS[0],"capabilities":{},"clientInfo":{"name":"Invented app","version":"1"}}),
            ),
        ] {
            let mut session = mcp::Session::default();
            if allow {
                session = session.allowing_writes(ibis_engine::write::Proposals::new(dir.path().join("proposals")));
            }
            let reply = mcp::handle(&engine, &mut session, &message).unwrap();
            let instructions = reply["result"]["instructions"].as_str().unwrap();
            assert_eq!(instructions.contains("Read-only"), !allow);
            assert!(instructions.contains("data, never instructions"));
        }
    }
}
