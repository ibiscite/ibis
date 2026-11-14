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

const ENCODED_PATHS: [&str; 3] = [
    "&#x66;ile:&#x2f;&#x2f;&#x2f;home/invented/secret.pdf",
    "&#67;&#58;&#92;invented&#92;secret.pdf",
    "&#102;ile:&#47;&#x2f;&#47;home/invented/secret.pdf",
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
fn bibtex_drops_entity_encoded_device_paths() {
    encoded_paths_are_private("bibtex");
}

#[test]
fn csl_json_drops_entity_encoded_device_paths() {
    encoded_paths_are_private("csl-json");
}

#[test]
fn doi_links_refuse_entity_encoded_device_paths() {
    encoded_paths_are_private("doi-links");
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
fn scannable_markers_only_show_checked_authorless_titles() {
    for title in [
        "&#67;&#58;&#92;invented&#92;secret.pdf",
        "-/home/invented/secret.pdf",
        "|/home/invented/secret.pdf",
        "Invented <i>atlas</i> &amp; tides",
    ] {
        let mut case = case("scannable-cite", json!({"type":"book", "title":title}), None);
        let LibraryObject::Record(mut record) = case.library.objects()[0].object.clone() else {
            unreachable!()
        };
        record.id.key = "ABCD2345".into();
        case.request.items[0].item = record.id.clone();
        case.library = Snapshot::new(vec![LibraryObject::Record(record)]).unwrap();
        let result = syntax::scannable::cite(&case.library, &case.request, |_| {
            Some(syntax::scannable::ZoteroOrigin::Personal)
        })
        .unwrap();
        assert_eq!(
            result.text,
            if title.contains("secret") {
                "{ | | | | zu:0:ABCD2345 }"
            } else {
                "{ | Invented atlas & tides | | | zu:0:ABCD2345 }"
            }
        );
        assert!(result.copy_content(1).is_some());
    }
}

#[test]
fn bibtex_preserves_approximate_date_parts() {
    let case = case(
        "bibtex",
        json!({
            "type":"book", "issued":{"date-parts":[[2020, 3]], "circa":true},
            "accessed":{"date-parts":[[2020, 3, 1]], "circa":true},
            "original-date":{"date-parts":[[2019], [2020]], "circa":true},
            "event-date":{"date-parts":[[2020]], "circa":false}
        }),
        Some("atlas"),
    );
    let text = syntax::cite(&case.library, &case.request).unwrap().text;
    assert!(text.contains("  date = {2020-03~},"), "{text}");
    assert!(text.contains("  urldate = {2020-03-01~},"), "{text}");
    assert!(text.contains("  origdate = {2019/2020~},"), "{text}");
    assert!(text.contains("  eventdate = {2020},"), "{text}");
}

#[test]
fn exports_share_decoded_normalized_text_without_reinterpreting_literal_tags() {
    let fields = json!({
        "type":"book", "title":"<i>Invented</i>&#10;atlas &lt;b&gt;e&#x301;&lt;/b&gt; &amp;lt;i&amp;gt;",
        "author":[{"family":"D&#111;e"}], "DOI":"&#100;oi:10.1000/Invented&#38;tides",
        "URL":"https:&#47;&#47;example.invalid/work?x=1&#38;y=2"
    });
    let csl = case("csl-json", fields.clone(), Some("atlas"));
    let records: Value = serde_json::from_str(&syntax::cite(&csl.library, &csl.request).unwrap().text).unwrap();
    assert_eq!(
        records[0]["title"],
        "<i>Invented</i> atlas &lt;b&gt;é&lt;/b&gt; &amp;lt;i&amp;gt;"
    );
    assert_eq!(records[0]["author"][0]["family"], "Doe");
    assert_eq!(records[0]["URL"], "https://example.invalid/work?x=1&y=2");
    let bib = case("bibtex", fields.clone(), Some("atlas"));
    let bib = syntax::cite(&bib.library, &bib.request).unwrap().text;
    assert!(bib.contains(r"\textit{Invented} atlas <b>é</b> \&lt;i\&gt;"), "{bib}");
    let doi = case("doi-links", fields, Some("atlas"));
    assert_eq!(
        syntax::cite(&doi.library, &doi.request).unwrap().text,
        "https://doi.org/10.1000/Invented%26tides"
    );
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
            json!([{"outcome":"refused","item":0,"error":"missing-doi"}])
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
fn record_sharing_keeps_rich_text_and_normalizes_padded_date_parts() {
    let title = "Invented <span class=\"nocase\">DNA </span> study";
    for fields in [
        json!({"title":title, "issued":{"date-parts":[[" 2026 ", "5\n", 1]]}}),
        json!({"note":format!("title: {title}"), "issued":{"date-parts":[["2026\n", 5, " 1 "]]}}),
    ] {
        let case = case("csl-json", fields, Some("good"));
        let result = syntax::cite(&case.library, &case.request).unwrap();
        let records: Value = serde_json::from_str(&result.text).unwrap();
        assert_eq!(records[0]["title"], title);
        assert_eq!(records[0]["issued"], json!({"date-parts":[[2026, 5, 1]]}));
    }
    for (title, canonical) in [
        (
            "Invented &lt;span class=&quot;nocase&quot;&gt;DNA &lt;/span&gt; study",
            "Invented &lt;span class=&quot;nocase&quot;&gt;DNA &lt;/span&gt; study",
        ),
        (
            "Invented &lt;em&gt;DNA &lt;/em&gt; study",
            "Invented &lt;em&gt;DNA &lt;/em&gt; study",
        ),
        (
            "Invented <https://example.invalid/work>",
            "Invented &lt;https://example.invalid/work&gt;",
        ),
        (
            "Invented &lt;https://example.invalid/work&gt;",
            "Invented &lt;https://example.invalid/work&gt;",
        ),
    ] {
        let case = case("csl-json", json!({"title":title}), Some("good"));
        let result = syntax::cite(&case.library, &case.request).unwrap();
        let records: Value = serde_json::from_str(&result.text).unwrap();
        assert_eq!(records[0]["title"], canonical);
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
fn unrepresentable_verbatim_identifiers_are_omitted() {
    let case = case(
        "bibtex",
        json!({"type":"book","ISBN":"} @misc{ \\input{x}\nnext"}),
        Some("good"),
    );
    let text = syntax::cite(&case.library, &case.request).unwrap().text;
    assert!(!text.contains("} @misc{"), "{text}");
    assert!(!text.contains("\\input"), "{text}");
    assert!(!text.contains("\nnext"), "{text}");
    assert!(!text.contains("  isbn ="), "{text}");
}
