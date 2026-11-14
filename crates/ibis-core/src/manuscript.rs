//! Pure extraction from literal TeX, without macro expansion or file access.
//! Only a complete scan returns occurrences; the first diagnostic discards them.
//! Resolution and record export belong to the existing engine paths.

use std::{borrow::Cow, ops::Range};

use crate::{
    contract::KeySyntax,
    scan::{Stopped, Work},
    syntax,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence<'a> {
    pub key: &'a str,
    /// Half-open UTF-8 byte offsets into the supplied text, excluding whitespace.
    pub range: Range<usize>,
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_bytes: usize,
    pub max_occurrences: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 16 * 1024 * 1024,
            max_occurrences: 100_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScanError {
    #[error("incomplete literal TeX construct at {range:?}")]
    Incomplete { range: Range<usize> },
    #[error("TeX construct requires unsupported interpretation at {range:?}")]
    Unsupported { range: Range<usize> },
    #[error("manuscript scan cancelled")]
    Cancelled,
    #[error("manuscript scan limit exhausted")]
    Limit,
}

impl From<Stopped> for ScanError {
    fn from(stopped: Stopped) -> Self {
        match stopped {
            Stopped::Cancelled => Self::Cancelled,
            Stopped::Limit => Self::Limit,
        }
    }
}

/// Read literal `\cite`, `\citep`, `\citet`, `\citeauthor`, `\citeyearpar`,
/// `\autocite`, `\parencite` and `\textcite`, including stars and square-bracket
/// notes; the last three also support plural note/key groups. Preserve written
/// order and repetitions. Other manuscript formats are ordinary text.
///
/// Comments, `\verb`/`\Verb` and common verbatim environments are excluded.
/// Definitions, includes, conditionals, unknown citation commands (including
/// `\nocite`) and nonliteral key arguments fail closed. Comments inside citation
/// arguments and biblatex's global parenthesized multicite notes are unsupported.
pub fn scan_tex<'a>(text: &'a str, limits: Limits, stop: &dyn Fn() -> bool) -> Result<Vec<Occurrence<'a>>, ScanError> {
    let mut tex = Tex {
        text,
        at: 0,
        work: Work::new(text.len(), stop),
    };
    tex.work.check()?;
    if text.len() > limits.max_bytes {
        return Err(ScanError::Limit);
    }
    let mut occurrences = Vec::new();
    while let Some(c) = tex.next()? {
        if c == '%' {
            let skip = tex.work.find(&text[tex.at..], |c| matches!(c, '\n' | '\r'));
            tex.work.check()?;
            tex.at += skip.unwrap_or(text.len() - tex.at);
        } else if c == '\\' {
            let start = tex.at - 1;
            let command = tex.word()?;
            if command.is_empty() {
                tex.next()?; // One escaped control symbol, including percent or backslash.
                continue;
            }
            if matches!(command, "verb" | "Verb") {
                if tex.peek() == Some('*') {
                    tex.next()?;
                }
                let delimiter = tex
                    .next()?
                    .filter(|c| !c.is_whitespace())
                    .ok_or_else(|| tex.incomplete(start))?;
                loop {
                    match tex.next()? {
                        Some(c) if c == delimiter => break,
                        None | Some('\n' | '\r') => return Err(tex.incomplete(start)),
                        _ => {}
                    }
                }
            } else if command == "begin" {
                tex.spaces()?;
                if tex.peek() != Some('{') {
                    return Err(tex.incomplete(start));
                }
                let name = tex.group(true)?;
                let environment = &text[name];
                if matches!(
                    environment,
                    "verbatim" | "verbatim*" | "Verbatim" | "Verbatim*" | "lstlisting" | "minted" | "comment"
                ) {
                    let end = format!("\\end{{{environment}}}");
                    let skip = tex.work.find_str(&text[tex.at..], &end);
                    tex.work.check()?;
                    let Some(skip) = skip else {
                        return Err(tex.incomplete(start));
                    };
                    tex.at += skip;
                    tex.advance(end.len())?;
                }
            } else if evaluated(command) {
                return Err(tex.unsupported(start));
            } else if command.to_ascii_lowercase().contains("cite") {
                // The existing capability table owns which commands have plural groups.
                let multi = syntax::CAPABILITIES
                    .iter()
                    .any(|caps| caps.syntax == KeySyntax::Biblatex && caps.command == command.strip_suffix('s'));
                if tex.peek() == Some('*') {
                    tex.next()?;
                }
                loop {
                    tex.spaces()?;
                    while tex.peek() == Some('[') {
                        tex.group(false)?;
                        tex.spaces()?;
                    }
                    if tex.peek() != Some('{') {
                        return Err(tex.incomplete(start));
                    }
                    tex.group(true)?;
                    if !multi {
                        break;
                    }
                    tex.spaces()?;
                    if !matches!(tex.peek(), Some('[' | '{')) {
                        break;
                    }
                }
                // Structural validation above is strict; only this reader splits keys.
                let Some(read) = syntax::read::citation(&text[start..tex.at], &mut tex.work)? else {
                    return Err(tex.unsupported(start));
                };
                if !read.rest.is_empty() {
                    return Err(tex.unsupported(start));
                }
                if read.keys.is_empty() {
                    return Err(tex.incomplete(start));
                }
                for key in read.keys {
                    let Cow::Borrowed(key) = key else {
                        return Err(tex.unsupported(start));
                    };
                    if occurrences.len() == limits.max_occurrences {
                        return Err(ScanError::Limit);
                    }
                    // TeX reader keys borrow directly from the original input.
                    let offset = key.as_ptr() as usize - text.as_ptr() as usize;
                    occurrences.push(Occurrence {
                        key,
                        range: offset..offset + key.len(),
                    });
                }
            }
        }
    }
    tex.work.check()?;
    if stop() {
        return Err(ScanError::Cancelled);
    }
    Ok(occurrences)
}

struct Tex<'a, 's> {
    text: &'a str,
    at: usize,
    work: Work<'s>,
}

fn evaluated(command: &str) -> bool {
    command.starts_with("if")
        || command.ends_with("command")
        || command.ends_with("Command")
        || command.ends_with("environment")
        || command.ends_with("Environment")
        || ["def", "gdef", "edef", "xdef", "let", "futurelet"].contains(&command)
        || ["input", "include", "includeonly", "subfile"].contains(&command)
        || ["import", "subimport", "InputIfFileExists", "endinput"].contains(&command)
        || ["catcode", "csname", "expandafter"].contains(&command)
        || ["lstinline", "mintinline"].contains(&command)
}

impl<'a> Tex<'a, '_> {
    fn peek(&self) -> Option<char> {
        self.text[self.at..].chars().next()
    }

    fn advance(&mut self, bytes: usize) -> Result<(), ScanError> {
        self.work.spend(bytes);
        self.work.check()?;
        self.at += bytes;
        Ok(())
    }

    fn next(&mut self) -> Result<Option<char>, ScanError> {
        let c = self.peek();
        if let Some(c) = c {
            self.advance(c.len_utf8())?;
        }
        Ok(c)
    }

    fn spaces(&mut self) -> Result<(), ScanError> {
        let rest = self.work.trim_start(&self.text[self.at..]);
        self.at = self.text.len() - rest.len();
        Ok(self.work.check()?)
    }

    fn word(&mut self) -> Result<&'a str, ScanError> {
        let start = self.at;
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.next()?;
        }
        Ok(&self.text[start..self.at])
    }

    fn incomplete(&self, start: usize) -> ScanError {
        ScanError::Incomplete { range: start..self.at }
    }

    fn unsupported(&self, start: usize) -> ScanError {
        ScanError::Unsupported { range: start..self.at }
    }

    /// Check balanced arguments and TeX evaluation tokens, never key spelling.
    fn group(&mut self, literal: bool) -> Result<Range<usize>, ScanError> {
        let start = self.at;
        let open = self.next()?;
        let close = if literal { '}' } else { ']' };
        let body = self.at;
        let (mut depth, mut braces) = (1, 0_usize);
        while let Some(c) = self.next()? {
            if c == '%' || literal && matches!(c, '\\' | '{' | '#' | '~' | '^' | '$' | '&') {
                return Err(self.unsupported(start));
            }
            if c == '\\' {
                let command = self.word()?;
                if evaluated(command) {
                    return Err(self.unsupported(start));
                }
                if command.is_empty() {
                    self.next()?;
                }
                continue;
            }
            if !literal {
                if c == '{' {
                    braces += 1;
                }
                if c == '}' {
                    if braces == 0 {
                        return Err(self.incomplete(start));
                    }
                    braces -= 1;
                    continue;
                }
                if braces > 0 {
                    continue;
                }
            }
            if Some(c) == open {
                depth += 1;
            }
            if c == close {
                depth -= 1;
                if depth == 0 {
                    let range = body..self.at - 1;
                    if literal && self.text[range.clone()].trim().is_empty() {
                        return Err(self.incomplete(start));
                    }
                    return Ok(range);
                }
            }
        }
        Err(self.incomplete(start))
    }
}
