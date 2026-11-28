#![cfg(test)]
//! The proposal tools appear only when the person ran `ibis mcp --allow-writes`,
//! and a proposal changes nothing until the person applies it in Ibis.
use ibis::mcp;
use ibis_engine::write::Proposals;
use ibis_engine::{Cancellation, Engine};
use serde_json::{Map, Value, json};
use std::fs;
use std::path::PathBuf;

const BIB: &str = "% An invented library\n@string{pub = {Synthetic Press}}\n\n@article{morgan2025,\n  title = {Synthetic noise},\n  author = {Morgan, Ada},\n  journal = pub,\n  year = {2025},\n  keywords = {noise, {Signal Processing}},\n}\n\n@book{taylor2026,\n  title = {Synthetic signals},\n  author = {Taylor, Sam},\n  year = {2026}\n}\n";

struct Fixture {
    _dir: tempfile::TempDir,
    bib: PathBuf,
    store: Proposals,
    engine: Engine,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("library")).unwrap();
    let bib = dir.path().join("library/refs.bib");
    fs::write(&bib, BIB).unwrap();
    let engine = Engine::new().unwrap().without_file_watches();
    engine.open_file_observed(&bib, &Cancellation::default()).unwrap();
    let store = Proposals::new(dir.path().join("proposals"));
    Fixture {
        _dir: dir,
        bib,
        store,
        engine,
    }
}

fn paper() -> Map<String, Value> {
    let Value::Object(map) = json!({
        "type": "article-journal",
        "title": "Quiet signals",
        "author": [{"family": "Çelik", "given": "Defne"}],
        "issued": {"date-parts": [[2024, 5]]},
        "container-title": "Invented Letters",
        "DOI": "10.5555/invented.1"
    }) else {
        unreachable!()
    };
    map
}

fn call(engine: &Engine, session: &mut mcp::Session, name: &str, arguments: Value) -> Value {
    mcp::handle(
        engine,
        session,
        &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": name, "arguments": arguments}}),
    )
    .unwrap()
}

fn initialized(session: &mut mcp::Session, engine: &Engine) {
    mcp::handle(
        engine,
        session,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": {"name": "Invented Client", "version": "1.0"}
        }}),
    )
    .unwrap();
}

fn listed(engine: &Engine, session: &mut mcp::Session) -> Vec<Value> {
    let reply = mcp::handle(
        engine,
        session,
        &json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list", "params": {}}),
    )
    .unwrap();
    reply["result"]["tools"].as_array().unwrap().clone()
}

#[test]
fn proposal_tools_are_offered_only_with_allow_writes() {
    let fixture = fixture();
    let mut session = mcp::Session::default();
    initialized(&mut session, &fixture.engine);
    let names = |tools: &[Value]| {
        tools
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    assert!(
        !names(&listed(&fixture.engine, &mut session))
            .iter()
            .any(|name| name.starts_with("propose"))
    );
    let refused = call(&fixture.engine, &mut session, "propose_add", json!({"fields": paper()}));
    assert_eq!(refused["error"]["message"], "Unknown tool");
    assert!(fixture.store.list().unwrap().is_empty());

    let mut session = mcp::Session::default().allowing_writes(fixture.store.clone());
    initialized(&mut session, &fixture.engine);
    let tools = listed(&fixture.engine, &mut session);
    for name in ["propose_add", "propose_change", "propose_tags", "proposals"] {
        assert!(names(&tools).contains(&name.to_owned()), "{name}");
    }
    assert!(!names(&tools).iter().any(|name| name.contains("apply")));
    let add = tools.iter().find(|tool| tool["name"] == "propose_add").unwrap();
    assert_eq!(add["annotations"]["readOnlyHint"], false);
    assert_eq!(add["annotations"]["destructiveHint"], false);

    let answer = call(&fixture.engine, &mut session, "propose_add", json!({"fields": paper()}));
    let content = &answer["result"]["structuredContent"];
    assert_eq!(answer["result"]["isError"], false, "{answer}");
    assert_eq!(content["status"], "pending");
    let id = content["proposal"].as_str().unwrap();
    assert!(
        content["bibtex"]
            .as_str()
            .unwrap()
            .starts_with("@article{celik2024quiet,")
    );
    assert!(answer["result"]["content"][0]["text"].as_str().unwrap().contains(id));
    assert_eq!(fs::read_to_string(&fixture.bib).unwrap(), BIB);
    assert_eq!(fixture.store.get(id).unwrap().source.client, "Invented Client");

    let mine = call(&fixture.engine, &mut session, "proposals", json!({}));
    assert_eq!(mine["result"]["structuredContent"]["proposals"][0]["proposal"], id);
    assert_eq!(mine["result"]["structuredContent"]["libraries"][0]["kind"], "bibtex");
    let refused = call(
        &fixture.engine,
        &mut session,
        "propose_add",
        json!({"fields": {"type": "book", "title": "A", "note": "private"}}),
    );
    assert_eq!(refused["result"]["isError"], true);
    assert!(
        refused["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("note")
    );
}
