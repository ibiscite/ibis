//! Library and record identity (#1535) as a person or agent meets it: every
//! scenario runs through the command line, JSON-RPC and MCP.
#![cfg(test)]
use ibis_engine::protocol::{read_frame, write_frame};
use ibis_engine::{Cancellation, Engine, Method};
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
    let mut underlying = answer.clone();
    // CLI advice adds a next command; the lookup's code/reason must still
    // match RPC exactly, including for source IDs that are not citation keys.
    if let Some(error) = underlying.get_mut("error").and_then(Value::as_object_mut) {
        error.remove("suggestions");
        error.remove("next");
    }
    assert_eq!(rpc(engine, method, params), underlying, "{}", method.as_str());
    answer
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
fn csl_json_export_refuses_two_records_that_would_share_an_id() {
    let directory = tempfile::tempdir().unwrap();
    let one = write(
        directory.path(),
        "one.json",
        &format!(r#"[{BOOK},"id":"item-7","citation-key":"One"}}]"#),
    );
    let two = write(
        directory.path(),
        "two.json",
        &format!(r#"[{BOOK},"id":"item-7","citation-key":"Two"}}]"#),
    );
    let engine = opened(&[&one, &two]);
    let items: Vec<_> = library_ids(&engine)
        .into_iter()
        .map(|id| json!({"item":{"library":id,"key":"record:item-7"}}))
        .collect();
    let exported = asked(
        &engine,
        Method::Cite,
        &json!({"items":items,"output":{"kind":"key","syntax":"csl-json"}}),
    );
    assert_eq!(exported["data"]["outcomes"][1]["error"], "ambiguous-key");
    let same = json!({"items":[items[0], items[0]],"output":{"kind":"key","syntax":"csl-json"}});
    let same = asked(&engine, Method::Cite, &same);
    assert_eq!(
        same["data"]["outcomes"][1]["outcome"], "rendered",
        "one record twice is fine"
    );
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
