//! Citation fidelity: a bibliography rendered by Pandoc from the original
//! `.bib` file must match the one rendered from Ibis's CSL-JSON export.
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
        .args(["csl", "--all", "--bib"])
        .arg(&bib)
        .output()
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(export.status.success(), "{}", String::from_utf8_lossy(&export.stderr));
    assert!(
        export.stderr.is_empty(),
        "fixture produced warnings: {}",
        String::from_utf8_lossy(&export.stderr)
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
        .map(|(e, a)| format!("  from .bib:  {e}\n  from Ibis:  {a}"))
        .collect();
    assert_eq!(
        expected_lines.len(),
        actual_lines.len(),
        "different number of entries:\n{expected}\n---\n{actual}"
    );
    assert!(mismatches.is_empty(), "rendering differs:\n{}", mismatches.join("\n\n"));
}
