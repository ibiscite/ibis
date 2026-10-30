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
    /// Number of string definitions preceding this entry, assigned by the loader.
    pub macro_epoch: usize,
}

/// Find all top-level blocks. Text between blocks is a comment, as in BibTeX.
pub(crate) fn split_blocks(src: &str) -> (Vec<Block<'_>>, bool) {
    let bytes = src.as_bytes();
    let mut blocks = Vec::new();
    let mut headers = Vec::new();
    let mut i = 0;
    let mut recovering = false;
    let mut limited = false;
    // Line numbers are counted incrementally so splitting stays linear.
    let (mut line, mut counted_to) = (1, 0);
    while let Some(offset) = src[i..].find('@') {
        // Keep one extra block to report truncation before allocating the
        // rest of a hostile file, including recovery headers.
        if blocks.len() + headers.len() > crate::MAX_LIBRARY_RECORDS {
            limited = true;
            break;
        }
        let start = i + offset;
        let mut j = start + 1;
        // Keep stray closing braces in a candidate type so damaged entries
        // produce a warning instead of disappearing as surrounding prose.
        while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || matches!(bytes[j], b'_' | b'}')) {
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
        if !recovering {
            if let Some(end) = find_close(bytes, k) {
                blocks.push(Block {
                    kind,
                    text: &src[start..=end],
                    body: &src[k + 1..end],
                    line,
                    complete: true,
                });
                i = end + 1;
                continue;
            }
            recovering = true;
        }
        headers.push(Header {
            kind,
            start,
            open: k,
            line,
            close: src.len(),
            next: usize::MAX,
        });
        i = k + 1;
    }
    // Recovery can collapse many candidate headers into one block, so the
    // truncation flag must survive independently of the final block count.
    limited |= blocks.len() + headers.len() > crate::MAX_LIBRARY_RECORDS;
    find_closes(bytes, &mut headers);
    let mut resume = 0;
    for Header {
        kind,
        start,
        open,
        line,
        close,
        ..
    } in headers
    {
        if start < resume {
            continue;
        }
        let complete = close < src.len();
        let body_end = if complete {
            close
        } else {
            next_entry_line(src, open).unwrap_or(src.len())
        };
        resume = body_end + usize::from(complete);
        blocks.push(Block {
            kind,
            text: &src[start..resume],
            body: &src[open + 1..body_end],
            line,
            complete,
        });
    }
    (blocks, limited)
}

struct Header {
    kind: String,
    start: usize,
    open: usize,
    line: usize,
    /// `bytes.len()` means there is no closing delimiter.
    close: usize,
    /// Next query sharing this scan state, or `usize::MAX` at the tail.
    next: usize,
}

type Waiting = Option<(usize, usize)>;

/// Join query lists in constant time; moving a large list never copies it.
fn merge_waiting(into: &mut Waiting, from: Waiting, headers: &mut [Header]) {
    if let Some((head, tail)) = from {
        if let Some((_, previous_tail)) = into {
            headers[*previous_tail].next = head;
            *previous_tail = tail;
        } else {
            *into = Some((head, tail));
        }
    }
}

fn close_waiting(group: Waiting, headers: &mut [Header], pos: usize) {
    if let Some((mut head, _)) = group {
        while head != usize::MAX {
            headers[head].close = pos;
            head = headers[head].next;
        }
    }
}

fn merge_bucket(buckets: &mut Vec<(isize, [Waiting; 4])>, depth: isize, groups: [Waiting; 4], headers: &mut [Header]) {
    if groups.iter().all(Option::is_none) {
        return;
    }
    if let Some((_, existing)) = buckets.last_mut().filter(|(base, _)| *base == depth) {
        for (into, from) in existing.iter_mut().zip(groups) {
            merge_waiting(into, from, headers);
        }
    } else {
        buckets.push((depth, groups));
    }
}

/// Compute every candidate's close in one forward pass, including candidates
/// inside values: they are used only if an earlier block is incomplete.
fn find_closes(bytes: &[u8], headers: &mut [Header]) {
    if headers.is_empty() {
        return;
    }
    // A query's local brace depth is global depth minus its bucket's base.
    // Only the bucket at global depth sees quotes/comments/closing delimiters;
    // deeper queries ignore them. Buckets need at least one header each, so
    // auxiliary storage depends on candidates, never on raw brace nesting.
    // Lists: braced unquoted/quoted, then parenthesized unquoted/quoted.
    let mut buckets: Vec<(isize, [Waiting; 4])> = Vec::new();
    let mut comments = [None; 2];
    let mut depth = 0isize;
    let mut candidate = 0;
    let mut escaped = false;
    for (pos, &b) in bytes.iter().enumerate() {
        // Queries start after an opener, return from groups after `}`, and
        // leave comments after a newline: each resets backslash parity.
        let escaped_before = escaped;
        escaped = !escaped && b == b'\\';
        match b {
            b'{' => depth += 1,
            b'}' => {
                let mut shifted = [None; 4];
                if buckets.last().is_some_and(|(base, _)| *base == depth)
                    && let Some((_, mut groups)) = buckets.pop()
                {
                    close_waiting(groups[0].take(), headers, pos);
                    close_waiting(groups[1].take(), headers, pos);
                    // Parenthesis queries ignore unmatched `}`. Keep them at
                    // local depth zero as the global depth decreases.
                    shifted = groups;
                }
                depth -= 1;
                merge_bucket(&mut buckets, depth, shifted, headers);
            }
            b'\n' => {
                let groups = [comments[0].take(), None, comments[1].take(), None];
                merge_bucket(&mut buckets, depth, groups, headers);
            }
            _ => {
                if let Some((_, groups)) = buckets.last_mut().filter(|(base, _)| *base == depth) {
                    match b {
                        b')' => close_waiting(groups[2].take(), headers, pos),
                        b'"' if !escaped_before => {
                            groups.swap(0, 1);
                            groups.swap(2, 3);
                        }
                        b'%' if !escaped_before => {
                            merge_waiting(&mut comments[0], groups[0].take(), headers);
                            merge_waiting(&mut comments[1], groups[2].take(), headers);
                        }
                        _ => {}
                    }
                }
                if buckets
                    .last()
                    .is_some_and(|(_, groups)| groups.iter().all(Option::is_none))
                {
                    buckets.pop();
                }
            }
        }
        if candidate < headers.len() && headers[candidate].open == pos {
            let mut groups = [None; 4];
            groups[2 * usize::from(b == b'(')] = Some((candidate, candidate));
            merge_bucket(&mut buckets, depth, groups, headers);
            candidate += 1;
        }
    }
}

/// Index of the delimiter closing the one at `open`. Braces nest; for
/// `(`-delimited blocks a `)` inside braces or a top-level quoted string does
/// not close the block.
/// Complete blocks use this simple scan; recovery batches the same queries.
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
    if block.kind.contains('}') {
        return Err("entry type contains an unexpected closing brace".to_string());
    }
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
        macro_epoch: 0,
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
        // Nearly every field is one literal. Vec's first growth reserves four
        // Parts, which otherwise keeps three unused slots per ordinary field.
        let mut parts = Vec::with_capacity(1);
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
    use proptest::prelude::*;

    #[test]
    fn block_collection_stops_before_allocating_a_whole_hostile_file() {
        let repeated = "@misc{same}\n".repeat(crate::MAX_LIBRARY_RECORDS + 100);
        assert_eq!(split_blocks(&repeated).0.len(), crate::MAX_LIBRARY_RECORDS + 1);
        let recovery = "@misc{same, title={\n".repeat(crate::MAX_LIBRARY_RECORDS + 100);
        assert!(split_blocks(&recovery).0.len() <= crate::MAX_LIBRARY_RECORDS + 1);
    }
    #[test]
    fn raw_nesting_and_unmatched_closes_need_only_one_query_bucket() {
        for text in [
            "{".repeat(500_000) + &"}".repeat(500_000),
            "(".to_string() + &"}".repeat(1_000_000) + ")",
        ] {
            let mut headers = [Header {
                kind: String::new(),
                start: 0,
                open: 0,
                line: 1,
                close: text.len(),
                next: usize::MAX,
            }];
            find_closes(text.as_bytes(), &mut headers);
            assert_eq!(Some(headers[0].close), find_close(text.as_bytes(), 0));
        }
    }

    proptest! {
        #[test]
        fn merged_closes_match_independent_forward_scans(
            body in prop::collection::vec(prop::sample::select(vec![
                '{', '}', '(', ')', '"', '\\', '%', '\n', ' ', 'a', '@', 'é',
            ]), 0..200),
        ) {
            let text: String = body.into_iter().collect();
            let mut headers: Vec<_> = text.char_indices().filter(|(_, c)| matches!(c, '{' | '(')).map(|(open, _)| {
                Header {
                    kind: String::new(), start: 0, open, line: 1, close: text.len(), next: usize::MAX,
                }
            }).collect();
            find_closes(text.as_bytes(), &mut headers);
            for header in headers {
                let actual = (header.close < text.len()).then_some(header.close);
                prop_assert_eq!(actual, find_close(text.as_bytes(), header.open), "{:?}", text);
            }
        }
    }

    #[test]
    fn quoted_parentheses_do_not_close_a_parenthesis_entry() {
        let (blocks, _) = split_blocks("@book(q, title = \"A (B) C\", year = 2020)\n@book{after, title={After}}");
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].complete);
        assert!(blocks[0].body.ends_with("2020"));
    }

    #[test]
    fn recovery_resumes_at_indented_entries() {
        let (blocks, _) = split_blocks("@article{broken, title={Unclosed}\n  @article{indented, title={Indented}}\n");
        assert_eq!(blocks.len(), 2);
        assert!(!blocks[0].complete);
        assert!(blocks[1].complete);
        assert_eq!(blocks[1].line, 2);
    }

    #[test]
    fn fields_with_concatenation_numbers_and_macros() {
        let (blocks, _) = split_blocks("@article{k, title = \"A {B} C\" # x, year = 2020, month = mar,}");
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
        let (blocks, _) = split_blocks("@misc{lonely}");
        let entry = parse_entry(&blocks[0]).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(entry.key, "lonely");
        assert!(entry.fields.is_empty());
    }

    #[test]
    fn literal_contents_share_the_source_after_blocks_are_dropped() {
        let cases = [
            ("{ordinary}", "ordinary"),
            ("{}", ""),
            ("{é\u{2003}字}", "é\u{2003}字"),
            ("{A {B} C}", "A {B} C"),
            (r#""A \"u (B) C""#, r#"A \"u (B) C"#),
            (r"{\{literal\} \\{group\\}}", r"\{literal\} \\{group\\}"),
            ("{\u{E000}\u{E001}\u{E002}\u{E003}}", "\u{E000}\u{E001}\u{E002}\u{E003}"),
        ];
        let mut text = String::from("@string{greeting=\"Macro é\"}\n");
        let macro_start = text.find("Macro é").unwrap_or_else(|| panic!("missing macro"));
        let mut spans = Vec::new();
        for (i, (literal, value)) in cases.iter().enumerate() {
            let prefix = format!("@book{{key{i},title=");
            let start = text.len() + prefix.len() + 1;
            text.push_str(&prefix);
            text.push_str(literal);
            text.push_str("}\n");
            spans.push(start..start + value.len());
        }
        let (blocks, limited) = split_blocks(&text);
        assert!(!limited);
        let (_, definition) = parse_string(&blocks[0]).unwrap_or_else(|| panic!("invalid macro"));
        let entries: Vec<_> = blocks[1..]
            .iter()
            .map(|block| parse_entry(block).unwrap_or_else(|e| panic!("{e}")))
            .collect();
        drop(blocks);
        for ((entry, span), (_, expected)) in entries.iter().zip(spans).zip(cases) {
            let Part::Text(value) = &entry.fields[0].parts[0] else {
                panic!("literal changed kind");
            };
            assert_eq!(&**value, expected);
            assert_eq!(
                value.as_ptr(),
                text[span].as_ptr(),
                "literal retained a separate buffer"
            );
        }
        let Part::Text(value) = &definition[0] else {
            panic!("macro literal changed kind");
        };
        assert_eq!(&**value, "Macro é");
        assert_eq!(value.as_ptr(), text[macro_start..].as_ptr());
    }

    #[test]
    fn ordinary_fields_have_a_bounded_raw_working_set() {
        let text = (0..1_000)
            .map(|i| format!("@article{{synthetic{i},title={{Signals}},author={{Invented, Avery}},year=2026,journal={{Letters}},pages={{1--20}},doi={{10.1000/synthetic{i}}},volume=1,number=2}}\n"))
            .collect::<String>();
        let (blocks, limited) = split_blocks(&text);
        assert!(!limited);
        let entries: Vec<_> = blocks
            .iter()
            .map(|block| parse_entry(block).unwrap_or_else(|e| panic!("{e}")))
            .collect();
        let retained: usize = entries
            .iter()
            .map(|entry| {
                std::mem::size_of::<RawEntry>()
                    + entry.key.capacity()
                    + entry.kind.capacity()
                    + entry.fields.capacity() * std::mem::size_of::<Field>()
                    + entry
                        .fields
                        .iter()
                        .map(|field| {
                            field.name.capacity()
                                + field.parts.capacity() * std::mem::size_of::<Part>()
                                + field
                                    .parts
                                    .iter()
                                    .map(|part| match part {
                                        Part::Text(value) | Part::Number(value) | Part::Macro(value) => {
                                            value.capacity()
                                        }
                                    })
                                    .sum::<usize>()
                        })
                        .sum::<usize>()
            })
            .sum();
        assert!(retained <= 1_024 * entries.len(), "retained raw bytes: {retained}");
    }
}
