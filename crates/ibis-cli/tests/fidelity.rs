//! Citation fidelity: a bibliography rendered by Pandoc from the original
//! `.bib` file must match the one rendered from Ibis's CSL-JSON export, with
//! exact expectations for the one title Pandoc recases during BibTeX import.
//!
//! Requires `pandoc` on PATH. Skipped when it is missing, unless
//! `IBIS_REQUIRE_PANDOC=1` is set (as in CI), in which case it fails.

use std::path::Path;
use std::process::Command;

fn pandoc_available() -> bool {
    Command::new("pandoc")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn render(bibliography: &Path) -> String {
    let doc = Path::new(env!("CARGO_TARGET_TMPDIR")).join("fidelity-doc.md");
    std::fs::write(&doc, "---\nnocite: '@*'\n---\n").unwrap_or_else(|e| panic!("{e}"));
    let out = Command::new("pandoc")
        .args(["--citeproc", "-t", "plain", "--wrap=none", "--bibliography"])
        .arg(bibliography)
        .arg(&doc)
        .output()
        .unwrap_or_else(|e| panic!("cannot run pandoc: {e}"));
    assert!(
        out.status.success(),
        "pandoc failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    known_pandoc_quirks(&String::from_utf8_lossy(&out.stdout))
}

/// Pandoc decodes `{\\'\\i}` as a dotless ı plus a combining accent ("Garcı́a")
/// instead of "í". Ibis produces the composed letter; normalize Pandoc's form
/// so the comparison checks meaning, not this encoding detail.
fn known_pandoc_quirks(text: &str) -> String {
    text.replace("ı\u{301}", "í")
}

#[test]
fn bibliography_from_csl_export_matches_original_bibtex() {
    if !pandoc_available() {
        assert!(
            std::env::var("IBIS_REQUIRE_PANDOC").is_err(),
            "pandoc is required but not installed"
        );
        eprintln!("pandoc not found; fidelity comparison skipped");
        return;
    }
    let bib = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/bibtex/fidelity.bib");
    let export = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .args([
            "cite",
            "accents2021",
            "protected2020",
            "italic2019",
            "others2018",
            "who2020",
            "particles2017",
            "german2016",
            "range2015",
            "conf2022",
            "chapter2022",
            "thesis2014",
            "news2020",
            "web2024",
            "translated2019",
            "--format",
            "csl-json",
            "--file",
        ])
        .arg(&bib)
        .output()
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(export.status.success(), "{}", String::from_utf8_lossy(&export.stderr));
    assert!(
        export.stderr.is_empty(),
        "fixture produced warnings: {}",
        String::from_utf8_lossy(&export.stderr)
    );
    let exported: serde_json::Value = serde_json::from_slice(&export.stdout).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        exported
            .as_array()
            .and_then(|items| items.iter().find(|item| item["id"] == "italic2019"))
            .and_then(|item| item["title"].as_str()),
        Some("The Role of <i>In Vivo</i> Studies in Sleep Research")
    );
    let json = Path::new(env!("CARGO_TARGET_TMPDIR")).join("fidelity.json");
    std::fs::write(&json, &export.stdout).unwrap_or_else(|e| panic!("{e}"));

    let expected = render(&bib);
    let actual = render(&json);
    let expected_lines: Vec<&str> = expected.lines().filter(|l| !l.trim().is_empty()).collect();
    let actual_lines: Vec<&str> = actual.lines().filter(|l| !l.trim().is_empty()).collect();
    let mismatches: Vec<String> = expected_lines
        .iter()
        .zip(&actual_lines)
        .filter(|(e, a)| e != a)
        .filter(|(e, a)| {
            // Pandoc lowercases the unprotected word inside TeX emphasis on
            // import. Ibis now preserves stored case (#1530); pin both exact
            // outputs instead of normalizing capitalization in the comparison.
            !(**e == "Lindqvist, Sara. 2019. “The Role of in Vivo Studies in Sleep Research.” Synthetic Biology Letters."
              && **a == "Lindqvist, Sara. 2019. “The Role of In Vivo Studies in Sleep Research.” Synthetic Biology Letters.")
        })
        .map(|(e, a)| format!("  from .bib:  {e}\n  from Ibis:  {a}"))
        .collect();
    assert_eq!(
        expected_lines.len(),
        actual_lines.len(),
        "different number of entries:\n{expected}\n---\n{actual}"
    );
    assert!(mismatches.is_empty(), "rendering differs:\n{}", mismatches.join("\n\n"));
}

/// Plain text of a Pandoc inline list.
fn stringify(inlines: &serde_json::Value) -> String {
    let mut out = String::new();
    for inline in inlines.as_array().into_iter().flatten() {
        match inline["t"].as_str() {
            Some("Str") => out.push_str(inline["c"].as_str().unwrap_or_default()),
            Some("Space" | "SoftBreak" | "LineBreak") => out.push(' '),
            Some("Code" | "RawInline") => out.push_str(inline["c"][1].as_str().unwrap_or_default()),
            Some("Emph" | "Strong" | "Underline" | "Strikeout" | "SmallCaps" | "Superscript" | "Subscript") => {
                out.push_str(&stringify(&inline["c"]));
            }
            Some("Span") => out.push_str(&stringify(&inline["c"][1])),
            other => out.push_str(&format!("<{other:?}>")),
        }
    }
    out
}

/// Pandoc must read back exactly the key, prefix and suffix Ibis meant,
/// whatever characters they contain.
#[test]
fn pandoc_reads_back_keys_prefixes_and_suffixes_exactly() {
    use ibis_core::contract::*;
    use ibis_engine::{Cancellation, Engine};
    use std::sync::Arc;
    if !pandoc_available() {
        assert!(
            std::env::var("IBIS_REQUIRE_PANDOC").is_err(),
            "pandoc is required but not installed"
        );
        return;
    }
    let cases = [
        ("smith2020", Some("see"), Some("emphasis added")),
        ("doe:2019", Some("cf. [note]"), Some("a; b")),
        ("a_b", Some("50% & more"), Some("under_score *star*")),
        ("Smith_2020.", Some("email@example.org"), Some("back\\slash")),
        ("o'neil", Some("`tick` <angle>"), Some("$dollar$ #hash")),
        ("x-y/z", None, Some("~tilde^caret")),
    ];
    let template: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json"))
        .unwrap_or_else(|error| panic!("{error}"));
    for (key, prefix, suffix) in cases {
        let LibraryObject::Record(mut record) = template.library.objects()[0].object.clone() else {
            panic!("record")
        };
        record.citation_keys[0].value = key.into();
        let mut request = template.request.clone();
        let item = &mut request.items[0];
        item.expected_cite = None;
        item.prefix = prefix
            .map(|text| TextRun {
                text: text.into(),
                ..Default::default()
            })
            .into_iter()
            .collect();
        item.suffix = suffix
            .map(|text| TextRun {
                text: text.into(),
                ..Default::default()
            })
            .into_iter()
            .collect();
        item.locator = Some(Locator {
            term: LocatorTerm::Page,
            value: "12".into(),
        });
        let engine = Engine::from_snapshot(Arc::new(
            Snapshot::new(vec![LibraryObject::Record(record)]).unwrap_or_else(|error| panic!("{error}")),
        ));
        let result = engine
            .cite(&request, &Cancellation::default())
            .unwrap_or_else(|error| panic!("{:?}", error.code));
        let text = result
            .copy_content(1)
            .unwrap_or_else(|| panic!("refused synthetic citation"))
            .text;
        let doc = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cite-roundtrip.md");
        std::fs::write(&doc, format!("Text {text} more.\n")).unwrap_or_else(|e| panic!("{e}"));
        let out = Command::new("pandoc")
            .args(["-f", "markdown-smart", "-t", "json"])
            .arg(&doc)
            .output()
            .unwrap_or_else(|e| panic!("{e}"));
        let ast: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}"));
        let cite = ast["blocks"][0]["c"]
            .as_array()
            .and_then(|inlines| inlines.iter().find(|i| i["t"] == "Cite"))
            .unwrap_or_else(|| panic!("Pandoc did not see a citation in {text}"));
        let citation = &cite["c"][0][0];
        assert_eq!(citation["citationId"], key, "{text}");
        assert_eq!(
            stringify(&citation["citationPrefix"]),
            prefix.unwrap_or_default(),
            "{text}"
        );
        // Pandoc preserves the explicit locator's delimiters in the AST and
        // consumes them during citeproc, before the unchanged literal suffix.
        let expected_suffix = format!(", {{p. 12}}, {}", suffix.unwrap_or_default());
        assert_eq!(
            stringify(&citation["citationSuffix"]).trim(),
            expected_suffix.trim(),
            "{text}"
        );
    }
}
