//! Pure, bounded Zotero-to-CSL conversion shared by both future access paths.
//! No filesystem, transport, installed Zotero, or library discovery lives here.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use ibis_core::contract::{CitationKey, CitationKeySource, ItemRef, LibraryId, RawProvenance, Record, SourceFormat};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

mod creators;
mod dates;
mod extra;

/// Access adapters construct this same neutral shape. Metadata such as API
/// versions, database IDs, and locations do not masquerade as citation fields.
/// BBT values must already have been obtained by its separate versioned adapter.
/// Date fields contain the typed text (Zotero's `multipartToStr`), never the
/// database's sortable multipart prefix. `accessDate` is a SQL UTC or ISO value.
/// This is not a raw API response: adapters select bibliographic string fields.
///
/// As in Zotero's CSL export, non-date Extra overrides remain in CSL `note`
/// for citeproc to consume once. Inspecting `csl` does not apply those overrides;
/// their complete content still participates in the citation fingerprint.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoteroItem {
    pub key: String,
    #[serde(rename = "itemType")]
    pub item_type: String,
    #[serde(default)]
    pub creators: Vec<ZoteroCreator>,
    #[serde(default)]
    pub better_bibtex_keys: Vec<String>,
    #[serde(flatten)]
    pub fields: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ZoteroCreator {
    pub creator_type: String,
    #[serde(default)]
    pub first_name: String,
    #[serde(default)]
    pub last_name: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub field_mode: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappedItem {
    pub record: Record,
    pub warnings: Vec<MappingWarning>,
}

/// Fixed codes only: diagnostics never copy item fields, labels or locations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MappingWarning {
    UnknownField,
    SkippedCreator,
    InvalidCitationKey,
    InvalidAlias,
    PrivateUrl,
    DateParseLimit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum MappingError {
    #[error("invalid Zotero item identity")]
    InvalidIdentity,
    #[error("unsupported Zotero item type")]
    UnsupportedType,
    #[error("Zotero mapping input limit exceeded")]
    InputLimit,
    #[error("invalid bundled Zotero schema")]
    InvalidSchema,
}

const MAX_FIELDS: usize = 256;
const MAX_CREATORS: usize = 4096;
const MAX_KEYS: usize = 4096;
const MAX_FIELD_BYTES: usize = 256 * 1024;
const MAX_ITEM_BYTES: usize = 2 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Schema {
    item_types: Vec<ItemType>,
    csl: CslSchema,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItemType {
    item_type: String,
    fields: Vec<Field>,
    creator_types: Vec<CreatorType>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Field {
    field: String,
    base_field: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreatorType {
    creator_type: String,
    #[serde(default)]
    primary: bool,
}
#[derive(Deserialize)]
struct CslSchema {
    types: BTreeMap<String, Vec<String>>,
    fields: FieldMappings,
    names: BTreeMap<String, String>,
}
#[derive(Deserialize)]
struct FieldMappings {
    text: BTreeMap<String, Vec<String>>,
    date: BTreeMap<String, String>,
}

fn schema() -> Result<&'static Schema, MappingError> {
    static SCHEMA: OnceLock<Result<Schema, serde_json::Error>> = OnceLock::new();
    SCHEMA
        .get_or_init(|| serde_json::from_str(include_str!("../../vendor/zotero/schema.json")))
        .as_ref()
        .map_err(|_| MappingError::InvalidSchema)
}

/// Maps one bibliographic work. Identity remains library + native item key;
/// citation-key priority never hides disagreeing values from other sources.
pub fn map_item(library: LibraryId, item: &ZoteroItem) -> Result<MappedItem, MappingError> {
    validate(item)?;
    let schema = schema()?;
    let kind = schema
        .item_types
        .iter()
        .find(|kind| kind.item_type == item.item_type)
        .ok_or(MappingError::UnsupportedType)?;
    let csl_type = schema
        .csl
        .types
        .iter()
        .find(|(_, types)| types.contains(&item.item_type))
        .map(|(csl, _)| csl)
        .ok_or(MappingError::UnsupportedType)?;
    let mut warnings = Vec::new();
    if item
        .fields
        .keys()
        .any(|name| !kind.fields.iter().any(|field| field.field == *name))
    {
        warn(&mut warnings, MappingWarning::UnknownField);
    }
    let mut csl = BTreeMap::from([("type".into(), json!(csl_type))]);
    for (variable, fields) in &schema.csl.fields.text {
        if variable == "shortTitle" {
            continue;
        }
        let event_place = matches!(
            item.item_type.as_str(),
            "audioRecording" | "presentation" | "videoRecording"
        );
        if event_place && variable == "publisher-place" {
            continue;
        }
        let place = vec!["place".into()];
        let fields = if event_place && variable == "event-place" {
            &place
        } else {
            fields
        };
        for field in fields {
            let Some(value) = field_value(item, kind, field).filter(|value| !value.is_empty()) else {
                continue;
            };
            let mut value = if field == "ISBN" {
                first_isbn(value).into()
            } else if field == "extra" {
                extra::normalize(value, schema)
            } else {
                value.to_owned()
            };
            // Zotero strips a single enclosing pair, not arbitrary inner quotes.
            if value.starts_with('"') && value.len() > 1 && value[1..].find('"') == Some(value.len() - 2) {
                value = value[1..value.len() - 1].to_owned();
            }
            csl.insert(variable.clone(), json!(value));
            break;
        }
    }
    creators::map(item, kind, &schema.csl.names, &mut csl, &mut warnings);
    for (variable, field) in &schema.csl.fields.date {
        let value = item
            .fields
            .get(field)
            .filter(|value| !value.is_empty())
            .map(String::as_str)
            .or_else(|| {
                kind.fields
                    .iter()
                    .find(|candidate| candidate.base_field.as_ref() == Some(field))
                    .and_then(|candidate| item.fields.get(&candidate.field))
                    .map(String::as_str)
            });
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            if value.len() > dates::MAX_PARSE_BYTES {
                warn(&mut warnings, MappingWarning::DateParseLimit);
            }
            csl.insert(variable.clone(), dates::to_csl(value, field == "accessDate"));
        }
    }
    if let Some(note) = csl.get("note").and_then(Value::as_str) {
        let (note, overrides) = extra::date_overrides(note);
        csl.insert("note".into(), json!(note));
        csl.extend(overrides);
    }
    // Search identifiers survive citation-only URL suppression without printing
    // a URL that Zotero would omit from a printed article.
    let identifiers = item
        .fields
        .get("url")
        .and_then(|url| ibis_core::identifier::parse_query(url))
        .into_iter()
        .collect();
    // Zotero's default cite.js behavior: printed periodical articles with pages
    // omit both URL and access date, even if an access timestamp was supplied.
    if matches!(
        item.item_type.as_str(),
        "journalArticle" | "newspaperArticle" | "magazineArticle"
    ) && item.fields.get("pages").is_some_and(|value| !value.is_empty())
    {
        csl.remove("URL");
        csl.remove("accessed");
    }
    if crate::urls::remove_private_urls(&mut csl) {
        warn(&mut warnings, MappingWarning::PrivateUrl);
    }
    let (citation_keys, aliases) = keys(item, &mut warnings)?;
    // csl citation-key is presentation data only; key output always resolves the
    // full provenance list and existing cross-library ambiguity/refusal rules.
    let raw = serde_json::to_value(item).map_err(|_| MappingError::InputLimit)?;
    Ok(MappedItem {
        record: Record {
            id: ItemRef {
                library,
                key: item.key.clone(),
            },
            csl,
            citation_keys,
            aliases,
            identifiers,
            raw: Some(RawProvenance {
                format: SourceFormat::Zotero,
                data: raw,
            }),
        },
        warnings,
    })
}

fn validate(item: &ZoteroItem) -> Result<(), MappingError> {
    if item.key.is_empty() || item.key.len() > 128 || item.key.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(MappingError::InvalidIdentity);
    }
    if matches!(item.item_type.as_str(), "attachment" | "note" | "annotation") {
        return Err(MappingError::UnsupportedType);
    }
    if item.fields.len() > MAX_FIELDS || item.creators.len() > MAX_CREATORS || item.better_bibtex_keys.len() > MAX_KEYS
    {
        return Err(MappingError::InputLimit);
    }
    let values = item
        .fields
        .iter()
        .flat_map(|(key, value)| [key.as_str(), value.as_str()])
        .chain(item.creators.iter().flat_map(|c| {
            [
                c.creator_type.as_str(),
                c.first_name.as_str(),
                c.last_name.as_str(),
                c.name.as_str(),
            ]
        }))
        .chain(item.better_bibtex_keys.iter().map(String::as_str))
        .chain([item.item_type.as_str()]);
    let mut bytes = 0usize;
    for value in values {
        if value.len() > MAX_FIELD_BYTES {
            return Err(MappingError::InputLimit);
        }
        bytes = bytes.saturating_add(value.len());
        if bytes > MAX_ITEM_BYTES {
            return Err(MappingError::InputLimit);
        }
    }
    Ok(())
}

fn field_value<'a>(item: &'a ZoteroItem, kind: &ItemType, name: &str) -> Option<&'a str> {
    if let Some(value) = item.fields.get(name) {
        return Some(value);
    }
    // Zotero schema aliases a type-specific field to its base field. A present
    // empty base field suppresses the alias, matching itemToCSLJSON.
    let field = kind
        .fields
        .iter()
        .find(|field| field.base_field.as_deref() == Some(name))?;
    item.fields.get(&field.field).map(String::as_str)
}

fn keys(
    item: &ZoteroItem,
    warnings: &mut Vec<MappingWarning>,
) -> Result<(Vec<CitationKey>, Vec<String>), MappingError> {
    let mut keys = Vec::new();
    let mut seen = BTreeSet::new();
    let mut add = |value: &str, source| {
        let value = value.trim();
        if value.is_empty() {
            return Ok(());
        }
        if !valid_key(value) {
            warn(warnings, MappingWarning::InvalidCitationKey);
        }
        let key = CitationKey {
            value: value.into(),
            source,
        };
        let source_id = match source {
            CitationKeySource::Zotero => 0,
            CitationKeySource::Extra => 1,
            _ => 2,
        };
        if seen.insert((source_id, value.to_owned())) {
            if keys.len() == MAX_KEYS {
                return Err(MappingError::InputLimit);
            }
            keys.push(key);
        }
        Ok(())
    };
    if let Some(value) = item.fields.get("citationKey") {
        add(value, CitationKeySource::Zotero)?;
    }
    let extra = item.fields.get("extra").map(String::as_str).unwrap_or_default();
    for line in extra.lines() {
        if let Some((label, value)) = line.split_once(':')
            && citation_key_label(label)
        {
            add(value, CitationKeySource::Extra)?;
        }
    }
    let mut remaining = extra;
    while let Some(start) = remaining.find("{:") {
        remaining = &remaining[start + 2..];
        // Match the same field-label grammar as citeproc. An invalid outer
        // opening cannot hide a valid nested field; a valid outer field owns
        // its complete value, including any nested opening, through `}`.
        let label_end = remaining
            .bytes()
            .take_while(|byte| byte.is_ascii_alphabetic() || matches!(byte, b'-' | b'_'))
            .count();
        let label = &remaining[..label_end];
        if label.is_empty()
            || remaining.as_bytes().get(label_end) != Some(&b':')
            || !(label.bytes().all(|byte| byte.is_ascii_uppercase())
                || label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || matches!(byte, b'-' | b'_')))
        {
            continue;
        }
        let Some(end) = remaining.find('}') else {
            break;
        };
        if citation_key_label(label) {
            add(&remaining[label_end + 1..end], CitationKeySource::Extra)?;
        }
        remaining = &remaining[end + 1..];
    }
    for value in &item.better_bibtex_keys {
        add(value, CitationKeySource::BetterBibtex)?;
    }
    let mut aliases = BTreeSet::new();
    for line in extra.lines() {
        if let Some((label, values)) = line.split_once([':', '='])
            && label.trim().eq_ignore_ascii_case("tex.ids")
        {
            for value in values.split(',').map(str::trim).filter(|value| !value.is_empty()) {
                if valid_key(value) {
                    aliases.insert(value.to_owned());
                    if aliases.len() > MAX_KEYS {
                        return Err(MappingError::InputLimit);
                    }
                } else {
                    warn(warnings, MappingWarning::InvalidAlias);
                }
            }
        }
    }
    Ok((keys, aliases.into_iter().collect()))
}

fn valid_key(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(|c| c.is_whitespace() || c.is_control())
}

fn citation_key_label(label: &str) -> bool {
    // Zotero's extraToCSL maps written-out and camel-case field labels to
    // citation-key. Retain those same explicit sources before citeproc reads note.
    label.trim().eq_ignore_ascii_case("citation key")
        || label.trim().eq_ignore_ascii_case("citation-key")
        || label.trim().eq_ignore_ascii_case("citationKey")
}

fn warn(warnings: &mut Vec<MappingWarning>, warning: MappingWarning) {
    if !warnings.contains(&warning) {
        warnings.push(warning);
    }
}

fn first_isbn(value: &str) -> &str {
    // Upstream accepts a 10- or 13-character ISBN prefix, with optional hyphens,
    // only if followed by a word boundary and no extra hyphen.
    let bytes = value.as_bytes();
    let prefix = if value.starts_with("978") || value.starts_with("979") {
        3
    } else {
        0
    };
    for start in [prefix, 0] {
        let mut index = start;
        if start > 0 && bytes.get(index) == Some(&b'-') {
            index += 1;
        }
        let mut valid = true;
        for _ in 0..9 {
            if !bytes.get(index).is_some_and(u8::is_ascii_digit) {
                valid = false;
                break;
            }
            index += 1;
            if bytes.get(index) == Some(&b'-') {
                index += 1;
            }
        }
        if valid
            && bytes
                .get(index)
                .is_some_and(|b| b.is_ascii_digit() || matches!(b, b'X' | b'x'))
        {
            index += 1;
            if !bytes
                .get(index)
                .is_some_and(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            {
                return &value[..index];
            }
        }
    }
    value
}
