//! One-shot, read-only diagnostics. No source watches, discovery, API requests,
//! database copies, settings recovery writes, or raw errors belong here.
use super::{OutputMode, source_values};
use clap::ArgMatches;
use ibis_core::{contract::*, settings};
use ibis_engine::{Cancellation, DispatchResult, Engine, LibraryInfo, Method, request::LibrariesRequest};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub(super) fn run(engine: &Engine, matches: &ArgMatches, mode: OutputMode) -> Result<DispatchResult, EngineError> {
    let args = matches
        .subcommand_matches("doctor")
        .ok_or(EngineError { code: ErrorCode::Usage })?;
    let config = config_check(args.get_one::<PathBuf>("config").map(PathBuf::as_path));
    // Capture existing state once. A later check cannot relabel this observation.
    let mut sources: Vec<_> = engine.libraries()?.libraries.iter().map(library_report).collect();
    // Serial checks own their results. Nothing polls or completes after reporting.
    sources.extend(
        source_values::<PathBuf>(matches, "file")
            .iter()
            .map(|path| inspect_file(path)),
    );
    let groups = source_values::<u64>(matches, "zotero-group");
    for _ in 0..usize::from(matches.get_flag("zotero")) + groups.len() {
        sources.push(unavailable(
            "zotero", "not-checked", "zotero-not-checked",
            "Zotero was not checked: its loader contacts the local API and may copy the database. Doctor sends nothing and writes nothing. Export a file and use --file to check it offline.",
        ));
    }
    for (index, source) in sources.iter_mut().enumerate() {
        // Stable within this report; no private location is needed to identify a check.
        source["index"] = json!(index + 1);
    }
    let findings =
        sources.is_empty() || config["status"] == "failed" || sources.iter().any(|source| source["status"] != "ok");
    let library_check = if sources.is_empty() {
        check(
            "failed",
            "no-library",
            "No library selected. Give --file <library.bib> (repeat for several libraries).",
        )
    } else {
        check(
            "ok",
            "sources-reported",
            "Each selected source has its own result below.",
        )
    };
    let key_outputs: Vec<_> = ibis_core::syntax::CAPABILITIES
        .iter()
        .map(|capability| {
            json!({
                "syntax":capability.syntax, "command":capability.command,
                "forms":capability.forms, "single_item_forms":capability.single_item_forms,
            })
        })
        .collect();
    let styles = engine
        .dispatch(Method::Styles, &json!({}), &Cancellation::default())?
        .data;
    let data = json!({
        "contract_version": Version,
        "cli_version": env!("CARGO_PKG_VERSION"),
        "stdout_tty": mode == OutputMode::Terminal,
        "capabilities": {"methods":Method::ALL, "key_outputs":key_outputs, "csl_styles":styles["styles"]},
        "config":config,
        "library_check":library_check,
        "sources":sources,
    });
    let mut text = format!(
        "Ibis {} · contract {}\n",
        env!("CARGO_PKG_VERSION"),
        data["contract_version"]
    );
    text.push_str(&format!(
        "Capabilities: {}\n",
        Method::ALL.map(Method::as_str).join(", ")
    ));
    for (label, result) in [("Config", &data["config"]), ("Libraries", &data["library_check"])] {
        text.push_str(&summary(label, result));
    }
    for source in sources {
        let label = format!(
            "Source {} ({})",
            source["index"],
            source["kind"].as_str().unwrap_or("loaded")
        );
        text.push_str(&summary(&label, &source));
        if let Some(time) = source["freshness"]["updated_at_ms"].as_u64() {
            text.push_str(&format!(
                "  Freshness: {}; last successful read: {time} ms since Unix epoch.\n",
                source["freshness"]["state"].as_str().unwrap_or("unknown")
            ));
        }
    }
    Ok(DispatchResult {
        data,
        text: Some(text.trim_end_matches('\n').into()),
        exit_code: if findings { ExitCode::Findings } else { ExitCode::Ok },
        // The report already includes warnings; do not emit a second generic diagnostic.
        source_findings: false,
    })
}

fn summary(label: &str, result: &Value) -> String {
    format!(
        "{label}: {} [{}] {}\n",
        result["status"].as_str().unwrap_or("failed"),
        result["code"].as_str().unwrap_or("internal"),
        result["message"].as_str().unwrap_or("Check failed.")
    )
}

fn check(status: &str, code: &str, message: &str) -> Value {
    json!({"status":status, "code":code, "message":message})
}

fn config_check(path: Option<&Path>) -> Value {
    let Some(path) = path else {
        return check(
            "not-checked",
            "config-not-requested",
            "Use --config <settings.json> to check a chosen settings file. CLI library choices come from flags.",
        );
    };
    // Within the filesystem deadline: a stalled disk reports unreadable, never hangs.
    let owned = path.to_owned();
    let Some(loaded) = crate::bounded(move || settings::load(&owned)) else {
        let mut result = check(
            "failed",
            "config-unreadable",
            "The chosen config could not be read in time. It was left unchanged.",
        );
        result["from_backup"] = json!(false);
        return result;
    };
    let mut result = match loaded.problem.as_ref() {
        None if loaded.primary_read == settings::PrimaryRead::Missing => {
            check("failed", "config-missing", "The chosen config does not exist.")
        }
        None => check("ok", "config-ok", "The chosen config is valid."),
        Some(problem) => {
            let (code, message) = config_problem(problem);
            check("failed", code, message)
        }
    };
    result["from_backup"] = json!(loaded.from_backup);
    result
}

fn config_problem(problem: &settings::SettingsError) -> (&'static str, &'static str) {
    match problem {
        settings::SettingsError::Invalid(_) | settings::SettingsError::InvalidZoteroLibraryEntry { .. } => {
            ("config-invalid", "The chosen config is invalid. It was left unchanged.")
        }
        settings::SettingsError::NewerVersion(_) => (
            "config-newer-format",
            "The chosen config needs a newer Ibis. It was left unchanged.",
        ),
        settings::SettingsError::Backup(problem) => config_problem(problem),
        settings::SettingsError::Unreadable(error) if error.kind() == std::io::ErrorKind::NotFound => (
            "config-missing",
            "The chosen config does not exist. It was left unchanged.",
        ),
        settings::SettingsError::Unreadable(_) => (
            "config-unreadable",
            "The chosen config could not be read. Check its permissions and file type.",
        ),
    }
}

fn inspect_file(path: &Path) -> Value {
    let Ok(params) = serde_json::to_value(LibrariesRequest::OpenFile { path: path.to_owned() }) else {
        return unavailable(
            "file",
            "failed",
            "library-invalid",
            "The selected path is not UTF-8. Choose a file with a UTF-8 path.",
        );
    };
    match std::fs::metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return unavailable(
                "file",
                "failed",
                "library-missing",
                "The selected file does not exist. Check the --file argument.",
            );
        }
        Err(_) => {
            return unavailable(
                "file",
                "failed",
                "library-unavailable",
                "The selected file could not be read. Check its permissions.",
            );
        }
        Ok(metadata) if !metadata.is_file() => {
            return unavailable(
                "file",
                "failed",
                "library-not-regular",
                "The selected library must be a regular file.",
            );
        }
        Ok(_) => {}
    }
    // Each file has a disposable engine: warnings and failures cannot bleed
    // into another source, and memory is bounded by the largest selected file.
    let loaded = Engine::new().and_then(|engine| {
        let engine = engine.without_file_watches();
        engine.dispatch(Method::Libraries, &params, &Cancellation::default())?;
        engine.libraries()
    });
    match loaded {
        Ok(result) => result
            .libraries
            .first()
            .map(library_report)
            .unwrap_or_else(|| unavailable("file", "failed", "internal", "The source returned no library report.")),
        Err(error) => {
            let (code, message) = match error.code {
                ErrorCode::BadInput => (
                    "library-invalid",
                    "The selected file is not a supported, readable library.",
                ),
                ErrorCode::ResourceLimit => ("resource-limit", "The selected library exceeds an Ibis resource limit."),
                ErrorCode::NewerFormat => ("newer-format", "The selected library needs a newer Ibis."),
                ErrorCode::Internal => ("internal", "The selected library check failed internally."),
                _ => (
                    "library-unavailable",
                    "The selected file could not be read. Check its permissions or try again when it stops changing.",
                ),
            };
            unavailable("file", "failed", code, message)
        }
    }
}

fn unavailable(kind: &str, status: &str, code: &str, message: &str) -> Value {
    let mut result = check(status, code, message);
    result["kind"] = json!(kind);
    result["freshness"] = json!({"state":"unknown", "updated_at_ms":null});
    result
}

fn library_report(library: &LibraryInfo) -> Value {
    let (status, code, message) = if library.refresh_error.is_some() {
        (
            "findings",
            "refresh-failed",
            "The last refresh failed; this is the last successful snapshot.",
        )
    } else if library.warnings_total > 0 {
        (
            "findings",
            "source-warnings",
            "The library loaded with source warnings. Use libraries --json to inspect them.",
        )
    } else {
        ("ok", "source-ok", "The library was read successfully.")
    };
    let mut result = check(status, code, message);
    result["kind"] = json!(if library.id.as_str().starts_with("zotero:") {
        "zotero"
    } else {
        "file"
    });
    result["id"] = json!(library.id);
    result["warning_count"] = json!(library.warnings_total);
    result["refresh_error"] = json!(library.refresh_error);
    result["freshness"] = json!({
        "state":if library.refresh_error.is_some() { "stale" } else { "current" },
        "updated_at_ms":library.updated_at_ms,
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_refresh_reports_the_retained_snapshot_as_stale() {
        let library = LibraryInfo {
            id: LibraryId::file(b"invented-source"),
            objects: 3,
            warnings: vec![],
            warnings_total: 0,
            refresh_error: Some(ErrorCode::SourceUnavailable),
            updated_at_ms: 123,
            watch_warning: None,
            zotero: None,
        };
        let source = library_report(&library);
        assert_eq!(source["status"], "findings");
        assert_eq!(source["code"], "refresh-failed");
        assert_eq!(source["freshness"], json!({"state":"stale", "updated_at_ms":123}));
        assert!(summary("Source 1", &source).contains("last successful snapshot"));
    }
}
