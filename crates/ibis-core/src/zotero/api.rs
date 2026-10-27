//! Reading a library through Zotero's local API while Zotero runs.
//!
//! The rules of the conversation live here and are tested against a fake
//! server; sending the requests is left to a [`Transport`], so this crate
//! never touches the network. Zotero serves the API on
//! `http://127.0.0.1:23119/api/` once "Allow other applications on this
//! computer to communicate with Zotero" is on.
//!
//! A library is read page by page, oldest item first, each page starting
//! where the items read so far end. Every page must carry
//! `Last-Modified-Version` and `Total-Results`, the count of the whole
//! listing, and all pages of one scan must agree on both; a disagreement
//! means the library changed and the scan starts over. So does an item key
//! seen twice in one scan: Zotero 7.0 reports the synced library version, so
//! a local edit mid-scan shows only as a listing that moved. More items than
//! the declared total, or a page with items when the total is zero, are an
//! inconsistent answer and fail closed, so a partial or mismatched library
//! is never published as complete. A short page inside a split export also
//! fails closed if later ranges would leave unread items between the pages.

use std::collections::HashSet;
use std::time::{Duration, Instant};

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
/// Aggregate response bodies across attempts, including failed exports and
/// key lookups. This bounds serialized input/work, not parsed heap size.
const MAX_SCAN_BYTES: usize = 512 * 1024 * 1024;
/// Cooperative limit: a synchronous transport or mapping call already in
/// progress cannot be interrupted, but a late result is never published.
const MAX_SCAN_ELAPSED: Duration = Duration::from_secs(10 * 60);
/// The listing order of every page: oldest first. Editing an item never
/// moves it, unlike Zotero's default `dateModified` order, and Zotero 7.0
/// changes neither the version nor the total on a local edit.
const ORDER: &str = "sort=dateAdded&direction=asc";

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
    #[error("reading Zotero exceeded Ibis's size limit")]
    TooLarge,
}

/// The version and server identity observed during one coherent scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// The library version (`Last-Modified-Version`) the items belong to.
    pub version: u64,
    /// Zotero's server ID, when it sends one.
    pub server_id: Option<String>,
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

/// Whether Zotero is there to read from, found without reading any library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// Zotero is open and lets other applications read it.
    Available,
    /// Zotero is open, but "Allow other applications on this computer to
    /// communicate with Zotero" is off.
    AccessOff,
    /// Nothing answered as Zotero.
    Absent,
}

/// Ask Zotero's local API root, which answers with fixed text and reads no
/// library.
pub fn presence(transport: &mut impl Transport) -> Presence {
    match transport.get("") {
        Ok(response) if response.status == 200 => Presence::Available,
        Ok(response) if response.status == 403 => Presence::AccessOff,
        _ => Presence::Absent,
    }
}

/// Read every item of the library at one version. If the library changes
/// between pages, or its pages disagree on the item count, the scan starts
/// again, at most a few times. All attempts share a 512 MiB response-body
/// budget and a ten-minute cooperative elapsed limit. These do not bound
/// parsed heap size or interrupt a transport/mapping call already underway.
pub fn scan(transport: &mut impl Transport, library: Library) -> Result<(LoadResult, Snapshot), ZoteroError> {
    scan_with_budget(transport, library, &mut ScanBudget::default())
}

pub(super) struct ScanBudget {
    // None means a response exceeded the quota; later libraries must not
    // restart I/O. Some(0) still accepts an exact fit or an empty body.
    bytes_left: Option<usize>,
    started: Instant,
    elapsed: Duration,
}

impl Default for ScanBudget {
    fn default() -> Self {
        Self::new(MAX_SCAN_BYTES, MAX_SCAN_ELAPSED)
    }
}

impl ScanBudget {
    pub(super) fn new(bytes_left: usize, elapsed: Duration) -> Self {
        Self {
            bytes_left: Some(bytes_left),
            started: Instant::now(),
            elapsed,
        }
    }

    fn check(&self, now: Instant) -> Result<(), ZoteroError> {
        if now.saturating_duration_since(self.started) >= self.elapsed {
            return Err(TransportError::TimedOut.into());
        }
        Ok(())
    }

    fn get(&mut self, transport: &mut impl Transport, path: &str) -> Result<Response, ZoteroError> {
        self.check(Instant::now())?;
        self.bytes_left.ok_or(ZoteroError::TooLarge)?;
        let response = transport.get(path)?;
        self.check(Instant::now())?;
        self.bytes_left = self.bytes_left.and_then(|left| left.checked_sub(response.body.len()));
        self.bytes_left.ok_or(ZoteroError::TooLarge)?;
        Ok(response)
    }
}

pub(super) fn scan_with_budget(
    transport: &mut impl Transport,
    library: Library,
    budget: &mut ScanBudget,
) -> Result<(LoadResult, Snapshot), ZoteroError> {
    for _ in 0..MAX_ATTEMPTS {
        if let Some(done) = scan_once(transport, library, budget)? {
            return Ok(done);
        }
    }
    Err(ZoteroError::Changing)
}

fn scan_once(
    transport: &mut impl Transport,
    library: Library,
    budget: &mut ScanBudget,
) -> Result<Option<(LoadResult, Snapshot)>, ZoteroError> {
    let mut items: Vec<Value> = Vec::new();
    let mut first: Option<Snapshot> = None;
    let mut total: Option<usize> = None;
    let mut skipped: Vec<LoadWarning> = Vec::new();
    // Every key read so far. A key that comes back means the listing moved
    // under the scan, so another item may have been passed over.
    let mut seen: HashSet<String> = HashSet::new();
    // Ranges still to read, last first. A page Zotero fails to export is
    // split in halves until the one item it cannot export is found.
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for _ in 0..MAX_REQUESTS {
        // The next page starts right after the items seen so far, read or
        // skipped, so nothing is passed over when a page comes back short.
        let (start, limit) = ranges.pop().unwrap_or_else(|| (items.len() + skipped.len(), PAGE_SIZE));
        let path = format!(
            "{}?format=json&include=data,csljson&{ORDER}&limit={limit}&start={start}",
            top_items_path(library)
        );
        let response = budget.get(transport, &path)?;
        if response.status == 500 {
            if limit > 1 {
                ranges.push((start + limit / 2, limit - limit / 2));
                ranges.push((start, limit / 2));
            } else {
                // A failed export carries no library version, so the skipped
                // item is tied to one through the key lookup's answer.
                let (warning, keys) = unexported(transport, library, start, budget)?;
                if !consistent(&keys, &mut first, &mut total)? {
                    return Ok(None);
                }
                if let Some(key) = &warning.key
                    && !seen.insert(key.clone())
                {
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
        for item in &page_items {
            if let Some(key) = item.get("key").and_then(Value::as_str)
                && !seen.insert(key.to_string())
            {
                return Ok(None);
            }
        }
        items.extend(page_items);
        if items.len() + skipped.len() > count {
            // More items than Zotero declared: the count is wrong or the
            // listing moved under the scan, so nothing it says can be trusted.
            return Err(ZoteroError::Incomplete);
        }
        if !ranges.is_empty() {
            // A queued sibling starts after the whole requested range, not
            // after a short answer. Never step over an unread gap.
            if got < limit && items.len() + skipped.len() < count {
                return Err(ZoteroError::Incomplete);
            }
            continue;
        }
        if items.len() + skipped.len() == count {
            if items.is_empty() && !skipped.is_empty() {
                // Zotero exported nothing at all: that is a broken Zotero,
                // not an empty library.
                return Err(ZoteroError::Status(500));
            }
            let snapshot = first.ok_or(ZoteroError::Incomplete)?;
            budget.check(Instant::now())?;
            let mut result = load_items(library, &items);
            budget.check(Instant::now())?;
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
    budget: &mut ScanBudget,
) -> Result<(LoadWarning, Response), ZoteroError> {
    let response = checked(
        budget.get(
            transport,
            &format!(
                "{}?format=keys&{ORDER}&limit=1&start={position}",
                top_items_path(library)
            ),
        )?,
        library,
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

    use super::*;
    use crate::zotero::test_support::{items, number};

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

    fn page(version: u64, total: usize, keys: &[&str]) -> Response {
        let items = items(keys);
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

    fn limited(
        transport: &mut impl Transport,
        bytes: usize,
        elapsed: Duration,
    ) -> Result<(LoadResult, Snapshot), ZoteroError> {
        scan_with_budget(transport, Library::User, &mut ScanBudget::new(bytes, elapsed))
    }

    #[test]
    fn cumulative_scan_bytes_accept_the_exact_boundary_and_refuse_one_more() {
        let first = page(1, 2, &["A"]);
        let last = page(1, 2, &["B"]);
        let bytes = first.body.len() + last.body.len();
        for (limit, succeeds) in [(bytes, true), (bytes - 1, false)] {
            let mut fake = Fake::default().answer(first.clone()).answer(last.clone());
            let result = limited(&mut fake, limit, Duration::from_secs(60));
            if succeeds {
                assert_eq!(keys(&result.unwrap_or_else(|e| panic!("{e}")).0), ["A", "B"]);
            } else {
                assert_eq!(result, Err(ZoteroError::TooLarge));
            }
            assert_eq!(fake.asked.len(), 2);
        }
        let mut malformed = page(1, 0, &[]);
        malformed.body = b"not JSON".to_vec();
        let mut fake = Fake::default().answer(malformed);
        assert_eq!(
            limited(&mut fake, 7, Duration::from_secs(60)),
            Err(ZoteroError::TooLarge)
        );
    }

    #[test]
    fn consistency_restarts_do_not_reset_the_scan_byte_budget() {
        let first = page(1, 2, &["A"]);
        let changed = page(2, 2, &["B"]);
        let retry = page(2, 2, &["A"]);
        let bytes = first.body.len() + changed.body.len() + retry.body.len() - 1;
        let mut fake = Fake::default()
            .answer(first)
            .answer(changed.clone())
            .answer(retry)
            .answer(changed);
        assert_eq!(
            limited(&mut fake, bytes, Duration::from_secs(60)),
            Err(ZoteroError::TooLarge)
        );
        assert_eq!(fake.asked.len(), 3);
    }

    #[test]
    fn failed_exports_and_auxiliary_key_lookups_consume_the_same_budget() {
        for (limit, requests) in [(0, 1), (7, 8)] {
            let mut fake = Fake::default();
            for _ in 0..7 {
                fake = fake.answer(Response {
                    status: 500,
                    body: vec![b'x'],
                    ..Response::default()
                });
            }
            let mut lookup = page(1, 1, &[]);
            lookup.body = b"K".to_vec();
            fake = fake.answer(lookup);
            assert_eq!(
                limited(&mut fake, limit, Duration::from_secs(60)),
                Err(ZoteroError::TooLarge)
            );
            assert_eq!(fake.asked.len(), requests);
            if requests == 8 {
                assert!(fake.asked.last().is_some_and(|path| path.contains("format=keys")));
            }
        }
    }

    #[test]
    fn expired_elapsed_budget_refuses_before_making_a_request() {
        let mut fake = Fake::default().answer(page(1, 0, &[]));
        assert_eq!(
            limited(&mut fake, 100, Duration::ZERO),
            Err(TransportError::TimedOut.into())
        );
        assert!(fake.asked.is_empty());
    }

    #[test]
    fn exact_fit_allows_empty_bodies_but_refusal_prevents_further_io() {
        let body = Response {
            body: vec![b'x'],
            ..Response::default()
        };
        let mut fake = Fake::default()
            .answer(body.clone())
            .answer(Response::default())
            .answer(body)
            .answer(Response::default());
        let mut budget = ScanBudget::new(1, Duration::from_secs(60));
        assert!(budget.get(&mut fake, "").is_ok());
        assert!(budget.get(&mut fake, "").is_ok());
        assert_eq!(budget.get(&mut fake, ""), Err(ZoteroError::TooLarge));
        assert_eq!(budget.get(&mut fake, ""), Err(ZoteroError::TooLarge));
        assert_eq!(fake.asked.len(), 3);
    }

    #[test]
    fn elapsed_budget_refuses_the_boundary_and_a_late_success() {
        let budget = ScanBudget::new(100, Duration::from_secs(60));
        assert!(budget.check(budget.started + Duration::from_secs(59)).is_ok());
        assert_eq!(
            budget.check(budget.started + Duration::from_secs(60)),
            Err(TransportError::TimedOut.into())
        );
        struct Late;
        impl Transport for Late {
            fn get(&mut self, _: &str) -> Result<Response, TransportError> {
                std::thread::sleep(Duration::from_millis(20));
                Ok(page(1, 0, &[]))
            }
        }
        assert_eq!(
            limited(&mut Late, 100, Duration::from_millis(10)),
            Err(TransportError::TimedOut.into())
        );
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
            let (start, limit) = (number(path, "start="), number(path, "limit="));
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
    fn presence_is_read_from_the_api_root_without_reading_a_library() {
        let answer = |status| Response {
            status,
            ..Response::default()
        };
        let mut open = Fake::default().answer(answer(200));
        assert_eq!(presence(&mut open), Presence::Available);
        assert_eq!(open.asked, [""]);
        assert_eq!(presence(&mut Fake::default().answer(answer(403))), Presence::AccessOff);
        assert_eq!(presence(&mut Fake::default().answer(answer(404))), Presence::Absent);
        assert_eq!(
            presence(&mut Fake::default().fail(TransportError::Unreachable)),
            Presence::Absent
        );
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
        // The key lookup must list items in the same order as the pages.
        assert!(
            zotero.asked.iter().all(|path| path.contains(ORDER)),
            "{:?}",
            zotero.asked
        );
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
                "groups/7/items/top?format=json&include=data,csljson&sort=dateAdded&direction=asc&limit=100&start=0",
                "groups/7/items/top?format=json&include=data,csljson&sort=dateAdded&direction=asc&limit=100&start=100",
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
    fn short_sequential_pages_still_read_every_item() {
        let mut fake = Fake::default()
            .answer(page(3, 3, &["A"]))
            .answer(page(3, 3, &["B", "C"]));
        let (result, _) = scan(&mut fake, Library::User).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(keys(&result), ["A", "B", "C"]);
        assert!(result.warnings.is_empty());
        assert_eq!(fake.asked.len(), 2);
        assert!(fake.asked[1].ends_with("start=1"));
    }

    #[test]
    fn a_short_split_page_at_the_total_still_reads_the_complete_library() {
        let all = many("A", 20);
        let all: Vec<&str> = all.iter().map(String::as_str).collect();
        let mut fake = Fake::default()
            .answer(Response {
                status: 500,
                ..Response::default()
            })
            .answer(page(3, 20, &all))
            .answer(page(3, 20, &[]));
        let (result, _) = scan(&mut fake, Library::User).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(keys(&result), all);
        assert!(result.warnings.is_empty());
        assert_eq!(fake.asked.len(), 3);
        assert!(fake.asked[1].ends_with("limit=50&start=0"));
        assert!(fake.asked[2].ends_with("limit=50&start=50"));
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
    fn pages_that_disagree_on_the_total_at_one_version_restart_then_give_up() {
        // Zotero's count is for the whole listing, so a different count on
        // the next page means the library moved even though the version did
        // not. A scan that never sees agreeing pages is reported, not
        // published as a 101-item library.
        let first = many("A", PAGE_SIZE);
        let first: Vec<&str> = first.iter().map(String::as_str).collect();
        let mut fake = Fake::default();
        for _ in 0..MAX_ATTEMPTS {
            fake = fake.answer(page(8, 200, &first)).answer(page(8, PAGE_SIZE + 1, &["B"]));
        }
        assert_eq!(scan(&mut fake, Library::User), Err(ZoteroError::Changing));
        assert_eq!(fake.asked.len(), 2 * MAX_ATTEMPTS);
        assert!(fake.asked.iter().step_by(2).all(|path| path.ends_with("start=0")));
    }

    /// A Zotero 7.0 that ignores the requested order and, like its default
    /// `dateModified` order, moves an item edited mid-scan to the front
    /// without changing the library version or the total.
    struct Reordering {
        keys: Vec<String>,
        pages: usize,
        /// Pages after which an item is edited.
        edits_after: Vec<usize>,
    }

    impl Transport for Reordering {
        fn get(&mut self, path: &str) -> Result<Response, TransportError> {
            let (start, limit) = (number(path, "start="), number(path, "limit="));
            if self.edits_after.contains(&self.pages) {
                let edited = self.keys.remove(150);
                self.keys.insert(0, edited);
            }
            self.pages += 1;
            let end = (start + limit).min(self.keys.len());
            let keys: Vec<&str> = self.keys[start.min(end)..end].iter().map(String::as_str).collect();
            Ok(page(0, self.keys.len(), &keys))
        }
    }

    #[test]
    fn an_edit_between_pages_that_reorders_items_restarts_the_scan() {
        let mut zotero = Reordering {
            keys: many("K", 250),
            pages: 0,
            edits_after: vec![1],
        };
        let (result, _) = scan(&mut zotero, Library::User).unwrap_or_else(|e| panic!("{e}"));
        let mut got = keys(&result);
        got.sort_unstable();
        got.dedup();
        assert_eq!(
            got.len(),
            250,
            "an item is missing from a library published as complete"
        );
        assert!(result.warnings.iter().all(|w| w.kind != WarningKind::DuplicateKey));
    }

    #[test]
    fn a_listing_that_keeps_reordering_is_reported_not_published() {
        let mut zotero = Reordering {
            keys: many("K", 250),
            pages: 0,
            edits_after: (0..100).collect(),
        };
        assert_eq!(scan(&mut zotero, Library::User).err(), Some(ZoteroError::Changing));
    }

    #[test]
    fn a_library_of_exactly_one_page_is_read_in_one_request() {
        let keys = many("A", PAGE_SIZE);
        let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
        let mut fake = Fake::default().answer(page(2, PAGE_SIZE, &keys));
        let (result, _) = scan(&mut fake, Library::User).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(result.references.len(), PAGE_SIZE);
        assert_eq!(fake.asked.len(), 1);
    }

    #[test]
    fn a_total_that_cannot_be_counted_is_incomplete() {
        let with_total = |total: &str| {
            let mut answer = page(1, 0, &[]);
            answer.headers.retain(|(n, _)| n != "Total-Results");
            answer.headers.push(("Total-Results".into(), total.into()));
            answer
        };
        for total in ["", "many", "-1", "1.5", "99999999999999999999999999", "1 2"] {
            assert_eq!(
                scan(&mut Fake::default().answer(with_total(total)), Library::User),
                Err(ZoteroError::Incomplete),
                "total {total:?}"
            );
        }
        // Surrounding whitespace is what a header may carry; the count stands.
        let mut fake = Fake::default().answer(with_total("  0 "));
        assert!(scan(&mut fake, Library::User).is_ok());
        let mut missing = page(1, 0, &[]);
        missing.headers.retain(|(n, _)| n != "Total-Results");
        assert_eq!(
            scan(&mut Fake::default().answer(missing), Library::User),
            Err(ZoteroError::Incomplete)
        );
    }
}
