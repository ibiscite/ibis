//! Better BibTeX citation keys that Zotero's own citation key field does not
//! hold.
//!
//! Better BibTeX 8 keeps its keys in Zotero's `citationKey` field, which both
//! Zotero readers already map. Two stores remain outside that field:
//!
//! - `better-bibtex.sqlite` (table `citationkey`), written by Better BibTeX 7
//!   and kept by Better BibTeX 8 until its one-time migration;
//! - `better-bibtex/read-only.json`, the keys Better BibTeX gives items in
//!   read-only group libraries, which it cannot write into Zotero.
//!
//! Both are read only from private copies while Zotero is closed. While Zotero
//! runs, Better BibTeX's JSON-RPC endpoint on Zotero's own loopback port gives
//! the keys it serves for any library. Only the read-only methods
//! `user.groups` and `item.citationkey` are called, with no proxy and no
//! redirect (maintainer decision on #1231).
//!
//! Keys are passed on as found: the shared mapper validates them and refuses
//! key output where they disagree with Zotero's own key or Extra.

use std::collections::BTreeMap;
use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Instant;

use ibis_core::zotero::api::TransportError;
use serde_json::{Value, json};

use crate::{CONNECT_TIMEOUT, DEADLINE, DEFAULT_PORT, Deadline, IO_TIMEOUT, io_error, read_response};

/// Better BibTeX's own database, in Zotero's data directory.
pub const STORE_FILE: &str = "better-bibtex.sqlite";
/// Keys for read-only libraries, relative to Zotero's data directory.
pub const READ_ONLY_FILE: [&str; 2] = ["better-bibtex", "read-only.json"];
/// More records than any library Ibis reads (the mapper's own limit is lower).
pub const MAX_RECORDS: usize = 2_000_000;
/// Item keys sent in one `item.citationkey` request.
const BATCH: usize = 5_000;

/// Why Better BibTeX keys could not be read. Fixed text; never library content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BbtError {
    /// No Better BibTeX store, or Zotero answers without Better BibTeX.
    #[error("Better BibTeX keys are not available")]
    Unavailable,
    #[error("Better BibTeX's keys could not be read")]
    Malformed,
    #[error("Better BibTeX holds more keys than Ibis reads")]
    TooLarge,
    #[error(transparent)]
    Transport(#[from] TransportError),
}

/// One key from a Better BibTeX store, addressed the way Zotero's database
/// addresses items: local library ID and item key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredKey {
    pub library_id: i64,
    pub item_key: String,
    pub citation_key: String,
}

/// Reads the `citationkey` table of a private copy of [`STORE_FILE`]. A store
/// without that table is an older layout Ibis does not read.
pub fn read_store(copy: &rusqlite::Connection) -> Result<Vec<StoredKey>, BbtError> {
    let has_table: bool = copy
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'citationkey')",
            [],
            |row| row.get(0),
        )
        .map_err(|_| BbtError::Malformed)?;
    if !has_table {
        return Err(BbtError::Unavailable);
    }
    let mut statement = copy
        .prepare("SELECT libraryID, itemKey, citationKey FROM citationkey")
        .map_err(|_| BbtError::Malformed)?;
    let mut rows = statement.query([]).map_err(|_| BbtError::Malformed)?;
    let mut keys = Vec::new();
    while let Some(row) = rows.next().map_err(|_| BbtError::Malformed)? {
        let library_id: i64 = row.get(0).map_err(|_| BbtError::Malformed)?;
        let item_key: String = row.get(1).map_err(|_| BbtError::Malformed)?;
        let citation_key: Option<String> = row.get(2).map_err(|_| BbtError::Malformed)?;
        push(&mut keys, library_id, item_key, citation_key)?;
    }
    Ok(keys)
}

/// Parses [`READ_ONLY_FILE`]: an array of `{itemID, libraryID, itemKey,
/// citationKey}` records. Better BibTeX serves Zotero's own key over these, so
/// the caller uses one only for an item whose own key is empty.
pub fn read_only_keys(json: &[u8]) -> Result<Vec<StoredKey>, BbtError> {
    let records: Vec<Value> = serde_json::from_slice(json).map_err(|_| BbtError::Malformed)?;
    let mut keys = Vec::new();
    for record in records {
        let library_id = record
            .get("libraryID")
            .and_then(Value::as_i64)
            .ok_or(BbtError::Malformed)?;
        let item_key = record
            .get("itemKey")
            .and_then(Value::as_str)
            .ok_or(BbtError::Malformed)?;
        let citation_key = record.get("citationKey").and_then(Value::as_str);
        push(
            &mut keys,
            library_id,
            item_key.to_owned(),
            citation_key.map(str::to_owned),
        )?;
    }
    Ok(keys)
}

fn push(
    keys: &mut Vec<StoredKey>,
    library_id: i64,
    item_key: String,
    citation_key: Option<String>,
) -> Result<(), BbtError> {
    // Better BibTeX skips records without a key; so does Ibis.
    let Some(citation_key) = citation_key.filter(|key| !key.trim().is_empty()) else {
        return Ok(());
    };
    if !item_key_shape(&item_key) {
        return Err(BbtError::Malformed);
    }
    if keys.len() == MAX_RECORDS {
        return Err(BbtError::TooLarge);
    }
    keys.push(StoredKey {
        library_id,
        item_key,
        citation_key,
    });
    Ok(())
}

/// Zotero item keys are eight upper-case letters and digits.
fn item_key_shape(key: &str) -> bool {
    key.len() == 8 && key.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

/// A library as Better BibTeX names it: Zotero's local library ID and name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BbtLibrary {
    pub library_id: i64,
    pub name: String,
}

/// The local library ID of the group Zotero's local API calls `name`, when
/// exactly one Better BibTeX library has that name. The local API addresses
/// groups by their online ID, Better BibTeX by local ID, and only the name is
/// shared; an ambiguous name gives no keys rather than another library's.
pub fn library_named(libraries: &[BbtLibrary], name: &str) -> Option<i64> {
    let mut matches = libraries.iter().filter(|library| library.name == name);
    let first = matches.next()?;
    matches.next().is_none().then_some(first.library_id)
}

/// Better BibTeX's JSON-RPC endpoint while Zotero runs.
#[derive(Debug, Clone, Copy)]
pub struct LiveBbt {
    port: u16,
}

impl Default for LiveBbt {
    fn default() -> Self {
        LiveBbt { port: DEFAULT_PORT }
    }
}

impl LiveBbt {
    /// Another port on this computer, for tests.
    pub fn on_port(port: u16) -> Self {
        LiveBbt { port }
    }

    /// Every library Better BibTeX knows (`user.groups`).
    pub fn libraries(&self) -> Result<Vec<BbtLibrary>, BbtError> {
        let result = self.call("user.groups", json!([false]))?;
        let list = result.as_array().ok_or(BbtError::Malformed)?;
        list.iter()
            .map(|library| {
                Ok(BbtLibrary {
                    library_id: library.get("id").and_then(Value::as_i64).ok_or(BbtError::Malformed)?,
                    name: library
                        .get("name")
                        .and_then(Value::as_str)
                        .ok_or(BbtError::Malformed)?
                        .to_owned(),
                })
            })
            .collect()
    }

    /// The keys Better BibTeX serves for `item_keys` (`item.citationkey`), by
    /// item key. `None` is the personal library. Items without a key are left
    /// out; malformed item keys are never sent.
    pub fn keys(&self, library_id: Option<i64>, item_keys: &[&str]) -> Result<BTreeMap<String, String>, BbtError> {
        let mut found = BTreeMap::new();
        let wanted: Vec<&str> = item_keys.iter().copied().filter(|key| item_key_shape(key)).collect();
        for batch in wanted.chunks(BATCH) {
            let names: Vec<String> = batch
                .iter()
                .map(|key| match library_id {
                    Some(id) => format!("{id}:{key}"),
                    None => (*key).to_owned(),
                })
                .collect();
            let result = self.call("item.citationkey", json!([names]))?;
            let map = result.as_object().ok_or(BbtError::Malformed)?;
            for (name, key) in batch.iter().zip(&names) {
                match map.get(key) {
                    Some(Value::String(citation_key)) if !citation_key.trim().is_empty() => {
                        found.insert((*name).to_owned(), citation_key.clone());
                    }
                    Some(Value::String(_) | Value::Null) | None => {}
                    Some(_) => return Err(BbtError::Malformed),
                }
            }
        }
        Ok(found)
    }

    fn call(&self, method: &str, params: Value) -> Result<Value, BbtError> {
        let body = json!({"jsonrpc": "2.0", "method": method, "params": params, "id": 1}).to_string();
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, self.port));
        let mut stream =
            TcpStream::connect_timeout(&address, CONNECT_TIMEOUT).map_err(|_| TransportError::Unreachable)?;
        stream
            .set_read_timeout(Some(IO_TIMEOUT))
            .map_err(|_| TransportError::Unreachable)?;
        stream
            .set_write_timeout(Some(IO_TIMEOUT))
            .map_err(|_| TransportError::Unreachable)?;
        let request = format!(
            "POST /better-bibtex/json-rpc HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nUser-Agent: Ibis/{}\r\n\
             Content-Type: application/json\r\nAccept: application/json\r\nAccept-Encoding: identity\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.port,
            env!("CARGO_PKG_VERSION"),
            body.len()
        );
        stream.write_all(request.as_bytes()).map_err(io_error)?;
        let response = read_response(Deadline {
            stream,
            until: Instant::now() + DEADLINE,
        })?;
        // Zotero answers 404 when Better BibTeX is not installed. Anything but
        // 200 (including a redirect, which is never followed) is not an answer.
        match response.status {
            200 => {}
            404 => return Err(BbtError::Unavailable),
            _ => return Err(BbtError::Malformed),
        }
        let mut answer: Value = serde_json::from_slice(&response.body).map_err(|_| BbtError::Malformed)?;
        if let Some(error) = answer.get("error") {
            // An older Better BibTeX without this method.
            return Err(if error.get("code").and_then(Value::as_i64) == Some(-32601) {
                BbtError::Unavailable
            } else {
                BbtError::Malformed
            });
        }
        answer.get_mut("result").map(Value::take).ok_or(BbtError::Malformed)
    }
}

#[cfg(test)]
#[path = "bbt/tests.rs"]
mod tests;
