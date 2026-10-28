//! One bounded scan of typed citation keys and complete identifiers; no text is retained.
use std::borrow::Cow;

use crate::{
    identifier::{self, Identifier},
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

/// Bare word lists still need exact keys, so ordinary prose stays a word search.
/// Explicit citations and identifiers keep their types even when pasted together.
pub(super) fn entries(query: &str) -> Option<Entries<'_>> {
    if query.len() > 64 * 1024 {
        return None;
    }
    let mut found = Vec::new();
    let mut cited = false;
    let comma = query.contains(',');
    let separated = query.contains(['\n', '\r', '\t', ',', ';']);
    let mut rest = query;
    while !rest.is_empty() {
        let c = rest.chars().next()?;
        if c.is_whitespace() || c == ',' || c == ';' && (!comma || cited || found.last().is_some_and(Entry::explicit)) {
            rest = &rest[c.len_utf8()..];
            continue;
        }
        // A DOI suffix may itself contain commas, semicolons and @. Consume a
        // complete identifier before exposing any of its internal punctuation.
        if let Some((text, value, after)) = identifier::parse_token(rest) {
            add(&mut found, Entry::Identifier { text, value })?;
            rest = after;
            continue;
        }
        let citation_text = rest
            .strip_prefix('-')
            .filter(|after| after.starts_with('@'))
            .unwrap_or(rest);
        if let Some(read) = syntax::read::citation(citation_text) {
            cited |= read.explicit;
            for value in read.keys {
                add(&mut found, Entry::Key { value, explicit: true })?;
            }
            rest = read.rest;
            continue;
        }
        let end = rest
            .char_indices()
            .find_map(|(i, c)| {
                (c.is_whitespace()
                    || c == ','
                    || !comma && c == ';'
                    || i > 0 && c == '\\' && syntax::read::citation(&rest[i..]).is_some_and(|read| read.explicit))
                .then_some(i)
            })
            .unwrap_or(rest.len());
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

fn add<'a>(entries: &mut Vec<Entry<'a>>, entry: Entry<'a>) -> Option<()> {
    if !entry.text().is_empty() {
        if entries.len() == syntax::MAX_CITATION_ITEMS {
            return None;
        }
        entries.push(entry);
    }
    Some(())
}
