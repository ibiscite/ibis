//! Sentences and summaries for people; `--json` output never passes through here.
pub(super) use ibis_core::model::is_hidden;
use ibis_core::{
    contract::*,
    destination::{CopyReason, Decision, Destination},
    model::strip_markup,
    summary::RecordSummary,
    syntax::MAX_CITATION_ITEMS,
};
use ibis_engine::catalogue::{self, Audience};
use ibis_engine::{
    DispatchResult, Doubt, EntryRefusal, EntryResult, KeyCheck, Method, ProjectKey, ProjectProblem, ProjectResult,
};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

/// What the open libraries hold, so an empty result can be told truthfully.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Holdings {
    NoLibrary,
    Empty,
    References,
}

/// Without --json, search, show, libraries and styles print short lines for people.
pub(super) fn with_summary(
    mut result: DispatchResult,
    method: Method,
    json: bool,
    holdings: Holdings,
    names: &BTreeMap<String, String>,
) -> DispatchResult {
    if !json && let Some(text) = summary(method, &result.data, holdings, names) {
        result.text = Some(text);
    }
    result
}

fn summary(method: Method, data: &Value, holdings: Holdings, names: &BTreeMap<String, String>) -> Option<String> {
    match method {
        Method::Show => reference(data),
        Method::Search => {
            let hits = data.get("hits")?.as_array()?;
            if hits.is_empty() {
                return match holdings {
                    Holdings::NoLibrary => Some("No library is open; add --file <library> or --zotero.".into()),
                    Holdings::Empty => catalogue::text("cli-no-references", None, &[]),
                    Holdings::References => Some("No references found.".into()),
                };
            }
            let rows = hits.iter().map(search_row).collect::<Option<Vec<_>>>()?.join("\n");
            // Closest matches are said to be so, never shown as full matches.
            if hits
                .iter()
                .any(|hit| hit.get("partial").and_then(Value::as_bool) == Some(true))
            {
                return Some(format!("No reference matches every word. Closest matches:\n{rows}"));
            }
            Some(rows)
        }
        Method::Libraries => {
            let libraries = data.get("libraries")?.as_array()?;
            if libraries.is_empty() {
                return Some("No libraries loaded. Open one with --file <library>.".into());
            }
            let mut lines = vec![format!(
                "{} {}",
                libraries.len(),
                if libraries.len() == 1 { "library" } else { "libraries" }
            )];
            for library in libraries {
                let count = library.get("objects")?.as_u64()?;
                let warnings = library.get("warnings_total").and_then(Value::as_u64).unwrap_or(0);
                lines.push(format!(
                    "{}\t{} {}{}{}{}",
                    field(&{
                        let id = library.get("id")?.as_str()?;
                        let zotero = library.get("kind").and_then(Value::as_str) == Some("zotero");
                        names.get(id).cloned().unwrap_or_else(|| {
                            match library.pointer("/zotero/group").and_then(Value::as_u64) {
                                Some(group) => format!("Zotero group {group}"),
                                None if zotero => "Zotero".into(),
                                None => id.into(),
                            }
                        })
                    }),
                    count,
                    if count == 1 { "item" } else { "items" },
                    match warnings {
                        0 => String::new(),
                        1 => ", 1 warning".into(),
                        n => format!(", {n} warnings"),
                    },
                    if library.get("refresh_error").is_some_and(|value| !value.is_null()) {
                        " (last good snapshot; refresh failed)"
                    } else {
                        ""
                    },
                    if library.get("watch_warning").is_some_and(|value| !value.is_null()) {
                        " (changes are not picked up automatically)"
                    } else {
                        ""
                    }
                ));
                let shown = library.get("warnings").and_then(Value::as_array);
                lines.extend(shown.into_iter().flatten().map(warning_line));
                let listed = shown.map_or(0, |shown| shown.len() as u64);
                if warnings > listed {
                    lines.push(format!("  and {} more", warnings - listed));
                }
            }
            Some(lines.join("\n"))
        }
        Method::Outputs => data
            .get("outputs")?
            .as_array()?
            .iter()
            .map(|output| {
                let example = output.get("example").and_then(Value::as_str).unwrap_or_default();
                Some(format!(
                    "{}\t{}\t{}",
                    field(output.get("id")?.as_str()?),
                    field(output.get("name")?.as_str()?),
                    field(example)
                ))
            })
            .collect::<Option<Vec<_>>>()
            .map(|lines| lines.join("\n")),
        Method::Project => project_report(data),
        Method::Entry => match serde_json::from_value(data.clone()).ok()? {
            EntryResult::Entry { text, .. } => Some(text.trim_end().to_owned()),
            EntryResult::Refused { reason } => Some(entry_refusal(reason).into()),
        },
        Method::Styles => {
            let styles = data.get("styles")?.as_array()?;
            let total = data.get("total")?.as_u64()?;
            let mut lines = styles
                .iter()
                .map(|style| {
                    Some(format!(
                        "{}\t{}\t{}",
                        field(style.get("id")?.as_str()?),
                        field(style.get("title")?.as_str()?),
                        field(style.get("updated")?.as_str()?)
                    ))
                })
                .collect::<Option<Vec<_>>>()?;
            if total == 0 {
                lines.push("No style matches those words.".into());
            } else if styles.len() as u64 != total {
                lines.push(format!(
                    "Showing {} of {total} styles; narrow it with: ibis styles WORDS",
                    styles.len()
                ));
            }
            Some(lines.join("\n"))
        }
        _ => None,
    }
}

/// One line per key, the file's own state first, in the words the app uses.
pub(super) fn project_report(data: &Value) -> Option<String> {
    let result: ProjectResult = serde_json::from_value(data.clone()).ok()?;
    let name = field(&result.name);
    let lines = |lines: &[usize]| lines.iter().map(usize::to_string).collect::<Vec<_>>().join(", ");
    let mut report = vec![];
    if let Some(problem) = result.problem {
        let why = match problem {
            ProjectProblem::Unavailable => "the file is not there or is not on this computer yet",
            ProjectProblem::NotAFile => "it is not a file",
            ProjectProblem::TooLarge => "the file is larger than 64 MiB",
            ProjectProblem::NotUtf8 => "the file is not UTF-8 text",
            ProjectProblem::Changing => "the file is still changing",
            ProjectProblem::NotBibtex => "a project bibliography must be a BibTeX or BibLaTeX file",
        };
        report.push(format!("Couldn't read {name}: {why}."));
    }
    if !result.damaged_lines.is_empty() {
        report.push(format!(
            "Couldn't read all of {name}: check line {}.",
            lines(&result.damaged_lines)
        ));
    }
    for ProjectKey { key, check } in &result.keys {
        let status = match check {
            KeyCheck::In { line } => format!("In {name} (line {line})"),
            KeyCheck::Missing => format!("Missing from {name}"),
            KeyCheck::CouldNotCheck => format!("Couldn't check {name}"),
            KeyCheck::Repeated { lines: found } => format!("Key appears more than once (lines {})", lines(found)),
            KeyCheck::MayNameAnother { line, doubt } => format!(
                "Key may name another reference (line {line}): {}",
                match doubt {
                    Doubt::Fields => "this entry and the library differ",
                    Doubt::LetterCase => "a key differs only by letter case",
                    Doubt::Alias => "it is another entry's alias",
                    Doubt::Helper => "it belongs to an @xdata entry, which cannot be cited",
                    Doubt::Commented => "it is inside @comment, which some tools still read",
                }
            ),
        };
        report.push(format!("{}\t{status}", field(key)));
    }
    Some(report.join("\n"))
}

fn entry_refusal(reason: EntryRefusal) -> &'static str {
    match reason {
        EntryRefusal::NoOriginal => {
            "This reference has no original BibTeX entry here. Export it from your reference manager in your project's dialect, keeping its key."
        }
        EntryRefusal::Changed => "The library file changed or could not be read. Reload it and try again.",
        EntryRefusal::Repeated => "The library file repeats this key, so Ibis cannot tell which entry to give.",
        EntryRefusal::Dependency => {
            "This entry relies on a crossref, xdata or @string that cannot come with it. Export it from your reference manager, keeping its key."
        }
    }
}

/// Citation languages for `ibis styles --locales`.
pub(super) fn locale_list(mut result: DispatchResult) -> DispatchResult {
    let lines: Option<Vec<String>> = result.data.get("locales").and_then(Value::as_array).map(|locales| {
        locales
            .iter()
            .filter_map(|locale| {
                Some(format!(
                    "{}\t{}",
                    field(locale.get("tag")?.as_str()?),
                    field(locale.get("english")?.as_str()?)
                ))
            })
            .collect()
    });
    result.text = lines.map(|lines| lines.join("\n"));
    result
}

fn search_row(hit: &Value) -> Option<String> {
    let record = hit.pointer("/item/object/data")?;
    let summary = record_summary(record.get("csl")?);
    // The engine decides a word matched only with a spelling change; the row says so.
    let spelling = (hit.get("typo").and_then(Value::as_bool) == Some(true)
        && hit.get("partial").and_then(Value::as_bool) != Some(true))
    .then(|| catalogue::text("cli-spelling-match", None, &[]))
    .flatten()
    .map(|label| format!(" {label}"))
    .unwrap_or_default();
    Some(format!(
        "{}\t{}\t{}\t{}{spelling}",
        field(citation_key(record)?),
        if summary.authors.is_empty() {
            "—".into()
        } else {
            field(&authors(&summary, Some(3)))
        },
        field(summary.year.as_deref().unwrap_or("n.d.")),
        field(summary.title.as_deref().unwrap_or("[untitled]"))
    ))
}

fn record_summary(csl: &Value) -> RecordSummary {
    RecordSummary::of(|name| csl.get(name))
}

fn citation_key(record: &Value) -> Option<&str> {
    let keys = record.get("citation_keys")?.as_array()?;
    Some(
        match keys.first().and_then(|key| key.get("value")).and_then(Value::as_str) {
            None => "[no citation key]",
            Some(key) if key.is_empty() || key.chars().any(|c| c.is_whitespace() || c.is_control()) => {
                "[invalid citation key]"
            }
            Some(key)
                if keys
                    .iter()
                    .any(|other| other.get("value").and_then(Value::as_str) != Some(key)) =>
            {
                "[conflicting citation keys]"
            }
            Some(key) => key,
        },
    )
}

/// The authors' reading names, the first `limit` then "et al.".
fn authors(summary: &RecordSummary, limit: Option<usize>) -> String {
    let limit = limit.unwrap_or(summary.authors.len());
    let mut names: Vec<_> = summary
        .authors
        .iter()
        .take(limit)
        .map(|author| author.name.as_str())
        .collect();
    if summary.authors.len() > limit && names.last() != Some(&"et al.") {
        names.push("et al.");
    }
    names.join(", ")
}

/// One reference for reading. Anything that is not a reference stays as JSON.
fn reference(data: &Value) -> Option<String> {
    let record = data.pointer("/object/data")?;
    let csl = record.get("csl")?;
    let text = |name: &str| csl.get(name).and_then(Value::as_str).map(strip_markup);
    let summary = record_summary(csl);
    let authors = Some(authors(&summary, None)).filter(|authors| !authors.is_empty());
    let number = |name: &str| {
        csl.get(name)
            .map(|value| value.as_str().map_or_else(|| value.to_string(), str::to_owned))
    };
    let rows = [
        ("Title", summary.title.clone()),
        ("Authors", authors),
        ("Year", summary.year.clone()),
        ("Published in", text("container-title")),
        ("Publisher", text("publisher")),
        ("Volume", number("volume")),
        ("Issue", number("issue")),
        ("Pages", number("page")),
        ("DOI", text("DOI")),
        ("URL", text("URL")),
    ];
    let mut lines = vec![field(citation_key(record)?)];
    lines.extend(
        rows.into_iter()
            .filter_map(|(label, value)| Some(format!("{label}: {}", field(&value?)))),
    );
    Some(lines.join("\n"))
}

/// A source warning as a sentence.
fn warning_line(warning: &Value) -> String {
    let key = warning.get("key").and_then(Value::as_str).map(field);
    let what = match (warning.get("code").and_then(Value::as_str), key) {
        (Some("duplicate-key"), Some(key)) => {
            // In CSL-JSON this is the record's `id`, which is never a citation key.
            format!("more than one entry uses '{key}' as its key or id, so Ibis skips them all")
        }
        (Some("syntax"), _) => "an entry could not be read, so Ibis skips it".into(),
        (Some("field"), _) => "a field could not be fully converted; check this entry".into(),
        (Some("work-limit"), _) => "the library is too large to read in full".into(),
        (Some("invalid-attachment"), _) => "an attachment link is not valid".into(),
        (Some("cloud-synced-directory"), _) => "the library is in a cloud-synced folder, so changes can be late".into(),
        (Some("citation-keys-unavailable"), _) => {
            "Better BibTeX keys could not be read, so some items have none".into()
        }
        _ => "something in the library needs checking".into(),
    };
    // Why, from the engine's words for the warning's cause, when it gives one.
    let why = serde_json::from_value::<ibis_engine::SourceWarning>(warning.clone())
        .ok()
        .and_then(|warning| catalogue::cause(&warning))
        .map(|cause| format!(" ({cause})"))
        .unwrap_or_default();
    match warning.get("line").and_then(Value::as_u64) {
        Some(line) => format!("  line {line}: {what}{why}"),
        None => format!("  {what}{why}"),
    }
}

/// A word for a command line, quoted when this system's shell would otherwise split it.
pub(super) fn quoted(text: &str) -> String {
    let plain = |c: char| c.is_alphanumeric() || "/._-:".contains(c) || (cfg!(windows) && c == '\\');
    if !text.is_empty() && text.chars().all(plain) {
        text.to_owned()
    } else if cfg!(windows) {
        format!("\"{text}\"")
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}

pub(super) fn field(text: &str) -> String {
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    super::terminal_display(&line)
}

/// What a `cite` run asked for, so findings can name the key and the output.
pub(super) struct Citing {
    pub labels: Vec<String>,
    /// The per-key options each item was given, such as `--omit-author`.
    pub flags: Vec<Vec<&'static str>>,
    pub output: String,
    pub form: CitationForm,
}
impl Citing {
    fn label(&self, item: usize) -> String {
        self.labels
            .get(item)
            .map_or_else(|| format!("item {}", item + 1), |key| format!("'{}'", field(key)))
    }
}

/// Sentences for a citation's refusals, losses and notices, without repeats.
pub(super) fn citation_report(data: &Value, citing: &Citing, printed: bool) -> Vec<String> {
    let parse = |name| data.get(name).cloned().unwrap_or(Value::Null);
    let outcomes: Vec<ItemOutcome> = serde_json::from_value(parse("outcomes")).unwrap_or_default();
    let losses: Vec<Loss> = serde_json::from_value(parse("losses")).unwrap_or_default();
    let notices: Vec<Notice> = serde_json::from_value(parse("notices")).unwrap_or_default();
    let warnings: Vec<CitationWarning> = serde_json::from_value(parse("warnings")).unwrap_or_default();
    let output = field(&citing.output);
    let mut lines = Vec::new();
    for warning in warnings {
        let label = citing.label(warning.item);
        let kind = match warning.code {
            CitationWarningCode::SourceField => "warning",
            CitationWarningCode::SentenceCaseExpected => "note",
        };
        let said = catalogue::sentence("warning", warning.code, Audience::Cli, &[("reference", &label)]);
        lines.push(format!("{kind}: {said}"));
    }
    let refused: Vec<_> = outcomes
        .into_iter()
        .filter_map(|outcome| match outcome {
            ItemOutcome::Refused { item, error } => Some((item, error)),
            ItemOutcome::Rendered { .. } => None,
        })
        .collect();
    // The formatter refuses a whole citation it cannot print in full, so no one key is to blame.
    if refused.len() > 1
        && citing.form == CitationForm::Normal
        && refused
            .iter()
            .all(|(_, error)| *error == CitationError::UnsupportedForm)
    {
        lines.push(cli("cli-together-refused", &[("output", &output)]));
    } else {
        for (item, error) in refused {
            let flags = citing
                .flags
                .get(item)
                .map_or_else(String::new, |flags| flags.join(" and "));
            lines.push(refusal(error, &citing.label(item), &output, citing.form, &flags));
        }
    }
    for loss in losses {
        let what = cli(&catalogue::id("loss", loss.field), &[]);
        let target = loss
            .item
            .map(|item| format!(" for {}", citing.label(item)))
            .unwrap_or_default();
        let id = match loss.reason {
            LossReason::Unsupported => "cli-loss-left-out",
            LossReason::Flattened => "cli-loss-flattened",
        };
        lines.push(cli(id, &[("output", &output), ("what", &what), ("target", &target)]));
    }
    // Notices describe printed text; a refused citation printed none.
    for notice in notices.into_iter().filter(|_| printed) {
        lines.push(format!(
            "note: {}",
            catalogue::sentence("notice", notice, Audience::Cli, &[])
        ));
    }
    // A person reading English output needs no note; any other language is a choice to see.
    if let Some(language) = data
        .get("language")
        .and_then(Value::as_str)
        .filter(|language| printed && *language != "en-US")
    {
        lines.push(format!("note: written in {}.", field(language)));
    }
    if printed && let Ok(destination) = serde_json::from_value(parse("destination")) {
        lines.push(destination_note(&destination));
    }
    let mut seen = std::collections::BTreeSet::new();
    lines.retain(|line| seen.insert(line.clone()));
    lines
}

/// What the app would do with this citation; pasting is on and permitted here.
fn destination_note(destination: &Destination) -> String {
    let name = destination.name.as_deref().unwrap_or("this app");
    let delivery = match destination.delivery {
        Decision::Paste => format!("Ibis pastes into {name}"),
        Decision::Copy(CopyReason::UnknownApp) => "Ibis does not know this app, so it asks before pasting".into(),
        Decision::Copy(CopyReason::UnsupportedSystem) => {
            "Ibis copies here, because it cannot paste on this system yet".into()
        }
        Decision::Copy(CopyReason::Terminal) => {
            format!("Ibis copies for {name}, because a paste runs there as typed input")
        }
        Decision::Copy(CopyReason::RemoteDesktop | CopyReason::VirtualMachine) => {
            format!("Ibis copies for {name}, because keystrokes reach another system there")
        }
        Decision::Copy(CopyReason::NonstandardShortcut) => {
            format!("Ibis copies for {name}, because its paste key does something else")
        }
        Decision::Copy(_) => format!("Ibis copies for {name}"),
    };
    match destination.output_question {
        Some(_) => format!("note: {delivery}; the app asks once which output {name} needs."),
        None => format!("note: {delivery}."),
    }
}

fn refusal(error: CitationError, label: &str, output: &str, form: CitationForm, flags: &str) -> String {
    let form = catalogue::id("form", form);
    let form = form.strip_prefix("form-").unwrap_or_default();
    let limit = MAX_CITATION_ITEMS.to_string();
    let args = [
        ("reference", label),
        ("output", output),
        ("form", form),
        ("options", flags),
        ("limit", &limit),
    ];
    match error {
        CitationError::UnsupportedForm if form != "normal" => cli("cli-form-refused", &args),
        CitationError::UnsupportedForm if !flags.is_empty() => cli("cli-options-refused", &args),
        error => catalogue::sentence("refusal", error, Audience::Cli, &args),
    }
}

/// One of the command line's own sentences from the catalogue.
fn cli(id: &str, args: &[(&str, &str)]) -> String {
    catalogue::text(id, None, args).unwrap_or_else(|| id.to_owned())
}

/// Fixed words for output that could not be written; the OS message is not repeated.
pub(super) fn write_failure(error: &std::io::Error) -> &'static str {
    if error.kind() == std::io::ErrorKind::StorageFull {
        "the output could not be written because the disk is full; free some space and try again."
    } else {
        "the output could not be written; check where it is sent and try again."
    }
}

/// The fallback sentence for each contract error code.
pub(super) fn code_sentence(code: ErrorCode) -> String {
    catalogue::code(code, Audience::Cli)
}

pub(super) fn file_failure(path: &Path, error: EngineError) -> String {
    format!(
        "cannot open {}: {}",
        field(&path.display().to_string()),
        file_reason(path, error)
    )
}

/// Why a library file did not open, without naming it.
pub(super) fn file_reason(path: &Path, error: EngineError) -> String {
    if let Some(reason) = error.reason {
        return reason_sentence(reason);
    }
    let code = error.code;
    // The bounded read has already failed; looking again is bounded too, so a
    // stalled disk or a path just replaced by a pipe cannot hold the command.
    // Without an answer in time, the read's own reason stands.
    let probed = path.to_owned();
    let found = crate::bounded(move || path_reason(&probed)).flatten();
    found.map_or_else(
        || match code {
            ErrorCode::BadInput => {
                catalogue::text("cli-file-unreadable", None, &[]).unwrap_or_default()
            }
            ErrorCode::SourceUnavailable => "it is being written or cannot be read right now; try again in a moment.".into(),
            ErrorCode::ResourceLimit => {
                "it is larger than Ibis reads, or too many files are open at once; split the library or open fewer files.".into()
            }
            code => code_sentence(code),
        },
        str::to_owned,
    )
}

/// What the path itself shows: missing, a folder, or a file this program may not read.
fn path_reason(path: &Path) -> Option<&'static str> {
    let link_to_nothing = std::fs::symlink_metadata(path).is_ok()
        && std::fs::metadata(path).is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound);
    if std::fs::symlink_metadata(path).is_err() || link_to_nothing {
        Some("there is no file at that path; check the spelling or give the full path.")
    } else if path.is_dir() {
        Some("it is a folder; give the library file inside it.")
    } else if std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
        // Only a regular file is opened to check: opening a pipe or a device can block.
        && std::fs::File::open(path).is_err_and(|error| error.kind() == std::io::ErrorKind::PermissionDenied)
    {
        Some("this program is not allowed to read it; check its permissions, or run it outside the sandbox.")
    } else {
        None
    }
}

pub(super) fn zotero_failure(error: EngineError, profiles: &[String]) -> String {
    let reason = match (error.code, error.reason) {
        (_, Some(reason)) => reason_sentence(reason),
        (ErrorCode::SourceUnavailable, None) => {
            "no local Zotero library was found or it could not be read; check that Zotero is installed and has been opened once, or use --file.".into()
        }
        (ErrorCode::BadInput, None) => {
            "its data could not be understood; open Zotero once so it can repair its database.".into()
        }
        (ErrorCode::NewerFormat, None) => "a newer version of Zotero wrote it; update Ibis.".into(),
        (ErrorCode::ResourceLimit, None) => "it is larger than Ibis reads at once.".into(),
        (code, None) => code_sentence(code),
    };
    if error.reason == Some(ErrorReason::ZoteroProfileNeeded) && !profiles.is_empty() {
        let names: Vec<_> = profiles.iter().map(|name| field(name)).collect();
        return format!(
            "cannot read the Zotero library: {reason} Profiles: {}.",
            names.join(", ")
        );
    }
    format!("cannot read the Zotero library: {reason}")
}

/// Why a citation key did not select exactly one reference.
pub(super) fn key_failure(
    key: &str,
    error: EngineError,
    libraries_open: bool,
    warnings: &[ibis_engine::SourceWarning],
) -> String {
    let shown = field(&key.chars().take(80).collect::<String>());
    let lines: Vec<usize> = warnings.iter().filter_map(|warning| warning.line).collect();
    match (error.code, error.reason) {
        (_, Some(ErrorReason::DuplicateKey)) => {
            let at = match lines.as_slice() {
                [] => String::new(),
                [line] => format!(" (line {line})"),
                [rest @ .., last] => format!(
                    " (lines {} and {last})",
                    rest.iter().map(usize::to_string).collect::<Vec<_>>().join(", ")
                ),
            };
            format!(
                "the citation key '{shown}' is used by more than one entry{at}, so Ibis skips them all; give each entry its own key."
            )
        }
        (_, Some(ErrorReason::DamagedEntry)) => {
            let at = lines.first().map(|line| format!(" on line {line}")).unwrap_or_default();
            let why = warnings
                .first()
                .and_then(catalogue::cause)
                .map(|cause| format!(" ({cause})"))
                .unwrap_or_default();
            format!("the entry '{shown}'{at} could not be read, so Ibis skips it{why}; fix it in your library.")
        }
        (_, Some(reason @ ErrorReason::LibraryCutShort)) => {
            format!("cannot find '{shown}': {}", reason_sentence(reason))
        }
        (_, Some(ErrorReason::AmbiguousKey)) => {
            format!("several references use the citation key '{shown}'; give each one its own key in your library.")
        }
        (ErrorCode::NotFound, _) if !libraries_open => {
            format!("cannot look up '{shown}': no library is open; add --file <library> or --zotero.")
        }
        (ErrorCode::NotFound, _) => {
            format!("no reference has the citation key '{shown}'; find the right key with: ibis search <words>")
        }
        (ErrorCode::BadInput, _) if key.is_empty() => "a citation key cannot be empty.".into(),
        (ErrorCode::BadInput, _) if key.len() > 64 * 1024 => format!("the citation key '{shown}…' is too long."),
        (code, reason) => reason.map_or_else(|| code_sentence(code), reason_sentence),
    }
}

/// The sentence for each reason an engine error gives.
pub(super) fn reason_sentence(reason: ErrorReason) -> String {
    catalogue::sentence("reason", reason, Audience::Cli, &[])
}
