//! Sentences and summaries for people; `--json` output never passes through here.
pub(super) use ibis_core::model::is_hidden;
use ibis_core::{
    contract::*,
    destination::{CopyReason, Decision, Destination},
    model::{Name, strip_markup},
    syntax::MAX_CITATION_ITEMS,
};
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
                return Some(
                    match holdings {
                        Holdings::NoLibrary => "No library is open; add --file <library> or --zotero.",
                        Holdings::Empty => {
                            "No open library holds any references. A file must be BibTeX, BibLaTeX or CSL-JSON."
                        }
                        Holdings::References => "No references found.",
                    }
                    .into(),
                );
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
fn project_report(data: &Value) -> Option<String> {
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
            ProjectProblem::Damaged => "part of the file cannot be read",
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
    let csl = record.get("csl")?;
    let authors = authors(csl, Some(3));
    Some(format!(
        "{}\t{}\t{}\t{}",
        field(citation_key(record)?),
        if authors.is_empty() {
            "—".into()
        } else {
            field(&authors)
        },
        field(&year(csl).unwrap_or_else(|| "n.d.".into())),
        field(&strip_markup(
            csl.get("title").and_then(Value::as_str).unwrap_or("[untitled]")
        ))
    ))
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

/// The authors' names, at most `limit` of them and then "et al.".
fn authors(csl: &Value, limit: Option<usize>) -> String {
    csl.get("author")
        .and_then(Value::as_array)
        .map_or_else(String::new, |names| {
            let limit = limit.unwrap_or(names.len());
            let mut authors = names
                .iter()
                .take(limit)
                .map(|name| Name::from_csl(name).display())
                .filter(|name| !name.is_empty())
                .collect::<Vec<_>>();
            if names.len() > limit {
                authors.push("et al.".into());
            }
            authors.join(", ")
        })
}

fn year(csl: &Value) -> Option<String> {
    let issued = csl.get("issued");
    issued
        .and_then(|date| date.pointer("/date-parts/0/0"))
        .map(|year| year.as_str().map_or_else(|| year.to_string(), str::to_owned))
        .or_else(|| {
            issued
                .and_then(|date| date.get("literal"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
}

/// One reference for reading. Anything that is not a reference stays as JSON.
fn reference(data: &Value) -> Option<String> {
    let record = data.pointer("/object/data")?;
    let csl = record.get("csl")?;
    let text = |name: &str| csl.get(name).and_then(Value::as_str).map(strip_markup);
    let authors = Some(authors(csl, None)).filter(|authors| !authors.is_empty());
    let number = |name: &str| {
        csl.get(name)
            .map(|value| value.as_str().map_or_else(|| value.to_string(), str::to_owned))
    };
    let rows = [
        ("Title", text("title")),
        ("Authors", authors),
        ("Year", year(csl)),
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
    match warning.get("line").and_then(Value::as_u64) {
        Some(line) => format!("  line {line}: {what}"),
        None => format!("  {what}"),
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
        lines.push(match warning.code {
            CitationWarningCode::SourceField => format!("warning: a source field for {label} could not be fully converted; check the library entry."),
            CitationWarningCode::SentenceCaseExpected => format!("note: APA expects sentence case for the title of {label}; stored capitalization was preserved, so check it in your document."),
        });
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
        lines.push(format!(
            "{output} cannot print these keys together as asked; cite them one at a time to see which one fails."
        ));
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
        let what = match loss.field {
            LossField::Locator => "the locator",
            LossField::Prefix => "the prefix",
            LossField::Suffix => "the suffix",
            LossField::AuthorSuppression => "an omitted author",
            LossField::RichText => "text formatting",
            LossField::Order => "the order of the keys",
        };
        let target = loss
            .item
            .map(|item| format!(" for {}", citing.label(item)))
            .unwrap_or_default();
        lines.push(match loss.reason {
            LossReason::Unsupported => format!("{output} cannot show {what}{target}, so it was left out."),
            LossReason::Flattened => format!("{output} printed {what}{target} as plain text."),
        });
    }
    // Notices describe printed text; a refused citation printed none.
    for notice in notices.into_iter().filter(|_| printed) {
        lines.push(
            match notice {
                Notice::NeedsDocumentContext => "note: this style numbers citations or places notes across the whole document, which Ibis does not see; check the result in your document.",
                Notice::ReferenceEntry => "note: printed the full reference entry.",
                Notice::FullFirstNote => "note: printed the full form a first note uses; later notes to the same work are usually shorter.",
                Notice::YearSuffixWithheld => "note: two of these works share author and year; add the year letters (such as 2026a and 2026b) in your document, because they depend on its whole reference list.",
                Notice::InputLanguageUnsupported => "note: locator words in this language are not recognised; give the locator in English, such as \"p. 12\".",
            }
            .into(),
        );
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
    match error {
        CitationError::UnsupportedForm => {
            let form = serde_json::to_value(form)
                .ok()
                .and_then(|form| form.as_str().map(str::to_owned))
                .unwrap_or_default();
            if form != "normal" {
                format!("cannot cite {label} in {output} with --form {form}; try --form normal.")
            } else if !flags.is_empty() {
                let it = if flags.contains(" and ") { "them" } else { "it" };
                format!("cannot cite {label} with {flags} in {output}; try without {it}.")
            } else {
                format!("{output} cannot print {label} correctly here; try another style, or cite it separately.")
            }
        }
        CitationError::InvalidLocator => format!("the locator for {label} is empty or has control characters."),
        CitationError::MissingItem | CitationError::ChangedItem => {
            format!("{label} changed or was removed while citing; try again.")
        }
        CitationError::MissingKey => format!("{label} has no citation key; cite it with --style instead."),
        CitationError::AmbiguousKey => {
            format!("several references share the citation key of {label}; give each one its own key in your library.")
        }
        CitationError::ConflictingKey => {
            format!("{label} has conflicting citation keys in its library; keep one.")
        }
        CitationError::InvalidKey => format!("the citation key of {label} has characters {output} cannot write."),
        CitationError::UnsupportedOutput => format!("there is no style {output}; list the styles with: ibis styles"),
        CitationError::InvalidLocale => "there is no such locale; list them with: ibis styles --locales".into(),
        CitationError::NotCitable => format!("{label} is not a reference (for example, a note) and cannot be cited."),
        CitationError::FormatterUnavailable => "the citation formatter could not start; try again.".into(),
        CitationError::AnnotationMismatch => format!("the annotation given for {label} belongs to another reference."),
        CitationError::InvalidRichText => format!("the prefix or suffix for {label} has text Ibis cannot write."),
        CitationError::TooManyItems => format!("one citation can hold at most {} references.", MAX_CITATION_ITEMS),
        CitationError::OutputTooLarge => "the citation is too large to print.".into(),
        CitationError::EmptySelection => "there is nothing to cite.".into(),
        CitationError::Cancelled => "the citation was cancelled.".into(),
    }
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
pub(super) fn code_sentence(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::Usage => "the command is not valid; see: ibis --help",
        ErrorCode::BadInput => "the input is not valid; see: ibis --help",
        ErrorCode::NotFound => "nothing matched.",
        ErrorCode::SourceUnavailable => "a library could not be read right now; try again.",
        ErrorCode::ResourceLimit => "the request or its result is too large; ask for less.",
        ErrorCode::NewerFormat => "a newer version of Ibis wrote this data; update Ibis.",
        ErrorCode::Internal => {
            "something went wrong inside Ibis; please report it at https://github.com/ibiscite/ibis/issues"
        }
        ErrorCode::Cancelled => "cancelled.",
    }
}

pub(super) fn file_failure(path: &Path, code: ErrorCode) -> String {
    format!(
        "cannot open {}: {}",
        field(&path.display().to_string()),
        file_reason(path, code)
    )
}

/// Why a library file did not open, without naming it.
pub(super) fn file_reason(path: &Path, code: ErrorCode) -> &'static str {
    // The bounded read has already failed; looking again is bounded too, so a
    // stalled disk or a path just replaced by a pipe cannot hold the command.
    // Without an answer in time, the read's own reason stands.
    let probed = path.to_owned();
    let found = crate::bounded(move || path_reason(&probed)).flatten();
    found.unwrap_or(match code {
        ErrorCode::BadInput => {
            "it cannot be read, or it is not a BibTeX, BibLaTeX or CSL-JSON library; check its permissions and contents."
        }
        ErrorCode::SourceUnavailable => "it is being written or cannot be read right now; try again in a moment.",
        ErrorCode::ResourceLimit => {
            "it is larger than Ibis reads, or too many files are open at once; split the library or open fewer files."
        }
        code => code_sentence(code),
    })
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
            "no local Zotero library was found or it could not be read; check that Zotero is installed and has been opened once, or use --file."
        }
        (ErrorCode::BadInput, None) => {
            "its data could not be understood; open Zotero once so it can repair its database."
        }
        (ErrorCode::NewerFormat, None) => "a newer version of Zotero wrote it; update Ibis.",
        (ErrorCode::ResourceLimit, None) => "it is larger than Ibis reads at once.",
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
pub(super) fn key_failure(key: &str, error: EngineError, libraries_open: bool, lines: &[usize]) -> String {
    let shown = field(&key.chars().take(80).collect::<String>());
    match (error.code, error.reason) {
        (_, Some(ErrorReason::DuplicateKey)) => {
            let at = match lines {
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
        (code, reason) => reason.map_or_else(|| code_sentence(code), reason_sentence).into(),
    }
}

/// The sentence for each reason an engine error gives.
pub(super) fn reason_sentence(reason: ErrorReason) -> &'static str {
    match reason {
        ErrorReason::UnknownStyle => "there is no style with that name; list the styles with: ibis styles",
        ErrorReason::UnknownOutput => "there is no output with that id; list them with: ibis outputs",
        ErrorReason::UnknownLocale => "there is no such locale; list them with: ibis styles --locales",
        ErrorReason::DuplicateKey => {
            "the citation key is used by more than one entry, so Ibis skips them all; give each entry its own key."
        }
        ErrorReason::AmbiguousKey => {
            "several references use this citation key; give each one its own key in your library."
        }
        ErrorReason::MissingKey => "this reference has no citation key; give it one in your library, or use --style.",
        ErrorReason::AppMissing => "the Ibis app was not found; install it, or give its location with --app <path>.",
        ErrorReason::NotIbis => {
            "the program given as the Ibis app is not Ibis; give the Ibis app's location with --app <path>."
        }
        ErrorReason::NoAnswer => "the Ibis app started but did not answer; open Ibis once by hand, then try again.",
        ErrorReason::ZoteroSharingOff => {
            "Zotero is not sharing its library; in Zotero open Settings, Advanced, and turn on Allow other applications on this computer to communicate with Zotero."
        }
        ErrorReason::ZoteroProfileNeeded => {
            "Zotero is closed and Ibis needs to know which profile to read; open Zotero, or name one with --zotero-profile NAME."
        }
        ErrorReason::ZoteroNeedsOpening => {
            "Zotero's library cannot be read while it is closed; open Zotero, then try again."
        }
        ErrorReason::ZoteroNotFound => {
            "no local Zotero library was found or it could not be read; check that Zotero is installed and has been opened once, or use --file."
        }
        ErrorReason::ZoteroNoSpace => {
            "there is not enough free space to read Zotero while it is closed; free some space, or open Zotero."
        }
    }
}
