//! Splits BibTeX text into top-level blocks and parses entry fields.
//!
//! This layer only finds structure. It never interprets values, so it can be
//! strict about syntax while staying tolerant of damage: an unclosed entry is
//! reported and scanning resumes at the next line that starts a new entry.

/// A top-level `@kind{...}` or `@kind(...)` block.
#[derive(Debug)]
pub(crate) struct Block<'a> {
    /// Lowercased entry kind, e.g. `article`, `string`, `comment`.
    pub kind: String,
    /// The whole block, from `@` to the closing delimiter (or to the point of recovery).
    pub text: &'a str,
    /// Text between the delimiters.
    pub body: &'a str,
    /// 1-based line where the block starts.
    pub line: usize,
    /// False when the closing delimiter was never found.
    pub complete: bool,
}

/// One part of a field value: `{...}`, `"..."`, a number, or a macro name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Part {
    Text(String),
    Number(String),
    Macro(String),
}

/// A parsed `name = value # value` pair. Names are lowercased.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Field {
    pub name: String,
    pub parts: Vec<Part>,
}

/// An entry with its key and fields, before macros and inheritance are resolved.
#[derive(Debug, Clone)]
pub(crate) struct RawEntry {
    pub kind: String,
    pub key: String,
    pub fields: Vec<Field>,
    pub line: usize,
}

/// Find all top-level blocks. Text between blocks is a comment, as in BibTeX.
pub(crate) fn split_blocks(src: &str) -> Vec<Block<'_>> {
    let bytes = src.as_bytes();
    let mut blocks = Vec::new();
    let mut i = 0;
    // Line numbers are counted incrementally so splitting stays linear.
    let (mut line, mut counted_to) = (1, 0);
    while let Some(offset) = src[i..].find('@') {
        let start = i + offset;
        let mut j = start + 1;
        while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
            j += 1;
        }
        let kind = src[start + 1..j].to_ascii_lowercase();
        let mut k = j;
        while k < bytes.len() && bytes[k].is_ascii_whitespace() {
            k += 1;
        }
        if kind.is_empty() || k >= bytes.len() || !matches!(bytes[k], b'{' | b'(') {
            i = start + 1;
            continue;
        }
        line += src[counted_to..start].matches('\n').count();
        counted_to = start;
        match find_close(bytes, k) {
            Some(end) => {
                blocks.push(Block {
                    kind,
                    text: &src[start..=end],
                    body: &src[k + 1..end],
                    line,
                    complete: true,
                });
                i = end + 1;
            }
            None => {
                let resume = next_entry_line(src, k).unwrap_or(src.len());
                blocks.push(Block {
                    kind,
                    text: &src[start..resume],
                    body: &src[(k + 1).min(resume)..resume],
                    line,
                    complete: false,
                });
                i = resume;
            }
        }
    }
    blocks
}

/// Index of the delimiter closing the one at `open`. Braces nest; for
/// `(`-delimited blocks a `)` inside braces or a top-level quoted string does
/// not close the block.
fn find_close(bytes: &[u8], open: usize) -> Option<usize> {
    let paren = bytes[open] == b'(';
    let mut depth = 0usize;
    let mut in_quote = false;
    let mut escaped = false;
    let mut in_comment = false;
    for (pos, &b) in bytes.iter().enumerate().skip(open + 1) {
        // `%` between fields starts a comment to the end of the line.
        if in_comment {
            in_comment = b != b'\n';
            continue;
        }
        if b == b'%' && depth == 0 && !in_quote && !escaped {
            in_comment = true;
            continue;
        }
        // `\"` (an umlaut command) inside a quoted value is not a quote.
        let escaped_before = std::mem::take(&mut escaped);
        if escaped_before && b == b'"' {
            continue;
        }
        escaped = !escaped_before && b == b'\\';
        match b {
            b'{' => depth += 1,
            b'}' if depth > 0 => depth -= 1,
            b'}' if !paren => return Some(pos),
            b'"' if depth == 0 => in_quote = !in_quote,
            b')' if paren && depth == 0 && !in_quote => return Some(pos),
            _ => {}
        }
    }
    None
}

/// Start of the next line (after `from`) whose first non-blank character is `@`.
fn next_entry_line(src: &str, from: usize) -> Option<usize> {
    let mut pos = src[from..].find('\n')? + from + 1;
    loop {
        let line_end = src[pos..].find('\n').map_or(src.len(), |e| pos + e);
        if src[pos..line_end].trim_start().starts_with('@') {
            return Some(pos);
        }
        if line_end >= src.len() {
            return None;
        }
        pos = line_end + 1;
    }
}

/// Parse an entry body: `key, name = value, ...`.
pub(crate) fn parse_entry(block: &Block) -> Result<RawEntry, String> {
    let mut p = Parser::new(block.body);
    p.skip_ws();
    let key_start = p.pos;
    while p.peek().is_some_and(|c| c != ',' && !c.is_whitespace()) {
        p.bump();
    }
    let key = block.body[key_start..p.pos].to_string();
    if key.is_empty() || key.contains(['=', '{', '}', '"', '(', ')', '#']) {
        return Err("entry has no citation key".to_string());
    }
    let mut fields = Vec::new();
    loop {
        p.skip_ws();
        match p.peek() {
            None => break,
            Some(',') => {
                p.bump();
                continue;
            }
            Some(_) => {}
        }
        let name = p
            .ident()
            .ok_or_else(|| format!("expected a field name near `{}`", p.context()))?;
        p.skip_ws();
        if p.peek() != Some('=') {
            return Err(format!("expected `=` after field `{name}`"));
        }
        p.bump();
        let parts = p.value()?;
        fields.push(Field {
            name: name.to_ascii_lowercase(),
            parts,
        });
    }
    Ok(RawEntry {
        kind: canonical_kind(&block.kind).to_string(),
        key,
        fields,
        line: block.line,
    })
}

/// BibLaTeX's canonical name for legacy entry-type aliases.
fn canonical_kind(kind: &str) -> &str {
    match kind {
        "conference" => "inproceedings",
        "electronic" | "www" => "online",
        other => other,
    }
}

/// Parse a `@string` body: `name = value`.
pub(crate) fn parse_string(block: &Block) -> Option<(String, Vec<Part>)> {
    let mut p = Parser::new(block.body);
    p.skip_ws();
    let name = p.ident()?;
    p.skip_ws();
    if p.peek() != Some('=') {
        return None;
    }
    p.bump();
    let parts = p.value().ok()?;
    p.skip_ws();
    Some((name.to_ascii_lowercase(), parts))
}

struct Parser<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Parser { src, pos: 0 }
    }

    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn bump(&mut self) {
        if let Some(c) = self.peek() {
            self.pos += c.len_utf8();
        }
    }

    /// Skip whitespace and `%` comments (to end of line) between fields.
    fn skip_ws(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => self.bump(),
                Some('%') => {
                    while self.peek().is_some_and(|c| c != '\n') {
                        self.bump();
                    }
                }
                _ => break,
            }
        }
    }

    fn context(&self) -> String {
        self.src[self.pos..].chars().take(20).collect()
    }

    /// Field or macro name: anything up to whitespace or a delimiter.
    fn ident(&mut self) -> Option<String> {
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|c| !c.is_whitespace() && !matches!(c, '=' | ',' | '{' | '}' | '"' | '#' | '(' | ')'))
        {
            self.bump();
        }
        (self.pos > start).then(|| self.src[start..self.pos].to_string())
    }

    /// `part # part # ...` up to the next top-level comma or end of body.
    fn value(&mut self) -> Result<Vec<Part>, String> {
        let mut parts = Vec::new();
        loop {
            self.skip_ws();
            let part = match self.peek() {
                Some('{') => Part::Text(self.delimited('{', '}')?),
                Some('"') => Part::Text(self.delimited('"', '"')?),
                Some(c) if c.is_ascii_digit() => {
                    let start = self.pos;
                    while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                        self.bump();
                    }
                    Part::Number(self.src[start..self.pos].to_string())
                }
                Some(_) => Part::Macro(
                    self.ident()
                        .ok_or_else(|| format!("unexpected text near `{}`", self.context()))?
                        .to_ascii_lowercase(),
                ),
                None => return Err("field value is missing".to_string()),
            };
            parts.push(part);
            self.skip_ws();
            match self.peek() {
                Some('#') => self.bump(),
                Some(',') | None => return Ok(parts),
                Some(_) => return Err(format!("unexpected text near `{}`", self.context())),
            }
        }
    }

    /// Contents of `{...}` (nested braces kept) or `"..."` (braces inside allowed).
    fn delimited(&mut self, open: char, close: char) -> Result<String, String> {
        self.bump();
        let start = self.pos;
        let mut depth = 0usize;
        let mut escaped = false;
        while let Some(c) = self.peek() {
            let escaped_before = std::mem::take(&mut escaped);
            if escaped_before && c == '"' {
                self.bump();
                continue;
            }
            escaped = !escaped_before && c == '\\';
            match c {
                '{' => depth += 1,
                '}' if depth > 0 => depth -= 1,
                c if c == close && depth == 0 => {
                    let inner = self.src[start..self.pos].to_string();
                    self.bump();
                    return Ok(inner);
                }
                _ => {}
            }
            self.bump();
        }
        Err(format!("unbalanced `{open}` in field value"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_parentheses_do_not_close_a_parenthesis_entry() {
        let blocks = split_blocks("@book(q, title = \"A (B) C\", year = 2020)\n@book{after, title={After}}");
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].complete);
        assert!(blocks[0].body.ends_with("2020"));
    }

    #[test]
    fn recovery_resumes_at_indented_entries() {
        let blocks = split_blocks("@article{broken, title={Unclosed}\n  @article{indented, title={Indented}}\n");
        assert_eq!(blocks.len(), 2);
        assert!(!blocks[0].complete);
        assert!(blocks[1].complete);
        assert_eq!(blocks[1].line, 2);
    }

    #[test]
    fn fields_with_concatenation_numbers_and_macros() {
        let blocks = split_blocks("@article{k, title = \"A {B} C\" # x, year = 2020, month = mar,}");
        let entry = parse_entry(&blocks[0]).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(entry.key, "k");
        assert_eq!(
            entry.fields[0].parts,
            [Part::Text("A {B} C".into()), Part::Macro("x".into())]
        );
        assert_eq!(entry.fields[1].parts, [Part::Number("2020".into())]);
        assert_eq!(entry.fields[2].parts, [Part::Macro("mar".into())]);
    }

    #[test]
    fn entry_without_fields() {
        let blocks = split_blocks("@misc{lonely}");
        let entry = parse_entry(&blocks[0]).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(entry.key, "lonely");
        assert!(entry.fields.is_empty());
    }
}
