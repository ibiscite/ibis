//! Renders arbitrary keys and notes through every key syntax and re-reads the
//! output with a small checker per syntax: user text must never reach the
//! output with a raw special character, and each key must come back exactly.
//! Input: one item per line; tab-separated key, prefix, locator, suffix, flags.

use ibis_core::cite::{CiteError, CiteItem, KeyFormat, Locator, LocatorLabel, render};

const FORMATS: [KeyFormat; 7] = [
    KeyFormat::Pandoc,
    KeyFormat::Autocite,
    KeyFormat::Parencite,
    KeyFormat::Textcite,
    KeyFormat::Cite,
    KeyFormat::Typst,
    KeyFormat::Keys,
];
const LABELS: [LocatorLabel; 9] = [
    LocatorLabel::Page,
    LocatorLabel::Chapter,
    LocatorLabel::Section,
    LocatorLabel::Paragraph,
    LocatorLabel::Figure,
    LocatorLabel::Line,
    LocatorLabel::Note,
    LocatorLabel::Volume,
    LocatorLabel::Verse,
];

fn items(input: &str) -> Vec<CiteItem> {
    let field = |s: Option<&str>| s.filter(|s| !s.is_empty()).map(str::to_string);
    input
        .split('\n')
        .take(8)
        .map(|line| {
            let mut f = line.split('\t');
            let (key, prefix, locator, suffix) = (f.next().unwrap_or_default(), f.next(), f.next(), f.next());
            let flags = f.next().unwrap_or_default();
            let label = LABELS[flags.chars().next().map_or(0, |c| c as usize % LABELS.len())];
            CiteItem {
                key: key.to_string(),
                prefix: field(prefix),
                locator: field(locator).map(|value| Locator { label, value }),
                suffix: field(suffix),
                suppress_author: flags.contains('!'),
            }
        })
        .collect()
}

/// The documented rejections, re-derived from the inputs.
fn expected_error(format: KeyFormat, items: &[CiteItem]) -> Option<CiteError> {
    if items.is_empty() {
        return Some(CiteError::Empty);
    }
    let latex = matches!(
        format,
        KeyFormat::Autocite | KeyFormat::Parencite | KeyFormat::Textcite | KeyFormat::Cite
    );
    for i in items {
        if i.key.is_empty()
            || i.key
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || ",{}".contains(c))
        {
            return Some(CiteError::InvalidKey(i.key.clone()));
        }
        if latex && i.key.contains(['%', '#', '\\', '~', '^', '$', '&']) {
            return Some(CiteError::KeyNotValidFor(i.key.clone(), "LaTeX"));
        }
        let texts = [
            i.prefix.as_deref(),
            i.suffix.as_deref(),
            i.locator.as_ref().map(|l| l.value.as_str()),
        ];
        if texts.into_iter().flatten().any(|t| t.chars().any(char::is_control)) {
            return Some(CiteError::ControlCharacter);
        }
    }
    None
}

/// Pandoc: `[`, items split by unescaped `;`, each with one unescaped `@key`
/// or `@{key}`; every other Markdown special in user text is backslashed.
fn check_pandoc(out: &str, keys: &[&str]) {
    let c: Vec<char> = out.chars().collect();
    assert!(c.first() == Some(&'[') && c.last() == Some(&']'), "unbracketed: {out}");
    let (mut i, mut found, mut semis) = (1, Vec::new(), 0);
    while i < c.len() - 1 {
        match c[i] {
            '\\' => i += 1,
            ';' => semis += 1,
            '@' => {
                let start = i + 1;
                let end = if c.get(start) == Some(&'{') {
                    i = start
                        + c[start..]
                            .iter()
                            .position(|&x| x == '}')
                            .unwrap_or_else(|| panic!("unclosed @{{ in {out}"));
                    i
                } else {
                    while c
                        .get(i + 1)
                        .is_some_and(|x| x.is_ascii_alphanumeric() || "_:.#$%&-+?<>~/".contains(*x))
                    {
                        i += 1;
                    }
                    i + 1
                };
                let key: String = c[start..end].iter().collect();
                found.push(key.trim_start_matches('{').to_string());
            }
            x if "[]*_`$<>^~&#|{}".contains(x) => panic!("raw {x:?} in Pandoc: {out}"),
            _ => {}
        }
        i += 1;
    }
    assert_eq!(semis + 1, keys.len(), "separators: {out}");
    assert_eq!(found, keys, "keys: {out}");
}

/// LaTeX optional argument: only escapes, the three text commands, balanced
/// braces and the locator's `~`; ends at an unbraced `]`.
fn latex_opt(c: &[char], mut i: usize, out: &str) -> usize {
    let mut depth = 0usize;
    loop {
        match *c.get(i).unwrap_or_else(|| panic!("unclosed [ in {out}")) {
            '\\' if c.get(i + 1).is_some_and(|x| x.is_ascii_alphabetic()) => {
                let name: String = c[i + 1..].iter().take_while(|x| x.is_ascii_alphabetic()).collect();
                assert!(
                    ["textbackslash", "textasciitilde", "textasciicircum"].contains(&name.as_str()),
                    "\\{name} in {out}"
                );
                i += name.len();
            }
            '\\' => {
                assert!(
                    c.get(i + 1).is_some_and(|x| "#$%&_{}".contains(*x)),
                    "bad escape in {out}"
                );
                i += 1;
            }
            '{' => depth += 1,
            '}' => depth = depth.checked_sub(1).unwrap_or_else(|| panic!("unbalanced }} in {out}")),
            ']' if depth == 0 => return i + 1,
            '[' if depth == 0 => panic!("raw [ in {out}"),
            x if "#$%&^_".contains(x) => panic!("raw {x:?} in LaTeX: {out}"),
            _ => {}
        }
        i += 1;
    }
}

/// LaTeX: `\name*` commands, optional arguments, then `{keys}`.
fn check_latex(out: &str, keys: &[&str]) {
    let c: Vec<char> = out.chars().collect();
    let (mut i, mut found) = (0, Vec::new());
    while i < c.len() {
        assert_eq!(c[i], '\\', "expected a command in {out}");
        i += 1 + c[i + 1..].iter().take_while(|x| x.is_ascii_alphabetic()).count();
        i += usize::from(c.get(i) == Some(&'*'));
        while matches!(c.get(i), Some('[' | '{')) {
            if c[i] == '[' {
                i = latex_opt(&c, i + 1, out);
            } else {
                let len = c[i + 1..]
                    .iter()
                    .position(|&x| x == '}')
                    .unwrap_or_else(|| panic!("unclosed key in {out}"));
                found.extend(
                    c[i + 1..i + 1 + len]
                        .iter()
                        .collect::<String>()
                        .split(',')
                        .map(str::to_string),
                );
                i += len + 2;
            }
        }
        if c[i..].starts_with(&[',', ' ']) {
            i += 2;
        }
    }
    assert_eq!(found, keys, "keys: {out}");
}

/// Typst markup text: specials and line-start markers are backslashed.
fn typst_text(c: &[char], mut i: usize, stop: &[char], mut line_start: bool, out: &str) -> usize {
    while i < c.len() && !stop.contains(&c[i]) {
        let x = c[i];
        if x == '\\' {
            i += 2;
            line_start = false;
            continue;
        }
        assert!(!"[]#@*_$<`".contains(x), "raw {x:?} in Typst: {out}");
        assert!(
            !(x == '/' && matches!(c.get(i + 1), Some('/' | '*'))),
            "comment in {out}"
        );
        assert!(!(line_start && "=+-/".contains(x)), "line-start {x:?} in {out}");
        if x.is_ascii_digit() && line_start {
            let digits = c[i..].iter().take_while(|d| d.is_ascii_digit()).count();
            assert!(c.get(i + digits) != Some(&'.'), "numbered list in {out}");
        }
        line_start = matches!(x, '\u{2028}' | '\u{2029}') || (line_start && x.is_whitespace());
        i += 1;
    }
    i
}

fn typst_supplement(c: &[char], i: usize, out: &str) -> usize {
    let end = typst_text(c, i, &[']'], true, out);
    assert_eq!(c.get(end), Some(&']'), "unclosed supplement in {out}");
    end + 1
}

/// Typst: optional prefix text, then `@key[...]` or `#cite(...)`, space-joined.
fn check_typst(out: &str, keys: &[&str]) {
    let c: Vec<char> = out.chars().collect();
    let (mut i, mut found) = (0, Vec::new());
    while i < c.len() {
        i = typst_text(&c, i, &['@', '#'], i == 0, out);
        let rest: String = c[i..].iter().collect();
        if c[i] == '@' {
            let len = c[i + 1..]
                .iter()
                .take_while(|x| x.is_ascii_alphanumeric() || "_-:.".contains(**x))
                .count();
            found.push(c[i + 1..i + 1 + len].iter().collect::<String>());
            i += 1 + len;
            if c.get(i) == Some(&'[') {
                i = typst_supplement(&c, i + 1, out);
            }
        } else if let Some(label) = rest.strip_prefix("#cite(<") {
            let len = label
                .chars()
                .position(|x| x == '>')
                .unwrap_or_else(|| panic!("unclosed label in {out}"));
            found.push(label.chars().take(len).collect());
            i += 7 + len + 1;
        } else {
            assert!(rest.starts_with("#cite(label(\""), "bad cite in {out}");
            i += 13;
            let mut key = String::new();
            while c[i] != '"' {
                if c[i] == '\\' {
                    i += 1;
                    assert!(matches!(c[i], '\\' | '"'), "string escape in {out}");
                }
                key.push(c[i]);
                i += 1;
            }
            found.push(key);
            i += 2;
        }
        if c[..i].ends_with(&['>']) || c[..i].ends_with(&['"', ')']) {
            let rest = |i: usize| c[i..].iter().collect::<String>();
            if rest(i).starts_with(", supplement: [") {
                i = typst_supplement(&c, i + 15, out);
            }
            if rest(i).starts_with(", form: \"year\"") {
                i += 14;
            }
            assert_eq!(c.get(i), Some(&')'), "unclosed #cite in {out}");
            i += 1;
        }
        if c.get(i) == Some(&' ') {
            i += 1;
        }
    }
    assert_eq!(found, keys, "keys: {out}");
}

pub fn check(data: &[u8]) {
    let input = String::from_utf8_lossy(data);
    let items = items(&input);
    check_items(&items);
}

pub fn check_items(items: &[CiteItem]) {
    let keys: Vec<&str> = items.iter().map(|i| i.key.as_str()).collect();
    for format in FORMATS {
        let result = render(format, items);
        let Ok(rendered) = result else {
            assert_eq!(result.err(), expected_error(format, items), "{format:?}");
            continue;
        };
        assert_eq!(expected_error(format, items), None, "{format:?} accepted invalid input");
        let out = rendered.text.as_str();
        assert!(!out.chars().any(char::is_control), "{format:?} control character");
        match format {
            KeyFormat::Pandoc => check_pandoc(out, &keys),
            KeyFormat::Typst => check_typst(out, &keys),
            KeyFormat::Keys => assert_eq!(out, keys.join(", ")),
            _ => check_latex(out, &keys),
        }
    }
}
