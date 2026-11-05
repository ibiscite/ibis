//! One-shot, read-only diagnostics. No source watches, discovery, API requests,
//! database copies, settings recovery writes, or raw errors belong here.
use super::{OutputMode, source_values};
use clap::ArgMatches;
use ibis_core::{contract::*, settings};
use ibis_engine::{Cancellation, DispatchResult, Engine, LibraryInfo, Method, health};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub(super) fn run(engine: &Engine, matches: &ArgMatches, mode: OutputMode) -> Result<DispatchResult, EngineError> {
    let args = matches
        .subcommand_matches("doctor")
        .ok_or(EngineError::new(ErrorCode::Usage))?;
    let cancel = Cancellation::default();
    let ids_last = health::inspect_identity(&cancel)?;
    let config = config_check(args.get_one::<PathBuf>("config").map(PathBuf::as_path), &cancel);
    // Capture existing state once. A later check cannot relabel this observation.
    let (_, sources) = engine.health()?;
    let mut sources: Vec<_> = sources.iter().map(library_report).collect();
    // Each selected file owns its result; late native I/O cannot revise the report.
    sources.extend(
        source_values::<PathBuf>(matches, "file")
            .iter()
            .map(|path| file_report(path, &cancel)),
    );
    let groups = source_values::<u64>(matches, "zotero-group");
    for _ in 0..usize::from(matches.get_flag("zotero")) + groups.len() {
        sources.push(unavailable("zotero", "not-checked", "zotero-not-checked"));
    }
    for (index, source) in sources.iter_mut().enumerate() {
        // Stable within this report; no private location is needed to identify a check.
        source["index"] = json!(index + 1);
    }
    let findings =
        sources.is_empty() || config["status"] == "failed" || sources.iter().any(|source| source["status"] != "ok");
    let library_check = if sources.is_empty() {
        check("failed", "no-library")
    } else {
        check("ok", "sources-reported")
    };
    let key_outputs: Vec<_> = ibis_core::syntax::CAPABILITIES
        .iter()
        .map(|capability| {
            json!({
                "id":capability.id, "syntax":capability.syntax, "command":capability.command,
                "forms":capability.forms, "single_item_forms":capability.single_item_forms,
            })
        })
        .collect();
    let styles = engine
        .dispatch(Method::Styles, &json!({}), &Cancellation::default())?
        .data;
    let library_ids = if ids_last {
        check("ok", "ids-last")
    } else {
        check("not-checked", "ids-this-run")
    };
    let data = json!({
        "contract_version": Version,
        "cli_version": ibis_core::VERSION,
        "stdout_tty": mode == OutputMode::Terminal,
        "capabilities": {"methods":Method::ALL, "key_outputs":key_outputs, "csl_styles":{
            "count":styles["total"], "locales":styles["locales"].as_array().map_or(0, Vec::len),
            "styles_release":styles["styles_release"], "locales_release":styles["locales_release"],
        }},
        "config":config,
        "library_check":library_check,
        "library_ids":library_ids,
        "sources":sources,
    });
    let text = report_text(&data, unix_ms());
    Ok(DispatchResult {
        data,
        text: Some(text),
        exit_code: if findings { ExitCode::Findings } else { ExitCode::Ok },
        // The report already includes warnings; do not emit a second generic diagnostic.
        source_findings: false,
    })
}

fn unix_ms() -> u64 {
    let since_epoch = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
    since_epoch.map_or(0, |elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
}

/// The report for a person: a plain verdict line per check, then one conclusion.
/// `--json` keeps every field; only these words are free to change.
fn report_text(data: &Value, now_ms: u64) -> String {
    let mut text = format!(
        "Ibis {} · contract {} · {} engine methods, {} key outputs, {} styles\n\n",
        ibis_core::VERSION,
        data["contract_version"],
        data["capabilities"]["methods"].as_array().map_or(0, Vec::len),
        data["capabilities"]["key_outputs"].as_array().map_or(0, Vec::len),
        data["capabilities"]["csl_styles"].as_array().map_or(0, Vec::len),
    );
    let mut checks = vec![("Settings".to_owned(), &data["config"])];
    // Each library has its own line; this one only matters when there are none.
    if data["library_check"]["status"] != "ok" {
        checks.push(("Libraries".to_owned(), &data["library_check"]));
    }
    for source in data["sources"].as_array().into_iter().flatten() {
        let kind = source["kind"].as_str().unwrap_or("source");
        checks.push((format!("Library {} ({kind})", source["index"]), source));
    }
    checks.push(("Library ids".to_owned(), &data["library_ids"]));
    for (label, result) in &checks {
        text.push_str(&summary(label, result));
        if let (Some("stale"), Some(time)) = (
            result["freshness"]["state"].as_str(),
            result["freshness"]["updated_at_ms"].as_u64(),
        ) {
            text.push_str(&format!(
                "      Last good read: {}.\n",
                age(now_ms.saturating_sub(time))
            ));
        }
    }
    let problems = checks
        .iter()
        .filter(|(_, result)| matches!(result["status"].as_str(), Some("findings" | "failed")))
        .count();
    text.push('\n');
    text.push_str(&match problems {
        0 => "All good.".to_owned(),
        1 => "1 thing to look at.".to_owned(),
        count => format!("{count} things to look at."),
    });
    text
}

fn summary(label: &str, result: &Value) -> String {
    let message = result["message"].as_str().unwrap_or("Check failed.");
    let code = result["code"].as_str().unwrap_or("internal");
    match result["status"].as_str() {
        Some("ok") => format!("ok    {label}: {message}\n"),
        Some("findings") => format!("warn  {label}: {message} [{code}]\n"),
        Some("not-checked") => format!("skip  {label}: {message}\n"),
        _ => format!("fail  {label}: {message} [{code}]\n"),
    }
}

fn age(elapsed_ms: u64) -> String {
    let minutes = elapsed_ms / 60_000;
    let (count, unit) = match minutes {
        0 => return "less than a minute ago".to_owned(),
        1..60 => (minutes, "minute"),
        60..1440 => (minutes / 60, "hour"),
        _ => (minutes / 1440, "day"),
    };
    format!("{count} {unit}{} ago", if count == 1 { "" } else { "s" })
}

fn check(status: &str, reason: &str) -> Value {
    let code = match reason {
        "path-not-utf8" => "library-invalid",
        "library-permission" => "library-unavailable",
        "config-timeout" => "config-unreadable",
        "config-not-found" => "config-missing",
        _ => reason,
    };
    let message = match reason {
        "no-library" => "No library selected. Give --file <library.bib> (repeat for several libraries).",
        "sources-reported" => "Each selected source has its own result below.",
        "ids-last" => "Library ids last across runs on this computer.",
        "ids-this-run" => {
            "Library ids hold for this run: there is no usable saved key, and doctor writes none. Any other command saves one where it can."
        }
        "zotero-not-checked" => {
            "Zotero was not checked: its loader contacts the local API and may copy the database. Doctor sends nothing and writes nothing. Export a file and use --file to check it offline."
        }
        "config-not-requested" => {
            "Use --config <settings.json> to check a chosen settings file. CLI library choices come from flags."
        }
        "config-timeout" => "The chosen config could not be read in time. It was left unchanged.",
        "config-missing" => "The chosen config does not exist.",
        "config-not-found" => "The chosen config does not exist. It was left unchanged.",
        "config-ok" => "The chosen config is valid.",
        "config-invalid" => "The chosen config is invalid. It was left unchanged.",
        "config-newer-format" => "The chosen config needs a newer Ibis. It was left unchanged.",
        "config-unreadable" => "The chosen config could not be read. Check its permissions and file type.",
        "path-not-utf8" => "The selected path is not UTF-8. Choose a file with a UTF-8 path.",
        "library-missing" => "The selected file does not exist. Check the --file argument.",
        "library-permission" => "The selected file could not be read. Check its permissions.",
        "library-not-regular" => "The selected library must be a regular file.",
        "library-invalid" => "The selected file is not a supported, readable library.",
        "resource-limit" => "The selected library exceeds an Ibis resource limit.",
        "newer-format" => "The selected library needs a newer Ibis.",
        "internal" => "The selected library check failed internally.",
        "refresh-failed" => "The last refresh failed; this is the last successful snapshot.",
        "source-warnings" => {
            "The library loaded with warnings. See them with ibis libraries and the same library options."
        }
        "source-ok" => "The library was read successfully.",
        _ => "The selected file could not be read. Check its permissions or try again when it stops changing.",
    };
    json!({"status":status, "code":code, "message":message})
}

fn config_check(path: Option<&Path>, cancel: &Cancellation) -> Value {
    path.map_or_else(
        || check("not-checked", "config-not-requested"),
        |path| config_report(health::inspect_config(path.to_owned(), cancel)),
    )
}

fn config_report(loaded: Option<settings::Loaded>) -> Value {
    let Some(loaded) = loaded else {
        let mut result = check("failed", "config-timeout");
        result["from_backup"] = json!(false);
        return result;
    };
    let reason = match loaded.problem.as_ref() {
        Some(problem) => config_problem(problem),
        None if loaded.primary_read == settings::PrimaryRead::Missing => "config-missing",
        None => "config-ok",
    };
    let mut result = check(if reason == "config-ok" { "ok" } else { "failed" }, reason);
    result["from_backup"] = json!(loaded.from_backup);
    result
}

fn config_problem(problem: &settings::SettingsError) -> &'static str {
    match problem {
        settings::SettingsError::Invalid(_) | settings::SettingsError::InvalidZoteroLibraryEntry { .. } => {
            "config-invalid"
        }
        settings::SettingsError::NewerVersion(_) => "config-newer-format",
        settings::SettingsError::Backup(problem) => config_problem(problem),
        settings::SettingsError::Unreadable(error) if error.kind() == std::io::ErrorKind::NotFound => {
            "config-not-found"
        }
        settings::SettingsError::Unreadable(_) => "config-unreadable",
    }
}

fn file_report(path: &Path, cancel: &Cancellation) -> Value {
    health::inspect_file(health::InspectionPolicy::ReadOnly, path, cancel).map_or_else(
        |reason| unavailable("file", "failed", reason),
        |info| library_report(&info),
    )
}

fn unavailable(kind: &str, status: &str, code: &str) -> Value {
    let mut result = check(status, code);
    result["kind"] = json!(kind);
    result["freshness"] = json!({"state":"unknown", "updated_at_ms":null});
    result
}

fn library_report(library: &LibraryInfo) -> Value {
    let (status, code) = health::classify(library);
    let mut result = check(status, code);
    result["kind"] = json!(library.kind);
    result["id"] = json!(library.id);
    result["warning_count"] = json!(library.warnings_total);
    result["refresh_error"] = json!(library.refresh_error);
    result["freshness"] = json!({"state":if code == "refresh-failed" { "stale" } else { "current" }, "updated_at_ms":library.updated_at_ms});
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_failed_refresh_reports_the_retained_snapshot_as_stale() {
        let library = LibraryInfo {
            id: LibraryId::digest(b"invented-source"),
            kind: ibis_engine::LibraryKind::File,
            capabilities: vec![],
            objects: 3,
            warnings: vec![],
            warnings_total: 0,
            refresh_error: Some(ErrorCode::SourceUnavailable),
            refresh_reason: None,
            updated_at_ms: 123,
            watch_warning: None,
            zotero: None,
            duplicate_keys: Default::default(),
        };
        let source = library_report(&library);
        assert_eq!(source["status"], "findings");
        assert_eq!(source["code"], "refresh-failed");
        assert_eq!(source["freshness"], json!({"state":"stale", "updated_at_ms":123}));
        assert!(summary("Source 1", &source).contains("last successful snapshot"));
    }

    #[test]
    fn a_stalled_config_reports_unreadable_without_recovery() {
        let result = config_report(None);
        assert_eq!(result["status"], "failed");
        assert_eq!(result["code"], "config-unreadable");
        assert_eq!(
            result["message"],
            "The chosen config could not be read in time. It was left unchanged."
        );
        assert_eq!(result["from_backup"], false);
    }

    fn report(sources: Vec<Value>) -> Value {
        json!({
            "contract_version": 1,
            "capabilities": {"methods": ["search", "cite"], "key_outputs": [{}], "csl_styles": [{}, {}, {}]},
            "config": {"status":"not-checked", "code":"config-not-requested", "message":"Add --config to check settings."},
            "library_check": {"status":"ok", "code":"sources-reported", "message":"Each selected source has its own result below."},
            "sources": sources,
        })
    }

    fn source(status: &str, code: &str, freshness: Value) -> Value {
        let mut source = json!({"status":status, "code":code, "message":"The fixed message."});
        source["kind"] = json!("file");
        source["index"] = json!(1);
        source["freshness"] = freshness;
        source
    }

    #[test]
    fn a_healthy_report_ends_in_all_good_and_a_skipped_check_is_not_a_problem() {
        let current = json!({"state":"current", "updated_at_ms":5});
        let text = report_text(&report(vec![source("ok", "source-ok", current)]), 10);
        assert!(text.contains("2 engine methods, 1 key outputs, 3 styles"), "{text}");
        assert!(text.contains("ok    Library 1 (file): The fixed message.\n"), "{text}");
        assert!(
            text.contains("skip  Settings: Add --config to check settings.\n"),
            "{text}"
        );
        assert!(!text.contains("Libraries:"), "{text}");
        assert!(text.ends_with("All good."), "{text}");
        // A current source was read just now, so no raw time reaches a person.
        assert!(!text.contains("Last good read"), "{text}");
        assert!(!text.contains("epoch"), "{text}");
    }

    #[test]
    fn problems_are_counted_named_by_code_and_old_copies_say_their_age() {
        let stale = json!({"state":"stale", "updated_at_ms":0});
        let unknown = json!({"state":"unknown", "updated_at_ms":null});
        let text = report_text(
            &report(vec![
                source("findings", "refresh-failed", stale),
                source("failed", "library-missing", unknown),
            ]),
            3 * 3_600_000,
        );
        assert!(
            text.contains("warn  Library 1 (file): The fixed message. [refresh-failed]\n"),
            "{text}"
        );
        assert!(text.contains("      Last good read: 3 hours ago.\n"), "{text}");
        assert!(
            text.contains("fail  Library 1 (file): The fixed message. [library-missing]\n"),
            "{text}"
        );
        assert!(text.ends_with("2 things to look at."), "{text}");
    }

    #[test]
    fn an_age_reads_like_speech() {
        for (elapsed_ms, expected) in [
            (0, "less than a minute ago"),
            (59_999, "less than a minute ago"),
            (60_000, "1 minute ago"),
            (59 * 60_000, "59 minutes ago"),
            (3_600_000, "1 hour ago"),
            (47 * 3_600_000, "1 day ago"),
            (72 * 3_600_000, "3 days ago"),
        ] {
            assert_eq!(age(elapsed_ms), expected);
        }
    }
}
