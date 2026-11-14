//! A shared BibTeX file is an ordinary library, with the same records and keys.
#![allow(clippy::unwrap_used)]
use ibis_engine::Engine;
use serde_json::{Value, json};
use std::{io::Cursor, path::Path};

fn run(path: &Path, args: &[&str]) -> (u8, String, String) {
    let mut argv = vec!["ibis", "--file", path.to_str().unwrap()];
    argv.extend_from_slice(args);
    let (mut output, mut errors) = (vec![], vec![]);
    let code = ibis::run_with(
        Engine::new().unwrap().without_file_watches(),
        argv.into_iter().map(Into::into).collect(),
        Cursor::new(vec![]),
        &mut output,
        &mut errors,
        ibis::OutputMode::Plain,
    );
    (
        code,
        String::from_utf8(output).unwrap(),
        String::from_utf8(errors).unwrap(),
    )
}

#[test]
fn shared_bibtex_reopens_with_the_same_records_and_keys() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let source = dir.join("share-source.json");
    let exported = dir.join("share-out.bib");
    let records = json!([
        {"id":"one","citation-key":"atlas2026","type":"book","title":"Invented atlas","author":[{"family":"Doe","given":"Ada"}],"publisher":"Imaginary Press","issued":{"date-parts":[[2026]]},"note":"PRIVATE","URL":"file:///home/PRIVATE/atlas.pdf"},
        {"id":"two","citation-key":"rivers2025","type":"article-journal","title":"Invented rivers","container-title":"Imaginary Journal","volume":"4","issue":"2","page":"12–20","DOI":"10.1000/Invented%2F~_#{x}^","URL":"https://example.invalid/a~b^c", "issued":{"date-parts":[[2025]]}},
        {"id":"three","citation-key":"hostile","type":"book","title":r"Invented } @misc{ \input{x} \write18{x} a--b and ``c''"},
        {"id":"four","citation-key":"rich","type":"book","author":[{"family":"Berg","given":"Jan","dropping-particle":"van der"},{"family":"Wit","given":"Ada","non-dropping-particle":"de"},{"literal":"Invented Research and Tests"}],"title":"Invented <i>rivers</i> &amp; tides <span class=\"nocase\">DNA</span> <b>bold</b> <sub>2</sub>"}
    ]);
    std::fs::write(&source, records.to_string()).unwrap();
    let (code, bibtex, errors) = run(
        &source,
        &[
            "cite",
            "--format",
            "bibtex",
            "atlas2026",
            "rivers2025",
            "hostile",
            "rich",
        ],
    );
    // The source's private URL produces the existing source-field warning.
    assert_eq!(code, 1, "{errors}");
    assert!(!bibtex.contains("PRIVATE"));
    std::fs::write(&exported, bibtex).unwrap();
    let (code, search, errors) = run(&exported, &["search", "Invented", "--json"]);
    assert_eq!(code, 0, "{errors}");
    let search: Value = serde_json::from_str(&search).unwrap();
    assert_eq!(search["data"]["hits"].as_array().unwrap().len(), 4, "{search}");
    let (code, text, errors) = run(
        &exported,
        &[
            "cite",
            "--format",
            "csl-json",
            "atlas2026",
            "rivers2025",
            "hostile",
            "rich",
        ],
    );
    assert_eq!(code, 0, "{errors}");
    let reopened: Vec<Value> = serde_json::from_str(&text).unwrap();
    for (original, reopened) in records.as_array().unwrap().iter().zip(reopened) {
        for field in [
            "citation-key",
            "type",
            "title",
            "author",
            "publisher",
            "issued",
            "container-title",
            "volume",
            "issue",
            "page",
            "DOI",
            "URL",
        ] {
            if field == "URL" && original["citation-key"] == "atlas2026" {
                assert!(reopened[field].is_null());
            } else {
                assert_eq!(original[field], reopened[field], "{field}: {reopened}");
            }
        }
    }
}

#[test]
fn doi_links_keep_successes_and_identify_records_without_a_doi() {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("share-dois.json");
    std::fs::write(
        &path,
        json!([
            {"id":"one","citation-key":"atlas","type":"book","title":"Invented atlas"},
            {"id":"two","citation-key":"rivers","type":"book","title":"Invented rivers","DOI":"10.1000/Invented"}
        ])
        .to_string(),
    )
    .unwrap();
    let (code, text, errors) = run(&path, &["cite", "--format", "doi-links", "atlas", "rivers"]);
    assert_eq!(code, 1, "{errors}");
    assert_eq!(text, "https://doi.org/10.1000/Invented\n");
    assert!(errors.contains("atlas") && errors.contains("DOI"), "{errors}");
}

#[test]
fn missing_doi_json_offers_output_selection() {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("recovery-doi.json");
    std::fs::write(
        &path,
        r#"[{"id":"one","citation-key":"atlas","type":"book","title":"Invented atlas"}]"#,
    )
    .unwrap();
    let (_, text, _) = run(&path, &["cite", "--format", "doi-links", "atlas", "--json"]);
    let reply: Value = serde_json::from_str(&text).unwrap();
    let refusal = &reply["data"]["outcomes"][0];
    assert_eq!(refusal["error"], "missing-doi");
    assert_eq!(refusal["recovery"]["action"], "choose-output");
    assert_eq!(refusal["focus"], "format-picker");
    let (_, _, advice) = run(&path, &["cite", "--format", "doi-links", "atlas"]);
    assert!(advice.contains("--format bibtex"), "{advice}");
}

#[test]
fn identifier_fields_are_verbatim_for_latex_and_reopen_identically() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let source = dir.join("share-identifiers.json");
    let exported = dir.join("share-identifiers.bib");
    let record = json!({
        "id":"one", "citation-key":"good", "type":"book", "title":"Invented identifiers",
        "URL":"https://e.org/a~b--c_d#x?y=1&z=2", "DOI":"10.1000/a~b--c_d#x?y=1&z=2",
        "ISBN":"978-0-1", "ISSN":"1234-5678", "PMID":"12345", "PMCID":"PMC12345"
    });
    let braced =
        json!({"id":"two","citation-key":"braced","type":"book","title":"Invented braces","URL":"https://e.org/{x}"});
    std::fs::write(&source, json!([record, braced]).to_string()).unwrap();
    let (code, text, errors) = run(&source, &["cite", "good", "braced", "--format", "bibtex"]);
    assert_eq!(code, 0, "{errors}");
    for (csl, bib) in [
        ("URL", "url"),
        ("DOI", "doi"),
        ("ISBN", "isbn"),
        ("ISSN", "issn"),
        ("PMID", "pmid"),
        ("PMCID", "pmcid"),
    ] {
        assert!(
            text.contains(&format!("  {bib} = {{{}}},", record[csl].as_str().unwrap())),
            "{text}"
        );
    }
    assert!(text.contains("  url = {https://e.org/%7Bx%7D},"), "{text}");
    std::fs::write(&exported, text).unwrap();
    let (code, text, errors) = run(&exported, &["cite", "good", "braced", "--format", "csl-json"]);
    assert_eq!(code, 0, "{errors}");
    let back: Value = serde_json::from_str(&text).unwrap();
    for field in ["citation-key", "URL", "DOI", "ISBN", "ISSN", "PMID", "PMCID"] {
        assert_eq!(back[0][field], record[field], "{field}");
    }
    assert_eq!(back[1]["URL"], "https://e.org/%7Bx%7D");
    assert_eq!(back[1]["citation-key"], "braced");
}

#[test]
fn exported_csl_json_replaces_zotero_user_urls_with_the_citation_key() {
    let source = Path::new(env!("CARGO_TARGET_TMPDIR")).join("share-zotero-id.json");
    std::fs::write(&source, json!([{"id":"http://zotero.org/users/12345/items/ABCD1234", "citation-key":"good", "type":"book", "title":"Invented atlas"}]).to_string()).unwrap();
    let (code, text, errors) = run(&source, &["cite", "good", "--format", "csl-json"]);
    assert_eq!(code, 0, "{errors}");
    let records: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(records[0]["id"], "good");
    assert!(!text.contains("12345") && !text.contains("ABCD1234"), "{text}");
}
