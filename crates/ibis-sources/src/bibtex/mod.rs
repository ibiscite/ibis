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
use ibis_core::contract::{LibraryId, Record};
use ibis_core::panic_guard;
use lexer::{RawEntry, parse_entry, parse_string, split_blocks};
use resolve::Resolver;

const LOAD_LIMIT: &str = "load work limit reached; this entry and remaining entries were skipped";

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
    load_with_budget(source, text, Budget::for_input(text.len())).0
}

pub(crate) fn load_with_budget(source: &LibraryId, text: &str, budget: Budget) -> (ParsedLibrary, bool) {
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
    let mut entries: Vec<RawEntry> = Vec::new();
    let mut raw_texts: Vec<&str> = Vec::new();
    let mut seen = HashSet::new();
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
                let message = format!("duplicate key `{}`; the first entry with this key is used", entry.key);
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

    let resolver = Resolver::new(macros, &entries, &budget);
    // `@xdata` entries only supply fields to other entries; they are not
    // references themselves and are never shown or exported.
    for (i, entry) in entries.iter().enumerate().filter(|(_, e)| e.kind != "xdata") {
        let outcome = panic_guard::attempt(|| load_entry(source, &resolver, i, entry, raw_texts[i]));
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
                result.records.push(record);
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
    resolver: &Resolver,
    i: usize,
    entry: &RawEntry,
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
        let (result, limited) = load_with_budget(&LibraryId::file(b"synthetic"), "@misc{one}", Budget::new(1024));
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
            ["record:one", "record:two"]
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
