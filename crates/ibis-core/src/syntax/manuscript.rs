//! What a plain-text manuscript cites (decision 0060): Markdown and Quarto,
//! LaTeX and Typst, read in place by the same bounded readers as pasted text.
//!
//! Each reader first blanks what is not prose (front matter, code, comments,
//! verbatim), keeping every byte offset, then reads citations from what is
//! left. One work allowance covers the whole text: a pathological text stops
//! with a [`ScanProblem::TooLong`] problem and is reported as read only
//! in part, never as a clean check.

use super::marker::{self, Found};
use super::read::{group, pandoc_key_start, typst};
use crate::contract::{Cite, Cited, Manuscript, Place, ScanFinding, ScanProblem, TextKind};
use crate::scan::Work;

/// Read `text` as `kind`. `stop` is polled as the allowance is spent.
pub fn scan(kind: TextKind, text: &str, stop: &dyn Fn() -> bool) -> Manuscript {
    let mut work = Work::new(text.len().max(marker::MAX_MARKER_BYTES), stop);
    let mut places = Places::new(text);
    let mut out = Manuscript::default();
    match kind {
        TextKind::Markdown => {
            if let Some(masked) = mask_markdown(text, &mut work) {
                markdown(&masked, &mut work, &mut places, &mut out);
            }
        }
        TextKind::Latex => {
            if let Some(masked) = mask_latex(text, &mut work) {
                latex(&masked, &mut work, &mut places, &mut out);
            }
        }
        TextKind::Typst => typst_text(text, &mut work, &mut places, &mut out),
    }
    if work.check().is_err() && out.complete() {
        out.problems.push(ScanFinding {
            place: places.at(text.len()),
            kind: ScanProblem::TooLong,
        });
    }
    out
}

/// Byte offsets to lines and characters, for offsets given in order.
struct Places<'a> {
    text: &'a str,
    line: usize,
    offset: usize,
    column: usize,
}

impl<'a> Places<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            line: 1,
            offset: 0,
            column: 1,
        }
    }

    /// Offsets must not go backwards; a later one is never cheaper to find.
    fn at(&mut self, offset: usize) -> Place {
        let offset = offset.min(self.text.len());
        if offset < self.offset {
            *self = Self::new(self.text);
        }
        for c in self.text[self.offset..offset].chars() {
            if c == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
            self.offset += c.len_utf8();
        }
        Place {
            line: self.line,
            column: self.column,
        }
    }
}

/// Blank `range` of `bytes`, keeping line breaks. Every byte of a character
/// becomes a space, so offsets and character boundaries are kept.
fn blank(bytes: &mut [u8], range: std::ops::Range<usize>) {
    for byte in &mut bytes[range] {
        if *byte != b'\n' && *byte != b'\r' {
            *byte = b' ';
        }
    }
}

fn into_text(bytes: Vec<u8>) -> String {
    // Only whole characters were replaced by ASCII spaces.
    String::from_utf8(bytes).unwrap_or_default()
}

/// Lines with their byte offsets, each without its line break.
fn lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut at = 0;
    text.split_inclusive('\n').map(move |line| {
        let start = at;
        at += line.len();
        (start, line.trim_end_matches(['\n', '\r']))
    })
}

// ---- Markdown and Quarto ----

/// Cross-reference prefixes of Quarto and pandoc-crossref: `@fig-map` is a
/// figure, not a reference.
const CROSS_REFERENCES: [&str; 20] = [
    "fig", "tbl", "lst", "tip", "nte", "wrn", "imp", "cau", "thm", "lem", "cor", "prp", "cnj", "def", "exm", "exr",
    "sol", "rem", "eq", "sec",
];

fn cross_reference(key: &str) -> bool {
    key.split_once('-')
        .is_some_and(|(head, tail)| !tail.is_empty() && CROSS_REFERENCES.contains(&head))
}

fn mask_markdown(text: &str, work: &mut Work<'_>) -> Option<String> {
    let mut bytes = text.as_bytes().to_vec();
    let mut fence: Option<(u8, usize, usize)> = None;
    // Unclosed front matter is a rule and text, as Pandoc reads it.
    let mut front = text.starts_with("---\n") || text.starts_with("---\r\n");
    for (start, line) in lines(text) {
        if !work.spend(line.len() + 1) {
            return None;
        }
        let end = start + line.len();
        if front {
            if start > 0 && matches!(line.trim_end(), "---" | "...") {
                blank(&mut bytes, 0..end);
                front = false;
            }
            continue;
        }
        let indent = line.len() - line.trim_start_matches(' ').len();
        let trimmed = &line[indent..];
        let run = |mark: u8| trimmed.bytes().take_while(|&b| b == mark).count();
        match fence {
            Some((mark, length, from)) => {
                if indent < 4 && run(mark) >= length && trimmed[run(mark)..].trim().is_empty() {
                    blank(&mut bytes, from..end);
                    fence = None;
                }
            }
            None if indent < 4 => {
                for mark in *b"`~" {
                    let length = run(mark);
                    if length >= 3 && !(mark == b'`' && trimmed[length..].contains('`')) {
                        fence = Some((mark, length, start));
                    }
                }
            }
            None => {}
        }
    }
    if let Some((_, _, from)) = fence {
        let len = bytes.len();
        blank(&mut bytes, from..len);
    }
    let text = into_text(bytes);
    let mut bytes = text.as_bytes().to_vec();
    inline(&text, &mut bytes, work);
    work.check().ok()?;
    Some(into_text(bytes))
}

/// One left-to-right pass over what fenced blocks left: an HTML comment or a
/// `` `code` `` span, whichever opens first, so neither hides the other. A
/// span is a run of backticks closed by a run of the same length within its
/// paragraph; a length that found no closer there is not looked for again.
fn inline(text: &str, bytes: &mut [u8], work: &mut Work<'_>) {
    let ends: Vec<usize> = paragraphs(text)
        .into_iter()
        .map(|(start, block)| start + block.len())
        .collect();
    let mut unclosed: (usize, std::collections::HashSet<usize>) = (usize::MAX, Default::default());
    let mut at = 0;
    while let Some(offset) = work.find(&text[at..], |c| c == '`' || c == '<') {
        let from = at + offset;
        if text[from..].starts_with("<!--") {
            let end = work
                .find_str(&text[from + 4..], "-->")
                .map_or(text.len(), |close| from + 4 + close + 3);
            blank(bytes, from..end);
            at = end;
            continue;
        }
        if text.as_bytes()[from] == b'<' {
            at = from + 1;
            continue;
        }
        let length = text[from..].bytes().take_while(|&b| b == b'`').count();
        let body = from + length;
        let limit = ends
            .get(ends.partition_point(|end| *end < body))
            .copied()
            .unwrap_or(text.len());
        if unclosed.0 != limit {
            unclosed = (limit, Default::default());
        }
        let mut close = None;
        if !unclosed.1.contains(&length) {
            let mut i = body;
            while let Some(next) = text[i..limit].find('`') {
                let run_start = i + next;
                let run = text[run_start..limit].bytes().take_while(|&b| b == b'`').count();
                if !work.spend(run_start - i + run) {
                    return;
                }
                if run == length {
                    close = Some(run_start + run);
                    break;
                }
                i = run_start + run;
            }
        }
        match close {
            Some(end) => {
                blank(bytes, from..end);
                at = end;
            }
            None => {
                unclosed.1.insert(length);
                at = body;
            }
        }
    }
}

fn markdown(text: &str, work: &mut Work<'_>, places: &mut Places<'_>, out: &mut Manuscript) {
    // Pandoc citations never cross a paragraph, so a damaged one stops only its own.
    for (start, block) in paragraphs(text) {
        for found in marker::read(block, work) {
            match found {
                Found::Marker(mut found) => {
                    found.cites.retain(|cite| !cross_reference(&cite.key));
                    if found.cites.is_empty() {
                        continue;
                    }
                    out.citations.push(Cited {
                        place: places.at(start + found.start),
                        narrative: found.narrative,
                        cites: found.cites,
                    });
                }
                Found::Problem(problem) => out.problems.push(ScanFinding {
                    place: places.at(start + problem.start),
                    kind: problem.kind,
                }),
            }
        }
        if work.check().is_err() {
            return;
        }
    }
}

/// Runs of non-blank lines, with their byte offsets.
fn paragraphs(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut from: Option<usize> = None;
    for (start, line) in lines(text) {
        match (line.trim().is_empty(), from) {
            (true, Some(begin)) => {
                out.push((begin, text[begin..start].trim_end_matches(['\n', '\r'])));
                from = None;
            }
            (false, None) => from = Some(start),
            _ => {}
        }
    }
    if let Some(begin) = from {
        out.push((begin, &text[begin..]));
    }
    out
}

// ---- LaTeX ----

/// Environments whose body is never prose.
const VERBATIM: [&str; 6] = ["verbatim", "verbatim*", "lstlisting", "minted", "comment", "Verbatim"];

fn mask_latex(text: &str, work: &mut Work<'_>) -> Option<String> {
    let mut bytes = text.as_bytes().to_vec();
    let mut at = 0;
    while at < text.len() {
        let Some(offset) = work.find(&text[at..], |c| c == '%' || c == '\\') else {
            break;
        };
        let i = at + offset;
        if text.as_bytes()[i] == b'%' {
            let end = text[i..].find('\n').map_or(text.len(), |n| i + n);
            work.spend(end - i);
            blank(&mut bytes, i..end);
            at = end;
            continue;
        }
        let rest = &text[i + 1..];
        if let Some(after) = rest.strip_prefix("begin{")
            && let Some(end) = after.bytes().take(16).position(|byte| byte == b'}')
            && let name = &after[..end]
            && VERBATIM.contains(&name)
        {
            let close = format!("\\end{{{name}}}");
            let end = work
                .find_str(&text[i..], &close)
                .map_or(text.len(), |close_at| i + close_at + close.len());
            blank(&mut bytes, i..end);
            at = end;
        } else if let Some(after) = rest.strip_prefix("verb")
            && let Some(delimiter) = after.strip_prefix('*').unwrap_or(after).chars().next()
            && !delimiter.is_alphabetic()
            && !delimiter.is_whitespace()
        {
            let body = i + 1 + 4 + usize::from(after.starts_with('*')) + delimiter.len_utf8();
            let end = text[body..].find([delimiter, '\n']).map_or(text.len(), |close| {
                body + close + text[body + close..].chars().next().map_or(0, char::len_utf8)
            });
            work.spend(end - i);
            blank(&mut bytes, i..end);
            at = end;
        } else {
            // An escaped character, `\%` included, is literal text.
            at = i + 1 + rest.chars().next().map_or(0, char::len_utf8);
        }
    }
    work.check().ok()?;
    Some(into_text(bytes))
}

/// Commands that take one `{keys}` argument after up to two `[notes]`.
const TEX_SINGLE: [&str; 40] = [
    "cite",
    "Cite",
    "citep",
    "Citep",
    "citet",
    "Citet",
    "citealp",
    "Citealp",
    "citealt",
    "Citealt",
    "citeauthor",
    "Citeauthor",
    "citeyear",
    "citeyearpar",
    "citenum",
    "parencite",
    "Parencite",
    "footcite",
    "footcitetext",
    "textcite",
    "Textcite",
    "smartcite",
    "Smartcite",
    "autocite",
    "Autocite",
    "supercite",
    "citetitle",
    "Citetitle",
    "citedate",
    "citeurl",
    "fullcite",
    "footfullcite",
    "nocite",
    "notecite",
    "Notecite",
    "pnotecite",
    "Pnotecite",
    "fnotecite",
    "citeyearnp",
    "citelist",
];
/// BibLaTeX's multicite commands: `(pre)(post)` then `[pre][post]{key}`, repeated.
const TEX_MULTI: [&str; 13] = [
    "cites",
    "Cites",
    "parencites",
    "Parencites",
    "footcites",
    "footcitetexts",
    "smartcites",
    "Smartcites",
    "textcites",
    "Textcites",
    "supercites",
    "autocites",
    "Autocites",
];
/// `\volcite[pre]{volume}[pages]{key}`: the first braced argument is a volume.
const TEX_VOLUME: [&str; 7] = [
    "volcite", "Volcite", "pvolcite", "Pvolcite", "fvolcite", "tvolcite", "avolcite",
];
const TEX_NARRATIVE: [&str; 7] = [
    "citet",
    "Citet",
    "textcite",
    "Textcite",
    "textcites",
    "Textcites",
    "citeauthor",
];

/// One optional `open … close` group at the start of `rest`, consumed:
/// `None` when there is none, `Some(false)` when it runs to the end unclosed.
fn optional(rest: &mut &str, open: char, close: char, work: &mut Work<'_>) -> Option<bool> {
    let after = work.trim_start(rest).strip_prefix(open)?;
    let (body, next) = group(after, open, close, false, work);
    *rest = next;
    Some(body.len() < after.len())
}

fn latex(text: &str, work: &mut Work<'_>, places: &mut Places<'_>, out: &mut Manuscript) {
    let mut at = 0;
    while let Some(offset) = work.find(&text[at..], |c| c == '\\') {
        let start = at + offset;
        let name_len = text[start + 1..].bytes().take_while(u8::is_ascii_alphabetic).count();
        let name = &text[start + 1..start + 1 + name_len];
        let multi = TEX_MULTI.contains(&name);
        let volume = TEX_VOLUME.contains(&name);
        if name_len == 0 || !(multi || volume || TEX_SINGLE.contains(&name)) {
            at = start + 1 + name_len.max(1).min(text.len() - start - 1);
            continue;
        }
        let mut rest = &text[start + 1 + name_len..];
        rest = rest.strip_prefix('*').unwrap_or(rest);
        let mut keys = Vec::new();
        let mut unclosed = false;
        if multi {
            for _ in 0..2 {
                match optional(&mut rest, '(', ')', work) {
                    Some(true) => {}
                    Some(false) => unclosed = true,
                    None => break,
                }
            }
        }
        loop {
            for _ in 0..2 {
                if unclosed {
                    break;
                }
                match optional(&mut rest, '[', ']', work) {
                    Some(true) => {}
                    Some(false) => unclosed = true,
                    None => break,
                }
            }
            if unclosed {
                break;
            }
            let Some(open) = work.trim_start(rest).strip_prefix('{') else {
                break;
            };
            let (mut body, mut after) = group(open, '{', '}', false, work);
            let mut input = open;
            if volume {
                // Skip the volume and its optional pages; the key follows.
                rest = after;
                if let Some(pages) = work.trim_start(rest).strip_prefix('[') {
                    let (body, next) = group(pages, '[', ']', false, work);
                    if body.len() == pages.len() {
                        unclosed = true;
                        break;
                    }
                    rest = next;
                }
                let Some(key) = work.trim_start(rest).strip_prefix('{') else {
                    break;
                };
                (body, after) = group(key, '{', '}', false, work);
                input = key;
            }
            if body.len() == input.len() {
                unclosed = true;
                break;
            }
            keys.extend(body.split(',').map(str::trim).filter(|key| !key.is_empty()));
            rest = after;
            if !multi {
                break;
            }
        }
        if work.check().is_err() {
            return;
        }
        let place = places.at(start);
        if unclosed {
            out.problems.push(ScanFinding {
                place,
                kind: ScanProblem::Unclosed,
            });
            return;
        }
        if keys.contains(&"*") {
            out.cites_everything = true;
        }
        let cites: Vec<Cite> = keys
            .into_iter()
            .filter(|key| *key != "*")
            .map(|key| Cite {
                key: key.to_owned(),
                ..Cite::default()
            })
            .collect();
        if !cites.is_empty() {
            out.citations.push(Cited {
                place,
                narrative: TEX_NARRATIVE.contains(&name),
                cites,
            });
        }
        at = text.len() - rest.len();
    }
}

// ---- Typst ----

/// Keywords whose code runs to the end of the line.
const TYPST_LINE: [&str; 5] = ["let", "set", "show", "import", "include"];

/// Typst text with comments and raw text blanked, and the byte ranges that
/// are code, where `@` is never a reference. Code is what follows `#`: the
/// rest of a `#let`, `#set`, `#show`, `#import` or `#include` line, or a
/// call's parentheses; a content block (`[…]`) inside code is markup again.
/// Strings exist only in code.
struct Typst {
    masked: String,
    code: Vec<std::ops::Range<usize>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Code: parentheses and braces open in it, and whether a line or `;` ends it.
    Code { parens: usize, line: bool },
    /// A content block inside code: brackets open in it.
    Content { brackets: usize },
}

fn lex_typst(text: &str, work: &mut Work<'_>) -> Option<Typst> {
    let mut bytes = text.as_bytes().to_vec();
    let mut code = Vec::new();
    let mut code_from: Option<usize> = None;
    let mut stack: Vec<Mode> = Vec::new();
    let in_code = |stack: &[Mode]| matches!(stack.last(), Some(Mode::Code { .. }));
    let mut at = 0;
    while at < text.len() {
        if !work.spend(1) {
            return None;
        }
        let rest = &text[at..];
        let c = rest.chars().next().unwrap_or(' ');
        let code_now = in_code(&stack);
        // Comments and raw text are blanked in markup and code alike.
        if rest.starts_with("//") && (code_now || !text[..at].ends_with(':')) {
            let end = rest.find('\n').map_or(text.len(), |n| at + n);
            work.spend(end - at);
            blank(&mut bytes, at..end);
            at = end;
            continue;
        }
        if rest.starts_with("/*") {
            let mut depth = 0_usize;
            let mut end = text.len();
            let mut j = at;
            while j < text.len() {
                if !work.spend(1) {
                    return None;
                }
                if text[j..].starts_with("/*") {
                    depth += 1;
                    j += 2;
                } else if text[j..].starts_with("*/") {
                    depth -= 1;
                    j += 2;
                    if depth == 0 {
                        end = j;
                        break;
                    }
                } else {
                    j += text[j..].chars().next().map_or(1, char::len_utf8);
                }
            }
            blank(&mut bytes, at..end);
            at = end;
            continue;
        }
        if c == '$' {
            // Math has no citations, and its braces and quotes are not code's.
            // A string inside it is text, so its `$` does not close the math.
            let mut j = at + 1;
            let mut string = false;
            while let Some(d) = text[j..].chars().next() {
                if !work.spend(d.len_utf8()) {
                    return None;
                }
                // Comments in math hold no delimiters.
                if !string && (text[j..].starts_with("//") || text[j..].starts_with("/*")) {
                    let close = if text[j..].starts_with("//") { "\n" } else { "*/" };
                    j = work
                        .find_str(&text[j + 2..], close)
                        .map_or(text.len(), |end| j + 2 + end + close.len());
                    continue;
                }
                j += d.len_utf8();
                match d {
                    '\\' => j += text[j..].chars().next().map_or(0, char::len_utf8),
                    '"' => string = !string,
                    '$' if !string => break,
                    _ => {}
                }
            }
            let end = j.min(text.len());
            blank(&mut bytes, at..end);
            at = end;
            continue;
        }
        if c == '`' {
            let length = rest.bytes().take_while(|&b| b == b'`').count();
            let body = at + length;
            let end = if length == 2 {
                body // An empty raw span.
            } else {
                work.find_str(&text[body..], &rest[..length])
                    .map_or(text.len(), |close| body + close + length)
            };
            blank(&mut bytes, at..end);
            at = end;
            continue;
        }
        let mut next = at + c.len_utf8();
        match (stack.last().copied(), c) {
            (Some(Mode::Code { .. }), '"') => {
                // A string, to its unescaped closing quote.
                let mut j = at + 1;
                while let Some(d) = text[j..].chars().next() {
                    if !work.spend(d.len_utf8()) {
                        return None;
                    }
                    j += d.len_utf8();
                    match d {
                        '\\' => j += text[j..].chars().next().map_or(0, char::len_utf8),
                        '"' => break,
                        _ => {}
                    }
                }
                next = j.min(text.len());
            }
            (Some(Mode::Code { parens, line }), '(' | '{') => {
                stack.pop();
                stack.push(Mode::Code {
                    parens: parens + 1,
                    line,
                });
            }
            (Some(Mode::Code { parens, line }), ')' | '}') => {
                stack.pop();
                if parens > 1 || line {
                    stack.push(Mode::Code {
                        parens: parens.saturating_sub(1),
                        line,
                    });
                }
            }
            (Some(Mode::Code { parens: 0, line: true }), '\n' | ';') => {
                stack.pop();
            }
            (Some(Mode::Code { .. }), '[') => stack.push(Mode::Content { brackets: 0 }),
            (Some(Mode::Content { brackets }), '[') => {
                stack.pop();
                stack.push(Mode::Content { brackets: brackets + 1 });
            }
            (Some(Mode::Content { brackets }), ']') => {
                stack.pop();
                if brackets > 0 {
                    stack.push(Mode::Content { brackets: brackets - 1 });
                }
            }
            (_, '\\') if !code_now => next += text[next..].chars().next().map_or(0, char::len_utf8),
            (_, '#') if !code_now => {
                let name_len = text[next..]
                    .find(|d: char| !(d.is_alphanumeric() || d == '_' || d == '-' || d == '.'))
                    .unwrap_or(text.len() - next);
                let name = &text[next..next + name_len];
                if TYPST_LINE.contains(&name) {
                    stack.push(Mode::Code { parens: 0, line: true });
                } else if name_len > 0 && text[next + name_len..].starts_with('(') {
                    stack.push(Mode::Code { parens: 0, line: false });
                }
                next += name_len;
            }
            _ => {}
        }
        if stack.len() > 256 {
            // Too deep to follow: stop, so the text reads as read in part.
            work.spend(usize::MAX);
            return None;
        }
        match (code_from, in_code(&stack)) {
            // Code starts after `#name`, so the call itself is markup's.
            (None, true) => code_from = Some(next),
            (Some(from), false) => {
                code.push(from..next);
                code_from = None;
            }
            _ => {}
        }
        at = next;
    }
    if let Some(from) = code_from {
        code.push(from..text.len());
    }
    work.check().ok()?;
    Some(Typst {
        masked: into_text(bytes),
        code,
    })
}

fn label_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':')
}

/// Whether `offset` is in code; offsets are asked in order.
struct Ranges<'a> {
    ranges: &'a [std::ops::Range<usize>],
    next: usize,
}

impl Ranges<'_> {
    fn contains(&mut self, offset: usize) -> bool {
        while self.ranges.get(self.next).is_some_and(|range| range.end <= offset) {
            self.next += 1;
        }
        self.ranges.get(self.next).is_some_and(|range| range.contains(&offset))
    }
}

fn typst_text(text: &str, work: &mut Work<'_>, places: &mut Places<'_>, out: &mut Manuscript) {
    let Some(lexed) = lex_typst(text, work) else {
        return;
    };
    let masked = lexed.masked.as_str();
    // Labels the document defines in markup (`<intro>`): `@intro` refers to them.
    let mut labels = std::collections::BTreeSet::new();
    let mut code = Ranges {
        ranges: &lexed.code,
        next: 0,
    };
    let mut at = 0;
    while let Some(offset) = work.find(&masked[at..], |c| c == '<') {
        let start = at + offset + 1;
        let length = masked[start..]
            .find(|c: char| !label_char(c))
            .unwrap_or(masked.len() - start);
        if length > 0 && masked[start + length..].starts_with('>') && !code.contains(start - 1) {
            labels.insert(&masked[start..start + length]);
        }
        at = start + length;
    }
    let mut code = Ranges {
        ranges: &lexed.code,
        next: 0,
    };
    let mut at = 0;
    while let Some(offset) = work.find(&masked[at..], |c| c == '@' || c == '#' || c == '\\') {
        let start = at + offset;
        let rest = &masked[start..];
        if let Some(escaped) = rest.strip_prefix('\\') {
            at = start + 1 + escaped.chars().next().map_or(0, char::len_utf8);
            continue;
        }
        if rest.starts_with('#') {
            if code.contains(start) {
                at = start + 1;
                continue;
            }
            match rest.starts_with("#cite(").then(|| typst(rest, work)).flatten() {
                Some(read) => {
                    let cites: Vec<Cite> = read
                        .keys
                        .iter()
                        .map(|key| Cite {
                            key: key.to_string(),
                            ..Cite::default()
                        })
                        .collect();
                    if !cites.is_empty() {
                        out.citations.push(Cited {
                            place: places.at(start),
                            narrative: false,
                            cites,
                        });
                    }
                    at = masked.len() - read.rest.len();
                }
                None => at = start + 1,
            }
            continue;
        }
        if code.contains(start) || !pandoc_key_start(&masked[..start]) {
            at = start + 1;
            continue;
        }
        let length = masked[start + 1..]
            .find(|c: char| !label_char(c))
            .unwrap_or(masked.len() - start - 1);
        let key = masked[start + 1..start + 1 + length].trim_end_matches(['.', ':']);
        if !key.is_empty() && !labels.contains(key) {
            out.citations.push(Cited {
                place: places.at(start),
                narrative: true,
                cites: vec![Cite {
                    key: key.to_owned(),
                    ..Cite::default()
                }],
            });
        }
        at = start + 1 + key.len().max(1).min(masked.len() - start - 1);
        if work.check().is_err() {
            return;
        }
    }
}

#[cfg(test)]
#[path = "manuscript_tests.rs"]
mod tests;
