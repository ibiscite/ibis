use serde_json::{Value, json};
use std::process::Command;
fn ibis(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ibis"))
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("{error}"))
}
#[test]
fn every_method_is_listed_and_json_usage_errors_are_envelopes() {
    for method in ibis_engine::Method::ALL {
        let help = ibis(&[method.as_str(), "--help"]);
        assert!(help.status.success(), "{}", method.as_str());
    }
    for args in [
        vec!["search", "--json"],
        vec!["--json", "unknown"],
        vec!["cite", "--json"],
        vec!["search", "x", "--limit", "bad", "--json"],
        vec!["libraries", "--unknown-source", "--json"],
    ] {
        let output = ibis(&args);
        assert_eq!(output.status.code(), Some(2));
        let value: Value =
            serde_json::from_slice(&output.stdout).unwrap_or_else(|error| panic!("{error}: {:?}", output.stdout));
        assert_eq!(value, json!({"ibis":1,"ok":false,"error":{"code":"usage"}}));
        assert!(output.stderr.is_empty());
    }
}
#[test]
fn schema_search_and_call_use_the_same_versioned_results() {
    let output = ibis(&["schema", "--json"]);
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        value["data"].as_array().map(Vec::len),
        Some(ibis_engine::Method::ALL.len())
    );
    let search = ibis(&["search", "no matches", "--json"]);
    let call = ibis(&["call", "search", "--params", r#"{"query":"no matches"}"#, "--json"]);
    assert!(search.status.success());
    assert!(call.status.success());
    assert_eq!(search.stdout, call.stdout);
    let missing = ibis(&["show", "missing", "--json"]);
    assert_eq!(missing.status.code(), Some(4));
}

#[test]
fn request_input_is_bounded_and_rejects_ambiguous_json_without_echoing_it() {
    use std::{io::Write, process::Stdio};
    for input in [
        r#"{"items":[],"items":"synthetic private payload"}"#.to_owned(),
        "x".repeat(1024 * 1024 + 1),
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ibis"))
            .args(["cite", "--request", "--json"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|error| panic!("{error}"));
        let _ = child
            .stdin
            .take()
            .unwrap_or_else(|| panic!("stdin"))
            .write_all(input.as_bytes());
        let output = child.wait_with_output().unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(output.status.code(), Some(3));
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap_or_else(|error| panic!("{error}")),
            json!({"ibis":1,"ok":false,"error":{"code":"bad-input"}})
        );
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn untrusted_controls_cannot_execute_in_terminal_output_and_json_stays_literal()
-> Result<(), Box<dyn std::error::Error>> {
    use ibis_core::contract::*;
    use std::{io::Cursor, sync::Arc};
    let case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json"))?;
    let LibraryObject::Record(mut record) = case.library.objects()[0].object.clone() else {
        panic!("record")
    };
    record
        .csl
        .insert("title".into(), json!("Synthetic\u{1b}[2J\u{202e}text"));
    let engine = ibis_engine::Engine::from_snapshot(Arc::new(Snapshot::new(vec![LibraryObject::Record(record)])?));
    for json in [false, true] {
        let mut args = vec!["ibis", "show", "Example2026"];
        if json {
            args.push("--json");
        }
        let mut output = vec![];
        let mut errors = vec![];
        assert_eq!(
            ibis::run_with(
                engine.clone(),
                args.into_iter().map(Into::into).collect(),
                Cursor::new(vec![]),
                &mut output,
                &mut errors,
                ibis::OutputMode::Terminal
            ),
            0
        );
        let text = std::str::from_utf8(&output)?;
        assert!(!text.contains('\u{1b}'));
        if json {
            assert_eq!(
                serde_json::from_slice::<Value>(&output)?["data"]["object"]["data"]["csl"]["title"],
                "Synthetic\u{1b}[2J\u{202e}text"
            );
        } else {
            assert!(!text.contains('\u{202e}'));
            assert!(text.contains("\\u{202e}"));
        }
    }
    Ok(())
}

#[test]
fn native_stdio_framing_cancellation_and_transport_errors_use_the_engine() -> Result<(), Box<dyn std::error::Error>> {
    use std::{
        io::{Cursor, Write},
        process::Stdio,
    };
    let run = |input: &[u8]| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ibis"))
            .args(["serve", "--stdio", "--json"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|error| panic!("{error}"));
        child
            .stdin
            .take()
            .unwrap_or_else(|| panic!("stdin"))
            .write_all(input)
            .unwrap_or_else(|error| panic!("{error}"));
        child.wait_with_output().unwrap_or_else(|error| panic!("{error}"))
    };
    let mut input = vec![];
    ibis_engine::protocol::write_frame(
        &mut input,
        &json!([
            {"jsonrpc":"2.0","id":1,"method":"search","params":{"query":"synthetic"}},
            {"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":1}}
        ]),
    )?;
    let output = run(&input);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let mut frames = Cursor::new(output.stdout);
    let mut response = None;
    while let Some(message) = ibis_engine::protocol::read_frame(&mut frames)? {
        if message.is_array() {
            response = Some(message);
        }
    }
    let response = response.ok_or("batch response")?;
    assert_eq!(response[0]["id"], 1);
    assert_eq!(response[0]["error"]["data"]["code"], "cancelled");
    let output = run(format!(
        "Content-Length: {}\r\n\r\n",
        ibis_engine::protocol::MAX_REQUEST_BYTES + 1
    )
    .as_bytes());
    assert_eq!(output.status.code(), Some(3));
    let mut frames = Cursor::new(output.stdout);
    let response = ibis_engine::protocol::read_frame(&mut frames)?.ok_or("parse error")?;
    assert_eq!(response["error"]["code"], -32700);
    assert_eq!(ibis_engine::protocol::read_frame(&mut frames)?, None);
    assert!(output.stderr.is_empty());
    let output = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .args(["serve", "--stdio", "--json", "--file"])
        .arg(std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("missing-synthetic-library.bib"))
        .output()?;
    assert_eq!(output.status.code(), Some(5));
    let mut frames = Cursor::new(output.stdout);
    let response = ibis_engine::protocol::read_frame(&mut frames)?.ok_or("source error")?;
    assert_eq!(response["error"]["data"]["code"], "source-unavailable");
    assert_eq!(ibis_engine::protocol::read_frame(&mut frames)?, None);
    assert!(output.stderr.is_empty());
    Ok(())
}

#[test]
fn piped_plain_citations_retain_literal_rtl_format_marks() -> Result<(), Box<dyn std::error::Error>> {
    use ibis_core::contract::*;
    use std::{io::Cursor, sync::Arc};
    let case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json"))?;
    let LibraryObject::Record(mut record) = case.library.objects()[0].object.clone() else {
        panic!("record")
    };
    let key = "مثال\u{200f}2026";
    record.citation_keys[0].value = key.into();
    let engine = ibis_engine::Engine::from_snapshot(Arc::new(Snapshot::new(vec![LibraryObject::Record(record)])?));
    let mut output = vec![];
    let mut errors = vec![];
    assert_eq!(
        ibis::run_with(
            engine.clone(),
            ["ibis", "cite", key, "--format", "keys"].map(Into::into).to_vec(),
            Cursor::new(vec![]),
            &mut output,
            &mut errors,
            ibis::OutputMode::Plain
        ),
        0
    );
    assert_eq!(output, format!("{key}\n").into_bytes());
    let mut output = vec![];
    assert_eq!(
        ibis::run_with(
            engine,
            ["ibis", "cite", key, "--format", "keys"].map(Into::into).to_vec(),
            Cursor::new(vec![]),
            &mut output,
            &mut errors,
            ibis::OutputMode::Terminal
        ),
        0
    );
    assert_eq!(
        output,
        format!("{}\n", key.replace('\u{200f}', "\\u{200f}")).into_bytes()
    );
    assert!(errors.is_empty());
    Ok(())
}

#[test]
fn mcp_serves_agents_from_the_files_given() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .args(["mcp", "--file", "../../fixtures/bibtex/fidelity.bib"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    let mut stdin = child.stdin.take().ok_or("stdin")?;
    for message in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":ibis_engine::mcp::PROTOCOL_VERSION,"capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cite","arguments":{"items":[{"key":"accents2021"}],"syntax":"pandoc"}}}),
    ] {
        writeln!(stdin, "{message}")?;
    }
    drop(stdin);
    let output = child.wait_with_output()?;
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let replies: Vec<Value> = String::from_utf8(output.stdout)?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    assert_eq!(replies.len(), 2);
    assert_eq!(replies[1]["result"]["content"][0]["text"], "[@accents2021]");
    Ok(())
}

#[test]
fn citations_use_note_overrides_but_never_note_text_or_app_links() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join(format!("ibis-notes-{}.json", std::process::id()));
    std::fs::write(
        &path,
        json!([{
            "id": "untitled",
            "type": "book",
            "author": [{"family": "Invented", "given": "Ada"}],
            "issued": {"date-parts": [[2020]]},
            "publisher": "Invented Press",
            "note": "Invented private memo\ntitle: Invented override title",
            "annote": "Invented private annotation",
            "URL": "zotero://open-pdf/library/items/INVENTED"
        }])
        .to_string(),
    )?;
    let file = path.to_str().ok_or("path")?;
    let output = ibis(&[
        "cite",
        "--file",
        file,
        "--style",
        "apa",
        "--form",
        "bibliography-only",
        "untitled",
    ]);
    std::fs::remove_file(&path)?;
    assert_eq!(
        String::from_utf8(output.stdout)?.trim(),
        "Invented, A. (2020). Invented override title. Invented Press."
    );
    Ok(())
}
