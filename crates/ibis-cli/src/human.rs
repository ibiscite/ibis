//! Sentences and summaries for people; `--json` output never passes through here.
use ibis_core::{
    contract::*,
    model::{Name, strip_markup},
    syntax::MAX_CITATION_ITEMS,
};
use ibis_engine::{DispatchResult, Method};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

/// Without --json, search, libraries and styles print short lines for people.
pub(super) fn with_summary(
    mut result: DispatchResult,
    method: Method,
    json: bool,
    libraries_open: bool,
    names: &BTreeMap<String, String>,
) -> DispatchResult {
    if !json && let Some(text) = summary(method, &result.data, libraries_open, names) {
        result.text = Some(text);
    }
    result
}

fn summary(method: Method, data: &Value, libraries_open: bool, names: &BTreeMap<String, String>) -> Option<String> {
    match method {
        Method::Search => {
            let hits = data.get("hits")?.as_array()?;
            if hits.is_empty() && !libraries_open {
                return Some("No library is open; add --file <library> or --zotero.".into());
            }
            if hits.is_empty() {
                return Some("No references found.".into());
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
                        names.get(id).cloned().unwrap_or_else(|| {
                            match library.pointer("/zotero/group").and_then(Value::as_u64) {
                                Some(group) => format!("Zotero group {group}"),
                                None if id.starts_with("zotero:") => "Zotero".into(),
                                None => id.into(),
                            }
                        })
                    }),
                    count,
                    if count == 1 { "item" } else { "items" },
                    match warnings {
                        0 => String::new(),
                        1 => ", 1 warning (details: ibis libraries --json)".into(),
                        n => format!(", {n} warnings (details: ibis libraries --json)"),
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
            }
            Some(lines.join("\n"))
        }
        Method::Styles => Some(
            data.get("styles")?
                .as_array()?
                .iter()
                .map(|style| {
                    Some(format!(
                        "{}\t{}",
                        field(style.get("id")?.as_str()?),
                        field(style.get("title")?.as_str()?)
                    ))
                })
                .collect::<Option<Vec<_>>>()?
                .join("\n"),
        ),
        _ => None,
    }
}

fn search_row(hit: &Value) -> Option<String> {
    let record = hit.pointer("/item/object/data")?;
    let csl = record.get("csl")?;
    let keys = record.get("citation_keys")?.as_array()?;
    let key = match keys.first().and_then(|key| key.get("value")).and_then(Value::as_str) {
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
    };
    let authors = csl
        .get("author")
        .and_then(Value::as_array)
        .map_or_else(String::new, |names| {
            let mut authors = names
                .iter()
                .take(3)
                .map(author)
                .filter(|name| !name.is_empty())
                .collect::<Vec<_>>();
            if names.len() > 3 {
                authors.push("et al.".into());
            }
            authors.join(", ")
        });
    let issued = csl.get("issued");
    let year = issued
        .and_then(|date| date.pointer("/date-parts/0/0"))
        .map(|year| year.as_str().map_or_else(|| year.to_string(), str::to_owned))
        .or_else(|| {
            issued
                .and_then(|date| date.get("literal"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "n.d.".into());
    Some(format!(
        "{}\t{}\t{}\t{}",
        field(key),
        if authors.is_empty() {
            "—".into()
        } else {
            field(&authors)
        },
        field(&year),
        field(&strip_markup(
            csl.get("title").and_then(Value::as_str).unwrap_or("[untitled]")
        ))
    ))
}

fn author(value: &Value) -> String {
    let text = |name| value.get(name).and_then(Value::as_str).map(str::to_owned);
    Name {
        family: text("family"),
        given: text("given"),
        literal: text("literal"),
        dropping_particle: text("dropping-particle"),
        non_dropping_particle: text("non-dropping-particle"),
        suffix: text("suffix"),
    }
    .display()
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
    let mut seen = std::collections::BTreeSet::new();
    lines.retain(|line| seen.insert(line.clone()));
    lines
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
        CitationError::InvalidLocale => "this locale is not available yet; Ibis formats in en-US.".into(),
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

/// The fallback sentence for each contract error code.
/// Fixed words for output that could not be written; the OS message is not repeated.
pub(super) fn write_failure(error: &std::io::Error) -> &'static str {
    if error.kind() == std::io::ErrorKind::StorageFull {
        "the output could not be written because the disk is full; free some space and try again."
    } else {
        "the output could not be written; check where it is sent and try again."
    }
}

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
    if std::fs::symlink_metadata(path).is_err() {
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

pub(super) fn zotero_failure(code: ErrorCode) -> String {
    let reason = match code {
        ErrorCode::SourceUnavailable => {
            "no local Zotero library was found or it could not be read; check that Zotero is installed and has been opened once, or use --file."
        }
        ErrorCode::BadInput => "its data could not be understood; open Zotero once so it can repair its database.",
        ErrorCode::NewerFormat => "a newer version of Zotero wrote it; update Ibis.",
        ErrorCode::ResourceLimit => "it is larger than Ibis reads at once.",
        code => code_sentence(code),
    };
    format!("cannot read the Zotero library: {reason}")
}

/// Why a citation key did not select exactly one reference.
pub(super) fn key_failure(key: &str, code: ErrorCode, libraries_open: bool) -> String {
    let shown = field(&key.chars().take(80).collect::<String>());
    match code {
        ErrorCode::NotFound if !libraries_open => {
            format!("cannot look up '{shown}': no library is open; add --file <library> or --zotero.")
        }
        ErrorCode::NotFound => {
            format!("no reference has the citation key '{shown}'; find the right key with: ibis search <words>")
        }
        ErrorCode::BadInput if key.is_empty() => "a citation key cannot be empty.".into(),
        ErrorCode::BadInput if key.len() > 64 * 1024 => format!("the citation key '{shown}…' is too long."),
        ErrorCode::BadInput => {
            format!("several references use the citation key '{shown}'; give each one its own key in your library.")
        }
        code => code_sentence(code).into(),
    }
}
