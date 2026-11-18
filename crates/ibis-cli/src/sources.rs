//! `ibis sources check`: the conformance check every source passes, built in
//! or external (decision 0056, step 4). It reads the libraries this command
//! names again and checks what a source promises: a reread gives the same
//! library and revision, an adapter's own change token agrees with what Ibis
//! read, an adapter sends only valid CSL-JSON, a library without keys has
//! none, and, with `--expect`, the records are exactly the shared invented
//! fixture's. `--changed-from` checks, in a second run after an edit, that
//! the revision changed. Nothing is written anywhere.

use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::{Arg, ArgMatches, Command};
use ibis_core::contract::{ErrorCode, ExitCode, LibraryId, LibraryObject};
use ibis_engine::sources::SourceAdapter;
use ibis_engine::{Cancellation, DispatchResult, Engine, LibraryKind, SourceCapability};
use serde_json::{Value, json};

use crate::Failure;

pub(crate) fn command() -> Command {
    Command::new("sources")
        .about("Check that a library's source keeps Ibis's promises, built-in readers and source adapters alike")
        .after_help(
            "Examples:\n  ibis sources check --source endnote=Invented.enl --expect fixtures/endnote/expected.json\n  ibis sources check --file refs.bib --changed-from REVISION",
        )
        .arg(
            Arg::new("action")
                .value_name("ACTION")
                .required(true)
                .value_parser(["check"])
                .help("check: read the libraries named here again and check what every source promises"),
        )
        .arg(
            Arg::new("expect")
                .long("expect")
                .value_name("FILE")
                .value_parser(clap::value_parser!(PathBuf))
                .help("CSL-JSON records the library should hold exactly, without their ids, in any order"),
        )
        .arg(
            Arg::new("changed-from")
                .long("changed-from")
                .value_name("REVISION")
                .help("A revision an earlier check printed: check that the library changed since"),
        )
}

struct Check {
    name: &'static str,
    passed: bool,
}

pub(crate) fn run(
    engine: &Engine,
    matches: &ArgMatches,
    args: &ArgMatches,
    names: &BTreeMap<String, String>,
) -> Result<DispatchResult, Failure> {
    let cancel = Cancellation::default();
    let opened = engine.libraries()?.libraries;
    if opened.is_empty() {
        return Err(Failure::new(
            ErrorCode::Usage,
            "name the libraries to check, such as --source endnote=Invented.enl or --file refs.bib",
        ));
    }
    let expected = match args.get_one::<PathBuf>("expect") {
        Some(path) => Some(expected_records(path)?),
        None => None,
    };
    let changed_from = args.get_one::<String>("changed-from");
    // Each adapter read twice more, with its own token each time.
    let mut tokens: BTreeMap<LibraryId, Vec<(LibraryId, String)>> = BTreeMap::new();
    let program = std::env::current_exe().ok();
    let search = std::env::var_os("PATH");
    for source in crate::source_values::<String>(matches, "source") {
        let (name, library) = crate::split_source(&source);
        let Some(first) = names
            .iter()
            .find(|(_, shown)| **shown == source)
            .map(|(id, _)| id.clone())
        else {
            continue;
        };
        let Some(first) = opened.iter().find(|info| info.id.as_str() == first).map(|info| info.id.clone()) else {
            continue;
        };
        let adapter = SourceAdapter::find(name, program.as_deref(), search.as_deref())
            .map_err(|_| Failure::new(ErrorCode::SourceUnavailable, format!("no source adapter is called {name}")))?;
        let mut reads = Vec::new();
        for _ in 0..2 {
            reads.push(engine.open_adapter(adapter.clone(), library, &cancel)?);
        }
        tokens.insert(first, reads);
    }
    let mut report = Vec::new();
    let mut text = String::new();
    let mut all_passed = true;
    for info in &opened {
        let before = info.revision.clone();
        let mut checks = Vec::new();
        let mut current = info.id.clone();
        let mut token = None;
        if let Some(reads) = tokens.get(&info.id) {
            let ids_agree = reads.iter().all(|(id, _)| *id == info.id);
            checks.push(Check {
                name: "same-scope-on-reread",
                passed: ids_agree,
            });
            let revisions: Vec<_> = reads.iter().map(|(id, _)| revision(engine, id)).collect::<Result<_, _>>()?;
            let stable = ids_agree && revisions.iter().all(|revision| *revision == before.as_str());
            checks.push(Check {
                name: "stable-revision-on-reread",
                passed: stable,
            });
            checks.push(Check {
                name: "token-agrees-with-revision",
                passed: (reads[0].1 == reads[1].1) == (revisions[0] == revisions[1]),
            });
            current = reads[1].0.clone();
            token = Some(reads[1].1.clone());
        } else if info.kind != LibraryKind::Snapshot {
            let request = json!({"action": "reload", "library": info.id});
            crate::dispatch(engine, ibis_engine::Method::Libraries, &request, &cancel)?;
            checks.push(Check {
                name: "stable-revision-on-reread",
                passed: revision(engine, &info.id)? == before.as_str(),
            });
        }
        let checked = engine
            .libraries()?
            .libraries
            .into_iter()
            .find(|library| library.id == current)
            .ok_or(Failure::new(ErrorCode::Internal, String::new()))?;
        let records = records(engine, &current)?;
        let keyless = !checked.capabilities.contains(&SourceCapability::NativeCitationKeys);
        if keyless {
            checks.push(Check {
                name: "no-keys-without-key-capability",
                passed: records.iter().all(|(_, keys)| !keys),
            });
        }
        if checked.kind == LibraryKind::Adapter {
            // Ibis's own checks name a record's place; an adapter's skips do not.
            checks.push(Check {
                name: "adapter-sends-valid-csl",
                passed: checked.warnings.iter().all(|warning| warning.line.is_none()),
            });
        }
        if let Some(expected) = &expected {
            let mut found: Vec<String> = records.iter().map(|(csl, _)| csl.to_string()).collect();
            found.sort();
            checks.push(Check {
                name: "records-match-expected",
                passed: found == *expected,
            });
        }
        if let Some(earlier) = changed_from {
            checks.push(Check {
                name: "revision-changed",
                passed: checked.revision.as_str() != earlier,
            });
        }
        let passed = checks.iter().all(|check| check.passed);
        all_passed &= passed;
        let shown = names.get(info.id.as_str()).cloned().unwrap_or_else(|| info.id.as_str().to_owned());
        text.push_str(&format!(
            "{shown}: {} records, {} warnings, revision {}\n",
            records.len(),
            checked.warnings_total,
            checked.revision.as_str()
        ));
        for check in &checks {
            text.push_str(&format!(
                "  {} {}\n",
                if check.passed { "ok  " } else { "FAIL" },
                check.name
            ));
        }
        report.push(json!({
            "library": current,
            "name": shown,
            "kind": checked.kind,
            "revision": checked.revision,
            "token": token,
            "records": records.len(),
            "warnings": checked.warnings_total,
            "passed": passed,
            "checks": checks.iter().map(|check| json!({"check": check.name, "passed": check.passed})).collect::<Vec<_>>(),
        }));
    }
    Ok(DispatchResult {
        data: json!({"passed": all_passed, "libraries": report}),
        text: Some(text.trim_end().to_owned()),
        source_findings: false,
        exit_code: if all_passed { ExitCode::Ok } else { ExitCode::Findings },
    })
}

fn revision(engine: &Engine, library: &LibraryId) -> Result<String, Failure> {
    Ok(engine
        .libraries()?
        .libraries
        .into_iter()
        .find(|info| info.id == *library)
        .map(|info| info.revision.as_str().to_owned())
        .unwrap_or_default())
}

/// Each record's CSL without its `id`, and whether it has a citation key.
fn records(engine: &Engine, library: &LibraryId) -> Result<Vec<(Value, bool)>, Failure> {
    Ok(engine
        .snapshot()?
        .objects()
        .iter()
        .filter_map(|object| match &object.object {
            LibraryObject::Record(record) if record.id.library == *library => {
                let mut csl = serde_json::to_value(&record.csl).unwrap_or_default();
                if let Some(fields) = csl.as_object_mut() {
                    fields.remove("id");
                }
                Some((csl, !record.citation_keys.is_empty()))
            }
            _ => None,
        })
        .collect())
}

/// The expected records, each as compact JSON, sorted.
#[allow(
    clippy::disallowed_methods,
    reason = "A fixture the person names for this check, read once and never written."
)]
fn expected_records(path: &std::path::Path) -> Result<Vec<String>, Failure> {
    let unreadable = || Failure::new(ErrorCode::BadInput, format!("{} is not a JSON array of CSL records", path.display()));
    let text = std::fs::read_to_string(path).map_err(|_| unreadable())?;
    let values: Vec<Value> = serde_json::from_str(&text).map_err(|_| unreadable())?;
    let mut records: Vec<String> = values
        .into_iter()
        .map(|mut value| {
            if let Some(fields) = value.as_object_mut() {
                fields.remove("id");
            }
            value.to_string()
        })
        .collect();
    records.sort();
    Ok(records)
}
