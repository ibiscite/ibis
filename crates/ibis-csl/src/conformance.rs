//! The CSL project's own processor tests (citation-style-language/test-suite),
//! run through the formatter Ibis ships. Three columns, one driver
//! (`tests/conformance/driver.js`):
//!
//! - citeproc-js: the pinned processor under Node, without Ibis's adapter;
//! - runtime: the shipped QuickJS runtime with ICU collation, locale lookup and
//!   the adapter's processor patches;
//! - Ibis: the runtime again, with each record read the way Ibis reads one
//!   (`CitableFields`, the processor fields and the adapter's item conversion).
//!
//! A fixture's cause follows from the columns: failing under Node is the
//! processor's; passing there but failing in the runtime is the runtime's;
//! passing in the runtime but failing with Ibis's records is the mapping's.
//! The Ibis column's failures must be exactly `known-failures.json`, so the list
//! can only shrink, and `CONFORMANCE.md` must be what this run writes.
//!
//! The suite comes from `scripts/csl-suite.mjs` at the commit in `suite.json`.

use crate::Limits;
use crate::runtime::Session;
use crate::service::Control;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const DRIVER: &str = include_str!("../tests/conformance/driver.js");
const SECTIONS: [&str; 8] = [
    "CSL",
    "INPUT",
    "MODE",
    "RESULT",
    "CITATION-ITEMS",
    "CITATIONS",
    "DESCRIPTION",
    "VERSION",
];
const FIXTURE_TIME: Duration = Duration::from_secs(30);

fn here() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/conformance")
}

struct Fixture {
    name: String,
    expected: String,
    /// What the driver reads: { csl, mode, input, citationItems?, citations? }.
    driven: Value,
}

/// The suite's human format: `>>===== NAME =====>>` lines open a section and
/// `<<===== NAME =====<<` lines close it, as citeproc-js's runner reads them.
fn parse(name: &str, text: &str) -> Fixture {
    let mut sections = BTreeMap::<&str, Vec<&str>>::new();
    let mut open: Option<&str> = None;
    let marker = |line: &str, start: &str, end: &str| {
        let (_, rest) = line.split_once(start)?;
        let rest = rest.trim_start_matches('=');
        let word = rest.strip_prefix(char::is_whitespace)?.split_whitespace().next()?;
        let after = rest.split_once(word)?.1;
        (after.contains(end) && after.starts_with(char::is_whitespace))
            .then(|| SECTIONS.into_iter().find(|section| *section == word))
            .flatten()
    };
    for line in text.split('\n').map(|line| line.strip_suffix('\r').unwrap_or(line)) {
        if let Some(section) = marker(line, ">>=", "=>>") {
            assert!(open.is_none(), "{name}: {section} opens inside another section");
            open = Some(section);
            sections.insert(section, Vec::new());
        } else if let Some(section) = marker(line, "<<=", "=<<") {
            assert_eq!(open, Some(section), "{name}: unexpected close of {section}");
            open = None;
        } else if let Some(section) = open {
            sections.get_mut(section).unwrap().push(line);
        }
    }
    let text = |section: &str| sections.get(section).map(|lines| lines.join("\n"));
    let parsed = |section: &str| {
        text(section).map(|json| {
            serde_json::from_str::<Value>(&json).unwrap_or_else(|error| panic!("{name}: {section}: {error}"))
        })
    };
    let required = |section: &str| text(section).unwrap_or_else(|| panic!("{name}: no {section}"));
    Fixture {
        name: name.trim_end_matches(".txt").to_owned(),
        expected: text("RESULT").unwrap_or_default(),
        driven: json!({
            "csl": required("CSL"),
            "mode": required("MODE").trim(),
            "input": parsed("INPUT").unwrap_or_else(|| panic!("{name}: no INPUT")),
            "citationItems": parsed("CITATION-ITEMS"),
            "citations": parsed("CITATIONS"),
        }),
    }
}

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Every pinned fixture, byte for byte; a missing or changed file is a failure.
fn fixtures(manifest: &Value) -> Vec<Fixture> {
    let suite = std::env::var_os("IBIS_CSL_SUITE").map_or_else(|| here().join("suite"), PathBuf::from);
    let files = manifest["files"].as_object().expect("suite.json lists its files");
    let mut present = BTreeSet::new();
    if let Ok(entries) = std::fs::read_dir(&suite) {
        for entry in entries.flatten() {
            present.insert(entry.file_name().to_string_lossy().into_owned());
        }
    }
    assert!(
        !present.is_empty(),
        "no CSL test suite at {}: run `node scripts/csl-suite.mjs`",
        suite.display()
    );
    let unlisted: Vec<_> = present
        .iter()
        .filter(|name| name.ends_with(".txt") && !files.contains_key(*name))
        .collect();
    assert!(unlisted.is_empty(), "fixtures not in suite.json: {unlisted:?}");
    files
        .iter()
        .map(|(name, sha)| {
            let bytes = std::fs::read(suite.join(name))
                .unwrap_or_else(|_| panic!("{name} is missing: run `node scripts/csl-suite.mjs`"));
            assert_eq!(
                Some(hex(&bytes).as_str()),
                sha.as_str(),
                "{name} differs from the pinned suite"
            );
            parse(name, &String::from_utf8(bytes).expect("fixtures are UTF-8"))
        })
        .collect()
}

/// What the processor printed, or why it printed nothing.
type Outcome = Result<String, String>;

fn node(fixtures: &[Fixture]) -> Vec<Outcome> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new(std::env::var("IBIS_NODE").unwrap_or_else(|_| "node".into()))
        .arg(here().join("node.mjs"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("the citeproc-js column needs Node (set IBIS_NODE)");
    let input = serde_json::to_vec(&fixtures.iter().map(|fixture| &fixture.driven).collect::<Vec<_>>()).unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap().unwrap();
    assert!(output.status.success(), "the citeproc-js column failed");
    let results: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(results.len(), fixtures.len());
    results
        .into_iter()
        .map(|result| match result["output"].as_str() {
            Some(text) => Ok(text.to_owned()),
            None => Err(result["error"].as_str().unwrap_or("no output").to_owned()),
        })
        .collect()
}

fn control() -> Control {
    Control {
        cancel: Arc::new(AtomicBool::new(false)),
        stopping: Arc::new(AtomicBool::new(false)),
        deadline: Instant::now() + FIXTURE_TIME,
    }
}

/// One fixture in the shipped runtime: as written, or with Ibis's records.
fn quickjs(session: &Session, fixture: &Fixture, mapped: bool) -> Outcome {
    let items = if mapped {
        let mut items = Vec::new();
        for item in fixture.driven["input"].as_array().ok_or("INPUT is not a list")? {
            let mut record = crate::input::processor_item(item.as_object().ok_or("an item is not an object")?.clone())
                .map_err(|error| format!("Ibis refused a record: {error}"))?;
            record.insert("id".into(), item["id"].clone());
            items.push(Value::Object(record));
        }
        Some(json!({ "items": items }))
    } else {
        None
    };
    session.with_context(&control(), |ctx| {
        let run = || -> rquickjs::Result<String> {
            let driven = ctx.json_parse(fixture.driven.to_string())?;
            let fixture: rquickjs::Function = ctx.globals().get("__cslFixture")?;
            let Some(items) = &items else {
                return fixture.call((driven,));
            };
            let open: rquickjs::Object = ctx.globals().get("__ibisItems")?;
            let converted: rquickjs::Value = open
                .get::<_, rquickjs::Function>("open")?
                .call((ctx.json_parse(items.to_string())?,))?;
            let result = fixture.call((driven, converted));
            open.get::<_, rquickjs::Function>("close")?.call::<_, ()>(())?;
            result
        };
        run().map_err(|error| {
            let exception = ctx.catch();
            let message = exception
                .as_object()
                .and_then(|object| object.get::<_, String>("message").ok())
                .or_else(|| exception.as_string().and_then(|text| text.to_string().ok()))
                .unwrap_or_else(|| error.to_string());
            message.chars().take(200).collect()
        })
    })
}

/// A fresh shipped runtime with the driver loaded.
fn session() -> Session {
    let session = Session::new(&Limits::default(), &control()).expect("the runtime starts");
    session
        .with_context(&control(), |ctx| ctx.eval::<(), _>(DRIVER))
        .expect("the driver loads");
    session
}

/// The runtime and Ibis columns, on as many worker threads as `threads`.
fn shipped(fixtures: &[Fixture], threads: usize) -> Vec<(Outcome, Outcome)> {
    let next = AtomicUsize::new(0);
    let results = Mutex::new(BTreeMap::new());
    std::thread::scope(|scope| {
        for _ in 0..threads {
            std::thread::Builder::new()
                .stack_size(crate::WORKER_STACK_BYTES)
                .spawn_scoped(scope, || {
                    let mut session = session();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(fixture) = fixtures.get(index) else { break };
                        let runtime = quickjs(&session, fixture, false);
                        let ibis = quickjs(&session, fixture, true);
                        // A runtime that hit a limit is not reused.
                        if [&runtime, &ibis].iter().any(|outcome| outcome.is_err()) {
                            session = self::session();
                        }
                        results.lock().unwrap().insert(index, (runtime, ibis));
                    }
                })
                .unwrap();
        }
    });
    results.into_inner().unwrap().into_values().collect()
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Cause {
    Processor,
    Runtime,
    Mapping,
}
impl Cause {
    fn label(self) -> &'static str {
        match self {
            Self::Processor => "citeproc-js",
            Self::Runtime => "runtime",
            Self::Mapping => "Ibis's records",
        }
    }
}

struct Row<'a> {
    fixture: &'a Fixture,
    node: bool,
    runtime: bool,
    ibis: bool,
    detail: Option<String>,
}
impl Row<'_> {
    fn cause(&self) -> Option<Cause> {
        if self.ibis {
            None
        } else if !self.node {
            Some(Cause::Processor)
        } else if !self.runtime {
            Some(Cause::Runtime)
        } else {
            Some(Cause::Mapping)
        }
    }
}

fn report(manifest: &Value, rows: &[Row<'_>], reasons: &BTreeMap<&str, &str>) -> String {
    let count = |pass: fn(&Row<'_>) -> bool| rows.iter().filter(|row| pass(row)).count();
    let total = rows.len();
    let percent = |passed: usize| format!("{passed} of {total} ({:.1}%)", 100.0 * passed as f64 / total as f64);
    let suite = &manifest["suite"];
    let mut out = format!(
        "# CSL conformance\n\n\
         Generated by `cargo test -p ibis-csl --lib conformance -- --ignored` with\n\
         `IBIS_CONFORMANCE_UPDATE=1`; do not edit by hand. The run fails when a fixture\n\
         starts or stops failing without a change to `tests/conformance/known-failures.json`.\n\n\
         The suite is the CSL project's processor tests, [{repo}](https://github.com/{repo})\n\
         at `{commit}`: the {total} fixtures in `processor-tests/humans`, without its\n\
         `experiments` and `off-spec` folders. Each runs as citeproc-js's own test runner\n\
         runs it and must match its expected output byte for byte.\n\n\
         | Column | Passes |\n|---|---|\n\
         | citeproc-js under Node, without Ibis's adapter | {node} |\n\
         | Ibis's QuickJS runtime, ICU collation and adapter patches | {runtime} |\n\
         | **Ibis**: that runtime with records read the way Ibis reads them | **{ibis}** |\n\n\
         citeproc-js publishes no per-fixture list; its CI requires every fixture to pass\n\
         at the suite commit it pins (`d2bd47f`, August 2024), with its own locales and\n\
         some fixtures replaced by local copies. So a fixture that passes in the first row\n\
         and fails in a later one is a gap in Ibis, not in citeproc-js.\n\n\
         ## Failures\n\n| Fixture | Cause | Why |\n|---|---|---|\n",
        repo = suite["repo"].as_str().unwrap(),
        commit = suite["commit"].as_str().unwrap(),
        node = percent(count(|row| row.node)),
        runtime = percent(count(|row| row.runtime)),
        ibis = percent(count(|row| row.ibis)),
    );
    let mut failing: Vec<_> = rows.iter().filter_map(|row| Some((row.cause()?, row))).collect();
    failing.sort_by(|(a, x), (b, y)| a.cmp(b).then(x.fixture.name.cmp(&y.fixture.name)));
    for (cause, row) in failing {
        let name = &row.fixture.name;
        let why = reasons.get(name.as_str()).copied().unwrap_or("not yet explained");
        out.push_str(&format!("| `{name}` | {} | {why} |\n", cause.label()));
    }
    out.push_str("\n## Passing\n\n");
    let passing: Vec<_> = rows
        .iter()
        .filter(|row| row.ibis)
        .map(|row| format!("`{}`", row.fixture.name))
        .collect();
    out.push_str(&passing.join(", "));
    out.push('\n');
    out
}

/// Run by hand and by the weekly styles workflow, with the suite fetched.
#[test]
#[ignore = "needs the CSL test suite: node scripts/csl-suite.mjs"]
fn csl_test_suite_failures_are_exactly_the_known_ones() {
    let manifest: Value = serde_json::from_str(&std::fs::read_to_string(here().join("suite.json")).unwrap()).unwrap();
    let fixtures = fixtures(&manifest);
    let threads = std::env::var("IBIS_CONFORMANCE_THREADS")
        .ok()
        .and_then(|threads| threads.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get().min(8)));
    let started = Instant::now();
    let node = node(&fixtures);
    let shipped = shipped(&fixtures, threads);
    eprintln!(
        "{} fixtures in {:.1?} on {threads} threads",
        fixtures.len(),
        started.elapsed()
    );
    let rows: Vec<_> = fixtures
        .iter()
        .zip(node)
        .zip(shipped)
        .map(|((fixture, node), (runtime, ibis))| {
            let pass = |outcome: &Outcome| outcome.as_deref() == Ok(fixture.expected.as_str());
            let detail = (!pass(&ibis)).then(|| match &ibis {
                Ok(text) => format!("expected:\n{}\nIbis:\n{text}", fixture.expected),
                Err(error) => format!("Ibis failed: {error}"),
            });
            Row {
                fixture,
                node: pass(&node),
                runtime: pass(&runtime),
                ibis: pass(&ibis),
                detail,
            }
        })
        .collect();

    let known: BTreeMap<String, Vec<String>> =
        serde_json::from_str(&std::fs::read_to_string(here().join("known-failures.json")).unwrap()).unwrap();
    let mut reasons = BTreeMap::new();
    for (reason, names) in &known {
        assert!(
            !reason.trim().is_empty() && !reason.contains('|'),
            "{reason:?} is not a one-line reason"
        );
        for name in names {
            assert!(
                reasons.insert(name.as_str(), reason.as_str()).is_none(),
                "{name} is listed twice"
            );
        }
    }
    let mut problems = Vec::new();
    for row in &rows {
        let name = row.fixture.name.as_str();
        match (row.ibis, reasons.contains_key(name)) {
            (false, false) => problems.push(format!(
                "{name} fails ({}), and is not a known failure\n{}",
                row.cause().unwrap().label(),
                row.detail.as_deref().unwrap_or_default()
            )),
            (true, true) => problems.push(format!("{name} passes now: remove it from known-failures.json")),
            _ => {}
        }
    }
    let names: BTreeSet<_> = rows.iter().map(|row| row.fixture.name.as_str()).collect();
    problems.extend(
        reasons
            .keys()
            .filter(|name| !names.contains(*name))
            .map(|name| format!("{name} is not in the suite")),
    );
    let written = report(&manifest, &rows, &reasons);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("CONFORMANCE.md");
    if std::env::var_os("IBIS_CONFORMANCE_UPDATE").is_some() {
        std::fs::write(&path, &written).unwrap();
    } else if std::fs::read_to_string(&path).ok().as_deref() != Some(written.as_str()) {
        problems.push("CONFORMANCE.md is out of date: run again with IBIS_CONFORMANCE_UPDATE=1".into());
    }
    assert!(problems.is_empty(), "{}", problems.join("\n\n"));
}

#[test]
fn fixture_sections_read_as_the_reference_runner_reads_them() {
    let fixture = parse(
        "group_Invented.txt",
        ">>===== MODE =====>>\ncitation\n<<===== MODE =====<<\n\n\
         >>==== RESULT ====>>\nOne\n\nTwo\n<<==== RESULT ====<<\r\n\
         >>===== CSL =====>>\n<style/>\n<<===== CSL =====<<\n\
         >>===== INPUT =====>>\n[{\"id\": \"ITEM-1\"}]\n<<===== INPUT =====<<\n",
    );
    assert_eq!(fixture.name, "group_Invented");
    assert_eq!(fixture.expected, "One\n\nTwo");
    assert_eq!(fixture.driven["mode"], "citation");
    assert_eq!(fixture.driven["input"][0]["id"], "ITEM-1");
    assert!(fixture.driven["citations"].is_null());
}
