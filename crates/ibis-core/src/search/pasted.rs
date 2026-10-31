//! One bounded scan of typed citation keys and complete identifiers; no text is retained.
use std::borrow::Cow;

use crate::{
    identifier::{self, Identifier},
    scan::{Stopped, Work},
    syntax,
};

pub(super) enum Entry<'a> {
    Key { value: Cow<'a, str>, explicit: bool },
    Identifier { text: &'a str, value: Identifier },
}
impl Entry<'_> {
    pub fn text(&self) -> &str {
        match self {
            Self::Key { value, .. } => value,
            Self::Identifier { text, .. } => text,
        }
    }
    pub fn explicit(&self) -> bool {
        !matches!(self, Self::Key { explicit: false, .. })
    }
}

pub(super) struct Entries<'a> {
    pub entries: Vec<Entry<'a>>,
}

/// Resolve literal lists before punctuation in a key can become citation syntax.
pub(super) fn raw_entries<'a>(query: &'a str, work: &mut Work<'_>) -> Result<Option<Entries<'a>>, Stopped> {
    if !work.spend(query.len()) {
        return work.check().map(|()| None);
    }
    let comma = query.contains(',');
    if !query.contains(['\n', '\r', '\t', ',', ';']) {
        return Ok(None);
    }
    let mut entries = Vec::new();
    for value in query.split(|c: char| c.is_whitespace() || c == ',' || !comma && c == ';') {
        if add(
            &mut entries,
            Entry::Key {
                value: Cow::Borrowed(value),
                explicit: false,
            },
        )
        .is_none()
        {
            return Ok(None);
        }
    }
    Ok((entries.len() >= 2).then_some(Entries { entries }))
}

/// Bare word lists still need exact keys, so ordinary prose stays a word search.
/// Explicit citations and identifiers keep their types even when pasted together.
pub(super) fn entries<'a>(query: &'a str, work: &mut Work<'_>) -> Result<Option<Entries<'a>>, Stopped> {
    let entries = scan(query, work);
    work.check()?;
    Ok(entries)
}

fn scan<'a>(query: &'a str, work: &mut Work<'_>) -> Option<Entries<'a>> {
    if !work.spend(query.len()) {
        return None;
    }
    let mut found = Vec::new();
    let mut cited = false;
    let comma = query.contains(',');
    let separated = query.contains(['\n', '\r', '\t', ',', ';']);
    let mut rest = query;
    while !rest.is_empty() {
        if !work.spend(1) {
            return None;
        }
        let c = rest.chars().next()?;
        if c.is_whitespace() || c == ',' || c == ';' && (!comma || cited || found.last().is_some_and(Entry::explicit)) {
            rest = &rest[c.len_utf8()..];
            continue;
        }
        let citation_text = rest
            .strip_prefix('-')
            .filter(|after| after.starts_with('@'))
            .unwrap_or(rest);
        if let Some(read) = syntax::read::citation(citation_text, work).ok()? {
            cited |= read.explicit;
            if read.explicit {
                for value in read.keys {
                    add(&mut found, Entry::Key { value, explicit: true })?;
                }
            } else {
                add(
                    &mut found,
                    Entry::Key {
                        value: Cow::Borrowed(&rest[..rest.len() - read.rest.len()]),
                        explicit: false,
                    },
                )?;
            }
            rest = read.rest;
            continue;
        }
        // Consume complete identifiers before their internal punctuation can
        // expose a citation start (DOI suffixes may contain commas and @).
        if let Some((text, value, after)) = identifier::parse_token(rest, work) {
            add(&mut found, Entry::Identifier { text, value })?;
            rest = after;
            continue;
        }
        let mut end = rest.len();
        let mut previous = None;
        for (i, c) in rest.char_indices() {
            if !work.spend(c.len_utf8()) {
                return None;
            }
            if c.is_whitespace()
                || c == ','
                || !comma && c == ';'
                || i > 0 && syntax::read::starts(&rest[i..]) && (c == '\\' || previous.is_some_and(boundary))
            {
                end = i;
                break;
            }
            previous = Some(c);
        }
        add(
            &mut found,
            Entry::Key {
                value: Cow::Borrowed(&rest[..end]),
                explicit: false,
            },
        )?;
        rest = &rest[end..];
    }
    if cited {
        found.retain(Entry::explicit); // Surrounding prose is never a guessed key.
    } else if !(separated && found.len() >= 2
        || !found.is_empty() && found.iter().all(|entry| matches!(entry, Entry::Identifier { .. })))
    {
        return None;
    }
    Some(Entries { entries: found })
}

fn boundary(c: char) -> bool {
    c.is_whitespace() || "[](){};,:.!?\"'–—".contains(c)
}

fn add<'a>(entries: &mut Vec<Entry<'a>>, entry: Entry<'a>) -> Option<()> {
    if !entry.text().is_empty() {
        if entries.len() == syntax::MAX_CITATION_ITEMS {
            return None;
        }
        entries.push(entry);
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_typst_consumes_its_scanned_span_in_linear_work() {
        for count in [1, 100, 4000] {
            let query = format!("@a {}", r##"#cite(label("\q "##.repeat(count));
            let mut work = Work::new(query.len(), &|| false);
            let parsed = entries(&query, &mut work)
                .unwrap_or_else(|error| panic!("scan stopped: {error:?}"))
                .unwrap_or_else(|| panic!("citation list"));
            assert_eq!(parsed.entries.len(), 1);
            assert_eq!(parsed.entries[0].text(), "a");
            assert!(
                work.used() <= 6 * query.len(),
                "{} visits for {} bytes",
                work.used(),
                query.len()
            );
        }
    }

    #[test]
    fn multimarkdown_notes_with_unmatched_pandoc_keys_scan_in_linear_work() {
        for count in [1, 100, 1000] {
            let query = "[@{section][#a]".repeat(count);
            let mut work = Work::new(query.len(), &|| false);
            let parsed = entries(&query, &mut work)
                .unwrap_or_else(|error| panic!("scan stopped: {error:?}"))
                .unwrap_or_else(|| panic!("citation list"));
            assert_eq!(parsed.entries.len(), count);
            assert!(parsed.entries.iter().all(|entry| entry.text() == "a"));
            assert!(
                work.used() <= 8 * query.len(),
                "{} visits for {} bytes",
                work.used(),
                query.len()
            );
        }
    }

    #[test]
    fn nested_scans_share_cancellation_and_work_limits() {
        let query = format!("@a {}", "[".repeat(60_000));
        let polls = std::cell::Cell::new(0);
        let stop = || {
            polls.set(polls.get() + 1);
            polls.get() > 8
        };
        let mut work = Work::new(query.len(), &stop);
        assert!(matches!(
            syntax::read::citation(&query[3..], &mut work),
            Err(Stopped::Cancelled)
        ));
        assert!(work.used() <= 8 * 256);

        // A nested reader cannot restart with a fresh allowance or return the
        // earlier @a as a list when a shared allowance is almost exhausted.
        let query = format!("@a {}", "[".repeat(1000));
        let mut work = Work::new(query.len(), &|| false);
        assert!(work.spend(32 * query.len() - (query.len() + 64)));
        assert!(matches!(entries(&query, &mut work), Err(Stopped::Limit)));
        assert!(work.used() <= 32 * query.len());
    }
}
