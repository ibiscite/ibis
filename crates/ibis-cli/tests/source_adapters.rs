//! The engine side of source adapters (decision 0056, step 4), proved with a
//! fake adapter and no reference manager: what the engine accepts, keeps,
//! drops and refuses, and what survives a bad read.
//!
//! This test program is also the fake adapter: copied as `ibis-source-NAME`,
//! it answers as the scenario file its request names says. It has its own
//! `main` (no test harness), so it can be both.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ibis_engine::{Cancellation, Engine, Method};
use ibis_sources::adapter::SourceAdapter;
use serde_json::{Value, json};

fn main() {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--hold-pipe")) {
        std::thread::sleep(Duration::from_secs(30));
        return;
    }
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
        (
            "reads_are_checked_and_stripped_of_what_an_adapter_may_not_claim",
            checked_and_stripped,
        ),
        ("rereads_keep_identity_follow_changes_and_retire_old_scopes", rereads),
        ("a_failed_reread_keeps_the_last_good_library", last_good),
        ("keys_count_only_when_declared", keys_when_declared),
        ("only_configured_absolute_executables_are_run", command_line),
        ("a_warm_daemon_uses_the_callers_adapter_directory", adapter_directory),
        ("an_intervening_revision_fails_conformance", changed_during_check),
        ("an_initial_token_change_fails_conformance", initial_token_change),
        (
            "an_older_adapter_read_cannot_publish_over_a_newer_read",
            stale_publication,
        ),
        (
            "selector_aliases_cannot_publish_over_a_newer_library_read",
            stale_alias_publication,
        ),
        ("overflow_stops_an_adapter_before_it_finishes_writing", streaming_limit),
        (
            "exiting_adapters_do_not_leave_children_holding_the_pipes",
            exited_with_child,
        ),
        #[cfg(unix)]
        ("termination_kills_and_reaps_active_adapter_groups", termination),
        #[cfg(unix)]
        ("pending_termination_wins_over_stdin_completion", termination_at_eof),
        ("a_daemon_crash_stops_the_active_parser_tree", daemon_crash),
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
        "hang" | "too-large" | "exit-with-child" => {
            let mode = scenario["mode"].as_str().unwrap();
            // The descendant inherits stdout and keeps it open.
            #[allow(clippy::zombie_processes, reason = "the engine must stop this descendant")]
            let child = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("--hold-pipe")
                .spawn()
                .unwrap();
            std::fs::write(scenario["pid"].as_str().unwrap(), child.id().to_string()).unwrap();
            if let Some(path) = scenario["adapter-pid"].as_str() {
                std::fs::write(path, std::process::id().to_string()).unwrap();
            }
            if mode == "exit-with-child" {
                print!("{}", scenario["response"]);
                return 0;
            }
            if mode == "too-large" {
                let mut out = std::io::stdout().lock();
                let block = [b'x'; 64 * 1024];
                for _ in 0..=ibis_sources::adapter::MAX_ANSWER_BYTES / block.len() as u64 {
                    if out.write_all(&block).and_then(|()| out.flush()).is_err() {
                        break;
                    }
                }
            }
            // No EOF or valid answer is forthcoming.
            std::thread::sleep(Duration::from_secs(30));
        }
        "sequence" => {
            let counter = path.to_owned() + ".count";
            let index = std::fs::read_to_string(&counter)
                .ok()
                .and_then(|text| text.parse::<usize>().ok())
                .unwrap_or(0);
            std::fs::write(counter, (index + 1).to_string()).unwrap();
            print!("{}", scenario["responses"][index]);
        }
        "paused-answer" => {
            std::fs::write(scenario["ready"].as_str().unwrap(), "ready").unwrap();
            wait_for(Path::new(scenario["release"].as_str().unwrap()));
            print!("{}", scenario["response"]);
        }
        _ => print!("{}", scenario["response"]),
    }
    0
}

struct Fake {
    directory: tempfile::TempDir,
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
            directory,
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
        SourceAdapter::at("fake", self.program.clone())
            .unwrap()
            .with_deadline(Duration::from_secs(3))
    }
    fn configure(&self, configuration: Value) {
        let directory = self.directory.path();
        let root = if cfg!(target_os = "macos") {
            directory.join("Library/Application Support")
        } else {
            directory.to_path_buf()
        };
        let app = root.join("com.ibiscite.ibis");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(app.join("source-adapters.json"), configuration.to_string()).unwrap();
    }

    fn cli(&self, args: &[&str]) -> (Option<i32>, Value) {
        let directory = self.directory.path();
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_ibis"))
            .args(args)
            .env("PATH", directory)
            .env("HOME", directory)
            .env("XDG_CONFIG_HOME", directory)
            .env("APPDATA", directory)
            .output()
            .unwrap();
        (output.status.code(), serde_json::from_slice(&output.stdout).unwrap())
    }

    fn source(&self) -> String {
        format!("fake={}", self.scenario.display())
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
    call(engine, Method::Libraries, json!({"action": "list"})).unwrap()["libraries"]
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
    engine
        .open_adapter(fake.adapter(), fake.library(), &Cancellation::default())
        .unwrap();
    let listed = libraries(&engine);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["kind"], "adapter");
    assert_eq!(listed[0]["capabilities"], json!(["bibliographic-records"]));
    let objects = objects_of(&engine);
    assert_eq!(
        objects.len(),
        4,
        "records only: no attachment from `file`, no invalid record"
    );
    for object in &objects {
        assert_eq!(object["kind"], "record");
        assert_eq!(
            object["data"]["citation_keys"],
            json!([]),
            "keys are claimed only when declared"
        );
        assert!(
            object["data"]["csl"].get("URL").is_none(),
            "private links never enter the library"
        );
        assert!(object["data"]["id"]["key"].as_str().unwrap().starts_with("record:"));
    }
    let warnings = listed[0]["warnings"].as_array().unwrap();
    let causes: Vec<_> = warnings.iter().map(|warning| warning["cause"].clone()).collect();
    assert!(causes.contains(&json!("link")), "{causes:?}");
    assert!(causes.contains(&json!("unknown-type")), "{causes:?}");
    // Ibis's own refusals name the record's place; the adapter's skip does not.
    assert!(warnings.iter().any(|warning| warning["line"] == json!(5)));
    assert!(
        warnings
            .iter()
            .any(|warning| warning["line"].is_null() && warning["entry"] == "An invented talk")
    );
    // A key output refuses with missing-key; a style cites.
    let item = objects[0]["data"]["id"].clone();
    let keyed = call(
        &engine,
        Method::Cite,
        json!({"items": [{"item": item}], "output": {"kind": "key", "syntax": "pandoc"}}),
    )
    .unwrap();
    assert_eq!(keyed["outcomes"][0]["error"], "missing-key");
    let styled = call(
        &engine,
        Method::Cite,
        json!({"items": [{"item": item}], "output": {"kind": "csl", "style": "apa"}}),
    )
    .unwrap();
    assert!(styled["text"].as_str().unwrap().contains("Ångström"), "{styled}");
}

fn rereads() {
    let fake = Fake::new("fake");
    fake.answer(read(invented()));
    let engine = engine();
    let cancel = Cancellation::default();
    let (first, _) = engine.open_adapter(fake.adapter(), fake.library(), &cancel).unwrap();
    let first = first.id;
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
    let shown = call(&engine, Method::Show, json!({"item": {"by": "id", "value": item}})).unwrap();
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
    let gone = call(&engine, Method::Show, json!({"item": {"by": "id", "value": item}})).unwrap_err();
    assert_eq!(gone["code"], "not-found", "{gone}");
}

fn last_good() {
    let fake = Fake::new("fake");
    fake.answer(read(invented()));
    let engine = engine();
    let (library, _) = engine
        .open_adapter(fake.adapter(), fake.library(), &Cancellation::default())
        .unwrap();
    let library = library.id;
    let revision = libraries(&engine)[0]["revision"].clone();
    let mut bad: Vec<(&str, Value)> = vec![
        ("not json", json!({"mode": "raw", "raw": "not json"})),
        (
            "an extra field",
            json!({"mode": "raw", "raw": "{\"ibis\":1,\"read\":null,\"refused\":null,\"extra\":1}"}),
        ),
        (
            "another protocol",
            json!({"response": {"ibis": 2, "read": read(invented())}}),
        ),
        (
            "both answers",
            json!({"response": {"ibis": 1, "read": read(invented()), "refused": "unreadable"}}),
        ),
        (
            "an empty scope",
            json!({"response": {"ibis": 1, "read": {"scope": "", "revision": "r", "records": []}}}),
        ),
        ("a failed exit", json!({"mode": "exit"})),
        (
            "a refusal",
            json!({"response": {"ibis": 1, "refused": "manager-closed"}}),
        ),
    ];
    let pid = fake.scenario.with_file_name("pid");
    bad.push(("no answer in time", json!({"mode": "hang", "pid": pid})));
    for (case, scenario) in bad {
        fake.scenario(scenario);
        let started = Instant::now();
        let failure = call(
            &engine,
            Method::Libraries,
            json!({"action": "reload", "library": library}),
        )
        .unwrap_err();
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{case}: the deadline holds"
        );
        let expected = if case == "a refusal" {
            "source-manager-closed"
        } else {
            "source-failed"
        };
        assert_eq!(failure["code"], "source-unavailable", "{case}: {failure}");
        assert_eq!(failure["reason"], expected, "{case}: {failure}");
        let listed = libraries(&engine);
        assert_eq!(listed.len(), 1, "{case}");
        assert_eq!(listed[0]["revision"], revision, "{case}: the last good read stays");
        assert_eq!(listed[0]["refresh_error"], "source-unavailable", "{case}");
        let hits = call(&engine, Method::Search, json!({"query": "lanterns"})).unwrap();
        assert_eq!(hits["hits"].as_array().unwrap().len(), 1, "{case}");
    }
    assert_stopped(&pid);
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
    engine
        .open_adapter(fake.adapter(), fake.library(), &Cancellation::default())
        .unwrap();
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
    let directory = fake.program.parent().unwrap();
    let source = fake.source();
    let search = ["search", "lanterns", "--source", &source, "--json"];

    // The old discoverable file exists on PATH and would return a valid answer.
    let (_, path_only) = fake.cli(&search);
    assert_eq!(path_only["error"]["code"], "source-unavailable", "{path_only}");
    assert_eq!(path_only["error"]["reason"], "source-not-found", "{path_only}");
    let request = json!({"action":"open-source","name":"fake","library":fake.scenario});
    let (_, rpc) = fake.cli(&["call", "libraries", "--params", &request.to_string(), "--json"]);
    assert_eq!(rpc["error"]["reason"], "source-not-found", "{rpc}");

    // The same executable cannot be approved through a relative spelling.
    fake.configure(json!({"fake": fake.program.file_name().unwrap()}));
    let (_, relative) = fake.cli(&search);
    assert_eq!(relative["error"]["reason"], "source-not-found", "{relative}");
    let (_, doctor) = fake.cli(&["doctor", "--json"]);
    assert_eq!(
        doctor["data"]["source_adapters"]["code"],
        "source-adapter-config-invalid"
    );

    // An arbitrary executable name outside PATH is allowed when configured.
    let adapters = directory.join("approved adapters");
    std::fs::create_dir(&adapters).unwrap();
    let approved = adapters.join(format!("ibis-source-test program{}", std::env::consts::EXE_SUFFIX));
    std::fs::rename(&fake.program, &approved).unwrap();
    fake.configure(json!({"fake": approved}));
    let (code, found) = fake.cli(&search);
    assert_eq!(code, Some(0), "{found}");
    assert_eq!(found["data"]["hits"].as_array().unwrap().len(), 1);
    let (code, rpc) = fake.cli(&["call", "libraries", "--params", &request.to_string(), "--json"]);
    assert_eq!(code, Some(0), "{rpc}");
    let (_, doctor) = fake.cli(&["doctor", "--json"]);
    assert_eq!(
        doctor["data"]["source_adapters"]["adapters"],
        json!([{"name":"fake","approved":true}])
    );
    let expected = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/endnote/expected.json");
    let (code, checked) = fake.cli(&[
        "sources",
        "check",
        "--source",
        &source,
        "--expect",
        expected.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(code, Some(0), "{checked}");
    assert_eq!(checked["data"]["passed"], true);

    // An external entry cannot shadow the built-in reader.
    fake.configure(json!({"endnote": approved}));
    let missing = directory.join("missing.xml");
    let (_, built_in) = fake.cli(&[
        "libraries",
        "--source",
        &format!("endnote={}", missing.display()),
        "--json",
    ]);
    assert_eq!(built_in["error"]["reason"], "source-library-not-found", "{built_in}");
}

fn changed_during_check() {
    let fake = Fake::new("fake");
    fake.configure(json!({"fake": fake.program}));
    let original = read(invented());
    let mut edited = original.clone();
    edited["records"][0]["volume"] = json!("99");
    // The adapter also lies about the token. A later identical read cannot
    // hide either failure, even though the library id and final revision hold.
    fake.scenario(json!({"mode":"sequence", "responses":[
        {"ibis":1,"read":original}, {"ibis":1,"read":edited}, {"ibis":1,"read":original},
    ]}));
    let (code, output) = fake.cli(&["sources", "check", "--source", &fake.source(), "--json"]);
    assert_ne!(code, Some(0), "{output}");
    let checks = output["data"]["libraries"][0]["checks"].as_array().unwrap();
    for name in ["stable-revision-on-reread", "token-agrees-with-revision"] {
        assert!(checks.contains(&json!({"check":name,"passed":false})), "{output}");
    }
}

fn initial_token_change() {
    let fake = Fake::new("fake");
    fake.configure(json!({"fake": fake.program}));
    let original = read(invented());
    let mut changed_token = original.clone();
    changed_token["revision"] = json!("r2");
    fake.scenario(json!({"mode":"sequence", "responses":[
        {"ibis":1,"read":original}, {"ibis":1,"read":changed_token}, {"ibis":1,"read":changed_token},
    ]}));
    let (code, output) = fake.cli(&["sources", "check", "--source", &fake.source(), "--json"]);
    assert_eq!(code, Some(1), "{output}");
    let checks = output["data"]["libraries"][0]["checks"].as_array().unwrap();
    assert!(
        checks.contains(&json!({"check":"token-agrees-with-revision","passed":false})),
        "{output}"
    );
}

fn stale_publication() {
    for changed_scope in [false, true] {
        let fake = Fake::new("fake");
        let ready = fake.directory.path().join("ready");
        let release = fake.directory.path().join("release");
        fake.scenario(json!({"mode":"paused-answer", "ready":ready, "release":release,
        "response":{"ibis":1,"read":read(invented())}}));
        let engine = engine();
        let older = engine.clone();
        let adapter = fake.adapter().with_deadline(Duration::from_secs(10));
        let scenario = fake.scenario.clone();
        let pending =
            std::thread::spawn(move || older.open_adapter(adapter, scenario.to_str(), &Cancellation::default()));
        wait_for(&ready);
        let mut edited = read(invented());
        edited["revision"] = json!("r2");
        edited["records"][0]["volume"] = json!("99");
        if changed_scope {
            edited["scope"] = json!("invented-replacement");
        }
        fake.answer(edited);
        let (newest, _) = engine
            .open_adapter(fake.adapter(), fake.library(), &Cancellation::default())
            .unwrap();
        std::fs::write(&release, "release").unwrap();
        let older = pending.join().unwrap();
        let listed = libraries(&engine);
        assert_eq!(listed.len(), 1, "a stale scope must not return");
        assert_eq!(listed[0]["id"], json!(newest.id));
        assert_eq!(
            listed[0]["revision"],
            json!(newest.revision),
            "an older read must not publish"
        );
        assert_eq!(older.unwrap_err().code, ibis_core::contract::ErrorCode::Cancelled);
    }
}

fn wait_for(path: &Path) {
    let started = Instant::now();
    while !path.exists() {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "adapter did not reach its barrier"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn stale_alias_publication() {
    for (already_open, changed_scope) in [(false, false), (true, false), (true, true)] {
        let fake = Fake::new("fake");
        let engine = engine();
        let alias = fake.directory.path().join("alias.json");
        if already_open {
            std::fs::write(
                &alias,
                json!({"response":{"ibis":1,"read":read(invented())}}).to_string(),
            )
            .unwrap();
            engine
                .open_adapter(fake.adapter(), alias.to_str(), &Cancellation::default())
                .unwrap();
        }
        let ready = fake.directory.path().join("ready");
        let release = fake.directory.path().join("release");
        fake.scenario(json!({"mode":"paused-answer", "ready":ready, "release":release,
            "response":{"ibis":1,"read":read(invented())}}));
        let older = engine.clone();
        let adapter = fake.adapter().with_deadline(Duration::from_secs(10));
        let scenario = fake.scenario.clone();
        let pending =
            std::thread::spawn(move || older.open_adapter(adapter, scenario.to_str(), &Cancellation::default()));
        wait_for(&ready);

        let mut edited = read(invented());
        edited["revision"] = json!("r2");
        edited["records"][0]["volume"] = json!("99");
        if changed_scope {
            edited["scope"] = json!("invented-replacement");
        }
        std::fs::write(&alias, json!({"response":{"ibis":1,"read":edited}}).to_string()).unwrap();
        let (newest, _) = engine
            .open_adapter(fake.adapter(), alias.to_str(), &Cancellation::default())
            .unwrap();
        let newest_objects = objects_of(&engine);
        std::fs::write(&release, "release").unwrap();
        let older = pending.join().unwrap();

        let listed = libraries(&engine);
        assert_eq!(listed.len(), 1, "an alias must not restore a retired scope");
        assert_eq!(listed[0]["id"], json!(newest.id));
        assert_eq!(listed[0]["revision"], json!(newest.revision));
        assert_eq!(
            objects_of(&engine),
            newest_objects,
            "an alias must not restore old records"
        );
        assert_eq!(older.unwrap_err().code, ibis_core::contract::ErrorCode::Cancelled);
    }
}

#[cfg(unix)]
fn termination_at_eof() {
    use nix::sys::signal::{Signal, kill};
    use nix::unistd::Pid;
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    for signal in [Signal::SIGINT, Signal::SIGTERM] {
        for _ in 0..16 {
            let directory = tempfile::tempdir().unwrap();
            let mut ibis = std::process::Command::new(env!("CARGO_BIN_EXE_ibis"))
                .args(["mcp"])
                .env("HOME", directory.path())
                .env("XDG_CONFIG_HOME", directory.path())
                .env("APPDATA", directory.path())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let mut input = ibis.stdin.take().unwrap();
            let output = ibis.stdout.take().unwrap();
            let (ready, reply) = std::sync::mpsc::channel();
            let reader = std::thread::spawn(move || {
                let mut line = String::new();
                BufReader::new(output).read_line(&mut line).unwrap();
                ready.send(line).unwrap();
            });
            writeln!(
                input,
                "{}",
                json!({"jsonrpc":"2.0","id":1,"method":"initialize",
                "params":{"protocolVersion":ibis_engine::mcp::INITIALIZE_VERSIONS[0],
                "capabilities":{},"clientInfo":{"name":"invented","version":"1"}}})
            )
            .unwrap();
            // A reply proves startup and signal installation completed; no
            // startup sleep decides when it is safe to send the signal.
            let initialized = match reply.recv_timeout(Duration::from_secs(5)) {
                Ok(initialized) => initialized,
                Err(error) => {
                    let _ = ibis.kill();
                    let _ = ibis.wait();
                    let _ = reader.join();
                    panic!("Ibis did not initialize: {error}");
                }
            };
            reader.join().unwrap();
            kill(Pid::from_raw(i32::try_from(ibis.id()).unwrap()), signal).unwrap();
            drop(input);
            let started = Instant::now();
            let status = loop {
                if let Some(status) = ibis.try_wait().unwrap() {
                    break status;
                }
                if started.elapsed() > Duration::from_secs(5) {
                    let _ = ibis.kill();
                    let _ = ibis.wait();
                    panic!("Ibis did not finish after {signal} and stdin EOF");
                }
                std::thread::sleep(Duration::from_millis(1));
            };
            let initialized: Value = serde_json::from_str(&initialized).unwrap();
            assert!(initialized["result"].is_object(), "{initialized}");
            assert_eq!(status.code(), Some(128 + signal as i32), "{signal} must win over EOF");
        }
    }
}

#[cfg(unix)]
fn termination() {
    use nix::sys::signal::{Signal, kill, killpg};
    use nix::unistd::Pid;
    for signal in [Signal::SIGINT, Signal::SIGTERM] {
        let fake = Fake::new("fake");
        fake.configure(json!({"fake": fake.program}));
        let pid = fake.directory.path().join("pid");
        let adapter_pid = fake.directory.path().join("adapter-pid");
        fake.scenario(json!({"mode":"hang", "pid":pid, "adapter-pid":adapter_pid}));
        let directory = fake.directory.path();
        let mut ibis = std::process::Command::new(env!("CARGO_BIN_EXE_ibis"))
            .args(["libraries", "--source", &fake.source(), "--json"])
            .env("HOME", directory)
            .env("XDG_CONFIG_HOME", directory)
            .env("APPDATA", directory)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        wait_for(&adapter_pid);
        let adapter = Pid::from_raw(std::fs::read_to_string(&adapter_pid).unwrap().parse().unwrap());
        kill(Pid::from_raw(i32::try_from(ibis.id()).unwrap()), signal).unwrap();
        let started = Instant::now();
        let status = loop {
            if let Some(status) = ibis.try_wait().unwrap() {
                break status;
            }
            if started.elapsed() > Duration::from_secs(5) {
                let _ = ibis.kill();
                let _ = ibis.wait();
                let _ = killpg(adapter, Signal::SIGKILL);
                panic!("Ibis did not stop after {signal}");
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        let adapter_was_running = kill(adapter, None).is_ok();
        // Also clean up the reproducer when testing the unfixed implementation.
        if adapter_was_running {
            let _ = killpg(adapter, Signal::SIGKILL);
        }
        assert!(!adapter_was_running, "{signal} left its adapter running");
        assert!(kill(adapter, None).is_err(), "Ibis must reap its direct adapter child");
        assert_eq!(status.code(), Some(128 + signal as i32));
    }
}

fn daemon_crash() {
    use std::process::{Command, Stdio};
    let fake = Fake::new("fake");
    fake.configure(json!({"fake": fake.program}));
    let directory = fake.directory.path();
    let descendant = directory.join("pid");
    let parser = directory.join("adapter-pid");
    fake.scenario(json!({"mode":"hang", "pid":descendant, "adapter-pid":parser}));
    let cache = directory.join("cache");
    let program = Path::new(env!("CARGO_BIN_EXE_ibis"));
    let configured = || {
        let mut command = Command::new(program);
        command
            .env("HOME", directory)
            .env("XDG_CONFIG_HOME", directory)
            .env("APPDATA", directory)
            .env("LOCALAPPDATA", &cache)
            .env("XDG_RUNTIME_DIR", directory)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
    };
    let (config, root) = if cfg!(target_os = "macos") {
        (directory.join("Library/Application Support"), None)
    } else if cfg!(windows) {
        (directory.to_path_buf(), Some(cache.as_path()))
    } else {
        (directory.to_path_buf(), Some(directory))
    };
    let endpoint = ibis::daemon::Endpoint::new(&config, root).unwrap();
    let mut daemon = configured().arg("daemon").spawn().unwrap();
    let started = Instant::now();
    let socket = loop {
        if let Ok(socket) = endpoint.connect(program) {
            break socket;
        }
        if started.elapsed() > Duration::from_secs(5) {
            let _ = daemon.kill();
            let _ = daemon.wait();
            panic!("daemon failed to start");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(socket.status().pid, daemon.id());
    let mut client = configured()
        .args(["libraries", "--source", &fake.source(), "--json"])
        .spawn()
        .unwrap();
    wait_for(&parser);
    daemon.kill().unwrap();
    daemon.wait().unwrap();
    assert_stopped(&parser);
    assert_stopped(&descendant);
    assert_eq!(client.wait().unwrap().code(), Some(5));
}

fn adapter_directory() {
    let fake = Fake::new("fake");
    fake.configure(json!({"fake": fake.program}));
    fake.answer(read(invented()));
    let directory = fake.directory.path();
    let command = || {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_ibis"));
        command
            .env("HOME", directory)
            .env("XDG_CONFIG_HOME", directory)
            .env("APPDATA", directory)
            .env("LOCALAPPDATA", directory.join("cache"))
            .env("XDG_RUNTIME_DIR", directory);
        command
    };
    let warmed = command().args(["styles", "--limit", "1"]).output().unwrap();
    assert!(warmed.status.success());
    let actual = command()
        .current_dir(directory)
        .args(["libraries", "--source", "fake=scenario.json", "--json"])
        .output()
        .unwrap();
    assert!(actual.status.success(), "{}", String::from_utf8_lossy(&actual.stderr));
    let answer: Value = serde_json::from_slice(&actual.stdout).unwrap();
    assert_eq!(answer["data"]["libraries"].as_array().unwrap().len(), 1);
}

fn streaming_limit() {
    let fake = Fake::new("fake");
    let pid = fake.scenario.with_file_name("pid");
    fake.scenario(json!({"mode":"too-large", "pid":pid}));
    let engine = engine().with_external_sources([("fake".into(), fake.program.clone())].into());
    let started = Instant::now();
    let failure = call(
        &engine,
        Method::Libraries,
        json!({"action":"open-source", "name":"fake", "library":fake.library()}),
    )
    .unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(10), "refused mid-stream");
    assert_eq!(failure["code"], "source-unavailable", "{failure}");
    assert_eq!(failure["reason"], "source-failed", "{failure}");
    assert_stopped(&pid);
}

fn exited_with_child() {
    let fake = Fake::new("fake");
    let pid = fake.scenario.with_file_name("pid");
    fake.scenario(json!({"mode":"exit-with-child", "pid":pid, "response":{"ibis":1,"read":read(invented())}}));
    let engine = engine();
    engine
        .open_adapter(fake.adapter(), fake.library(), &Cancellation::default())
        .unwrap();
    assert_stopped(&pid);
}

fn assert_stopped(pid: &Path) {
    let pid = std::fs::read_to_string(pid).expect("the adapter actually spawned a descendant");
    let started = Instant::now();
    while still_running(&pid) {
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "a descendant is still running"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
fn still_running(pid: &str) -> bool {
    let result = std::process::Command::new("/bin/ps")
        .args(["-o", "stat=", "-p", pid])
        .output()
        .unwrap();
    let state = String::from_utf8_lossy(&result.stdout);
    // Normally init reaps killed grandchildren. A zombie in a test container
    // whose init doesn't reap has exited and holds no live resources.
    result.status.success() && !state.trim().starts_with('Z')
}

#[cfg(windows)]
fn still_running(pid: &str) -> bool {
    let result = std::process::Command::new("tasklist.exe")
        .args(["/NH", "/FO", "CSV", "/FI", &format!("PID eq {pid}")])
        .output()
        .unwrap();
    assert!(result.status.success());
    String::from_utf8_lossy(&result.stdout).lines().any(|line| {
        line.split(',')
            .nth(1)
            .is_some_and(|field| field.trim_matches('"') == pid)
    })
}
