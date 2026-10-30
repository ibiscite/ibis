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
#[cfg(not(test))]
pub(crate) struct Budget(Cell<Option<usize>>);
// Unit tests retain an observer of this same counter after the real loader
// consumes its Budget. Production storage and charging are unchanged.
#[cfg(test)]
pub(crate) struct Budget(std::rc::Rc<Cell<Option<usize>>>);
impl Budget {
    #[cfg(not(test))]
    pub(crate) fn new(credits: usize) -> Self {
        Self(Cell::new(Some(credits)))
    }
    #[cfg(test)]
    pub(crate) fn new(credits: usize) -> Self {
        Self(std::rc::Rc::new(Cell::new(Some(credits))))
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

    // These are needed only while parsing and deduplicating entry headers.
    drop(blocks);
    drop(seen);
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
    use proptest::prelude::*;

    const FLAT_CASES: &[(&str, &str, &str)] = &[
        (
            "normal",
            "@article{synthetic,title={Synthetic signals},author={Researcher, Avery and Morgan, Ann},year={2024},journal={Invented Letters},pages={1--20},doi={10.1000/synthetic},volume={3},number={2}}",
            "synthetic",
        ),
        (
            "unicode-whitespace-hyphens",
            "@WWW{ключ,title={ \tA\u{a0}B\n },note={- -- --- ---- ----- ------ -------},author={García, Ana and others},year={2024}}",
            "ключ",
        ),
        ("empty", "@misc{empty,title={},author={},year={}}", "empty"),
        (
            "invalid-getters",
            "@article{Ω,title={Invented},editor={Morgan, Ann},editortype={unknown},year={bad},month={13},day={0},pages={x-y}}",
            "Ω",
        ),
        (
            "case-collisions",
            "@conference{key,TITLE={Earlier},title={Later},İD={A},id={B}}",
            "key",
        ),
        ("encoded", "@book{k,title={\u{e000}\u{e001}\u{e002}\u{e003}}}", "k"),
        ("protected", "@book{k,title={The {NASA} Example}}", "k"),
        ("tex", r"@book{k,title={Garc{\'\i}a \unknown{kept}}}", "k"),
        ("math", "@book{k,title={$x+y$}}", "k"),
        ("literal-url", "@book{k,url={https://example.invalid/$literal}}", "k"),
        ("bare-month", "@book{k,year={2024},month=mar}", "k"),
        ("crossref", "@book{k,title={Child},crossref={missing}}", "k"),
        ("xdata-link", "@book{k,title={Child},xdata={missing}}", "k"),
        ("ids", "@book{k,title={Alias},ids={other}}", "k"),
        ("xdata-helper", "@XDATA{helper,title={Hidden}}", "helper"),
        ("wrong-key", "@book{k,title={Value}}", "other"),
        ("multiple", "@book{a,title={First}}@book{b,title={Second}}", "a"),
        ("abbreviation", "@string{x={Label}}@book{k,title=x}", "k"),
        ("syntax", "@book{k,title={Unterminated}", "k"),
    ];

    #[test]
    fn flat_entry_canonical_ast_and_getter_oracle() {
        use ibis_core::contract::Fingerprint;
        use serde_json::json;

        // Freeze the upstream result before introducing a constructed-entry
        // path. Entry's Debug covers every field, chunk variant and exact span.
        let observations: Vec<_> = FLAT_CASES
            .iter()
            .map(|(name, text, key)| match Bibliography::parse(text) {
                Ok(bib) => match bib.get(key) {
                    Some(entry) => json!({
                        "name": name,
                        "entry": format!("{entry:?}"),
                        "authors": format!("{:?}", entry.author()),
                        "editors": format!("{:?}", entry.editors()),
                        "date": format!("{:?}", entry.date()),
                        "date_error": entry.date().err().map(|error| error.to_string()),
                        "pages": format!("{:?}", entry.pages()),
                    }),
                    None => json!({ "name": name, "missing": true }),
                },
                Err(error) => json!({ "name": name, "error": format!("{error:?}") }),
            })
            .collect();
        let fingerprint =
            Fingerprint::of_bytes(&serde_json::to_vec(&observations).unwrap_or_else(|error| panic!("{error}")));
        assert_eq!(
            fingerprint.as_str(),
            "04011c1d8f3879f5ed1ab53a9af1130cbf5895e01f6474aba4f9f716e5ce2a6a"
        );
    }

    const FLAT_TRANSCRIPT: &str = concat!(
        "@string{label={Early}}\n",
        "@article{plain,title={After},author={Researcher, Avery and Morgan, Ann and others},year={2024},journal={Invented Letters},pages={1--20},doi={10.1000/synthetic}}\n",
        "@book{empty,title={},year={}}\n",
        "@book{unicode,title={ A\u{a0}B },note={---- ----- ------},year={bad},month={13}}\n",
        "@book{encoded,title={\u{e000}\u{e001}\u{e002}\u{e003}},url={https://127.0.0.1/synthetic},file={synthetic-paper.pdf}}\n",
        "@book{tex,title={Garc{\\'\\i}a \\unknown{kept}},date={2024-13-01}}\n",
        "@xdata{shared,publisher=label}\n",
        "@string{label={Late}}\n",
        "@book{child,title=label,xdata={shared},crossref={plain},month=mar}\n",
        "@book{broken,title={Unclosed\n@book{last,title={Still Visible}}\n",
    );

    #[test]
    fn flat_entry_loader_credit_and_fingerprint_oracle() {
        use ibis_core::contract::{Fingerprint, LibraryObject};
        use serde_json::json;

        let source = LibraryId::file(b"synthetic-flat-entry.bib");
        let old = load_str(&source, &FLAT_TRANSCRIPT.replace("After", "Before"));
        let previous = Snapshot::new(old.records.into_iter().map(LibraryObject::Record).collect())
            .unwrap_or_else(|error| panic!("{error}"));
        let mut observations = Vec::new();
        for reuse in [false, true] {
            for credits in (0..=65_536).step_by(257).chain([usize::MAX]) {
                let budget = Budget::new(credits);
                let remaining = std::rc::Rc::clone(&budget.0);
                take_caught_parser_panics();
                // Observe the real loader's final counter, not a second model of
                // its post-conversion problem charges or publication decisions.
                let (loaded, limited) = load_with_budget(&source, FLAT_TRANSCRIPT, budget, reuse.then_some(&previous));
                assert_eq!(take_caught_parser_panics(), 0);
                let records = serde_json::to_value(&loaded.records).unwrap_or_else(|error| panic!("{error}"));
                let snapshot = Snapshot::new_reusing(
                    loaded.records.into_iter().map(LibraryObject::Record).collect(),
                    &previous,
                    &loaded.reused,
                )
                .unwrap_or_else(|error| panic!("{error}"));
                observations.push(json!({
                    "reuse": reuse,
                    "credits": credits,
                    "remaining": remaining.get(),
                    "records": records,
                    "reused": loaded.reused,
                    "warnings": loaded.warnings,
                    "attachments": loaded.attachments,
                    "fingerprints": snapshot.fingerprints(),
                    "limited": limited,
                }));
            }
        }
        let fingerprint =
            Fingerprint::of_bytes(&serde_json::to_vec(&observations).unwrap_or_else(|error| panic!("{error}")));
        assert_eq!(
            fingerprint.as_str(),
            "10bd40de402f0f64e19653136febb3fbcf703559c2998dca76d4aedfa8e3917c"
        );
    }

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
