//! CSL-JSON 1.0.2 ingestion; malformed fields warn and locations stay private.
//! Field vocabulary: citation-style-language/schema v1.0.2, schemas/input/csl-data.json.
use crate::{ParsedLibrary, SourceError, SourceWarning, WarningCode};
use ibis_core::contract::{CitationKey, CitationKeySource, ItemRef, LibraryId, Record};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

pub(crate) fn load_str(library: &LibraryId, text: &str) -> Result<ParsedLibrary, SourceError> {
    use serde_json::value::RawValue;
    if text.trim().is_empty() {
        return Ok(ParsedLibrary::default());
    }
    // RawValue validates/skips nested tokens iteratively. Only known CSL fields
    // are then deserialized, so private vendor metadata cannot exhaust a stack.
    struct BoundedItems;
    impl<'de> serde::de::Visitor<'de> for BoundedItems {
        type Value = Vec<&'de RawValue>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an array of reference objects")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
            let mut items = Vec::new();
            while let Some(item) = sequence.next_element::<&RawValue>()? {
                // Retain an overflow sentinel; still validate the whole input.
                if items.len() <= crate::MAX_LIBRARY_RECORDS {
                    items.push(item);
                }
            }
            Ok(items)
        }
    }
    let mut parser = serde_json::Deserializer::from_str(text);
    let items = serde::Deserializer::deserialize_seq(&mut parser, BoundedItems)
        .and_then(|items| parser.end().map(|()| items))
        .map_err(|error| {
            if error.is_eof() {
                SourceError::Incomplete
            } else {
                SourceError::InvalidData
            }
        })?;
    let limited = items.len() > crate::MAX_LIBRARY_RECORDS;
    let nonempty = !items.is_empty();
    let spans = item_spans(text);
    let mut result = ParsedLibrary::default();
    let mut seen = HashSet::new();
    for (index, raw) in items.into_iter().take(crate::MAX_LIBRARY_RECORDS).enumerate() {
        let line = spans.get(index).map(|(_, _, line)| *line);
        let warning = |code| SourceWarning { code, line };
        if raw.get().len() > 1024 * 1024 {
            result.warnings.push(warning(WarningCode::WorkLimit));
            continue;
        }
        let Ok(mut fields) = object_fields(raw.get()) else {
            result.warnings.push(warning(WarningCode::Syntax));
            continue;
        };
        let scalar = |name: &str| {
            fields
                .get(name)
                .and_then(|value| serde_json::from_str::<Value>(value.get()).ok())
        };
        let id = fields.get("id").and_then(|value| id_string(value));
        let numeric_native = fields
            .get("id")
            .filter(|value| {
                value
                    .get()
                    .starts_with(|character: char| character.is_ascii_digit() || character == '-')
            })
            .map(|value| value.get().to_owned());
        if fields.contains_key("id") && id.is_none() {
            result.warnings.push(warning(WarningCode::Field));
        }
        let explicit = scalar("citation-key")
            .and_then(|value| value.as_str().filter(|key| !key.trim().is_empty()).map(str::to_owned));
        if fields.contains_key("citation-key") && explicit.is_none() {
            result.warnings.push(warning(WarningCode::Field));
        }
        let Some(key) = explicit.clone().or_else(|| id.clone()) else {
            result.warnings.push(warning(WarningCode::Syntax));
            continue;
        };
        let native = id.as_deref().or(numeric_native.as_deref()).unwrap_or(&key);
        let kind = scalar("type");
        if native.trim().is_empty()
            || native.chars().any(char::is_control)
            || !kind.as_ref().and_then(Value::as_str).is_some_and(valid_type)
        {
            result.warnings.push(warning(WarningCode::Syntax));
            continue;
        }
        if key.len() > 64 * 1024 || native.len() > 64 * 1024 {
            result.warnings.push(warning(WarningCode::WorkLimit));
            continue;
        }
        if !seen.insert(native.to_owned()) {
            result.warnings.push(warning(WarningCode::DuplicateKey));
            continue;
        }
        let item = ItemRef {
            library: library.clone(),
            key: format!("record:{native}"),
        };
        if let Some(file) = fields.remove("file") {
            if let Ok(file) = serde_json::from_str::<String>(file.get()) {
                result.attachments.push((item.clone(), file, line));
            } else {
                result.warnings.push(warning(WarningCode::InvalidAttachment));
            }
        }
        let added_at = fields.remove("date-added").and_then(|value| serde_json::from_str::<String>(value.get()).ok()).and_then(|date| ibis_core::contract::added_date(&date));
        let mut csl = BTreeMap::new();
        for (name, value) in fields {
            // Test the vocabulary before parsing its payload. In particular,
            // unknown/custom values may have arbitrary depth or private paths.
            if !known_field(&name) {
                result.warnings.push(warning(WarningCode::Field));
                continue;
            }
            match serde_json::from_str::<Value>(value.get()) {
                Ok(value) if valid_field(&name, &value) => {
                    csl.insert(name, value);
                }
                _ => result.warnings.push(warning(WarningCode::Field)),
            }
        }
        if crate::urls::remove_private_urls(&mut csl) {
            result.warnings.push(warning(WarningCode::Field));
        }
        csl.insert("id".into(), Value::String(native.into()));
        for (alias, canonical) in [
            ("shortTitle", "title-short"),
            ("journalAbbreviation", "container-title-short"),
        ] {
            if let Some(value) = csl.remove(alias) {
                csl.entry(canonical.into()).or_insert(value);
            }
        }
        result.records.push(Record {
            added_at,
            id: item,
            csl,
            citation_keys: vec![CitationKey {
                value: key,
                source: if explicit.is_some() {
                    CitationKeySource::CslJsonCitationKey
                } else {
                    CitationKeySource::CslJsonId
                },
            }],
            aliases: vec![],
            identifiers: vec![],
        });
    }
    if nonempty && result.records.is_empty() {
        return Err(SourceError::InvalidData);
    }
    if limited {
        result.warnings.push(SourceWarning {
            code: WarningCode::WorkLimit,
            line: spans.get(crate::MAX_LIBRARY_RECORDS).map(|(_, _, line)| *line),
        });
    }
    Ok(result)
}
fn object_fields(text: &str) -> serde_json::Result<BTreeMap<String, &serde_json::value::RawValue>> {
    struct UniqueFields;
    impl<'de> serde::de::Visitor<'de> for UniqueFields {
        type Value = BTreeMap<String, &'de serde_json::value::RawValue>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an object with unique property names")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut fields = BTreeMap::new();
            while let Some((name, value)) = map.next_entry::<String, &serde_json::value::RawValue>()? {
                if fields.insert(name, value).is_some() {
                    return Err(serde::de::Error::custom("duplicate property"));
                }
            }
            Ok(fields)
        }
    }
    let mut parser = serde_json::Deserializer::from_str(text);
    let result = serde::Deserializer::deserialize_map(&mut parser, UniqueFields)?;
    parser.end()?;
    Ok(result)
}

fn known_field(name: &str) -> bool {
    field_kind(name).is_some()
}
#[derive(Clone, Copy)]
enum FieldKind {
    Text,
    Number,
    Strings,
    Names,
    Date,
}

fn id_string(value: &serde_json::value::RawValue) -> Option<String> {
    let raw = value.get();
    if raw.starts_with('"') {
        serde_json::from_str(raw).ok()
    } else {
        // std's parser rounds decimal tokens correctly. The default JSON float
        // parser can differ by one ULP and silently change a citation key.
        raw.parse::<f64>().ok().and_then(numeric_id)
    }
}
/// CSL numeric IDs pass through citeproc-js. Use its decimal spelling instead
/// of preserving JSON presentation (1.0, -0, exponent notation). Outside the
/// exact integer range, an export must use string IDs so distinct works cannot
/// silently collapse onto the same JavaScript number.
fn numeric_id(value: f64) -> Option<String> {
    if !value.is_finite() || value.abs() > 9_007_199_254_740_991.0 {
        return None;
    }
    if value == 0.0 {
        return Some("0".into());
    }
    let text = serde_json::Number::from_f64(value)?.to_string();
    let negative = text.starts_with('-');
    let unsigned = text.trim_start_matches('-');
    let (mantissa, exponent) = unsigned
        .split_once(['e', 'E'])
        .map_or(Some((unsigned, 0)), |(mantissa, exponent)| {
            exponent.parse::<i32>().ok().map(|exponent| (mantissa, exponent))
        })?;
    let point = mantissa.find('.').unwrap_or(mantissa.len()) as i32;
    let mut digits: String = mantissa.chars().filter(|character| *character != '.').collect();
    let leading = digits.bytes().take_while(|byte| *byte == b'0').count();
    digits.drain(..leading);
    if digits.is_empty() {
        return Some("0".into());
    }
    let position = point + exponent - leading as i32;
    while digits.len() > 1 && digits.ends_with('0') {
        digits.pop();
    }
    let scientific = position - 1;
    let mut result = if !(-6..21).contains(&scientific) {
        let rest = if digits.len() > 1 {
            format!(".{}", &digits[1..])
        } else {
            String::new()
        };
        format!(
            "{}{rest}e{}{scientific}",
            &digits[..1],
            if scientific >= 0 { "+" } else { "" }
        )
    } else if position <= 0 {
        format!("0.{}{digits}", "0".repeat((-position) as usize))
    } else if position as usize >= digits.len() {
        format!("{digits}{}", "0".repeat(position as usize - digits.len()))
    } else {
        let position = position as usize;
        format!("{}.{}", &digits[..position], &digits[position..])
    };
    if negative {
        result.insert(0, '-');
    }
    Some(result)
}

fn valid_type(value: &str) -> bool {
    matches!(
        value,
        "article"
            | "article-journal"
            | "article-magazine"
            | "article-newspaper"
            | "bill"
            | "book"
            | "broadcast"
            | "chapter"
            | "classic"
            | "collection"
            | "dataset"
            | "document"
            | "entry"
            | "entry-dictionary"
            | "entry-encyclopedia"
            | "event"
            | "figure"
            | "graphic"
            | "hearing"
            | "interview"
            | "legal_case"
            | "legislation"
            | "manuscript"
            | "map"
            | "motion_picture"
            | "musical_score"
            | "pamphlet"
            | "paper-conference"
            | "patent"
            | "performance"
            | "periodical"
            | "personal_communication"
            | "post"
            | "post-weblog"
            | "regulation"
            | "report"
            | "review"
            | "review-book"
            | "software"
            | "song"
            | "speech"
            | "standard"
            | "thesis"
            | "treaty"
            | "webpage"
    )
}
fn field_kind(name: &str) -> Option<FieldKind> {
    match name {
        "type"
        | "citation-key"
        | "language"
        | "journalAbbreviation"
        | "shortTitle"
        | "abstract"
        | "annote"
        | "archive"
        | "archive_collection"
        | "archive_location"
        | "archive-place"
        | "authority"
        | "call-number"
        | "citation-label"
        | "collection-title"
        | "container-title"
        | "container-title-short"
        | "dimensions"
        | "division"
        | "DOI"
        | "event"
        | "event-title"
        | "event-place"
        | "genre"
        | "ISBN"
        | "ISSN"
        | "jurisdiction"
        | "keyword"
        | "medium"
        | "note"
        | "original-publisher"
        | "original-publisher-place"
        | "original-title"
        | "part-title"
        | "PMCID"
        | "PMID"
        | "publisher"
        | "publisher-place"
        | "references"
        | "reviewed-genre"
        | "reviewed-title"
        | "scale"
        | "section"
        | "source"
        | "status"
        | "title"
        | "title-short"
        | "URL"
        | "version"
        | "volume-title"
        | "volume-title-short"
        | "year-suffix" => Some(FieldKind::Text),
        "id"
        | "chapter-number"
        | "citation-number"
        | "collection-number"
        | "edition"
        | "first-reference-note-number"
        | "issue"
        | "locator"
        | "number"
        | "number-of-pages"
        | "number-of-volumes"
        | "page"
        | "page-first"
        | "part"
        | "printing"
        | "supplement"
        | "volume" => Some(FieldKind::Number),
        "categories" => Some(FieldKind::Strings),
        "author" | "chair" | "collection-editor" | "compiler" | "composer" | "container-author" | "contributor"
        | "curator" | "director" | "editor" | "editorial-director" | "executive-producer" | "guest" | "host"
        | "interviewer" | "illustrator" | "narrator" | "organizer" | "original-author" | "performer" | "producer"
        | "recipient" | "reviewed-author" | "script-writer" | "series-creator" | "translator" => Some(FieldKind::Names),
        "accessed" | "available-date" | "event-date" | "issued" | "original-date" | "submitted" => {
            Some(FieldKind::Date)
        }
        // Custom metadata is retained verbatim in engine-only provenance: it may
        // include vendor file locations and is not a CSL formatting variable.
        _ => None,
    }
}
fn valid_field(name: &str, value: &Value) -> bool {
    match field_kind(name) {
        Some(FieldKind::Text) => value.is_string(),
        Some(FieldKind::Number) => value.is_string() || value.is_number(),
        Some(FieldKind::Strings) => value
            .as_array()
            .is_some_and(|values| values.iter().all(Value::is_string)),
        Some(FieldKind::Names) => value.as_array().is_some_and(|values| values.iter().all(valid_name)),
        Some(FieldKind::Date) => valid_date(value),
        None => false,
    }
}

fn valid_name(value: &Value) -> bool {
    value.as_object().is_some_and(|fields| {
        fields.iter().all(|(name, value)| match name.as_str() {
            "family" | "given" | "dropping-particle" | "non-dropping-particle" | "suffix" | "literal" => {
                value.is_string()
            }
            "comma-suffix" | "static-ordering" | "parse-names" => {
                value.is_string() || value.is_number() || value.is_boolean()
            }
            _ => false,
        })
    })
}
fn valid_date(value: &Value) -> bool {
    value.as_object().is_some_and(|fields| {
        fields.iter().all(|(name, value)| match name.as_str() {
            "date-parts" => value.as_array().is_some_and(|dates| {
                (1..=2).contains(&dates.len())
                    && dates.iter().all(|date| {
                        date.as_array().is_some_and(|parts| {
                            (1..=3).contains(&parts.len())
                                && parts.iter().all(|part| part.is_string() || part.is_number())
                        })
                    })
            }),
            "season" => value.is_string() || value.is_number(),
            "circa" => value.is_string() || value.is_number() || value.is_boolean(),
            "literal" | "raw" => value.is_string(),
            _ => false,
        })
    })
}

/// Locate already-validated top-level array items without searching for their
/// values (which may repeat). The scan is linear and never recurses.
fn item_spans(text: &str) -> Vec<(usize, usize, usize)> {
    let mut spans = Vec::new();
    let (mut depth, mut line) = (0_usize, 1_usize);
    let (mut quoted, mut escaped) = (false, false);
    let mut start = None;
    for (index, byte) in text.bytes().enumerate() {
        if spans.len() > crate::MAX_LIBRARY_RECORDS {
            break;
        }
        if !quoted && depth == 1 && start.is_none() && !byte.is_ascii_whitespace() && byte != b']' {
            start = Some((index, line));
        }
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
        } else {
            match byte {
                b'"' => quoted = true,
                b',' if depth == 1 => {
                    if let Some((start, line)) = start.take() {
                        spans.push((start, index, line));
                    }
                }
                b']' if depth == 1 => {
                    if let Some((start, line)) = start.take() {
                        spans.push((start, index, line));
                    }
                    depth -= 1;
                }
                b'[' | b'{' => depth += 1,
                b']' | b'}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        if byte == b'\n' {
            line += 1;
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn item_locations_keep_repeated_strings_nested_arrays_and_escapes() {
        let text = "[\n {\"id\":\"x\",\"title\":\"[, \\\" ]\"},\n {\"id\":\"x\",\"names\":[[1,2]]}\n]";
        let spans = item_spans(text);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans.iter().map(|(_, _, line)| *line).collect::<Vec<_>>(), [2, 3]);
        for (start, end, _) in spans {
            assert!(serde_json::from_str::<Value>(&text[start..end]).is_ok());
        }
    }
}
