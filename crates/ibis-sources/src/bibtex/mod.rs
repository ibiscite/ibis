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
use ibis_core::panic_guard;
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
    /// Reads the parallel path finished, so tests know they reached it.
    static PARALLEL_READS: Cell<usize> = const { Cell::new(0) };
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
    load(source, text, budget, previous, PARALLEL_FROM)
}

fn load(
    source: &LibraryId,
    text: &str,
    budget: Budget,
    previous: Option<&Snapshot>,
    parallel_from: usize,
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
    // Resolution is sequential and charges the budget; parsing and converting
    // a resolved entry charge nothing. Charges only add up, so when the budget
    // lasts the whole file the order of the two cannot change the result. If
    // it runs out, only the one-entry-at-a-time order says where it stops:
    // start again from the same budget with a fresh resolver.
    let start = budget.0.get();
    if entries.len() >= parallel_from {
        let resolver = Resolver::new(macros.clone(), &entries, &budget);
        let mut read = ParsedLibrary::default();
        if read_in_parallel(source, &resolver, &entries, &raw_texts, previous, &mut read) {
            #[cfg(test)]
            PARALLEL_READS.with(|count| count.set(count.get() + 1));
            return finish(result, read, duplicates);
        }
        budget.0.set(start);
    }
    let resolver = Resolver::new(macros, &entries, &budget);
    let mut read = ParsedLibrary::default();
    let outcomes = references(&entries).map(|i| {
        let outcome = panic_guard::attempt(|| {
            resolve_entry(source, &resolver, i, raw_texts[i])
                .and_then(|resolved| convert_entry(source, &entries[i], resolved))
        });
        (i, outcome)
    });
    read_entries(&mut read, &entries, resolver.budget, previous, outcomes);
    finish(result, read, duplicates)
}

/// Libraries this size and larger convert on several threads.
const PARALLEL_FROM: usize = if cfg!(test) { 2 } else { 512 };
/// Entries resolved before each parallel step: bounds what is held at once.
const WINDOW: usize = if cfg!(test) { 3 } else { 4096 };
/// Loads are safe on a 2 MiB stack (tests/bibtex_damage.rs); workers get more.
const WORKER_STACK: usize = 4 * 1024 * 1024;

type Loaded = (Record, Option<String>, Vec<Problem>);
type Outcome = std::thread::Result<Result<Loaded, Problem>>;

/// `@xdata` entries only supply fields to other entries; they are not
/// references themselves and are never shown or exported.
fn references<'e>(entries: &'e [RawEntry<'_>]) -> impl Iterator<Item = usize> + 'e {
    (0..entries.len()).filter(|&i| entries[i].kind != "xdata")
}

/// Whole windows: resolve in order, convert in parallel, then record in
/// order. False when the budget ran out anywhere.
fn read_in_parallel(
    source: &LibraryId,
    resolver: &Resolver<'_, '_>,
    entries: &[RawEntry<'_>],
    raw_texts: &[&str],
    previous: Option<&Snapshot>,
    read: &mut ParsedLibrary,
) -> bool {
    let workers = std::thread::available_parallelism().map_or(1, usize::from).clamp(1, 8);
    let indices: Vec<usize> = references(entries).collect();
    for window in indices.chunks(WINDOW) {
        let resolved: Vec<_> = window
            .iter()
            .map(|&i| panic_guard::attempt(|| resolve_entry(source, resolver, i, raw_texts[i])))
            .collect();
        if resolver.budget.exhausted() {
            return false;
        }
        let size = window.len().div_ceil(workers);
        let mut work = resolved.into_iter().zip(window.iter().copied());
        let batches: Vec<Vec<_>> = std::iter::from_fn(|| Some(work.by_ref().take(size).collect::<Vec<_>>()))
            .take_while(|batch| !batch.is_empty())
            .collect();
        let converted: Option<Vec<Vec<(usize, Outcome)>>> = std::thread::scope(|scope| {
            let handles: Vec<_> = batches
                .into_iter()
                .map(|batch| {
                    std::thread::Builder::new()
                        .stack_size(WORKER_STACK)
                        .spawn_scoped(scope, move || {
                            batch
                                .into_iter()
                                .map(|(resolved, i)| {
                                    let outcome = match resolved {
                                        Ok(Ok(resolved)) => {
                                            panic_guard::attempt(|| convert_entry(source, &entries[i], resolved))
                                        }
                                        Ok(Err(problem)) => Ok(Err(problem)),
                                        Err(panic) => Err(panic),
                                    };
                                    (i, outcome)
                                })
                                .collect::<Vec<_>>()
                        })
                })
                .collect();
            // A thread the system would not start, or one that failed outside
            // the guard, leaves the file to the sequential read.
            handles.into_iter().map(|handle| handle.ok()?.join().ok()).collect()
        });
        let Some(converted) = converted else { return false };
        let outcomes = converted.into_iter().flatten();
        if !read_entries(read, entries, resolver.budget, previous, outcomes) {
            return false;
        }
    }
    !resolver.budget.exhausted()
}

/// Record each outcome in order, charging its problems. False when the
/// budget ran out, after the warning that says so.
fn read_entries(
    read: &mut ParsedLibrary,
    entries: &[RawEntry<'_>],
    budget: &Budget,
    previous: Option<&Snapshot>,
    outcomes: impl Iterator<Item = (usize, Outcome)>,
) -> bool {
    for (i, outcome) in outcomes {
        let entry = &entries[i];
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
            read.warnings
                .push(SourceWarning::at(WarningCode::WorkLimit, Some(entry.line)));
            read.cut_short = true;
            return false;
        }
        match outcome {
            Ok(Ok((record, file, problems))) => {
                read.warnings
                    .extend(problems.iter().map(|p| warn(WarningCode::Field, p)));
                if let Some(file) = file {
                    read.attachments.push((record.id.clone(), file, Some(entry.line)));
                }
                read.push_record(record, previous);
            }
            Ok(Err(problem)) => read.skip(Some(&entry.key), warn(WarningCode::Syntax, &problem)),
            Err(_) => read.skip(
                Some(&entry.key),
                warn(WarningCode::Syntax, &Problem::new(WarningCause::Unreadable, None)),
            ),
        }
    }
    true
}

/// The entries read after the file's own warnings, as one loop would leave them.
fn finish(mut result: ParsedLibrary, read: ParsedLibrary, duplicates: Vec<SourceWarning>) -> ParsedLibrary {
    result.records.extend(read.records);
    result.reused.extend(read.reused);
    result.warnings.extend(read.warnings);
    result.attachments.extend(read.attachments);
    result.cut_short |= read.cut_short;
    for (key, warning) in read.damaged_keys {
        result.damaged_keys.entry(key).or_insert(warning);
    }
    result.warnings.extend(duplicates);
    result
}

/// Resolve one entry and charge for parsing it.
fn resolve_entry(
    source: &LibraryId,
    resolver: &Resolver<'_, '_>,
    i: usize,
    raw: &str,
) -> Result<resolve::Resolved, Problem> {
    let resolved = resolver.resolve(i);
    if !resolver.budget.charge(
        raw.len()
            .saturating_add(source.as_str().len())
            .saturating_add(resolved.source.len().saturating_mul(8)),
    ) {
        return Err(Problem::new(WarningCause::TooLarge, None));
    }
    Ok(resolved)
}

/// Parse and convert one resolved entry; charges nothing.
fn convert_entry(source: &LibraryId, entry: &RawEntry<'_>, resolved: resolve::Resolved) -> Result<Loaded, Problem> {
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

        #[test]
        fn arbitrary_text_reads_the_same_in_parallel(text in any::<String>()) {
            let library = LibraryId::digest(b"synthetic-property");
            let budget = || Budget::for_input(text.len());
            prop_assert_eq!(
                fingerprint(load(&library, &text, budget(), None, 0)),
                fingerprint(load(&library, &text, budget(), None, usize::MAX))
            );
        }
    }

    /// Everything a load leaves, comparable across the two read orders.
    fn fingerprint(read: ParsedLibrary) -> String {
        format!(
            "{:?}|{:?}|{:?}|{:?}|{}|{:?}|{:?}",
            read.records,
            read.reused,
            read.warnings,
            read.attachments,
            read.cut_short,
            read.duplicate_keys,
            read.damaged_keys
        )
    }

    #[test]
    fn reading_in_parallel_matches_one_entry_at_a_time_at_every_budget() {
        // Crossrefs, xdata, string macros, a duplicate key, damaged and
        // unclosed entries, across several windows.
        let mut text =
            String::from("@string{press={Imaginary Press}}\n@xdata{shared,publisher=press,location={Nowhere}}\n");
        for index in 0..14 {
            text.push_str(&match index % 7 {
                0 => format!("@book{{parent{index},title={{Invented volume {index}}},author={{Avery, Taylor}},year={{2026}},xdata={{shared}}}}\n"),
                1 => format!("@inbook{{child{index},crossref={{parent{}}},title={{Invented chapter}},pages={{1--9}}}}\n", index - 1),
                2 => "@article{same,title={Invented first},year=2001}\n@article{same,title={Invented second}}\n".into(),
                3 => format!("@article{{broken{index},title=}}\n"),
                4 => format!("@article{{open{index},title={{Unclosed}}\n"),
                5 => format!("@misc{{plain{index},title={{Synthetic {{\\emph{{signals}}}}}},note={{press # \" edition\"}},file={{a.pdf}}}}\n"),
                _ => format!("@unknownkind{{odd{index},title={{Odd}},url={{https://example.invalid/?token=x}}}}\n"),
            });
        }
        let library = LibraryId::digest(b"invented parallel equivalence");
        let limited = |credits| {
            let read = load(&library, &text, Budget::new(credits), None, usize::MAX);
            read.cut_short
                || read
                    .warnings
                    .iter()
                    .any(|warning| warning.code == WarningCode::WorkLimit)
        };
        // The fewest credits a whole read needs; then stop it at every point.
        let (mut low, mut high) = (0, Budget::for_input(text.len()).0.get().unwrap());
        assert!(!limited(high));
        while low < high {
            let middle = low + (high - low) / 2;
            if limited(middle) {
                low = middle + 1
            } else {
                high = middle
            }
        }
        let used = low;
        let mut checked = 0;
        PARALLEL_READS.with(|count| count.set(0));
        for credits in (0..=used + 64).step_by((used / 400).max(1)) {
            let sequential = fingerprint(load(&library, &text, Budget::new(credits), None, usize::MAX));
            let parallel = fingerprint(load(&library, &text, Budget::new(credits), None, 0));
            assert_eq!(parallel, sequential, "budget {credits} of {used}");
            checked += 1;
        }
        assert!(checked > 100);
        // Every budget that lasts the whole file stayed on the parallel path.
        assert!(PARALLEL_READS.with(Cell::get) >= 64 / (used / 400).max(1));
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
