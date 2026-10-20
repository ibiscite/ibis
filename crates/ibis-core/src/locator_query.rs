//! A locator typed at the end of a search, as in `smith 2020 p 45` or
//! `garcia, 12-14`.
//!
//! Only two forms are read, so a plain search is never changed by guesswork:
//! a locator label followed by a value (`p 45`, `chap. 3`, `§ 2`), or a value
//! after a comma (`garcia, 12-14`, read as pages). After a comma only plain
//! numbers count, and never a four-digit one, which is most likely a year, so
//! `smith, 2020, 45` keeps `smith, 2020` as the search. The caller shows what
//! was read (`typed`) so the person can undo it.

use crate::cite::{Locator, LocatorLabel};

/// A search with a locator split off its end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatorQuery {
    /// What remains to search for.
    pub search: String,
    pub locator: Locator,
    /// The text read as the locator, exactly as typed.
    pub typed: String,
}

/// Longer text is searched as written; no locator is that long.
const MAX_LOCATOR_LEN: usize = 64;

/// Split a trailing locator off `query`, or `None` when there is none or
/// nothing would be left to search for.
pub fn split(query: &str) -> Option<LocatorQuery> {
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
            return Some(LocatorQuery {
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
    let word = before.rfind(char::is_whitespace).map_or(0, |i| i + 1);
    word < floor && label(&format!("{} ", &before[word..])).is_some()
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
    starts.into_iter().find(|&i| is_range(&text[i..], true))
}

/// Read `tail` as a whole locator.
fn read(tail: &str, after_comma: bool) -> Option<Locator> {
    if let Some((label, rest)) = label(tail) {
        let value = rest.trim();
        return is_value(value, true).then(|| Locator {
            label,
            value: value.to_string(),
        });
    }
    (after_comma && is_value(tail, false)).then(|| Locator {
        label: LocatorLabel::Page,
        value: tail.to_string(),
    })
}

/// A label word at the start of `text`, and the text after it.
fn label(text: &str) -> Option<(LocatorLabel, &str)> {
    for sign in ["§§", "§", "¶¶", "¶"] {
        if let Some(rest) = text.strip_prefix(sign) {
            let kind = if sign.starts_with('¶') {
                LocatorLabel::Paragraph
            } else {
                LocatorLabel::Section
            };
            return Some((kind, rest));
        }
    }
    let end = text.find(|c: char| !c.is_alphabetic()).unwrap_or(text.len());
    let kind = match text[..end].to_lowercase().as_str() {
        "p" | "pp" | "pg" | "pgs" | "page" | "pages" => LocatorLabel::Page,
        "ch" | "chap" | "chaps" | "chapter" | "chapters" => LocatorLabel::Chapter,
        "sec" | "sect" | "secs" | "section" | "sections" => LocatorLabel::Section,
        // "para" and "n" are also words in other languages ("para 2030",
        // "n 50"), so they count only with a period: "para. 4", "n. 12".
        "para" | "n" if !text[end..].starts_with('.') => return None,
        "para" | "paras" | "paragraph" | "paragraphs" => LocatorLabel::Paragraph,
        "fig" | "figs" | "figure" | "figures" => LocatorLabel::Figure,
        "l" | "ll" | "line" | "lines" => LocatorLabel::Line,
        "n" | "nn" | "fn" | "note" | "notes" => LocatorLabel::Note,
        "vol" | "vols" | "volume" | "volumes" => LocatorLabel::Volume,
        "v" | "vv" | "verse" | "verses" => LocatorLabel::Verse,
        _ => return None,
    };
    let rest = &text[end..];
    let rest = rest.strip_prefix('.').unwrap_or(rest);
    // The label must be its own word: `p45` is not read, `p 45` and `p.45` are.
    (rest.starts_with(char::is_whitespace) || end < text.len() && text[end..].starts_with('.')).then_some((kind, rest))
}

/// One or more numbers or ranges separated by commas or `&`, such as
/// `12`, `24-26`, `3, 7 & 9a`. After a label, numbers may carry a letter or
/// `ff` (`45a`, `45ff`) and Roman numerals count; after a comma only plain
/// numbers of other than four digits do.
fn is_value(text: &str, labelled: bool) -> bool {
    !text.is_empty() && text.split([',', '&']).all(|item| is_range(item.trim(), labelled))
}

fn is_range(item: &str, labelled: bool) -> bool {
    let mut parts = item.split(['-', '–', '—']);
    let first = parts.next().unwrap_or("");
    let second = parts.next();
    parts.next().is_none() && is_number(first.trim(), labelled) && second.is_none_or(|s| is_number(s.trim(), labelled))
}

fn is_number(text: &str, labelled: bool) -> bool {
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    let rest = &text[digits..];
    if !labelled {
        return digits > 0 && digits != 4 && digits <= 9 && rest.is_empty();
    }
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

    fn read(query: &str) -> Option<(String, LocatorLabel, String)> {
        split(query).map(|q| (q.search, q.locator.label, q.locator.value))
    }

    fn page(search: &str, value: &str) -> Option<(String, LocatorLabel, String)> {
        Some((search.into(), LocatorLabel::Page, value.into()))
    }

    #[test]
    fn a_labelled_locator_is_split_off() {
        assert_eq!(read("smith 2020 p 45"), page("smith 2020", "45"));
        assert_eq!(read("smith 2020 p. 45"), page("smith 2020", "45"));
        assert_eq!(read("smith 2020 p.45"), page("smith 2020", "45"));
        assert_eq!(read("Smith PP 12–14"), page("Smith", "12–14"));
        assert_eq!(read("smith pages 3, 7 & 9a"), page("smith", "3, 7 & 9a"));
        assert_eq!(read("smith, p 45  "), page("smith", "45"));
        assert_eq!(read("kant chap. 3").map(|r| r.1), Some(LocatorLabel::Chapter));
        assert_eq!(read("kant § 2").map(|r| r.1), Some(LocatorLabel::Section));
        assert_eq!(
            read("kant §2-4").map(|r| (r.1, r.2)),
            Some((LocatorLabel::Section, "2-4".into()))
        );
        assert_eq!(read("kant ¶ 12").map(|r| r.1), Some(LocatorLabel::Paragraph));
        assert_eq!(read("bible vv 3-5").map(|r| r.1), Some(LocatorLabel::Verse));
        assert_eq!(read("collected vol 2").map(|r| r.1), Some(LocatorLabel::Volume));
        assert_eq!(read("preface p xii"), page("preface", "xii"));
        assert_eq!(read("preface p XIV-xvi"), page("preface", "XIV-xvi"));
        assert_eq!(read("smith p 45ff"), page("smith", "45ff"));
        assert_eq!(read("kant ¶¶ 3-4").map(|r| r.1), Some(LocatorLabel::Paragraph));
        assert_eq!(read("kant para. 3").map(|r| r.1), Some(LocatorLabel::Paragraph));
        assert_eq!(read("kant n. 12").map(|r| r.1), Some(LocatorLabel::Note));
    }

    #[test]
    fn a_value_after_a_comma_is_read_as_pages() {
        assert_eq!(read("garcia, 12-14"), page("garcia", "12-14"));
        assert_eq!(read("garcia 2019, 7"), page("garcia 2019", "7"));
        assert_eq!(read("garcia,12"), page("garcia", "12"));
        // A year after a comma stays in the search.
        assert_eq!(read("smith, 2020, 45"), page("smith, 2020", "45"));
        assert_eq!(read("garcia, 2019, 12-14"), page("garcia, 2019", "12-14"));
    }

    #[test]
    fn plain_searches_are_left_alone() {
        for query in [
            "smith 2020",
            "smith, 2020",
            "garcia 12-14",
            "p 45",
            ", 12",
            "smith p45",
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
            "",
        ] {
            assert_eq!(split(query), None, "{query}");
        }
    }

    #[test]
    fn an_overlong_locator_is_left_whole_in_the_search() {
        for query in [
            format!("smith p {}1", "1, ".repeat(30)),
            format!("smith, {}1", "1, ".repeat(30)),
            format!("smith pp {}1", "1-2, ".repeat(20)),
        ] {
            assert_eq!(split(&query), None, "{query}");
        }
        // Spaced ranges, with the limit falling at every point inside one.
        for dash in ["-", "–"] {
            for last in ["1", "12", "123", "1234"] {
                for count in 10..18 {
                    let query = format!("smith pp {}{last}", format!("1 {dash} 2, ").repeat(count));
                    assert_eq!(split(&query), None, "{query}");
                }
            }
        }
        // A long search before a short locator is still split.
        // The label counts toward the length too, wherever the limit falls.
        let label_split = format!("smith pp {}12", "1, ".repeat(20));
        assert_eq!(label_split.len() - "smith ".len(), 65);
        let label_spaced = format!("smith pp{}1, 2", " ".repeat(65));
        for query in [label_split, label_spaced, format!("smith §1, {}2", "1, ".repeat(20))] {
            assert_eq!(split(&query), None, "{query}");
        }
        let at_limit = format!("smith pp {}2", "1, ".repeat(20));
        assert_eq!(at_limit.len() - "smith ".len(), 64);
        assert_eq!(split(&at_limit).map(|q| q.typed.len()), Some(64));
        let long_search = format!("{} p 45", "sleep ".repeat(30));
        assert_eq!(split(&long_search).map(|q| q.locator.value), Some("45".into()));
        let long_list_search = format!("{}smith, 2020, 45", "1, ".repeat(30));
        assert_eq!(split(&long_list_search).map(|q| q.locator.value), Some("45".into()));
    }

    #[test]
    fn the_typed_text_is_kept_for_undo() {
        let q = split("smith 2020 chap. 3").map(|q| (q.search, q.typed));
        assert_eq!(q, Some(("smith 2020".into(), "chap. 3".into())));
    }

    #[test]
    fn any_text_is_handled_without_panicking() {
        for query in [
            "§",
            "日本 p 4",
            "ü, 1",
            "a ¶",
            "x §§ 1",
            &"9, ".repeat(10_000),
            &"a ".repeat(10_000),
            &format!("a{} 1", ",".repeat(100_000)),
        ] {
            let _ = split(query);
        }
        assert_eq!(read("日本 p 4"), page("日本", "4"));
    }
}
