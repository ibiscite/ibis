//! RIS exports and the CSV template as a person and a client meet them
//! (#1531, decision 0041): through the command line and the engine contract.
#![allow(clippy::unwrap_used)]
use ibis_engine::{Cancellation, Engine, Method};
use serde_json::{Value, json};
use std::{
    io::Cursor,
    path::{Path, PathBuf},
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name)
}

/// The command as a person runs it: exit code, output and errors.
fn run(args: &[&str]) -> (u8, String, String) {
    let (mut output, mut errors) = (vec![], vec![]);
    let code = ibis::run_with(
        Engine::new().unwrap().without_file_watches(),
        args.iter().map(|argument| (*argument).into()).collect(),
        Cursor::new([]),
        &mut output,
        &mut errors,
        ibis::OutputMode::Plain,
    );
    (code, String::from_utf8(output).unwrap(), String::from_utf8(errors).unwrap())
}

fn call(engine: &Engine, method: Method, params: Value) -> Result<Value, Value> {
    engine
        .dispatch(method, &params, &Cancellation::default())
        .map(|result| result.data)
        .map_err(|error| serde_json::to_value(error).unwrap())
}

fn opened(path: &Path) -> Result<Engine, Value> {
    let engine = Engine::new().unwrap().without_file_watches();
    call(&engine, Method::Libraries, json!({"action":"open-file","path":path}))?;
    Ok(engine)
}

/// The reference a search for `words` finds first.
fn found(engine: &Engine, words: &str) -> Value {
    let hits = call(engine, Method::Search, json!({"query": words})).unwrap();
    hits["hits"][0]["item"]["object"]["data"]["id"].clone()
}

fn cite(engine: &Engine, item: &Value, form: &str, output: Value) -> Value {
    call(
        engine,
        Method::Cite,
        json!({"items":[{"item":item}],"form":form,"output":output}),
    )
    .unwrap()
}

fn apa() -> Value {
    json!({"kind":"csl","style":"apa"})
}

#[test]
fn ris_and_csv_references_format_and_key_output_refuses_them() {
    for path in [fixture("ris/library.ris"), fixture("csv/library.csv")] {
        let engine = opened(&path).unwrap();
        let item = found(&engine, "tides");
        assert_eq!(cite(&engine, &item, "normal", apa())["text"], "(Doe & Roe, 2026)");
        let keys = cite(&engine, &item, "normal", json!({"kind":"key","syntax":"pandoc"}));
        assert_eq!(keys["outcomes"][0]["error"], "missing-key", "{}", path.display());
        assert_eq!(keys["text"], "");
    }
    // An article number must print, or reading it would lose it.
    let engine = opened(&fixture("ris/library.ris")).unwrap();
    let entry = cite(&engine, &found(&engine, "only a number"), "bibliography-only", apa());
    assert!(entry["text"].as_str().unwrap().contains("e1234"), "{entry}");
    // A person searching sees each reference without a key.
    let (code, text, _) = run(&["ibis", "search", "tides", "--file", fixture("ris/library.ris").to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(text.contains("Invented tides of the northern lake"), "{text}");
}

#[test]
fn skipped_records_are_listed_with_their_lines() {
    let path = fixture("ris/damaged.ris");
    let engine = opened(&path).unwrap();
    let listed = call(&engine, Method::Libraries, json!({"action":"list"})).unwrap();
    let library = &listed["libraries"][0];
    assert_eq!(library["objects"], 2);
    let lines: Vec<_> = library["warnings"].as_array().unwrap().iter().map(|w| w["line"].clone()).collect();
    assert_eq!(lines, [7, 11, 16, 22, 31]);
    let (code, text, errors) = run(&["ibis", "libraries", "--file", path.to_str().unwrap()]);
    assert_eq!(code, 1, "{text}{errors}");
    let said = format!("{text}{errors}");
    assert!(said.contains("line 31: an entry could not be read, so Ibis skips it"), "{said}");
}

#[test]
fn a_csv_with_a_missing_column_says_what_to_change() {
    let path = fixture("csv/missing-title.csv");
    let refused = opened(&path).err().unwrap();
    assert_eq!(refused, json!({"code":"bad-input","reason":"csv-template"}));
    let (code, _, errors) = run(&["ibis", "search", "spreadsheet", "--file", path.to_str().unwrap()]);
    assert_eq!(code, 3);
    assert!(errors.contains("does not match Ibis’s template"), "{errors}");
    assert!(errors.contains("Rename the columns in its first row"), "{errors}");
}

#[test]
fn empty_and_missing_files() {
    let (code, text, errors) = run(&["ibis", "search", "x", "--file", fixture("ris/empty.ris").to_str().unwrap()]);
    assert_eq!(code, 0, "{text}{errors}");
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("missing.ris");
    let (code, _, errors) = run(&["ibis", "search", "x", "--file", missing.to_str().unwrap()]);
    assert_eq!(code, 5, "{errors}");
    assert!(errors.contains("there is no file at that path"), "{errors}");
}

#[test]
fn a_refreshed_ris_file_keeps_the_reference_a_client_holds() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("export.ris");
    let one = "TY  - BOOK\nAU  - Doe, Jane\nTI  - Invented atlas\nPY  - 2026\nER  - \n";
    std::fs::write(&path, one).unwrap();
    let engine = opened(&path).unwrap();
    let item = found(&engine, "atlas");
    std::fs::write(&path, format!("TY  - BOOK\nTI  - Invented sequel\nER  - \n{one}")).unwrap();
    let listed = call(&engine, Method::Libraries, json!({"action":"list"})).unwrap();
    let library = listed["libraries"][0]["id"].clone();
    call(&engine, Method::Libraries, json!({"action":"reload","library":library})).unwrap();
    let listed = call(&engine, Method::Libraries, json!({"action":"list"})).unwrap();
    assert_eq!(listed["libraries"][0]["objects"], 2);
    assert_eq!(cite(&engine, &item, "normal", apa())["text"], "(Doe, 2026)");
}
