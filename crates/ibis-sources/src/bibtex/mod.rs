//! Read-only BibTeX/BibLaTeX source adapter.
//!
//! Loading happens in three steps, each testable on its own:
//!
//! 1. [`lexer`] finds top-level blocks and parses entry fields, tolerating
//!    damage: an unclosed entry is reported and scanning resumes at the next
//!    entry.
//! 2. [`resolve`] expands `@string` macros and follows `crossref`/`xdata`
//!    inheritance as a checked graph. Cycles and missing targets are reported
//!    and broken before anything reaches the field parser.
//! 3. [`convert`] turns each resolved entry into a [`Record`], keeping
//!    protected capitalization, italics, truncated author lists, date ranges
//!    and language tags.
//!
//! Completed entries load; every problem becomes a [`SourceWarning`] with a
//! line number. A failure inside the third-party field parser is contained to
//! the entry that caused it.

mod convert;
mod dates;
pub mod edit;
mod lexer;
pub(crate) mod project;
mod resolve;

use std::cell::Cell;

use biblatex::Bibliography;

use crate::candidates::Candidates;
use crate::{ParsedLibrary, SourceWarning, WarningCause, WarningCode};
use ibis_core::contract::{LibraryId, Record, Snapshot};
use ibis_os::panic_guard;
use lexer::{Block, RawEntry, header_key, parse_entry, parse_string, split_blocks};
use resolve::Resolver;

/// Why an entry was skipped or one of its fields needs checking: a fixed
/// cause and the field's name, never the entry's own text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct Problem {
    pub cause: WarningCause,
    pub field: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

impl Problem {
    /// What keeping it costs the load budget.
    pub(crate) fn size(&self) -> usize {
        self.field.map_or(0, str::len).saturating_add(16)
    }

    pub(crate) fn new(cause: WarningCause, field: Option<&str>) -> Self {
        Self {
            cause,
            field: field.and_then(diagnostic_field),
            line: None,
        }
    }
}

/// Only known BibTeX/BibLaTeX names can leave the parser in a diagnostic.
/// Returning a static name also keeps repeated warnings allocation-free.
fn diagnostic_field(name: &str) -> Option<&'static str> {
    const FIELDS: &[&str] = &[
        "abstract",
        "address",
        "afterword",
        "annotator",
        "archiveprefix",
        "author",
        "bookauthor",
        "booksubtitle",
        "booktitle",
        "booktitleaddon",
        "chapter",
        "commentator",
        "crossref",
        "date",
        "day",
        "doi",
        "edition",
        "editor",
        "editora",
        "editorb",
        "editorc",
        "editortype",
        "editoratype",
        "editorbtype",
        "editorctype",
        "endyear",
        "entrysubtype",
        "eprint",
        "eprintclass",
        "eprinttype",
        "eventdate",
        "eventday",
        "eventmonth",
        "eventtitle",
        "eventyear",
        "file",
        "foreword",
        "howpublished",
        "hyphenation",
        "institution",
        "introduction",
        "isbn",
        "issn",
        "issue",
        "journal",
        "journalsubtitle",
        "journaltitle",
        "journaltitleaddon",
        "keywords",
        "langid",
        "language",
        "location",
        "mainsubtitle",
        "maintitle",
        "maintitleaddon",
        "month",
        "note",
        "number",
        "organization",
        "origdate",
        "origday",
        "origmonth",
        "origyear",
        "pages",
        "pagetotal",
        "pmcid",
        "pmid",
        "publisher",
        "school",
        "series",
        "shorttitle",
        "subtitle",
        "title",
        "titleaddon",
        "translator",
        "type",
        "url",
        "urldate",
        "urlday",
        "urlmonth",
        "urlyear",
        "version",
        "volume",
        "xdata",
        "xref",
        "year",
    ];
    FIELDS.iter().copied().find(|field| name.eq_ignore_ascii_case(field))
}

/// Recognize entries with the loader's scanner, including parenthesized and
/// damaged blocks, without treating directives or their contents as entries.
pub(crate) fn has_entry_block(text: &str) -> bool {
    lexer::has_entry(text).unwrap_or_else(|| {
        let (blocks, limited) = split_blocks(text);
        blocks
            .iter()
            .any(|block| !matches!(block.kind.as_str(), "comment" | "preamble" | "string"))
            || limited && has_unread_entry(text, &blocks)
    })
}

fn has_unread_entry(text: &str, blocks: &[Block<'_>]) -> bool {
    // Comments past the cap leave no reference unread. If recovery cannot
    // locate the unread tail or tell what it holds, assume it may have entries.
    blocks.get(crate::MAX_LIBRARY_RECORDS).is_none_or(|first| {
        let unread = first.text.as_ptr() as usize - text.as_ptr() as usize;
        lexer::has_entry(&text[unread..]).unwrap_or(true)
    })
}

#[cfg(test)]
thread_local! {
    // Per-thread so concurrent generated cases cannot hide each other's panics.
    static CAUGHT_PARSER_PANICS: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
fn take_caught_parser_panics() -> usize {
    CAUGHT_PARSER_PANICS.with(|count| count.replace(0))
}

/// One sticky counter for actual work, including empty operations and byte copies.
pub(crate) struct Budget(Cell<Option<usize>>);
impl Budget {
    pub(crate) fn new(credits: usize) -> Self {
        Self(Cell::new(Some(credits)))
    }
    pub(crate) fn for_input(bytes: usize) -> Self {
        Self::new((64 * 1024 * 1024usize).saturating_add(bytes.saturating_mul(128)))
    }
    fn charge(&self, bytes: usize) -> bool {
        self.0
            .set(self.0.get().and_then(|n| n.checked_sub(bytes.saturating_add(32))));
        !self.exhausted()
    }
    fn exhausted(&self) -> bool {
        self.0.get().is_none()
    }
}

/// Parse BibTeX/BibLaTeX text. `source` identifies where the text came from
/// and becomes part of each record's identity.
#[cfg(test)]
fn load_str(source: &LibraryId, text: &str) -> ParsedLibrary {
    load_with_budget(source, text, Budget::for_input(text.len()), None)
}

pub(crate) fn load_with_budget(
    source: &LibraryId,
    text: &str,
    budget: Budget,
    previous: Option<&Snapshot>,
) -> ParsedLibrary {
    let mut result = ParsedLibrary::default();
    if !budget.charge(text.len()) {
        result.warnings.push(SourceWarning {
            code: WarningCode::WorkLimit,
            line: None,
            key: None,
            entry: None,
            cause: None,
            field: None,
        });
        result.cut_short = true;
        return result;
    }
    let (mut blocks, limited) = split_blocks(text);
    if limited {
        result.cut_short = has_unread_entry(text, &blocks);
        result.warnings.push(SourceWarning {
            code: WarningCode::WorkLimit,
            line: blocks.get(crate::MAX_LIBRARY_RECORDS).map(|block| block.line),
            key: None,
            entry: None,
            cause: None,
            field: None,
        });
        blocks.truncate(crate::MAX_LIBRARY_RECORDS);
    }

    let mut macros = Vec::new();
    let mut entries: Vec<RawEntry<'_>> = Vec::new();
    let mut raw_texts: Vec<&str> = Vec::new();
    let candidates = Candidates::collect(blocks, |block| {
        let claims = (!matches!(block.kind.as_str(), "comment" | "preamble" | "string"))
            .then(|| header_key(block.body))
            .flatten()
            .map(|key| (format!("record:{key}"), Some(block.line)))
            .into_iter()
            .collect();
        (claims, block)
    });
    let mut duplicates = Vec::new();
    let blocks = candidates.finish(&mut result.duplicate_keys, &mut duplicates);
    for block in &blocks {
        let warn = |code| SourceWarning::at(code, Some(block.line));
        let skip = |problem: Problem| warn(WarningCode::Syntax).because(problem.cause, problem.field);
        if !budget.charge(block.text.len().saturating_mul(4).saturating_add(2048)) {
            result.warnings.push(warn(WarningCode::WorkLimit));
            result.cut_short = true;
            result.warnings.extend(duplicates);
            return result;
        }
        match block.kind.as_str() {
            "comment" | "preamble" => continue,
            "string" => {
                match block.complete.then(|| parse_string(block)).flatten() {
                    Some(definition) => macros.push(definition),
                    None => result
                        .warnings
                        .push(skip(Problem::new(WarningCause::StringDefinition, None))),
                }
                continue;
            }
            _ => {}
        }
        if !block.complete {
            result.skip(
                header_key(block.body),
                skip(Problem::new(WarningCause::NotClosed, None)),
            );
            continue;
        }
        match parse_entry(block) {
            Err(problem) => result.skip(header_key(block.body), skip(problem)),
            Ok(mut entry) => {
                entry.macro_epoch = macros.len();
                entries.push(entry);
                raw_texts.push(block.text);
            }
        }
    }

    drop(blocks);
    let resolver = Resolver::new(macros, &entries, &budget);
    // `@xdata` entries only supply fields to other entries; they are not
    // references themselves and are never shown or exported.
    for (i, entry) in entries.iter().enumerate().filter(|(_, entry)| entry.kind != "xdata") {
        let outcome = panic_guard::attempt(|| load_entry(source, &resolver, i, entry, raw_texts[i]));
        #[cfg(test)]
        if outcome.is_err() {
            CAUGHT_PARSER_PANICS.with(|count| count.set(count.get() + 1));
        }
        let warn = |code, problem: &Problem| {
            SourceWarning::at(code, problem.line.or(Some(entry.line)))
                .because(problem.cause, problem.field)
                .for_entry(Some(&entry.key))
        };
        if let Ok(Ok((_, _, problems))) = &outcome {
            budget.charge(
                problems
                    .iter()
                    .map(Problem::size)
                    .sum::<usize>()
                    .saturating_add(problems.len().saturating_mul(entry.key.len())),
            );
        }
        if budget.exhausted() {
            result
                .warnings
                .push(SourceWarning::at(WarningCode::WorkLimit, Some(entry.line)));
            result.cut_short = true;
            break;
        }
        match outcome {
            Ok(Ok((record, file, problems))) => {
                result
                    .warnings
                    .extend(problems.iter().map(|p| warn(WarningCode::Field, p)));
                if let Some(file) = file {
                    result.attachments.push((record.id.clone(), file, Some(entry.line)));
                }
                result.push_record(record, previous);
            }
            Ok(Err(problem)) => result.skip(Some(&entry.key), warn(WarningCode::Syntax, &problem)),
            Err(_) => result.skip(
                Some(&entry.key),
                warn(WarningCode::Syntax, &Problem::new(WarningCause::Unreadable, None)),
            ),
        }
    }
    result.warnings.extend(duplicates);
    result
}

/// Resolve, parse and convert one entry.
fn load_entry(
    source: &LibraryId,
    resolver: &Resolver<'_, '_>,
    i: usize,
    entry: &RawEntry<'_>,
    raw: &str,
) -> Result<(Record, Option<String>, Vec<Problem>), Problem> {
    let resolved = resolver.resolve(i);
    if !resolver.budget.charge(
        raw.len()
            .saturating_add(source.as_str().len())
            .saturating_add(resolved.source.len().saturating_mul(8)),
    ) {
        return Err(Problem::new(WarningCause::TooLarge, None));
    }
    let mut problems = resolved.problems;
    let unreadable = || Problem::new(WarningCause::Unreadable, None);
    let bib = Bibliography::parse(&resolved.source).map_err(|_| unreadable())?;
    let parsed = bib.get(&entry.key).ok_or_else(unreadable)?;
    let mut record = convert::convert(source, &entry.kind, parsed, &resolved.fields, &mut problems);
    if crate::urls::remove_private_urls(&mut record.csl) {
        problems.push(Problem::new(WarningCause::Link, Some("url")));
    }
    let file = resolved.fields.get("file").map(|value| convert::literal_value(value));
    if !problems.is_empty() {
        record
            .warnings
            .push(ibis_core::contract::CitationWarningCode::SourceField);
    }
    Ok((record, file, problems))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // These properties live beside the private panic hook: integration tests
    // cannot distinguish a caught parser panic from an ordinary Syntax warning.
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        #[test]
        fn arbitrary_text_never_panics(text in any::<String>()) {
            take_caught_parser_panics();
            let _ = load_str(&LibraryId::digest(b"synthetic-property"), &text);
            prop_assert_eq!(take_caught_parser_panics(), 0);
        }
    }

    #[test]
    fn damaged_identity_claims_also_quarantine_reused_records() {
        use ibis_core::contract::LibraryObject;
        let library = LibraryId::digest(b"invented duplicate headers");
        let prefix = "@book{same,title={Invented first}}\n@book{other,title={Invented neighbor}}\n";
        let previous = Snapshot::new(
            load_str(&library, prefix)
                .records
                .into_iter()
                .map(LibraryObject::Record)
                .collect(),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        for contender in [
            "@book{same,title=}",
            "@book{same,title={Unclosed}",
            "@unknown{same,title={Invented second}}",
        ] {
            let text = format!("{prefix}{contender}");
            let parsed = load_with_budget(&library, &text, Budget::for_input(text.len()), Some(&previous));
            assert!(parsed.records.is_empty());
            assert_eq!(parsed.reused.len(), 1);
            assert_eq!(previous.objects()[parsed.reused[0]].object.id().key, "record:other");
            assert_eq!(
                parsed
                    .warnings
                    .iter()
                    .filter(|warning| warning.code == WarningCode::DuplicateKey)
                    .count(),
                2
            );
        }
    }

    #[test]
    fn damaged_and_duplicate_entries_keep_good_records() {
        let result = load_str(
            &LibraryId::digest(b"synthetic"),
            "@book{one,title={First}}\n@book{broken,title={Unclosed}\n@book{two,title={Second}}\n@book{one}",
        );
        assert_eq!(
            result
                .records
                .iter()
                .map(|record| record.id.key.as_str())
                .collect::<Vec<_>>(),
            ["record:two"]
        );
        assert_eq!(
            result.warnings.iter().map(|warning| warning.line).collect::<Vec<_>>(),
            [Some(2), Some(1), Some(4)]
        );
        // Both entries under the repeated key are named, the first included.
        assert_eq!(
            result
                .warnings
                .iter()
                .filter(|warning| warning.code == WarningCode::DuplicateKey)
                .map(|warning| (warning.line, warning.key.as_deref()))
                .collect::<Vec<_>>(),
            [(Some(1), Some("one")), (Some(4), Some("one"))]
        );
    }
}
