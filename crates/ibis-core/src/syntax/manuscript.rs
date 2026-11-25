//! What a plain-text manuscript cites (decision 0060): Markdown and Quarto,
//! LaTeX and Typst, read in place by the same bounded readers as pasted text.
//!
//! Each reader first blanks what is not prose (front matter, code, comments,
//! verbatim), keeping every byte offset, then reads citations from what is
//! left. One work allowance covers the whole text: a pathological text stops
//! with a [`MarkerProblemKind::TooLong`] problem and is reported as read only
//! in part, never as a clean check.

use super::marker::{self, Found, MarkerCite, MarkerProblemKind};
use super::read::{group, pandoc_key_start, typst};
use crate::scan::Work;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The largest manuscript read, in bytes.
pub const MAX_TEXT_BYTES: usize = 16 * 1024 * 1024;

/// How a manuscript writes its citations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TextKind {
    /// Markdown, Quarto and R Markdown: Pandoc's `[@key, p. 4]` and `@key`.
    Markdown,
    /// LaTeX with BibTeX, natbib or BibLaTeX commands.
    Latex,
    /// Typst's `@key` and `#cite(<key>)`.
    Typst,
}

impl TextKind {
    /// The kind a file's extension names.
    pub fn of(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        Some(match extension.as_str() {
            "md" | "markdown" | "qmd" | "rmd" => Self::Markdown,
            "tex" | "ltx" => Self::Latex,
            "typ" => Self::Typst,
            _ => return None,
        })
    }
}

/// Where something is, as a person finds it in an editor: line and
/// character, both from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct Place {
    pub line: usize,
    pub column: usize,
}

/// One citation, with its references in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cited {
    pub place: Place,
    /// The author is part of the sentence (`@key`, `\textcite`).
    pub narrative: bool,
    pub cites: Vec<MarkerCite>,
}

/// Text that starts a citation but is not one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Problem {
    pub place: Place,
    pub kind: MarkerProblemKind,
}

/// A manuscript's citations and problems, in document order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Manuscript {
    pub citations: Vec<Cited>,
    pub problems: Vec<Problem>,
    /// `\nocite{*}`: every entry of the bibliography counts as cited.
    pub cites_everything: bool,
}

impl Manuscript {
    /// Whether the whole text was read: no limit or cancellation stopped it.
    pub fn complete(&self) -> bool {
        !self
            .problems
            .iter()
            .any(|problem| problem.kind == MarkerProblemKind::TooLong)
    }
}

/// Read `text` as `kind`. `stop` is polled as the allowance is spent.
pub fn scan(kind: TextKind, text: &str, stop: &dyn Fn() -> bool) -> Manuscript {
    let mut work = Work::new(text.len().max(marker::MAX_MARKER_BYTES), stop);
    let mut places = Places::new(text);
    let mut out = Manuscript::default();
    let masked = match kind {
        TextKind::Markdown => mask_markdown(text, &mut work),
        TextKind::Latex => mask_latex(text, &mut work),
        TextKind::Typst => mask_typst(text, &mut work),
    };
    if let Some(masked) = masked {
        match kind {
            TextKind::Markdown => markdown(&masked, &mut work, &mut places, &mut out),
            TextKind::Latex => latex(&masked, &mut work, &mut places, &mut out),
            TextKind::Typst => typst_text(text, &masked, &mut work, &mut places, &mut out),
        }
    }
    if work.check().is_err() && out.complete() {
        out.problems.push(Problem {
            place: places.at(text.len()),
            kind: MarkerProblemKind::TooLong,
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
    // Comments, then code spans, outside fenced blocks.
    let mut at = 0;
    while let Some(offset) = work.find_str(&text[at..], "<!--") {
        let from = at + offset;
        let end = work
            .find_str(&text[from + 4..], "-->")
            .map_or(text.len(), |close| from + 4 + close + 3);
        blank(&mut bytes, from..end);
        at = end;
    }
    let text = into_text(bytes);
    let mut bytes = text.as_bytes().to_vec();
    code_spans(&text, &mut bytes, work);
    work.check().ok()?;
    Some(into_text(bytes))
}

/// Blank `` `code` `` spans: a run of backticks closed by a run of the same
/// length within the paragraph.
fn code_spans(text: &str, bytes: &mut [u8], work: &mut Work<'_>) {
    let mut at = 0;
    while let Some(offset) = work.find(&text[at..], |c| c == '`') {
        let from = at + offset;
        let length = text[from..].bytes().take_while(|&b| b == b'`').count();
        let body = from + length;
        let paragraph_end = work
            .find_str(&text[body..], "\n\n")
            .map_or(text.len(), |end| body + end);
        let mut close = None;
        let mut i = body;
        while let Some(next) = text[i..paragraph_end].find('`') {
            let run_start = i + next;
            let run = text[run_start..].bytes().take_while(|&b| b == b'`').count();
            if !work.spend(run_start - i + run) {
                return;
            }
            if run == length {
                close = Some(run_start + run);
                break;
            }
            i = run_start + run;
        }
        match close {
            Some(end) => {
                blank(bytes, from..end);
                at = end;
            }
            None => at = body,
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
                Found::Problem(problem) => out.problems.push(Problem {
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
            && let Some(name) = after.split_once('}').map(|(name, _)| name)
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
            let end = text[body..]
                .find([delimiter, '\n'])
                .map_or(text.len(), |close| body + close + 1)
                .min(text.len());
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
                match work.trim_start(rest).strip_prefix('(') {
                    Some(after) => rest = group(after, '(', ')', false, work).1,
                    None => break,
                }
            }
        }
        loop {
            for _ in 0..2 {
                match work.trim_start(rest).strip_prefix('[') {
                    Some(after) => rest = group(after, '[', ']', false, work).1,
                    None => break,
                }
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
                    rest = group(pages, '[', ']', false, work).1;
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
            out.problems.push(Problem {
                place,
                kind: MarkerProblemKind::Unclosed,
            });
            return;
        }
        if keys.contains(&"*") {
            out.cites_everything = true;
        }
        let cites: Vec<MarkerCite> = keys
            .into_iter()
            .filter(|key| *key != "*")
            .map(|key| MarkerCite {
                key: key.to_owned(),
                ..MarkerCite::default()
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

fn mask_typst(text: &str, work: &mut Work<'_>) -> Option<String> {
    let mut bytes = text.as_bytes().to_vec();
    let mut at = 0;
    while at < text.len() {
        let Some(offset) = work.find(&text[at..], |c| matches!(c, '/' | '`' | '\\' | '"')) else {
            break;
        };
        let i = at + offset;
        let rest = &text[i..];
        if rest.starts_with("//") && !text[..i].ends_with(':') {
            let end = rest.find('\n').map_or(text.len(), |n| i + n);
            work.spend(end - i);
            blank(&mut bytes, i..end);
            at = end;
        } else if rest.starts_with("/*") {
            // Block comments nest.
            let mut depth = 0_usize;
            let mut end = text.len();
            let mut j = i;
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
            blank(&mut bytes, i..end);
            at = end;
        } else if rest.starts_with('`') {
            let length = rest.bytes().take_while(|&b| b == b'`').count();
            let fence = &rest[..length];
            let body = i + length;
            let end = if length == 2 {
                body // An empty raw span.
            } else {
                work.find_str(&text[body..], fence)
                    .map_or(text.len(), |close| body + close + length)
            };
            blank(&mut bytes, i..end);
            at = end;
        } else if rest.starts_with('"') && text[..i].ends_with(['(', ',', ' ', '=']) && in_code(&text[..i]) {
            // A string inside code: `@` there is not a reference.
            let end = rest[1..]
                .find(['"', '\n'])
                .map_or(text.len(), |close| i + 1 + close + 1);
            work.spend(end - i);
            blank(&mut bytes, i..end);
            at = end;
        } else {
            at = i
                + 1
                + rest[1..]
                    .chars()
                    .next()
                    .filter(|_| rest.starts_with('\\'))
                    .map_or(0, char::len_utf8);
        }
    }
    work.check().ok()?;
    Some(into_text(bytes))
}

/// Whether the text before a quote is inside a `#function(` call on its line.
fn in_code(before: &str) -> bool {
    let line = before.rsplit('\n').next().unwrap_or(before);
    line.rfind('#').is_some_and(|hash| line[hash..].contains('('))
}

fn label_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':')
}

/// Labels the document defines (`<intro>`): `@intro` refers to them.
fn labels(text: &str, work: &mut Work<'_>) -> std::collections::BTreeSet<String> {
    let mut labels = std::collections::BTreeSet::new();
    let mut at = 0;
    while let Some(offset) = work.find(&text[at..], |c| c == '<') {
        let start = at + offset + 1;
        let length = text[start..]
            .find(|c: char| !label_char(c))
            .unwrap_or(text.len() - start);
        if length > 0 && text[start + length..].starts_with('>') && !text[..start - 1].ends_with("#cite(") {
            labels.insert(text[start..start + length].to_owned());
        }
        at = start + length;
    }
    labels
}

/// `text` is the original, for `#cite(` arguments whose strings `masked` blanked.
fn typst_text(original: &str, text: &str, work: &mut Work<'_>, places: &mut Places<'_>, out: &mut Manuscript) {
    let labels = labels(text, work);
    let mut at = 0;
    while let Some(offset) = work.find(&text[at..], |c| c == '@' || c == '#' || c == '\\') {
        let start = at + offset;
        let rest = &text[start..];
        if let Some(escaped) = rest.strip_prefix('\\') {
            at = start + 1 + escaped.chars().next().map_or(0, char::len_utf8);
            continue;
        }
        if rest.starts_with('#') {
            match rest
                .starts_with("#cite(")
                .then(|| typst(&original[start..], work))
                .flatten()
            {
                Some(read) => {
                    let cites: Vec<MarkerCite> = read
                        .keys
                        .iter()
                        .map(|key| MarkerCite {
                            key: key.to_string(),
                            ..MarkerCite::default()
                        })
                        .collect();
                    if !cites.is_empty() {
                        out.citations.push(Cited {
                            place: places.at(start),
                            narrative: false,
                            cites,
                        });
                    }
                    at = original.len() - read.rest.len();
                }
                None => at = start + 1,
            }
            continue;
        }
        if !pandoc_key_start(&text[..start]) {
            at = start + 1;
            continue;
        }
        let length = text[start + 1..]
            .find(|c: char| !label_char(c))
            .unwrap_or(text.len() - start - 1);
        let key = text[start + 1..start + 1 + length].trim_end_matches(['.', ':']);
        if !key.is_empty() && !labels.contains(key) {
            out.citations.push(Cited {
                place: places.at(start),
                narrative: true,
                cites: vec![MarkerCite {
                    key: key.to_owned(),
                    ..MarkerCite::default()
                }],
            });
        }
        at = start + 1 + key.len().max(1).min(text.len() - start - 1);
        if work.check().is_err() {
            return;
        }
    }
}

#[cfg(test)]
#[path = "manuscript_tests.rs"]
mod tests;
