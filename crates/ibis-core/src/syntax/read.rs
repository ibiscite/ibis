//! Literal key reading paired with `render`: no document evaluation or note interpretation.
use std::borrow::Cow;

use super::{CAPABILITIES, KeySyntax};
use crate::scan::{Stopped, Work};

pub(crate) struct Citation<'a> {
    pub keys: Vec<Cow<'a, str>>,
    pub rest: &'a str,
    pub explicit: bool,
}

/// Cheap starts only: lookahead must not run the readers at every prose byte.
pub(crate) fn starts(text: &str) -> bool {
    text.starts_with(['\\', '[', '@'])
        || ["-@", "#cite(", "{cite:", "{% cite ", "cite:[", "<cite data-cite=\""]
            .iter()
            .any(|head| text.starts_with(head))
}

/// Read one written citation. The capabilities table selects the supported
/// syntaxes and TeX commands; the round-trip tests cover every table entry.
pub(crate) fn citation<'a>(text: &'a str, work: &mut Work<'_>) -> Result<Option<Citation<'a>>, Stopped> {
    let read = select(text, work);
    work.check()?;
    Ok(read)
}

fn select<'a>(text: &'a str, work: &mut Work<'_>) -> Option<Citation<'a>> {
    // A literal MultiMarkdown note/key pair can start with an unmatched brace;
    // recognize that complete shape before treating it as damaged JSON.
    if let Some(read) = mmd(text, work) {
        return Some(read);
    }
    // JSON is a protected span. Its metadata never reaches Markdown readers,
    // including when the JSON is damaged or does not contain citation keys.
    if let Some(read) = json(text, work) {
        return Some(read);
    }
    for caps in &CAPABILITIES {
        if !work.spend(1) {
            return None;
        }
        let read = match caps.syntax {
            KeySyntax::Biblatex | KeySyntax::Latex => caps.command.and_then(|command| {
                tex(text, command, false, work).or_else(|| {
                    (caps.syntax == KeySyntax::Biblatex)
                        .then(|| tex(text, command, true, work))
                        .flatten()
                })
            }),
            KeySyntax::Natbib => ["citep", "citet", "citeauthor", "citeyearpar"]
                .into_iter()
                .find_map(|command| tex(text, command, false, work)),
            KeySyntax::Typst => typst(text, work),
            KeySyntax::OrgCite => org(text, work),
            KeySyntax::Myst => myst(text, work),
            KeySyntax::AsciiDoc => asciidoc(text, work),
            KeySyntax::Jupyter => html(text, work),
            KeySyntax::JekyllScholar => jekyll(text, work),
            KeySyntax::Pandoc
            | KeySyntax::MultiMarkdown
            | KeySyntax::Keys
            | KeySyntax::ScannableCite
            | KeySyntax::CslJson
            | KeySyntax::CaywJson => None,
        };
        if read.is_some() {
            return read;
        }
    }
    pandoc(text, work)
}

fn explicit<'a>(keys: impl IntoIterator<Item = Cow<'a, str>>, rest: &'a str) -> Citation<'a> {
    Citation {
        keys: keys.into_iter().filter(|key| !key.is_empty()).collect(),
        rest,
        explicit: true,
    }
}

fn comma_keys(text: &str) -> impl Iterator<Item = Cow<'_, str>> {
    text.split(',').map(|key| Cow::Borrowed(key.trim()))
}

fn tex<'a>(text: &'a str, command: &str, multi: bool, work: &mut Work<'_>) -> Option<Citation<'a>> {
    let mut rest = text.strip_prefix('\\')?.strip_prefix(command)?;
    if multi {
        rest = rest.strip_prefix('s')?;
    }
    rest = work.trim_start(rest.strip_prefix('*').unwrap_or(rest));
    let mut keys = Vec::new();
    loop {
        while let Some(after) = rest.strip_prefix('[') {
            let (_, after) = group(after, '[', ']', false, work);
            rest = work.trim_start(after);
        }
        let Some(after) = rest.strip_prefix('{') else { break };
        let (body, after) = group(after, '{', '}', false, work);
        if !work.spend(body.len()) {
            return None;
        }
        keys.extend(comma_keys(body));
        rest = after;
        if !multi {
            break;
        }
        rest = work.trim_start(rest);
    }
    // Even a damaged optional argument is consumed once, keeping scanning linear.
    (rest.len() < text.len() - command.len() - 1).then(|| explicit(keys, rest))
}

fn typst<'a>(text: &'a str, work: &mut Work<'_>) -> Option<Citation<'a>> {
    let after = text.strip_prefix("#cite(")?;
    let (mut args, rest) = group(after, '(', ')', true, work);
    let mut keys = Vec::new();
    loop {
        args = work.trim_start(args);
        if let Some(after) = args.strip_prefix('<') {
            let end = work.find(after, |c| c == '>').unwrap_or(after.len());
            keys.push(Cow::Borrowed(&after[..end]));
            args = after.get(end + 1..).unwrap_or("");
        } else if let Some(after) = args.strip_prefix("label(") {
            let (body, after) = group(after, '(', ')', true, work);
            let Some((key, tail)) = string(work.trim_start(body), work) else {
                break;
            };
            if !work.trim_start(tail).is_empty() {
                break; // Only a literal constructor, never a Typst expression.
            }
            keys.push(Cow::Owned(key));
            args = after;
        } else {
            break; // Named form/supplement arguments are notes, not keys.
        }
        match work.trim_start(args).strip_prefix(',') {
            Some(after) => args = after,
            None => break,
        }
    }
    Some(explicit(keys, rest))
}

/// Typst's string escapes, including the two emitted by render::quoted.
fn string<'a>(text: &'a str, work: &mut Work<'_>) -> Option<(String, &'a str)> {
    let mut rest = text.strip_prefix('"')?;
    let mut value = String::new();
    while let Some(c) = rest.chars().next() {
        if !work.spend(c.len_utf8()) {
            return None;
        }
        rest = &rest[c.len_utf8()..];
        match c {
            '"' => return Some((value, rest)),
            '\\' => {
                let escaped = rest.chars().next()?;
                if !work.spend(escaped.len_utf8()) {
                    return None;
                }
                rest = &rest[escaped.len_utf8()..];
                value.push(match escaped {
                    '\\' | '"' => escaped,
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    'u' => {
                        let hex = rest.strip_prefix('{')?;
                        let end = work.find(hex, |c| c == '}')?;
                        if !work.spend(end) {
                            return None;
                        }
                        let code = u32::from_str_radix(&hex[..end], 16).ok()?;
                        rest = &hex[end + 1..];
                        char::from_u32(code)?
                    }
                    _ => return None,
                });
            }
            _ => value.push(c),
        }
    }
    None
}

fn org<'a>(text: &'a str, work: &mut Work<'_>) -> Option<Citation<'a>> {
    let after = text.strip_prefix("[cite")?;
    let colon = work.find(after, |c| c == ':')?;
    if !matches!(&after[..colon], "" | "/t" | "/a" | "/na") {
        return None;
    }
    let (body, rest) = group(&after[colon + 1..], '[', ']', false, work);
    if !work.spend(body.len()) {
        return None;
    }
    let keys = body.split(';').filter_map(|item| {
        let (_, key) = item.split_once('@')?;
        let end = key.find(|c: char| c.is_whitespace() || c == ',').unwrap_or(key.len());
        Some(Cow::Borrowed(&key[..end]))
    });
    Some(explicit(keys, rest))
}

fn myst<'a>(text: &'a str, work: &mut Work<'_>) -> Option<Citation<'a>> {
    let after = text.strip_prefix("{cite:")?;
    let after = after.strip_prefix("p}`").or_else(|| after.strip_prefix("t}`"))?;
    let end = work.find(after, |c| c == '`').unwrap_or(after.len());
    if !work.spend(end) {
        return None;
    }
    Some(explicit(comma_keys(&after[..end]), after.get(end + 1..).unwrap_or("")))
}

fn mmd<'a>(text: &'a str, work: &mut Work<'_>) -> Option<Citation<'a>> {
    // MultiMarkdown notes and keys are flat brackets; braces are literal.
    let after = text.strip_prefix('[')?;
    let end = work.find(after, |c| matches!(c, '[' | ']')).unwrap_or(after.len());
    let body = &after[..end];
    let rest = &after[end..];
    let (key, rest) = match body.strip_prefix('#') {
        Some(key) => {
            // A recognized key may be truncated at EOF, but never at a '['.
            let rest = rest.strip_prefix(']').or_else(|| rest.is_empty().then_some(rest))?;
            (key, rest.strip_prefix("[]").unwrap_or(rest))
        }
        None => {
            let after = rest.strip_prefix(']')?.strip_prefix("[#")?;
            // In particular, JSON strings/objects cannot become notes even if
            // their metadata contains a closing bracket and citation marker.
            if !work.spend(body.len()) || !super::escape::mmd_literal(body) {
                return None;
            }
            let end = work.find(after, |c| matches!(c, '[' | ']')).unwrap_or(after.len());
            let rest = &after[end..];
            let rest = rest.strip_prefix(']').or_else(|| rest.is_empty().then_some(rest))?;
            (&after[..end], rest)
        }
    };
    Some(explicit([Cow::Borrowed(key)], rest))
}

fn asciidoc<'a>(text: &'a str, work: &mut Work<'_>) -> Option<Citation<'a>> {
    // Keys and page values cannot contain brackets, and backslashes are literal.
    let after = text.strip_prefix("cite:[")?;
    let end = work.find(after, |c| c == ']').unwrap_or(after.len());
    let (body, rest) = (&after[..end], after.get(end + 1..).unwrap_or(""));
    if !work.spend(body.len()) {
        return None;
    }
    let keys = body
        .split(',')
        .map(|item| Cow::Borrowed(item.split('(').next().unwrap_or(item).trim()));
    Some(explicit(keys, rest))
}

fn html<'a>(text: &'a str, work: &mut Work<'_>) -> Option<Citation<'a>> {
    let after = text.strip_prefix("<cite data-cite=\"")?;
    let end = work.find(after, |c| c == '"')?;
    let key = entities(&after[..end], work)?;
    let rest = work
        .find_str(&after[end + 1..], "</cite>")
        .map_or("", |i| &after[end + 1 + i + 7..]);
    Some(explicit([Cow::Owned(key)], rest))
}

fn entities(mut text: &str, work: &mut Work<'_>) -> Option<String> {
    let mut value = String::new();
    while let Some(start) = work.find(text, |c| c == '&') {
        value.push_str(&text[..start]);
        let after = &text[start + 1..];
        let end = work.find(after, |c| c == ';')?;
        value.push(match &after[..end] {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            numeric => {
                let code = if let Some(hex) = numeric.strip_prefix("#x") {
                    u32::from_str_radix(hex, 16).ok()?
                } else {
                    numeric.strip_prefix('#')?.parse().ok()?
                };
                char::from_u32(code)?
            }
        });
        text = &after[end + 1..];
    }
    value.push_str(text);
    Some(value)
}

fn jekyll<'a>(text: &'a str, work: &mut Work<'_>) -> Option<Citation<'a>> {
    let after = text.strip_prefix("{% cite ")?;
    let end = work.find_str(after, "%}").unwrap_or(after.len());
    if !work.spend(end) {
        return None;
    }
    let keys = after[..end]
        .split_whitespace()
        .take_while(|key| !key.starts_with("--"))
        .map(Cow::Borrowed);
    Some(explicit(keys, after.get(end + 2..).unwrap_or("")))
}

fn json<'a>(text: &'a str, work: &mut Work<'_>) -> Option<Citation<'a>> {
    let after = text.strip_prefix('[')?;
    let first = work.trim_start(after);
    if !first.starts_with(['{', '[', '"', ']']) {
        return None;
    }
    let (_, rest) = group(after, '[', ']', true, work);
    let span = &text[..text.len() - rest.len()];
    if !work.spend(span.len()) {
        return None;
    }
    let records = serde_json::from_str::<Vec<serde_json::Value>>(span).ok();
    // Nested Pandoc brackets can start with [[ too. Only valid JSON protects
    // that shape; damaged arrays of objects/strings still protect metadata.
    if records.is_none() && first.starts_with('[') {
        return None;
    }
    let keys = records.and_then(|records| {
        records
            .iter()
            .map(|record| {
                record
                    .get("citation-key")
                    .or_else(|| record.get("citationKey"))?
                    .as_str()
                    .map(|key| Cow::Owned(key.to_owned()))
            })
            .collect::<Option<Vec<_>>>()
    });
    Some(Citation {
        explicit: keys.is_some(),
        keys: keys.unwrap_or_default(),
        rest,
    })
}

fn pandoc<'a>(text: &'a str, work: &mut Work<'_>) -> Option<Citation<'a>> {
    if let Some(after) = text.strip_prefix('@') {
        let (key, rest) = at_key(after, work);
        return Some(explicit([Cow::Borrowed(key)], rest));
    }
    let (mut body, rest) = pandoc_bracket(text.strip_prefix('[')?, work);
    let whole = body;
    let mut keys = Vec::new();
    // JSON strings and objects are not Markdown citations.
    if !body.trim_start().starts_with(['{', '"']) {
        while !body.is_empty() {
            if !work.spend(1) {
                return None;
            }
            if let Some(after) = body.strip_prefix('\\') {
                body = after.chars().next().map_or("", |c| &after[c.len_utf8()..]);
            } else if let Some(after) = body.strip_prefix('@').filter(|_| {
                let before = &whole[..whole.len() - body.len()];
                before
                    .strip_suffix('-')
                    .unwrap_or(before)
                    .chars()
                    .next_back()
                    .is_none_or(|c| c.is_whitespace() || "[({;,\"'".contains(c))
            }) {
                let (key, after) = at_key(after, work);
                keys.push(Cow::Borrowed(key));
                body = after;
            } else {
                body = &body[body.chars().next()?.len_utf8()..];
            }
        }
    }
    let explicit = !keys.is_empty();
    Some(Citation { keys, rest, explicit })
}

fn pandoc_bracket<'a>(text: &'a str, work: &mut Work<'_>) -> (&'a str, &'a str) {
    let mut rest = text;
    let mut depth = 1;
    while let Some(c) = rest.chars().next() {
        if !work.spend(c.len_utf8()) {
            return (text, "");
        }
        // Braced keys are literal: neither brackets nor a trailing backslash
        // inside one can close or escape the surrounding citation bracket.
        if let Some(after) = rest.strip_prefix("@{") {
            let end = work.find(after, |c| c == '}').unwrap_or(after.len());
            rest = after.get(end + 1..).unwrap_or("");
            continue;
        }
        let offset = text.len() - rest.len();
        rest = &rest[c.len_utf8()..];
        match c {
            '\\' => rest = rest.chars().next().map_or("", |c| &rest[c.len_utf8()..]),
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return (&text[..offset], rest);
                }
            }
            _ => {}
        }
    }
    (text, "")
}

fn at_key<'a>(after: &'a str, work: &mut Work<'_>) -> (&'a str, &'a str) {
    if let Some(after) = after.strip_prefix('{') {
        // Braced Pandoc keys are literal, including backslashes. The writer
        // rejects braces in keys, so a backslash cannot hide their closing brace.
        let end = work.find(after, |c| c == '}').unwrap_or(after.len());
        return (&after[..end], after.get(end + 1..).unwrap_or(""));
    }
    let end = work
        .find(after, |c: char| c.is_whitespace() || ";,[]{}()\"'".contains(c))
        .unwrap_or(after.len());
    let key = after[..end].trim_end_matches(['.', ':', '!', '?']);
    // Attached Typst supplements belong to this citation, not the pasted list.
    let rest = &after[end..];
    (
        key,
        rest.strip_prefix('[')
            .map_or(rest, |body| group(body, '[', ']', true, work).1),
    )
}

/// Consume one group without recursion. TeX optional notes respect brace groups;
/// Typst strings and nested content blocks cannot close a code argument.
fn group<'a>(text: &'a str, open: char, close: char, quoted: bool, work: &mut Work<'_>) -> (&'a str, &'a str) {
    let mut depth = 1;
    let mut braces = 0_usize;
    let mut brackets = 0_usize;
    let mut string = false;
    let mut escaped = false;
    for (i, c) in text.char_indices() {
        if !work.spend(c.len_utf8()) {
            return (text, "");
        }
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if quoted && c == '"' {
            string = !string;
            continue;
        }
        if string {
            continue;
        }
        if open == '[' {
            match c {
                '{' => braces += 1,
                '}' => braces = braces.saturating_sub(1),
                _ => {}
            }
            if braces > 0 || c == '}' {
                continue;
            }
        }
        if quoted && open == '(' {
            match c {
                '[' => brackets += 1,
                ']' => brackets = brackets.saturating_sub(1),
                _ => {}
            }
            if brackets > 0 || c == ']' {
                continue;
            }
        }
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return (&text[..i], &text[i + c.len_utf8()..]);
            }
        }
    }
    (text, "")
}
