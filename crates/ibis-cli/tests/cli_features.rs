//! Approved CLI workflows, observed through both output streams.
use ibis_engine::Engine;
use serde_json::{Value, json};
use std::{io::Cursor, process::Command};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn library() -> Result<(tempfile::TempDir, String)> {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("refs with spaces.json");
    let mut records = vec![];
    for (id, key) in [
        ("one", "vale2024lanterns"),
        ("two", "fen2023clouds"),
        ("three", ""),
        ("four", ""),
    ] {
        let mut record = json!({"id":id,"type":"book",
            "title":format!("Lantern gardens {}", "invented cities ".repeat(20)),
            "author":[{"family":"Vale"}],"issued":{"date-parts":[[2024]]}});
        if !key.is_empty() {
            record["citation-key"] = json!(key);
        }
        records.push(record);
    }
    std::fs::write(&file, json!(records).to_string())?;
    Ok((dir, file.to_str().ok_or("path")?.into()))
}

fn run(file: &str, args: &[&str], input: &[u8], mode: ibis::OutputMode) -> Result<(u8, String, String)> {
    let mut arguments = vec!["ibis", "--file", file];
    arguments.extend_from_slice(args);
    let (mut stdout, mut stderr) = (vec![], vec![]);
    let code = ibis::run_with(
        Engine::new().map_err(|_| "engine")?,
        arguments.into_iter().map(Into::into).collect(),
        Cursor::new(input),
        &mut stdout,
        &mut stderr,
        mode,
    );
    Ok((code, String::from_utf8(stdout)?, String::from_utf8(stderr)?))
}

#[test]
fn duplicate_suggested_keys_offer_an_executable_id_route_and_keys_explain_omissions() -> Result {
    let dir = tempfile::tempdir()?;
    let a = dir.path().join("a.bib");
    let b = dir.path().join("b.bib");
    std::fs::write(&a, "@book{same, title={Invented lantern A}}")?;
    std::fs::write(&b, "@book{same, title={Invented lantern B}}")?;
    let a = a.to_str().ok_or("path")?;
    let b = b.to_str().ok_or("path")?;
    let (_, output, _) = run(
        a,
        &["--file", b, "show", "samx", "--json"],
        b"",
        ibis::OutputMode::Plain,
    )?;
    let error: Value = serde_json::from_str(&output)?;
    let next = error["error"]["next"].as_array().ok_or("next")?;
    assert_eq!(
        next[1], "search",
        "an ambiguous suggestion must use search --json and the id route"
    );
    assert!(next.contains(&json!("--json")), "{next:?}");
    let next_args = next.iter().skip(1).filter_map(Value::as_str).collect::<Vec<_>>();
    let (code, choices, errors) = run(a, &next_args, b"", ibis::OutputMode::Plain)?;
    assert_eq!(code, 0, "{errors}");
    let choices: Value = serde_json::from_str(&choices)?;
    let hits = choices["data"]["hits"].as_array().ok_or("choices")?;
    assert_eq!(hits.len(), 2);
    for hit in hits {
        let id = hit["item"]["object"]["data"]["id"].to_string();
        let (code, selected, errors) = run(
            a,
            &["--file", b, "show", "--id", &id, "--json"],
            b"",
            ibis::OutputMode::Plain,
        )?;
        assert_eq!(code, 0, "{selected}{errors}");
    }
    let (_, keys, explanation) = run(
        a,
        &["--file", b, "search", "lantern", "--keys", "--limit", "1"],
        b"",
        ibis::OutputMode::Plain,
    )?;
    assert!(
        keys.is_empty(),
        "even a duplicate beyond the requested page makes a bare key unsafe: {keys}"
    );
    assert!(
        explanation.contains("ambiguous") && explanation.contains("--json"),
        "{explanation}"
    );
    Ok(())
}

#[test]
fn sentinel_record_ids_never_enter_cli_human_output() -> Result {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("invented.json");
    std::fs::write(
        &path,
        json!([
            {"id":"SENTINEL-ID-keyless", "type":"book", "title":"Invented keyless lantern"},
            {"id":"SENTINEL-ID-keyed", "citation-key":"lantern2026", "type":"book", "title":"Invented keyed lantern"}
        ])
        .to_string(),
    )?;
    let path = path.to_str().ok_or("path")?;
    for mode in [ibis::OutputMode::Plain, ibis::OutputMode::Terminal] {
        for args in [
            vec!["search", "lantern"],
            vec!["search", "lantern", "--keys"],
            vec!["show", "lantern2026"],
            vec!["cite", "lantern2026"],
        ] {
            let (_, output, errors) = run(path, &args, b"", mode)?;
            assert!(!format!("{output}{errors}").contains("SENTINEL-ID"), "{output}{errors}");
        }
    }
    Ok(())
}

#[test]
fn keys_are_exact_ranked_lines_even_at_a_terminal_and_keyless_hits_are_counted() -> Result {
    let (_dir, file) = library()?;
    let (_, output, _) = run(&file, &["search", "lantern", "--json"], b"", ibis::OutputMode::Plain)?;
    let data: Value = serde_json::from_str(&output)?;
    let expected = data["data"]["hits"]
        .as_array()
        .ok_or("hits")?
        .iter()
        .filter_map(|hit| {
            hit.pointer("/item/object/data/citation_keys/0/value")
                .and_then(Value::as_str)
        })
        .filter(|key| !key.is_empty())
        .map(|key| format!("{key}\n"))
        .collect::<String>();
    for mode in [ibis::OutputMode::Plain, ibis::OutputMode::Terminal] {
        let (code, output, errors) = run(&file, &["search", "lantern", "--keys", "--color", "always"], b"", mode)?;
        assert_eq!(code, 0, "{errors}");
        assert_eq!(output, expected);
        assert_eq!(errors.lines().count(), 1, "{errors}");
        assert!(
            errors.contains("2 hits have no citation key; see ibis search lantern"),
            "{errors}"
        );
        assert!(errors.contains("--json"), "{errors}");
    }
    let (code, output, _) = run(
        &file,
        &["search", "lantern", "--keys", "--json"],
        b"",
        ibis::OutputMode::Plain,
    )?;
    assert_eq!(code, 2);
    assert_eq!(serde_json::from_str::<Value>(&output)?["error"]["code"], "usage");
    let (_, output, _) = run(&file, &["search", "absent", "--keys"], b"", ibis::OutputMode::Terminal)?;
    assert_eq!(output, "");
    Ok(())
}

#[test]
fn unknown_keys_suggest_without_citing_and_preserve_library_arguments() -> Result {
    let (_dir, file) = library()?;
    for command in ["show", "cite", "entry"] {
        let (code, output, errors) = run(&file, &[command, "vale2024lantern"], b"", ibis::OutputMode::Plain)?;
        assert_eq!((code, output.as_str()), (4, ""));
        assert!(errors.contains("Did you mean 'vale2024lanterns'?"), "{errors}");
        // The path is quoted the way this system's shell expects.
        assert!(errors.contains("Try: ibis show vale2024lanterns --file "), "{errors}");
        assert!(errors.contains(file.as_str()), "{errors}");
        let (code, output, _) = run(
            &file,
            &[command, "vale2024lantern", "--json"],
            b"",
            ibis::OutputMode::Plain,
        )?;
        assert_eq!(code, 4);
        let error = serde_json::from_str::<Value>(&output)?["error"].clone();
        assert_eq!(error["code"], "not-found");
        assert!(error.get("reason").is_none());
        assert_eq!(error["suggestions"], json!(["vale2024lanterns"]));
        assert_eq!(
            error["next"],
            json!(["ibis", "show", "vale2024lanterns", "--file", file])
        );
    }
    let params = json!({"item":{"by":"key","value":"vale2024lantern"}}).to_string();
    let (code, output, _) = run(
        &file,
        &["call", "show", "--params", &params, "--json"],
        b"",
        ibis::OutputMode::Plain,
    )?;
    assert_eq!(code, 4);
    assert_eq!(
        serde_json::from_str::<Value>(&output)?["error"]["suggestions"],
        json!(["vale2024lanterns"])
    );
    Ok(())
}

#[test]
fn suggestions_are_capped_and_dash_prefixed_keys_have_an_executable_next_command() -> Result {
    let (_dir, file) = library()?;
    let records: Vec<_> = ["a", "b", "c", "d"]
        .into_iter()
        .map(|suffix| json!({"id":suffix,"type":"book","citation-key":format!("-vale2024{suffix}")}))
        .collect();
    std::fs::write(&file, json!(records).to_string())?;
    let (code, output, _) = run(
        &file,
        &["show", "--json", "--", "-vale2024"],
        b"",
        ibis::OutputMode::Plain,
    )?;
    assert_eq!(code, 4);
    let error = serde_json::from_str::<Value>(&output)?["error"].clone();
    assert_eq!(error["suggestions"], json!(["-vale2024a", "-vale2024b", "-vale2024c"]));
    assert_eq!(
        error["next"],
        json!(["ibis", "show", "--file", file, "--", "-vale2024a"])
    );
    let args: Vec<_> = error["next"]
        .as_array()
        .ok_or("next")?
        .iter()
        .skip(1)
        .map(|arg| arg.as_str().ok_or("argument"))
        .collect::<std::result::Result<_, _>>()?;
    let (code, _, errors) = run(&file, &args, b"", ibis::OutputMode::Plain)?;
    assert_eq!(code, 0, "{errors}");
    Ok(())
}

#[test]
fn completeness_reports_more_matches_independently_of_source_health() -> Result {
    let (_dir, file) = library()?;
    for (limit, complete) in [("1", false), ("4", true), ("5", true)] {
        let (_, _, errors) = run(
            &file,
            &["search", "lantern", "--limit", limit],
            b"",
            ibis::OutputMode::Terminal,
        )?;
        assert_eq!(
            errors.contains("note: showing the first 1 match; see more with --limit 2"),
            !complete,
            "{errors}"
        );
        let (code, output, errors) = run(
            &file,
            &["search", "lantern", "--limit", limit, "--json"],
            b"",
            ibis::OutputMode::Plain,
        )?;
        assert_eq!((code, errors.as_str()), (0, ""));
        let data = serde_json::from_str::<Value>(&output)?["data"].clone();
        assert_eq!(data["complete"], complete);
        assert_eq!(data["source_health"]["status"], "ok");
    }
    for (stdout, stderr) in [(true, false), (false, true)] {
        let (_, output, errors) = run(
            &file,
            &["search", "lantern", "--limit", "1"],
            b"",
            ibis::OutputMode::Streams {
                stdout,
                stderr,
                stdin: false,
            },
        )?;
        assert_eq!(errors.contains("note: showing the first"), stderr, "{errors}");
        if !stdout {
            assert!(output.contains('\t'));
            assert!(!output.contains('\x1b'));
        }
    }
    Ok(())
}

#[test]
fn color_is_explicit_and_machine_streams_stay_undecorated() -> Result {
    let (_dir, file) = library()?;
    for (extra, no_color, codes) in [
        (vec![], false, false),
        (vec!["--color", "never"], false, false),
        (vec![], true, false),
        (vec!["--color", "always"], true, true),
        (vec!["--color", "always", "--json"], false, false),
        (vec!["--color", "always", "--keys"], false, false),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ibis"));
        command
            .args(["search", "lantern", "--file", &file])
            .args(extra)
            .env("TERM", "xterm");
        if no_color {
            command.env("NO_COLOR", "");
        } else {
            command.env_remove("NO_COLOR");
        }
        let output = command.output()?;
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(output.stdout.contains(&27), codes);
    }
    let (_, output, _) = run(
        &file,
        &["search", "lantern", "--color", "never", "--width", "60"],
        b"",
        ibis::OutputMode::Terminal,
    )?;
    assert!(!output.contains('\x1b'));
    assert!(output.lines().all(|line| line.chars().count() <= 60), "{output}");
    assert!(output.contains('…'), "{output}");
    let key = "unbrokenkey".repeat(12);
    let long_file = std::path::Path::new(&file).with_file_name("long.json");
    std::fs::write(
        &long_file,
        json!([{"id":"long","type":"book","citation-key":key,"title":"Lantern"}]).to_string(),
    )?;
    let (_, output, _) = run(
        long_file.to_str().ok_or("path")?,
        &["search", "lantern", "--width", "60", "--color", "never"],
        b"",
        ibis::OutputMode::Terminal,
    )?;
    assert_eq!(output.lines().next(), Some(key.as_str()));
    Ok(())
}

#[test]
fn explicit_key_input_keeps_order_duplicates_and_whole_citation_refusals() -> Result {
    let (_dir, file) = library()?;
    let input = b" # invented list\n vale2024lanterns \n\nfen2023clouds\nvale2024lanterns\n";
    let (code, output, errors) = run(
        &file,
        &["cite", "--keys-from", "-", "--format", "pandoc"],
        input,
        ibis::OutputMode::Plain,
    )?;
    assert_eq!(
        (code, output.as_str(), errors.as_str()),
        (0, "[@vale2024lanterns; @fen2023clouds; @vale2024lanterns]\n", "")
    );
    let path = std::path::Path::new(&file).with_file_name("keys.txt");
    std::fs::write(&path, input)?;
    let (code, from_file, _) = run(
        &file,
        &["cite", "--keys-from", path.to_str().ok_or("path")?],
        b"ignored",
        ibis::OutputMode::Plain,
    )?;
    assert_eq!((code, from_file), (0, output));
    let (code, output, _) = run(
        &file,
        &["cite", "--keys-from", "-"],
        b"vale2024lanterns\nmissing\n",
        ibis::OutputMode::Plain,
    )?;
    assert_eq!((code, output.as_str()), (4, ""));
    for input in [
        b"".to_vec(),
        b"vale2024lanterns\n".repeat(ibis_core::syntax::MAX_CITATION_ITEMS + 1),
    ] {
        let (code, output, _) = run(
            &file,
            &["cite", "--keys-from", "-", "--json"],
            &input,
            ibis::OutputMode::Plain,
        )?;
        assert_eq!(code, 3);
        assert_eq!(serde_json::from_str::<Value>(&output)?["error"]["code"], "bad-input");
    }
    let (code, output, _) = run(
        &file,
        &["cite", "--keys-from", "-", "--json"],
        &vec![b'x'; 1024 * 1024 + 1],
        ibis::OutputMode::Plain,
    )?;
    assert_eq!(code, 5);
    assert_eq!(
        serde_json::from_str::<Value>(&output)?["error"]["code"],
        "resource-limit"
    );
    let (code, _, _) = run(
        &file,
        &["cite", "vale2024lanterns", "--keys-from", "-"],
        input,
        ibis::OutputMode::Plain,
    )?;
    assert_eq!(code, 2);
    let (code, _, errors) = run(&file, &["cite", "--keys-from", "-"], input, ibis::OutputMode::Terminal)?;
    assert_eq!(code, 0);
    assert!(
        errors.contains("waiting for keys") && errors.contains("Ctrl-D"),
        "{errors}"
    );
    let (code, output, _) = run(
        &file,
        &["cite", "--keys-from", "-", "--json"],
        b"\xff",
        ibis::OutputMode::Plain,
    )?;
    assert_eq!(code, 3);
    assert_eq!(serde_json::from_str::<Value>(&output)?["error"]["code"], "bad-input");
    let (code, output, _) = run(
        &file,
        &["cite", "--keys-from", "missing-keys.txt"],
        b"",
        ibis::OutputMode::Plain,
    )?;
    assert_eq!((code, output.as_str()), (5, ""));
    Ok(())
}
