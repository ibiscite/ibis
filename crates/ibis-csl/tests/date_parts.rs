use ibis_core::contract::{CitationForm, CitationRequest, ItemRef, LibraryId, LibraryObject, Record, Snapshot};
use ibis_csl::Formatter;
use serde_json::json;

#[test]
fn partial_dates_keep_their_valid_parts_in_citations() {
    let formatter = Formatter::new().unwrap();
    let id = ItemRef {
        library: LibraryId::digest(b"synthetic-dates"),
        key: "example".into(),
    };
    let request: CitationRequest = serde_json::from_value(json!({
        "items": [{"item": id}],
        "form": CitationForm::BibliographyOnly,
        "output": {"kind": "csl", "style": "apa"},
    }))
    .unwrap();
    for (date, expected) in [
        (json!({"date-parts": [[2000]]}), "2000"),
        (json!({"date-parts": [[" 2000 ", "05"]]}), "2000, May"),
        (json!({"date-parts": [[2000, 5, 3]]}), "2000, May 3"),
        (json!({"date-parts": [["2000", "", ""]]}), "2000"),
        (json!({"date-parts": [[2000, 5, ""]]}), "2000, May"),
        (json!({"date-parts": [[2000, "unknown", 3]]}), "2000"),
        (json!({"date-parts": [[2000, 5, "unknown"]]}), "2000, May"),
        (json!({"date-parts": [[2000, 5.5, 3]]}), "2000"),
        (json!({"date-parts": [[2000, 25, 3]]}), "2000"),
        (json!({"date-parts": [[2000, 2, 30]]}), "2000, February"),
        (json!({"date-parts": [[2000, 2, 29]]}), "2000, February 29"),
        (json!({"date-parts": [[2000, "", ""]], "season": 2}), "2000, Summer"),
        (json!({"date-parts": [[2000, 21, ""]]}), "2000, Spring"),
        (json!({"date-parts": [[2000, 13], [2000, 14]]}), "2000, Spring–Summer"),
        (json!({"date-parts": [[2000, "unknown"], [2002, ""]]}), "2000–2002"),
        (
            json!({"date-parts": [[2000, 5, "unknown"], [2002, 7, 3]]}),
            "2000–2002, May–July",
        ),
        (json!({"date-parts": [[2000], ["unknown", 7]]}), "2000"),
        (json!({"date-parts": [[2000, "", ""]], "raw": "1999"}), "2000"),
        (json!({"date-parts": [["unknown", 5, 3]], "raw": "1999"}), "1999"),
    ] {
        let csl = serde_json::from_value(json!({
            "type": "webpage",
            "title": "Invented item",
            "author": [{"family": "Taylor"}],
            "issued": date,
        }))
        .unwrap();
        let snapshot = Snapshot::new(vec![LibraryObject::Record(Record {
            added_at: None,
            id: id.clone(),
            csl,
            citation_keys: vec![],
            aliases: vec![],
            warnings: vec![],
            identifiers: vec![],
        })])
        .unwrap();
        let citation = formatter.format(&snapshot, &request).unwrap();
        assert_eq!(citation.text, format!("Taylor. ({expected}). Invented item."), "{date}");
    }
}
