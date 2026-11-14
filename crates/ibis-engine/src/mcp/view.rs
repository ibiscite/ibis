//! What an agent may read of a record. Everything here is library text, so it
//! is treated as untrusted: only citable fields, without markup or hidden
//! characters, cut short. Refs name a record in one short string an agent can
//! pass back: its citation key when that selects it alone, or else an id.
use super::MAX_LIST;
use crate::request::ItemSelector;
use ibis_core::citable::{citable_fields, is_variable};
use ibis_core::contract::{EngineError, ErrorCode, ItemRef, Record, Snapshot};
use ibis_core::model::{is_hidden, strip_markup};
use ibis_core::summary::RecordSummary;
use serde_json::{Map, Value};

/// Characters kept of any one text value.
pub const MAX_TEXT: usize = 1_000;
const ID_MARK: &str = "id:";
const CHECK_LENGTH: usize = 8;

/// A record's fields as formatting sees them.
pub(super) fn citable(record: &Record) -> Map<String, Value> {
    let mut csl = record.csl.to_map();
    citable_fields(&mut csl);
    csl
}

/// Fields [`summary`] already gives; `show` leaves them out of the rest.
const SUMMARIZED: [&str; 5] = ["type", "title", "author", "container-title", "DOI"];

/// What an agent needs to recognise and cite a record. Fields the record lacks are left out.
pub(super) fn summary(csl: &Map<String, Value>, reference: String) -> Map<String, Value> {
    let summary = RecordSummary::of(|name| csl.get(name));
    let text = |text: Option<&String>| text.map(|text| Value::String(clean(text)));
    let authors: Vec<Value> = summary
        .authors
        .iter()
        .take(MAX_LIST)
        .map(|author| Value::String(clean(&author.name)))
        .collect();
    let mut data = Map::new();
    data.insert("ref".into(), Value::String(reference));
    let found = [
        ("type", text(summary.item_type.as_ref())),
        ("title", text(summary.title.as_ref())),
        ("authors", (!authors.is_empty()).then_some(Value::Array(authors))),
        ("year", text(summary.year.as_ref())),
        ("container", text(summary.container.as_ref())),
        ("doi", csl.get("DOI").map(shorten)),
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

/// An opaque selector for a record identity, without exposing its lookup key.
pub fn reference(id: &ItemRef) -> String {
    id.opaque_reference()
}

/// A ref as search returns it: the record, then a short check of what it said then.
pub(super) fn checked(base: String, fingerprint: &str) -> String {
    format!("{base}#{}", &fingerprint[..fingerprint.len().min(CHECK_LENGTH)])
}

/// Reads a ref into the record it names and the check it carries, if any.
pub(super) fn parse<'a>(
    reference: &'a str,
    snapshot: &Snapshot,
    cancel: &crate::Cancellation,
) -> Result<(ItemSelector, Option<&'a str>), EngineError> {
    cancel.check()?;
    let (base, check) = match reference.rsplit_once('#') {
        Some((base, check)) if check.len() == CHECK_LENGTH && check.bytes().all(|byte| byte.is_ascii_hexdigit()) => {
            (base, Some(check))
        }
        _ => (reference, None),
    };
    let item = if base.starts_with(ID_MARK) {
        let id = snapshot
            .resolve_opaque(base)
            .cloned()
            .ok_or_else(|| EngineError::new(ErrorCode::NotFound))?;
        ItemSelector::Id(id)
    } else {
        ItemSelector::Key(base.to_owned())
    };
    Ok((item, check))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelled_opaque_resolution_returns_cancellation_instead_of_not_found() {
        let snapshot = Snapshot::new(vec![]).unwrap_or_else(|error| panic!("{error:?}"));
        let cancel = crate::Cancellation::default();
        cancel.cancel();
        assert_eq!(
            parse("id:absent", &snapshot, &cancel).err().map(|error| error.code),
            Some(ErrorCode::Cancelled)
        );
    }
}
