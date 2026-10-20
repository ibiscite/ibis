//! Reading a library through Zotero's local API while Zotero runs.
//!
//! The rules of the conversation live here and are tested against a fake
//! server; sending the requests is left to a [`Transport`], so this crate
//! never touches the network. Zotero serves the API on
//! `http://127.0.0.1:23119/api/` once "Allow other applications on this
//! computer to communicate with Zotero" is on.

use serde_json::Value;
use thiserror::Error;

use super::{Library, load_items};
use crate::model::{LoadResult, LoadWarning, WarningKind};

/// Items requested per page; the most Zotero returns at once.
pub const PAGE_SIZE: usize = 100;
/// Requests made in one scan before giving up: one million items, with room
/// to narrow down items Zotero cannot export.
const MAX_REQUESTS: usize = 12_000;
/// Items Zotero could not export before the whole scan is given up as broken.
const MAX_SKIPPED: usize = 100;
/// Scans restarted because the library changed underneath them.
const MAX_ATTEMPTS: usize = 3;

/// A response from Zotero. Header names are matched without regard to case.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.trim())
    }
}

/// Sends one GET request to the local API. `path` starts after `/api/`, for
/// example `users/0/items?limit=1`.
pub trait Transport {
    fn get(&mut self, path: &str) -> Result<Response, TransportError>;
}

/// Why a request could not be completed. Never carries payloads or
/// operating system messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TransportError {
    #[error("Zotero is not running, or it is not accepting connections")]
    Unreachable,
    #[error("Zotero did not answer in time")]
    TimedOut,
    #[error("Zotero's answer could not be read")]
    Malformed,
    #[error("Zotero's answer was too large")]
    TooLarge,
}

/// Why a library could not be read. Messages are fixed text for users and
/// diagnostics; they never include library content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ZoteroError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error(
        "Zotero refused the connection. In Zotero, open Settings > Advanced and turn on \
         \"Allow other applications on this computer to communicate with Zotero\""
    )]
    NotAllowed,
    #[error("this Zotero group library was not found; check that you are still a member")]
    GroupNotFound,
    #[error("Zotero answered with an unexpected status ({0})")]
    Status(u16),
    #[error("this version of Zotero speaks a different API version; update Zotero or Ibis")]
    UnsupportedApi,
    #[error("Zotero's answer was incomplete")]
    Incomplete,
    #[error("the library kept changing while it was read; Ibis will try again")]
    Changing,
    #[error("the library is larger than Ibis can read at once")]
    TooLarge,
}

/// What a scan saw of the library, kept to decide whether the next scan
/// needs to read it again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// The library version (`Last-Modified-Version`) the items belong to.
    pub version: u64,
    /// Zotero's server ID, when it sends one. A different ID means another
    /// Zotero installation or a reset database, so versions are not comparable.
    pub server_id: Option<String>,
}

/// The outcome of [`refresh`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refresh {
    /// Nothing changed since the snapshot; keep the references already loaded.
    Unchanged,
    /// The whole library, read at one version.
    Loaded { result: LoadResult, snapshot: Snapshot },
}

fn items_path(library: Library) -> String {
    match library {
        Library::User => "users/0/items".to_string(),
        Library::Group(id) => format!("groups/{id}/items"),
    }
}

/// Only top-level items are references. Asking for them alone spares Zotero
/// a citation export for every child note and attachment (#301).
fn top_items_path(library: Library) -> String {
    format!("{}/top", items_path(library))
}

/// Read the library if it changed since `previous`, otherwise report that
/// nothing changed. A change is detected cheaply with `since=`, which lists
/// only the versions of changed items; any change reads the whole library at
/// one consistent version.
pub fn refresh(
    transport: &mut impl Transport,
    library: Library,
    previous: Option<&Snapshot>,
) -> Result<Refresh, ZoteroError> {
    if let Some(previous) = previous {
        let probe = request(
            transport,
            library,
            &format!(
                "{}?format=versions&includeTrashed=1&since={}",
                items_path(library),
                previous.version
            ),
        )?;
        let unchanged = snapshot(&probe)? == *previous
            && serde_json::from_slice::<serde_json::Map<String, Value>>(&probe.body)
                .map_err(|_| ZoteroError::Incomplete)?
                .is_empty();
        if unchanged {
            return Ok(Refresh::Unchanged);
        }
    }
    let (result, snapshot) = scan(transport, library)?;
    Ok(Refresh::Loaded { result, snapshot })
}

/// Read every item of the library at one version. If the library changes
/// between pages, the scan starts again, at most a few times.
pub fn scan(transport: &mut impl Transport, library: Library) -> Result<(LoadResult, Snapshot), ZoteroError> {
    for _ in 0..MAX_ATTEMPTS {
        if let Some(done) = scan_once(transport, library)? {
            return Ok(done);
        }
    }
    Err(ZoteroError::Changing)
}

fn scan_once(transport: &mut impl Transport, library: Library) -> Result<Option<(LoadResult, Snapshot)>, ZoteroError> {
    let mut items: Vec<Value> = Vec::new();
    let mut first: Option<Snapshot> = None;
    let mut total: Option<usize> = None;
    let mut skipped: Vec<LoadWarning> = Vec::new();
    // Ranges still to read, last first. A page Zotero fails to export is
    // split in halves until the one item it cannot export is found.
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut next = 0;
    for _ in 0..MAX_REQUESTS {
        let (start, limit) = ranges.pop().unwrap_or_else(|| {
            next += PAGE_SIZE;
            (next - PAGE_SIZE, PAGE_SIZE)
        });
        let path = format!(
            "{}?format=json&include=data,csljson&limit={limit}&start={start}",
            top_items_path(library)
        );
        let response = transport.get(&path)?;
        if response.status == 500 {
            if limit > 1 {
                ranges.push((start + limit / 2, limit - limit / 2));
                ranges.push((start, limit / 2));
            } else {
                // A failed export carries no library version, so the skipped
                // item is tied to one through the key lookup's answer.
                let (warning, keys) = unexported(transport, library, start)?;
                if !consistent(&keys, &mut first, &mut total)? {
                    return Ok(None);
                }
                skipped.push(warning);
                if skipped.len() > MAX_SKIPPED {
                    return Err(ZoteroError::Status(500));
                }
            }
            continue;
        }
        let response = checked(response, library)?;
        if !consistent(&response, &mut first, &mut total)? {
            return Ok(None);
        }
        let count = total.ok_or(ZoteroError::Incomplete)?;
        let Value::Array(page_items) = serde_json::from_slice(&response.body).map_err(|_| ZoteroError::Incomplete)?
        else {
            return Err(ZoteroError::Incomplete);
        };
        if page_items.len() > limit {
            return Err(ZoteroError::Incomplete);
        }
        let got = page_items.len();
        items.extend(page_items);
        if !ranges.is_empty() {
            continue;
        }
        if items.len() + skipped.len() >= count {
            if items.is_empty() && !skipped.is_empty() {
                // Zotero exported nothing at all: that is a broken Zotero,
                // not an empty library.
                return Err(ZoteroError::Status(500));
            }
            let snapshot = first.ok_or(ZoteroError::Incomplete)?;
            let mut result = load_items(library, &items);
            result.warnings.extend(skipped);
            return Ok(Some((result, snapshot)));
        }
        if got == 0 {
            // The library shrank between pages without a new version, or
            // the server miscounts: never return a partial library.
            return Err(ZoteroError::Incomplete);
        }
    }
    Err(ZoteroError::TooLarge)
}

/// Whether `response` belongs to the same library version and size as the
/// scan so far; the first answer sets both.
fn consistent(
    response: &Response,
    first: &mut Option<Snapshot>,
    total: &mut Option<usize>,
) -> Result<bool, ZoteroError> {
    let seen = snapshot(response)?;
    let count: usize = response
        .header("Total-Results")
        .and_then(|t| t.parse().ok())
        .ok_or(ZoteroError::Incomplete)?;
    if first.as_ref().is_some_and(|first| *first != seen) || total.is_some_and(|total| total != count) {
        return Ok(false);
    }
    *first = Some(seen);
    *total = Some(count);
    Ok(true)
}

/// A reading notice for the item at `position` that Zotero could not export,
/// and the key lookup's answer. Listing keys exports nothing, so it answers
/// even when the export fails, with the library version and size.
fn unexported(
    transport: &mut impl Transport,
    library: Library,
    position: usize,
) -> Result<(LoadWarning, Response), ZoteroError> {
    let response = request(
        transport,
        library,
        &format!("{}?format=keys&limit=1&start={position}", top_items_path(library)),
    )?;
    let key = std::str::from_utf8(&response.body)
        .ok()
        .map(|keys| keys.trim().to_string())
        .filter(|key| !key.is_empty() && key.len() <= 32 && key.bytes().all(|b| b.is_ascii_alphanumeric()));
    let message = match &key {
        Some(key) => format!("Zotero could not export the item `{key}`; it was skipped"),
        None => "Zotero could not export one item; it was skipped".to_string(),
    };
    let warning = LoadWarning {
        kind: WarningKind::Syntax,
        line: None,
        key,
        message,
    };
    Ok((warning, response))
}

pub(super) fn request(transport: &mut impl Transport, library: Library, path: &str) -> Result<Response, ZoteroError> {
    checked(transport.get(path)?, library)
}

fn checked(response: Response, library: Library) -> Result<Response, ZoteroError> {
    match response.status {
        200 => {}
        403 => return Err(ZoteroError::NotAllowed),
        404 if matches!(library, Library::Group(_)) => return Err(ZoteroError::GroupNotFound),
        status => return Err(ZoteroError::Status(status)),
    }
    if response.header("Zotero-API-Version") != Some("3") {
        return Err(ZoteroError::UnsupportedApi);
    }
    Ok(response)
}

fn snapshot(response: &Response) -> Result<Snapshot, ZoteroError> {
    let version = response
        .header("Last-Modified-Version")
        .and_then(|v| v.parse().ok())
        .ok_or(ZoteroError::Incomplete)?;
    let server_id = response.header("Zotero-Server-ID").map(str::to_string);
    Ok(Snapshot { version, server_id })
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use serde_json::json;

    use super::*;

    /// A scripted Zotero: answers requests in order and records them.
    #[derive(Default)]
    struct Fake {
        answers: VecDeque<Result<Response, TransportError>>,
        asked: Vec<String>,
    }

    impl Fake {
        fn answer(mut self, response: Response) -> Self {
            self.answers.push_back(Ok(response));
            self
        }
        fn fail(mut self, error: TransportError) -> Self {
            self.answers.push_back(Err(error));
            self
        }
    }

    impl Transport for Fake {
        fn get(&mut self, path: &str) -> Result<Response, TransportError> {
            self.asked.push(path.to_string());
            self.answers.pop_front().unwrap_or(Err(TransportError::Unreachable))
        }
    }

    fn book(key: &str) -> Value {
        // The local API sends CSL JSON as the text of its export, an array.
        let csl = json!([{ "type": "book", "title": key }]).to_string();
        json!({ "key": key, "data": { "itemType": "book" }, "csljson": csl })
    }

    fn page(version: u64, total: usize, keys: &[&str]) -> Response {
        let items: Vec<Value> = keys.iter().map(|k| book(k)).collect();
        Response {
            status: 200,
            headers: vec![
                ("zotero-api-version".into(), "3".into()),
                ("Last-Modified-Version".into(), version.to_string()),
                ("Total-Results".into(), total.to_string()),
            ],
            body: serde_json::to_vec(&items).unwrap_or_default(),
        }
    }

    fn keys(result: &LoadResult) -> Vec<&str> {
        result.references.iter().map(|r| r.id.native_key.as_str()).collect()
    }

    fn many(prefix: &str, n: usize) -> Vec<String> {
        (0..n).map(|i| format!("{prefix}{i}")).collect()
    }

    /// A Zotero whose export fails for the items at `bad` positions, answering
    /// any page by its `start` and `limit`, as the local API does.
    struct Exporting {
        keys: Vec<String>,
        bad: Vec<usize>,
        asked: Vec<String>,
        version: u64,
        /// Right after a key lookup, the library gains an item and the
        /// failing export is fixed.
        change_on_lookup: bool,
    }

    impl Exporting {
        fn new(keys: Vec<String>, bad: Vec<usize>) -> Self {
            Self {
                keys,
                bad,
                asked: Vec::new(),
                version: 9,
                change_on_lookup: false,
            }
        }
    }

    impl Transport for Exporting {
        fn get(&mut self, path: &str) -> Result<Response, TransportError> {
            self.asked.push(path.to_string());
            let number = |name: &str| -> usize {
                path.split(['?', '&'])
                    .find_map(|part| part.strip_prefix(name))
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0)
            };
            let (start, limit) = (number("start="), number("limit="));
            let end = (start + limit).min(self.keys.len());
            let range = start.min(end)..end;
            if path.contains("format=keys") {
                let mut answer = page(self.version, self.keys.len(), &[]);
                answer.body = self.keys[range].join("\n").into_bytes();
                if std::mem::take(&mut self.change_on_lookup) {
                    self.version += 1;
                    self.bad.clear();
                    self.keys.push("NEW".into());
                }
                return Ok(answer);
            }
            if self.bad.iter().any(|bad| range.contains(bad)) {
                return Ok(Response {
                    status: 500,
                    ..Response::default()
                });
            }
            let keys: Vec<&str> = self.keys[range].iter().map(String::as_str).collect();
            Ok(page(self.version, self.keys.len(), &keys))
        }
    }

    #[test]
    fn an_item_zotero_cannot_export_is_skipped_and_named_not_fatal() {
        let mut zotero = Exporting::new(many("K", PAGE_SIZE + 30), vec![7, PAGE_SIZE + 3]);
        let (result, _) = scan(&mut zotero, Library::User).unwrap_or_else(|e| panic!("{e}"));
        let expected: Vec<String> = zotero
            .keys
            .iter()
            .enumerate()
            .filter(|(i, _)| !zotero.bad.contains(i))
            .map(|(_, k)| k.clone())
            .collect();
        assert_eq!(keys(&result), expected);
        let skipped: Vec<_> = result.warnings.iter().map(|w| (w.kind, w.key.as_deref())).collect();
        assert_eq!(
            skipped,
            [(WarningKind::Syntax, Some("K7")), (WarningKind::Syntax, Some("K103"))]
        );
        assert!(result.warnings[0].message.contains("could not export"));
        // Narrowing a page down costs a handful of requests, not one per item.
        assert!(zotero.asked.len() < 40, "{} requests", zotero.asked.len());
        assert!(zotero.asked.iter().all(|path| path.starts_with("users/0/items/top?")));
    }

    #[test]
    fn a_zotero_that_cannot_export_anything_is_an_error_not_an_empty_library() {
        let mut zotero = Exporting::new(many("K", 3 * MAX_SKIPPED), (0..3 * MAX_SKIPPED).collect());
        assert_eq!(scan(&mut zotero, Library::User).err(), Some(ZoteroError::Status(500)));
    }

    #[test]
    fn a_change_after_skipping_a_bad_first_item_restarts_the_scan() {
        // The first item fails before any page has given a version, so its
        // key lookup sets version 9. The library then changes: the next page
        // at version 10 restarts the scan rather than dropping an item that
        // version 10 exports fine.
        let mut zotero = Exporting::new(many("K", 80), vec![0]);
        zotero.change_on_lookup = true;
        let (result, snapshot) = scan(&mut zotero, Library::User).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(result.references.len(), 81);
        assert_eq!(keys(&result).first(), Some(&"K0"));
        assert!(result.warnings.is_empty());
        assert_eq!(snapshot.version, 10);
    }

    #[test]
    fn a_library_whose_every_item_fails_is_an_error_not_empty() {
        let mut zotero = Exporting::new(many("K", 3), vec![0, 1, 2]);
        assert_eq!(scan(&mut zotero, Library::User).err(), Some(ZoteroError::Status(500)));
    }

    #[test]
    fn a_skipped_item_whose_key_cannot_be_read_is_still_reported() {
        let mut zotero = Exporting::new(vec!["A".into(), "B<script>".into(), "C".into()], vec![1]);
        let (result, _) = scan(&mut zotero, Library::User).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(keys(&result), ["A", "C"]);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].key, None);
        assert!(!result.warnings[0].message.contains("script"));
    }

    #[test]
    fn a_library_is_read_page_by_page_at_one_version() {
        let first = many("A", PAGE_SIZE);
        let first: Vec<&str> = first.iter().map(String::as_str).collect();
        let mut fake =
            Fake::default()
                .answer(page(9, PAGE_SIZE + 2, &first))
                .answer(page(9, PAGE_SIZE + 2, &["B1", "B2"]));
        let (result, snapshot) = scan(&mut fake, Library::Group(7)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(result.references.len(), PAGE_SIZE + 2);
        assert_eq!(
            snapshot,
            Snapshot {
                version: 9,
                server_id: None
            }
        );
        assert_eq!(
            fake.asked,
            [
                "groups/7/items/top?format=json&include=data,csljson&limit=100&start=0",
                "groups/7/items/top?format=json&include=data,csljson&limit=100&start=100",
            ]
        );
    }

    #[test]
    fn a_library_that_changes_mid_scan_is_read_again_from_the_start() {
        let first = many("A", PAGE_SIZE);
        let first: Vec<&str> = first.iter().map(String::as_str).collect();
        let mut fake = Fake::default()
            .answer(page(9, PAGE_SIZE + 1, &first))
            .answer(page(10, PAGE_SIZE + 1, &["B"]))
            .answer(page(10, 1, &["C"]));
        let (result, snapshot) = scan(&mut fake, Library::User).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(keys(&result), ["C"]);
        assert_eq!(snapshot.version, 10);
        assert_eq!(fake.asked.len(), 3);
        assert!(fake.asked[2].ends_with("start=0"));
    }

    #[test]
    fn a_library_that_never_settles_is_reported() {
        let first = many("A", PAGE_SIZE);
        let first: Vec<&str> = first.iter().map(String::as_str).collect();
        let mut fake = Fake::default();
        for version in 0..MAX_ATTEMPTS as u64 {
            fake = fake.answer(page(version * 2, PAGE_SIZE + 1, &first)).answer(page(
                version * 2 + 1,
                PAGE_SIZE + 1,
                &["B"],
            ));
        }
        assert_eq!(scan(&mut fake, Library::User), Err(ZoteroError::Changing));
    }

    #[test]
    fn a_short_page_never_yields_a_partial_library() {
        let mut fake = Fake::default().answer(page(3, 5, &["A"])).answer(page(3, 5, &[]));
        assert_eq!(scan(&mut fake, Library::User), Err(ZoteroError::Incomplete));
    }

    #[test]
    fn refused_missing_and_foreign_answers_are_explained() {
        let status = |status: u16| Response {
            status,
            ..page(1, 0, &[])
        };
        let ask = |response: Response, library: Library| scan(&mut Fake::default().answer(response), library);
        assert_eq!(ask(status(403), Library::User), Err(ZoteroError::NotAllowed));
        assert_eq!(ask(status(404), Library::Group(5)), Err(ZoteroError::GroupNotFound));
        assert_eq!(ask(status(404), Library::User), Err(ZoteroError::Status(404)));
        assert_eq!(ask(status(302), Library::User), Err(ZoteroError::Status(302)));
        let mut v2 = page(1, 0, &[]);
        v2.headers[0].1 = "2".into();
        assert_eq!(ask(v2, Library::User), Err(ZoteroError::UnsupportedApi));
        let mut unversioned = page(1, 0, &[]);
        unversioned.headers.retain(|(n, _)| n != "Last-Modified-Version");
        assert_eq!(ask(unversioned, Library::User), Err(ZoteroError::Incomplete));
        let mut garbled = page(1, 1, &[]);
        garbled.body = b"<html>".to_vec();
        assert_eq!(ask(garbled, Library::User), Err(ZoteroError::Incomplete));
        assert_eq!(
            scan(&mut Fake::default().fail(TransportError::Unreachable), Library::User),
            Err(ZoteroError::Transport(TransportError::Unreachable))
        );
        assert!(ZoteroError::NotAllowed.to_string().contains("Allow other applications"));
    }

    #[test]
    fn an_empty_library_loads_as_empty() {
        let (result, _) =
            scan(&mut Fake::default().answer(page(4, 0, &[])), Library::User).unwrap_or_else(|e| panic!("{e}"));
        assert!(result.references.is_empty());
    }

    #[test]
    fn refresh_reads_nothing_more_when_no_item_changed() {
        let previous = Snapshot {
            version: 4,
            server_id: None,
        };
        let mut probe = page(4, 0, &[]);
        probe.body = b"{}".to_vec();
        let mut fake = Fake::default().answer(probe);
        assert_eq!(
            refresh(&mut fake, Library::User, Some(&previous)),
            Ok(Refresh::Unchanged)
        );
        assert_eq!(fake.asked, ["users/0/items?format=versions&includeTrashed=1&since=4"]);
    }

    #[test]
    fn refresh_rereads_after_a_change_or_a_different_server() {
        let previous = Snapshot {
            version: 4,
            server_id: Some("one".into()),
        };
        let mut changed = page(5, 0, &[]);
        changed.body = br#"{"ABCD2345": 5}"#.to_vec();
        changed.headers.push(("Zotero-Server-ID".into(), "one".into()));
        let mut reread = page(5, 1, &["ABCD2345"]);
        reread.headers.push(("Zotero-Server-ID".into(), "one".into()));
        let mut fake = Fake::default().answer(changed).answer(reread);
        let Ok(Refresh::Loaded { result, snapshot }) = refresh(&mut fake, Library::User, Some(&previous)) else {
            panic!("expected a new scan");
        };
        assert_eq!(keys(&result), ["ABCD2345"]);
        assert_eq!(snapshot.version, 5);

        // Same version and no changed items, but another Zotero installation.
        let mut other = page(4, 0, &[]);
        other.body = b"{}".to_vec();
        other.headers.push(("Zotero-Server-ID".into(), "two".into()));
        let mut fake = Fake::default().answer(other).answer(page(4, 0, &[]));
        assert!(matches!(
            refresh(&mut fake, Library::User, Some(&previous)),
            Ok(Refresh::Loaded { .. })
        ));
    }
}
