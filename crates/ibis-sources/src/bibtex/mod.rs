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
mod lexer;
mod resolve;

use std::cell::Cell;
use std::collections::HashSet;

use biblatex::Bibliography;

use crate::{ParsedLibrary, SourceWarning, WarningCode};
use ibis_core::contract::{LibraryId, Record, Snapshot};
use ibis_core::panic_guard;
use lexer::{RawEntry, parse_entry, parse_string, split_blocks};
use resolve::Resolver;

const LOAD_LIMIT: &str = "load work limit reached; this entry and remaining entries were skipped";

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
    load_with_budget(source, text, Budget::for_input(text.len()), None).0
}

pub(crate) fn load_with_budget(
    source: &LibraryId,
    text: &str,
    budget: Budget,
    previous: Option<&Snapshot>,
) -> (ParsedLibrary, bool) {
    let mut result = ParsedLibrary::default();
    if !budget.charge(text.len()) {
        result.warnings.push(SourceWarning {
            code: WarningCode::WorkLimit,
            line: None,
        });
        return (result, true);
    }
    let (mut blocks, limited) = split_blocks(text);
    if limited {
        result.warnings.push(SourceWarning {
            code: WarningCode::WorkLimit,
            line: blocks.get(crate::MAX_LIBRARY_RECORDS).map(|block| block.line),
        });
        blocks.truncate(crate::MAX_LIBRARY_RECORDS);
    }

    let mut macros = Vec::new();
    let mut entries: Vec<RawEntry<'_>> = Vec::new();
    let mut raw_texts: Vec<&str> = Vec::new();
    let mut seen = HashSet::new();
    let mut duplicates = HashSet::new();
    for block in &blocks {
        let warn = |code, _key: Option<String>, _message: String| SourceWarning {
            code,
            line: Some(block.line),
        };
        if !budget.charge(block.text.len().saturating_mul(4).saturating_add(2048)) {
            result
                .warnings
                .push(warn(WarningCode::WorkLimit, None, LOAD_LIMIT.into()));
            return (result, true);
        }
        match block.kind.as_str() {
            "comment" | "preamble" => continue,
            "string" => {
                match block.complete.then(|| parse_string(block)).flatten() {
                    Some(definition) => macros.push(definition),
                    None => result.warnings.push(warn(
                        WarningCode::Syntax,
                        None,
                        "@string definition could not be read; skipped".to_string(),
                    )),
                }
                continue;
            }
            _ => {}
        }
        if !block.complete {
            let key = parse_entry(block).ok().map(|e| e.key);
            result.warnings.push(warn(
                WarningCode::Syntax,
                key,
                "entry is not closed; skipped".to_string(),
            ));
            continue;
        }
        match parse_entry(block) {
            Err(message) => result
                .warnings
                .push(warn(WarningCode::Syntax, None, format!("{message}; skipped"))),
            Ok(entry) if !seen.insert(entry.key.clone()) => {
                duplicates.insert(entry.key.clone());
                let message = format!("duplicate key `{}`; all entries with this key are skipped", entry.key);
                result
                    .warnings
                    .push(warn(WarningCode::DuplicateKey, Some(entry.key), message));
            }
            Ok(mut entry) => {
                entry.macro_epoch = macros.len();
                entries.push(entry);
                raw_texts.push(block.text);
            }
        }
    }

    // These are needed only while parsing and deduplicating entry headers.
    drop(blocks);
    drop(seen);
    // Neither a citation nor inheritance can choose between repeated keys.
    // Remove every contender before building the resolver's ownership map.
    let (entries, raw_texts): (Vec<_>, Vec<_>) = entries
        .into_iter()
        .zip(raw_texts)
        .filter(|(entry, _)| !duplicates.contains(&entry.key))
        .unzip();
    let resolver = Resolver::new(macros, &entries, &budget);
    // `@xdata` entries only supply fields to other entries; they are not
    // references themselves and are never shown or exported.
    for (i, entry) in entries.iter().enumerate().filter(|(_, entry)| entry.kind != "xdata") {
        let outcome = panic_guard::attempt(|| load_entry(source, &resolver, i, entry, raw_texts[i]));
        #[cfg(test)]
        if outcome.is_err() {
            CAUGHT_PARSER_PANICS.with(|count| count.set(count.get() + 1));
        }
        let warn = |code, _message: &str| SourceWarning {
            code,
            line: Some(entry.line),
        };
        if let Ok(Ok((_, _, problems))) = &outcome {
            budget.charge(
                problems
                    .iter()
                    .map(String::len)
                    .sum::<usize>()
                    .saturating_add(problems.len().saturating_mul(entry.key.len())),
            );
        }
        if budget.exhausted() {
            result.warnings.push(warn(WarningCode::WorkLimit, LOAD_LIMIT));
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
            Ok(Err(message)) => result.warnings.push(warn(WarningCode::Syntax, &message)),
            Err(_) => result.warnings.push(warn(
                WarningCode::Syntax,
                "entry could not be read safely (parser error); skipped",
            )),
        }
    }
    (result, limited || budget.exhausted())
}

/// Whether a recognized block is demonstrably unfinished, for reload safety.
pub(crate) fn has_incomplete_blocks(text: &str) -> bool {
    split_blocks(text).0.iter().any(|block| !block.complete)
}

/// Resolve, parse and convert one entry.
fn load_entry(
    source: &LibraryId,
    resolver: &Resolver<'_, '_>,
    i: usize,
    entry: &RawEntry<'_>,
    raw: &str,
) -> Result<(Record, Option<String>, Vec<String>), String> {
    let resolved = resolver.resolve(i);
    if !resolver.budget.charge(
        raw.len()
            .saturating_add(source.as_str().len())
            .saturating_add(resolved.source.len().saturating_mul(8)),
    ) {
        return Err(LOAD_LIMIT.into());
    }
    let mut problems = resolved.problems;
    let bib = Bibliography::parse(&resolved.source).map_err(|e| format!("{}; skipped", e.kind))?;
    let parsed = bib
        .get(&entry.key)
        .ok_or_else(|| "entry could not be read; skipped".to_string())?;
    let mut record = convert::convert(source, &entry.kind, parsed, &resolved.fields, &mut problems);
    if crate::urls::remove_private_urls(&mut record.csl) {
        problems.push("private URL omitted".into());
    }
    let file = resolved.fields.get("file").map(|value| convert::file_value(value));
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
            let _ = load_str(&LibraryId::file(b"synthetic-property"), &text);
            prop_assert_eq!(take_caught_parser_panics(), 0);
        }

        #[test]
        fn mutated_entries_never_panic(
            prefix in "\\PC{0,40}",
            field in prop::sample::select(vec!["title", "crossref", "xdata", "author", "editor", "date", "year", "month", "journal", "pages", "langid"]),
            value in "\\PC{0,40}",
            closer in prop::sample::select(vec!["}", ")", "", "}}", "{"]),
        ) {
            let text = format!("{prefix}@article{{k, {field} = {{{value}}}{closer}\n@book(b, title={{{value}}})");
            take_caught_parser_panics();
            let _ = load_str(&LibraryId::file(b"synthetic-property"), &text);
            prop_assert_eq!(take_caught_parser_panics(), 0);
        }
    }

    #[test]
    fn reused_records_keep_macro_epochs_and_parent_reads() {
        let source = LibraryId::file(b"synthetic field ownership");
        let macros = load_str(
            &source,
            "@string{label={First}}\n@book{a,title=label}\n@string{label={Second}}\n@book{b,title=label}",
        );
        assert_eq!(macros.records[0].csl["title"], "First");
        assert_eq!(macros.records[1].csl["title"], "Second");
        for text in [
            "@book{parent,title={Inherited},publisher={Press}}\n@inbook{child,crossref={parent}}\n@inbook{later,crossref={parent}}",
            "@inbook{child,crossref={parent}}\n@book{parent,title={Inherited},publisher={Press}}\n@inbook{later,crossref={parent}}",
            "@xdata{parent,publisher={Press}}\n@book{child,xdata={parent}}\n@book{later,xdata={parent}}",
        ] {
            let loaded = load_str(&source, text);
            for record in loaded.records.iter().filter(|record| record.id.key != "record:parent") {
                assert_eq!(record.csl["publisher"], "Press");
            }
            assert!(loaded.records.iter().any(|record| record.id.key == "record:later"));
            let previous = Snapshot::new(
                loaded
                    .records
                    .into_iter()
                    .map(ibis_core::contract::LibraryObject::Record)
                    .collect(),
            )
            .unwrap_or_else(|error| panic!("{error}"));
            let repeated = load_with_budget(&source, text, Budget::for_input(text.len()), Some(&previous)).0;
            assert!(repeated.records.is_empty());
            assert_eq!(repeated.reused.len(), previous.objects().len());
        }
    }
    #[test]
    fn refresh_retains_only_changed_csl_payloads() {
        use ibis_core::contract::LibraryObject;
        let source = LibraryId::file(b"synthetic candidate reuse");
        let old = load_str(
            &source,
            "@book{a,title={Keep}}\n@book{b,title={Before}}\n@book{c,title={Deleted}}",
        );
        let previous = Snapshot::new(old.records.into_iter().map(LibraryObject::Record).collect())
            .unwrap_or_else(|error| panic!("{error}"));
        let text = "@book{a,title={Keep}}\n@book{b,title={After}}\n@book{d,title={Added}}";
        let (candidate, limited) = load_with_budget(&source, text, Budget::for_input(text.len()), Some(&previous));
        assert!(!limited);
        assert_eq!(candidate.records.len(), 2);
        assert_eq!(candidate.reused.len(), 1);
        let next = Snapshot::new_reusing(
            candidate.records.into_iter().map(LibraryObject::Record).collect(),
            &previous,
            &candidate.reused,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let fresh = Snapshot::new(
            load_str(&source, text)
                .records
                .into_iter()
                .map(LibraryObject::Record)
                .collect(),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(next, fresh);
    }
    #[test]
    fn recovery_header_limits_are_reported_even_when_candidates_collapse() {
        let text = format!(
            "@book{{broken,title={{ {}\n@book{{good}}",
            "@misc{nested}".repeat(crate::MAX_LIBRARY_RECORDS + 1)
        );
        let result = load_str(&LibraryId::file(b"synthetic"), &text);
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.code == WarningCode::WorkLimit)
        );
    }
    #[test]
    fn minimal_entries_pay_fixed_allocation_work() {
        let (result, limited) = load_with_budget(&LibraryId::file(b"synthetic"), "@misc{one}", Budget::new(1024), None);
        assert!(limited);
        assert!(result.records.is_empty());
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.code == WarningCode::WorkLimit)
        );
    }
    #[test]
    fn damaged_and_duplicate_entries_keep_good_records() {
        let result = load_str(
            &LibraryId::file(b"synthetic"),
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
            [Some(2), Some(4)]
        );
    }
    proptest::proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(64))]
        #[test]
        fn arbitrary_damaged_text_keeps_completed_prefix(bytes in proptest::collection::vec(proptest::arbitrary::any::<u8>(),0..4096)) {
            // The pure parser never resolves file locations from generated data.
            let text=format!("@book{{first,title={{Invented}}}}\n{}\n@book{{last,title={{Last}}}}",String::from_utf8_lossy(&bytes));
            let result=load_str(&LibraryId::file(b"synthetic-property"),&text);
            proptest::prop_assert!(result.records.iter().any(|record|record.citation_key()==Ok(Some("first"))));
        }
    }
}
