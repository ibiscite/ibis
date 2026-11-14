//! What any output may use of a record: its bibliographic fields, never its
//! notes, annotations or private links.
//!
//! Every formatted citation, and every record shown to an AI agent, goes
//! through [`CitableFields`] first. The CSL `note` field holds Zotero's Extra
//! field or a file's `note`; `annote` holds annotations. Neither reaches a
//! formatter or an agent. Field overrides written in a note the way citeproc-js
//! reads them (`original-title: …` lines, or `{:title: …}`) are bibliographic
//! data the user wrote to be printed, so they become ordinary fields first,
//! filling only fields the record lacks, as Zotero's citations do.
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

pub(crate) mod text;
mod values;
use text::Projection;
pub use text::TextPart;
pub use values::{DoiLink, ExportName, PublicUrl, Verbatim};

/// Bibliographic metadata allowed to leave a record. Construction applies the
/// privacy filter once; callers can read or serialize it, never mutate it.
#[derive(Debug, serde::Serialize)]
#[serde(transparent)]
pub struct CitableFields {
    fields: Map<String, Value>,
    #[serde(skip)]
    plain: Map<String, Value>,
    #[serde(skip)]
    rich: BTreeMap<String, Vec<TextPart>>,
    #[serde(skip)]
    source_id: Option<String>,
    #[serde(skip)]
    doi: Option<DoiLink>,
    #[serde(skip)]
    identifiers: BTreeMap<String, Verbatim>,
    #[serde(skip)]
    names: BTreeMap<String, Vec<ExportName>>,
}

impl CitableFields {
    /// Apply note overrides, remove private fields and links, and reject embedded
    /// device references or malformed shapes hiding data inside a CSL variable.
    pub fn new(mut fields: Map<String, Value>) -> Self {
        let source_id = fields.remove("id").and_then(|mut id| {
            project_scalar(&mut id, false)?;
            id.as_str()
                .filter(|id| !id.is_empty() && id.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '_')))
                .map(str::to_owned)
        });
        fields.remove("annote");
        // Preserve source precedence, with a note fallback for a private URL.
        // Both candidates enter the projection exactly once.
        let source_url = fields.remove("URL");
        if let Some(Value::String(note)) = fields.remove("note") {
            apply_overrides(&mut fields, &note);
        }
        let fallback_url = fields.remove("URL");
        // Zotero's names for the short titles, which citeproc copies in when the
        // CSL ones are missing.
        for (alias, canonical) in [
            ("shortTitle", "title-short"),
            ("journalAbbreviation", "container-title-short"),
        ] {
            if let Some(value) = fields.remove(alias)
                && !fields.get(canonical).is_some_and(truthy)
            {
                fields.insert(canonical.into(), value);
            }
        }
        let mut rich = BTreeMap::new();
        let mut export_names = BTreeMap::new();
        let scalar = |value: &mut Value| project_scalar(value, false).is_some();
        fields.retain(|name, value| {
            if !is_variable(name) {
                false
            } else if NAMES.contains(&name.as_str()) {
                let Value::Array(names) = value else { return false };
                names.retain_mut(|name| {
                    let Value::Object(parts) = name else { return false };
                    parts.retain(|name, value| NAME_PARTS.contains(&name.as_str()) && scalar(value));
                    !parts.is_empty()
                });
                export_names.insert(
                    name.to_owned(),
                    names
                        .iter()
                        .filter_map(|name| Some(ExportName::new(name.as_object()?.clone())))
                        .collect::<Vec<_>>(),
                );
                *names = export_names[name].iter().map(|name| json!(name)).collect();
                !names.is_empty()
            } else if DATES.contains(&name.as_str()) {
                let Value::Object(parts) = value else { return false };
                parts.retain(|part, _| DATE_PARTS.contains(&part.as_str()));
                let has_raw = parts
                    .get("raw")
                    .and_then(Value::as_str)
                    .is_some_and(|raw| !raw.is_empty());
                parts.retain(|name, value| {
                    if name == "date-parts" {
                        value.as_array_mut().is_some_and(|dates| {
                            dates.iter_mut().all(|date| {
                                date.as_array_mut().is_some_and(|parts| {
                                    parts.iter_mut().all(|part| {
                                        if !scalar(part) {
                                            return false;
                                        }
                                        if let Some(number) =
                                            part.as_str().and_then(|part| part.trim().parse::<i64>().ok())
                                        {
                                            *part = json!(number);
                                        }
                                        part.is_number()
                                    })
                                })
                            })
                        })
                    } else {
                        scalar(value)
                    }
                });
                normalize_raw_date(parts, has_raw);
                if let Some(circa) = parts.get_mut("circa") {
                    *circa = Value::Bool(truthy(circa));
                }
                !parts.is_empty()
            } else if matches!(value, Value::String(_) | Value::Number(_)) {
                let Some(projected) = project_scalar(value, true) else {
                    return false;
                };
                if value.is_string() && RICH_TEXT.contains(&name.as_str()) {
                    rich.insert(name.to_owned(), projected);
                }
                true
            } else {
                false
            }
        });
        let checked_url = |mut value: Value| {
            project_scalar(&mut value, false)?;
            PublicUrl::new(value.as_str()?)
        };
        if let Some(url) = source_url
            .and_then(checked_url)
            .or_else(|| fallback_url.and_then(checked_url))
        {
            fields.insert("URL".into(), json!(url.as_str()));
        }
        let doi = fields.remove("DOI").and_then(|value| DoiLink::new(value.as_str()?));
        if let Some(doi) = &doi {
            fields.insert("DOI".into(), json!(doi.identifier()));
        }
        let identifiers = ["URL", "DOI", "ISBN", "ISSN", "PMID", "PMCID"]
            .into_iter()
            .filter_map(|name| {
                let text = fields.get(name)?.as_str()?;
                let text = if name == "URL" {
                    text.replace('{', "%7B").replace('}', "%7D")
                } else {
                    text.into()
                };
                Some((name.into(), Verbatim::new(&text)?))
            })
            .collect();
        let plain = fields;
        let mut fields = plain.clone();
        encode_values(fields.values_mut());
        for (name, parts) in &rich {
            fields.insert(name.clone(), json!(text::encode(parts)));
        }
        Self {
            fields,
            plain,
            rich,
            source_id,
            doi,
            identifiers,
            names: export_names,
        }
    }

    /// Decoded inline text already checked by the privacy projection. Consumers
    /// must escape it for their representation, never interpret entities again.
    pub fn plain(&self) -> &Map<String, Value> {
        &self.plain
    }

    /// Checked literal text and supported markup, without downstream decoding.
    pub fn rich_text(&self) -> &BTreeMap<String, Vec<TextPart>> {
        &self.rich
    }

    /// A plain source identity token; library URLs never leave as export IDs.
    pub fn source_id(&self) -> Option<&str> {
        self.source_id.as_deref()
    }

    pub fn doi(&self) -> Option<&DoiLink> {
        self.doi.as_ref()
    }

    pub fn identifiers(&self) -> &BTreeMap<String, Verbatim> {
        &self.identifiers
    }

    pub fn names(&self, role: &str) -> &[ExportName] {
        self.names.get(role).map_or(&[], Vec::as_slice)
    }
}

const RICH_TEXT: [&str; 6] = [
    "title",
    "title-short",
    "container-title",
    "container-title-short",
    "volume-title",
    "volume-title-short",
];

fn project_scalar(value: &mut Value, keep_markup: bool) -> Option<Vec<TextPart>> {
    match value {
        Value::String(source) => {
            let projected = Projection::new(source);
            if points_to_device(&projected.plain) || points_to_device(&projected.attributes) {
                return None;
            }
            *source = projected.plain;
            Some(if keep_markup { projected.parts } else { Vec::new() })
        }
        Value::Number(_) | Value::Bool(_) => Some(Vec::new()),
        _ => None,
    }
}

fn encode_values<'a>(values: impl Iterator<Item = &'a mut Value>) {
    for value in values {
        match value {
            Value::String(text) => *text = text::encode_plain(text),
            Value::Object(fields) => encode_values(fields.values_mut()),
            Value::Array(values) => encode_values(values.iter_mut()),
            _ => {}
        }
    }
}

const NAME_PARTS: [&str; 9] = [
    "family",
    "given",
    "literal",
    "suffix",
    "dropping-particle",
    "non-dropping-particle",
    "comma-suffix",
    "static-ordering",
    "parse-names",
];
const DATE_PARTS: [&str; 5] = ["date-parts", "season", "circa", "literal", "raw"];

/// The numeric year and optional source letters of an already literal year.
/// Every alphabetic suffix is data here, including calendar and era names.
pub fn literal_year(value: &str) -> Option<(i32, &str)> {
    let (year, suffix) = value.trim().split_at_checked(4)?;
    let suffix = suffix.trim();
    if !(year.bytes().all(|byte| byte.is_ascii_digit()) && suffix.bytes().all(|byte| byte.is_ascii_alphabetic())) {
        return None;
    }
    Some((year.parse().ok()?, suffix))
}

/// Dates with source-supplied year letters or publication status must not go
/// through a permissive parser that silently keeps only their numeric year.
/// Shared by source dates and note/Extra date overrides; this does not generate
/// document-level disambiguation letters.
pub fn literal_date(value: &str) -> bool {
    let text = value.trim().to_ascii_lowercase();
    let suffix = literal_year(&text).is_some_and(|(_, suffix)| {
        // Unclassified source dates still leave calendar and era names to the
        // date parser; explicitly literal dates have already made that choice.
        !suffix.is_empty()
            && !matches!(
                suffix.to_ascii_lowercase().as_str(),
                "ad" | "bc"
                    | "bce"
                    | "ce"
                    | "jan"
                    | "january"
                    | "feb"
                    | "february"
                    | "mar"
                    | "march"
                    | "apr"
                    | "april"
                    | "may"
                    | "jun"
                    | "june"
                    | "jul"
                    | "july"
                    | "aug"
                    | "august"
                    | "sep"
                    | "sept"
                    | "september"
                    | "oct"
                    | "october"
                    | "nov"
                    | "november"
                    | "dec"
                    | "december"
                    | "spring"
                    | "summer"
                    | "autumn"
                    | "fall"
                    | "winter"
            )
    });
    let mut previous = "";
    suffix
        || text
            .split(|ch: char| !ch.is_ascii_alphabetic())
            .filter(|word| !word.is_empty())
            .any(|word| {
                let status = matches!(word, "forthcoming" | "unpublished" | "submitted" | "accepted")
                    || matches!((previous, word), ("in", "press") | ("to", "appear"));
                previous = word;
                status
            })
}

// Raw parsing replaces the whole date when structured parts are absent/empty.
// Resolve that priority once for note overrides and CSL dates, before projecting
// sort data. Curly delimiters need explicit normalization; ASCII stays native.
fn normalize_raw_date(date: &mut Map<String, Value>, has_raw: bool) {
    if !has_raw
        || date
            .get("date-parts")
            .is_some_and(|parts| !parts.as_array().is_some_and(Vec::is_empty))
    {
        return;
    }
    // Source raw wins even if normalizing its line breaks left it empty.
    let literal = date
        .get("raw")
        .and_then(Value::as_str)
        .and_then(|raw| raw.trim().strip_prefix('“').and_then(|raw| raw.strip_suffix('”')))
        .map(|text| Value::String(text.into()));
    if let Some(literal) = literal {
        date.clear();
        date.insert("literal".into(), literal);
    } else {
        // Keep the exact raw string for native parsing, discarding only fields
        // that processor would replace (literal, circa, season, empty parts).
        date.retain(|part, _| part == "raw");
    }
}

/// A CSL variable a note may set and an agent may see. Fields a source keeps
/// beyond these (custom keys, processor state) are not bibliographic data.
fn is_variable(name: &str) -> bool {
    name == "type" || TEXT.contains(&name) || NAMES.contains(&name) || DATES.contains(&name)
}

/// Inspect only the decoded, normalized projection that consumers receive.
pub(crate) fn points_to_device(value: &str) -> bool {
    let path = |word: &str| {
        let bytes = word.as_bytes();
        (word.starts_with('/') || word.starts_with("~/")) && word.len() > 2
            || word.starts_with("\\\\")
            || (bytes.len() > 3 && bytes[0].is_ascii_alphabetic() && matches!(&bytes[1..3], b":\\" | b":/"))
            || word
                .get(..6)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("file:/"))
            || word
                .get(..8)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("zotero:/"))
    };
    value.split_whitespace().any(|word| {
        let word = word.trim_matches(|c: char| "()[]{},.\"'".contains(c));
        path(word)
            || word.char_indices().any(|(index, c)| {
                let rest = &word[index + c.len_utf8()..];
                // A scheme separator is not the beginning of a device path.
                matches!(c, '=' | ':' | '\"' | '\'' | '(' | '[' | '{' | ',' | ';' | '<' | '>')
                    && !(c == ':' && rest.starts_with("//"))
                    // Escaped tags remain literal text in this projection.
                    // Reuse the markup grammar to distinguish them from paths
                    // in angle brackets; attributes are still checked below.
                    && !(c == '<'
                        && crate::model::markup_tag(&word[index..]).is_some_and(|(_, tail)| tail.starts_with('>')))
                    && path(rest)
            })
            || (word.contains("://") && PublicUrl::new(word.trim_matches(['<', '>'])).is_none())
    })
}

const DOCUMENT_SUFFIXES: &[&str] = &[
    "pdf", "epub", "djvu", "doc", "docx", "odt", "rtf", "txt", "md", "tex", "bib", "ris", "json", "sqlite", "html",
    "htm", "png", "jpg", "jpeg", "gif", "svg", "webp", "mp3", "mp4", "zip",
];

const NAMES: &[&str] = &[
    "author",
    "chair",
    "collection-editor",
    "compiler",
    "composer",
    "container-author",
    "contributor",
    "curator",
    "director",
    "editor",
    "editor-translator",
    "editorial-director",
    "executive-producer",
    "guest",
    "host",
    "illustrator",
    "interviewer",
    "narrator",
    "organizer",
    "original-author",
    "performer",
    "producer",
    "recipient",
    "reviewed-author",
    "script-writer",
    "series-creator",
    "translator",
    "commenter",
];

const DATES: &[&str] = &[
    "issued",
    "event-date",
    "accessed",
    "original-date",
    "publication-date",
    "available-date",
    "submitted",
    "container",
];

/// CSL 1.0.2 text and number variables a note may set. Processor-managed
/// variables (locator, citation-number, year-suffix, …) and the note itself
/// are not on the list.
const TEXT: &[&str] = &[
    "abstract",
    "archive",
    "archive_collection",
    "archive_location",
    "archive-place",
    "authority",
    "call-number",
    "chapter-number",
    "citation-label",
    "collection-number",
    "collection-title",
    "container-title",
    "container-title-short",
    "dimensions",
    "division",
    "DOI",
    "edition",
    "event",
    "event-place",
    "event-title",
    "genre",
    "ISBN",
    "ISSN",
    "issue",
    "jurisdiction",
    "keyword",
    "language",
    "license",
    "medium",
    "number",
    "number-of-pages",
    "number-of-volumes",
    "original-publisher",
    "original-publisher-place",
    "original-title",
    "page",
    "part-number",
    "part-title",
    "PMCID",
    "PMID",
    "printing-number",
    "publisher",
    "publisher-place",
    "references",
    "reviewed-genre",
    "reviewed-title",
    "scale",
    "section",
    "source",
    "status",
    "supplement-number",
    "title",
    "title-short",
    "URL",
    "version",
    "volume",
    "volume-title",
    "volume-title-short",
    "collection-title-short",
];

/// citeproc splits only at `\n`, so a CR or Unicode separator after an
/// override would carry the following prose into the field. Every line break
/// ends a field here.
const LINE_BREAKS: [char; 4] = ['\n', '\r', '\u{2028}', '\u{2029}'];

/// citeproc-js `parseNoteFieldHacks` with dates allowed to override, as the
/// bundled processor runs it, limited to CSL variables.
fn apply_overrides(fields: &mut Map<String, Value>, note: &str) {
    let mut lines = Vec::new();
    for (index, line) in note.split(LINE_BREAKS).enumerate() {
        lines.push(inline_fields(line, index == 0));
    }
    let lines = lines.join("\n");
    let mut names = Map::new();
    for (index, line) in lines.split(LINE_BREAKS).enumerate() {
        if js_trim(line).is_empty() {
            continue;
        }
        let Some((key, value)) = field_line(line) else {
            if index == 0 {
                continue;
            }
            break;
        };
        let value = js_trim(value);
        if value.is_empty() {
            continue;
        }
        if key == "type" {
            fields.insert(key.into(), json!(value));
        } else if DATES.contains(&key) {
            // ASCII quotes use native raw-date parsing; curly delimiters are
            // normalized with all other date fields below.
            let quoted = value.len() > 1
                && ((value.starts_with('"') && value.ends_with('"'))
                    || (value.starts_with('“') && value.ends_with('”')));
            fields.insert(
                key.into(),
                if !quoted && literal_date(value) {
                    json!({"literal":value})
                } else {
                    json!({"raw":value})
                },
            );
        } else if fields.get(key).is_some_and(truthy) {
            // A note fills gaps; it never replaces the record's own data.
        } else if NAMES.contains(&key) {
            let name = match value.split("||").map(js_trim).collect::<Vec<_>>()[..] {
                [literal] => json!({"literal": literal}),
                [family, given] => json!({"family": family, "given": given}),
                _ => continue,
            };
            match names.get_mut(key) {
                Some(Value::Array(list)) => list.push(name),
                _ => {
                    names.insert(key.into(), json!([name]));
                }
            }
        } else if TEXT.contains(&key) {
            fields.insert(key.into(), json!(value));
        }
    }
    fields.extend(names);
}

/// citeproc's first pass: `{:key: value}` becomes a line of its own, unless
/// prose that is not itself a field comes before it (the very first one on the
/// note's first line may follow prose).
fn inline_fields(line: &str, first_line: bool) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    let mut first = true;
    while let Some((start, end)) = next_inline(rest) {
        let before = &rest[..start];
        if !js_trim(before).is_empty() && (!first_line || !first) && field_line(before).is_none() {
            break;
        }
        out.push_str(before);
        out.push('\n');
        out.push_str(js_trim(&rest[start + 2..end - 1]));
        out.push('\n');
        rest = &rest[end..];
        first = false;
    }
    out.push_str(rest);
    out
}

/// The next match of `\{:(?:[\-_a-z]+|[A-Z]+):[^\}]+\}`, as byte offsets.
fn next_inline(text: &str) -> Option<(usize, usize)> {
    let mut from = 0;
    while let Some(offset) = text[from..].find("{:") {
        let start = from + offset;
        let body = &text[start + 2..];
        let key = key_length(body);
        if key > 0 && body[key..].starts_with(':') {
            // No later candidate can close either: stop rather than rescan.
            let close = body[key + 1..].find('}')?;
            if close > 0 {
                return Some((start, start + 2 + key + 1 + close + 1));
            }
        }
        from = start + 1;
    }
    None
}

/// `^([\-_a-z]+|[A-Z]+):\s*([^\}]+)$`
fn field_line(line: &str) -> Option<(&str, &str)> {
    let key = key_length(line);
    let value = line[key..].strip_prefix(':')?;
    (key > 0 && !value.is_empty() && !value.contains('}')).then(|| (&line[..key], value))
}

fn key_length(text: &str) -> usize {
    let lower = text
        .bytes()
        .take_while(|b| b.is_ascii_lowercase() || matches!(b, b'-' | b'_'))
        .count();
    if lower > 0 {
        lower
    } else {
        text.bytes().take_while(u8::is_ascii_uppercase).count()
    }
}

/// JavaScript's `\s`, which `String.prototype.trim` also uses.
fn js_trim(text: &str) -> &str {
    text.trim_matches(|c: char| {
        matches!(
            c,
            '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
                ..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
        )
    })
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(number) => number.as_f64().is_some_and(|number| number != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn literal_year_suffixes_and_source_calendar_dates_are_distinct() {
        for suffix in ["ad", "AD", "bc", "bce", "ce", "january", "may", "spring"] {
            let date = format!("2020{suffix}");
            assert_eq!(literal_year(&date), Some((2020, suffix)));
            assert!(!literal_date(&date), "{date} stays with the source date parser");
        }
        for date in ["2020ac", "2020ae", "2020a", "in press"] {
            assert!(literal_date(date), "{date} stays literal");
        }
    }

    fn citable(fields: Value) -> Value {
        let Value::Object(fields) = fields else { unreachable!() };
        serde_json::to_value(CitableFields::new(fields)).unwrap()
    }

    #[test]
    fn notes_and_annotations_never_pass() {
        assert_eq!(
            citable(
                json!({"title": "Invented title", "note": "Invented private memo", "annote": "Invented annotation"})
            ),
            json!({"title": "Invented title"})
        );
    }

    #[test]
    fn overrides_fill_only_missing_fields() {
        let note = "Invented prose first\noriginal-title: Invented earlier title\ntitle: Ignored title\n{:publisher-place: Invented City}\nauthor: Doe || Jane\nauthor: Invented Group\nissued: 1859\nMore invented prose\ngenre: after prose is ignored";
        assert_eq!(
            citable(json!({"title": "Invented title", "note": note})),
            json!({
                "title": "Invented title",
                "original-title": "Invented earlier title",
                "publisher-place": "Invented City",
                "author": [{"family": "Doe", "given": "Jane"}, {"literal": "Invented Group"}],
                "issued": {"raw": "1859"},
            })
        );
    }

    #[test]
    fn inline_overrides_follow_citeproc() {
        // The first inline field on the first line may follow prose; later
        // ones, and any on later lines, may not.
        assert_eq!(
            citable(json!({"note": "Invented prose {:title: Invented title} more {:genre: Invented genre}"})),
            json!({"title": "Invented title"})
        );
        assert_eq!(
            citable(json!({"note": "\nInvented prose {:title: Invented title}"})),
            json!({})
        );
        assert_eq!(
            citable(json!({"note": "{:title: Invented title}{:DOI: 10.1000/invented}"})),
            json!({"title": "Invented title", "DOI": "10.1000/invented"})
        );
    }

    #[test]
    fn only_csl_variables_and_public_urls_are_lifted() {
        assert_eq!(
            citable(
                json!({"note": "todo: invented private task\nnote: invented\nannote: invented\nURL: zotero://select/library/items/INVENTED"})
            ),
            json!({})
        );
        assert_eq!(
            citable(
                json!({"URL": "zotero://open-pdf/library/items/INVENTED", "note": "URL: https://example.invalid/work"})
            ),
            json!({"URL": "https://example.invalid/work"})
        );
    }

    #[test]
    fn private_and_application_urls_are_not_public() {
        for url in [
            "zotero://open-pdf/library/items/INVENTED",
            "file:///home/invented/private.pdf",
            "x-devonthink-item://INVENTED",
            "C:\\invented\\private.pdf",
            "/home/invented/private.pdf",
            "private.pdf",
            "papers.zip",
            "https://example.invalid /home/invented",
        ] {
            assert!(PublicUrl::new(url).is_none(), "{url}");
        }
        for url in [
            "https://example.invalid/work",
            "example.invalid/work",
            "doi:10.1000/invented",
        ] {
            assert!(PublicUrl::new(url).is_some(), "{url}");
        }
    }

    #[test]
    fn custom_keys_and_name_or_date_parts_are_dropped() {
        assert_eq!(
            citable(json!({
                "x-todo": "invented private task",
                "author": [{"family": "Invented", "x-private": "invented"}],
                "issued": {"date-parts": [[2020]], "x-private": "invented"},
            })),
            json!({"author": [{"family": "Invented"}], "issued": {"date-parts": [[2020]]}})
        );
    }

    #[test]
    fn zotero_short_titles_become_csl_ones() {
        assert_eq!(
            citable(json!({"journalAbbreviation": "Inv. J.", "shortTitle": "Invented", "title-short": "Kept"})),
            json!({"container-title-short": "Inv. J.", "title-short": "Kept"})
        );
    }

    #[test]
    fn every_line_break_ends_an_override() {
        for separator in ["\r", "\u{2028}", "\u{2029}", "\r\n"] {
            assert_eq!(
                citable(json!({"note": format!("title: Invented title{separator}Invented private memo")})),
                json!({"title": "Invented title"}),
                "{separator:?}"
            );
        }
    }

    #[test]
    fn unclosed_inline_fields_take_linear_time() {
        let started = std::time::Instant::now();
        assert_eq!(citable(json!({"note": "{:a:".repeat(1 << 18)})), json!({}));
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }
}
