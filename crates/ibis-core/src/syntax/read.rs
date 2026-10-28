//! Literal key reading paired with `render`: no document evaluation or note interpretation.
use std::borrow::Cow;

use super::{CAPABILITIES, KeySyntax};

pub(crate) struct Citation<'a> {
    pub keys: Vec<Cow<'a, str>>,
    pub rest: &'a str,
    pub explicit: bool,
}

/// Read one written citation. The capabilities table selects the supported
/// syntaxes and TeX commands; the round-trip tests cover every table entry.
pub(crate) fn citation(text: &str) -> Option<Citation<'_>> {
    for caps in &CAPABILITIES {
        let read = match caps.syntax {
            KeySyntax::Biblatex | KeySyntax::Latex => caps.command.and_then(|command| {
                tex(text, command, false).or_else(|| {
                    (caps.syntax == KeySyntax::Biblatex)
                        .then(|| tex(text, command, true))
                        .flatten()
                })
            }),
            // These are the four command choices in render::natbib.
            KeySyntax::Natbib => ["citep", "citet", "citeauthor", "citeyearpar"]
                .into_iter()
                .find_map(|command| tex(text, command, false)),
            KeySyntax::Typst => typst(text),
            KeySyntax::OrgCite => org(text),
            KeySyntax::Myst => myst(text),
            KeySyntax::MultiMarkdown => mmd(text),
            KeySyntax::AsciiDoc => asciidoc(text),
            KeySyntax::Jupyter => html(text),
            KeySyntax::JekyllScholar => jekyll(text),
            KeySyntax::CslJson => json(text, "citation-key"),
            KeySyntax::CaywJson => json(text, "citationKey"),
            // Bare keys have no citation delimiters. Pandoc's generic brackets
            // run last so notes, HTML and JSON cannot become extra citations.
            KeySyntax::Pandoc | KeySyntax::Keys | KeySyntax::ScannableCite => None,
        };
        if read.is_some() {
            return read;
        }
    }
    pandoc(text)
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

fn tex<'a>(text: &'a str, command: &str, multi: bool) -> Option<Citation<'a>> {
    let mut rest = text.strip_prefix('\\')?.strip_prefix(command)?;
    if multi {
        rest = rest.strip_prefix('s')?;
    }
    rest = rest.strip_prefix('*').unwrap_or(rest).trim_start();
    let mut keys = Vec::new();
    loop {
        while let Some(after) = rest.strip_prefix('[') {
            rest = group(after, '[', ']', false).1.trim_start();
        }
        let Some(after) = rest.strip_prefix('{') else { break };
        let (body, after) = group(after, '{', '}', false);
        keys.extend(comma_keys(body));
        rest = after;
        if !multi {
            break;
        }
        rest = rest.trim_start();
    }
    // Even a damaged optional argument is consumed once, keeping scanning linear.
    (rest.len() < text.len() - command.len() - 1).then(|| explicit(keys, rest))
}

fn typst(text: &str) -> Option<Citation<'_>> {
    let after = text.strip_prefix("#cite(")?;
    let (mut args, rest) = group(after, '(', ')', true);
    let mut keys = Vec::new();
    loop {
        args = args.trim_start();
        if let Some(after) = args.strip_prefix('<') {
            let end = after.find('>').unwrap_or(after.len());
            keys.push(Cow::Borrowed(&after[..end]));
            args = after.get(end + 1..).unwrap_or("");
        } else if let Some(after) = args.strip_prefix("label(") {
            let (body, after) = group(after, '(', ')', true);
            let (key, tail) = string(body.trim_start())?;
            if !tail.trim().is_empty() {
                break; // Only a literal constructor, never a Typst expression.
            }
            keys.push(Cow::Owned(key));
            args = after;
        } else {
            break; // Named form/supplement arguments are notes, not keys.
        }
        match args.trim_start().strip_prefix(',') {
            Some(after) => args = after,
            None => break,
        }
    }
    Some(explicit(keys, rest))
}

/// Typst's string escapes, including the two emitted by render::quoted.
fn string(text: &str) -> Option<(String, &str)> {
    let mut rest = text.strip_prefix('"')?;
    let mut value = String::new();
    while let Some(c) = rest.chars().next() {
        rest = &rest[c.len_utf8()..];
        match c {
            '"' => return Some((value, rest)),
            '\\' => {
                let escaped = rest.chars().next()?;
                rest = &rest[escaped.len_utf8()..];
                value.push(match escaped {
                    '\\' | '"' => escaped,
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    'u' => {
                        let hex = rest.strip_prefix('{')?;
                        let end = hex.find('}')?;
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

fn org(text: &str) -> Option<Citation<'_>> {
    let after = text.strip_prefix("[cite")?;
    let colon = after.find(':')?;
    if !matches!(&after[..colon], "" | "/t" | "/a" | "/na") {
        return None;
    }
    let (body, rest) = group(&after[colon + 1..], '[', ']', false);
    let keys = body.split(';').filter_map(|item| {
        let (_, key) = item.split_once('@')?;
        let end = key.find(|c: char| c.is_whitespace() || c == ',').unwrap_or(key.len());
        Some(Cow::Borrowed(&key[..end]))
    });
    Some(explicit(keys, rest))
}

fn myst(text: &str) -> Option<Citation<'_>> {
    let after = text.strip_prefix("{cite:")?;
    let after = after.strip_prefix("p}`").or_else(|| after.strip_prefix("t}`"))?;
    let end = after.find('`').unwrap_or(after.len());
    Some(explicit(comma_keys(&after[..end]), after.get(end + 1..).unwrap_or("")))
}

fn mmd(text: &str) -> Option<Citation<'_>> {
    let (body, rest) = group(text.strip_prefix('[')?, '[', ']', false);
    let (key, rest) = match body.strip_prefix('#') {
        Some(key) => (key, rest.strip_prefix("[]").unwrap_or(rest)),
        None => group(rest.strip_prefix("[#")?, '[', ']', false),
    };
    Some(explicit([Cow::Borrowed(key)], rest))
}

fn asciidoc(text: &str) -> Option<Citation<'_>> {
    // Keys and page values cannot contain brackets, and backslashes are literal.
    let after = text.strip_prefix("cite:[")?;
    let end = after.find(']').unwrap_or(after.len());
    let (body, rest) = (&after[..end], after.get(end + 1..).unwrap_or(""));
    let keys = body
        .split(',')
        .map(|item| Cow::Borrowed(item.split('(').next().unwrap_or(item).trim()));
    Some(explicit(keys, rest))
}

fn html(text: &str) -> Option<Citation<'_>> {
    let after = text.strip_prefix("<cite data-cite=\"")?;
    let end = after.find('"')?;
    let key = entities(&after[..end])?;
    let rest = after[end + 1..]
        .find("</cite>")
        .map_or("", |i| &after[end + 1 + i + 7..]);
    Some(explicit([Cow::Owned(key)], rest))
}

fn entities(mut text: &str) -> Option<String> {
    let mut value = String::new();
    while let Some(start) = text.find('&') {
        value.push_str(&text[..start]);
        let after = &text[start + 1..];
        let end = after.find(';')?;
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

fn jekyll(text: &str) -> Option<Citation<'_>> {
    let after = text.strip_prefix("{% cite ")?;
    let end = after.find("%}").unwrap_or(after.len());
    let keys = after[..end]
        .split_whitespace()
        .take_while(|key| !key.starts_with("--"))
        .map(Cow::Borrowed);
    Some(explicit(keys, after.get(end + 2..).unwrap_or("")))
}

fn json<'a>(text: &'a str, field: &str) -> Option<Citation<'a>> {
    if !text.starts_with('[') {
        return None;
    }
    let mut stream = serde_json::Deserializer::from_str(text).into_iter::<Vec<serde_json::Value>>();
    let records = stream.next()?.ok()?;
    let keys: Option<Vec<_>> = records
        .iter()
        .map(|record| Some(Cow::Owned(record.get(field)?.as_str()?.to_owned())))
        .collect();
    Some(explicit(keys?, &text[stream.byte_offset()..]))
}

fn pandoc(text: &str) -> Option<Citation<'_>> {
    if let Some(after) = text.strip_prefix('@') {
        let (key, rest) = at_key(after);
        return Some(explicit([Cow::Borrowed(key)], rest));
    }
    let (mut body, rest) = pandoc_bracket(text.strip_prefix('[')?);
    let whole = body;
    let mut keys = Vec::new();
    // JSON strings and objects are not Markdown citations.
    if !body.trim_start().starts_with(['{', '"']) {
        while !body.is_empty() {
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
                let (key, after) = at_key(after);
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

fn pandoc_bracket(text: &str) -> (&str, &str) {
    let mut rest = text;
    let mut depth = 1;
    while let Some(c) = rest.chars().next() {
        // Braced keys are literal: neither brackets nor a trailing backslash
        // inside one can close or escape the surrounding citation bracket.
        if let Some(after) = rest.strip_prefix("@{") {
            let end = after.find('}').unwrap_or(after.len());
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

fn at_key(after: &str) -> (&str, &str) {
    if let Some(after) = after.strip_prefix('{') {
        // Braced Pandoc keys are literal, including backslashes. The writer
        // rejects braces in keys, so a backslash cannot hide their closing brace.
        let end = after.find('}').unwrap_or(after.len());
        return (&after[..end], after.get(end + 1..).unwrap_or(""));
    }
    let end = after
        .find(|c: char| c.is_whitespace() || ";,[]{}()\"'".contains(c))
        .unwrap_or(after.len());
    let key = after[..end].trim_end_matches(['.', ':', '!', '?']);
    // Attached Typst supplements belong to this citation, not the pasted list.
    let rest = &after[end..];
    (
        key,
        rest.strip_prefix('[')
            .map_or(rest, |body| group(body, '[', ']', true).1),
    )
}

/// Consume one group without recursion. TeX optional notes respect brace groups;
/// Typst strings and nested content blocks cannot close a code argument.
fn group(text: &str, open: char, close: char, quoted: bool) -> (&str, &str) {
    let mut depth = 1;
    let mut braces = 0_usize;
    let mut brackets = 0_usize;
    let mut string = false;
    let mut escaped = false;
    for (i, c) in text.char_indices() {
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
