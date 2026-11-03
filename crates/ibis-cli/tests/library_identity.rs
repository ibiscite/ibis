//! Library and record identity (#1535) as a person or agent meets it: every
//! scenario runs through the command line, JSON-RPC and MCP.
#![allow(clippy::unwrap_used)]
use ibis_engine::protocol::{read_frame, write_frame};
use ibis_engine::{Cancellation, Engine, Method, mcp};
use serde_json::{Value, json};
use std::{io::Cursor, path::Path};

const BOOK: &str =
    r#"{"type":"book","title":"Invented atlas","issued":{"date-parts":[[2026]]},"author":[{"family":"Doe"}]"#;

fn envelope(result: Result<Value, Value>) -> Value {
    match result {
        Ok(data) => json!({"ok": true, "data": data}),
        Err(error) => json!({"ok": false, "error": error}),
    }
}

fn command_line(engine: &Engine, method: Method, params: &Value) -> Value {
    let mut output = vec![];
    let args = ["ibis", "call", method.as_str(), "--stdin", "--json"];
    ibis::run_with(
        engine.clone(),
        args.map(Into::into).to_vec(),
        Cursor::new(serde_json::to_vec(params).unwrap()),
        &mut output,
        &mut vec![],
        ibis::OutputMode::Plain,
    );
    let answer: Value = serde_json::from_slice(&output).unwrap();
    let ok = answer["ok"].as_bool().unwrap();
    envelope(if ok {
        Ok(answer["data"].clone())
    } else {
        Err(answer["error"].clone())
    })
}

fn rpc(engine: &Engine, method: Method, params: &Value) -> Value {
    let mut input = vec![];
    write_frame(
        &mut input,
        &json!({"jsonrpc": "2.0", "id": 7, "method": method.as_str(), "params": params}),
    )
    .unwrap();
    let mut output = vec![];
    ibis::run_with(
        engine.clone(),
        ["ibis", "serve", "--stdio"].map(Into::into).to_vec(),
        Cursor::new(input),
        &mut output,
        &mut vec![],
        ibis::OutputMode::Plain,
    );
    let mut frames = Cursor::new(output);
    while let Some(frame) = read_frame(&mut frames).unwrap() {
        if frame["id"] == json!(7) {
            return envelope(match frame.get("result") {
                Some(result) => Ok(result.clone()),
                None => Err(frame["error"]["data"].clone()),
            });
        }
    }
    panic!("no reply");
}

/// What the command line says, after checking that RPC says the same.
fn asked(engine: &Engine, method: Method, params: &Value) -> Value {
    let answer = command_line(engine, method, params);
    assert_eq!(rpc(engine, method, params), answer, "{}", method.as_str());
    answer
}

/// An MCP tool's answer: its structured content, or its error text.
fn tool(engine: &Engine, name: &str, arguments: Value) -> Result<Value, String> {
    let reply = mcp::handle(
        engine,
        &mut mcp::Session::default(),
        &json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":arguments,
            "_meta":{"io.modelcontextprotocol/protocolVersion":mcp::PROTOCOL_VERSION,"io.modelcontextprotocol/clientCapabilities":{}}}}),
    )
    .unwrap();
    let result = &reply["result"];
    if result["isError"] == json!(false) {
        Ok(result["structuredContent"].clone())
    } else {
        Err(result["content"][0]["text"].as_str().unwrap().to_owned())
    }
}

fn opened(paths: &[&Path]) -> Engine {
    let engine = Engine::new().unwrap().without_file_watches();
    for path in paths {
        engine
            .dispatch(
                Method::Libraries,
                &json!({"action":"open-file","path":path}),
                &Cancellation::default(),
            )
            .unwrap();
    }
    engine
}

fn write(directory: &Path, name: &str, text: &str) -> std::path::PathBuf {
    let path = directory.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

fn library_ids(engine: &Engine) -> Vec<String> {
    let listed = asked(engine, Method::Libraries, &json!({"action":"list"}));
    let libraries = listed["data"]["libraries"].as_array().unwrap();
    libraries
        .iter()
        .map(|library| library["id"].as_str().unwrap().to_owned())
        .collect()
}

fn request(item: &Value, output: Value) -> Value {
    json!({"items":[{"item":item}],"output":output})
}

fn pandoc() -> Value {
    json!({"kind":"key","syntax":"pandoc"})
}

#[test]
fn two_libraries_get_distinct_opaque_ids_and_report_their_kind() {
    let directory = tempfile::tempdir().unwrap();
    let bib = write(directory.path(), "a.bib", "@book{Doe2026,title={Invented atlas}}");
    let json = write(directory.path(), "b.json", &format!(r#"[{BOOK},"id":"item-7"}}]"#));
    let engine = opened(&[&bib, &json]);
    let listed = asked(&engine, Method::Libraries, &json!({"action":"list"}));
    let libraries = listed["data"]["libraries"].as_array().unwrap();
    assert_eq!(libraries.len(), 2);
    assert_ne!(libraries[0]["id"], libraries[1]["id"]);
    for library in libraries {
        assert_eq!(library["kind"], "file");
        assert!(
            library["capabilities"]
                .as_array()
                .unwrap()
                .contains(&json!("bibliographic-records"))
        );
        let id = library["id"].as_str().unwrap();
        assert!(
            id.bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'),
            "{id}"
        );
        assert!(id.len() <= 64);
        assert!(!id.contains("file") && !listed.to_string().contains(directory.path().to_str().unwrap()));
    }
}

#[test]
fn the_same_file_keeps_its_id_across_close_and_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = write(directory.path(), "a.bib", "@book{Doe2026,title={Invented atlas}}");
    let first = library_ids(&opened(&[&path]));
    write(directory.path(), "a.bib", "@book{Roe2026,title={An edited file}}");
    assert_eq!(library_ids(&opened(&[&path])), first);
}

#[test]
fn a_keyless_csl_record_formats_and_key_output_refuses_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = write(directory.path(), "a.json", &format!(r#"[{BOOK},"id":"item-7"}}]"#));
    let engine = opened(&[&path]);
    let id = library_ids(&engine).remove(0);
    let item = json!({"library":id,"key":"record:item-7"});
    let formatted = asked(
        &engine,
        Method::Cite,
        &request(&item, json!({"kind":"csl","style":"apa"})),
    );
    assert_eq!(formatted["data"]["text"], "(Doe, 2026)");
    let key = asked(&engine, Method::Cite, &request(&item, pandoc()));
    assert_eq!(key["data"]["outcomes"][0]["error"], "missing-key");
    assert_eq!(key["data"]["text"], "");

    assert_eq!(
        tool(&engine, "cite", json!({"items":[{"id":item}]})).unwrap()["text"],
        "(Doe, 2026)"
    );
    let refused = tool(&engine, "cite", json!({"items":[{"id":item}],"syntax":"pandoc"})).unwrap_err();
    assert!(refused.contains("has no citation key"), "{refused}");
    assert_eq!(
        tool(&engine, "show", json!({"item":{"id":item}})).unwrap()["key"],
        Value::Null
    );
    // The source `id` was never a key, so looking it up finds nothing.
    let lookup = asked(&engine, Method::Show, &json!({"item":{"by":"key","value":"item-7"}}));
    assert_eq!(lookup["error"]["code"], "not-found");
    assert!(
        tool(&engine, "show", json!({"item":{"key":"item-7"}}))
            .unwrap_err()
            .contains("not-found")
    );
}

#[test]
fn csl_json_key_export_keeps_the_source_id() {
    let directory = tempfile::tempdir().unwrap();
    let path = write(
        directory.path(),
        "a.json",
        &format!(r#"[{BOOK},"id":"item-7","citation-key":"Doe2026"}}]"#),
    );
    let engine = opened(&[&path]);
    let id = library_ids(&engine).remove(0);
    let item = json!({"library":id,"key":"record:item-7"});
    let exported = asked(
        &engine,
        Method::Cite,
        &request(&item, json!({"kind":"key","syntax":"csl-json"})),
    );
    let records: Value = serde_json::from_str(exported["data"]["text"].as_str().unwrap()).unwrap();
    assert_eq!(records[0]["id"], "item-7");
    assert_eq!(records[0]["citation-key"], "Doe2026");
    // MCP offers no CSL-JSON key output, but finds the record by its real key.
    let cited = tool(&engine, "cite", json!({"items":[{"key":"Doe2026"}],"syntax":"pandoc"})).unwrap();
    assert_eq!(cited["text"], "[@Doe2026]");
}

#[test]
fn an_id_that_is_not_loaded_is_not_found_and_a_malformed_one_is_bad_input() {
    let directory = tempfile::tempdir().unwrap();
    let path = write(directory.path(), "a.bib", "@book{Doe2026,title={Invented atlas}}");
    let engine = opened(&[&path]);
    let before = engine.snapshot().unwrap();
    let absent = "lib_00000000000000000000000000000000";
    for params in [
        json!({"item":{"by":"id","value":{"library":absent,"key":"record:Doe2026"}}}),
        json!({"item":{"by":"id","value":{"library":"zotero:user","key":"record:Doe2026"}}}),
    ] {
        let code = if params.to_string().contains(absent) {
            "not-found"
        } else {
            "bad-input"
        };
        assert_eq!(asked(&engine, Method::Show, &params)["error"]["code"], code);
    }
    for (library, code) in [
        (absent, "not-found"),
        ("file:0123", "bad-input"),
        ("../secret", "bad-input"),
    ] {
        let search = json!({"query":"atlas","libraries":[library]});
        assert_eq!(asked(&engine, Method::Search, &search)["error"]["code"], code);
        let reload = json!({"action":"reload","library":library});
        assert_eq!(asked(&engine, Method::Libraries, &reload)["error"]["code"], code);
        let item = json!({"id":{"library":library,"key":"record:Doe2026"}});
        let text = tool(&engine, "show", json!({"item":item})).unwrap_err();
        assert!(text.contains(code), "{text}");
    }
    // Nothing was read or replaced by asking.
    assert_eq!(engine.snapshot().unwrap().fingerprints(), before.fingerprints());
    assert_eq!(library_ids(&engine).len(), 1);
}

#[test]
fn a_key_in_two_libraries_formats_by_reference_and_its_lookup_says_ambiguous() {
    let directory = tempfile::tempdir().unwrap();
    let one = write(
        directory.path(),
        "one.bib",
        "@book{Doe2026,title={Invented atlas},author={Doe, Ann},year={2026}}",
    );
    let two = write(
        directory.path(),
        "two.bib",
        "@book{Doe2026,title={Other atlas},author={Doe, Ben},year={2026}}",
    );
    let engine = opened(&[&one, &two]);
    for id in library_ids(&engine) {
        let item = json!({"library":id,"key":"record:Doe2026"});
        let formatted = asked(
            &engine,
            Method::Cite,
            &request(&item, json!({"kind":"csl","style":"apa"})),
        );
        assert_eq!(formatted["data"]["outcomes"][0]["outcome"], "rendered");
        assert!(tool(&engine, "cite", json!({"items":[{"id":item}]})).is_ok());
    }
    let lookup = asked(&engine, Method::Show, &json!({"item":{"by":"key","value":"Doe2026"}}));
    assert_eq!(lookup["error"]["code"], "bad-input");
    assert_eq!(lookup["error"]["reason"], "ambiguous-key");
    let text = tool(&engine, "show", json!({"item":{"key":"Doe2026"}})).unwrap_err();
    assert!(text.contains("Several references have this key"), "{text}");
}

#[test]
fn records_with_ids_one_and_quoted_one_are_both_refused_with_a_warning() {
    let directory = tempfile::tempdir().unwrap();
    let path = write(
        directory.path(),
        "a.json",
        &format!(r#"[{BOOK},"id":1}},{BOOK},"id":"1"}},{BOOK},"id":"kept"}}]"#),
    );
    let engine = opened(&[&path]);
    let listed = asked(&engine, Method::Libraries, &json!({"action":"list"}));
    let library = &listed["data"]["libraries"][0];
    assert_eq!(library["objects"], 1, "neither wins");
    let warnings = library["warnings"].as_array().unwrap();
    assert_eq!(
        warnings
            .iter()
            .filter(|warning| warning["code"] == "duplicate-key")
            .count(),
        2
    );
    assert!(warnings.iter().all(|warning| warning["key"] == "1"));
    let id = library["id"].as_str().unwrap();
    let show = json!({"item":{"by":"id","value":{"library":id,"key":"record:1"}}});
    assert_eq!(asked(&engine, Method::Show, &show)["error"]["code"], "not-found");

    let only_repeats = write(
        directory.path(),
        "b.json",
        &format!(r#"[{BOOK},"id":1}},{BOOK},"id":"1"}}]"#),
    );
    let engine = opened(&[&only_repeats]);
    assert_eq!(
        asked(&engine, Method::Libraries, &json!({"action":"list"}))["data"]["libraries"][0]["objects"],
        0
    );
}

#[test]
fn a_file_that_cannot_be_reread_keeps_its_id_and_its_last_good_data() {
    let directory = tempfile::tempdir().unwrap();
    let path = write(directory.path(), "a.bib", "@book{Doe2026,title={Invented atlas}}");
    let engine = opened(&[&path]);
    let id = library_ids(&engine).remove(0);
    std::fs::remove_file(&path).unwrap();
    let failed = asked(&engine, Method::Libraries, &json!({"action":"reload","library":id}));
    assert_eq!(failed["error"]["code"], "source-unavailable");
    let listed = asked(&engine, Method::Libraries, &json!({"action":"list"}));
    assert_eq!(listed["data"]["libraries"][0]["id"], json!(id));
    assert_eq!(listed["data"]["libraries"][0]["objects"], 1);
    assert_eq!(listed["data"]["libraries"][0]["refresh_error"], "source-unavailable");
}

/// The real command in its own process, with its configuration folder moved.
fn command(home: &Path, args: &[&str]) -> Value {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ibis"))
        .args(args)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("APPDATA", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .output()
        .unwrap();
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn separate_runs_share_one_saved_key_and_doctor_never_writes_it() {
    let home = tempfile::tempdir().unwrap();
    let library = tempfile::tempdir().unwrap();
    let path = write(library.path(), "a.bib", "@book{Doe2026,title={Invented atlas}}");
    let path = path.to_str().unwrap();
    let files = || walk(home.path());

    let report = command(home.path(), &["doctor", "--file", path, "--json"]);
    assert_eq!(report["data"]["library_ids"]["code"], "ids-this-run");
    assert!(files().is_empty(), "doctor writes nothing: {:?}", files());

    let id =
        |home: &Path| command(home, &["libraries", "--file", path, "--json"])["data"]["libraries"][0]["id"].clone();
    let first = id(home.path());
    assert_eq!(files().len(), 1, "one key file and nothing else");
    assert_eq!(id(home.path()), first);
    let report = command(home.path(), &["doctor", "--file", path, "--json"]);
    assert_eq!(report["data"]["sources"][0]["id"], first);
    assert_eq!(report["data"]["library_ids"]["code"], "ids-last");

    // Another person's key gives the same file another id, so an id seen
    // elsewhere confirms nothing about a guessed path.
    let other = tempfile::tempdir().unwrap();
    assert_ne!(id(other.path()), first);
}

#[cfg(unix)]
#[test]
fn a_home_that_cannot_keep_a_key_still_reads_libraries() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    let library = tempfile::tempdir().unwrap();
    let path = write(library.path(), "a.bib", "@book{Doe2026,title={Invented atlas}}");
    std::fs::set_permissions(home.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
    let answer = command(home.path(), &["libraries", "--file", path.to_str().unwrap(), "--json"]);
    std::fs::set_permissions(home.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(answer["ok"], true);
    assert_eq!(answer["data"]["libraries"][0]["objects"], 1);
}

fn walk(directory: &Path) -> Vec<std::path::PathBuf> {
    let mut files = vec![];
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(walk(&path));
        } else {
            files.push(path);
        }
    }
    files
}
