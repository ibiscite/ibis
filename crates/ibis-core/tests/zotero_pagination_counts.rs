//! Synthetic regressions for inconsistent Zotero pagination (#263).

use std::collections::VecDeque;

use ibis_core::zotero::Library;
use ibis_core::zotero::api::{PAGE_SIZE, Response, Transport, TransportError, ZoteroError, scan};
use serde_json::{Value, json};

struct Fake(VecDeque<Response>);

impl Transport for Fake {
    fn get(&mut self, _path: &str) -> Result<Response, TransportError> {
        self.0.pop_front().ok_or(TransportError::Unreachable)
    }
}

fn item(number: usize) -> Value {
    json!({
        "key": format!("SYN{number:05}"),
        "data": {"itemType": "book"},
        "csljson": {"type": "book", "title": "Synthetic pagination record"}
    })
}

fn page(total: usize, items: &[Value]) -> Response {
    Response {
        status: 200,
        headers: vec![
            ("Zotero-API-Version".into(), "3".into()),
            ("Last-Modified-Version".into(), "8".into()),
            ("Total-Results".into(), total.to_string()),
            ("Zotero-Server-ID".into(), "synthetic-server".into()),
        ],
        body: serde_json::to_vec(items).unwrap_or_else(|error| panic!("synthetic items failed to serialize: {error}")),
    }
}

#[test]
fn a_changed_total_at_one_version_cannot_publish_a_partial_snapshot() {
    let first: Vec<_> = (0..PAGE_SIZE).map(item).collect();
    let mut fake = Fake([page(200, &first), page(101, &[item(100)])].into());
    let result = scan(&mut fake, Library::User);
    assert!(
        result.is_err(),
        "inconsistent totals must not publish {} synthetic records",
        result.as_ref().map_or(0, |(loaded, _)| loaded.references.len())
    );
}

#[test]
fn a_page_exceeding_its_declared_total_is_incomplete() {
    let mut fake = Fake([page(0, &[item(0)])].into());
    assert_eq!(scan(&mut fake, Library::User), Err(ZoteroError::Incomplete));
}

#[test]
fn a_last_page_with_more_items_than_the_declared_total_is_incomplete() {
    let first: Vec<_> = (0..PAGE_SIZE).map(item).collect();
    let mut fake = Fake(
        [
            page(PAGE_SIZE + 1, &first),
            page(PAGE_SIZE + 1, &[item(100), item(101)]),
        ]
        .into(),
    );
    assert_eq!(scan(&mut fake, Library::User), Err(ZoteroError::Incomplete));
}

#[test]
fn a_short_split_page_never_publishes_a_snapshot_with_missing_items() {
    // The initial export fails; narrowing it returns a short first half.
    // A raw item count is no longer the next unread offset after the
    // queued second half has been read.
    #[derive(Default)]
    struct ShortSplit {
        first: bool,
        asked: Vec<(usize, usize)>,
    }
    impl Transport for ShortSplit {
        fn get(&mut self, path: &str) -> Result<Response, TransportError> {
            let number = |name: &str| {
                path.split(['?', '&'])
                    .find_map(|part| part.strip_prefix(name))
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or_else(|| panic!("missing synthetic query parameter {name}"))
            };
            let (start, limit) = (number("start="), number("limit="));
            self.asked.push((start, limit));
            if !std::mem::replace(&mut self.first, true) {
                return Ok(Response {
                    status: 500,
                    ..Response::default()
                });
            }
            let end = if (start, limit) == (0, 50) {
                20
            } else {
                (start + limit).min(120)
            };
            let items: Vec<_> = (start.min(end)..end).map(item).collect();
            Ok(page(120, &items))
        }
    }
    let mut fake = ShortSplit::default();
    assert_eq!(scan(&mut fake, Library::User), Err(ZoteroError::Incomplete));
    assert_eq!(fake.asked, [(0, 100), (0, 50)]);
}
