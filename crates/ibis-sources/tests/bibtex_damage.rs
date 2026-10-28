//! Damaged, malformed or hostile BibTeX keeps every good record, never
//! invents metadata, and reports each problem on the line it came from.
//! Synthetic data only.

#[path = "support/bib.rs"]
mod bib;

use bib::{all_field, csl, date, date_parts, keys, load, month, plain, records, text, warned_at, year};
use ibis_sources::WarningCode;
use proptest::prelude::*;
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

#[test]
fn prose_and_malformed_type_text_inside_values_remain_comments_or_data() {
    let loaded = load(
        "Contact student@example.invalid. @not-an-entry\n\
         @comment{Example: { @art}icle(k3, title={T3})}\n\
         @misc{kept, title={Example { @art}icle(k3, title={T3})}}",
    );
    assert_eq!(keys(&loaded), ["kept"]);
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
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

#[test]
fn deeply_nested_braces_and_long_lines_are_handled() {
    let nested = format!(
        "@article{{deep, title = {{{}x{}}}}}",
        "{".repeat(5_000),
        "}".repeat(5_000)
    );
    let _ = load(&nested);
    let long = format!("@article{{long, title = {{{}}}}}", "a".repeat(1_000_000));
    assert_eq!(keys(&load(&long)), ["long"]);
}

#[test]
fn field_text_containing_parser_error_is_not_a_caught_panic() {
    let loaded = load("@article{k,title={Retained},date={parser error}}");
    assert_eq!(keys(&loaded), ["k"]);
    assert!(loaded.warnings.iter().any(|w| w.code == WarningCode::Field));
    // A contained parser panic is reported as a syntax problem on the entry.
    assert!(loaded.warnings.iter().all(|w| w.code != WarningCode::Syntax));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn arbitrary_text_never_panics(text in any::<String>()) {
        let _ = load(&text);
    }

    #[test]
    fn mutated_entries_never_panic(
        prefix in "\\PC{0,40}",
        field in prop::sample::select(vec!["title", "crossref", "xdata", "author", "editor", "date", "year", "month", "journal", "pages", "langid"]),
        value in "\\PC{0,40}",
        closer in prop::sample::select(vec!["}", ")", "", "}}", "{"]),
    ) {
        let text = format!("{prefix}@article{{k, {field} = {{{value}}}{closer}\n@book(b, title={{{value}}})");
        let _ = load(&text);
    }
}

// --- value structure -------------------------------------------------------------

#[test]
fn unbalanced_day_is_reported_before_date_normalization() {
    for (source, line) in [
        (r#"@article(k,title={Safe},year=2024,month=3,day="x}")"#, 1),
        (
            "@string(bad=\"x}\")\n@xdata(p,year=2024,month=3,day=bad)\n@article{k,title={Safe},xdata={p}}",
            3,
        ),
    ] {
        let loaded = load(source);
        assert_eq!(keys(&loaded), ["k"]);
        assert_eq!(text(&loaded, "k", "title"), Some("Safe"));
        assert_eq!(year(&loaded, "k"), Some(2024));
        assert_eq!(month(&loaded, "k"), Some(3));
        assert_eq!(loaded.warnings.len(), 1, "{:?}", loaded.warnings);
        assert_eq!(loaded.warnings[0].code, WarningCode::Field);
        assert_eq!(loaded.warnings[0].line, Some(line));
    }
}

#[test]
fn cached_malformed_parents_and_macros_keep_each_consumers_warning() {
    for (definition, fields) in [
        (r#"@xdata(p,title="x}",publisher={Synthetic Press})"#, "xdata={p}"),
        (r#"@string(bad="x}")"#, "title=bad,publisher={Synthetic Press}"),
    ] {
        let source = format!(
            "{definition}\n@article(a,{fields},year=2024)\n@article(b,{fields},year=2024)\n\
             @string(open={{{{\\}}}})\n@string(close={{\\{{}}}})\n\
             @article{{safe,title={{Literal \\{{braces\\}}}},note=open # close,year=2025}}"
        );
        let loaded = load(&source);
        assert_eq!(keys(&loaded), ["a", "b", "safe"], "{:?}", loaded.warnings);
        assert_eq!(loaded.warnings.len(), 2, "{:?}", loaded.warnings);
        for (index, key, line) in [(0, "a", 2), (1, "b", 3)] {
            assert_eq!(text(&loaded, key, "title"), None);
            assert!(csl(&loaded, key).get("author").is_none());
            assert_eq!(year(&loaded, key), Some(2024));
            assert_eq!(text(&loaded, key, "publisher"), Some("Synthetic Press"));
            let warning = &loaded.warnings[index];
            assert_eq!(warning.code, WarningCode::Field);
            assert_eq!(warning.line, Some(line));
        }
        assert_eq!(plain(&loaded, "safe", "title").as_deref(), Some("Literal {braces}"));
        assert_eq!(text(&loaded, "safe", "note"), Some("}{"));
    }
}

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
            assert_eq!(loaded.warnings[0].line, Some(3));
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
fn a_separate_day_is_kept_when_valid_and_named_when_not() {
    let loaded = load(
        "@article{bad, title={T}, year={2024}, month={3}, day={32}}\n\
         @article{short, title={T}, year={95}, month={3}, day={5}}\n\
         @article{nomonth, title={T}, year={2024}, day={5}}\n\
         @article{season, title={T}, year={2024}, month={Spring}, day={5}}\n\
         @article{feb31, title={T}, year={95}, month={2}, day={31}}\n\
         @article{signed, title={T}, year={-50}, month={4}, day={31}}\n\
         @article{leap, title={T}, year={2024}, month={2}, day={29}}\n\
         @xdata{parent, date={2024-02-29}}\n\
         @article{child, title={T}, year={95}, xdata={parent}}\n\
         @article{era, title={T}, year={2024 BC}, month={2}, day={29}}\n\
         @xdata{plain, year={2023}, month={2}, day={29}}\n\
         @article{override, title={T}, year={2024}, xdata={plain}}\n\
         @article{spaced, title={T}, year={- 2024}, month={2}, day={29}}\n\
         @article{braced, title={T}, year={{2023}}, month={2}, day={29}}\n\
         @article{named, title={T}, year={2024}, month={February}, day={5}}\n\
         @xdata{wrong, year={2024}, month={3}, day={32}}\n\
         @article{inherits, title={T}, xdata={wrong}}\n\
         @xdata{dayonly, day={29}}\n\
         @article{supplies, title={T}, year={2024}, month={2}, xdata={dayonly}}\n\
         @book{dated, title={B}, date={2024-03}}\n\
         @xdata{dayfive, day={05}}\n\
         @inbook{mixed, title={T}, crossref={dated}, xdata={dayfive}}\n\
         @xdata{daybad, day={32}}\n\
         @inbook{mixedbad, title={T}, crossref={dated}, xdata={daybad}}\n\
         @article{grouped, title={T}, year={2024}, month={3}, day={0{5}}}\n\
         @article{good, title={T}, year={2024}, month=mar, day={5}}",
    );
    let issued = |key: &str| date(&loaded, key, "issued");
    assert_eq!(issued("bad"), Some((2024, Some(3), None)));
    assert_eq!(issued("short"), Some((95, Some(3), Some(5))));
    assert_eq!(issued("nomonth"), Some((2024, None, None)));
    assert_eq!(issued("good"), Some((2024, Some(3), Some(5))));
    assert_eq!(issued("leap"), Some((2024, Some(2), Some(29))));
    // The entry's own year decides whether its parent's 29 February exists.
    assert_eq!(issued("override"), Some((2024, Some(2), Some(29))));
    assert_eq!(issued("named"), Some((2024, Some(2), Some(5))));
    assert_eq!(issued("supplies"), Some((2024, Some(2), Some(29))));
    assert_eq!(issued("mixed"), Some((2024, Some(3), Some(5))));
    assert_eq!(issued("grouped"), Some((2024, Some(3), Some(5))));
    assert!(bib::find(&loaded, "spaced").is_some(), "{:?}", loaded.warnings);
    // Warnings carry the entry's line: one per malformed or impossible day.
    let warned = |line: usize| warned_at(&loaded, WarningCode::Field, line);
    assert_eq!(warned(1), 1, "bad: {:?}", loaded.warnings);
    assert_eq!(warned(3), 1, "nomonth: {:?}", loaded.warnings);
    assert!(warned(4) >= 1, "season: {:?}", loaded.warnings);
    for (key, line) in [
        ("short", 2),
        ("override", 12),
        ("named", 15),
        ("supplies", 19),
        ("mixed", 22),
        ("grouped", 25),
        ("good", 26),
    ] {
        assert_eq!(warned(line), 0, "{key}: {:?}", loaded.warnings);
    }
    for (key, line) in [
        ("feb31", 5),
        ("signed", 6),
        ("child", 9),
        ("era", 10),
        ("braced", 14),
        ("inherits", 17),
        ("mixedbad", 24),
    ] {
        assert_eq!(issued(key).and_then(|(_, _, day)| day), None, "{key}");
        assert_eq!(warned(line), 1, "{key}: {:?}", loaded.warnings);
    }
}

#[test]
fn unsafe_separate_parts_are_checked_after_macro_expansion_for_every_date_prefix() {
    for (prefix, csl_field) in PREFIXES {
        for month in [
            "0",
            "00",
            "13",
            "99",
            " 0 trailing",
            "١",
            "²",
            "March 256",
            "March 999999999999999999999",
            "3 256",
            "000 256",
        ] {
            let text = format!(
                "@string{{bad={{{month}}}}}\n@xdata{{p,{prefix}year={{2024}},{prefix}month=bad}}\n@article{{k,title={{Retained}},xdata={{p}}}}"
            );
            let loaded = load(&text);
            assert_eq!(keys(&loaded), ["k"], "{prefix}month={month}");
            assert_eq!(
                date(&loaded, "k", csl_field).map(|(year, _, _)| year),
                Some(2024),
                "{prefix}month={month}"
            );
            if prefix.is_empty() && month == "١" {
                assert!(loaded.warnings.is_empty());
                assert_eq!(date_parts(&loaded, "k", csl_field), json!([[2024, 1]]));
            } else {
                assert!(!loaded.warnings.is_empty(), "{prefix}month={month}");
            }
            assert!(all_field(&loaded));
        }
    }
}

#[test]
fn safe_separate_parts_and_explicit_date_precedence_keep_their_precision() {
    for (prefix, csl_field) in PREFIXES {
        for parts in [
            format!("{prefix}year={{2024}},{prefix}month={{3}},{prefix}day={{2}}"),
            format!("{prefix}year={{2024}},{prefix}month={{3 2}}"),
            format!("{prefix}date={{2024-03-02}},{prefix}year={{2020}},{prefix}month={{0}}"),
        ] {
            let loaded = load(&format!("@article{{k,{parts}}}"));
            assert_eq!(keys(&loaded), ["k"]);
            assert_eq!(date_parts(&loaded, "k", csl_field), json!([[2024, 3, 2]]), "{parts}");
            assert!(loaded.warnings.is_empty(), "{parts}: {:?}", loaded.warnings);
        }
    }
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
fn malformed_uncertain_dates_are_guarded_after_expansion_in_all_date_fields() {
    for (field, csl_field) in [
        ("date", "issued"),
        ("urldate", "accessed"),
        ("origdate", "original-date"),
        ("eventdate", "event-date"),
    ] {
        for (value, retained_year) in [
            ("20190x", None),
            (" 20190X", None),
            ("999999999999999999999999X", None),
            ("٢٠١٩٠x", None),
            ("2019-00-XX", Some(2019)),
            ("2019 - 00 - XX", Some(2019)),
            ("2019--00-XX", Some(2019)),
            ("2019 - 00garbageX", Some(2019)),
        ] {
            let text = format!(
                "@string{{bad={{{value}}}}}\n@xdata{{p,{field}=bad}}\n@article{{k,title={{Kept}},xdata={{p}}}}"
            );
            let loaded = load(&text);
            assert_eq!(keys(&loaded), ["k"], "{field} = {value}");
            assert_eq!(bib::text(&loaded, "k", "title"), Some("Kept"));
            assert_eq!(
                date(&loaded, "k", csl_field).map(|(year, _, _)| year),
                retained_year,
                "{field} = {value}"
            );
            assert_eq!(
                warned_at(&loaded, WarningCode::Field, 3),
                1,
                "{field} = {value}: {:?}",
                loaded.warnings
            );
        }
    }
}

#[test]
fn valid_uncertain_years_keep_their_precision() {
    for (value, first, last) in [
        ("20XX", 2000, Some(2099)),
        ("2024", 2024, None),
        ("2024-XX", 2024, Some(2024)),
    ] {
        let loaded = load(&format!("@article{{k,date={{{value}}}}}"));
        assert_eq!(keys(&loaded), ["k"]);
        assert_eq!(year(&loaded, "k"), Some(first), "{value}");
        let end = date_parts(&loaded, "k", "issued")
            .get(1)
            .and_then(|end| end.get(0))
            .and_then(serde_json::Value::as_i64);
        assert_eq!(end, last, "{value}");
        assert!(loaded.warnings.is_empty(), "{value}: {:?}", loaded.warnings);
    }
}

#[test]
fn letters_that_change_length_when_lowercased_do_not_shift_positions() {
    // 'ẞ' (3 bytes) lowercases to 'ß' (2 bytes); 'İ' (2 bytes) to "i̇" (3 bytes).
    for prefix in ["ẞẞẞẞẞẞ", "İİİİİİ", "ΣΣΣ", "Kelvin K"] {
        let text = format!(
            "@inproceedings{{a, title = {{{prefix}}}, crossref = {{p}}}}\n@proceedings{{p, title = {{Parent}}, year = {{2020}}}}\n"
        );
        let result = load(&text);
        assert_eq!(keys(&result), ["a", "p"], "{prefix}: {:?}", result.warnings);
        assert_eq!(year(&result, "a"), Some(2020), "{prefix}: crossref year not inherited");
    }
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
fn separated_year_signs_do_not_become_caught_parser_panics() {
    for (prefix, _) in PREFIXES {
        for sign in ["-", "+"] {
            let loaded = load(&format!(
                "@article{{kept,title={{Retained}},{prefix}year={{{sign} 2024}}}}"
            ));
            assert_eq!(keys(&loaded), ["kept"], "{prefix}year");
            assert!(loaded.warnings.iter().any(|w| w.code == WarningCode::Field));
            assert!(all_field(&loaded));
        }
    }
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

#[test]
fn non_decimal_numerals_in_explicit_dates_are_reported_without_panics() {
    for field in ["date", "urldate", "origdate", "eventdate"] {
        for value in [
            "²",
            "2020-²",
            "2020-03-²",
            "2020-03-02T²",
            "2020-03-02T01:²",
            "2020-03-02T01:02:²",
            "2020-03-02T01:02:03+²",
            "2020-03-02T01:02:03+01:²",
        ] {
            let loaded = load(&format!("@article{{kept,title={{Retained}},{field}={{{value}}}}}"));
            assert_eq!(keys(&loaded), ["kept"], "{field}");
            assert_eq!(bib::text(&loaded, "kept", "title"), Some("Retained"));
            assert!(!loaded.warnings.is_empty(), "{field}");
            assert!(all_field(&loaded), "{:?}", loaded.warnings);
        }
    }
}

#[test]
fn unsafe_months_preserve_safe_signed_short_and_era_years() {
    for (prefix, csl_field) in PREFIXES {
        for (year_text, expected) in [("2024 BC", -2023), ("-2024", -2024), ("+2024", 2024), ("24", 24)] {
            for month in ["Ⅻ", "123"] {
                let loaded = load(&format!(
                    "@article{{kept,{prefix}year={{{year_text}}},{prefix}month={{{month}}}}}"
                ));
                assert_eq!(keys(&loaded), ["kept"]);
                assert!(loaded.warnings.iter().any(|w| w.code == WarningCode::Field));
                assert!(all_field(&loaded));
                assert_eq!(
                    date_parts(&loaded, "kept", csl_field),
                    json!([[expected]]),
                    "{prefix}year={year_text}; month={month}"
                );
            }
        }
    }
}

#[test]
fn ordinary_signed_years_and_named_months_keep_their_precision() {
    for (prefix, csl_field) in PREFIXES {
        for (year_text, expected) in [("-2024", -2024), ("+2024", 2024)] {
            let loaded = load(&format!(
                "@article{{kept,{prefix}year={{{year_text}}},{prefix}month={{March 2}}}}"
            ));
            assert_eq!(keys(&loaded), ["kept"]);
            assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
            assert_eq!(date_parts(&loaded, "kept", csl_field), json!([[expected, 3, 2]]));
        }
    }
}
