//! Convert engine data to identity-free presentation before rendering.
use super::record::{Key, Row};
use ibis_core::{model::strip_markup, summary::RecordSummary};
use serde_json::Value;
pub(super) fn row(record: &Value, partial: bool, typo: bool) -> Option<Row> {
    let csl = record.get("csl")?;
    let keys = record.get("citation_keys")?.as_array()?;
    let first = keys.first().and_then(|key| key.get("value")).and_then(Value::as_str);
    let key = match first {
        None => Key::Missing,
        Some(key) if key.is_empty() || key.chars().any(|c| c.is_whitespace() || c.is_control()) => Key::Invalid,
        Some(key)
            if keys
                .iter()
                .any(|other| other.get("value").and_then(Value::as_str) != Some(key)) =>
        {
            Key::Conflicting
        }
        Some(key) => Key::Present(key.into()),
    };
    let text = |name| csl.get(name).and_then(Value::as_str).map(strip_markup);
    let number = |name| {
        csl.get(name)
            .map(|value| value.as_str().map_or_else(|| value.to_string(), str::to_owned))
    };
    Some(Row {
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
    })
}
pub(super) fn hit(hit: &Value) -> Option<Row> {
    row(
        hit.pointer("/item/object/data")?,
        hit.get("partial").and_then(Value::as_bool).unwrap_or(false),
        hit.get("typo").and_then(Value::as_bool).unwrap_or(false),
    )
}
