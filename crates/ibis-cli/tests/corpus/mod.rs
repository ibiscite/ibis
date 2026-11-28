//! The invented items of the Zotero mapping corpus (fixtures/zotero/mapping),
//! read by Ibis two ways: as Zotero items, and as a CSL-JSON file exported
//! from Zotero through the CLI's own loader (Ibis with no Zotero at all).
// Each test that includes this module uses only part of it.
#![allow(dead_code)]
use ibis_core::contract::{LibraryObject, Snapshot};
use ibis_engine::Engine;
use ibis_sources::zotero::{ZoteroItem, map_item};
use ibis_sources::zotero_source::{Library, ZoteroSource};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Cursor, path::Path, sync::Arc};

pub type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// The mapping corpus's cases.
pub fn cases() -> TestResult<Vec<Value>> {
    let cases: Value = serde_json::from_str(include_str!("../../../../fixtures/zotero/mapping/cases.json"))?;
    Ok(cases.as_array().ok_or("mapping cases")?.clone())
}

/// The engine and each case's item, read one way.
pub struct Reader {
    pub name: &'static str,
    pub engine: Engine,
    /// Each case's citation item, by case name.
    pub items: BTreeMap<String, Value>,
}

pub fn zotero_reader(cases: &[Value]) -> TestResult<Reader> {
    let library = ZoteroSource::default().id_of(Library::User).ok_or("library id")?;
    let mut objects = vec![];
    let mut items = BTreeMap::new();
    for case in cases {
        let item: ZoteroItem = serde_json::from_value(case["item"].clone())?;
        let mapped = map_item(library.clone(), &item).map_err(|error| format!("{error:?}"))?;
        items.insert(
            case["name"].as_str().ok_or("name")?.to_owned(),
            json!({"item": mapped.record.id}),
        );
        objects.push(LibraryObject::Record(mapped.record));
    }
    let engine = Engine::from_snapshot(Arc::new(Snapshot::new(objects)?));
    Ok(Reader {
        name: "zotero",
        engine,
        items,
    })
}

/// The cases that cover every item type once: what a CSL-JSON file needs.
pub fn every_type(cases: &[Value]) -> Vec<Value> {
    cases
        .iter()
        .filter(|case| {
            case["name"]
                .as_str()
                .is_some_and(|name| name.starts_with("type-") || name.starts_with("formatted-"))
        })
        .cloned()
        .collect()
}

/// Each Zotero item with its own citation key, as Zotero's field or Better
/// BibTeX gives one. Several invented items share a key, which Ibis rightly
/// refuses to guess between.
pub fn keyed(cases: &[Value]) -> Vec<Value> {
    let mut cases = cases.to_vec();
    for (index, case) in cases.iter_mut().enumerate() {
        case["item"]["citationKey"] = json!(format!("item{index}"));
    }
    cases
}

/// The cases as a CSL-JSON file exported from Zotero, through the CLI's own
/// file loader: Ibis with no Zotero at all.
pub fn csl_json_reader(cases: &[Value], dir: &Path) -> TestResult<Reader> {
    let records: Vec<Value> = cases
        .iter()
        .enumerate()
        .map(|(index, case)| {
            // Each record gets its own key: several invented items share one,
            // which a file reader rightly refuses to guess between.
            let mut record = case["expected"].clone();
            record["id"] = json!(format!("item{index}"));
            record["citation-key"] = json!(format!("item{index}"));
            record
        })
        .collect();
    let path = dir.join("exported.json");
    std::fs::write(&path, serde_json::to_vec(&records)?)?;
    let engine = Engine::new().map_err(|error| format!("{error:?}"))?;
    let (mut output, mut errors) = (vec![], vec![]);
    let code = ibis::run_with(
        engine.clone(),
        ["ibis", "libraries", "--file", path.to_str().ok_or("path")?, "--json"]
            .map(Into::into)
            .into(),
        Cursor::new(vec![]),
        &mut output,
        &mut errors,
        ibis::OutputMode::Plain,
    );
    // Exit 1 reports, as findings, the Zotero fields CSL-JSON readers do not use.
    assert!(code <= 1, "{}", String::from_utf8_lossy(&errors));
    let envelope: Value = serde_json::from_slice(&output)?;
    assert_eq!(
        envelope["data"]["libraries"][0]["objects"],
        records.len(),
        "every record read"
    );
    let snapshot = engine.snapshot().map_err(|error| format!("{error:?}"))?;
    let mut by_key = BTreeMap::new();
    for object in snapshot.objects() {
        if let LibraryObject::Record(record) = &object.object {
            let key = record.citation_key()?.ok_or("record without key")?.to_owned();
            by_key.insert(key, json!({"item": record.id}));
        }
    }
    let mut items = BTreeMap::new();
    for (index, case) in cases.iter().enumerate() {
        let item = by_key.remove(&format!("item{index}")).ok_or("CSL-JSON lost a record")?;
        items.insert(case["name"].as_str().ok_or("name")?.to_owned(), item);
    }
    Ok(Reader {
        name: "csl-json",
        engine,
        items,
    })
}
