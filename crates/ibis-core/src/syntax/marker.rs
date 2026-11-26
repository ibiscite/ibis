//! Pandoc citation markers in a document's own text, for `ibis format`: the
//! markers Ibis pastes as Pandoc keys (`[see @vale2024, p. 12; -@fen2023]`,
//! `@vale2024 [ch. 3]`). The same key and bracket readers as pasted lists,
//! keeping what those drop: prefixes, locators, suffixes and omitted authors.
//!
//! A bracket with no citation key is ordinary text. A bracket with a key that
//! is not a whole citation is a problem, never guessed at or left in place.

use super::read::{bare_key, pandoc_bracket, pandoc_key_start};
use crate::contract::{Cite, Locator, ScanProblem};
use crate::locator::read_given;
use crate::scan::Work;

/// The longest marker read, brackets included.
pub const MAX_MARKER_BYTES: usize = 4096;

/// One citation marker at `start..end` (bytes) of the text it was read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker {
    pub start: usize,
    pub end: usize,
    /// `@key`, the author as part of the sentence.
    pub narrative: bool,
    pub cites: Vec<Cite>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkerProblem {
    pub kind: ScanProblem,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    Marker(Marker),
    Problem(MarkerProblem),
}

/// Every marker and marker problem in `text`, in order. Linear in `text`:
/// a pathological text stops with one [`ScanProblem::TooLong`].
pub fn markers(text: &str) -> impl Iterator<Item = Found> + '_ {
    let mut work = Work::new(text.len().max(MAX_MARKER_BYTES), &|| false);
    let mut at = 0;
    std::iter::from_fn(move || next(text, &mut at, &mut work))
}

/// [`markers`] on one allowance shared with the caller's other reading, so a
/// whole document is linear in its length and stops when cancelled.
pub(crate) fn read<'a>(text: &'a str, work: &'a mut Work<'_>) -> impl Iterator<Item = Found> + 'a {
    let mut at = 0;
    std::iter::from_fn(move || next(text, &mut at, work))
}

/// Read only the next marker, so callers can stop at their own count bound
/// without allocating the rest of a paragraph's markers.
fn next(text: &str, at: &mut usize, work: &mut Work<'_>) -> Option<Found> {
    while let Some(c) = text[*at..].chars().next() {
        let read = match c {
            '\\' => {
                // An escaped character is literal text.
                *at += 1 + text[*at + 1..].chars().next().map_or(0, char::len_utf8);
                continue;
            }
            '[' => bracket(text, *at, work),
            '@' if pandoc_key_start(&text[..*at]) => narrative(text, *at, work).map(Found::Marker),
            _ => None,
        };
        if work.check().is_err() {
            let problem = Found::Problem(MarkerProblem {
                kind: ScanProblem::TooLong,
                start: *at,
                end: text.len(),
            });
            *at = text.len();
            return Some(problem);
        }
        match read {
            Some(Found::Problem(problem)) if problem.kind == ScanProblem::Unclosed => {
                // Nothing after an unclosed marker can be placed with certainty.
                *at = text.len();
                return Some(Found::Problem(problem));
            }
            Some(read) => {
                *at = match &read {
                    Found::Marker(marker) => marker.end,
                    Found::Problem(problem) => problem.end,
                };
                return Some(read);
            }
            None => *at += c.len_utf8(),
        }
    }
    None
}

/// At most [`MAX_MARKER_BYTES`] of `text` from `start`, on a character boundary.
fn window(text: &str, start: usize) -> &str {
    let mut end = (start + MAX_MARKER_BYTES).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[start..end]
}

/// Byte offsets of the `@`s in `text` that start a key.
fn key_starts(text: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut chars = text.char_indices();
    while let Some((i, c)) = chars.next() {
        if c == '\\' {
            chars.next();
        } else if c == '@' && pandoc_key_start(&text[..i]) {
            starts.push(i);
        }
    }
    starts
}

/// Split at unescaped `;` outside braces.
fn segments(body: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut from, mut braces, mut escaped) = (0, 0_usize, false);
    for (i, c) in body.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '{' => braces += 1,
            '}' => braces = braces.saturating_sub(1),
            ';' if braces == 0 => {
                parts.push(&body[from..i]);
                from = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&body[from..]);
    parts
}

fn bracket(text: &str, start: usize, work: &mut Work<'_>) -> Option<Found> {
    let open = &window(text, start)[1..];
    let (body, _) = pandoc_bracket(open, work);
    let closed = body.len() < open.len();
    if !closed {
        // Brackets without a key are prose; an opened citation must close.
        // Looking for a key reads the whole window again, so it is paid for.
        if !work.spend(open.len()) {
            return None;
        }
        return (!key_starts(open).is_empty()).then(|| {
            Found::Problem(MarkerProblem {
                kind: ScanProblem::Unclosed,
                start,
                end: start + 1 + open.len(),
            })
        });
    }
    let end = start + 1 + body.len() + 1;
    // A bracket around a citation is prose; the citation inside is read next.
    if body.contains('[') || key_starts(body).is_empty() {
        return None;
    }
    // A problem names the whole marker, as the person sees it.
    let problem = |kind| Some(Found::Problem(MarkerProblem { kind, start, end }));
    let mut cites = Vec::new();
    for segment in segments(body) {
        let keys = key_starts(segment);
        let Some(&at) = keys.first() else {
            return problem(ScanProblem::WithoutKey);
        };
        if keys.len() > 1 {
            return problem(ScanProblem::TwoKeys);
        }
        let (key, used) = bare_key(&segment[at + 1..], work);
        if key.is_empty() {
            return problem(ScanProblem::WithoutKey);
        }
        let before = &segment[..at];
        let suppress_author = before.ends_with('-');
        let prefix = before.strip_suffix('-').unwrap_or(before);
        let Some((locator, suffix)) = notes(&segment[at + 1 + used..]) else {
            return problem(ScanProblem::Locator);
        };
        cites.push(Cite {
            key: key.to_owned(),
            prefix: unescape(prefix.trim()),
            locator,
            suffix,
            suppress_author,
        });
    }
    Some(Found::Marker(Marker {
        start,
        end,
        narrative: false,
        cites,
    }))
}

/// `@key`, and its `[locator, suffix]` after one space.
fn narrative(text: &str, start: usize, work: &mut Work<'_>) -> Option<Marker> {
    let after = &text[start + 1..];
    let (key, token) = bare_key(after, work);
    if key.is_empty() {
        return None;
    }
    let used = if after.starts_with('{') { token } else { key.len() };
    let mut marker = Marker {
        start,
        end: start + 1 + used,
        narrative: true,
        cites: vec![Cite {
            key: key.to_owned(),
            ..Cite::default()
        }],
    };
    if let Some(open) = text[marker.end..].strip_prefix(" [") {
        let open = window(open, 0);
        let (body, _) = pandoc_bracket(open, work);
        if body.len() < open.len()
            && !body.contains('[')
            && key_starts(body).is_empty()
            && let Some((locator, suffix)) = notes(body)
        {
            marker.cites[0].locator = locator;
            marker.cites[0].suffix = suffix;
            marker.end += 2 + body.len() + 1;
        }
    }
    Some(marker)
}

/// What follows a key: `, p. 12`, `, {p. 12}, and passim`, `, {}see note`.
/// `None` when a braced locator is not one.
fn notes(text: &str) -> Option<(Option<Locator>, String)> {
    let text = text.trim_start();
    let text = text.strip_prefix(',').unwrap_or(text).trim_start();
    if let Some(inner) = text.strip_prefix('{') {
        let close = inner.find('}')?;
        let given = unescape(&inner[..close]);
        let locator = if given.trim().is_empty() {
            None
        } else {
            Some(read_given(&given)?)
        };
        let rest = inner[close + 1..].trim_start();
        let rest = rest.strip_prefix(',').unwrap_or(rest);
        return Some((locator, unescape(rest.trim())));
    }
    let whole = unescape(text.trim());
    if whole.is_empty() {
        return Some((None, whole));
    }
    if let Some(locator) = read_given(&whole) {
        return Some((Some(locator), String::new()));
    }
    if let Some((head, tail)) = text.split_once(',')
        && let Some(locator) = read_given(&unescape(head))
    {
        return Some((Some(locator), unescape(tail.trim())));
    }
    Some((None, whole))
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.extend(chars.next()),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::LocatorTerm;

    fn read(text: &str) -> Vec<Found> {
        markers(text).collect()
    }
    fn only(text: &str) -> Marker {
        match read(text).as_slice() {
            [Found::Marker(marker)] => marker.clone(),
            other => panic!("{text}: {other:?}"),
        }
    }
    fn cite(key: &str) -> Cite {
        Cite {
            key: key.into(),
            ..Cite::default()
        }
    }
    fn page(value: &str) -> Option<Locator> {
        Some(Locator {
            term: LocatorTerm::Page,
            value: value.into(),
        })
    }

    #[test]
    fn markers_read_every_form_ibis_pastes() {
        let text = "As shown [@vale2024, p. 12], and [see @fen2023, ch. 3; -@vale2024].";
        let found = read(text);
        let [Found::Marker(first), Found::Marker(second)] = found.as_slice() else {
            panic!("{found:?}")
        };
        assert_eq!(&text[first.start..first.end], "[@vale2024, p. 12]");
        assert_eq!(
            first.cites,
            [Cite {
                locator: page("12"),
                ..cite("vale2024")
            }]
        );
        assert_eq!(&text[second.start..second.end], "[see @fen2023, ch. 3; -@vale2024]");
        assert_eq!(second.cites[0].prefix, "see");
        assert_eq!(
            second.cites[0].locator.as_ref().map(|l| l.term),
            Some(LocatorTerm::Chapter)
        );
        assert!(second.cites[1].suppress_author);

        let narrative = only("@vale2024 [p. 4] argues.");
        assert!(narrative.narrative);
        assert_eq!((narrative.start, narrative.end), (0, 16));
        assert_eq!(narrative.cites[0].locator, page("4"));
        let narrative = only("as @vale2024.");
        assert_eq!(&"as @vale2024."[narrative.start..narrative.end], "@vale2024");

        // Ibis's own escaping of a suffix and a braced locator.
        assert_eq!(
            only("[@a, {}see note]").cites[0],
            Cite {
                suffix: "see note".into(),
                ..cite("a")
            }
        );
        assert_eq!(
            only("[@a, {pp. 3-4}, and passim]").cites[0],
            Cite {
                locator: page("3-4"),
                suffix: "and passim".into(),
                ..cite("a")
            }
        );
        assert_eq!(only("[@{odd key}]").cites[0].key, "odd key");
    }

    #[test]
    fn prose_is_left_alone_and_damaged_markers_are_problems() {
        for prose in ["write to a@b.example", "[a note]", "costs \\@ 5", "[[@a] around]"] {
            assert!(
                read(prose)
                    .iter()
                    .all(|found| matches!(found, Found::Marker(m) if m.cites[0].key == "a")),
                "{prose}"
            );
        }
        let kinds = |text: &str| {
            read(text)
                .into_iter()
                .map(|found| match found {
                    Found::Problem(problem) => Some(problem.kind),
                    Found::Marker(_) => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(kinds("[@a; see]"), [Some(ScanProblem::WithoutKey)]);
        assert_eq!(kinds("[@a @b]"), [Some(ScanProblem::TwoKeys)]);
        assert_eq!(kinds("[@a, {here}]"), [Some(ScanProblem::Locator)]);
        assert_eq!(kinds("[@a, p. 3 and on"), [Some(ScanProblem::Unclosed)]);
        assert_eq!(kinds(&"[".repeat(200_000)), [Some(ScanProblem::TooLong)]);
    }

    #[test]
    fn marker_reading_waits_for_the_consumer_before_allocating_the_next_marker() {
        let text = format!("[@a] [@{}]", "b".repeat(512));
        let cancelled = std::cell::Cell::new(false);
        let stop = || cancelled.get();
        let mut work = Work::new(text.len(), &stop);
        let mut stream = super::read(&text, &mut work);
        assert!(matches!(stream.next(), Some(Found::Marker(marker)) if marker.cites[0].key == "a"));
        cancelled.set(true);
        assert!(matches!(stream.next(), Some(Found::Problem(problem)) if problem.kind == ScanProblem::TooLong));
        assert_eq!(stream.next(), None);
    }
}
