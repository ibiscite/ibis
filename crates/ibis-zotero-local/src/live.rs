//! Read-only, bounded raw local-API acquisition. Never invokes an exporter.
//!
//! Protocol pinned to Zotero db41b99fddcc52d38897c59c161b6adaca036afb,
//! server_localAPI.js: versions/keys at 504–548, itemKey at 1057–1068.
//! Zotero 10 versions are local transaction counters; 7–9 versions are sync
//! counters, so those releases conservatively reread every item.
//! The transport supplies only loopback GETs; it never follows response links.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub use ibis_core::zotero::Library;
use ibis_core::zotero::api::{Response, Transport, TransportError};
use serde_json::Value;

const KEY_BATCH: usize = 50;

/// Raw bibliographic data. Locations, versions and modification watermarks are
/// provider state, never fields passed to the shared source mapper.
#[derive(Clone, PartialEq, Eq)]
pub struct ItemData {
    pub key: String,
    pub item_type: String,
    pub creators: Vec<CreatorData>,
    pub fields: BTreeMap<String, String>,
}
impl fmt::Debug for ItemData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ItemData").finish_non_exhaustive()
    }
}
#[derive(Clone, Default, PartialEq, Eq)]
pub struct CreatorData {
    pub creator_type: String,
    pub first_name: String,
    pub last_name: String,
    pub name: String,
    pub field_mode: u8,
}
impl fmt::Debug for CreatorData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreatorData").finish_non_exhaustive()
    }
}

/// Fixed messages only; no payload, path or OS error can escape in diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveError {
    Transport(TransportError),
    NotAllowed,
    GroupNotFound,
    Status(u16),
    UnsupportedVersion,
    Incomplete,
    Changing,
    Limit,
    Cancelled,
}
impl fmt::Display for LiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(e) => e.fmt(f),
            Self::NotAllowed => f.write_str("enable Allow other applications in Zotero's Advanced settings"),
            Self::GroupNotFound => f.write_str("the Zotero group library was not found"),
            Self::Status(status) => write!(f, "Zotero returned an unexpected status ({status})"),
            Self::UnsupportedVersion => f.write_str("this Zotero API version is not supported"),
            Self::Incomplete => f.write_str("Zotero returned incomplete data"),
            Self::Changing => f.write_str("the Zotero library kept changing while being read"),
            Self::Limit => f.write_str("reading Zotero exceeded the work limit"),
            Self::Cancelled => f.write_str("reading Zotero was cancelled"),
        }
    }
}
impl std::error::Error for LiveError {}

/// Per-refresh limits, shared across every retry. Cancellation is cooperative:
/// an in-flight Transport call is bounded by the transport's own deadline.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub requests: usize,
    pub bytes: usize,
    pub items: usize,
    pub elapsed: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            requests: 12_000,
            bytes: 512 * 1024 * 1024,
            items: 1_000_000,
            elapsed: Duration::from_secs(600),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refresh {
    Changed,
    Unchanged,
}

#[derive(Clone, PartialEq, Eq)]
struct Server {
    major: u8,
    id: Option<String>,
}
#[derive(Clone)]
struct Inventory {
    server: Server,
    version: u64,
    versions: BTreeMap<String, u64>,
}
#[derive(Clone, PartialEq, Eq)]
struct Entry {
    item: Option<ItemData>,
    modified: String,
}
#[derive(Clone)]
struct State {
    inventory: Inventory,
    entries: BTreeMap<String, Arc<Entry>>,
}

/// A failed/cancelled refresh never replaces the last successful state.
/// A caller publishes its immutable mapped snapshot only after this succeeds.
#[derive(Clone)]
pub struct LiveReader {
    library: Library,
    state: Option<State>,
    limits: Limits,
}
impl fmt::Debug for LiveReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LiveReader")
            .field("loaded", &self.state.is_some())
            .finish_non_exhaustive()
    }
}
impl LiveReader {
    pub fn new(library: Library) -> Self {
        Self {
            library,
            state: None,
            limits: Limits::default(),
        }
    }
    pub fn with_limits(library: Library, limits: Limits) -> Self {
        Self {
            library,
            state: None,
            limits,
        }
    }
    pub fn items(&self) -> Vec<&ItemData> {
        self.state
            .iter()
            .flat_map(|state| state.entries.values())
            .filter_map(|entry| entry.item.as_ref())
            .collect()
    }
    /// Charge retained cache payloads, including nonbibliographic IDs and
    /// collection/string allocations. This is independent of refresh I/O: a
    /// growing library must not accumulate a new allowance on every refresh.
    pub fn retained_size(&self) -> (usize, usize) {
        let Some(state) = &self.state else { return (0, 0) };
        let mut bytes = 1024usize;
        for (key, entry) in &state.entries {
            bytes = bytes.saturating_add(512 + key.capacity() + entry.modified.capacity());
            if let Some(item) = &entry.item {
                bytes = bytes.saturating_add(item.key.capacity() + item.item_type.capacity());
                bytes = bytes.saturating_add(
                    item.creators
                        .capacity()
                        .saturating_mul(std::mem::size_of::<CreatorData>()),
                );
                for creator in &item.creators {
                    for text in [
                        &creator.creator_type,
                        &creator.first_name,
                        &creator.last_name,
                        &creator.name,
                    ] {
                        bytes = bytes.saturating_add(text.capacity());
                    }
                }
                for (name, value) in &item.fields {
                    bytes = bytes.saturating_add(128 + name.capacity() + value.capacity());
                }
            }
        }
        for key in state.inventory.versions.keys() {
            bytes = bytes.saturating_add(96 + key.capacity());
        }
        if let Some(id) = &state.inventory.server.id {
            bytes = bytes.saturating_add(id.capacity());
        }
        (state.entries.len(), bytes)
    }
    /// Zotero 10 reuses unchanged versions; older releases read all items.
    /// Reload and any server/counter discontinuity discard cached payloads.
    pub fn refresh(
        &mut self,
        transport: &mut impl Transport,
        reload: bool,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Refresh, LiveError> {
        if self.library == Library::Group(0) {
            return Err(LiveError::GroupNotFound);
        }
        let mut budget = Budget {
            limits: self.limits,
            started: Instant::now(),
            cancelled,
        };
        for attempt in 0..3 {
            let next = match self.read(transport, reload || attempt != 0, &mut budget) {
                Err(LiveError::Changing) => continue,
                outcome => outcome?,
            };
            budget.check()?;
            let changed = self
                .state
                .as_ref()
                .is_none_or(|old| old.entries != next.entries || old.inventory.server != next.inventory.server);
            let previous = self.state.replace(next);
            let (items, bytes) = self.retained_size();
            if items > self.limits.items || bytes > self.limits.bytes {
                self.state = previous;
                return Err(LiveError::Limit);
            }
            return Ok(if changed { Refresh::Changed } else { Refresh::Unchanged });
        }
        Err(LiveError::Changing)
    }
    fn prefix(&self) -> String {
        match self.library {
            Library::User => "users/0".into(),
            Library::Group(id) => format!("groups/{id}"),
        }
    }
    fn inventory(&self, transport: &mut impl Transport, budget: &mut Budget<'_>) -> Result<Inventory, LiveError> {
        let response = budget.get(transport, &format!("{}/items/top?format=versions", self.prefix()))?;
        let (server, version) = metadata(&response)?;
        let value: Value = serde_json::from_slice(&response.body).map_err(|_| LiveError::Incomplete)?;
        let object = value.as_object().ok_or(LiveError::Incomplete)?;
        if object.len() > budget.limits.items {
            return Err(LiveError::Limit);
        }
        let mut versions = BTreeMap::new();
        for (key, value) in object {
            if !valid_key(key) {
                return Err(LiveError::Incomplete);
            }
            let value = value.as_u64().ok_or(LiveError::Incomplete)?;
            if server.major == 10 && value > version {
                return Err(LiveError::Changing);
            }
            versions.insert(key.clone(), value);
        }
        let total = count(&response)?;
        if total != versions.len() {
            return Err(LiveError::Incomplete);
        }
        Ok(Inventory {
            server,
            version,
            versions,
        })
    }
    fn read(&self, transport: &mut impl Transport, reload: bool, budget: &mut Budget<'_>) -> Result<State, LiveError> {
        let before = self.inventory(transport, budget)?;
        let previous = self.state.as_ref().filter(|s| {
            !reload
                && before.server.major == 10
                && s.inventory.server == before.server
                && s.inventory.version <= before.version
        });
        let mut entries = previous.map(|s| s.entries.clone()).unwrap_or_default();
        entries.retain(|key, _| before.versions.contains_key(key));
        let changed: Vec<_> = before
            .versions
            .iter()
            .filter(|(key, version)| previous.is_none_or(|s| s.inventory.versions.get(*key) != Some(*version)))
            .map(|(key, _)| key.clone())
            .collect();
        for keys in changed.chunks(KEY_BATCH) {
            let response = budget.get(
                transport,
                &format!(
                    "{}/items/top?format=json&include=data&itemKey={}&limit={KEY_BATCH}",
                    self.prefix(),
                    keys.join(",")
                ),
            )?;
            fence(&response, &before)?;
            let rows = rows(&response)?;
            if rows.len() != keys.len() || count(&response)? != keys.len() {
                return Err(LiveError::Changing);
            }
            let mut seen = BTreeSet::new();
            for row in rows {
                let (key, entry, version) = entry(row)?;
                if !keys.contains(&key)
                    || !seen.insert(key.clone())
                    || before.server.major == 10 && before.versions.get(&key) != Some(&version)
                {
                    return Err(LiveError::Changing);
                }
                entries.insert(key, Arc::new(entry));
            }
        }
        let after = self.inventory(transport, budget)?;
        if before.server != after.server
            || before.version != after.version
            || before.versions != after.versions
            || entries.len() != after.versions.len()
        {
            return Err(LiveError::Changing);
        }
        Ok(State {
            inventory: after,
            entries,
        })
    }
}

struct Budget<'a> {
    limits: Limits,
    started: Instant,
    cancelled: &'a dyn Fn() -> bool,
}
impl Budget<'_> {
    fn check(&self) -> Result<(), LiveError> {
        if (self.cancelled)() {
            return Err(LiveError::Cancelled);
        }
        if self.started.elapsed() >= self.limits.elapsed {
            return Err(LiveError::Transport(TransportError::TimedOut));
        }
        Ok(())
    }
    fn get(&mut self, transport: &mut impl Transport, path: &str) -> Result<Response, LiveError> {
        self.check()?;
        if self.limits.requests == 0 {
            return Err(LiveError::Limit);
        }
        self.limits.requests -= 1;
        let response = transport.get(path).map_err(LiveError::Transport)?;
        self.check()?;
        self.limits.bytes = self
            .limits
            .bytes
            .checked_sub(response.body.len())
            .ok_or(LiveError::Limit)?;
        match response.status {
            200 => (),
            403 => return Err(LiveError::NotAllowed),
            404 if path.starts_with("groups/") => return Err(LiveError::GroupNotFound),
            status => return Err(LiveError::Status(status)),
        }
        if response.header("Zotero-API-Version") != Some("3") {
            return Err(LiveError::UnsupportedVersion);
        }
        Ok(response)
    }
}
fn server(response: &Response) -> Result<Server, LiveError> {
    let major = response
        .header("X-Zotero-Version")
        .and_then(|v| v.split('.').next())
        .and_then(|v| v.parse::<u8>().ok())
        .filter(|v| (7..=10).contains(v))
        .ok_or(LiveError::UnsupportedVersion)?;
    let id = response.header("Zotero-Server-ID").map(str::to_owned);
    if id
        .as_ref()
        .is_some_and(|id| id.is_empty() || id.len() > 128 || !id.bytes().all(|b| b.is_ascii_graphic()))
        || major == 10 && id.is_none()
    {
        return Err(LiveError::Incomplete);
    }
    Ok(Server { major, id })
}
fn metadata(response: &Response) -> Result<(Server, u64), LiveError> {
    let server = server(response)?;
    let version = response
        .header("Last-Modified-Version")
        .and_then(number)
        .ok_or(LiveError::Incomplete)?;
    Ok((server, version))
}
fn fence(response: &Response, inventory: &Inventory) -> Result<(), LiveError> {
    let (server, version) = metadata(response)?;
    if server != inventory.server || version != inventory.version {
        return Err(LiveError::Changing);
    }
    Ok(())
}
fn number(value: &str) -> Option<u64> {
    (!value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()))
        .then(|| value.parse().ok())
        .flatten()
}
fn count(response: &Response) -> Result<usize, LiveError> {
    response
        .header("Total-Results")
        .and_then(number)
        .and_then(|n| n.try_into().ok())
        .ok_or(LiveError::Incomplete)
}
fn rows(response: &Response) -> Result<Vec<Value>, LiveError> {
    serde_json::from_slice::<Vec<Value>>(&response.body).map_err(|_| LiveError::Incomplete)
}
fn valid_key(key: &str) -> bool {
    key.len() == 8 && key.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
fn timestamp(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() != 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
    {
        return None;
    }
    let part = |a: usize, b: usize| std::str::from_utf8(&bytes[a..b]).ok().and_then(number);
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        part(0, 4),
        part(5, 7),
        part(8, 10),
        part(11, 13),
        part(14, 16),
        part(17, 19),
    ) else {
        return None;
    };
    let days = match month {
        2 => {
            if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    if !(year > 0 && (1..=days).contains(&day) && hour < 24 && minute < 60 && second < 60) {
        return None;
    }
    let year = year as i64 - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let month = month as i64;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i64 - 1;
    let days = era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468;
    Some(days * 86400 + hour as i64 * 3600 + minute as i64 * 60 + second as i64)
}
fn entry(row: Value) -> Result<(String, Entry, u64), LiveError> {
    let data = row
        .get("data")
        .and_then(Value::as_object)
        .ok_or(LiveError::Incomplete)?;
    let key = data
        .get("key")
        .and_then(Value::as_str)
        .filter(|key| valid_key(key))
        .ok_or(LiveError::Incomplete)?;
    if row.get("key").and_then(Value::as_str) != Some(key) {
        return Err(LiveError::Incomplete);
    }
    let version = data
        .get("version")
        .and_then(Value::as_u64)
        .ok_or(LiveError::Incomplete)?;
    if row.get("version").and_then(Value::as_u64) != Some(version) {
        return Err(LiveError::Incomplete);
    }
    let modified = data
        .get("dateModified")
        .and_then(Value::as_str)
        .filter(|v| timestamp(v).is_some())
        .ok_or(LiveError::Incomplete)?
        .to_owned();
    let kind = data
        .get("itemType")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 128)
        .ok_or(LiveError::Incomplete)?;
    let item = if matches!(kind, "note" | "attachment" | "annotation")
        || data.get("deleted").is_some_and(|v| v == 1 || v == true)
    {
        None
    } else {
        let mut creators = Vec::new();
        if let Some(values) = data.get("creators") {
            let values = values.as_array().ok_or(LiveError::Incomplete)?;
            if values.len() > 4096 {
                return Err(LiveError::Limit);
            }
            for value in values {
                let object = value.as_object().ok_or(LiveError::Incomplete)?;
                let text = |name| -> Result<String, LiveError> {
                    match object.get(name) {
                        None => Ok(String::new()),
                        Some(Value::String(text)) if text.len() <= 256 * 1024 => Ok(text.clone()),
                        _ => Err(LiveError::Incomplete),
                    }
                };
                let field_mode = match object.get("fieldMode") {
                    None => 0,
                    Some(v) => v.as_u64().filter(|v| *v <= 1).ok_or(LiveError::Incomplete)? as u8,
                };
                creators.push(CreatorData {
                    creator_type: text("creatorType")?,
                    first_name: text("firstName")?,
                    last_name: text("lastName")?,
                    name: text("name")?,
                    field_mode,
                });
            }
        }
        let mut fields = BTreeMap::new();
        for (name, value) in data {
            if matches!(
                name.as_str(),
                "key"
                    | "creators"
                    | "tags"
                    | "collections"
                    | "relations"
                    | "itemType"
                    | "dateAdded"
                    | "dateModified"
                    | "parentItem"
                    | "version"
                    | "deleted"
                    | "path"
                    | "filename"
                    | "note"
                    | "contentType"
                    | "linkMode"
                    | "md5"
                    | "charset"
                    | "mtime"
            ) {
                continue;
            }
            if let Some(text) = value.as_str() {
                if text.len() > 256 * 1024 {
                    return Err(LiveError::Limit);
                }
                fields.insert(name.clone(), text.to_owned());
            }
        }
        if fields.len() > 256 || serde_json::to_vec(data).map_err(|_| LiveError::Incomplete)?.len() > 2 * 1024 * 1024 {
            return Err(LiveError::Limit);
        }
        Some(ItemData {
            key: key.into(),
            item_type: kind.into(),
            creators,
            fields,
        })
    };
    Ok((key.to_owned(), Entry { item, modified }, version))
}
