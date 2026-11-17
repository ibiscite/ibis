//! Export contracts exercised at the CLI boundary, including rich CSL output.
#![allow(clippy::unwrap_used)]
use ibis_engine::Engine;
use serde_json::{Value, json};
use std::io::Cursor;

fn cite(records: &Value, format: &str) -> (u8, Value) {
    let mut args = vec!["ibis", "cite", "--records", "--format", format, "--json"];
    if format == "apa" {
        args.extend(["--form", "full"]);
    }
    let (mut output, mut errors) = (vec![], vec![]);
    let code = ibis::run_with(
        Engine::new().unwrap().without_file_watches(),
        args.into_iter().map(Into::into).collect(),
        Cursor::new(serde_json::to_vec(records).unwrap()),
        &mut output,
        &mut errors,
        ibis::OutputMode::Plain,
    );
    assert!(code <= 1, "{format}: {}", String::from_utf8_lossy(&errors));
    let envelope: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(envelope["ok"], true, "{envelope}");
    (code, envelope["data"].clone())
}

fn representations(result: &Value) -> impl Iterator<Item = &str> {
    ["text", "html", "rtf"]
        .into_iter()
        .filter_map(|field| result[field].as_str())
}

#[test]
fn markup_wrapped_device_paths_never_reach_any_export_representation() {
    let records = json!([{
        "id":"invented", "citation-key":"invented", "type":"book",
        "title":"<i>/Users/invented/secret.pdf</i>", "publisher":"Invented Press",
        "author":[{"family":"<b>/Users/invented/secret.pdf</b>"}],
        "issued":{"literal":"<i>/Users/invented/secret.pdf</i>"},
        "abstract":"<i>/Users/invented/secret.pdf</i>", "DOI":"10.1000/Invented"
    }]);
    for format in ["csl-json", "bibtex", "doi-links", "apa"] {
        let (_, result) = cite(&records, format);
        for text in representations(&result) {
            assert!(!text.contains("secret"), "{format}: {text}");
            assert!(
                text.contains(if format == "doi-links" {
                    "10.1000/Invented"
                } else {
                    "Invented Press"
                }),
                "{format}: {text}"
            );
        }
    }
}

#[test]
fn non_global_literal_addresses_and_local_names_cannot_leave_as_urls_or_prose() {
    let hosts = [
        "0.0.0.0",
        "0.1.2.3",
        "10.0.0.1",
        "100.64.0.1",
        "100.127.255.255",
        "127.0.0.1",
        "169.254.1.1",
        "172.16.0.1",
        "192.168.1.1",
        "192.0.0.8",
        "192.0.2.1",
        "192.88.99.1",
        "198.18.0.1",
        "198.19.255.255",
        "198.51.100.1",
        "203.0.113.1",
        "224.0.0.1",
        "240.0.0.1",
        "255.255.255.255",
        "[::]",
        "[::1]",
        "[::ffff:100.64.0.1]",
        "[::ffff:8.8.8.8]",
        "[::127.0.0.1]",
        "[64:ff9b:1::a00:1]",
        "[100::1]",
        "[100:0:0:1::1]",
        "[2001:2::1]",
        "[2001:10::1]",
        "[2001:20::1]",
        "[2001:db8::1]",
        "[3fff::1]",
        "[5f00::1]",
        "[fc00::1]",
        "[fd00::1]",
        "[fe80::1]",
        "[fec0::1]",
        "[ff02::1]",
        "localhost",
        "papers.local",
        "papers.internal",
        "papers.lan",
        "papers.home",
        "papers.home.arpa",
        "papers",
        "127.1",
        "2130706433",
        "0x7f000001",
        "%31%32%37.0.0.1",
        "localhost.",
    ];
    let records = Value::Array(
        hosts
            .iter()
            .enumerate()
            .map(|(index, host)| {
                json!({
                    "id":format!("r{index}"), "citation-key":format!("r{index}"), "type":"book",
                    "title":format!("See https://{host}/secret"), "URL":format!("https://{host}/secret"),
                    "publisher":"Invented Press"
                })
            })
            .collect(),
    );
    for format in ["csl-json", "bibtex", "apa"] {
        let (_, result) = cite(&records, format);
        for text in representations(&result) {
            assert!(!text.contains("secret"), "{format}: {text}");
            assert!(text.contains("Invented Press"), "{format}: {text}");
        }
    }
    for url in [
        "https://8.8.8.8/work",
        "https://[2606:4700:4700::1111]/work",
        "https://example.invalid/work",
    ] {
        let records = json!([{"id":"public", "citation-key":"public", "type":"book", "title":"Invented", "URL":url}]);
        for format in ["csl-json", "bibtex", "apa"] {
            let (code, result) = cite(&records, format);
            assert_eq!(code, 0, "{result}");
            assert!(result["text"].as_str().unwrap().contains(url), "{format}: {result}");
        }
    }
}

#[test]
fn doi_dot_segments_are_refused_before_any_writer_can_change_the_identifier() {
    for doi in ["10.1000/a/../b", "10.1000/./b", "10.1000/..", "10.1000/a/."] {
        let records =
            json!([{"id":"invented", "citation-key":"invented", "type":"book", "title":"Invented atlas", "DOI":doi}]);
        for format in ["csl-json", "bibtex", "apa", "doi-links"] {
            let (code, result) = cite(&records, format);
            for text in representations(&result) {
                assert!(!text.contains("10.1000"), "{format}: {text}");
            }
            if format == "doi-links" {
                assert_eq!(code, 1);
                assert_eq!(result["text"], "");
                assert!(result["outcomes"].to_string().contains("missing-doi"), "{result}");
            }
        }
    }
    let records = json!([{"id":"invented", "citation-key":"invented", "type":"book", "title":"Invented atlas", "DOI":"10.1000/a/b%2Fc?#"}]);
    let expected = "https://doi.org/10.1000/a/b%252Fc%3F%23";
    for format in ["doi-links", "apa"] {
        let (code, result) = cite(&records, format);
        assert_eq!(code, 0, "{result}");
        for text in representations(&result) {
            assert!(text.contains(expected), "{format}: {text}");
        }
    }
}

#[test]
fn prose_and_identifiers_shaped_like_paths_stay_public() {
    let titles = [
        "Prices in \u{20ac}/kWh and US$/kg",
        "Porting C#/F#/VB code",
        "Fe(II)/Fe(III)/Mn redox and/or TCP/IP",
        "HTTP: The definitive guide",
        "Doses of 5\u{b0}/s/s",
        "Na\u{207a}/K\u{207a}/2Cl\u{207b} cotransport",
        "\u{201c}yes\u{201d}/\u{201c}no\u{201d}/\u{201c}maybe\u{201d} answers",
        "Costs in $/kW/yr, \u{20ac}/MWh/a and \u{b0}/sec/sec",
        "pre-/intra-/post-operative",
    ];
    for title in titles {
        let records = json!([{
            "id":"invented", "citation-key":"invented", "type":"article-journal", "title":title,
            "DOI":"10.1016/S0140-6736(20)30183-5"
        }]);
        let (code, result) = cite(&records, "csl-json");
        assert_eq!(code, 0, "{result}");
        let back: Value = serde_json::from_str(result["text"].as_str().unwrap()).unwrap();
        assert_eq!(back[0]["title"], title, "{back}");
        assert_eq!(back[0]["DOI"], "10.1016/S0140-6736(20)30183-5", "{back}");
    }
}
