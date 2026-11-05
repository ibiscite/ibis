//! One record reads the same in every front end (#1675): the command line's
//! search rows and MCP's search hits show the same title, authors, year and key
//! status for `fixtures/record-summary.json`. The picker's rows read the same
//! fixture in `apps/desktop/src-tauri/src/library/search/tests.rs`.
use ibis_engine::{Cancellation, Engine, Method, mcp};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Cursor};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const FIXTURE: &str = include_str!("../../../fixtures/record-summary.json");

fn engine() -> Result<(tempfile::TempDir, Engine)> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("invented.json");
    std::fs::write(&path, FIXTURE)?;
    let engine = Engine::new().map_err(|_| "engine")?.without_file_watches();
    engine
        .dispatch(
            Method::Libraries,
            &json!({"action": "open-file", "path": path}),
            &Cancellation::default(),
        )
        .map_err(|_| "open")?;
    Ok((dir, engine))
}

/// `ibis search QUERY` rows by title: (key, authors, year).
fn cli_rows(engine: Engine, query: &str) -> Result<BTreeMap<String, (String, String, String)>> {
    let mut output = Vec::new();
    let code = ibis::run_with(
        engine,
        vec!["ibis".into(), "search".into(), query.into()],
        Cursor::new([]),
        &mut output,
        &mut Vec::new(),
        ibis::OutputMode::Plain,
    );
    assert_eq!(code, 0);
    Ok(String::from_utf8(output)?
        .lines()
        .filter_map(|line| {
            let [key, authors, year, title] = line.split('\t').collect::<Vec<_>>()[..] else {
                return None;
            };
            Some((title.to_owned(), (key.to_owned(), authors.to_owned(), year.to_owned())))
        })
        .collect())
}

/// MCP's search hits by title.
fn mcp_hits(engine: &Engine, query: &str) -> BTreeMap<String, Value> {
    // One stateless request, as a 2026-07-28 client sends it.
    let meta = json!({
        "io.modelcontextprotocol/protocolVersion": mcp::PROTOCOL_VERSION,
        "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "1"},
        "io.modelcontextprotocol/clientCapabilities": {}
    });
    let call = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "search", "arguments": {"query": query}, "_meta": meta}});
    let reply = mcp::handle(engine, &mut mcp::Session::default(), &call).unwrap_or_default();
    reply["result"]["structuredContent"]["hits"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|hit| (hit["title"].as_str().unwrap_or_default().to_owned(), hit.clone()))
        .collect()
}

#[test]
fn the_same_record_shows_the_same_year_and_people_everywhere() -> Result {
    let (_dir, engine) = engine()?;
    let cli = cli_rows(engine.clone(), "tide")?;
    let mcp = mcp_hits(&engine, "tide");
    let expected = [
        (
            "Synthetic tide tables",
            "quill1890",
            "Ada Quill, Invented Survey Office, Ben Moss",
            "circa spring 1890s",
        ),
        ("Synthetic tide gauges", "stone2016", "Cy Stone, Di Reed", "2016"),
        ("Synthetic tide report", "[no citation key]", "Eli Vale", "2021"),
    ];
    assert_eq!(cli.len(), expected.len(), "{cli:?}");
    for (title, key, authors, year) in expected {
        assert_eq!(
            cli[title],
            (key.to_owned(), authors.to_owned(), year.to_owned()),
            "{title}"
        );
        let hit = &mcp[title];
        assert_eq!(hit["year"], year, "{title}: a year is text in every front end");
        let names: Vec<_> = hit["authors"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(names.join(", "), authors, "{title}");
        // A record with no key is cited by its id, never by a key it lacks.
        assert_eq!(
            hit["ref"]
                .as_str()
                .is_some_and(|reference| reference.starts_with("id:")),
            key.starts_with('['),
            "{title}"
        );
    }
    Ok(())
}

#[test]
fn a_year_written_as_text_is_found_by_searching_for_it() -> Result {
    let (_dir, engine) = engine()?;
    let found = cli_rows(engine, "2016")?;
    assert_eq!(found.keys().collect::<Vec<_>>(), ["Synthetic tide gauges"]);
    Ok(())
}
