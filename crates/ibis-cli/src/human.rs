//! Sentences and summaries for people; `--json` output never passes through here.
mod projection;
mod record;
use super::out::{self, Out};
use crate::{DispatchResult, Response};
pub(super) use ibis_core::model::is_hidden;
use ibis_core::{
    contract::*,
    destination::{CopyReason, Decision, Destination},
};
use ibis_engine::catalogue::{self, Audience};
use ibis_engine::{
    CheckResult, DocumentProblem, Doubt, EntryRefusal, EntryResult, Finding, KeyCheck, ProjectKey, ProjectProblem,
    ProjectResult, Which,
};
use ibis_engine::{
    LibraryInfo, Reply, SourceWarning,
    health::{SourceFinding, SourceHealth},
};
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
    json: bool,
    holdings: Holdings,
    names: &BTreeMap<String, String>,
    out: Out,
) -> DispatchResult {
    if !json && let Some(text) = summary_reply(&result.data, holdings, names, out) {
        result.text = Some(text);
    }
    result
}

/// The search's rows, or what it means that there are none.
fn search_summary(hits: &[record::Row], holdings: Holdings, out: Out) -> Option<String> {
    if hits.is_empty() {
        return match holdings {
            Holdings::NoLibrary => Some(cli("cli-no-library", &[])),
            Holdings::Empty => catalogue::text("cli-no-references", None, &[]),
            Holdings::References if out.width.is_some() => Some(cli("cli-search-no-match-terminal", &[])),
            Holdings::References => Some(cli("cli-search-no-match", &[])),
        };
    }
    let rows = if let Some(width) = out.width {
        record::terminal_search(hits, width, out)?
    } else {
        hits.iter()
            .map(record::search_row)
            .collect::<Option<Vec<_>>>()?
            .join("\n")
    };
    // Closest matches are said to be so, never shown as full matches.
    if hits.iter().any(|hit| hit.partial) {
        return Some(cli("cli-search-partial", &[("rows", &rows)]));
    }
    Some(rows)
}

fn summary_reply(
    response: &Response,
    holdings: Holdings,
    names: &BTreeMap<String, String>,
    out: Out,
) -> Option<String> {
    let Response::Engine(reply) = response else {
        return None;
    };
    match reply {
        Reply::Show(data) => match &data.value.object {
            LibraryObject::Record(record) => Some(record::reference(&projection::row(record, false, false))),
            _ => None,
        },
        Reply::Search(data) => {
            let hits = &data.hits;
            // What was read around the words, said once before the rows, as the picker shows it.
            let read = data.shorthand.as_ref().map(|read| {
                let prefix = read.prefix.as_deref();
                let locator = read.locator.as_ref().map(|locator| locator.typed.as_str());
                let words = read.search.as_str();
                let cited = [prefix, locator].into_iter().flatten().collect::<Vec<_>>().join(" · ");
                format!(
                    "{}\n",
                    cli(
                        "cli-search-shorthand",
                        &[("cited", &field(&cited)), ("words", &field(words))]
                    )
                )
            });
            let rows = hits.iter().map(projection::hit).collect::<Option<Vec<_>>>()?;
            let summary = search_summary(&rows, holdings, out)?;
            Some(format!("{}{summary}", read.unwrap_or_default()))
        }
        Reply::Libraries(data) => {
            let libraries = &data.value.libraries;
            if libraries.is_empty() {
                return Some(cli("cli-libraries-empty", &[]));
            }
            if out.width.is_some() {
                return terminal_libraries(libraries, names, out);
            }
            let mut lines = vec![format!(
                "{} {}",
                libraries.len(),
                if libraries.len() == 1 { "library" } else { "libraries" }
            )];
            for library in libraries {
                let count = library.objects;
                let warnings = library.warnings_total;
                lines.push(format!(
                    "{}\t{} {}{}{}{}",
                    library_name(library, names),
                    count,
                    if count == 1 { "item" } else { "items" },
                    match warnings {
                        0 => String::new(),
                        1 => ", 1 warning".into(),
                        n => format!(", {n} warnings"),
                    },
                    if library.refresh_error.is_some() {
                        format!(" {}", cli("cli-library-stale-note", &[]))
                    } else {
                        String::new()
                    },
                    if library.watch_warning.is_some() {
                        format!(" {}", cli("cli-library-watch-note", &[]))
                    } else {
                        String::new()
                    }
                ));
                let shown = &library.warnings;
                lines.extend(shown.iter().map(warning_line));
                let listed = shown.len();
                if warnings > listed {
                    lines.push(format!("  and {} more", warnings - listed));
                }
            }
            Some(lines.join("\n"))
        }
        Reply::Outputs(data) => Some(
            data.outputs
                .iter()
                .map(|output| {
                    format!(
                        "{}\t{}\t{}",
                        field(&output.id),
                        field(&output.name),
                        field(output.example.as_deref().unwrap_or_default())
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Reply::Project(data) => Some(project_report(&data.value)),
        Reply::Check(data) => Some(check_report(&data.value)),
        Reply::Entry(data) => match &data.value {
            EntryResult::Entry { text, .. } => Some(text.trim_end().to_owned()),
            EntryResult::Refused { reason } => Some(entry_refusal(*reason)),
        },
        Reply::Outline(data) => Some(outline_summary(&data.value)),
        Reply::Read(data) => Some(read_summary(&data.value)),
        Reply::FindIn(data) => Some(find_in_summary(&data.value)),
        Reply::Styles(data) => {
            let styles = &data.styles;
            let total = data.total;
            let mut lines = styles
                .iter()
                .map(|style| {
                    format!(
                        "{}\t{}\t{}",
                        field(&style.id),
                        field(&style.title),
                        field(&style.updated)
                    )
                })
                .collect::<Vec<_>>();
            if total == 0 {
                lines.push(cli("cli-styles-no-match", &[]));
            } else if styles.len() != total {
                lines.push(cli(
                    "cli-styles-showing",
                    &[("shown", &styles.len().to_string()), ("total", &total.to_string())],
                ));
            }
            Some(lines.join("\n"))
        }
        // `ibis share` writes its own report, naming the keys it was given.
        Reply::Citation(_) | Reply::Schema(_) | Reply::Share(_) => None,
    }
}

/// One line per file of a pack with its size, then what was written or what
/// would be; notes name the keys a file leaves out.
pub(super) fn share_report(
    share: &ibis_engine::ShareResult,
    keys: &[&str],
    destination: Option<&std::path::Path>,
) -> (String, Vec<String>) {
    let mut lines: Vec<String> = share
        .files
        .iter()
        .map(|file| format!("{}\t{}", field(&file.path), size(file.bytes)))
        .collect();
    let count = share.files.len().to_string();
    let total = size(share.total_bytes);
    lines.push(match destination.filter(|_| share.written) {
        Some(path) => cli(
            "cli-share-written",
            &[
                ("count", &count),
                ("size", &total),
                ("path", &quoted(&path.display().to_string())),
            ],
        ),
        None => cli("cli-share-preview", &[("count", &count), ("size", &total)]),
    });
    let named = |places: &[usize]| {
        places
            .iter()
            .filter_map(|place| keys.get(*place))
            .map(|key| field(key))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let notes = [
        ("cli-share-not-in-list", &share.not_in_list),
        ("cli-share-not-in-bibtex", &share.not_in_bibtex),
        ("cli-share-without-pdf", &share.without_pdf),
    ]
    .into_iter()
    .filter(|(_, places)| !places.is_empty())
    .map(|(id, places)| cli(id, &[("keys", &named(places))]))
    .collect();
    (lines.join("\n"), notes)
}

/// Bytes as a person reads them: 812 bytes, 4.2 KB, 18.6 MB.
fn size(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss, reason = "One decimal is shown.")]
    let value = bytes as f64;
    match bytes {
        0..1_000 => format!("{bytes} bytes"),
        1_000..1_000_000 => format!("{:.1} KB", value / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1} MB", value / 1e6),
        _ => format!("{:.1} GB", value / 1e9),
    }
}

/// One line per attachment: its id, title, type and whether its text can be read.
fn outline_summary(data: &ibis_engine::OutlineResult) -> String {
    let attachments = &data.attachments;
    if attachments.is_empty() {
        return cli("cli-outline-none", &[]);
    }
    attachments
        .iter()
        .map(|attachment| {
            let pages = attachment.pages;
            let text = match attachment.text {
                ibis_engine::TextAvailability::Available if attachment.pages_known => cli(
                    if attachment.cite_pages_known {
                        "cli-outline-printed-pages"
                    } else {
                        "cli-outline-pages"
                    },
                    &[("count", &pages.unwrap_or_default().to_string())],
                ),
                ibis_engine::TextAvailability::Available => cli("cli-outline-text", &[]),
                ibis_engine::TextAvailability::NotIndexed => cli("cli-outline-not-indexed", &[]),
                ibis_engine::TextAvailability::NoFile => cli("cli-outline-no-file", &[]),
                ibis_engine::TextAvailability::TooLarge => cli("cli-outline-too-large", &[]),
                ibis_engine::TextAvailability::Missing => cli("cli-outline-missing", &[]),
                ibis_engine::TextAvailability::NotLocal => cli("cli-outline-not-local", &[]),
                ibis_engine::TextAvailability::NotPdf => cli("cli-outline-not-pdf", &[]),
                ibis_engine::TextAvailability::Damaged => cli("cli-outline-damaged", &[]),
                ibis_engine::TextAvailability::TimedOut => cli("cli-outline-timed-out", &[]),
                ibis_engine::TextAvailability::NotYet => cli("cli-outline-not-yet", &[]),
            };
            format!(
                "{}\t{}\t{}\t{text}",
                field(&attachment.id),
                field(attachment.title.as_deref().unwrap_or_default()),
                field(attachment.media_type.as_deref().unwrap_or_default()),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Attachment text as it reads, each page under its number, then how to go on.
fn read_summary(data: &ibis_engine::ReadResult) -> String {
    let mut parts = Vec::new();
    for page in &data.pages {
        let text: Vec<String> = page.text.lines().map(super::terminal_display).collect();
        if let Some(mark) = page_mark(page.pdf_page, page.cite_page.as_deref()) {
            parts.push(mark);
        }
        parts.push(text.join("\n"));
    }
    if let Some(next) = &data.next {
        let attachment = field(&data.attachment);
        let mut command = format!("ibis read {attachment}");
        if let Some(page) = next.first_page {
            command.push_str(&format!(" --pages {page}-{}", page + 4));
        }
        if let Some(offset) = next.offset {
            command.push_str(&format!(" --offset {offset}"));
        }
        parts.push(cli("cli-read-more", &[("command", &command)]));
    }
    parts.join("\n")
}

/// The file's page, and the printed page to cite when its label is known.
fn page_mark(page: Option<u32>, label: Option<&str>) -> Option<String> {
    let page = page?.to_string();
    Some(match label {
        Some(label) => cli("cli-read-printed-page", &[("page", &page), ("label", &field(label))]),
        None => cli("cli-read-page", &[("page", &page)]),
    })
}

/// One line per passage, after its page when pages are known.
fn find_in_summary(data: &ibis_engine::FindInResult) -> String {
    let passages = &data.passages;
    if passages.is_empty() {
        return cli("cli-find-in-none", &[]);
    }
    let mut lines = passages
        .iter()
        .map(|passage| {
            let text = field(&passage.text);
            match page_mark(passage.pdf_page, passage.cite_page.as_deref()) {
                Some(mark) => format!("{mark}\t{text}"),
                None => text,
            }
        })
        .collect::<Vec<_>>();
    if !data.complete {
        lines.push(cli("cli-find-in-more", &[("count", &passages.len().to_string())]));
    }
    lines.join("\n")
}

/// One line per key, the file's own state first, in the words the app uses.
pub(super) fn project_report(result: &ProjectResult) -> String {
    let name = field(&result.name);
    let lines = |lines: &[usize]| lines.iter().map(usize::to_string).collect::<Vec<_>>().join(", ");
    let mut report = vec![];
    if let Some(problem) = &result.problem {
        let why = match problem {
            ProjectProblem::Unavailable => cli("cli-project-unavailable", &[]),
            ProjectProblem::NotAFile => cli("cli-project-not-file", &[]),
            ProjectProblem::TooLarge => cli("cli-project-too-large", &[]),
            ProjectProblem::NotUtf8 => cli("cli-project-not-utf8", &[]),
            ProjectProblem::Changing => cli("cli-project-changing", &[]),
            ProjectProblem::NotBibtex => cli("cli-project-not-bibtex", &[]),
        };
        report.push(cli("cli-project-unreadable", &[("name", &name), ("reason", &why)]));
    }
    if !result.damaged_lines.is_empty() {
        report.push(cli(
            "cli-project-damaged",
            &[("name", &name), ("lines", &lines(&result.damaged_lines))],
        ));
    }
    for ProjectKey { key, check } in &result.keys {
        let status = match check {
            KeyCheck::In { line } => cli("cli-project-key-in", &[("name", &name), ("line", &line.to_string())]),
            KeyCheck::Missing => cli("cli-project-key-missing", &[("name", &name)]),
            KeyCheck::CouldNotCheck => cli("cli-project-key-unchecked", &[("name", &name)]),
            KeyCheck::Repeated { lines: found } => cli("cli-project-key-repeated", &[("lines", &lines(found))]),
            KeyCheck::MayNameAnother { line, doubt } => cli(
                "cli-project-key-doubt",
                &[
                    ("line", &line.to_string()),
                    (
                        "reason",
                        &match doubt {
                            Doubt::Fields => cli("cli-project-fields-differ", &[]),
                            Doubt::LetterCase => cli("cli-project-case-differs", &[]),
                            Doubt::Alias => cli("cli-project-alias", &[]),
                            Doubt::Helper => cli("cli-project-helper", &[]),
                            Doubt::Commented => cli("cli-project-commented", &[]),
                        },
                    ),
                ],
            ),
        };
        report.push(format!("{}\t{status}", field(key)));
    }
    report.join("\n")
}

/// One line per problem, at its line and column, then what was read.
fn check_report(result: &CheckResult) -> String {
    use ibis_sources::document::Refusal;
    let checked = match result {
        CheckResult::Refused { file, reason } => {
            let reason = cli(
                match reason {
                    Refusal::Missing => "cli-check-file-missing",
                    Refusal::NotLocal => "cli-check-file-not-local",
                    Refusal::TimedOut => "cli-check-file-timed-out",
                    Refusal::Unsupported => "cli-check-file-unsupported",
                    Refusal::TooLarge => "cli-check-file-too-large",
                    Refusal::NotText => "cli-check-file-not-text",
                    Refusal::Damaged | Refusal::Unsafe | Refusal::Mismatch => "cli-check-file-damaged",
                    Refusal::Unavailable => "cli-check-file-unavailable",
                },
                &[],
            );
            return cli(
                match file {
                    Which::Document => "cli-check-document-unreadable",
                    Which::Set => "cli-check-set-unreadable",
                },
                &[("reason", &reason)],
            );
        }
        CheckResult::Checked(checked) => &checked.read,
    };
    let mut report: Vec<String> = checked.findings.iter().map(check_finding).collect();
    let counts = [
        ("citations", checked.citations.to_string()),
        ("keys", checked.keys.to_string()),
        ("problems", checked.findings.len().to_string()),
    ];
    let counts: Vec<_> = counts.iter().map(|(name, value)| (*name, value.as_str())).collect();
    report.push(cli("cli-check-summary", &counts));
    if !checked.complete {
        report.push(cli("cli-check-incomplete", &[]));
    }
    if checked.cites_everything {
        report.push(cli("cli-check-cites-everything", &[]));
    }
    report.join("\n")
}

fn check_finding(finding: &Finding) -> String {
    let current = finding.current.as_deref().map(field).unwrap_or_default();
    let pages = finding
        .pages
        .iter()
        .map(|page| field(page))
        .collect::<Vec<_>>()
        .join(", ");
    let problem = match finding.problem {
        DocumentProblem::Unclosed => cli("cli-check-unclosed", &[]),
        DocumentProblem::WithoutKey => cli("cli-check-without-key", &[]),
        DocumentProblem::TwoKeys => cli("cli-check-two-keys", &[]),
        DocumentProblem::Locator => cli("cli-check-locator", &[]),
        DocumentProblem::TooLong => cli("cli-check-too-long", &[]),
        DocumentProblem::Missing => cli("cli-check-missing", &[]),
        DocumentProblem::Ambiguous => cli("cli-check-ambiguous", &[]),
        DocumentProblem::Duplicate => cli("cli-check-duplicate", &[]),
        DocumentProblem::Damaged => cli("cli-check-damaged", &[]),
        DocumentProblem::CutShort => cli("cli-check-cut-short", &[]),
        DocumentProblem::Invalid => cli("cli-check-invalid", &[]),
        DocumentProblem::NotInSet => cli("cli-check-not-in-set", &[]),
        DocumentProblem::Changed => cli("cli-check-changed", &[]),
        DocumentProblem::TwiceInSet => cli("cli-check-twice-in-set", &[]),
        DocumentProblem::Uncompared => cli("cli-check-uncompared", &[]),
        DocumentProblem::Renamed => cli("cli-check-renamed", &[("current", &current)]),
        DocumentProblem::CitesEverything => cli("cli-check-cites-everything", &[]),
        DocumentProblem::QuoteNotFound => cli("cli-check-quote-not-found", &[]),
        DocumentProblem::QuoteOnOtherPage => cli("cli-check-quote-on-other-page", &[("pages", &pages)]),
        DocumentProblem::QuotePageUnknown => cli("cli-check-quote-page-unknown", &[]),
        DocumentProblem::QuoteUnchecked => cli("cli-check-quote-unchecked", &[]),
    };
    let place = finding
        .place
        .map_or_else(|| "-".to_owned(), |place| format!("{}:{}", place.line, place.column));
    let key = finding.key.as_deref().map(field).unwrap_or_default();
    format!("{place}\t{key}\t{problem}")
}

fn entry_refusal(reason: EntryRefusal) -> String {
    match reason {
        EntryRefusal::NoOriginal => cli("cli-entry-no-original", &[]),
        EntryRefusal::Changed => cli("cli-entry-changed", &[]),
        EntryRefusal::Repeated => cli("cli-entry-repeated", &[]),
        EntryRefusal::Dependency => cli("cli-entry-dependency", &[]),
    }
}

/// Citation languages for `ibis styles --locales`.
pub(super) fn locale_list(mut result: DispatchResult) -> DispatchResult {
    if let Response::Engine(Reply::Styles(data)) = &result.data {
        result.text = Some(
            data.locales
                .iter()
                .map(|locale| format!("{}\t{}", field(&locale.tag), field(&locale.english)))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    result
}

fn library_name(library: &LibraryInfo, names: &BTreeMap<String, String>) -> String {
    let id = library.id.as_str();
    field(&names.get(id).cloned().unwrap_or_else(|| match library.zotero {
        Some(ibis_core::zotero::Library::Group(group)) => format!("Zotero group {group}"),
        _ if library.kind == ibis_engine::LibraryKind::Zotero => "Zotero".into(),
        _ => id.into(),
    }))
}

fn terminal_libraries(libraries: &[LibraryInfo], names: &BTreeMap<String, String>, out: Out) -> Option<String> {
    let width = out.width?;
    let labels = libraries
        .iter()
        .map(|library| library_name(library, names))
        .collect::<Vec<_>>();
    let columns = labels
        .iter()
        .map(|name| out::width(name))
        .max()
        .unwrap_or(0)
        .min(width / 2);
    let mut lines = vec![format!(
        "{} {}",
        libraries.len(),
        if libraries.len() == 1 { "library" } else { "libraries" }
    )];
    for (library, name) in libraries.iter().zip(labels) {
        let count = library.objects;
        let total = library.warnings_total;
        let name = out::truncate(&name, columns);
        let counts = format!(
            "{count} {}{}",
            if count == 1 { "item" } else { "items" },
            match total {
                0 => String::new(),
                1 => ", 1 warning".into(),
                n => format!(", {n} warnings"),
            }
        );
        lines.push(format!(
            "{}{} — {}",
            out.bold(&name),
            " ".repeat(columns - out::width(&name)),
            out.dim(&out::truncate(&counts, width.saturating_sub(columns + 3)))
        ));
        for (present, message) in [
            (library.refresh_error.is_some(), cli("cli-library-refresh-failed", &[])),
            (library.watch_warning.is_some(), cli("cli-library-not-watched", &[])),
        ] {
            if present {
                lines.extend(wrap_warning(&message, width));
            }
        }
        let shown = &library.warnings;
        for warning in shown.iter().take(10) {
            lines.extend(wrap_warning(&entry_warning(warning), width));
        }
        let listed = shown.len().min(10);
        if total > listed {
            lines.extend(wrap_warning(&format!("and {} more; see --json", total - listed), width));
        }
    }
    Some(lines.join("\n"))
}

fn wrap_warning(text: &str, width: usize) -> Vec<String> {
    let indent = " ".repeat(2.min(width.saturating_sub(1)));
    let available = width.saturating_sub(indent.len());
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let word = out::truncate(word, available);
        if !line.is_empty() && out::width(&line) + 1 + out::width(&word) > available {
            lines.push(format!("{indent}{line}"));
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&word);
    }
    if !line.is_empty() {
        lines.push(format!("{indent}{line}"));
    }
    lines
}

fn entry_warning(warning: &SourceWarning) -> String {
    let label = warning
        .entry
        .as_deref()
        .or(warning.key.as_deref())
        .map(field)
        .or_else(|| warning.line.map(|line| format!("entry on line {line}")));
    let field = warning.field.as_deref().map(field);
    let subject = field.map_or_else(|| "a field".into(), |field| format!("the \"{field}\" field"));
    let what = match warning.code {
        ibis_engine::WarningCode::Field | ibis_engine::WarningCode::WorkLimit
            if warning.cause == Some(ibis_engine::WarningCause::TooLarge)
                && warning
                    .field
                    .as_deref()
                    .is_some_and(|field| matches!(field, "issued" | "accessed" | "original-date" | "date")) =>
        {
            cli("cli-warning-text-kept", &[("subject", &subject)])
        }
        ibis_engine::WarningCode::Field => match warning.cause {
            Some(ibis_engine::WarningCause::TexCommand) => cli("cli-warning-text-kept", &[("subject", &subject)]),
            Some(ibis_engine::WarningCause::Date) => cli("cli-warning-date", &[("subject", &subject)]),
            Some(ibis_engine::WarningCause::UnknownField) => cli("cli-warning-unknown-field", &[]),
            Some(ibis_engine::WarningCause::UnknownType) => cli("cli-warning-unknown-type", &[]),
            Some(ibis_engine::WarningCause::Link) => cli("cli-warning-link", &[("subject", &subject)]),
            Some(ibis_engine::WarningCause::Names) => cli("cli-warning-names", &[("subject", &subject)]),
            _ => cli("cli-warning-fields-kept", &[("subject", &subject)]),
        },
        ibis_engine::WarningCode::DuplicateKey => cli("cli-warning-duplicate-key", &[]),
        ibis_engine::WarningCode::Syntax => {
            if warning.field.as_deref().is_some() {
                cli("cli-warning-syntax-field", &[("subject", &subject)])
            } else {
                cli("cli-warning-syntax-entry", &[])
            }
        }
        ibis_engine::WarningCode::WorkLimit if label.is_some() => cli("cli-warning-entry-too-large", &[]),
        ibis_engine::WarningCode::WorkLimit => cli("cli-warning-library-incomplete", &[]),
        ibis_engine::WarningCode::InvalidAttachment => cli("cli-warning-attachment-skipped", &[]),
        _ => warning_line(warning).trim().to_owned(),
    };
    match label {
        Some(label) => format!("{label}: {what}"),
        None => what,
    }
}

pub(super) fn search_notes(
    health: &SourceHealth,
    libraries: &[LibraryInfo],
    names: &BTreeMap<String, String>,
) -> Vec<String> {
    health
        .findings
        .iter()
        .filter_map(|finding| {
            let SourceFinding::RefreshFailed {
                library,
                last_successful_read_ms,
                ..
            } = finding
            else {
                return None;
            };
            let name = libraries
                .iter()
                .find(|info| &info.id == library)
                .map(|info| library_name(info, names))
                .unwrap_or_else(|| "A library".into());
            let time = i64::try_from(*last_successful_read_ms)
                .ok()
                .and_then(|ms| jiff::Timestamp::from_millisecond(ms).ok())
                .map(|time| {
                    time.to_zoned(jiff::tz::TimeZone::system())
                        .strftime("%-I:%M %p")
                        .to_string()
                });
            Some(match time {
                Some(time) => cli("cli-search-stale-at", &[("name", &name), ("time", &time)]),
                None => cli("cli-search-stale", &[("name", &name)]),
            })
        })
        .collect()
}

/// A source warning as a sentence.
pub(super) fn warning_line(warning: &SourceWarning) -> String {
    let key = warning.key.as_deref().map(field);
    let what = match (warning.code, key) {
        (ibis_engine::WarningCode::DuplicateKey, Some(key)) => {
            // In CSL-JSON this is the record's `id`, which is never a citation key.
            cli("cli-warning-duplicate-key-named", &[("key", &key)])
        }
        (ibis_engine::WarningCode::ZoteroNewerThanTested, _) => catalogue::sentence(
            "source-warning",
            ibis_engine::WarningCode::ZoteroNewerThanTested,
            Audience::Cli,
            &[],
        ),
        (ibis_engine::WarningCode::Syntax, _) => cli("cli-warning-syntax", &[]),
        (ibis_engine::WarningCode::Field, _) => cli("cli-warning-field", &[]),
        (ibis_engine::WarningCode::WorkLimit, _) => cli("cli-warning-work-limit", &[]),
        (ibis_engine::WarningCode::InvalidAttachment, _) => cli("cli-warning-invalid-attachment", &[]),
        (ibis_engine::WarningCode::PdfsNotKept, _) => cli("cli-warning-pdfs-not-kept", &[]),
        (ibis_engine::WarningCode::CloudSyncedDirectory, _) => cli("cli-warning-cloud-directory", &[]),
        (ibis_engine::WarningCode::CitationKeysUnavailable, _) => cli("cli-warning-keys-unavailable", &[]),
        _ => cli("cli-warning-unknown", &[]),
    };
    // Why, from the engine's words for the warning's cause, when it gives one.
    let why = catalogue::cause(warning)
        .map(|cause| format!(" ({cause})"))
        .unwrap_or_default();
    match warning.line {
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
pub(super) fn citation_report(data: &CitationResult, citing: &Citing, printed: bool) -> Vec<String> {
    let outcomes = &data.outcomes;
    let losses = &data.losses;
    let notices = &data.notices;
    let warnings = &data.warnings;
    let output = field(&citing.output);
    let mut lines = Vec::new();
    for warning in warnings {
        let label = citing.label(warning.item);
        let kind = match warning.code {
            CitationWarningCode::SourceField | CitationWarningCode::Unknown => "warning",
            CitationWarningCode::SentenceCaseExpected => "note",
        };
        let said = catalogue::sentence("warning", warning.code, Audience::Cli, &[("reference", &label)]);
        lines.push(format!("{kind}: {said}"));
    }
    let refused: Vec<_> = outcomes
        .iter()
        .filter_map(|outcome| match outcome {
            ItemOutcome::Refused { item, error } => Some((*item, *error)),
            ItemOutcome::Rendered { .. } | ItemOutcome::Unknown => None,
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
        // A newer engine's field or reason is named as needing a check, never as one Ibis knows.
        let what = if loss.field == LossField::Unknown {
            cli("cli-loss-something", &[])
        } else {
            cli(&catalogue::id("loss", loss.field), &[])
        };
        let target = loss
            .item
            .map(|item| format!(" for {}", citing.label(item)))
            .unwrap_or_default();
        let id = match loss.reason {
            LossReason::Unsupported => "cli-loss-left-out",
            LossReason::Flattened => "cli-loss-flattened",
            LossReason::Unknown => "cli-loss-unknown",
        };
        lines.push(cli(id, &[("output", &output), ("what", &what), ("target", &target)]));
    }
    // Notices describe printed text; a refused citation printed none.
    for notice in notices.iter().copied().filter(|_| printed) {
        lines.push(format!(
            "note: {}",
            catalogue::sentence("notice", notice, Audience::Cli, &[])
        ));
    }
    // A person reading English output needs no note; any other language is a choice to see.
    if let Some(language) = data
        .language
        .as_deref()
        .filter(|language| printed && *language != "en-US")
    {
        lines.push(cli("cli-citation-language", &[("language", &field(language))]));
    }
    if printed && let Some(destination) = &data.destination {
        lines.push(destination_note(destination));
    }
    let mut seen = std::collections::BTreeSet::new();
    lines.retain(|line| seen.insert(line.clone()));
    lines
}

/// What the app would do with this citation; pasting is on and permitted here.
fn destination_note(destination: &Destination) -> String {
    let name = destination.name.as_deref().unwrap_or("this app");
    let id = match destination.delivery {
        Decision::Paste => "cli-destination-paste",
        Decision::Copy(CopyReason::UnknownApp) => "cli-destination-unknown",
        Decision::Copy(CopyReason::UnsupportedSystem) => "cli-destination-unsupported",
        Decision::Copy(CopyReason::Terminal) => "cli-destination-terminal",
        Decision::Copy(CopyReason::RemoteDesktop | CopyReason::VirtualMachine) => "cli-destination-remote",
        Decision::Copy(CopyReason::NonstandardShortcut) => "cli-destination-shortcut",
        Decision::Copy(_) => "cli-destination-copy",
    };
    cli(id, &[("name", name)])
}

fn refusal(error: CitationError, label: &str, output: &str, form: CitationForm, flags: &str) -> String {
    let form = catalogue::id("form", form);
    let form = form.strip_prefix("form-").unwrap_or_default();
    let args = [
        ("reference", label),
        ("output", output),
        ("form", form),
        ("options", flags),
    ];
    catalogue::citation(error, Audience::Cli, &args)
}

/// One of the command line's own sentences from the catalogue.
pub(super) fn cli(id: &str, args: &[(&str, &str)]) -> String {
    catalogue::text(id, None, args).unwrap_or_else(|| id.to_owned())
}

/// Fixed words for output that could not be written; the OS message is not repeated.
pub(super) fn write_failure(error: &std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::StorageFull {
        cli("cli-output-disk-full", &[])
    } else {
        cli("cli-output-unwritten", &[])
    }
}

/// The fallback sentence for each contract error code.
pub(super) fn code_sentence(code: ErrorCode) -> String {
    catalogue::code(code, Audience::Cli)
}

pub(super) fn file_failure(path: &Path, error: EngineError) -> String {
    cli(
        "cli-file-unopened",
        &[
            ("path", &field(&path.display().to_string())),
            ("reason", &file_reason(path, error)),
        ],
    )
}

/// Why a library file did not open, without naming it.
pub(super) fn file_reason(path: &Path, error: EngineError) -> String {
    // A folder of PDFs has a library only once someone keeps it (decision 0061).
    let probed = path.to_owned();
    let unkept = crate::bounded(move || {
        probed.is_dir() && std::fs::symlink_metadata(probed.join(ibis_sources::folder::LIBRARY_FILE)).is_err()
    });
    if unkept == Some(true) {
        return cli("cli-folder-not-kept", &[]);
    }
    if let Some(reason) = error.reason {
        return reason_sentence(reason);
    }
    let code = error.code;
    // The bounded read has already failed; looking again is bounded too, so a
    // stalled disk or a path just replaced by a pipe cannot hold the command.
    // Without an answer in time, the read's own reason stands.
    let probed = path.to_owned();
    let found = crate::bounded(move || path_reason(&probed)).flatten();
    found.unwrap_or_else(|| match code {
        ErrorCode::BadInput => catalogue::text("cli-file-unreadable", None, &[]).unwrap_or_default(),
        ErrorCode::SourceUnavailable => cli("cli-file-busy", &[]),
        ErrorCode::ResourceLimit => cli("cli-file-too-large", &[]),
        code => code_sentence(code),
    })
}

/// What the path itself shows: missing, a folder, or a file this program may not read.
fn path_reason(path: &Path) -> Option<String> {
    let link_to_nothing = std::fs::symlink_metadata(path).is_ok()
        && std::fs::metadata(path).is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound);
    if std::fs::symlink_metadata(path).is_err() || link_to_nothing {
        Some(cli("cli-file-missing", &[]))
    } else if path.is_dir() {
        Some(cli("cli-file-is-folder", &[]))
    } else if std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
        // Only a regular file is opened to check: opening a pipe or a device can block.
        && std::fs::File::open(path).is_err_and(|error| error.kind() == std::io::ErrorKind::PermissionDenied)
    {
        Some(cli("cli-file-permission", &[]))
    } else {
        None
    }
}

pub(super) fn zotero_failure(error: EngineError, profiles: &[String]) -> String {
    let reason = match (error.code, error.reason) {
        (_, Some(reason)) => reason_sentence(reason),
        (ErrorCode::SourceUnavailable, None) => cli("cli-zotero-unavailable", &[]),
        (ErrorCode::BadInput, None) => cli("cli-zotero-damaged", &[]),
        (ErrorCode::NewerFormat, None) => cli("cli-zotero-newer", &[]),
        (ErrorCode::ResourceLimit, None) => cli("cli-zotero-too-large", &[]),
        (code, None) => code_sentence(code),
    };
    if error.reason == Some(ErrorReason::ZoteroProfileNeeded) && !profiles.is_empty() {
        let names: Vec<_> = profiles.iter().map(|name| field(name)).collect();
        return cli(
            "cli-zotero-unreadable-profiles",
            &[("reason", &reason), ("profiles", &names.join(", "))],
        );
    }
    cli("cli-zotero-unreadable", &[("reason", &reason)])
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
            cli("cli-key-duplicate", &[("key", &shown), ("at", &at)])
        }
        (_, Some(ErrorReason::DamagedEntry)) => {
            let at = lines.first().map(|line| format!(" on line {line}")).unwrap_or_default();
            let why = warnings
                .first()
                .and_then(catalogue::cause)
                .map(|cause| format!(" ({cause})"))
                .unwrap_or_default();
            cli("cli-key-damaged", &[("key", &shown), ("at", &at), ("reason", &why)])
        }
        (_, Some(reason @ ErrorReason::LibraryCutShort)) => cli(
            "cli-key-library-incomplete",
            &[("key", &shown), ("reason", &reason_sentence(reason))],
        ),
        (_, Some(ErrorReason::AmbiguousKey)) => cli("cli-key-ambiguous", &[("key", &shown)]),
        (ErrorCode::NotFound, _) if !libraries_open => cli("cli-key-without-library", &[("key", &shown)]),
        (ErrorCode::NotFound, _) => cli("cli-key-not-found-search", &[("key", &shown)]),
        (ErrorCode::BadInput, _) if key.is_empty() => cli("cli-key-empty", &[]),
        (ErrorCode::BadInput, _) if key.len() > 64 * 1024 => cli("cli-key-too-long", &[("key", &shown)]),
        (code, reason) => reason.map_or_else(|| code_sentence(code), reason_sentence),
    }
}

/// The sentence for each reason an engine error gives.
pub(super) fn reason_sentence(reason: ErrorReason) -> String {
    catalogue::sentence("reason", reason, Audience::Cli, &[])
}

#[cfg(test)]
fn test_library(id: &str, objects: usize) -> LibraryInfo {
    LibraryInfo {
        id: LibraryId::new(id).unwrap(),
        kind: ibis_engine::LibraryKind::Zotero,
        capabilities: vec![],
        revision: ibis_core::contract::Fingerprint::of_bytes(id.as_bytes()),
        objects,
        warnings: vec![],
        warnings_total: 0,
        refresh_error: None,
        refresh_reason: None,
        updated_at_ms: 0,
        watch_warning: None,
        zotero: None,
        duplicate_keys: Default::default(),
        damaged_keys: Default::default(),
        cut_short: false,
    }
}
#[cfg(test)]
fn test_libraries(libraries: Vec<LibraryInfo>) -> Response {
    Response::Engine(Reply::Libraries(ibis_engine::health::Checked {
        value: ibis_engine::LibrariesResult {
            libraries,
            zotero_profiles: None,
            zotero_groups: None,
            zotero_installed: None,
        },
        source_health: SourceHealth::of(std::iter::empty()),
    }))
}
#[cfg(test)]
fn test_search(hits: Vec<ibis_engine::SearchHit>) -> Response {
    Response::Engine(Reply::Search(Box::new(ibis_engine::SearchResult {
        snapshot: Snapshot::new(vec![]).unwrap().fingerprints().clone(),
        hits,
        complete: true,
        list: None,
        shorthand: None,
        kept_words: false,
        source_health: SourceHealth::of(std::iter::empty()),
    })))
}

#[cfg(test)]
mod terminal_tests {
    use super::*;
    use serde_json::json;

    fn search_hit(key: &str, title: &str) -> ibis_engine::SearchHit {
        let record = Record {
            id: ItemRef { library: LibraryId::new("invented").unwrap(), key: "record:invented".into() },
            added_at: None,
            csl: serde_json::from_value(json!({"title":title, "author":[{"family":"Valé"},{"family":"Fen"},{"family":"Ng"},{"family":"Park"}], "issued":{"date-parts":[[2024]]}})).unwrap(),
            citation_keys: vec![CitationKey { value: key.into(), source: CitationKeySource::Zotero }],
            aliases: vec![], warnings: vec![], identifiers: vec![],
        };
        ibis_engine::SearchHit {
            item: SnapshotObject::new(LibraryObject::Record(record)).unwrap(),
            score: 1,
            matched: vec![],
            partial: false,
            typo: false,
        }
    }

    fn hit(key: &str, title: &str) -> record::Row {
        projection::hit(&search_hit(key, title)).unwrap_or_else(|| panic!("synthetic row"))
    }

    #[test]
    fn eighty_and_120_columns_keep_hits_aligned_and_truncate_titles_by_display_width() {
        let hits = vec![
            hit("vale2024", &"城市庭園 e\u{301}lan ".repeat(30)),
            hit("fen2024longer", &"Lantern gardens ".repeat(30)),
            hit(&"longkey".repeat(9), &"城市庭園 ".repeat(30)),
        ];
        for width in [80, 120] {
            let text = record::terminal_search(
                &hits,
                width,
                Out {
                    width: Some(width),
                    color: false,
                },
            )
            .unwrap_or_default();
            let lines: Vec<_> = text.lines().collect();
            assert_eq!(lines.len(), 4, "{text}");
            assert!(lines.iter().all(|line| out::width(line) <= width), "{text}");
            assert!(lines[0].starts_with("vale2024       Valé et al. 2024  "), "{text}");
            assert!(lines[1].starts_with("fen2024longer  Valé et al. 2024  "), "{text}");
            assert!(lines[3].starts_with("  Valé et al. 2024  "), "{text}");
            for line in [lines[0], lines[1], lines[3]] {
                assert!(line.ends_with('…'), "{text}");
                assert!(out::width(line) >= width - 1, "{text}");
            }
            assert!(!text.contains('\x1b'));
        }
        // Plain stdout keeps the original four TSV columns, authors and title.
        assert_eq!(
            record::search_row(&hit("vale2024", "城市庭園")),
            Some("vale2024\tValé, Fen, Ng, et al.\t2024\t城市庭園".into())
        );
        let key = "長".repeat(100);
        let text = record::terminal_search(
            &[hit(&key, &"城市庭園".repeat(30))],
            80,
            Out {
                width: Some(80),
                color: false,
            },
        )
        .unwrap_or_default();
        assert_eq!(text.lines().count(), 2);
        assert_eq!(text.lines().next(), Some(key.as_str()));
        assert!(
            text.lines()
                .skip(1)
                .all(|line| out::width(line) <= 80 && line.ends_with('…')),
            "{text}"
        );
        for width in [1, 4] {
            let text = record::terminal_search(
                &[hit(&key, "Lantern gardens")],
                width,
                Out {
                    width: Some(width),
                    color: false,
                },
            )
            .unwrap_or_default();
            assert_eq!(text.lines().next(), Some(key.as_str()));
            assert!(text.contains("2024"), "only titles and authors shrink: {text}");
        }
    }

    #[test]
    fn terminal_warnings_name_entries_wrap_and_cap_while_pipes_include_causes() {
        let warnings: Vec<_> = (0..12)
            .map(|i| json!({"code":"field","entry":format!("vale{i}"),"field":"title","cause":"tex-command"}))
            .collect();
        let mut library = test_library("invented", 165);
        library.warnings = warnings
            .into_iter()
            .map(|warning| serde_json::from_value(warning).unwrap())
            .collect();
        library.warnings_total = 12;
        let data = test_libraries(vec![library]);
        let out = Out {
            width: Some(80),
            color: false,
        };
        let text = summary_reply(&data, Holdings::References, &BTreeMap::new(), out).unwrap_or_default();
        assert!(
            text.starts_with("1 library\nZotero — 165 items, 12 warnings\n"),
            "{text}"
        );
        assert!(
            text.contains("vale0: the \"title\" field could not be fully converted; Ibis uses the"),
            "{text}"
        );
        assert!(
            text.split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .contains("Ibis uses the text as written"),
            "{text}"
        );
        assert!(text.contains("vale9:"), "{text}");
        assert!(!text.contains("vale10:"), "{text}");
        assert!(text.ends_with("and 2 more; see --json"), "{text}");
        assert!(text.lines().all(|line| out::width(line) <= 80));
        assert_eq!(
            entry_warning(
                &serde_json::from_value(
                    json!({"code":"field","entry":"Invented keyless title","field":"issued","cause":"value"})
                )
                .unwrap()
            ),
            "Invented keyless title: the \"issued\" field could not be fully converted; Ibis kept the reference with the fields it could read"
        );
        let plain = summary_reply(
            &data,
            Holdings::References,
            &BTreeMap::new(),
            Out {
                width: None,
                color: false,
            },
        )
        .unwrap_or_default();
        assert_eq!(
            plain,
            format!(
                "1 library\nZotero\t165 items, 12 warnings\n{}",
                ["  a field could not be fully converted; check this entry (it uses a TeX command Ibis does not understand, kept as written)"; 12].join("\n")
            )
        );
        assert_eq!(
            summary_reply(&test_search(vec![]), Holdings::References, &BTreeMap::new(), out).as_deref(),
            Some("No references matched. Try a shorter query.")
        );
        let libraries = test_libraries(vec![test_library("one", 1), test_library("two", 165)]);
        let names = BTreeMap::from([("one".into(), "甲文".into()), ("two".into(), "Zotero".into())]);
        assert_eq!(
            summary_reply(&libraries, Holdings::References, &names, out).as_deref(),
            Some("2 libraries\n甲文   — 1 item\nZotero — 165 items")
        );
    }
}
