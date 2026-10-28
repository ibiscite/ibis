//! Scalar escaping shared by the old entrypoints and the contract renderer.

use crate::contract::{Locator, RichText};

use super::Escaping;

pub(crate) fn is_range(value: &str) -> bool {
    value.contains(['-', '–', '—', ','])
}

pub(crate) fn normalize_dashes(value: &str, dash: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if matches!(c, '-' | '–' | '—') {
            while chars.peek().is_some_and(|n| matches!(n, '-' | '–' | '—')) {
                chars.next();
            }
            out.push_str(dash);
        } else {
            out.push(c);
        }
    }
    out
}

pub(crate) fn pandoc_key(key: &str) -> String {
    const INNER: &str = ":.#$%&-+?<>~/";
    let chars: Vec<char> = key.chars().collect();
    // Use a conservative ASCII subset: Rust's Alphabetic also includes marks
    // and symbols that Pandoc's isAlphaNum rejects. Braces keep Unicode exact.
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let simple = chars.first().is_some_and(|c| word(*c))
        && chars.last().is_some_and(|c| word(*c))
        && chars.iter().all(|c| word(*c) || INNER.contains(*c))
        && chars.windows(2).all(|w| word(w[0]) || word(w[1]));
    if simple { key.to_string() } else { format!("{{{key}}}") }
}

pub(crate) fn pandoc_escape(text: &str) -> String {
    pandoc_text(text, false)
}

pub(crate) fn pandoc_affix_escape(text: &str) -> String {
    // Runs can meet at a boundary that would create -- or ...; quote each
    // literal punctuation character before the trusted style wrappers join.
    pandoc_text(text, true)
}

fn pandoc_text(text: &str, affix: bool) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    let mut previous = None;
    while let Some(c) = chars.next() {
        if matches!(
            c,
            '[' | ']'
                | ';'
                | '@'
                | '\\'
                | '*'
                | '_'
                | '`'
                | '$'
                | '<'
                | '>'
                | '^'
                | '~'
                | '&'
                | '#'
                | '|'
                | '{'
                | '}'
                | '\''
                | '"'
        ) || matches!(c, '-' | '.') && (affix || previous == Some(c) || chars.peek() == Some(&c))
        {
            out.push('\\');
        }
        out.push(c);
        previous = Some(c);
    }
    out
}

pub(crate) fn latex_escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        latex_character(&mut out, c);
    }
    out
}

/// Literal affixes must not activate TeX quote/dash ligatures or consumer smart
/// typography. Locators retain their intentional range-dash normalization.
pub(crate) fn latex_affix_escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '-' => out.push_str("{-}"),
            '.' => out.push_str("{.}"),
            '"' => out.push_str("\\textquotedbl{}"),
            '\'' => out.push_str("\\textquotesingle{}"),
            '`' => out.push_str("\\`{}"),
            _ => latex_character(&mut out, c),
        }
    }
    out
}

/// Keep intentional locator range dashes while preventing quote ligatures.
pub(crate) fn latex_locator_escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '"' | '\'' | '`' => out.push_str(&latex_affix_escape(&c.to_string())),
            _ => latex_character(&mut out, c),
        }
    }
    out
}

fn latex_character(out: &mut String, c: char) {
    match c {
        '\\' => out.push_str("\\textbackslash{}"),
        '~' => out.push_str("\\textasciitilde{}"),
        '^' => out.push_str("\\textasciicircum{}"),
        '#' | '$' | '%' | '&' | '_' | '{' | '}' => {
            out.push('\\');
            out.push(c);
        }
        '[' => out.push_str("{[}"),
        ']' => out.push_str("{]}"),
        _ => out.push(c),
    }
}

pub(crate) fn typst_simple_label(key: &str) -> bool {
    // This ASCII subset is accepted by Typst's XID-based label grammar.
    // Other keys retain their exact identity through the string constructor.
    key.chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ':' | '.'))
        && key
            .chars()
            .last()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub(crate) fn typst_markup_escape(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    let mut line_start = true;
    let mut numbered_prefix = false;
    while let Some(c) = chars.next() {
        if matches!(c, '[' | ']' | '#' | '@' | '*' | '_' | '\\' | '$' | '<' | '`')
            || (c == '/' && matches!(chars.peek(), Some('/' | '*')))
            || (line_start && matches!(c, '=' | '+' | '-' | '/'))
            || (numbered_prefix && c == '.')
        {
            out.push('\\');
        }
        out.push(c);
        // Both a prefix and a supplement can begin a markup block. Unicode
        // line/paragraph separators also start a new line in Typst.
        numbered_prefix = c.is_ascii_digit() && (line_start || numbered_prefix);
        if matches!(c, '\u{2028}' | '\u{2029}') {
            line_start = true;
        } else if !c.is_whitespace() {
            line_start = false;
        }
    }
    out
}

/// Whether `runs` can be written literally as an affix. Org mode and
/// MultiMarkdown have no reliable escape for some text; it is dropped with a
/// typed loss rather than written as markup or smart typography.
pub(crate) fn carries(escaping: Escaping, runs: &RichText, suffix: bool) -> bool {
    let text: String = runs.iter().map(|run| run.text.as_str()).collect();
    let text = text.trim();
    if text.is_empty() {
        return true;
    }
    match escaping {
        Escaping::Org => {
            // A suffix must not start like a locator, even after one: Org's
            // processors can absorb it as another locator value.
            if suffix && starts_like_locator(text) {
                return false;
            }
            org_literal(text)
        }
        Escaping::MultiMarkdown => mmd_literal(text),
        _ => true,
    }
}

fn starts_like_locator(text: &str) -> bool {
    let mut words = text.split_whitespace();
    let first = words.next().unwrap_or_default();
    first.starts_with(|c: char| c.is_ascii_digit())
        || crate::locator::parse(&format!("page {first}")).is_some()
        || crate::locator::parse(first).is_some()
        || words
            .next()
            .is_some_and(|second| crate::locator::parse(&format!("{first} {second}")).is_some())
}

/// Org reads emphasis only from a marker that opens a word, sub- and
/// superscripts from any `_` or `^`, and special strings such as `--`.
pub(crate) fn org_literal(text: &str) -> bool {
    let mut previous: Option<char> = None;
    for c in text.chars() {
        let opens = previous.is_none_or(|p| p.is_whitespace() || "-('{\"".contains(p));
        if ";[]@\\$^_{}<>".contains(c) || "*/=~+".contains(c) && opens {
            return false;
        }
        previous = Some(c);
    }
    !text.contains("--") && !text.contains("...")
}

/// MultiMarkdown applies Markdown, HTML and smart typography inside the
/// citation's locator brackets.
pub(crate) fn mmd_literal(text: &str) -> bool {
    !text.contains("--") && !text.contains("...") && !text.contains(|c| "[]\\*_`<>&^~$#\"'|".contains(c))
}

pub(crate) fn key_fits(escaping: Escaping, key: &str) -> bool {
    let forbidden: &[char] = match escaping {
        Escaping::Myst => &['`', ';', '<', '>'],
        Escaping::MultiMarkdown => &['[', ']', '\\', '^'],
        Escaping::AsciiDoc => &['(', ')', '[', ']'],
        Escaping::Liquid => &['%', '"', '\''],
        Escaping::Org => &[';', '[', ']', ',', '"', '\\', '='],
        _ => &[],
    };
    !key.contains(forbidden) && !(escaping == Escaping::Liquid && key.starts_with('-'))
}

pub(crate) fn locator_fits(escaping: Escaping, locator: &Locator) -> bool {
    let value = locator.value.trim();
    match escaping {
        Escaping::Org => org_literal(&normalize_dashes(value, "–")),
        Escaping::MultiMarkdown => mmd_literal(&normalize_dashes(value, "–")),
        // asciidoctor-bibtex treats its locator field exclusively as pages.
        Escaping::AsciiDoc => {
            locator.term == crate::contract::LocatorTerm::Page && !value.contains(['(', ')', '[', ']', ',', '{', '}'])
        }
        Escaping::Liquid => {
            !value.starts_with('-') && !value.contains(|c: char| c.is_whitespace() || "\"'%{}".contains(c))
        }
        _ => true,
    }
}

/// Text for an HTML attribute or element.
pub(crate) fn html_escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}
