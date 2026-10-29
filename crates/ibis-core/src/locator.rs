//! Reading a locator a person typed, for all 29 CSL locator types: at the end of
//! a search (`smith 2020 p 45`, `garcia, 12-14`) or on its own (`chap. 3`).
//!
//! Only these forms are read, so a plain search is never changed by guesswork:
//! - a label and a value: `p 45`, `p. 45`, `chap. 3`, `§ 2`, `s.v. ratio`;
//! - a full label word joined to its value, as Zotero accepts: `page123`,
//!   `chapter3`, `chap12` (short labels never join: `p53` is a protein);
//! - a label and quoted text, for textual locators: `note "the epigraph"`;
//! - plain page numbers after a comma: `garcia, 12-14`, never a four-digit
//!   number, which is most likely a year.
//!
//! Abbreviations that are also ordinary words (`no`, `art`, `para`, `n`, `op`)
//! count only with a period. The caller shows what was read (`typed`) so the
//! person can undo it.

use crate::contract::{Locator, LocatorTerm};

/// A search with a locator split off its end.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrailingLocator {
    /// What remains to search for.
    pub search: String,
    pub locator: Locator,
    /// The text read as the locator, exactly as typed.
    pub typed: String,
}

/// Longer text is searched as written; no locator is that long.
pub const MAX_LOCATOR_LEN: usize = 64;

/// What a label accepts as its value.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Value {
    /// Numbers, ranges and Roman numerals, in lists.
    Numbers,
    /// Also single letters, as in `appendix B`.
    Lettered,
    /// Clock times, as in `timestamp 1:02:03-1:05`.
    Time,
    /// Any short text, as in `s.v. ratio`.
    Text,
}

struct Label {
    term: LocatorTerm,
    /// Whole words, singular and plural; these may join their value.
    words: &'static [&'static str],
    /// Abbreviations, read with or without a period.
    short: &'static [&'static str],
    /// Abbreviations that are also ordinary words, read only with a period.
    dotted: &'static [&'static str],
    value: Value,
}

const fn label(term: LocatorTerm, words: &'static [&'static str], short: &'static [&'static str]) -> Label {
    Label {
        term,
        words,
        short,
        dotted: &[],
        value: Value::Numbers,
    }
}

/// English terms and abbreviations from the CSL `en-US` locale, plus common
/// variants. `c.` (canon) is left out because it usually means circa, and `v.`
/// means verse, never version.
const LABELS: [Label; 29] = {
    use LocatorTerm::*;
    [
        label(Act, &["act", "acts"], &[]),
        Label {
            short: &[],
            dotted: &["app", "apps"],
            value: Value::Lettered,
            ..label(Appendix, &["appendix", "appendices", "appendixes"], &[])
        },
        Label {
            dotted: &["art", "arts"],
            ..label(ArticleLocator, &["article", "articles"], &[])
        },
        label(Book, &["book", "books"], &["bk", "bks"]),
        label(Canon, &["canon", "canons"], &[]),
        label(Chapter, &["chapter", "chapters", "chap"], &["ch", "chaps"]),
        label(Column, &["column", "columns"], &["col", "cols"]),
        label(Elocation, &["location", "locations"], &["loc", "locs"]),
        label(Equation, &["equation", "equations"], &["eq", "eqs"]),
        label(Figure, &["figure", "figures"], &["fig", "figs"]),
        label(Folio, &["folio", "folios"], &["fol", "fols"]),
        Label {
            dotted: &["no", "nos"],
            ..label(Issue, &["issue", "issues"], &[])
        },
        label(Line, &["line", "lines"], &["l", "ll"]),
        Label {
            dotted: &["n"],
            ..label(Note, &["note", "notes"], &["nn", "fn"])
        },
        Label {
            dotted: &["op", "opp"],
            ..label(Opus, &["opus", "opera"], &[])
        },
        label(Page, &["page", "pages"], &["p", "pp", "pg", "pgs"]),
        Label {
            dotted: &["para"],
            ..label(Paragraph, &["paragraph", "paragraphs"], &["paras"])
        },
        label(Part, &["part", "parts"], &["pt", "pts"]),
        Label {
            dotted: &["r", "rr"],
            ..label(Rule, &["rule", "rules"], &[])
        },
        label(Scene, &["scene", "scenes"], &["sc", "scs"]),
        label(Section, &["section", "sections"], &["sec", "sect", "secs"]),
        Label {
            value: Value::Text,
            ..label(SubVerbo, &[], &[])
        },
        Label {
            value: Value::Lettered,
            ..label(Supplement, &["supplement", "supplements"], &["supp", "supps"])
        },
        label(Table, &["table", "tables"], &["tbl", "tbls"]),
        Label {
            value: Value::Time,
            ..label(Timestamp, &["timestamp", "time"], &[])
        },
        Label {
            dotted: &["tit"],
            ..label(TitleLocator, &["title", "titles"], &[])
        },
        label(Verse, &["verse", "verses"], &["v", "vv"]),
        label(Version, &["version", "versions"], &[]),
        label(Volume, &["volume", "volumes"], &["vol", "vols"]),
    ]
};

/// Read `text` as a whole locator, such as `pp. 12-14` or `chap12`.
pub fn parse(text: &str) -> Option<Locator> {
    let text = text.trim();
    (text.len() <= MAX_LOCATOR_LEN).then(|| read(text, false)).flatten()
}

/// Why a Page field cannot become a locator value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageValueError {
    MissingValue,
    InvalidValue,
}

/// Read a Page field without leaving a recognised page label in its value.
/// Bare printed labels stay literal; a recognised page prefix must parse fully.
pub fn page_value(text: &str) -> Result<String, PageValueError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(PageValueError::MissingValue);
    }
    if trimmed.len() > MAX_LOCATOR_LEN {
        return Err(PageValueError::InvalidValue);
    }
    if let Some((label, rest)) = find_label(trimmed)
        && label.term == LocatorTerm::Page
    {
        if rest.trim().is_empty() {
            return Err(PageValueError::MissingValue);
        }
        return parse(trimmed)
            .map(|locator| locator.value)
            .ok_or(PageValueError::InvalidValue);
    }
    Ok(text.to_owned())
}

/// Split a trailing locator off `query`, or `None` when there is none or
/// nothing would be left to search for.
pub fn parse_trailing(query: &str) -> Option<TrailingLocator> {
    let q = query.trim_end();
    let floor = q.len().saturating_sub(MAX_LOCATOR_LEN);
    let mut previous = None;
    for (i, c) in q.char_indices() {
        let starts_word =
            !c.is_whitespace() && c != ',' && matches!(previous, Some(p) if p == ',' || char::is_whitespace(p));
        previous = Some(c);
        if i < floor || !starts_word {
            continue;
        }
        let search = q[..i].trim_end();
        let after_comma = search.ends_with(',');
        let search = search.trim_end_matches(',').trim_end();
        if search.is_empty() {
            continue;
        }
        let tail = &q[i..];
        if let Some(locator) = read(tail, after_comma) {
            if continues_before(q, i, floor) {
                return None;
            }
            return Some(TrailingLocator {
                search: search.to_string(),
                locator,
                typed: tail.to_string(),
            });
        }
    }
    None
}

/// Whether the locator found at `start` is the end of a longer locator that
/// begins before `floor`, so it is too long to read and only its end would be
/// taken. That longer locator is a list of values, possibly after a label
/// (`pp 1, 2, …`). Starts from `floor` on were already tried in order.
fn continues_before(q: &str, start: usize, floor: usize) -> bool {
    let mut before = q[..start].trim_end();
    while let Some(rest) = before.strip_suffix([',', '&']) {
        let rest = rest.trim_end();
        match value_start(rest) {
            Some(item) if item < floor => return true,
            Some(item) => before = rest[..item].trim_end(),
            None => {
                before = rest;
                break;
            }
        }
    }
    let word = before
        .char_indices()
        .rfind(|(_, c)| c.is_whitespace())
        .map_or(0, |(i, c)| i + c.len_utf8());
    word < floor && find_label(&format!("{} ", &before[word..])).is_some()
}

/// Where the value ending `text` starts, if `text` ends with one. A value is
/// at most three words (`12 - 14`), so only the last three are tried.
fn value_start(text: &str) -> Option<usize> {
    let item = text.rfind([',', '&']).map_or(0, |i| i + 1);
    let mut starts: Vec<usize> = text[item..]
        .char_indices()
        .filter(|&(i, c)| !c.is_whitespace() && text[item..][..i].chars().next_back().is_none_or(char::is_whitespace))
        .map(|(i, _)| item + i)
        .collect();
    starts.drain(..starts.len().saturating_sub(3));
    starts.into_iter().find(|&i| is_range(&text[i..], Value::Numbers))
}

/// Read `tail` as a whole locator.
fn read(tail: &str, after_comma: bool) -> Option<Locator> {
    if let Some((label, rest)) = find_label(tail) {
        let value = rest.trim();
        let value = quoted(value).or_else(|| is_value(value, label.value).then_some(value))?;
        return Some(Locator {
            term: label.term,
            value: value.to_string(),
        });
    }
    (after_comma && is_plain_pages(tail)).then(|| Locator {
        term: LocatorTerm::Page,
        value: tail.to_string(),
    })
}

/// The text inside straight or curly double quotes, when it is all of `value`.
fn quoted(value: &str) -> Option<&str> {
    let inner = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| value.strip_prefix('“').and_then(|rest| rest.strip_suffix('”')))?;
    let inner = inner.trim();
    (!inner.is_empty() && !inner.contains(['"', '“', '”'])).then_some(inner)
}

/// A label at the start of `text`, and the text after it. The label must be
/// followed by a space or a period, except a whole word joined to a value
/// that starts with a digit (`page123`).
fn find_label(text: &str) -> Option<(&'static Label, &str)> {
    let by_term = |term| LABELS.iter().find(|label| label.term == term);
    for (sign, term) in [
        ("§§", LocatorTerm::Section),
        ("§", LocatorTerm::Section),
        ("¶¶", LocatorTerm::Paragraph),
        ("¶", LocatorTerm::Paragraph),
    ] {
        if let Some(rest) = text.strip_prefix(sign) {
            return Some((by_term(term)?, rest));
        }
    }
    let lower = text.to_lowercase();
    for (written, term) in [("s.vv.", LocatorTerm::SubVerbo), ("s.v.", LocatorTerm::SubVerbo)] {
        if lower.starts_with(written) && text.is_char_boundary(written.len()) {
            let rest = &text[written.len()..];
            return rest
                .starts_with(char::is_whitespace)
                .then(|| by_term(term).map(|label| (label, rest)))
                .flatten();
        }
    }
    if let Some(rest) = lower.strip_prefix("sub verb") {
        let word = rest.find(|c: char| !c.is_alphabetic()).unwrap_or(rest.len());
        if matches!(&rest[..word], "o" | "is") {
            let skip = "sub verb".len() + word;
            return text[skip..]
                .starts_with(char::is_whitespace)
                .then(|| by_term(LocatorTerm::SubVerbo).map(|label| (label, &text[skip..])))
                .flatten();
        }
    }
    let end = text.find(|c: char| !c.is_alphabetic()).unwrap_or(text.len());
    let word = text[..end].to_lowercase();
    let rest = &text[end..];
    let dot = rest.starts_with('.');
    if word == "s" && dot {
        // German "S. 12" means page; a bare "s" is too common to read.
        return by_term(LocatorTerm::Page).map(|label| (label, &rest[1..]));
    }
    let label = LABELS.iter().find(|label| {
        label.words.contains(&word.as_str())
            || label.short.contains(&word.as_str())
            || dot && label.dotted.contains(&word.as_str())
    })?;
    if dot {
        return Some((label, &rest[1..]));
    }
    if rest.is_empty() || rest.starts_with(char::is_whitespace) {
        return Some((label, rest));
    }
    (label.words.contains(&word.as_str()) && rest.starts_with(char::is_numeric)).then_some((label, rest))
}

/// One or more values separated by commas or `&`, such as `12`, `24-26`,
/// `3, 7 & 9a`.
fn is_value(text: &str, kind: Value) -> bool {
    if text.is_empty() {
        return false;
    }
    if kind == Value::Text {
        return !text.contains(['"', '“', '”']) && text.chars().any(char::is_alphanumeric);
    }
    text.split([',', '&']).all(|item| is_range(item.trim(), kind))
}

/// Plain page numbers after a comma: no letters, and no four-digit numbers.
fn is_plain_pages(text: &str) -> bool {
    !text.is_empty()
        && text.split([',', '&']).all(|item| {
            let mut parts = item.trim().split(['-', '–', '—']);
            let plain = |part: Option<&str>| {
                part.is_some_and(|part| {
                    let part = part.trim();
                    let count = part.chars().count();
                    count > 0 && count != 4 && count <= 9 && part.chars().all(char::is_numeric)
                })
            };
            let first = parts.next();
            let second = parts.next();
            parts.next().is_none() && plain(first) && (second.is_none() || plain(second))
        })
}

fn is_range(item: &str, kind: Value) -> bool {
    let mut parts = item.split(['-', '–', '—']);
    let first = parts.next().unwrap_or("");
    let second = parts.next();
    parts.next().is_none() && is_single(first.trim(), kind) && second.is_none_or(|s| is_single(s.trim(), kind))
}

fn is_single(text: &str, kind: Value) -> bool {
    if kind == Value::Time {
        let parts: Vec<&str> = text.split(':').collect();
        return (2..=3).contains(&parts.len())
            && parts
                .iter()
                .all(|part| (1..=2).contains(&part.len()) && part.bytes().all(|b| b.is_ascii_digit()));
    }
    if kind == Value::Lettered && text.chars().count() == 1 && text.chars().all(|c| c.is_ascii_alphabetic()) {
        return true;
    }
    let end = text.find(|c: char| !c.is_numeric()).unwrap_or(text.len());
    let digits = text[..end].chars().count();
    let rest = &text[end..];
    if digits > 0 {
        let suffix = rest.is_empty()
            || rest.eq_ignore_ascii_case("ff")
            || rest.len() == 1 && rest.bytes().all(|b| b.is_ascii_alphabetic());
        return digits <= 9 && suffix;
    }
    is_roman(text)
}

/// A well-formed Roman numeral from 1 to 3999, in either case.
fn is_roman(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let value_of = |c| match c {
        'i' => 1,
        'v' => 5,
        'x' => 10,
        'l' => 50,
        'c' => 100,
        'd' => 500,
        'm' => 1000,
        _ => 0,
    };
    let values: Vec<u32> = lower.chars().map(value_of).collect();
    if values.is_empty() || values.len() > 15 || values.contains(&0) {
        return false;
    }
    // Each letter adds its value, or subtracts it when a larger one follows.
    let mut value: i64 = 0;
    for (i, &v) in values.iter().enumerate() {
        let next = values.get(i + 1).copied().unwrap_or(0);
        value += if next > v { -i64::from(v) } else { i64::from(v) };
    }
    let Ok(value) = u32::try_from(value) else {
        return false;
    };
    (1..=3999).contains(&value) && to_roman(value) == lower
}

fn to_roman(mut value: u32) -> String {
    let mut out = String::new();
    for (step, letters) in [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ] {
        while value >= step {
            out.push_str(letters);
            value -= step;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct Case {
        query: String,
        search: String,
        locator: Option<Locator>,
    }

    #[test]
    fn shared_locator_queries_preserve_literal_searches_and_typed_intent() {
        let cases: Vec<Case> = serde_json::from_str(include_str!("../../../fixtures/locators.json"))
            .unwrap_or_else(|error| panic!("{error}"));
        for case in cases {
            let read = parse_trailing(&case.query);
            assert_eq!(
                read.as_ref().map(|q| &q.locator),
                case.locator.as_ref(),
                "{}",
                case.query
            );
            assert_eq!(
                read.as_ref().map_or(case.query.as_str(), |q| &q.search),
                case.search,
                "{}",
                case.query
            );
            if let Some(read) = read {
                assert!(case.query.ends_with(&read.typed));
                assert!(read.typed.len() <= MAX_LOCATOR_LEN);
            }
        }
    }

    #[test]
    fn unquoted_unicode_page_numbers_preserve_digits_and_count_characters() {
        for value in ["١٢–١٥", "۱۲–۱۵", "१२–१५", "１２–１５", "๑๒–๑๕", "𝟙𝟚–𝟙𝟝", "١٢٣٤٥٦٧٨٩a"]
        {
            let expected = Some(Locator {
                term: LocatorTerm::Page,
                value: value.into(),
            });
            assert_eq!(parse(&format!("pp. {value}")), expected);
            assert_eq!(
                parse_trailing(&format!("Synthetic pp. {value}")).map(|p| p.locator),
                expected
            );
            assert_eq!(parse(&format!("page{value}")), expected);
        }
        assert!(
            parse("pp. ١٢٣٤٥٦٧٨٩٠").is_none(),
            "ten digits still exceed the numeric limit"
        );
        assert!(
            parse_trailing("Synthetic, ٢٠٢٦").is_none(),
            "four Unicode digits can be a year"
        );
        assert_eq!(
            parse_trailing("Synthetic, ١٢–١٥").map(|p| p.locator.value),
            Some("١٢–١٥".into())
        );
    }

    #[test]
    fn page_fields_distinguish_literal_values_from_incomplete_or_invalid_labels() {
        for value in ["", " ", "p", "p.", "PP", "pp.", "page", "pages", "pgs.", "S."] {
            assert_eq!(page_value(value), Err(PageValueError::MissingValue), "{value:?}");
        }
        for value in [
            "pp. \"\"",
            "pp. 12 appendix",
            "pp. \"12\" trailing",
            "p. --",
            "pp. ١٢٣٤٥٦٧٨٩٠",
        ] {
            assert_eq!(page_value(value), Err(PageValueError::InvalidValue), "{value:?}");
        }
        assert_eq!(
            page_value(&"1".repeat(MAX_LOCATOR_LEN + 1)),
            Err(PageValueError::InvalidValue)
        );
        for (typed, value) in [
            ("pp. ١٢–١٥", "١٢–١٥"),
            (" P. iv ", "iv"),
            ("pp. \"the epigraph\"", "the epigraph"),
            ("p53", "p53"),
            ("S12", "S12"),
            ("chap. 3", "chap. 3"),
        ] {
            assert_eq!(page_value(typed), Ok(value.into()));
        }
    }

    #[test]
    fn every_csl_locator_type_can_be_typed() {
        for term in LocatorTerm::ALL {
            let label = LABELS
                .iter()
                .find(|label| label.term == term)
                .unwrap_or_else(|| panic!("{term:?}"));
            let written = label
                .words
                .first()
                .or(label.short.first())
                .map_or_else(|| "s.v.".to_string(), |word| (*word).to_string());
            let value = match label.value {
                Value::Time => "1:02:03",
                Value::Text => "ratio",
                _ => "12",
            };
            let query = format!("invented work {written} {value}");
            assert_eq!(
                parse_trailing(&query).map(|q| q.locator),
                Some(Locator {
                    term,
                    value: value.into()
                }),
                "{query}"
            );
            // A quoted textual locator works for every type.
            assert_eq!(
                parse(&format!("{written} \"the epigraph\"")).map(|l| (l.term, l.value)),
                Some((term, "the epigraph".into()))
            );
        }
    }

    #[test]
    fn plain_searches_are_left_alone() {
        for query in [
            "smith 2020",
            "smith, 2020",
            "garcia 12-14",
            "p 45",
            ", 12",
            "tumour suppressor p53",
            "smith p",
            "smith, xii",
            "trains, planes",
            "vitamin d",
            "smith p 45 onwards",
            "smith p 1-2-3",
            "smith p 1234567890",
            "smith, 2020a",
            "smith, 1999, 2001",
            "smith, 1999-2001",
            "smith, 2020-21",
            "railways, 1850s",
            "printing, 3d",
            "video, 4k",
            "agenda para 2030",
            "cartas para mi",
            "decreto n 50",
            "smith p civil",
            "people v. mill",
            "smith p iiii",
            "yes or no 5",
            "modern art 3",
            "rights of man op 3",
            "smith c. 1900",
            "smith fig\"x\"",
            "smith note \"\"",
            "smith note \"half",
            "reference at 01:23",
            "reference S 12",
            "reference s 12",
            "reference s. civil",
            "smith v2",
            "smith sec61",
            "",
        ] {
            assert_eq!(parse_trailing(query), None, "{query}");
        }
    }

    #[test]
    fn an_overlong_locator_is_left_whole_in_the_search() {
        for query in [
            format!("smith p {}1", "1, ".repeat(30)),
            format!("smith, {}1", "1, ".repeat(30)),
            format!("smith pp {}1", "1-2, ".repeat(20)),
            format!("smith note \"{}\"", "a".repeat(70)),
        ] {
            assert_eq!(parse_trailing(&query), None, "{query}");
        }
        let at_limit = format!("smith pp {}2", "1, ".repeat(20));
        assert_eq!(parse_trailing(&at_limit).map(|q| q.typed.len()), Some(64));
        let long_search = format!("{} p 45", "sleep ".repeat(30));
        assert_eq!(parse_trailing(&long_search).map(|q| q.locator.value), Some("45".into()));
    }

    #[test]
    fn any_text_is_handled_without_panicking() {
        for query in [
            "§",
            "日本 p 4",
            "ü, 1",
            "a ¶",
            "x §§ 1",
            "x s.v.",
            "x sub verbø 1",
            "x S.",
            "x “”",
            &"9, ".repeat(10_000),
            &"a ".repeat(10_000),
            &format!("a{} 1", ",".repeat(100_000)),
        ] {
            let _ = parse_trailing(query);
            let _ = parse(query);
        }
    }
}
