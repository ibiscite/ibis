//! Zotero items read from a copy of Zotero's database while Zotero is closed
//! (#14), turned into the items its local API would send, so that
//! [`super::load_items`] maps both access paths in one way.
//!
//! The CSL mapping is Zotero's own: Zotero keeps its global schema, with
//! the CSL type, field and creator mappings, in each database, so the
//! mapping always matches the Zotero that wrote the data. The conversion
//! follows Zotero's CSL export (`Zotero.Utilities.Item.itemToCSLJSON`,
//! AGPL-3.0, as Ibis is) for the cases it handles; [`load`] lists the
//! known differences. Reading the database itself is left to the caller,
//! so this module touches no files.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

use super::{Library, load_items};

mod particles;
use crate::model::LoadResult;

/// Item types whose `place` is the event's place, not the publisher's
/// (Zotero's temporary hard-coded mapping).
const EVENT_PLACE_TYPES: [&str; 3] = ["audioRecording", "presentation", "videoRecording"];

/// Zotero's mappings between its item model and CSL, with the item-type
/// details stored next to them in the database.
#[derive(Debug, Clone, Default)]
pub struct Schema {
    /// Zotero item type to CSL type.
    types: BTreeMap<String, String>,
    /// CSL text variable to the Zotero fields that fill it, first one first.
    text: Vec<(String, Vec<String>)>,
    /// CSL date variable to its Zotero field.
    dates: Vec<(String, String)>,
    /// Zotero creator type to CSL name variable.
    names: BTreeMap<String, String>,
    /// Zotero field to CSL variable, for field names written into Extra.
    reverse: BTreeMap<String, String>,
    /// Fields whose stored values are Zotero's two-part dates.
    date_fields: BTreeSet<String>,
    /// Item type and base field to the item type's own field
    /// (`publisher` is `label` for an audio recording).
    base: BTreeMap<(String, String), String>,
    /// Item type to its primary creator type, cited as author.
    primary: BTreeMap<String, String>,
}

impl Schema {
    /// Read the global schema Zotero stores in its `settings` table (already
    /// inflated to JSON), with the base-field mappings as
    /// `(item type, base field, field)` and each type's primary creator type.
    /// `None` if the schema has no CSL mapping.
    pub fn new(
        global_schema: &[u8],
        base_fields: impl IntoIterator<Item = (String, String, String)>,
        primary_creators: impl IntoIterator<Item = (String, String)>,
    ) -> Option<Schema> {
        let schema: Value = serde_json::from_slice(global_schema).ok()?;
        let csl = schema.get("csl")?;
        let strings = |value: &Value| -> Vec<String> {
            value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        };
        let mut result = Schema::default();
        for (csl_type, zotero_types) in csl.get("types")?.as_object()? {
            for zotero_type in strings(zotero_types) {
                result.types.insert(zotero_type, csl_type.clone());
            }
        }
        for (variable, fields) in csl.pointer("/fields/text")?.as_object()? {
            let fields = strings(fields);
            for field in &fields {
                result.reverse.insert(field.clone(), variable.clone());
            }
            result.text.push((variable.clone(), fields));
        }
        for (variable, field) in csl.pointer("/fields/date")?.as_object()? {
            let field = field.as_str()?.to_string();
            result.reverse.insert(field.clone(), variable.clone());
            result.dates.push((variable.clone(), field));
        }
        for (creator, variable) in csl.get("names")?.as_object()? {
            result.names.insert(creator.clone(), variable.as_str()?.to_string());
        }
        // Zotero stores fields of type date, and the fields mapped to them,
        // as "2020-03-00 March 2020". The access date is a plain timestamp.
        let date_bases: BTreeSet<String> = schema
            .pointer("/meta/fields")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .filter(|(_, meta)| meta.get("type").and_then(Value::as_str) == Some("date"))
            .map(|(field, _)| field.clone())
            .collect();
        for (item_type, base, field) in base_fields {
            if date_bases.contains(&base) {
                result.date_fields.insert(field.clone());
            }
            result.base.insert((item_type, base), field);
        }
        result.date_fields.extend(date_bases);
        result.primary.extend(primary_creators);
        Some(result)
    }

    fn field<'a>(&self, item: &'a Item, field: &str) -> Option<&'a str> {
        item.fields
            .get(field)
            .or_else(|| {
                self.base
                    .get(&(item.item_type.clone(), field.to_string()))
                    .and_then(|own| item.fields.get(own))
            })
            .map(String::as_str)
            .filter(|value| !value.is_empty())
    }
}

/// One regular item (not a note, attachment or annotation, not in the trash)
/// as stored in Zotero's database.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Item {
    pub key: String,
    pub version: u64,
    pub item_type: String,
    /// `YYYY-MM-DD hh:mm:ss`, in UTC, as stored.
    pub date_added: String,
    pub date_modified: String,
    /// Values by Zotero field name, as stored.
    pub fields: BTreeMap<String, String>,
    /// In Zotero's order.
    pub creators: Vec<Creator>,
    /// Tag names with Zotero's tag type (0 manual, 1 automatic).
    pub tags: Vec<(String, u8)>,
    /// The content type of the item's best attachment, if it has one.
    pub attachment_type: Option<String>,
    /// Better BibTeX's key from its own database, used after Zotero's
    /// citation key field and a key in Extra (ADR 0006).
    pub better_bibtex_key: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Creator {
    pub creator_type: String,
    pub first_name: String,
    pub last_name: String,
    /// One name only (an organization), kept in `last_name`.
    pub single_field: bool,
}

/// Turn items from one library into references, as [`load_items`] turns the
/// local API's answers.
///
/// Known differences from Zotero's export, all rare: EDTF dates and seasons
/// are read as Zotero's stored year, month and day; an access date keeps
/// its UTC day rather than the local one; and an item type without a CSL
/// type is reported instead of being left out of Zotero's export.
pub fn load(library: Library, schema: &Schema, items: &[Item]) -> LoadResult {
    let items: Vec<Value> = items.iter().map(|item| api_item(schema, item)).collect();
    load_items(library, &items)
}

/// The item as the local API returns it with `include=data,csljson`.
fn api_item(schema: &Schema, item: &Item) -> Value {
    let mut data = Map::new();
    data.insert("key".into(), item.key.clone().into());
    data.insert("version".into(), item.version.into());
    data.insert("itemType".into(), item.item_type.clone().into());
    for (field, value) in &item.fields {
        let shown = if schema.date_fields.contains(field) {
            multipart(value).1
        } else {
            value
        };
        data.insert(field.clone(), shown.into());
    }
    let creators: Vec<Value> = item
        .creators
        .iter()
        .map(|c| match c.single_field {
            true => json!({ "creatorType": c.creator_type, "name": c.last_name }),
            false => json!({ "creatorType": c.creator_type, "firstName": c.first_name, "lastName": c.last_name }),
        })
        .collect();
    data.insert("creators".into(), creators.into());
    let tags: Vec<Value> = item
        .tags
        .iter()
        .map(|(tag, kind)| match kind {
            0 => json!({ "tag": tag }),
            kind => json!({ "tag": tag, "type": kind }),
        })
        .collect();
    data.insert("tags".into(), tags.into());
    data.insert("dateAdded".into(), iso(&item.date_added).into());
    data.insert("dateModified".into(), iso(&item.date_modified).into());
    let mut out = json!({ "key": item.key, "version": item.version, "data": data });
    if let Some(csl) = csl_item(schema, item) {
        out["csljson"] = csl;
    }
    if let Some(kind) = &item.attachment_type {
        out["links"] = json!({ "attachment": { "attachmentType": kind } });
    }
    out
}

/// The item's CSL-JSON, as Zotero's CSL export writes it.
fn csl_item(schema: &Schema, item: &Item) -> Option<Value> {
    let mut csl = Map::new();
    csl.insert("id".into(), item.key.clone().into());
    csl.insert("type".into(), schema.types.get(&item.item_type)?.clone().into());
    let event_place = EVENT_PLACE_TYPES.contains(&item.item_type.as_str());
    let place = ["place".to_string()];
    for (variable, fields) in &schema.text {
        let fields: &[String] = match variable.as_str() {
            "shortTitle" => continue,
            "event-place" if event_place => &place,
            "publisher-place" if event_place => continue,
            _ => fields,
        };
        let Some((field, value)) = fields.iter().find_map(|f| Some((f, schema.field(item, f)?))) else {
            continue;
        };
        let value = match field.as_str() {
            "ISBN" => first_isbn(value).unwrap_or(value).to_string(),
            "extra" => extra_to_csl(schema, value),
            _ => value.to_string(),
        };
        csl.insert(variable.clone(), strip_quotes(&value).into());
    }
    let primary = schema.primary.get(&item.item_type);
    for creator in &item.creators {
        let variable = match schema.names.get(&creator.creator_type) {
            Some(variable) => variable.as_str(),
            None if primary == Some(&creator.creator_type) => "author",
            None => continue,
        };
        let name = if creator.single_field && !creator.last_name.is_empty() && creator.first_name.is_empty() {
            json!({ "literal": creator.last_name })
        } else if !creator.last_name.is_empty() || !creator.first_name.is_empty() {
            let mut name = Map::new();
            let family = &creator.last_name;
            name.insert("family".into(), family.clone().into());
            name.insert("given".into(), creator.first_name.clone().into());
            if !family.is_empty() && !creator.first_name.is_empty() {
                if family.chars().count() > 1 && family.starts_with('"') && family.ends_with('"') {
                    name.insert("family".into(), family[1..family.len() - 1].into());
                } else {
                    particles::parse(&mut name);
                }
            }
            Value::Object(name)
        } else {
            Value::Null
        };
        match csl.get_mut(variable).and_then(Value::as_array_mut) {
            Some(list) => list.push(name),
            None => {
                csl.insert(variable.to_string(), json!([name]));
            }
        }
    }
    if !csl.contains_key("citation-key")
        && let Some(key) = &item.better_bibtex_key
    {
        csl.insert("citation-key".into(), key.clone().into());
    }
    for (variable, field) in &schema.dates {
        if let Some(value) = schema.field(item, field) {
            csl.insert(variable.clone(), csl_date(value, field == "accessDate"));
        }
    }
    Some(Value::Object(csl))
}

/// Zotero's two-part date, "2020-03-00 March 2020": the SQL-style part and
/// what the user typed. A value without the SQL part is all text.
fn multipart(value: &str) -> (Option<&str>, &str) {
    let bytes = value.as_bytes();
    let sql = bytes.len() >= 11
        && bytes[10] == b' '
        && bytes[..10].iter().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                *b == b'-'
            } else {
                b.is_ascii_digit()
            }
        });
    if sql {
        (Some(&value[..10]), &value[11..])
    } else {
        (None, value)
    }
}

fn csl_date(value: &str, access: bool) -> Value {
    let (sql, shown) = if access {
        (
            value.get(..10).filter(|d| multipart(&format!("{d} ")).0.is_some()),
            value,
        )
    } else {
        multipart(value)
    };
    let shown = shown.trim();
    if !access && let Some(quoted) = quoted_date(shown) {
        return json!({ "literal": quoted });
    }
    let number = |part: &str| part.parse::<u32>().ok().filter(|n| *n > 0);
    let parts = sql.map(|sql| (number(&sql[..4]), number(&sql[5..7]), number(&sql[8..10])));
    match parts {
        Some((Some(year), month, day)) => {
            let mut parts = vec![year];
            if let Some(month) = month {
                parts.push(month);
                parts.extend(day);
            }
            json!({ "date-parts": [parts] })
        }
        _ => json!({ "literal": shown }),
    }
}

/// A date in straight or curly double quotes, cited as written.
fn quoted_date(text: &str) -> Option<&str> {
    let inner = text.strip_prefix(['"', '\u{201C}'])?.strip_suffix(['"', '\u{201D}'])?;
    (!inner.is_empty()).then_some(inner)
}

fn strip_quotes(value: &str) -> &str {
    match value.strip_prefix('"') {
        Some(rest) if !rest.is_empty() && rest.find('"') == Some(rest.len() - 1) => &rest[..rest.len() - 1],
        _ => value,
    }
}

fn iso(timestamp: &str) -> String {
    match multipart(timestamp).0 {
        Some(day) if timestamp.len() == 19 => format!("{day}T{}Z", &timestamp[11..]),
        _ => timestamp.to_string(),
    }
}

/// The first ISBN in the field, as `^(?:97[89]-?)?(?:\d-?){9}[\dx](?!-)\b`.
fn first_isbn(value: &str) -> Option<&str> {
    let b = value.as_bytes();
    let rest = |mut i: usize| -> Option<usize> {
        for _ in 0..9 {
            if !b.get(i)?.is_ascii_digit() {
                return None;
            }
            i += 1;
            if b.get(i) == Some(&b'-') {
                i += 1;
            }
        }
        let last = *b.get(i)?;
        if !(last.is_ascii_digit() || last.eq_ignore_ascii_case(&b'x')) {
            return None;
        }
        i += 1;
        let next = b.get(i).copied();
        let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
        (next != Some(b'-') && !next.is_some_and(word)).then_some(i)
    };
    let prefixed = (b.starts_with(b"978") || b.starts_with(b"979"))
        .then(|| rest(if b.get(3) == Some(&b'-') { 4 } else { 3 }))
        .flatten();
    prefixed.or_else(|| rest(0)).map(|end| &value[..end])
}

/// CSL variables Extra may name as they are (`original-date: 2001`).
const CSL_LABELS: [&str; 66] = [
    "abstract",
    "accessed",
    "annote",
    "archive",
    "archive-place",
    "author",
    "authority",
    "call-number",
    "chapter-number",
    "citation-label",
    "citation-number",
    "collection-editor",
    "collection-number",
    "collection-title",
    "composer",
    "container",
    "container-author",
    "container-title",
    "container-title-short",
    "dimensions",
    "director",
    "edition",
    "editor",
    "editorial-director",
    "event",
    "event-date",
    "event-place",
    "first-reference-note-number",
    "genre",
    "illustrator",
    "interviewer",
    "issue",
    "issued",
    "jurisdiction",
    "keyword",
    "language",
    "locator",
    "medium",
    "note",
    "number",
    "number-of-pages",
    "number-of-volumes",
    "original-author",
    "original-date",
    "original-publisher",
    "original-publisher-place",
    "original-title",
    "page",
    "page-first",
    "publisher",
    "publisher-place",
    "recipient",
    "references",
    "reviewed-author",
    "reviewed-title",
    "scale",
    "section",
    "source",
    "status",
    "submitted",
    "title",
    "title-short",
    "translator",
    "type",
    "version",
    "volume",
];

/// Extra with the labels of its `Label: value` lines written as CSL
/// variables, as `Zotero.Utilities.Item.extraToCSL` does: "Original Date"
/// becomes `original-date` and "Publication Title" `container-title`.
fn extra_to_csl(schema: &Schema, extra: &str) -> String {
    let label_char = |c: char| c.is_ascii_alphabetic() || c == ' ' || c == '-';
    let mut out = String::with_capacity(extra.len());
    let mut at = 0;
    while at < extra.len() {
        let line = &extra[at..];
        let label_end = line.find(|c: char| !label_char(c)).unwrap_or(line.len());
        let value_end = (label_end > 0 && line[label_end..].starts_with(':'))
            .then(|| {
                // `(:\s*.+)` may run over line breaks before its first character.
                let after = &line[label_end + 1..];
                let text = after.trim_start();
                let start = label_end + 1 + after.len() - text.len();
                match text.find('\n') {
                    Some(0) => None,
                    Some(end) => Some(start + end),
                    None if !text.is_empty() => Some(line.len()),
                    None => after
                        .ends_with(|c: char| c.is_whitespace() && c != '\n')
                        .then_some(line.len()),
                }
            })
            .flatten();
        let end = match value_end {
            Some(end) => {
                out.push_str(&csl_label(schema, &line[..label_end]));
                out.push_str(&line[label_end..end]);
                end
            }
            None => {
                let end = line.find('\n').unwrap_or(line.len());
                out.push_str(&line[..end]);
                end
            }
        };
        at += end;
        if extra[at..].starts_with('\n') {
            out.push('\n');
            at += 1;
        }
    }
    out
}

fn csl_label(schema: &Schema, label: &str) -> String {
    let field = label.to_lowercase().replace(' ', "-");
    match field.as_str() {
        "doi" | "isbn" | "issn" | "pmcid" | "pmid" | "url" => return field.to_uppercase(),
        "archive-location" => return "archive_location".into(),
        f if CSL_LABELS.contains(&f) => return field,
        _ => {}
    }
    // A Zotero field written out: "Publication Title" is `publicationTitle`.
    let mut zotero = label.to_string();
    if let Some(space) = label
        .match_indices(' ')
        .map(|(i, _)| i)
        .find(|&i| label[i + 1..].starts_with(|c: char| c.is_ascii_uppercase()))
    {
        zotero.remove(space);
    }
    let mut chars = zotero.chars();
    if let (Some(first), Some(second)) = (chars.next(), chars.next())
        && second.to_lowercase().eq(std::iter::once(second))
    {
        zotero = first.to_lowercase().chain(zotero.chars().skip(1)).collect();
    }
    schema
        .reverse
        .get(&zotero)
        .cloned()
        .unwrap_or_else(|| label.to_string())
}

#[cfg(test)]
mod tests;
