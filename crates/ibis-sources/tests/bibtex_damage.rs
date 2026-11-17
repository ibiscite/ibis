//! Damaged, malformed or hostile BibTeX keeps every good record, never
//! invents metadata, and reports each problem on the line it came from.
//! Synthetic data only.
#![cfg(test)]

#[path = "support/bib.rs"]
mod bib;

use bib::{all_field, csl, date_parts, keys, load, month, plain, records, text, warned_at, year};
use ibis_sources::WarningCode;
use serde_json::json;

const PREFIXES: [(&str, &str); 4] = [
    ("", "issued"),
    ("url", "accessed"),
    ("orig", "original-date"),
    ("event", "event-date"),
];

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../fixtures/bibtex/{name}"))
}

fn fixture_text(name: &str) -> String {
    std::fs::read_to_string(fixture(name)).unwrap_or_else(|e| panic!("{e}"))
}

// --- entry recovery -------------------------------------------------------------

#[test]
fn incomplete_entries_recover_the_following_record_with_line_numbers() {
    for open in ['{', '('] {
        let text = format!("@article{open}first,\n  @article{open}second,\n\t@book{{kept,title={{Retained}}}}\n");
        let loaded = load(&text);
        assert_eq!(keys(&loaded), ["kept"]);
        assert_eq!(bib::text(&loaded, "kept", "title"), Some("Retained"));
        assert_eq!(loaded.warnings.len(), 2, "{:?}", loaded.warnings);
        for (index, warning) in loaded.warnings.iter().enumerate() {
            assert_eq!(warning.code, WarningCode::Syntax);
            assert_eq!(warning.line, Some(index + 1));
        }
    }
}

#[test]
fn entry_looking_lines_inside_complete_values_are_preserved() {
    for (open, close) in [('{', '}'), ('(', ')')] {
        for (value_open, value_close) in [('{', '}'), ('"', '"')] {
            let outer = format!(
                "@article{open}outer,title={value_open}Before\n  @book{{inside,title={{Synthetic}}}}\nAfter{value_close}{close}"
            );
            let text = format!("@book{{broken,\n{outer}\n@book{{kept,title={{Retained}}}}\n");
            let loaded = load(&text);
            assert_eq!(keys(&loaded), ["kept", "outer"], "{text}: {:?}", loaded.warnings);
            let title = plain(&loaded, "outer", "title").unwrap_or_default();
            for fragment in ["before", "@book", "inside,title=", "synthetic", "after"] {
                assert!(title.to_lowercase().contains(fragment), "{title:?} lost {fragment:?}");
            }
            let isolated = load(&outer);
            assert_eq!(csl(&loaded, "outer"), csl(&isolated, "outer"));
            assert!(isolated.warnings.is_empty(), "{:?}", isolated.warnings);
            assert_eq!(loaded.warnings.len(), 1, "{text}: {:?}", loaded.warnings);
        }
    }
}

#[test]
fn quotes_parentheses_escapes_and_comments_keep_their_structure() {
    let text = "@book{broken,\n@book(outer, title=\"A (B) C \\\"u and 100%\",\n% ignored ) } { \"\n year={2024})\n@book{kept,title={Retained}}\n";
    let loaded = load(text);
    assert_eq!(keys(&loaded), ["kept", "outer"], "{:?}", loaded.warnings);
    assert_eq!(year(&loaded, "outer"), Some(2024));
    let title = plain(&loaded, "outer", "title").unwrap_or_default();
    assert!(title.to_lowercase().contains("a (b) c"));
    assert_eq!(loaded.warnings.len(), 1, "{:?}", loaded.warnings);
}

#[test]
fn stray_closing_braces_in_entry_types_warn_without_dropping_neighbors() {
    for damaged in [
        "@art}icle(k3, title={T3})",
        "@art}icle{k3, title={T3}}",
        "@article}(k3, title={T3})",
        "@art}}icle{k3, title={T3}}",
    ] {
        for prefix in ["", "@book(unfinished, title=\"\n"] {
            let text = format!("{prefix}@misc{{before}}\n{damaged}\n@misc{{after}}");
            let loaded = load(&text);
            assert_eq!(keys(&loaded), ["after", "before"]);
            let warning = loaded
                .warnings
                .last()
                .unwrap_or_else(|| panic!("missing warning: {text}"));
            assert_eq!(warning.code, WarningCode::Syntax);
            assert_eq!(warning.line, Some(if prefix.is_empty() { 2 } else { 3 }));
        }
    }
}

// --- nesting and parser containment ----------------------------------------------

#[test]
fn nested_commands_warn_without_aborting_small_stack_loads() {
    let path = fixture("deep-commands.bib");
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            for reload in [false, true] {
                let loaded = if reload {
                    ibis_sources::reload_file(&path)
                } else {
                    ibis_sources::load_file(&path)
                }
                .unwrap_or_else(|e| panic!("{e}"));
                assert_eq!(keys(&loaded), ["deep", "retained"]);
                assert_eq!(text(&loaded, "deep", "title"), None);
                assert_eq!(year(&loaded, "deep"), Some(2024));
                assert_eq!(text(&loaded, "deep", "publisher"), Some("Synthetic Press"));
                assert_eq!(text(&loaded, "retained", "title"), Some("Retained"));
                assert_eq!(loaded.warnings.len(), 1, "{:?}", loaded.warnings);
                assert_eq!(loaded.warnings[0].code, WarningCode::Field);
                assert_eq!(loaded.warnings[0].line, Some(2));
            }
        })
        .unwrap_or_else(|e| panic!("{e}"))
        .join()
        .unwrap_or_else(|e| std::panic::resume_unwind(e));
}

#[test]
fn nesting_boundary_is_safe_on_a_two_mib_stack() {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            for depth in [255, 256, 257] {
                let value = format!("{}x{}", "\\emph{".repeat(depth), "}".repeat(depth));
                let loaded = load(&format!("@article{{k,title={{{value}}},year=2024}}"));
                assert_eq!(keys(&loaded), ["k"]);
                assert_eq!(year(&loaded, "k"), Some(2024));
                if depth <= 256 {
                    assert!(text(&loaded, "k", "title").is_some_and(|title| title.contains('x')));
                    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
                } else {
                    assert_eq!(text(&loaded, "k", "title"), None);
                    assert_eq!(loaded.warnings.len(), 1);
                }
            }
        })
        .unwrap_or_else(|e| panic!("{e}"))
        .join()
        .unwrap_or_else(|e| std::panic::resume_unwind(e));
}

// Generated parser guards run as unit tests in `bibtex::tests`, where they
// also assert the private count of caught parser panics stays zero.

// --- value structure -------------------------------------------------------------

#[test]
fn unbalanced_values_never_create_metadata_or_hide_safe_data() {
    let loaded = load(&fixture_text("unbalanced-values.bib"));
    assert_eq!(records(&loaded).len(), 6, "{:?}", loaded.warnings);
    for (key, line) in [("paren-quoted", 2), ("paren-braced", 6), ("brace-braced", 7)] {
        assert_eq!(text(&loaded, key, "title"), None, "{key}");
        assert!(csl(&loaded, key).get("author").is_none(), "{key}");
        assert!(csl(&loaded, key).get("issued").is_none(), "{key}");
        assert_eq!(text(&loaded, key, "publisher"), Some("Synthetic Press"));
        assert_eq!(warned_at(&loaded, WarningCode::Field, line), 1, "{key}");
    }
    for key in ["safe-quoted", "safe-braced"] {
        assert_eq!(plain(&loaded, key, "title").as_deref(), Some("Literal {braces}"));
        assert_eq!(year(&loaded, key), Some(2024));
    }
    assert_eq!(text(&loaded, "neighbor", "title"), Some("Retained"));
    assert_eq!(loaded.warnings.len(), 4, "{:?}", loaded.warnings);
    assert_eq!(warned_at(&loaded, WarningCode::Syntax, 4), 1);
    assert!(serde_json::to_vec(&loaded.snapshot).is_ok());
}

#[test]
fn expanded_and_inherited_values_cannot_escape_known_or_unknown_fields() {
    for field in ["title", "journal", "note", "unknown", "url", "author", "date"] {
        for written in [r#""x}, date = {1999-01-01}, author = {Evil Person}""#, "bad # {}"] {
            let source = format!(
                "@string(bad=\"x}}, date = {{1999-01-01}}, author = {{Evil Person}}\")\n\
                 @xdata(p,{field}={written},publisher={{Synthetic Press}})\n\
                 @article{{k,xdata={{p}}}}\n@article{{neighbor,title={{Retained}}}}"
            );
            let loaded = load(&source);
            assert_eq!(keys(&loaded), ["k", "neighbor"], "{field}: {:?}", loaded.warnings);
            assert!(csl(&loaded, "k").get("author").is_none(), "{field}");
            assert!(csl(&loaded, "k").get("issued").is_none(), "{field}");
            assert_eq!(text(&loaded, "k", "title"), None, "{field}");
            assert_eq!(text(&loaded, "k", "publisher"), Some("Synthetic Press"));
            assert_eq!(text(&loaded, "neighbor", "title"), Some("Retained"));
            assert_eq!(loaded.warnings.len(), 1, "{field}: {:?}", loaded.warnings);
            assert_eq!(loaded.warnings[0].code, WarningCode::Field);
            assert_eq!(loaded.warnings[0].line, Some(2));
        }
    }
}

// --- dates ---------------------------------------------------------------------------

#[test]
fn malformed_separate_date_parts_retain_references_with_warnings() {
    let loaded = load(&fixture_text("malformed-date-parts.bib"));
    assert_eq!(keys(&loaded), ["embedded", "nonlatin", "zero"]);
    assert_eq!(loaded.warnings.len(), 3, "{:?}", loaded.warnings);
    assert!(all_field(&loaded));
    assert!(serde_json::to_vec(&loaded.snapshot).is_ok());
}

#[test]
fn oversized_uncertain_years_keep_the_reference_and_warn() {
    let loaded = load(&fixture_text("oversized-range-year.bib"));
    let all = records(&loaded);
    assert_eq!(all.len(), 1, "the invalid date must not discard the reference");
    assert!(!all[0].csl.contains_key("issued"));
    assert!(loaded.warnings.iter().any(|w| w.code == WarningCode::Field));
    assert!(serde_json::to_vec(&loaded.snapshot).is_ok());
}

#[test]
fn dates_written_with_non_latin_digits_are_read_not_crashed_on() {
    let cases = [
        ("date = {٢٠٢٠-٠٣}", 2020, Some(3)),             // Arabic-Indic
        ("date = {۱۳۹۹}", 1399, None),                   // Extended Arabic-Indic (Persian)
        ("year = {২০২১}", 2021, None),                   // Bengali
        ("year = {२०१९}", 2019, None),                   // Devanagari
        ("date = {２０２２-１１-０５}", 2022, Some(11)), // Full-width
    ];
    for (field, expected_year, expected_month) in cases {
        let result = load(&format!("@article{{k, title = {{T}}, {field}}}"));
        assert_eq!(keys(&result), ["k"], "{field}: {:?}", result.warnings);
        assert_eq!(year(&result, "k"), Some(expected_year), "{field}");
        assert_eq!(month(&result, "k"), expected_month, "{field}");
    }
}

#[test]
fn minimized_signed_year_fuzz_input_keeps_its_reference_and_field_warnings() {
    let source = fixture_text("fuzz-signed-year.bib");
    assert_eq!(source.as_bytes(), b"@s{h u={},year={- 3},e=c}");
    let loaded = load(&source);
    assert_eq!(keys(&loaded), ["h"], "{:?}", loaded.warnings);
    assert!(csl(&loaded, "h").get("issued").is_none());
    assert!(!loaded.warnings.is_empty());
    assert!(all_field(&loaded));
}

#[test]
fn malformed_date_parts_keep_the_reference_and_warn_for_each_prefix() {
    for (prefix, csl_field) in PREFIXES {
        for parts in [
            format!("{prefix}year=2020,{prefix}month={{0}}"),
            format!("{prefix}year=2020,{prefix}month={{Ⅻ}}"),
            format!("{prefix}year=2020,{prefix}month={{123}}"),
            format!("{prefix}year={{- 2024}}"),
            format!("{prefix}year={{+ 2024}}"),
        ] {
            let loaded = load(&format!("@article{{kept,title={{Retained}},{parts}}}"));
            assert_eq!(keys(&loaded), ["kept"], "{parts}");
            assert_eq!(bib::text(&loaded, "kept", "title"), Some("Retained"));
            assert!(!loaded.warnings.is_empty(), "{parts}: missing warning");
            assert!(all_field(&loaded), "{parts}: {:?}", loaded.warnings);
            if parts.contains("month") {
                assert_eq!(date_parts(&loaded, "kept", csl_field), json!([[2020]]), "{parts}");
            }
        }
        if !prefix.is_empty() {
            for month in ["٣", "0001", "001"] {
                let loaded = load(&format!(
                    "@article{{kept,title={{Retained}},{prefix}year=2020,{prefix}month={{{month}}}}}"
                ));
                assert_eq!(keys(&loaded), ["kept"]);
                assert!(loaded.warnings.iter().any(|w| w.code == WarningCode::Field));
                assert_eq!(date_parts(&loaded, "kept", csl_field), json!([[2020]]));
            }
        }
    }
}
