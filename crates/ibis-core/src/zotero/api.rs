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
use crate::model::LoadResult;

/// Items requested per page; the most Zotero returns at once.
pub const PAGE_SIZE: usize = 100;
/// Pages read in one scan before giving up: one million items.
const MAX_PAGES: usize = 10_000;
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
    for page in 0..MAX_PAGES {
        let start = page * PAGE_SIZE;
        let response = request(
            transport,
            library,
            &format!(
                "{}?format=json&include=data,csljson&limit={PAGE_SIZE}&start={start}",
                items_path(library)
            ),
        )?;
        let seen = snapshot(&response)?;
        match &first {
            None => first = Some(seen),
            Some(first) if *first != seen => return Ok(None),
            Some(_) => {}
        }
        let total: usize = response
            .header("Total-Results")
            .and_then(|t| t.parse().ok())
            .ok_or(ZoteroError::Incomplete)?;
        let Value::Array(page_items) = serde_json::from_slice(&response.body).map_err(|_| ZoteroError::Incomplete)?
        else {
            return Err(ZoteroError::Incomplete);
        };
        if page_items.len() > PAGE_SIZE {
            return Err(ZoteroError::Incomplete);
        }
        let got = page_items.len();
        items.extend(page_items);
        if items.len() >= total {
            let snapshot = first.ok_or(ZoteroError::Incomplete)?;
            return Ok(Some((load_items(library, &items), snapshot)));
        }
        if got == 0 {
            // The library shrank between pages without a new version, or
            // the server miscounts: never return a partial library.
            return Err(ZoteroError::Incomplete);
        }
    }
    Err(ZoteroError::TooLarge)
}

fn request(transport: &mut impl Transport, library: Library, path: &str) -> Result<Response, ZoteroError> {
    let response = transport.get(path)?;
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
                "groups/7/items?format=json&include=data,csljson&limit=100&start=0",
                "groups/7/items?format=json&include=data,csljson&limit=100&start=100",
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
