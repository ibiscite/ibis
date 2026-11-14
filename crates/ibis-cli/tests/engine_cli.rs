use serde_json::{Value, json};
use std::process::Command;
fn ibis(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ibis"))
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn downloaded_bibliography_selections_and_replacement_are_read_by_the_real_cli()
-> Result<(), Box<dyn std::error::Error>> {
    use std::fs;

    let home = tempfile::tempdir()?;
    let first = home.path().join("references.bib");
    let second = home.path().join("supplement.bib");
    let neighbor = home.path().join("unselected.bib");
    let download = home.path().join("completed-download.bib");
    let initial = b"@book{vale2024, title={Lantern Atlas}, author={Vale, Mira}, year={2024}}\n@book{old2020, title={Lantern Archive}, author={Old, Ada}, year={2020}}\n";
    let updated = b"@book{vale2024, title={Lantern Atlas Revised}, author={Vale, Mira}, year={2025}}\n@book{new2026, title={Lantern Almanac}, author={New, Nia}, year={2026}}\n";
    let supplement = b"@book{fen2023, title={Lantern Gardens}, author={Fen, Lee}, year={2023}}\n";
    let unselected = b"@book{hidden2022, title={Lantern Secret}, author={Hidden, Sam}, year={2022}}\n";
    fs::write(&first, initial)?;
    fs::write(&second, supplement)?;
    fs::write(&neighbor, unselected)?;
    fs::write(&download, updated)?;

    // The reader must work with files the user cannot write, and leave their modes alone.
    for path in [&first, &second, &neighbor, &download] {
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_readonly(true);
        fs::set_permissions(path, permissions)?;
    }
    let first_path = first.to_str().ok_or("first path")?;
    let second_path = second.to_str().ok_or("second path")?;
    let run = |args: &[&str]| -> Result<std::process::Output, Box<dyn std::error::Error>> {
        let output = Command::new(env!("CARGO_BIN_EXE_ibis"))
            .args(args)
            .env("HOME", home.path())
            .env("USERPROFILE", home.path())
            .env("APPDATA", home.path().join("AppData"))
            .env("XDG_CONFIG_HOME", home.path().join("config"))
            .output()?;
        assert!(
            output.status.success(),
            "{:?}: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty(), "{}", String::from_utf8_lossy(&output.stderr));
        Ok(output)
    };

    for (expected, absent, bytes) in [
        (
            [
                ("vale2024", "Lantern Atlas", "2024"),
                ("old2020", "Lantern Archive", "2020"),
                ("fen2023", "Lantern Gardens", "2023"),
            ],
            "new2026",
            &initial[..],
        ),
        (
            [
                ("vale2024", "Lantern Atlas Revised", "2025"),
                ("new2026", "Lantern Almanac", "2026"),
                ("fen2023", "Lantern Gardens", "2023"),
            ],
            "old2020",
            &updated[..],
        ),
    ] {
        let modes: Vec<_> = [&first, &second, &neighbor]
            .iter()
            .map(|path| fs::metadata(path).map(|m| m.permissions()))
            .collect::<Result<_, _>>()?;
        let args = ["search", "lantern", "--file", first_path, "--file", second_path];
        let plain = String::from_utf8(run(&args)?.stdout)?;
        let mut json_args = args.to_vec();
        json_args.push("--json");
        let json: Value = serde_json::from_slice(&run(&json_args)?.stdout)?;
        let hits = json["data"]["hits"].as_array().ok_or("search hits")?;
        assert_eq!(hits.len(), expected.len(), "{json}");
        assert_eq!(plain.lines().count(), expected.len(), "{plain}");
        for (key, title, year) in expected {
            assert!(
                plain
                    .lines()
                    .any(|line| line.starts_with(&format!("{key}\t")) && line.contains(title) && line.contains(year)),
                "{plain}"
            );
            assert!(
                hits.iter().any(|hit| {
                    let record = &hit["item"]["object"]["data"];
                    record["csl"]["title"] == title
                        && record["citation_keys"]
                            .as_array()
                            .is_some_and(|keys| keys.iter().any(|candidate| candidate["value"] == key))
                        && record["csl"]["issued"].to_string().contains(year)
                }),
                "{json}"
            );
        }
        for missing in [absent, "hidden2022"] {
            assert!(!plain.contains(missing), "{plain}");
            assert!(!json.to_string().contains(missing), "{json}");
        }
        let cite = [
            "cite",
            "vale2024",
            "fen2023",
            "--format",
            "cite",
            "--file",
            first_path,
            "--file",
            second_path,
        ];
        assert_eq!(run(&cite)?.stdout, b"\\cite{vale2024,fen2023}\n");
        let mut cite_json = cite.to_vec();
        cite_json.push("--json");
        let citation: Value = serde_json::from_slice(&run(&cite_json)?.stdout)?;
        assert_eq!(citation["data"]["text"], "\\cite{vale2024,fen2023}");
        for (index, (path, content)) in [
            (&first, bytes),
            (&second, &supplement[..]),
            (&neighbor, &unselected[..]),
        ]
        .iter()
        .enumerate()
        {
            assert_eq!(fs::read(path)?, *content);
            assert_eq!(fs::metadata(path)?.permissions(), modes[index]);
        }
        if download.exists() {
            // Windows cannot replace an existing read-only file by renaming over it.
            #[cfg(windows)]
            {
                let mut permissions = fs::metadata(&first)?.permissions();
                permissions.set_readonly(false);
                fs::set_permissions(&first, permissions)?;
                fs::remove_file(&first)?;
            }
            fs::rename(&download, &first)?;
        }
    }
    Ok(())
}

#[test]
fn piped_search_keeps_the_original_tsv_even_with_forced_color() -> Result<(), Box<dyn std::error::Error>> {
    for (name, value) in [("NO_COLOR", ""), ("TERM", "dumb"), ("CLICOLOR_FORCE", "1")] {
        let output = Command::new(env!("CARGO_BIN_EXE_ibis"))
            .args(["search", "lantern", "--file", "../../fixtures/website/sample.bib"])
            .env(name, value)
            .output()?;
        assert!(output.status.success());
        assert_eq!(
            output.stdout,
            b"vale2024lanterns\tMira Vale\t2024\tLantern gardens and the rhythm of imaginary cities\n"
        );
        assert!(output.stderr.is_empty());
    }
    Ok(())
}
#[test]
fn file_flags_accumulate_across_positions_and_ignore_duplicates() -> Result<(), Box<dyn std::error::Error>> {
    for duplicates in [false, true] {
        let mut args = vec![
            "--file",
            "../../fixtures/bibtex/empty.bib",
            "libraries",
            "--file",
            "../../fixtures/bibtex/fidelity.bib",
        ];
        if duplicates {
            args.extend(["--file", "../../fixtures/bibtex/empty.bib"]);
        }
        let human = ibis(&args);
        assert!(human.status.success());
        let text = std::str::from_utf8(&human.stdout)?;
        assert!(text.starts_with("2 libraries\n"), "{text}");
        assert!(text.contains("../../fixtures/bibtex/empty.bib\t"), "{text}");
        assert!(text.contains("../../fixtures/bibtex/fidelity.bib\t"), "{text}");
        assert_eq!(text.lines().count(), 3, "{text}");
        args.push("--json");
        let output = ibis(&args);
        assert!(output.status.success());
        let value: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(value["data"]["libraries"].as_array().map(Vec::len), Some(2));
    }
    Ok(())
}

#[test]
fn file_flags_after_the_subcommand_keep_readable_errors() {
    let output = ibis(&[
        "--file",
        "../../fixtures/bibtex/empty.bib",
        "libraries",
        "--file",
        "/invented/missing.bib",
    ]);
    assert_eq!(output.status.code(), Some(5));
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        b"ibis: cannot open /invented/missing.bib: The library file was moved, renamed or removed. Choose its current location with --file.\n"
    );
}

#[test]
fn every_method_is_listed_and_json_usage_errors_are_envelopes() {
    // `records` is `cite --records` on the command line.
    for method in ibis_engine::Method::ALL
        .into_iter()
        .filter(|method| *method != ibis_engine::Method::Records)
    {
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
        assert_eq!(
            value,
            json!({"ibis":1,"ok":false,"error":{"code":"usage","message":{"id":"error-usage","args":{}},"recovery":{"action":"edit-input"},"focus":"request"}})
        );
        // The stable code is on stdout; the sentence that names the argument is on stderr.
        assert!(output.stderr.starts_with(b"ibis: "), "{:?}", output.stderr);
    }
}

#[test]
fn a_search_query_after_the_end_of_options_is_not_a_json_flag() -> Result<(), Box<dyn std::error::Error>> {
    let file = "--file=../../fixtures/bibtex/fidelity.bib";
    let human = ibis(&["search", file, "--", "--json"]);
    assert!(human.status.success());
    assert!(human.stderr.is_empty());
    assert_eq!(std::str::from_utf8(&human.stdout)?, "No references found.\n");

    let machine = ibis(&["search", file, "--json", "--", "--json"]);
    assert!(machine.status.success());
    assert!(machine.stderr.is_empty());
    let value: Value = serde_json::from_slice(&machine.stdout)?;
    assert_eq!(value["ibis"], 1);
    assert_eq!(value["ok"], true);
    assert_eq!(value["data"]["hits"], json!([]));
    Ok(())
}

#[test]
fn help_and_usage_errors_respect_the_end_of_options() -> Result<(), Box<dyn std::error::Error>> {
    let plain_usage = ibis(&["search", "--limit", "bad", "--", "--json"]);
    let json_usage = ibis(&["search", "--limit", "bad", "--json", "--", "--json"]);
    assert_eq!(plain_usage.status.code(), Some(2));
    assert_eq!(json_usage.status.code(), Some(2));
    assert!(plain_usage.stdout.is_empty());
    assert!(plain_usage.stderr.starts_with(b"ibis: "));
    assert_eq!(plain_usage.stderr, json_usage.stderr);
    assert_eq!(
        serde_json::from_slice::<Value>(&json_usage.stdout)?,
        json!({"ibis":1,"ok":false,"error":{"code":"usage","message":{"id":"error-usage","args":{}},"recovery":{"action":"edit-input"},"focus":"request"}})
    );

    let plain_help = ibis(&["search", "--help", "--", "--json"]);
    let json_help = ibis(&["search", "--help", "--json", "--", "--json"]);
    assert!(plain_help.status.success() && json_help.status.success());
    let help = std::str::from_utf8(&plain_help.stdout)?;
    assert!(
        help.lines()
            .any(|line| line.starts_with("Usage: ") && line.ends_with(" search [OPTIONS] <WORDS>")),
        "{help}"
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&json_help.stdout)?,
        json!({"ibis":1,"ok":true,"data":{"help":help}})
    );
    Ok(())
}

#[test]
fn schema_matches_the_published_contract() {
    let expected: Value = serde_json::from_str(include_str!("../../../fixtures/cli-schema.json"))
        .unwrap_or_else(|error| panic!("{error}"));
    for (args, expected) in [
        (&["schema"][..], &expected["data"]),
        (&["schema", "--json"][..], &expected),
    ] {
        let output = ibis(args);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let value: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            &value, expected,
            "update the snapshot only after reviewing an intentional schema change"
        );
    }
}

#[test]
fn search_and_call_use_the_same_versioned_results() {
    // Named, so the test never reads the app's settings on the machine running it.
    let empty = "--file=../../fixtures/bibtex/empty.bib";
    let search = ibis(&["search", "no matches", "--json", empty]);
    let call = ibis(&[
        "call",
        "search",
        "--params",
        r#"{"query":"no matches"}"#,
        "--json",
        empty,
    ]);
    assert!(search.status.success());
    assert!(call.status.success());
    assert_eq!(search.stdout, call.stdout);
    let missing = ibis(&["show", "missing", "--json", empty]);
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
            .args(["cite", "--request", "--json", "--file=../../fixtures/bibtex/empty.bib"])
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
            json!({"ibis":1,"ok":false,"error":{"code":"bad-input","message":{"id":"error-bad-input","args":{}},"recovery":{"action":"edit-input"},"focus":"request"}})
        );
        assert_eq!(
            output.stderr,
            b"ibis: stdin must hold one CitationRequest JSON object; see: ibis schema cite\n"
        );
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
    // A cancellation notification has no reply and does not cancel a future use of its ID.
    for message in [
        json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":1}}),
        json!({"jsonrpc":"2.0","id":1,"method":"search","params":{"query":"synthetic"}}),
    ] {
        ibis_engine::protocol::write_frame(&mut input, &message)?;
    }
    let output = run(&input);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let mut frames = Cursor::new(output.stdout);
    let response = ibis_engine::protocol::read_frame(&mut frames)?.ok_or("request response")?;
    assert_eq!(response["id"], 1);
    assert_eq!(response["result"]["hits"], json!([]));
    assert_eq!(ibis_engine::protocol::read_frame(&mut frames)?, None);
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
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":ibis_engine::mcp::LEGACY_VERSIONS[0],"capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cite","arguments":{"items":[{"ref":"accents2021"}],"format":"pandoc"}}}),
        // A 2026-07-28 client in the same process: no handshake, the revision in `_meta`.
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"cite","arguments":{"items":[{"ref":"accents2021"}],"format":"pandoc"},"_meta":{"io.modelcontextprotocol/protocolVersion":ibis_engine::mcp::PROTOCOL_VERSION,"io.modelcontextprotocol/clientCapabilities":{}}}}),
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
    assert_eq!(replies.len(), 3);
    assert_eq!(
        replies[0]["result"]["protocolVersion"],
        ibis_engine::mcp::LEGACY_VERSIONS[0]
    );
    assert_eq!(replies[1]["result"]["structuredContent"]["text"], "[@accents2021]");
    assert_eq!(replies[2]["result"]["structuredContent"]["text"], "[@accents2021]");
    assert_eq!(replies[2]["result"]["resultType"], "complete");
    Ok(())
}

#[test]
fn mcp_that_cannot_open_its_library_still_starts_and_says_why() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    let missing = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("private-folder-name.bib");
    let mut child = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .arg("mcp")
        .arg("--file")
        .arg(&missing)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    let mut stdin = child.stdin.take().ok_or("stdin")?;
    for message in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":ibis_engine::mcp::LEGACY_VERSIONS[0],"capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"search","arguments":{"query":"lantern"}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"styles","arguments":{}}}),
    ] {
        writeln!(stdin, "{message}")?;
    }
    drop(stdin);
    let output = child.wait_with_output()?;
    assert_eq!(output.status.code(), Some(0));
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("private-folder-name.bib"), "{stderr}");
    let replies: Vec<Value> = String::from_utf8(output.stdout)?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    assert_eq!(replies[1]["result"]["isError"], true);
    let text = replies[1]["result"]["content"][0]["text"].as_str().ok_or("text")?;
    assert!(text.starts_with("error: source-unavailable."), "{text}");
    assert!(text.contains("was moved, renamed or removed"), "{text}");
    assert!(text.contains("start a new session"), "{text}");
    assert!(!text.contains("private-folder-name"), "an agent sees no path: {text}");
    assert_eq!(replies[2]["result"]["structuredContent"]["styles"][0]["id"], "apa");
    Ok(())
}

#[test]
fn citations_use_note_overrides_but_never_note_text_or_app_links() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join(format!("ibis-notes-{}.json", std::process::id()));
    std::fs::write(
        &path,
        json!([{
            "id": "untitled",
            "citation-key": "untitled",
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
