//! Zotero rules carried over from the earlier Web API mapper: refreshes never
//! publish partial libraries, attachment details stay private, malformed items
//! cannot change their neighbours, and diagnostics never copy field labels.
//! Invented API responses only; no Zotero process or library is used.
#![cfg(test)]

use std::collections::BTreeMap;

use ibis_core::contract::{LibraryId, LibraryObject, Record};
use ibis_sources::zotero::{self, ZoteroItem};
use ibis_sources::zotero_local::api::{Response, Transport, TransportError};
use ibis_sources::zotero_source::{LiveError, ZoteroError, ZoteroSource};
use ibis_sources::{LoadedLibrary, WarningCode};
use serde_json::{Value, json};

// --- an invented Zotero 10 local API ------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    None,
    /// The inventory declares this many results, whatever it lists.
    InventoryTotal(usize),
    /// The first item request returns only this many of its rows.
    ShortFirstBatch(usize),
}

struct Api {
    rows: Vec<Value>,
    fault: Fault,
}

impl Api {
    fn new(rows: Vec<Value>) -> Self {
        Self {
            rows,
            fault: Fault::None,
        }
    }
}

fn response(body: &Value, total: usize) -> Response {
    Response {
        status: 200,
        headers: vec![
            ("Zotero-API-Version".into(), "3".into()),
            ("X-Zotero-Version".into(), "10.0".into()),
            ("Zotero-Server-ID".into(), "synthetic-server".into()),
            ("Last-Modified-Version".into(), "1".into()),
            ("Total-Results".into(), total.to_string()),
        ],
        body: serde_json::to_vec(body).unwrap(),
    }
}

impl Transport for Api {
    fn get(&mut self, path: &str) -> Result<Response, TransportError> {
        if path.ends_with("format=versions") {
            let versions: BTreeMap<_, _> = self
                .rows
                .iter()
                .filter_map(|row| Some((row.get("key")?.as_str()?.to_owned(), json!(1))))
                .collect();
            let total = match self.fault {
                Fault::InventoryTotal(total) => total,
                _ => versions.len(),
            };
            return Ok(response(&json!(versions), total));
        }
        let wanted: Vec<&str> = path
            .split(['?', '&'])
            .find_map(|part| part.strip_prefix("itemKey="))
            .unwrap_or_default()
            .split(',')
            .collect();
        let mut rows: Vec<Value> = self
            .rows
            .iter()
            .filter(|row| {
                row.get("key")
                    .and_then(Value::as_str)
                    .is_some_and(|key| wanted.contains(&key))
            })
            .cloned()
            .collect();
        let mut total = rows.len();
        match self.fault {
            Fault::ShortFirstBatch(kept) if wanted.contains(&key(0).as_str()) => {
                rows.truncate(kept);
                total = rows.len();
            }
            _ => {}
        }
        Ok(response(&json!(rows), total))
    }
}

fn key(number: usize) -> String {
    format!("SYN{number:05}")
}

/// One Web API row with the identity fields the live reader requires.
fn row(key: &str, data: Value) -> Value {
    let mut full = json!({"key": key, "version": 1, "dateModified": "2026-10-26T12:00:00Z"});
    full.as_object_mut()
        .unwrap()
        .extend(data.as_object().cloned().unwrap_or_default());
    json!({"key": key, "version": 1, "data": full})
}

fn book(number: usize) -> Value {
    row(
        &key(number),
        json!({"itemType": "book", "title": "Synthetic pagination record"}),
    )
}

fn refresh(api: &mut Api) -> Result<std::sync::Arc<LoadedLibrary>, ZoteroError> {
    // The default choice is the personal library alone.
    ZoteroSource::default()
        .refresh(api, false, &|| false)
        .map(|mut libraries| libraries.remove(0))
}

fn records(loaded: &LoadedLibrary) -> Vec<&Record> {
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

fn find<'a>(loaded: &'a LoadedLibrary, key: &str) -> &'a Record {
    records(loaded)
        .into_iter()
        .find(|record| record.id.key == key)
        .unwrap_or_else(|| panic!("`{key}` not loaded; warnings: {:?}", loaded.warnings))
}

/// Fixture items, rewritten as rows of the data-only request the live reader
/// makes: the CSL title moves into the item's own `title` field.
fn fixture_rows(file: &str) -> Vec<Value> {
    let path = format!("{}/../../fixtures/zotero/{file}", env!("CARGO_MANIFEST_DIR"));
    let items: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    items
        .into_iter()
        .map(|item| {
            let key = item["key"].as_str().unwrap().to_owned();
            let mut data = item["data"].clone();
            if let Some(title) = item["csljson"]["title"].as_str() {
                data["title"] = json!(title);
            }
            let mut row = row(&key, data);
            row["links"] = item["links"].clone();
            row
        })
        .collect()
}

// --- pagination and declared counts (#263) -------------------------------------------

#[test]
fn a_changed_total_at_one_version_cannot_publish_a_partial_snapshot() {
    let mut api = Api::new((0..51).map(book).collect());
    api.fault = Fault::InventoryTotal(200);
    let result = refresh(&mut api);
    assert!(
        result.is_err(),
        "inconsistent totals must not publish {} synthetic records",
        result.as_ref().map_or(0, |loaded| records(loaded).len())
    );
}

#[test]
fn a_short_page_never_publishes_a_snapshot_with_missing_items() {
    let mut api = Api::new((0..51).map(book).collect());
    api.fault = Fault::ShortFirstBatch(20);
    assert!(refresh(&mut api).is_err());
}

#[test]
fn consistent_pages_publish_every_item() {
    let loaded = refresh(&mut Api::new((0..51).map(book).collect())).unwrap();
    assert_eq!(records(&loaded).len(), 51);
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
}

// --- attachments (#573) ------------------------------------------------------------------

#[test]
fn parents_load_and_child_attachments_are_not_records() {
    let loaded = refresh(&mut Api::new(fixture_rows("attachments.json"))).unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    let keys: Vec<_> = records(&loaded).iter().map(|record| record.id.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "SYN00001", "SYN00002", "SYN00003", "SYN00004", "SYN00005", "SYN00006", "SYN00007"
        ],
        "the child attachment item is still not a reference"
    );
    assert_eq!(find(&loaded, "SYN00001").csl["title"], "Synthetic article with a PDF");
}

#[test]
fn hostile_attachment_links_keep_the_reference() {
    let loaded = refresh(&mut Api::new(fixture_rows("attachments-hostile.json"))).unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert_eq!(records(&loaded).len(), 6, "one bad link never drops an item");
    for key in ["SYN00101", "SYN00102", "SYN00103", "SYN00104", "SYN00105", "SYN00106"] {
        assert!(find(&loaded, key).csl.contains_key("title"), "{key}");
    }
}

#[test]
fn attachment_details_never_reach_the_public_snapshot() {
    let rows = fixture_rows("attachments-hostile.json");
    assert!(
        rows.iter().any(|row| row["links"]["attachment"]["href"].is_string()),
        "the fixture carries sentinels"
    );
    let loaded = refresh(&mut Api::new(rows)).unwrap();
    let text = serde_json::to_string(&loaded.snapshot).unwrap();
    for sentinel in [
        "SENTINEL",
        "sentinel-user",
        "999999999999",
        "localhost:23119",
        "SYN00112",
    ] {
        assert!(!text.contains(sentinel), "`{sentinel}` leaked into {text}");
    }
}

// --- field warnings and labels (#262, #342) ------------------------------------------------

#[test]
fn arbitrary_field_labels_never_enter_diagnostics_or_public_data() {
    let label = format!("SYNTHETIC_PRIVATE_LABEL\n`日本🌙{}", "x".repeat(4096));
    for value in ["Synthetic value", "{\"date-parts\": [[2021, 2, 31]]}", "unreadable"] {
        let mut item = json!({"key": "SYN00001", "itemType": "book", "title": "Synthetic title"});
        item[&label] = json!(value);
        let item: ZoteroItem = serde_json::from_value(item).unwrap();
        let mapped = zotero::map_item(LibraryId::digest(b"invented"), &item).unwrap();
        assert!(!mapped.warnings.is_empty());
        assert!(!format!("{:?}", mapped.warnings).contains("SYNTHETIC_PRIVATE_LABEL"));
        assert!(
            !serde_json::to_string(&mapped.record)
                .unwrap()
                .contains("SYNTHETIC_PRIVATE_LABEL")
        );

        let mut data = json!({"itemType": "book", "title": "Synthetic title"});
        data[&label] = json!(value);
        let loaded = refresh(&mut Api::new(vec![row("SYN00001", data)])).unwrap();
        assert_eq!(records(&loaded).len(), 1);
        assert!(!loaded.warnings.is_empty());
        for warning in &loaded.warnings {
            assert_eq!(warning.code, WarningCode::Field);
            assert!(!format!("{warning:?}").contains("SYNTHETIC_PRIVATE_LABEL"));
        }
        assert!(
            !serde_json::to_string(&loaded.snapshot)
                .unwrap()
                .contains("SYNTHETIC_PRIVATE_LABEL")
        );
    }
}

#[test]
fn compound_creator_names_are_refused_at_neutral_and_live_boundaries() {
    let data = json!({
        "itemType": "book", "title": "Synthetic title",
        "creators": [{"creatorType": "author", "lastName": ["Synthetic"], "firstName": "Visible"}]
    });
    let mut neutral = data.clone();
    neutral["key"] = json!("SYN00001");
    assert!(serde_json::from_value::<ZoteroItem>(neutral).is_err());
    assert_eq!(
        refresh(&mut Api::new(vec![row("SYN00001", data)])).err(),
        Some(ZoteroError::Live(LiveError::Incomplete))
    );
}

// --- canonical normalization -------------------------------------------------------

#[test]
fn mapping_keeps_canonical_fields_without_a_duplicate_source_item() {
    let item: ZoteroItem = serde_json::from_value(json!({
        "key": "SYN00001", "itemType": "book", "title": "  Été <I>日本🌙</I> & < 0.05  ",
        "series": "  <b >Synthetic</b>  ", "abstractNote": "  Synthetic abstract  ",
        "extra": "  Synthetic extra\n"
    }))
    .unwrap();
    let mapped = zotero::map_item(LibraryId::digest(b"invented"), &item).unwrap();
    assert!(mapped.warnings.is_empty(), "{:?}", mapped.warnings);
    assert_eq!(mapped.record.id.key, "SYN00001");
    assert_eq!(
        serde_json::to_value(&mapped.record.csl).unwrap(),
        json!({
            "type": "book", "title": "Été <i>日本🌙</i> &amp; &lt; 0.05",
            "collection-title": "  <b >Synthetic</b>  ",
            "abstract": "  Synthetic abstract  ", "note": "  Synthetic extra\n"
        })
    );
    assert!(serde_json::to_value(&mapped.record).unwrap().get("raw").is_none());
}

// --- malformed payloads -----------------------------------------------------------------------

#[test]
fn malformed_neutral_payload_roots_are_refused() {
    for value in [json!(null), json!([]), json!({}), json!(false), json!(0), json!("bad")] {
        assert!(serde_json::from_value::<ZoteroItem>(value).is_err());
    }
}

#[test]
fn duplicate_api_rows_cannot_publish_an_ambiguous_identity() {
    assert_eq!(
        refresh(&mut Api::new(vec![book(0), book(0)])).err(),
        Some(ZoteroError::Live(LiveError::Changing))
    );
}
