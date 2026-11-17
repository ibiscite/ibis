//! Sharing uses the catalogue and the same privacy boundary for every format.
#![cfg(test)]
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

const ENCODED_PATHS: &[&str] = &[
    "&#x66;ile:&#x2f;&#x2f;&#x2f;home/invented/secret.pdf",
    "&#67;&#58;&#92;invented&#92;secret.pdf",
    "&#102;ile:&#47;&#x2f;&#47;home/invented/secret.pdf",
    "%2FUsers%2Finvented%2Fsecret.pdf",
    "%252fUsers%252finvented%252fsecret.pdf",
    "&#37;2FUsers&#37;2Finvented&#37;2Fsecret.pdf",
    "%26%2347%3BUsers%26%2347%3Binvented%26%2347%3Bsecret.pdf",
    "&amp;#47;Users/invented/secret.pdf",
    "%43%3A%5Cinvented%5Csecret.pdf",
    "<i>%2FUsers%2F</i><b>invented%2Fsecret.pdf</b>",
    "<span class=\"nocase\" data-path=\"%2FUsers%2Finvented%2Fsecret.pdf\">secret</span>",
];

fn encoded_paths_are_private(format: &str) {
    let mut leaks = Vec::new();
    for path in ENCODED_PATHS {
        let case = case(
            format,
            json!({
                "type":"book", "title":path, "publisher":"Invented Press", "DOI":format!("10.1000/x;{path}"),
                "author":[{"family":path}], "issued":{"literal":path}, "note":format!("archive: {path}")
            }),
            Some("atlas"),
        );
        let result = syntax::cite(&case.library, &case.request).unwrap();
        if format == "doi-links" {
            if !result.text.is_empty()
                || result.outcomes
                    != [ItemOutcome::Refused {
                        item: 0,
                        error: CitationError::MissingDoi,
                    }]
            {
                leaks.push(result.text);
            }
        } else {
            assert!(result.text.contains("Invented Press"), "{}", result.text);
            if result.text.contains("secret") {
                leaks.push(result.text);
            }
        }
    }
    assert!(leaks.is_empty(), "{format}: {leaks:?}");
}

#[test]
fn exports_drop_entity_encoded_device_paths() {
    for format in ["bibtex", "csl-json", "doi-links"] {
        encoded_paths_are_private(format);
    }
}

#[test]
fn exports_check_dois_after_decoding_and_removing_resolver_wrappers() {
    for doi in [
        "https://doi.org/&#47;home/invented/secret.pdf",
        "&#100;oi:&#47;home/invented/secret.pdf",
        "https:&#47;&#47;doi.org/&#47;home/invented/secret.pdf",
    ] {
        for format in ["bibtex", "csl-json", "doi-links"] {
            let case = case(
                format,
                json!({"type":"book", "title":"Invented atlas", "DOI":doi}),
                Some("atlas"),
            );
            let result = syntax::cite(&case.library, &case.request).unwrap();
            assert!(!result.text.contains("secret"), "{format}: {}", result.text);
            if format == "doi-links" {
                assert_eq!(
                    result.outcomes,
                    [ItemOutcome::Refused {
                        item: 0,
                        error: CitationError::MissingDoi
                    }]
                );
            }
        }
    }
}

#[test]
fn doi_shaped_paths_are_refused_by_the_identity_constructors_and_exports() {
    use ibis_core::citable::{DoiLink, PublicUrl};
    for doi in [
        "10.1000//Users/invented/secret.pdf",
        "10.1000/%2FUsers/invented/secret.pdf",
        "10.1000/a//secret.pdf",
        "10.1000/Users/invented/secret.pdf/",
        "10.1000/C:/invented/secret.pdf",
        "10.1000/~/invented/secret.pdf",
    ] {
        assert!(DoiLink::new(doi).is_none(), "{doi}");
        assert!(PublicUrl::new(&format!("https://doi.org/{doi}")).is_none(), "{doi}");
        let case = case("doi-links", json!({"type":"book", "DOI":doi}), Some("atlas"));
        let result = syntax::cite(&case.library, &case.request).unwrap();
        assert_eq!(result.text, "");
        assert_eq!(
            result.outcomes,
            [ItemOutcome::Refused {
                item: 0,
                error: CitationError::MissingDoi
            }]
        );
    }
    for url in [
        "https://doi.org/&#47;Users/invented/secret.pdf",
        "https://dx.doi.org/%252FUsers/invented/secret.pdf",
    ] {
        assert!(PublicUrl::new(url).is_none(), "{url}");
    }
}

#[test]
fn decoding_that_exceeds_the_privacy_bound_is_refused() {
    let mut path = "%2FUsers%2Finvented%2Fsecret.pdf".to_owned();
    for _ in 0..32 {
        path = path.replace('%', "%25");
    }
    let case = case("csl-json", json!({"type":"book", "title":path}), Some("atlas"));
    let result = syntax::cite(&case.library, &case.request).unwrap();
    assert!(!result.text.contains("secret"), "{}", result.text);
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
                "issued":{"date-parts":[["2026"]],"x-private":"PRIVATE"},
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
            assert!(result.text.contains("2026"));
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
    let mut case = case(
        "doi-links",
        json!({"type":"book","DOI":"HTTPS://DOI.ORG/10.1000/Invented%2Fpart?#"}),
        None,
    );
    let result = syntax::cite(&case.library, &case.request).unwrap();
    assert_eq!(result.text, "https://doi.org/10.1000/Invented%252Fpart%3F%23");
    case.request.items.push(case.request.items[0].clone());
    let repeated = syntax::cite(&case.library, &case.request).unwrap();
    assert_eq!(repeated.text, result.text);
    assert_eq!(
        repeated.outcomes,
        [ItemOutcome::Rendered { item: 0 }, ItemOutcome::Rendered { item: 1 }]
    );
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
        "https://invented.&#108;ocal/PRIVATE",
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
fn record_sharing_removes_device_paths_after_value_delimiters() {
    for value in [
        "path=/Users/invented/secret.pdf",
        "path:/Users/invented/secret.pdf",
        "prefix=/home/invented/secret.pdf",
        "path=~/secret.pdf",
        r"path=C:\invented\secret.pdf",
        r"path=\\invented\secret.pdf",
        "path=file:///Users/invented/secret.pdf",
        "path;/Users/invented/secret.pdf",
        "&lt;/i&gt;/Users/invented/secret.pdf",
        "&lt;/home/invented/secret.pdf&gt;",
        "<i>/home/</i>invented/secret.pdf",
        "<span class=\"nocase\">file:</span>///home/invented/secret.pdf",
        "&lt;span class=&quot;nocase&quot; title=&quot;/home/invented/secret.pdf&quot;&gt;DNA&lt;/span&gt;",
        "Invented <span class=\"nocase\" data-private=\"/home/invented/secret.pdf\">DNA</span>",
        "Invented <span class=\"nocase\" data-private=\"C&#x202e;:&#92;invented&#92;secret.pdf\">DNA</span>",
        "C\u{2060}:\\invented\\secret.pdf",
    ] {
        let case = case(
            "csl-json",
            json!({"type":"book","title":"Invented &lt;i&gt;atlas&lt;/i&gt;","archive_location":value}),
            Some("good"),
        );
        let records: Value = serde_json::from_str(&syntax::cite(&case.library, &case.request).unwrap().text).unwrap();
        assert!(records[0].get("archive_location").is_none(), "{value}: {records}");
        assert_eq!(records[0]["title"], "Invented &lt;i&gt;atlas&lt;/i&gt;");
    }
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
