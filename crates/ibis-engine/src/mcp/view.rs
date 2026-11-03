//! What an agent may read of a record. Everything here is library text, so it
//! is treated as untrusted: only citable fields, without markup or hidden
//! characters, cut short. Refs name a record in one short string an agent can
//! pass back: its citation key when that selects it alone, or else an id.
use super::MAX_LIST;
use crate::request::ItemSelector;
use ibis_core::citable::{citable_fields, is_variable};
use ibis_core::contract::{ItemRef, LibraryId, Record};
use ibis_core::model::{Name, is_hidden, strip_markup};
use serde_json::{Map, Value};
use std::fmt::Write;

/// Characters kept of any one text value.
pub const MAX_TEXT: usize = 1_000;
const ID_MARK: &str = "id:";

/// A record's fields as formatting sees them.
pub(super) fn citable(record: &Record) -> Map<String, Value> {
    let mut csl = record
        .csl
        .iter()
        .map(|(field, value)| (field.clone(), value.clone()))
        .collect();
    citable_fields(&mut csl);
    csl
}

/// Fields [`summary`] already gives; `show` leaves them out of the rest.
const SUMMARIZED: [&str; 5] = ["type", "title", "author", "container-title", "DOI"];

/// What an agent needs to recognise and cite a record. Fields the record lacks are left out.
pub(super) fn summary(csl: &Map<String, Value>, reference: String) -> Map<String, Value> {
    let field = |name: &str| csl.get(name).map(shorten);
    let authors: Vec<Value> = csl
        .get("author")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(MAX_LIST)
        .map(|name| Value::String(clean(&Name::from_csl(name).display())))
        .collect();
    let issued = csl.get("issued");
    let year = issued
        .and_then(|issued| issued.pointer("/date-parts/0/0"))
        .or_else(|| issued.and_then(|issued| issued.get("raw")))
        .or_else(|| issued.and_then(|issued| issued.get("literal")))
        .map(shorten);
    let mut data = Map::new();
    data.insert("ref".into(), Value::String(reference));
    let found = [
        ("type", field("type")),
        ("title", field("title")),
        ("authors", (!authors.is_empty()).then_some(Value::Array(authors))),
        ("year", year),
        ("container", field("container-title")),
        ("doi", field("DOI")),
    ];
    for (name, value) in found {
        if let Some(value) = value.filter(|value| !value.is_null()) {
            data.insert(name.into(), value);
        }
    }
    data
}

/// The citable fields [`summary`] does not already give.
pub(super) fn details(csl: &Map<String, Value>) -> Value {
    Value::Object(
        csl.iter()
            .filter(|(name, _)| is_variable(name) && !SUMMARIZED.contains(&name.as_str()))
            .map(|(name, value)| (name.clone(), shorten(value)))
            .collect(),
    )
}

fn shorten(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(clean(text)),
        Value::Array(values) => Value::Array(values.iter().take(MAX_LIST).map(shorten).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), shorten(value)))
                .collect::<Map<_, _>>(),
        ),
        other => other.clone(),
    }
}

/// One value's readable text on one line: no markup, nothing hidden, cut short.
pub(super) fn clean(text: &str) -> String {
    let plain = strip_markup(text);
    let visible = scrub(&plain);
    let line = visible.split_whitespace().collect::<Vec<_>>().join(" ");
    match line.char_indices().nth(MAX_TEXT) {
        Some((end, _)) => format!("{}…", &line[..end]),
        None => line,
    }
}

/// Text without the characters that hide words or reorder what a person sees.
pub(super) fn scrub(text: &str) -> String {
    text.chars().filter(|character| !is_hidden(*character)).collect()
}

/// The id form of a ref: a marker no citation key is expected to start with, the
/// library, then the record's key, escaped so nothing from the library arrives unread.
pub fn reference(id: &ItemRef) -> String {
    let mut text = format!("{ID_MARK}{}/", id.library.as_str());
    for byte in id.key.bytes() {
        if byte.is_ascii_alphanumeric() || b"._:-".contains(&byte) {
            text.push(char::from(byte));
        } else {
            // Writing to a String cannot fail.
            let _ = write!(text, "%{byte:02X}");
        }
    }
    text
}

/// Reads a ref. Text that is not an id is a citation key.
pub(super) fn selector(reference: &str) -> ItemSelector {
    reference
        .strip_prefix(ID_MARK)
        .and_then(|id| id.split_once('/'))
        .and_then(|(library, key)| {
            Some(ItemRef {
                library: LibraryId::new(library).ok()?,
                key: unescaped(key)?,
            })
        })
        .map_or_else(|| ItemSelector::Key(reference.to_owned()), ItemSelector::Id)
}

fn unescaped(text: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut rest = text.bytes();
    while let Some(byte) = rest.next() {
        if byte == b'%' {
            let pair = [rest.next()?, rest.next()?];
            bytes.push(u8::from_str_radix(std::str::from_utf8(&pair).ok()?, 16).ok()?);
        } else {
            bytes.push(byte);
        }
    }
    String::from_utf8(bytes).ok()
}
