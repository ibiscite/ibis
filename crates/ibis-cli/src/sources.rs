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
use ibis_engine::{Cancellation, DispatchResult, Engine, LibraryInfo, LibraryKind, SourceCapability};
use serde_json::{Value, json};

use crate::Failure;

pub(crate) fn command() -> Command {
    Command::new("sources")
        .about("Check that a library's source keeps Ibis's promises, or keep a folder's PDFs as references")
        .after_help(
            "Examples:\n  ibis sources check --source endnote=Invented.enl --expect fixtures/endnote/expected.json\n  ibis sources check --file refs.bib --changed-from REVISION\n  ibis sources keep --file ~/Papers",
        )
        .arg(
            Arg::new("action")
                .value_name("ACTION")
                .required(true)
                .value_parser(["check", "keep"])
                .help(
                    "check: read the libraries named here again and check what every source promises; \
                     keep: read each new PDF in the folders named with --file and keep what it says in \
                     the folder's ibis-references.bib",
                ),
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

/// One executable per name, in the person's Ibis config directory. This
/// file is separate from portable settings and is never supplied by a library.
pub(crate) type Configuration = BTreeMap<String, PathBuf>;
const CONFIG_FILE: &str = "com.ibiscite.ibis/source-adapters.json";
const MAX_CONFIG_BYTES: u64 = 64 * 1024;

pub(crate) fn configuration() -> Result<Configuration, &'static str> {
    super::bounded(read_configuration).unwrap_or(Err("source-adapter-config-timeout"))
}

#[allow(
    clippy::disallowed_methods,
    reason = "Read the local adapter approvals once, with a byte limit and on the bounded settings worker."
)]
fn read_configuration() -> Result<Configuration, &'static str> {
    use std::io::{ErrorKind, Read};
    let Some(directory) = ibis_engine::identity::config_dir().filter(|dir| dir.is_absolute()) else {
        return Ok(BTreeMap::new());
    };
    let file = match std::fs::File::open(directory.join(CONFIG_FILE)) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(_) => return Err("source-adapter-config-unreadable"),
    };
    let mut bytes = Vec::new();
    file.take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "source-adapter-config-unreadable")?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return Err("source-adapter-config-invalid");
    }
    serde_json::from_slice(&bytes).map_err(|_| "source-adapter-config-invalid")
}

pub(crate) fn configuration_report() -> Value {
    use ibis_engine::sources::SourceAdapter;
    match configuration() {
        Ok(configured) => {
            let adapters: Vec<_> = configured
                .into_iter()
                .map(|(name, path)| {
                    let approved = SourceAdapter::at(&name, path).is_ok();
                    json!({"name": name, "approved": approved})
                })
                .collect();
            let valid = adapters.iter().all(|adapter| adapter["approved"] == true);
            json!({"status": if valid { "ok" } else { "failed" }, "code": if valid { "source-adapter-config-ok" } else { "source-adapter-config-invalid" }, "adapters": adapters})
        }
        Err(code) => json!({"status": "failed", "code": code, "adapters": []}),
    }
}

/// `ibis sources keep`: each folder named with `--file` keeps its new PDFs,
/// a batch at a time until none waits (decision 0061). Run before any
/// library opens, since a folder no one has kept has no library yet.
pub(crate) fn keep(engine: &Engine, matches: &ArgMatches) -> Result<DispatchResult, Failure> {
    let folders = crate::source_values::<PathBuf>(matches, "file");
    if folders.is_empty() {
        return Err(Failure::new(
            ErrorCode::Usage,
            "name the folder to keep with --file, such as --file ~/Papers",
        ));
    }
    let (_, settings) = crate::settings_backup::settings_file(false)?;
    let store = ibis_engine::write::Proposals::beside(&settings);
    let cancel = Cancellation::default();
    let mut reports = Vec::new();
    let mut text = String::new();
    for folder in folders {
        let mut total = ibis_engine::write::Kept::default();
        let mut skip = std::collections::BTreeSet::new();
        loop {
            let kept = engine.keep_folder(&store, &folder, &skip, &cancel).map_err(|error| {
                Failure::new(
                    ErrorCode::SourceUnavailable,
                    ibis_engine::catalogue::problem(error.problem(), ibis_engine::catalogue::Audience::Cli),
                )
            })?;
            total.kept += kept.kept;
            total.unread += kept.unread;
            total.cut_short = kept.cut_short;
            skip.extend(kept.retried.iter().cloned());
            total.retried.extend(kept.retried);
            if kept.waiting == 0 {
                break;
            }
        }
        text.push_str(&crate::human::cli(
            "cli-folder-kept",
            &[
                ("folder", &folder.display().to_string()),
                ("kept", &total.kept.to_string()),
                ("unread", &total.unread.to_string()),
                ("retry", &total.retried.len().to_string()),
            ],
        ));
        text.push('\n');
        reports.push(json!({"folder": folder, "kept": total}));
    }
    Ok(DispatchResult {
        text: Some(text),
        data: json!({"folders": reports}),
        source_findings: false,
        exit_code: ExitCode::Ok,
    })
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
    initial_tokens: &BTreeMap<LibraryId, String>,
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
    let mut tokens: BTreeMap<LibraryId, [(LibraryInfo, String); 2]> = BTreeMap::new();
    for source in crate::source_values::<String>(matches, "source") {
        let (name, library) = crate::split_source(&source);
        let Some(first) = names
            .iter()
            .find(|(_, shown)| **shown == source)
            .map(|(id, _)| id.clone())
        else {
            continue;
        };
        let Some(first) = opened
            .iter()
            .find(|info| info.id.as_str() == first)
            .map(|info| info.id.clone())
        else {
            continue;
        };
        let first_read = engine.open_source(name, library, &cancel)?;
        let second_read = engine.open_source(name, library, &cancel)?;
        tokens.insert(first, [first_read, second_read]);
    }
    let mut report = Vec::new();
    let mut text = String::new();
    let mut all_passed = true;
    for info in &opened {
        let before = info.revision.clone();
        let mut checks = Vec::new();
        let mut current = info.id.clone();
        let mut token = None;
        if let Some([(first, first_token), (second, second_token)]) = tokens.get(&info.id) {
            let ids_agree = first.id == info.id && second.id == info.id;
            checks.push(Check {
                name: "same-scope-on-reread",
                passed: ids_agree,
            });
            checks.push(Check {
                name: "stable-revision-on-reread",
                passed: ids_agree && first.revision == before && second.revision == before,
            });
            checks.push(Check {
                name: "token-agrees-with-revision",
                passed: initial_tokens.get(&info.id).is_some_and(|initial| {
                    (initial == first_token) == (before == first.revision)
                        && (first_token == second_token) == (first.revision == second.revision)
                }),
            });
            current = second.id.clone();
            token = Some(second_token.clone());
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
        let shown = names
            .get(info.id.as_str())
            .cloned()
            .unwrap_or_else(|| info.id.as_str().to_owned());
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
    let unreadable = || {
        Failure::new(
            ErrorCode::BadInput,
            format!("{} is not a JSON array of CSL records", path.display()),
        )
    };
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
