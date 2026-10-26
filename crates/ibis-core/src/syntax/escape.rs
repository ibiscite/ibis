//! Scalar escaping shared by the old entrypoints and the contract renderer.

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
    let mut out = String::new();
    for c in text.chars() {
        if matches!(
            c,
            '[' | ']' | ';' | '@' | '\\' | '*' | '_' | '`' | '$' | '<' | '>' | '^' | '~' | '&' | '#' | '|' | '{' | '}'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

pub(crate) fn latex_escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
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
    out
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
