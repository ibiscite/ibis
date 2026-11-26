//! Regressions for decoded rich text and identity after export and reimport.
#![allow(clippy::unwrap_used)]
use ibis_engine::Engine;
use serde_json::{Value, json};
use std::io::Cursor;

fn cite(records: Value, format: &str) -> Value {
    let mut args = vec!["ibis", "cite", "--records", "--format", format, "--json"];
    if format == "apa" {
        args.push("--bibliography");
    }
    let (mut output, mut errors) = (vec![], vec![]);
    let code = ibis::run_with(
        Engine::new().unwrap().without_file_watches(),
        args.into_iter().map(Into::into).collect(),
        Cursor::new(records.to_string().into_bytes()),
        &mut output,
        &mut errors,
        ibis::OutputMode::Plain,
    );
    assert!(code <= 1, "{}", String::from_utf8_lossy(&errors));
    let reply: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(reply["ok"], true, "{format}: {reply}");
    reply["data"].clone()
}

#[test]
fn paths_split_across_markup_are_checked_as_one_decoded_value() {
    for title in [
        "<i>/Users/</i><b>invented/secret.pdf</b>",
        "<i>&#47;Users/invented/secret.pdf</i>",
    ] {
        for format in ["csl-json", "bibtex", "apa"] {
            let result = cite(
                json!([{"id":"r1", "citation-key":"good", "type":"book",
                "title":title, "publisher":"Invented Press",
                // Something to cite it by once the private title is gone.
                "editor":[{"family":"Invented Editor"}]}]),
                format,
            );
            assert!(!result.to_string().contains("secret"), "{format}: {result}");
            assert!(result["text"].as_str().unwrap().contains("Invented Press"), "{result}");
        }
    }
}

#[test]
fn local_authorities_cannot_hide_behind_identifier_schemes() {
    for url in [
        "hdl://100.64.0.1/secret",
        "urn://[::1]/secret",
        "hdl:/Users/invented/secret",
    ] {
        for format in ["csl-json", "bibtex", "apa"] {
            let result = cite(
                json!([{"id":"r1", "citation-key":"good", "type":"book", "title":"Invented atlas",
                "URL":url, "publisher":format!("See {url}")}]),
                format,
            );
            assert!(!result.to_string().contains("secret"), "{format}: {result}");
            assert!(result["text"].as_str().unwrap().contains("Invented atlas"));
        }
    }
}
