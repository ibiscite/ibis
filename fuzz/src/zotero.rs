//! Zotero oracles: its local API on 127.0.0.1, which any local program could
//! answer instead of Zotero, and Better BibTeX's exported key list.
//!
//! `zotero_live` input is the item list Zotero would send, optionally after
//! the version inventory and a zero byte. Without an inventory, one is made
//! from the items so the read reaches them. Every item the reader keeps is
//! mapped as Ibis maps it; a mapped record must be one Ibis can publish.

use std::collections::BTreeMap;
use std::time::Instant;

use ibis_core::contract::LibraryId;
use ibis_sources::zotero::{ZoteroCreator, ZoteroItem, map_item};
use ibis_sources::zotero_local::api::{Response, Transport, TransportError};
use ibis_sources::zotero_local::bbt::read_only_keys;
use ibis_sources::zotero_local::live::{Library, LiveReader};
use serde_json::Value;

use crate::{MAX_INPUT_BYTES, assert_in_time, assert_no_contained_panic, assert_wire_round_trip, watch_panics};

/// Answers like Zotero 10, with the fuzzed inventory and item list.
struct Invented {
    versions: Vec<u8>,
    items: Vec<u8>,
    count: usize,
    version: u64,
    requests: usize,
}

impl Transport for Invented {
    fn get(&mut self, path: &str) -> Result<Response, TransportError> {
        self.requests += 1;
        if self.requests > 16 {
            return Err(TransportError::Unreachable);
        }
        let body = if path.ends_with("format=versions") {
            self.versions.clone()
        } else {
            self.items.clone()
        };
        Ok(Response {
            status: 200,
            headers: vec![
                ("Zotero-API-Version".into(), "3".into()),
                ("X-Zotero-Version".into(), "10.0".into()),
                ("Zotero-Server-ID".into(), "invented".into()),
                ("Last-Modified-Version".into(), self.version.to_string()),
                ("Total-Results".into(), self.count.to_string()),
            ],
            body,
        })
    }
}

/// The inventory Zotero would send for these items, when they parse.
fn inventory(items: &[u8]) -> Vec<u8> {
    let rows: Vec<Value> = serde_json::from_slice(items).unwrap_or_default();
    let versions: BTreeMap<String, Value> = rows
        .iter()
        .filter_map(|row| Some((row.get("key")?.as_str()?.to_owned(), row.get("version")?.clone())))
        .collect();
    serde_json::to_vec(&versions).unwrap_or_default()
}

pub fn check_zotero_live(data: &[u8]) {
    if data.len() > MAX_INPUT_BYTES {
        return;
    }
    watch_panics();
    let started = Instant::now();
    let (versions, items) = match data.iter().position(|byte| *byte == 0) {
        Some(split) => (data[..split].to_vec(), data[split + 1..].to_vec()),
        None => (inventory(data), data.to_vec()),
    };
    let parsed: BTreeMap<String, Value> = serde_json::from_slice(&versions).unwrap_or_default();
    let mut transport = Invented {
        count: parsed.len(),
        version: parsed.values().filter_map(Value::as_u64).max().unwrap_or(1),
        versions,
        items,
        requests: 0,
    };
    let mut reader = LiveReader::new(Library::User);
    let result = reader.refresh(&mut transport, false, &|| false);
    assert_no_contained_panic();
    if result.is_ok() {
        let kept = reader.items();
        assert!(kept.len() <= parsed.len(), "more items than the inventory names");
        let library = LibraryId::digest(b"invented zotero library");
        for raw in kept {
            let item = ZoteroItem {
                key: raw.key.clone(),
                item_type: raw.item_type.clone(),
                fields: raw.fields.clone(),
                creators: raw
                    .creators
                    .iter()
                    .map(|creator| ZoteroCreator {
                        creator_type: creator.creator_type.clone(),
                        first_name: creator.first_name.clone(),
                        last_name: creator.last_name.clone(),
                        name: creator.name.clone(),
                        field_mode: creator.field_mode,
                    })
                    .collect(),
                better_bibtex_keys: vec![],
            };
            if let Ok(mapped) = map_item(library.clone(), &item) {
                assert_eq!(mapped.record.id.library, library, "wrong library identity");
                assert_wire_round_trip(&mapped.record);
            }
            assert_no_contained_panic();
        }
    } else {
        assert!(reader.items().is_empty(), "a failed first read kept items");
    }
    assert_in_time(started);
}

/// `better-bibtex/read-only.json`: keys only ever for well-formed items.
pub fn check_bbt_keys(data: &[u8]) {
    if data.len() > MAX_INPUT_BYTES {
        return;
    }
    watch_panics();
    let started = Instant::now();
    if let Ok(keys) = read_only_keys(data) {
        for key in keys {
            assert!(!key.citation_key.is_empty(), "an empty Better BibTeX key");
            assert!(!key.item_key.is_empty(), "a key for no item");
        }
    }
    assert_no_contained_panic();
    assert_in_time(started);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The seeds reach the item reader and Better BibTeX's keys, not an early refusal.
    #[test]
    fn seeds_reach_the_readers() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");
        let items = std::fs::read(root.join("zotero_live/items.json")).unwrap_or_else(|_| panic!("no seed"));
        let mut transport = Invented {
            versions: inventory(&items),
            items,
            count: 2,
            version: 1,
            requests: 0,
        };
        let mut reader = LiveReader::new(Library::User);
        assert!(reader.refresh(&mut transport, false, &|| false).is_ok());
        assert_eq!(reader.items().len(), 2);
        let keys = std::fs::read(root.join("bbt_keys/read-only.json")).unwrap_or_else(|_| panic!("no seed"));
        assert!(read_only_keys(&keys).is_ok_and(|keys| !keys.is_empty()));
    }
}
