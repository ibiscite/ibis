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
//! 3. [`convert`] turns each resolved entry into a [`Reference`], keeping
//!    protected capitalization, italics, truncated author lists, date ranges
//!    and language tags.
//!
//! Good entries always load; every problem becomes a [`LoadWarning`] with a
//! line number. A failure inside the third-party field parser is contained to
//! the entry that caused it.

mod convert;
mod lexer;
mod resolve;

use std::collections::HashSet;
use std::panic::AssertUnwindSafe;

use biblatex::Bibliography;

pub use convert::csl_type;

use crate::model::{LoadResult, LoadWarning, Reference, WarningKind};
use lexer::{RawEntry, parse_entry, parse_string, split_blocks};
use resolve::Resolver;

/// Parse BibTeX/BibLaTeX text. `source` identifies where the text came from
/// and becomes part of each record's identity.
pub fn load_str(source: &str, text: &str) -> LoadResult {
    let mut result = LoadResult::default();
    let blocks = split_blocks(text);

    let mut macros = Vec::new();
    let mut entries: Vec<RawEntry> = Vec::new();
    let mut raw_texts: Vec<&str> = Vec::new();
    let mut seen = HashSet::new();
    for block in &blocks {
        let warn = |kind, key: Option<String>, message: String| LoadWarning {
            kind,
            line: Some(block.line),
            key,
            message,
        };
        match block.kind.as_str() {
            "comment" | "preamble" => continue,
            "string" => {
                match block.complete.then(|| parse_string(block)).flatten() {
                    Some(definition) => macros.push(definition),
                    None => result.warnings.push(warn(
                        WarningKind::Syntax,
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
                WarningKind::Syntax,
                key,
                "entry is not closed; skipped".to_string(),
            ));
            continue;
        }
        match parse_entry(block) {
            Err(message) => result
                .warnings
                .push(warn(WarningKind::Syntax, None, format!("{message}; skipped"))),
            Ok(entry) if !seen.insert(entry.key.clone()) => {
                let message = format!("duplicate key `{}`; the first entry with this key is used", entry.key);
                result
                    .warnings
                    .push(warn(WarningKind::DuplicateKey, Some(entry.key), message));
            }
            Ok(entry) => {
                entries.push(entry);
                raw_texts.push(block.text);
            }
        }
    }

    let resolver = Resolver::new(macros, &entries);
    // `@xdata` entries only supply fields to other entries; they are not
    // references themselves and are never shown or exported.
    for (i, entry) in entries.iter().enumerate().filter(|(_, e)| e.kind != "xdata") {
        let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
            load_entry(source, &resolver, i, entry, raw_texts[i])
        }));
        let warn = |kind, message: &str| LoadWarning {
            kind,
            line: Some(entry.line),
            key: Some(entry.key.clone()),
            message: message.to_string(),
        };
        match outcome {
            Ok(Ok((reference, problems))) => {
                result
                    .warnings
                    .extend(problems.iter().map(|p| warn(WarningKind::Field, p)));
                result.references.push(reference);
            }
            Ok(Err(message)) => result.warnings.push(warn(WarningKind::Syntax, &message)),
            Err(_) => result.warnings.push(warn(
                WarningKind::Syntax,
                "entry could not be read safely (parser error); skipped",
            )),
        }
    }
    result
}

/// Whether a recognized block is demonstrably unfinished, for reload safety.
pub(crate) fn has_incomplete_blocks(text: &str) -> bool {
    split_blocks(text).iter().any(|block| !block.complete)
}

/// Resolve, parse and convert one entry.
fn load_entry(
    source: &str,
    resolver: &Resolver,
    i: usize,
    entry: &RawEntry,
    raw: &str,
) -> Result<(Reference, Vec<String>), String> {
    let resolved = resolver.resolve(i);
    let mut problems = resolved.problems;
    let bib = Bibliography::parse(&resolved.source).map_err(|e| format!("{}; skipped", e.kind))?;
    let parsed = bib
        .get(&entry.key)
        .ok_or_else(|| "entry could not be read; skipped".to_string())?;
    let reference = convert::convert(source, &entry.kind, raw, parsed, &resolved.fields, &mut problems);
    Ok((reference, problems))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_broken_entry_does_not_hide_the_others() {
        let src = "@article{good1, title={One}}\n@article{bad, title={Unclosed}\n@article{good2, title={Two}}\n";
        let result = load_str("test.bib", src);
        let keys: Vec<_> = result.references.iter().map(|r| r.id.native_key.as_str()).collect();
        assert_eq!(keys, ["good1", "good2"]);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].line, Some(2));
    }

    #[test]
    fn duplicate_keys_keep_the_first_and_warn() {
        let result = load_str("t", "@article{k, title={First}}\n@book{k, title={Second}}\n");
        assert_eq!(result.references.len(), 1);
        assert_eq!(result.references[0].title.as_deref(), Some("First"));
        assert_eq!(result.warnings[0].kind, WarningKind::DuplicateKey);
    }

    #[test]
    fn cross_reference_cycles_are_reported_not_fatal() {
        for src in [
            "@book{a,title={A},crossref={a}}",
            "@book{a,title={A},crossref={b}}\n@book{b,title={B},crossref={a}}",
            "@book{a,title={A},xdata={b}}\n@book{b,title={B},xdata={c}}\n@book{c,title={C},xdata={a}}",
        ] {
            let result = load_str("t", src);
            assert!(!result.references.is_empty(), "{src}");
            assert!(
                result.warnings.iter().any(|w| w.message.contains("cycle")),
                "{src}: {:?}",
                result.warnings
            );
        }
    }
}
