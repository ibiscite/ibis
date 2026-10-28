//! Bounded, transient citation-list recognition; no document text is retained.
use crate::identifier;

pub(super) struct Entries<'a> {
    pub entries: Vec<&'a str>,
    pub cited: bool,
}

/// Ordinary prose stays a word search. Explicit citations may name missing keys.
pub(super) fn entries(query: &str) -> Option<Entries<'_>> {
    if query.len() > 64 * 1024 {
        return None;
    }
    let mut found = Vec::new();
    let mut cited = false;
    let mut rest = query;
    while !rest.is_empty() {
        let offset = query.len() - rest.len();
        let before = &query[..offset];
        // DOI suffixes can themselves contain `@`, including after punctuation.
        if before
            .chars()
            .next_back()
            .is_none_or(|c| c.is_whitespace() || matches!(c, ',' | ';'))
        {
            let end = rest
                .find(|c: char| c.is_whitespace() || matches!(c, ',' | ';'))
                .unwrap_or(rest.len());
            if identifier::parse_query(&rest[..end]).is_some() {
                rest = &rest[end..];
                continue;
            }
        }
        if let Some(after) = rest.strip_prefix("#cite(") {
            cited = true;
            let (args, after) = delimited(after, '(', ')');
            let mut args = args.trim_start();
            while let Some(key) = args.strip_prefix('<') {
                let (key, after) = delimited(key, '<', '>');
                add(&mut found, key)?;
                args = after.trim_start().strip_prefix(',').unwrap_or(after).trim_start();
            }
            rest = after;
            continue;
        }
        if let Some(after) = rest.strip_prefix('\\') {
            let len = after
                .bytes()
                .take_while(|c| c.is_ascii_alphabetic() || *c == b'*')
                .count();
            if after[..len].to_ascii_lowercase().contains("cite") {
                let mut args = after[len..].trim_start();
                while let Some(after) = args.strip_prefix('[') {
                    args = delimited(after, '[', ']').1.trim_start();
                }
                if let Some(after) = args.strip_prefix('{') {
                    cited = true;
                    let (keys, after) = delimited(after, '{', '}');
                    for key in keys.split(',') {
                        add(&mut found, key)?;
                    }
                    rest = after;
                    continue;
                }
                // Even an unclosed optional argument is consumed once.
                rest = args;
                continue;
            }
        }
        if let Some(after) = rest.strip_prefix('@') {
            let previous = before.strip_suffix('-').unwrap_or(before).chars().next_back();
            if previous.is_none_or(|c| c.is_whitespace() || "[({;,\"'".contains(c)) {
                if let Some(after) = after.strip_prefix('{') {
                    cited = true;
                    let (key, after) = delimited(after, '{', '}');
                    add(&mut found, key)?;
                    rest = after;
                    continue;
                }
                let end = after
                    .find(|c: char| c.is_whitespace() || ";,[]{}()\"'".contains(c))
                    .unwrap_or(after.len());
                let key = after[..end].trim_end_matches(['.', ':', '!', '?']);
                if !key.is_empty() {
                    cited = true;
                    add(&mut found, key)?;
                    rest = &after[end..];
                    continue;
                }
            }
        }
        // Advance on a UTF-8 boundary; each delimiter scan consumes its input.
        rest = &rest[rest.chars().next()?.len_utf8()..];
    }
    if cited {
        return Some(Entries { entries: found, cited });
    }
    let separated = query.contains(['\n', '\r', '\t', ',', ';']);
    let mut found = Vec::new();
    for word in query.split(|c: char| c.is_whitespace() || matches!(c, ',' | ';')) {
        add(&mut found, word)?;
    }
    (found.len() >= 2 && (separated || found.iter().all(|entry| identifier::parse_query(entry).is_some()))).then_some(
        Entries {
            entries: found,
            cited: false,
        },
    )
}

fn add<'a>(entries: &mut Vec<&'a str>, entry: &'a str) -> Option<()> {
    let entry = entry.trim();
    if !entry.is_empty() {
        if entries.len() == 1000 {
            return None;
        }
        entries.push(entry);
    }
    Some(())
}

/// Unclosed citation syntax consumes the remaining input, without recursion.
fn delimited(text: &str, open: char, close: char) -> (&str, &str) {
    let mut depth = 1;
    for (index, c) in text.char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return (&text[..index], &text[index + c.len_utf8()..]);
            }
        }
    }
    (text, "")
}
