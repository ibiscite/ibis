//! Sharing uses the catalogue and the same privacy boundary for every format.
#![allow(clippy::unwrap_used)]
use ibis_core::{contract::*, syntax};
use serde_json::{Value, json};

fn case(format: &str, fields: Value, key: Option<&str>) -> GoldenCase {
    let mut case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap();
    let mut record = match case.library.objects()[0].object.clone() {
        LibraryObject::Record(record) => record,
        _ => unreachable!(),
    };
    record.csl = serde_json::from_value(fields).unwrap();
    record.citation_keys = key
        .into_iter()
        .map(|key| CitationKey {
            value: key.into(),
            source: CitationKeySource::Bibtex,
        })
        .collect();
    case.library = Snapshot::new(vec![LibraryObject::Record(record)]).unwrap();
    case.request.output = syntax::CAPABILITIES
        .iter()
        .find(|caps| caps.id == format)
        .unwrap()
        .output();
    case
}

#[test]
fn every_record_export_removes_private_metadata_and_keeps_public_fields() {
    for format in ["csl-json", "bibtex", "doi-links"] {
        let case = case(
            format,
            json!({
                "id":"file:///home/PRIVATE/id", "type":"book", "title":"Invented atlas", "DOI":"10.1000/Invented",
                "note":"PRIVATE memo", "annote":"PRIVATE annotation", "file":"/home/PRIVATE/work.pdf",
                "attachments":[{"path":"/home/PRIVATE/work.pdf"}], "URL":"https://localhost/PRIVATE",
                "author":[{"family":"Doe", "x-private":"PRIVATE"}],
                "issued":{"date-parts":[[2026]],"x-private":"PRIVATE"},
            "original-date":{"date-parts":[[{"note":"PRIVATE"}]]},
            "publisher":{"note":"PRIVATE"},
                "abstract":"See file:///home/PRIVATE/work.pdf"
            }),
            Some("atlas"),
        );
        let result = syntax::cite(&case.library, &case.request).unwrap();
        assert!(!result.text.contains("PRIVATE"), "{format}: {}", result.text);
        assert!(!result.text.contains("localhost"));
        assert!(result.text.contains("10.1000/Invented"));
        if format != "doi-links" {
            assert!(result.text.contains("Invented atlas"));
            assert!(result.text.contains("Doe"));
        }
    }
}

#[test]
fn bibtex_quotes_hostile_field_text_and_refuses_missing_or_unsafe_keys() {
    let case = case(
        "bibtex",
        json!({"type":"book","title":r"} @misc{ \input{x} \write18{touch x} % # $ & _ ^ ~"}),
        Some("atlas"),
    );
    let text = syntax::cite(&case.library, &case.request).unwrap().text;
    assert!(
        text.contains(
            r"\textbraceright{} @misc\textbraceleft{} \textbackslash{}input\textbraceleft{}x\textbraceright{}"
        ),
        "{text}"
    );
    assert!(!text.contains(r"\input"));
    assert!(!text.contains(r"\write18"));
    for (key, error) in [
        (None, CitationError::MissingKey),
        (Some("bad\\input"), CitationError::InvalidKey),
        (Some("bad%key"), CitationError::InvalidKey),
    ] {
        let case = self::case("bibtex", json!({"type":"book","title":"Invented atlas"}), key);
        let result = syntax::cite(&case.library, &case.request).unwrap();
        assert!(result.text.is_empty());
        assert_eq!(result.outcomes, [ItemOutcome::Refused { item: 0, error }]);
    }
}

#[test]
fn doi_links_preserve_identifier_spelling_encode_url_delimiters_and_name_missing_dois() {
    let case = case(
        "doi-links",
        json!({"type":"book","DOI":"HTTPS://DOI.ORG/10.1000/Invented%2Fpart?#"}),
        None,
    );
    let result = syntax::cite(&case.library, &case.request).unwrap();
    assert_eq!(result.text, "https://doi.org/10.1000/Invented%252Fpart%3F%23");
    for doi in [
        Value::Null,
        json!(""),
        json!("file:///home/PRIVATE/work"),
        json!("10.1000/x\nhttps://localhost/PRIVATE"),
    ] {
        let case = self::case("doi-links", json!({"type":"book","DOI":doi}), Some("atlas"));
        let result = serde_json::to_value(syntax::cite(&case.library, &case.request).unwrap()).unwrap();
        assert_eq!(result["text"], "");
        assert_eq!(
            result["outcomes"],
            json!([{"outcome":"refused","item":0,"error":"missing-doi","message":{"id":"problem-missing-doi","args":{}},"recovery":{"action":"choose-output"},"focus":"format-picker"}])
        );
    }
}

#[test]
fn sharing_private_network_urls_never_exposes_a_link() {
    for url in [
        "https://localhost./PRIVATE",
        "https://[fd00::1]/PRIVATE",
        "https://[fe80::1]/PRIVATE",
        "https://[::ffff:127.0.0.1]/PRIVATE",
        "https://nas/PRIVATE",
        "https://127.1/PRIVATE",
        "https://2130706433/PRIVATE",
        "https://0x7f000001/PRIVATE",
        "https://%6cocalhost/PRIVATE",
        "http:////localhost/PRIVATE",
        "http:localhost/PRIVATE",
        "https://１２７.０.０.１/PRIVATE",
        "https://nas.ｌｏｃａｌ/PRIVATE",
    ] {
        for format in ["bibtex", "csl-json"] {
            let case = case(
                format,
                json!({"type":"book", "title":"Invented atlas", "URL":url}),
                Some("atlas"),
            );
            let result = syntax::cite(&case.library, &case.request).unwrap();
            assert!(!result.text.contains("PRIVATE"), "{format}: {}", result.text);
        }
    }
}

#[test]
fn csl_json_keeps_plain_source_tokens_but_never_url_shaped_ids() {
    for (id, expected) in [
        ("http://zotero.org/users/12345/items/ABCD1234", "good"),
        ("https://example.invalid/item", "good"),
        ("example.invalid/item", "good"),
        ("urn:private:item", "good"),
        ("item-7", "item-7"),
    ] {
        let case = case(
            "csl-json",
            json!({"id":id,"type":"book","title":"Invented atlas"}),
            Some("good"),
        );
        let result = syntax::cite(&case.library, &case.request).unwrap();
        let records: Value = serde_json::from_str(&result.text).unwrap();
        assert_eq!(records[0]["id"], expected, "{id}");
        assert_eq!(records[0]["citation-key"], "good");
    }
}

#[test]
fn record_exports_preserve_citation_keys_that_resemble_device_paths() {
    for format in ["csl-json", "bibtex"] {
        let case = case(format, json!({"type":"book","title":"Invented atlas"}), Some("a:/_"));
        let result = syntax::cite(&case.library, &case.request).unwrap();
        assert!(result.copy_content(1).is_some(), "{format}: {:?}", result.outcomes);
        if format == "csl-json" {
            let records: Value = serde_json::from_str(&result.text).unwrap();
            assert_eq!(records[0]["citation-key"], "a:/_");
        } else {
            assert!(result.text.starts_with("@book{a:/_,"), "{}", result.text);
        }
    }
}

#[test]
fn record_sharing_removes_device_paths_after_value_delimiters() {
    for value in [
        "path=/Users/invented/secret.pdf",
        "path:/Users/invented/secret.pdf",
        "prefix=/home/invented/secret.pdf",
        "path=~/secret.pdf",
        r"path=C:\invented\secret.pdf",
        r"path=\\invented\secret.pdf",
        "path=file:///Users/invented/secret.pdf",
    ] {
        let case = case(
            "csl-json",
            json!({"type":"book","title":"Invented atlas","archive_location":value}),
            Some("good"),
        );
        let records: Value = serde_json::from_str(&syntax::cite(&case.library, &case.request).unwrap().text).unwrap();
        assert!(records[0].get("archive_location").is_none(), "{value}: {records}");
        assert_eq!(records[0]["title"], "Invented atlas");
    }
}

#[test]
fn an_article_writes_its_issue_as_one_number_even_with_an_article_number() {
    let case = case(
        "bibtex",
        json!({"type":"article-journal","title":"Invented atlas","issue":"2","number":"e1234"}),
        Some("good"),
    );
    let text = syntax::cite(&case.library, &case.request).unwrap().text;
    assert_eq!(
        text.lines()
            .filter(|line| line.trim_start().starts_with("number ="))
            .collect::<Vec<_>>(),
        ["  number = {2},"]
    );
}

#[test]
fn malformed_dates_are_omitted_instead_of_writing_empty_or_partial_dates() {
    for parts in [json!([[2020.5]]), json!([[2020, 3.5]]), json!([[]])] {
        let case = case(
            "bibtex",
            json!({"type":"book","title":"Invented atlas","issued":{"date-parts":parts}}),
            Some("good"),
        );
        let text = syntax::cite(&case.library, &case.request).unwrap().text;
        assert!(!text.contains("  date ="), "{text}");
        assert!(text.contains("Invented atlas"));
    }
}

#[test]
fn verbatim_identifiers_still_cannot_break_fields_or_activate_tex() {
    let case = case(
        "bibtex",
        json!({"type":"book","ISBN":"} @misc{ \\input{x}\nnext"}),
        Some("good"),
    );
    let text = syntax::cite(&case.library, &case.request).unwrap().text;
    assert!(!text.contains("} @misc{"), "{text}");
    assert!(!text.contains("\\input"), "{text}");
    assert!(!text.contains("\nnext"), "{text}");
    assert!(text.contains("input"));
}
