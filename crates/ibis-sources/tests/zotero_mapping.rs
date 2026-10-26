//! Invented data only. No Zotero process or library is used.
#[path = "../src/zotero/mod.rs"]
mod zotero;

use ibis_core::contract::LibraryId;
use serde_json::json;

#[test]
fn neutral_book_maps_without_zotero_export() {
    let item = serde_json::from_value(json!({
        "key": "SYN00001", "itemType": "book", "title": "Invented book",
        "creators": [{"creatorType": "author", "firstName": "Example", "lastName": "Synthetic"}],
        "date": "2026-10-25", "citationKey": "Synthetic2026"
    })).unwrap();
    let mapped = zotero::map_item(LibraryId::zotero(b"invented library"), &item).unwrap();
    assert_eq!(mapped.record.csl["type"], "book");
    assert_eq!(mapped.record.csl["title"], "Invented book");
    assert_eq!(mapped.record.csl["author"], json!([{"family": "Synthetic", "given": "Example"}]));
    assert_eq!(mapped.record.csl["issued"], json!({"date-parts": [[2026, 10, 25]]}));
    assert_eq!(mapped.record.citation_key().unwrap(), Some("Synthetic2026"));
}
