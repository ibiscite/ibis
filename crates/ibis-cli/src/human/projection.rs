//! Convert engine data to identity-free presentation before rendering.
use super::record::{Key, Row};
use ibis_core::contract::{LibraryObject, Record};
use ibis_core::{model::strip_markup, summary::RecordSummary};
use serde_json::Value;
pub(super) fn row(record: &Record, partial: bool, typo: bool) -> Row {
    let csl = &record.csl;
    let keys = &record.citation_keys;
    let first = keys.first().map(|key| key.value.as_str());
    let key = match first {
        None => Key::Missing,
        Some(key) if key.is_empty() || key.chars().any(|c| c.is_whitespace() || c.is_control()) => Key::Invalid,
        Some(key) if keys.iter().any(|other| other.value != key) => Key::Conflicting,
        Some(key) => Key::Present(key.into()),
    };
    let text = |name| csl.get(name).and_then(Value::as_str).map(strip_markup);
    let number = |name| {
        csl.get(name)
            .map(|value| value.as_str().map_or_else(|| value.to_string(), str::to_owned))
    };
    Row {
        summary: RecordSummary::of(|name| csl.get(name)),
        key,
        partial,
        typo,
        details: vec![
            ("Published in", text("container-title")),
            ("Publisher", text("publisher")),
            ("Volume", number("volume")),
            ("Issue", number("issue")),
            ("Pages", number("page")),
            ("DOI", text("DOI")),
            ("URL", text("URL")),
        ],
    }
}
pub(super) fn hit(hit: &ibis_engine::SearchHit) -> Option<Row> {
    match &hit.item.object {
        LibraryObject::Record(record) => Some(row(record, hit.partial, hit.typo)),
        _ => None,
    }
}
