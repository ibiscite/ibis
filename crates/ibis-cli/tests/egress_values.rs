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
fn bibtex_identifiers_preserve_literal_braces_without_prose_commands() {
    let records = json!([{
        "id":"invented", "citation-key":"invented:key/1", "type":"book",
        "title":"Invented {prose}", "DOI":"10.1000/{x}", "URL":"https://example.invalid/{x}",
        "ISBN":"978-0-{1}", "ISSN":"1234-{5678}"
    }]);
    let (code, result) = cite(&records, "bibtex");
    assert_eq!(code, 0, "{result}");
    let bib = result["text"].as_str().unwrap();
    for (field, value) in [("doi", "10.1000/{x}"), ("isbn", "978-0-{1}"), ("issn", "1234-{5678}")] {
        assert!(bib.contains(&format!("  {field} = {{{value}}},")), "{bib}");
    }
    assert!(
        bib.contains(r"Invented \textbraceleft{}prose\textbraceright{}"),
        "{bib}"
    );
    // Pandoc's independent BibLaTeX reader must recover the literal identifier.
    use std::process::{Command, Stdio};
    if Command::new("pandoc")
        .arg("--version")
        .output()
        .is_ok_and(|out| out.status.success())
    {
        use std::io::Write;
        let mut child = Command::new("pandoc")
            .args(["-f", "biblatex", "-t", "csljson"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(bib.as_bytes()).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        let back: Value = serde_json::from_slice(&output.stdout).unwrap();
        for field in ["DOI", "ISBN", "ISSN"] {
            assert_eq!(back[0][field], records[0][field], "{field}: {back}");
        }
    } else {
        assert!(
            std::env::var("IBIS_REQUIRE_PANDOC").is_err(),
            "pandoc is required but not installed"
        );
        eprintln!("pandoc not found; independent BibLaTeX readback skipped");
    }
}

#[test]
fn a_name_with_both_particle_kinds_keeps_every_part_on_export_and_reopen() {
    let records = json!([{
        "id":"invented", "citation-key":"invented", "type":"book", "title":"Invented atlas",
        "author":[{"family":"Berg", "given":"Jan", "dropping-particle":"van der", "non-dropping-particle":"de", "suffix":"Jr."}]
    }]);
    for format in ["csl-json", "bibtex", "apa"] {
        let (_, result) = cite(&records, format);
        for text in representations(&result) {
            assert!(
                text.contains("van der") && text.contains("de") && text.contains("Berg") && text.contains("Jr."),
                "{format}: {text}"
            );
        }
        if format == "bibtex" {
            let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("egress-both-particles.bib");
            std::fs::write(&path, result["text"].as_str().unwrap()).unwrap();
            let output = read_export(&path);
            let back: Value = serde_json::from_slice(&output).unwrap();
            assert_eq!(
                ibis_core::model::Name::from_csl(&back[0]["author"][0]).display(),
                "Jan van der de Berg Jr.",
                "{back}"
            );
        }
    }
}

fn read_export(path: &std::path::Path) -> Vec<u8> {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ibis"))
        .args(["--file"])
        .arg(path)
        .args(["cite", "invented", "--format", "csl-json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    output.stdout
}
