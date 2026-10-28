//! Better BibTeX citation keys that Zotero's own citation key field does not
//! hold.
//!
//! Better BibTeX 8 keeps its keys in Zotero's `citationKey` field, which both
//! Zotero readers already map. Two stores remain outside that field:
//!
//! - `better-bibtex.sqlite` (table `citationkey`), where Better BibTeX 7 (for
//!   Zotero 7) keeps every key. Better BibTeX 8 only migrates this file and
//!   never serves keys from it, so it applies to Zotero 7 alone;
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
//! key output where they disagree with Zotero's own key or Extra. Any error,
//! including [`BbtError::Unavailable`] while Better BibTeX is still starting,
//! and an ambiguous group name means "Better BibTeX keys unknown": callers
//! refuse key output that depends on them rather than publish "no keys",
//! which would change the library's fingerprint from one read to the next.

use std::collections::BTreeMap;
use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use ibis_core::zotero::api::TransportError;
use serde_json::{Value, json};

use crate::{CONNECT_TIMEOUT, DEADLINE, DEFAULT_PORT, Deadline, IO_TIMEOUT, io_error, read_response};

/// Better BibTeX's own database, in Zotero's data directory.
pub const STORE_FILE: &str = "better-bibtex.sqlite";
/// Keys for read-only libraries, relative to Zotero's data directory.
pub const READ_ONLY_FILE: [&str; 2] = ["better-bibtex", "read-only.json"];
/// As many records as the closed reader reads items.
pub const MAX_RECORDS: usize = 1_000_000;
/// Better BibTeX keys are short; a longer one marks a damaged store. With
/// [`MAX_RECORDS`] this bounds the memory a store's keys can hold.
const MAX_KEY_BYTES: usize = 256;
/// Item keys sent in one `item.citationkey` request. Better BibTeX looks each
/// one up by scanning all its keys on Zotero's main thread, so batches stay
/// small and a whole call is bounded.
const BATCH: usize = 500;
/// Item keys one [`LiveBbt::keys`] call accepts.
pub const MAX_LIVE_KEYS: usize = 20_000;
/// Time for all batches of one [`LiveBbt::keys`] call.
const KEYS_DEADLINE: Duration = Duration::from_secs(60);

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

/// Reads the `citationkey` table of a private copy of [`STORE_FILE`] for a
/// Zotero of major version `zotero`. Only Better BibTeX 7, for Zotero 7, serves
/// these keys; a store without that table is an older layout Ibis does not read.
pub fn read_store(copy: &rusqlite::Connection, zotero: u32) -> Result<Vec<StoredKey>, BbtError> {
    if zotero != 7 {
        return Err(BbtError::Unavailable);
    }
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
    if !item_key_shape(&item_key) || citation_key.len() > MAX_KEY_BYTES {
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
        // Zotero answers 404 without Better BibTeX: no library has its keys.
        // An older Better BibTeX without this method holds group keys that
        // cannot be placed, so that is not "no keys".
        let result = self.call_until(
            "user.groups",
            json!([false]),
            Instant::now() + DEADLINE,
            BbtError::Malformed,
        )?;
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
    /// item key. `None` is the personal library. Pass only items whose own
    /// Zotero key is empty: Better BibTeX 8 serves that key for every other
    /// item. Items without a key are left out; malformed item keys are never
    /// sent.
    pub fn keys(&self, library_id: Option<i64>, item_keys: &[&str]) -> Result<BTreeMap<String, String>, BbtError> {
        let mut found = BTreeMap::new();
        let wanted: Vec<&str> = item_keys.iter().copied().filter(|key| item_key_shape(key)).collect();
        if wanted.len() > MAX_LIVE_KEYS {
            return Err(BbtError::TooLarge);
        }
        let until = Instant::now() + KEYS_DEADLINE;
        for batch in wanted.chunks(BATCH) {
            if Instant::now() >= until {
                return Err(TransportError::TimedOut.into());
            }
            let names: Vec<String> = batch
                .iter()
                .map(|key| match library_id {
                    Some(id) => format!("{id}:{key}"),
                    None => (*key).to_owned(),
                })
                .collect();
            let result = self.call_until("item.citationkey", json!([names]), until, BbtError::Unavailable)?;
            let map = result.as_object().ok_or(BbtError::Malformed)?;
            for (name, key) in batch.iter().zip(&names) {
                match map.get(key) {
                    // A key longer than any real one marks a damaged answer.
                    Some(Value::String(citation_key)) if citation_key.len() > MAX_KEY_BYTES => {
                        return Err(BbtError::Malformed);
                    }
                    Some(Value::String(citation_key)) if !citation_key.trim().is_empty() => {
                        found.insert((*name).to_owned(), citation_key.clone());
                    }
                    Some(Value::String(_) | Value::Null) => {}
                    // Better BibTeX answers for every key it was asked about.
                    Some(_) | None => return Err(BbtError::Malformed),
                }
            }
        }
        Ok(found)
    }

    /// `missing` is the error for a Better BibTeX that lacks `method`.
    fn call_until(&self, method: &str, params: Value, until: Instant, missing: BbtError) -> Result<Value, BbtError> {
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
            until: until.min(Instant::now() + DEADLINE),
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
                missing
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
