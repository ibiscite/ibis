//! The engine side of source adapters (decision 0056, step 4), proved with a
//! fake adapter and no reference manager: what the engine accepts, keeps,
//! drops and refuses, and what survives a bad read.
//!
//! This test program is also the fake adapter: copied as `ibis-source-NAME`,
//! it answers as the scenario file its request names says. It has its own
//! `main` (no test harness), so it can be both.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ibis_engine::sources::SourceAdapter;
use ibis_engine::{Cancellation, Engine, Method};
use serde_json::{Value, json};

fn main() {
    let program = std::env::args_os().next().map(PathBuf::from).unwrap_or_default();
    if program
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.starts_with("ibis-source-"))
    {
        std::process::exit(fake());
    }
    let filter: Vec<String> = std::env::args().skip(1).filter(|arg| !arg.starts_with('-')).collect();
    let tests: &[(&str, fn())] = &[
        ("reads_are_checked_and_stripped_of_what_an_adapter_may_not_claim", checked_and_stripped),
        ("rereads_keep_identity_follow_changes_and_retire_old_scopes", rereads),
        ("a_failed_reread_keeps_the_last_good_library", last_good),
        ("keys_count_only_when_declared", keys_when_declared),
        ("the_command_finds_adapters_on_path_and_built_ins_first", command_line),
    ];
    for (name, test) in tests {
        if filter.is_empty() || filter.iter().any(|wanted| name.contains(wanted.as_str())) {
            print!("test {name} ... ");
            test();
            println!("ok");
        }
    }
}

/// The fake adapter: the request's `library` is a scenario file.
fn fake() -> i32 {
    let mut request = String::new();
    std::io::stdin().read_to_string(&mut request).unwrap();
    let request: Value = serde_json::from_str(&request).unwrap();
    let Some(path) = request["library"].as_str() else {
        println!("{}", json!({"ibis": 1, "refused": "library-needed"}));
        return 0;
    };
    let scenario: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    match scenario["mode"].as_str().unwrap_or("answer") {
        "raw" => print!("{}", scenario["raw"].as_str().unwrap()),
        "exit" => return 3,
        "hang" => {
            // A program the adapter started must stop with it.
            #[cfg(unix)]
            {
                let child = std::process::Command::new("sleep").arg("30").spawn().unwrap();
                std::fs::write(scenario["pid"].as_str().unwrap(), child.id().to_string()).unwrap();
            }
            std::thread::sleep(Duration::from_secs(30));
        }
        _ => print!("{}", scenario["response"]),
    }
    0
}

struct Fake {
    _directory: tempfile::TempDir,
    scenario: PathBuf,
    program: PathBuf,
}

impl Fake {
    fn new(name: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let program = directory
            .path()
            .join(format!("ibis-source-{name}{}", std::env::consts::EXE_SUFFIX));
        std::fs::copy(std::env::current_exe().unwrap(), &program).unwrap();
        let scenario = directory.path().join("scenario.json");
        Self {
            _directory: directory,
            scenario,
            program,
        }
    }
    fn answer(&self, read: Value) {
        self.scenario(json!({"response": {"ibis": 1, "read": read}}));
    }
    fn scenario(&self, scenario: Value) {
        std::fs::write(&self.scenario, scenario.to_string()).unwrap();
    }
    fn adapter(&self) -> SourceAdapter {
        SourceAdapter::at("fake", self.program.clone()).with_deadline(Duration::from_secs(3))
    }
    fn library(&self) -> Option<&str> {
        self.scenario.to_str()
    }
}

/// The shared invented records, each with the id its manager gave it.
fn invented() -> Vec<Value> {
    let expected = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/endnote/expected.json");
    let records: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(expected).unwrap()).unwrap();
    records
        .into_iter()
        .enumerate()
        .map(|(index, mut record)| {
            record["id"] = json!(format!("{}", index + 1));
            record
        })
        .collect()
}

fn read(records: Vec<Value>) -> Value {
    json!({"scope": "invented-library", "revision": "r1", "records": records})
}

fn call(engine: &Engine, method: Method, params: Value) -> Result<Value, Value> {
    engine
        .dispatch(method, &params, &Cancellation::default())
        .map(|result| result.data)
        .map_err(|error| serde_json::to_value(error).unwrap())
}

fn libraries(engine: &Engine) -> Vec<Value> {
    call(engine, Method::Libraries, json!({})).unwrap()["libraries"]
        .as_array()
        .unwrap()
        .clone()
}

fn objects_of(engine: &Engine) -> Vec<Value> {
    engine
        .snapshot()
        .unwrap()
        .objects()
        .iter()
        .map(|object| serde_json::to_value(&object.object).unwrap())
        .collect()
}

fn engine() -> Engine {
    Engine::new().unwrap().without_file_watches()
}

fn checked_and_stripped() {
    let fake = Fake::new("fake");
    let mut records = invented();
    records[0]["citation-key"] = json!("vale2026tides");
    records[0]["file"] = json!("/Users/invented/private.pdf");
    records[0]["URL"] = json!("file:///Users/invented/private.pdf");
    records.push(json!({"id": "5", "title": "No type"}));
    let mut answer = read(records);
    answer["skipped"] = json!([{"cause": "unsupported-type", "entry": "An invented talk", "field": "ref-type"}]);
    fake.answer(answer);
    let engine = engine();
    engine.open_adapter(fake.adapter(), fake.library(), &Cancellation::default()).unwrap();
    let listed = libraries(&engine);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["kind"], "adapter");
    assert_eq!(listed[0]["capabilities"], json!(["bibliographic-records"]));
    let objects = objects_of(&engine);
    assert_eq!(objects.len(), 4, "records only: no attachment from `file`, no invalid record");
    for object in &objects {
        assert_eq!(object["kind"], "record");
        assert_eq!(object["data"]["citation_keys"], json!([]), "keys are claimed only when declared");
        assert!(object["data"]["csl"].get("URL").is_none(), "private links never enter the library");
        assert!(object["data"]["id"]["key"].as_str().unwrap().starts_with("record:"));
    }
    let warnings = listed[0]["warnings"].as_array().unwrap();
    let causes: Vec<_> = warnings.iter().map(|warning| warning["cause"].clone()).collect();
    assert!(causes.contains(&json!("link")), "{causes:?}");
    assert!(causes.contains(&json!("unknown-type")), "{causes:?}");
    // Ibis's own refusals name the record's place; the adapter's skip does not.
    assert!(warnings.iter().any(|warning| warning["line"] == json!(5)));
    assert!(warnings.iter().any(|warning| warning["line"].is_null() && warning["entry"] == "An invented talk"));
    // A key output refuses with missing-key; a style cites.
    let item = objects[0]["data"]["id"].clone();
    let keyed = call(&engine, Method::Cite, json!({"items": [{"item": item}], "output": {"kind": "key", "syntax": "pandoc"}})).unwrap();
    assert_eq!(keyed["outcomes"][0]["error"], "missing-key");
    let styled = call(&engine, Method::Cite, json!({"items": [{"item": item}], "output": {"kind": "csl", "style": "apa"}})).unwrap();
    assert!(styled["text"].as_str().unwrap().contains("Ångström"), "{styled}");
}

fn rereads() {
    let fake = Fake::new("fake");
    fake.answer(read(invented()));
    let engine = engine();
    let cancel = Cancellation::default();
    let (first, _) = engine.open_adapter(fake.adapter(), fake.library(), &cancel).unwrap();
    let revision = libraries(&engine)[0]["revision"].clone();
    let item = json!({"library": first, "key": "record:1"});
    // Unchanged: the same library and revision.
    call(&engine, Method::Libraries, json!({"action": "reload"})).unwrap();
    assert_eq!(libraries(&engine)[0]["revision"], revision);
    // Edited: the same handle finds the new fields.
    let mut edited = invented();
    edited[0]["volume"] = json!("13");
    let mut answer = read(edited.clone());
    answer["revision"] = json!("r2");
    fake.answer(answer);
    call(&engine, Method::Libraries, json!({"action": "reload"})).unwrap();
    let listed = libraries(&engine);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["id"], json!(first));
    assert_ne!(listed[0]["revision"], revision);
    let shown = call(&engine, Method::Show, json!({"item": {"id": item}})).unwrap();
    assert_eq!(shown["object"]["data"]["csl"]["volume"], "13");
    // A new scope: the adapter cannot show the ids still mean the same
    // records, so every earlier handle is retired with the old library.
    let mut moved = read(edited);
    moved["scope"] = json!("invented-library-copy");
    fake.answer(moved);
    call(&engine, Method::Libraries, json!({"action": "reload"})).unwrap();
    let listed = libraries(&engine);
    assert_eq!(listed.len(), 1);
    assert_ne!(listed[0]["id"], json!(first));
    let gone = call(&engine, Method::Show, json!({"item": {"id": item}})).unwrap_err();
    assert_eq!(gone["code"], "not-found", "{gone}");
}

fn last_good() {
    let fake = Fake::new("fake");
    fake.answer(read(invented()));
    let engine = engine();
    let (library, _) = engine
        .open_adapter(fake.adapter(), fake.library(), &Cancellation::default())
        .unwrap();
    let revision = libraries(&engine)[0]["revision"].clone();
    let mut bad: Vec<(&str, Value)> = vec![
        ("not json", json!({"mode": "raw", "raw": "not json"})),
        ("an extra field", json!({"mode": "raw", "raw": "{\"ibis\":1,\"read\":null,\"refused\":null,\"extra\":1}"})),
        ("another protocol", json!({"response": {"ibis": 2, "read": read(invented())}})),
        ("both answers", json!({"response": {"ibis": 1, "read": read(invented()), "refused": "unreadable"}})),
        ("an empty scope", json!({"response": {"ibis": 1, "read": {"scope": "", "revision": "r", "records": []}}})),
        ("a failed exit", json!({"mode": "exit"})),
        ("a refusal", json!({"response": {"ibis": 1, "refused": "manager-closed"}})),
    ];
    let pid = fake.scenario.with_file_name("pid");
    bad.push(("no answer in time", json!({"mode": "hang", "pid": pid})));
    for (case, scenario) in bad {
        fake.scenario(scenario);
        let started = Instant::now();
        let failure = call(&engine, Method::Libraries, json!({"action": "reload", "library": library})).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(10), "{case}: the deadline holds");
        let expected = if case == "a refusal" { "source-manager-closed" } else { "source-failed" };
        assert_eq!(failure["reason"], expected, "{case}: {failure}");
        let listed = libraries(&engine);
        assert_eq!(listed.len(), 1, "{case}");
        assert_eq!(listed[0]["revision"], revision, "{case}: the last good read stays");
        assert_eq!(listed[0]["refresh_error"], "source-unavailable", "{case}");
        let hits = call(&engine, Method::Search, json!({"query": "lanterns"})).unwrap();
        assert_eq!(hits["hits"].as_array().unwrap().len(), 1, "{case}");
    }
    #[cfg(unix)]
    {
        let pid = std::fs::read_to_string(&pid).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        let alive = std::process::Command::new("kill").args(["-0", pid.trim()]).status().unwrap();
        assert!(!alive.success(), "the adapter's own child stopped with it");
    }
    // The next good read clears the failure.
    fake.answer(read(invented()));
    call(&engine, Method::Libraries, json!({"action": "reload"})).unwrap();
    assert!(libraries(&engine)[0]["refresh_error"].is_null());
}

fn keys_when_declared() {
    let fake = Fake::new("fake");
    let mut records = invented();
    records[0]["citation-key"] = json!("angstrom2026tides");
    let mut answer = read(records);
    answer["keys"] = json!(true);
    fake.answer(answer);
    let engine = engine();
    engine.open_adapter(fake.adapter(), fake.library(), &Cancellation::default()).unwrap();
    assert_eq!(
        libraries(&engine)[0]["capabilities"],
        json!(["bibliographic-records", "native-citation-keys"])
    );
    let cited = call(
        &engine,
        Method::Cite,
        json!({"items": [{"item": {"library": libraries(&engine)[0]["id"], "key": "record:1"}}], "output": {"kind": "key", "syntax": "pandoc"}}),
    )
    .unwrap();
    assert_eq!(cited["text"], "[@angstrom2026tides]");
}

fn command_line() {
    let fake = Fake::new("fake");
    fake.answer(read(invented()));
    // A program on PATH with a built-in's name is never run for it.
    let shadow = fake.program.with_file_name(format!("ibis-source-endnote{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(&fake.program, &shadow).unwrap();
    let directory = fake.program.parent().unwrap();
    let ibis = |args: &[&str]| {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_ibis"))
            .args(args)
            .env("PATH", directory)
            .output()
            .unwrap();
        (output.status.code(), serde_json::from_slice::<Value>(&output.stdout).unwrap_or_default())
    };
    let source = format!("fake={}", fake.scenario.display());
    let (code, found) = ibis(&["search", "lanterns", "--source", &source, "--json"]);
    assert_eq!(code, Some(0), "{found}");
    assert_eq!(found["data"]["hits"].as_array().unwrap().len(), 1);
    let expected = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/endnote/expected.json");
    let (code, checked) = ibis(&["sources", "check", "--source", &source, "--expect", expected.to_str().unwrap(), "--json"]);
    assert_eq!(code, Some(0), "{checked}");
    assert_eq!(checked["data"]["passed"], true);
    let missing = directory.join("missing.xml");
    let (_, built_in) = ibis(&["libraries", "--source", &format!("endnote={}", missing.display()), "--json"]);
    assert_eq!(built_in["error"]["reason"], "source-library-not-found", "{built_in}");
    let (_, unknown) = ibis(&["libraries", "--source", "nothing-here", "--json"]);
    assert_eq!(unknown["error"]["reason"], "source-not-found", "{unknown}");
}
