//! Shared helpers for BibTeX tests: load synthetic text through the public
//! file loader and read the CSL meaning of the records it publishes.
#![allow(dead_code)]

use std::collections::BTreeMap;

use ibis_core::contract::{LibraryObject, Record};
use ibis_sources::{LoadedLibrary, SourceWarning, WarningCode};
use serde_json::Value;

/// Load BibTeX text from a temporary `.bib` file with the tolerant initial read.
pub fn load(text: &str) -> LoadedLibrary {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let path = dir.path().join("synthetic.bib");
    std::fs::write(&path, text).unwrap_or_else(|e| panic!("{e}"));
    ibis_sources::load_file(&path).unwrap_or_else(|e| panic!("{e}"))
}

/// Records in snapshot order (sorted by identity, not by source position).
pub fn records(loaded: &LoadedLibrary) -> Vec<&Record> {
    loaded
        .snapshot
        .objects()
        .iter()
        .filter_map(|item| match &item.object {
            LibraryObject::Record(record) => Some(record),
            _ => None,
        })
        .collect()
}

/// The BibTeX keys of every record, sorted.
pub fn keys(loaded: &LoadedLibrary) -> Vec<String> {
    let mut keys: Vec<_> = records(loaded).iter().map(|record| key_of(record).to_owned()).collect();
    keys.sort();
    keys
}

pub fn key_of(record: &Record) -> &str {
    record
        .citation_keys
        .first()
        .map(|key| key.value.as_str())
        .unwrap_or_default()
}

pub fn find<'a>(loaded: &'a LoadedLibrary, key: &str) -> Option<&'a Record> {
    records(loaded).into_iter().find(|record| key_of(record) == key)
}

pub fn record<'a>(loaded: &'a LoadedLibrary, key: &str) -> &'a Record {
    find(loaded, key).unwrap_or_else(|| panic!("`{key}` not loaded; warnings: {:?}", loaded.warnings))
}

pub fn csl<'a>(loaded: &'a LoadedLibrary, key: &str) -> &'a BTreeMap<String, Value> {
    &record(loaded, key).csl
}

/// A CSL string variable, with its rich-text markup.
pub fn text<'a>(loaded: &'a LoadedLibrary, key: &str, field: &str) -> Option<&'a str> {
    csl(loaded, key).get(field).and_then(Value::as_str)
}

/// A CSL string variable without markup, as search and display read it.
pub fn plain(loaded: &LoadedLibrary, key: &str, field: &str) -> Option<String> {
    text(loaded, key, field).map(strip_markup)
}

/// First date part of a CSL date variable: (year, month, day).
pub fn date(loaded: &LoadedLibrary, key: &str, field: &str) -> Option<(i64, Option<i64>, Option<i64>)> {
    let start = csl(loaded, key).get(field)?.get("date-parts")?.get(0)?;
    Some((
        start.get(0)?.as_i64()?,
        start.get(1).and_then(Value::as_i64),
        start.get(2).and_then(Value::as_i64),
    ))
}

pub fn year(loaded: &LoadedLibrary, key: &str) -> Option<i64> {
    date(loaded, key, "issued").map(|(year, _, _)| year)
}

pub fn month(loaded: &LoadedLibrary, key: &str) -> Option<i64> {
    date(loaded, key, "issued").and_then(|(_, month, _)| month)
}

pub fn date_parts(loaded: &LoadedLibrary, key: &str, field: &str) -> Value {
    csl(loaded, key)
        .get(field)
        .and_then(|date| date.get("date-parts"))
        .cloned()
        .unwrap_or(Value::Null)
}

pub fn with_code(loaded: &LoadedLibrary, code: WarningCode) -> Vec<&SourceWarning> {
    loaded.warnings.iter().filter(|warning| warning.code == code).collect()
}

pub fn all_field(loaded: &LoadedLibrary) -> bool {
    loaded.warnings.iter().all(|warning| warning.code == WarningCode::Field)
}

pub fn warned_at(loaded: &LoadedLibrary, code: WarningCode, line: usize) -> usize {
    loaded
        .warnings
        .iter()
        .filter(|warning| warning.code == code && warning.line == Some(line))
        .count()
}

/// Remove CSL rich-text tags and decode the entities used to escape text.
pub fn strip_markup(rich: &str) -> String {
    let mut out = String::with_capacity(rich.len());
    let mut in_tag = false;
    for c in rich.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

/// The reading-order display that snapshot search builds from a CSL name.
pub fn display(name: &Value) -> String {
    let field = |key: &str| name.get(key).and_then(Value::as_str).map(str::to_owned);
    ibis_core::model::Name {
        family: field("family"),
        given: field("given"),
        literal: field("literal"),
        dropping_particle: field("dropping-particle"),
        non_dropping_particle: field("non-dropping-particle"),
        suffix: field("suffix"),
    }
    .display()
}
