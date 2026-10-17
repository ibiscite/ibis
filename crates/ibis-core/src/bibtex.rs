//! Read-only BibTeX/BibLaTeX source adapter.
//!
//! Real-world `.bib` files often contain a broken entry, a duplicate key or an
//! undefined `@string` macro. The underlying parser rejects a whole file for
//! any of those, so this adapter splits the file into top-level blocks first
//! and parses every entry on its own. Good entries always load; each problem
//! becomes a [`LoadWarning`] with a line number. Nothing is dropped silently.

use std::collections::HashSet;

use std::collections::BTreeMap;
use std::panic::AssertUnwindSafe;

use biblatex::{Bibliography, ChunksExt, DateValue, EditorType, Entry, ParseErrorKind, PermissiveType, RetrievalError};

use crate::model::{LoadResult, LoadWarning, Name, PartialDate, RecordId, Reference, WarningKind};

/// Upper bound on undefined macros recovered per entry (guards pathological input).
const MAX_UNDEFINED_MACROS: usize = 16;

/// A top-level `@kind{...}` block in the source text.
#[derive(Debug)]
struct Block<'a> {
    kind: String,
    text: &'a str,
    line: usize,
    complete: bool,
}

/// Parse BibTeX/BibLaTeX text. `source` labels where the text came from and
/// becomes part of each record's identity.
pub fn load_str(source: &str, text: &str) -> LoadResult {
    let blocks = split_blocks(text);
    let strings: Vec<&Block> = blocks.iter().filter(|b| b.kind == "string" && b.complete).collect();
    let mut result = LoadResult::default();
    let mut seen = HashSet::new();

    for block in blocks
        .iter()
        .filter(|b| !matches!(b.kind.as_str(), "string" | "preamble" | "comment"))
    {
        let key = block_key(block.text);
        if !block.complete {
            result
                .warnings
                .push(warning(WarningKind::Syntax, block, key, "entry is not closed; skipped"));
            continue;
        }
        let Some(key) = key else {
            result.warnings.push(warning(
                WarningKind::Syntax,
                block,
                None,
                "entry has no citation key; skipped",
            ));
            continue;
        };
        if !seen.insert(key.clone()) {
            let message = format!("duplicate key `{key}`; the first entry with this key is used");
            result
                .warnings
                .push(warning(WarningKind::DuplicateKey, block, Some(key), &message));
            continue;
        }

        // A third-party parser panic must never take down the whole library:
        // it becomes a warning for this one entry.
        let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| load_entry(source, &key, block, &strings, &blocks)));
        match outcome {
            Ok((reference, warnings)) => {
                result.warnings.extend(warnings);
                result.references.extend(reference);
            }
            Err(_) => result.warnings.push(warning(
                WarningKind::Syntax,
                block,
                Some(key),
                "entry could not be read safely (parser error); skipped",
            )),
        }
    }
    result
}

/// Parse and convert one entry in isolation.
fn load_entry(
    source: &str,
    key: &str,
    block: &Block,
    strings: &[&Block],
    blocks: &[Block],
) -> (Option<Reference>, Vec<LoadWarning>) {
    let mut warnings = Vec::new();
    // An undefined `@string` macro is defined as empty and reported, as
    // BibTeX itself does, so one bad field does not hide the whole entry.
    let mut mini = isolated_source(block, strings, blocks);
    let mut undefined: Vec<String> = Vec::new();
    let parsed = loop {
        match Bibliography::parse(&mini) {
            Err(err) => match &err.kind {
                ParseErrorKind::UnknownAbbreviation(name)
                    if undefined.len() < MAX_UNDEFINED_MACROS && !undefined.contains(name) =>
                {
                    mini = format!("@string{{{name} = \"\"}}\n{mini}");
                    undefined.push(name.clone());
                }
                _ => break Err(err),
            },
            ok => break ok,
        }
    };
    let bib = match parsed {
        Ok(bib) => bib,
        Err(err) => {
            let message = format!("{}; skipped", err.kind);
            warnings.push(warning(WarningKind::Syntax, block, Some(key.to_string()), &message));
            return (None, warnings);
        }
    };
    for name in &undefined {
        let message = format!("undefined abbreviation `{name}`; the field using it is left empty");
        warnings.push(warning(WarningKind::Field, block, Some(key.to_string()), &message));
    }
    let Some(entry) = bib.get(key) else {
        warnings.push(warning(
            WarningKind::Syntax,
            block,
            Some(key.to_string()),
            "entry could not be read; skipped",
        ));
        return (None, warnings);
    };
    let reference = convert(source, key, &block.kind, block.text, entry, block, &mut warnings);
    (Some(reference), warnings)
}

fn warning(kind: WarningKind, block: &Block, key: Option<String>, message: &str) -> LoadWarning {
    LoadWarning {
        kind,
        line: Some(block.line),
        key,
        message: message.to_string(),
    }
}

/// Build a small source containing one entry plus only what it depends on:
/// the `@string` macros it mentions and its `crossref` parent.
fn isolated_source(block: &Block, strings: &[&Block], all: &[Block]) -> String {
    let parent = field_value(block.text, "crossref").and_then(|parent_key| {
        all.iter().find(|b| {
            b.complete && !std::ptr::eq(*b, block) && block_key(b.text).as_deref() == Some(parent_key.as_str())
        })
    });
    let mut entries: Vec<&str> = parent.map(|p| p.text).into_iter().collect();
    entries.push(block.text);
    let lower = entries.join("\n").to_ascii_lowercase();
    let mut parts: Vec<String> = strings
        .iter()
        .filter(|s| string_name(s.text).is_some_and(|name| mentions_word(&lower, &name)))
        .map(|s| prepare(s.text))
        .collect();
    parts.extend(entries.into_iter().map(prepare));
    parts.join("\n")
}

/// Adjust valid BibTeX that the underlying parser does not accept:
/// `@kind( ... )` delimiters become braces, and the dotless `\i`/`\j`
/// commands (as in `{\'\i}` for "í") become plain letters so accents apply.
fn prepare(text: &str) -> String {
    let mut out = text.to_string();
    if let Some(open) = out.find(['{', '('])
        && out.as_bytes()[open] == b'('
        && out.ends_with(')')
    {
        out.replace_range(open..=open, "{");
        let last = out.len() - 1;
        out.replace_range(last.., "}");
    }
    // The parser needs a comma after the key, even when an entry has no fields.
    if !out.contains(',')
        && let Some(close) = out.rfind('}')
    {
        out.insert(close, ',');
    }
    for (command, letter) in [("\\i", "i"), ("\\j", "j")] {
        let mut result = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(pos) = rest.find(command) {
            let after = rest[pos + command.len()..].chars().next();
            result.push_str(&rest[..pos]);
            if after.is_some_and(|c| c.is_ascii_alphabetic()) {
                result.push_str(command);
            } else {
                result.push_str(letter);
                if after == Some(' ') {
                    rest = &rest[pos + command.len() + 1..];
                    continue;
                }
            }
            rest = &rest[pos + command.len()..];
        }
        result.push_str(rest);
        out = result;
    }
    out
}

fn convert(
    source: &str,
    key: &str,
    kind: &str,
    raw: &str,
    entry: &Entry,
    block: &Block,
    warnings: &mut Vec<LoadWarning>,
) -> Reference {
    let mut field_problem = |field: &str, err: &dyn std::fmt::Display| {
        warnings.push(warning(
            WarningKind::Field,
            block,
            Some(key.to_string()),
            &format!("field `{field}` could not be read: {err}"),
        ));
    };

    let text = |field: &str| {
        entry
            .get(field)
            .map(|c| clean(&c.format_verbatim()))
            .filter(|s| !s.is_empty())
    };

    let authors = match entry.author() {
        Ok(people) => people.into_iter().map(person_to_name).collect(),
        Err(RetrievalError::Missing(_)) => Vec::new(),
        Err(err) => {
            field_problem("author", &err);
            Vec::new()
        }
    };
    let editors = match entry.editors() {
        Ok(groups) => groups
            .into_iter()
            .filter(|(_, role)| *role == EditorType::Editor)
            .flat_map(|(people, _)| people.into_iter().map(person_to_name))
            .collect(),
        Err(err) => {
            field_problem("editor", &err);
            Vec::new()
        }
    };
    // Dates written with non-Latin digits (٢٠٢٠, ২০২০, ２０２０) are read here;
    // the underlying parser cannot handle them.
    let raw_date = text("date").or_else(|| text("year"));
    let non_latin_digits = raw_date
        .as_deref()
        .is_some_and(|d| d.chars().any(|c| c.is_numeric() && !c.is_ascii_digit()));
    let issued = if non_latin_digits {
        let date = raw_date.as_deref().map(ascii_digits).and_then(|d| simple_date(&d));
        let month = text("month")
            .map(|m| ascii_digits(&m))
            .and_then(|m| m.trim().parse::<u8>().ok());
        date.map(|d| PartialDate {
            month: d.month.or(month.filter(|m| (1..=12).contains(m))),
            ..d
        })
    } else {
        parse_date(entry, &mut field_problem, &text)
    };

    // Further CSL variables. BibLaTeX names first, then classic BibTeX names.
    let mut variables = BTreeMap::new();
    let mut set = |csl: &str, fields: &[&str]| {
        if let Some(value) = fields.iter().find_map(|f| text(f)) {
            variables.insert(csl.to_string(), value);
        }
    };
    set("volume", &["volume"]);
    if kind == "article" {
        set("issue", &["number", "issue"]);
    } else {
        set("number", &["number"]);
        set("issue", &["issue"]);
    }
    set("page", &["pages"]);
    set("publisher", &["publisher", "institution", "school", "organization"]);
    set("publisher-place", &["location", "address"]);
    set("edition", &["edition"]);
    set("ISBN", &["isbn"]);
    set("ISSN", &["issn"]);
    set("collection-title", &["series"]);
    set("genre", &["type"]);
    set("note", &["note"]);
    set("event-title", &["eventtitle"]);
    // BibLaTeX `type` values such as `phdthesis` are localization keys, not text.
    let genre = variables.get("genre").map(String::as_str).or(match kind {
        "phdthesis" => Some("phdthesis"),
        "mastersthesis" => Some("mathesis"),
        "techreport" => Some("techreport"),
        _ => None,
    });
    if let Some(readable) = genre.and_then(readable_genre) {
        variables.insert("genre".to_string(), readable.to_string());
    }

    Reference {
        id: RecordId {
            source: source.to_string(),
            native_key: key.to_string(),
        },
        citekey: Some(key.to_string()),
        item_type: csl_type(kind).to_string(),
        title: text("title"),
        authors,
        editors,
        issued,
        container_title: text("journaltitle")
            .or_else(|| text("journal"))
            .or_else(|| text("booktitle")),
        doi: text("doi"),
        url: text("url"),
        abstract_text: text("abstract"),
        keywords: text("keywords")
            .map(|k| {
                k.split([',', ';'])
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default(),
        language: text("langid").or_else(|| text("language")),
        variables,
        raw: Some(raw.to_string()),
    }
}

/// English text for the BibLaTeX type keys that describe theses and reports.
fn readable_genre(key: &str) -> Option<&'static str> {
    Some(match key.to_ascii_lowercase().as_str() {
        "phdthesis" => "PhD thesis",
        "mathesis" | "mastersthesis" => "Master's thesis",
        "candthesis" => "Candidate thesis",
        "techreport" => "Technical report",
        "resreport" => "Research report",
        "software" => "Computer software",
        "datacd" => "CD-ROM",
        "audiocd" => "Audio CD",
        _ => return None,
    })
}

fn person_to_name(p: biblatex::Person) -> Name {
    let some = |s: String| {
        let s = clean(&s);
        (!s.is_empty()).then_some(s)
    };
    Name {
        family: some(p.name),
        given: some(p.given_name),
        particle: some(p.prefix),
        suffix: some(p.suffix),
    }
}

/// Map a BibTeX/BibLaTeX entry type to the closest CSL item type.
pub fn csl_type(kind: &str) -> &'static str {
    match kind {
        "article" => "article-journal",
        "book" | "mvbook" | "proceedings" | "mvproceedings" | "collection" | "mvcollection" | "booklet" | "manual" => {
            "book"
        }
        "inbook" | "incollection" | "bookinbook" | "suppbook" | "suppcollection" => "chapter",
        "inproceedings" | "conference" => "paper-conference",
        "thesis" | "phdthesis" | "mastersthesis" => "thesis",
        "report" | "techreport" => "report",
        "online" | "www" | "electronic" => "webpage",
        "dataset" => "dataset",
        "software" => "software",
        "patent" => "patent",
        "unpublished" => "manuscript",
        _ => "document",
    }
}

/// Split source text into top-level `@kind{...}` / `@kind(...)` blocks.
fn split_blocks(src: &str) -> Vec<Block<'_>> {
    let bytes = src.as_bytes();
    let mut blocks = Vec::new();
    let mut i = 0;
    while let Some(offset) = src[i..].find('@') {
        let start = i + offset;
        let mut j = start + 1;
        while j < bytes.len() && bytes[j].is_ascii_alphabetic() {
            j += 1;
        }
        let kind = src[start + 1..j].to_ascii_lowercase();
        let mut k = j;
        while k < bytes.len() && bytes[k].is_ascii_whitespace() {
            k += 1;
        }
        if kind.is_empty() || k >= bytes.len() || !matches!(bytes[k], b'{' | b'(') {
            // Text outside entries is a comment in BibTeX; a stray `@` is not an entry.
            i = start + 1;
            continue;
        }
        let closer = if bytes[k] == b'{' { b'}' } else { b')' };
        let mut depth = 0usize;
        let mut end = None;
        for (pos, &b) in bytes.iter().enumerate().skip(k + 1) {
            match b {
                b'{' => depth += 1,
                b'}' if depth > 0 => depth -= 1,
                c if c == closer && depth == 0 => {
                    end = Some(pos + 1);
                    break;
                }
                _ => {}
            }
        }
        let line = src[..start].matches('\n').count() + 1;
        match end {
            Some(end) => {
                blocks.push(Block {
                    kind,
                    text: &src[start..end],
                    line,
                    complete: true,
                });
                i = end;
            }
            None => {
                // Unclosed: resume at the next line that starts a new entry.
                let resume = src[k..].find("\n@").map_or(src.len(), |p| k + p + 1);
                blocks.push(Block {
                    kind,
                    text: &src[start..resume],
                    line,
                    complete: false,
                });
                i = resume;
            }
        }
    }
    blocks
}

/// Citation key: text between the opening delimiter and the first comma.
fn block_key(text: &str) -> Option<String> {
    let open = text.find(['{', '('])?;
    let rest = &text[open + 1..];
    // `@misc{key}` (no fields) is valid; the key then ends at the closing delimiter.
    let end = rest.find(',').or_else(|| rest.rfind(['}', ')']))?;
    let key = rest[..end].trim();
    (!key.is_empty() && !key.contains(['=', '{', '}', '"', '(', ')'])).then(|| key.to_string())
}

/// Name defined by an `@string{name = ...}` block, lowercased.
fn string_name(text: &str) -> Option<String> {
    let open = text.find(['{', '('])?;
    let rest = &text[open + 1..];
    let name = rest[..rest.find('=')?].trim().to_ascii_lowercase();
    (!name.is_empty()).then_some(name)
}

fn mentions_word(haystack: &str, word: &str) -> bool {
    haystack.match_indices(word).any(|(pos, _)| {
        let before = haystack[..pos].chars().next_back();
        let after = haystack[pos + word.len()..].chars().next();
        let boundary = |c: Option<char>| c.is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        boundary(before) && boundary(after)
    })
}

/// Raw value of a simple `field = {value}` or `field = "value"` pair.
///
/// Field names are ASCII, so matching uses ASCII lowercasing, which keeps
/// byte offsets identical to `text` for every script.
fn field_value(text: &str, field: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let mut search = 0;
    while let Some(pos) = lower[search..].find(field) {
        let at = search + pos;
        search = at + field.len();
        let before = lower[..at].chars().next_back();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let rest = lower[search..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else { continue };
        let value_start = text.len() - rest.len();
        let value = text[value_start..].trim_start();
        let inner = value.strip_prefix('{').or_else(|| value.strip_prefix('"'))?;
        let end = inner.find(['}', '"'])?;
        return Some(inner[..end].trim().to_string());
    }
    None
}

/// Date from the parser, falling back to a plain year when it cannot read the field.
fn parse_date(
    entry: &Entry,
    field_problem: &mut impl FnMut(&str, &dyn std::fmt::Display),
    text: &impl Fn(&str) -> Option<String>,
) -> Option<PartialDate> {
    match entry.date() {
        Ok(PermissiveType::Typed(date)) => {
            let d = match date.value {
                DateValue::At(d) | DateValue::After(d) | DateValue::Before(d) | DateValue::Between(d, _) => d,
            };
            // The parser counts months and days from zero; CSL counts from one.
            Some(PartialDate {
                year: d.year,
                month: d.month.map(|m| m + 1),
                day: d.day.map(|day| day + 1),
            })
        }
        Ok(PermissiveType::Chunks(chunks)) => leading_year(&chunks.format_verbatim()).map(PartialDate::year),
        Err(RetrievalError::Missing(_)) => None,
        Err(err) => {
            field_problem("date", &err);
            text("year").as_deref().and_then(leading_year).map(PartialDate::year)
        }
    }
}

/// Convert decimal digits from common scripts to ASCII digits.
fn ascii_digits(text: &str) -> String {
    const ZEROS: [u32; 12] = [
        0x0660, 0x06F0, 0x07C0, 0x0966, 0x09E6, 0x0A66, 0x0AE6, 0x0B66, 0x0BE6, 0x0C66, 0x0E50, 0xFF10,
    ];
    text.chars()
        .map(|c| {
            ZEROS
                .iter()
                .find(|&&zero| (zero..zero + 10).contains(&(c as u32)))
                .and_then(|&zero| char::from_digit(c as u32 - zero, 10))
                .unwrap_or(c)
        })
        .collect()
}

/// `YYYY`, `YYYY-MM` or `YYYY-MM-DD` (ASCII digits).
fn simple_date(text: &str) -> Option<PartialDate> {
    let mut parts = text.trim().split(['-', '/']);
    let year = leading_year(parts.next()?)?;
    let month = parts
        .next()
        .and_then(|m| m.trim().parse::<u8>().ok())
        .filter(|m| (1..=12).contains(m));
    let day = parts
        .next()
        .and_then(|d| d.trim().parse::<u8>().ok())
        .filter(|d| (1..=31).contains(d));
    Some(PartialDate {
        year,
        month,
        day: month.and(day),
    })
}

fn leading_year(text: &str) -> Option<i32> {
    let digits: String = text.trim().chars().take_while(char::is_ascii_digit).collect();
    if digits.len() == 4 { digits.parse().ok() } else { None }
}

/// Collapse internal whitespace (BibTeX values often span lines).
fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_blocks_and_skips_stray_at_signs() {
        let src = "Contact me @ home.\n@article{a, title={T}}\n@string(x = \"y\")";
        let blocks = split_blocks(src);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].kind, "article");
        assert_eq!(blocks[0].line, 2);
        assert_eq!(blocks[1].kind, "string");
    }

    #[test]
    fn one_broken_entry_does_not_hide_the_others() {
        let src = "@article{good1, title={One}}\n@article{bad, title={Unclosed}\n@article{good2, title={Two}}\n";
        let result = load_str("test.bib", src);
        let keys: Vec<_> = result.references.iter().map(|r| r.id.native_key.as_str()).collect();
        assert_eq!(keys, ["good1", "good2"]);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].kind, WarningKind::Syntax);
        assert_eq!(result.warnings[0].line, Some(2));
    }

    #[test]
    fn duplicate_keys_keep_the_first_and_warn() {
        let src = "@article{k, title={First}}\n@book{k, title={Second}}\n";
        let result = load_str("test.bib", src);
        assert_eq!(result.references.len(), 1);
        assert_eq!(result.references[0].title.as_deref(), Some("First"));
        assert_eq!(result.warnings[0].kind, WarningKind::DuplicateKey);
    }

    #[test]
    fn field_value_reads_crossref() {
        assert_eq!(
            field_value("@inproceedings{a, crossref = {conf2020}, title={x}}", "crossref").as_deref(),
            Some("conf2020")
        );
        assert_eq!(field_value("@inproceedings{a, mycrossref = {no}}", "crossref"), None);
    }
}
