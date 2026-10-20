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
#[ignore = "#263: an unchanged-version count drop silently publishes a partial library"]
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
#[ignore = "#263: a nonempty page with zero total is accepted as a complete snapshot"]
fn a_page_exceeding_its_declared_total_is_incomplete() {
    let mut fake = Fake([page(0, &[item(0)])].into());
    assert_eq!(scan(&mut fake, Library::User), Err(ZoteroError::Incomplete));
}
