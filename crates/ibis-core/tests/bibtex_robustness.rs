//! The BibTeX reader must never panic, whatever bytes it is given.

use ibis_core::bibtex::load_str;
use proptest::prelude::*;

#[test]
fn malformed_separate_date_parts_retain_references_with_warnings() {
    let loaded = load_str(
        "synthetic.bib",
        include_str!("../../../fixtures/bibtex/malformed-date-parts.bib"),
    );
    assert_eq!(loaded.references.len(), 3);
    assert_eq!(loaded.warnings.len(), 3);
    assert!(
        loaded
            .warnings
            .iter()
            .all(|warning| warning.kind == ibis_core::model::WarningKind::Field)
    );
    assert!(ibis_core::library::csl_items(&loaded.references).is_ok());
}

#[test]
fn unsafe_separate_parts_are_checked_after_macro_expansion_for_every_date_prefix() {
    for (prefix, csl_field) in [
        ("", "issued"),
        ("url", "accessed"),
        ("orig", "original-date"),
        ("event", "event-date"),
    ] {
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
            let loaded = load_str("synthetic.bib", &text);
            assert_eq!(loaded.references.len(), 1, "{prefix}month={month}");
            assert_eq!(
                loaded.references[0].to_csl_json()[csl_field]["date-parts"][0][0].as_i64(),
                Some(2024),
                "{prefix}month={month}"
            );
            if prefix.is_empty() && month == "١" {
                assert!(loaded.warnings.is_empty());
                assert_eq!(
                    loaded.references[0].to_csl_json()[csl_field]["date-parts"],
                    serde_json::json!([[2024, 1]])
                );
            } else {
                assert!(!loaded.warnings.is_empty(), "{prefix}month={month}");
            }
            assert!(
                loaded
                    .warnings
                    .iter()
                    .all(|warning| warning.kind == ibis_core::model::WarningKind::Field)
            );
        }
    }
}

#[test]
fn safe_separate_parts_and_explicit_date_precedence_keep_their_precision() {
    for (prefix, csl_field) in [
        ("", "issued"),
        ("url", "accessed"),
        ("orig", "original-date"),
        ("event", "event-date"),
    ] {
        for parts in [
            format!("{prefix}year={{2024}},{prefix}month={{3}},{prefix}day={{2}}"),
            format!("{prefix}year={{2024}},{prefix}month={{3 2}}"),
            format!("{prefix}date={{2024-03-02}},{prefix}year={{2020}},{prefix}month={{0}}"),
        ] {
            let loaded = load_str("synthetic.bib", &format!("@article{{k,{parts}}}"));
            assert_eq!(loaded.references.len(), 1);
            assert_eq!(
                loaded.references[0].to_csl_json()[csl_field]["date-parts"],
                serde_json::json!([[2024, 3, 2]]),
                "{parts}"
            );
            assert!(loaded.warnings.is_empty(), "{parts}: {:?}", loaded.warnings);
        }
    }
}

#[test]
fn oversized_uncertain_years_keep_the_reference_and_warn() {
    let fixture = include_str!("../../../fixtures/bibtex/oversized-range-year.bib");
    let loaded = load_str("synthetic.bib", fixture);
    assert_eq!(
        loaded.references.len(),
        1,
        "the invalid date must not discard the reference"
    );
    assert!(loaded.references[0].issued.is_none());
    assert!(loaded.warnings.iter().any(|warning| warning.message.contains("date")));
    assert!(ibis_core::library::csl_items(&loaded.references).is_ok());
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
            let loaded = load_str("synthetic.bib", &text);
            assert_eq!(loaded.references.len(), 1, "{field} = {value}");
            assert_eq!(loaded.references[0].title.as_deref(), Some("Kept"));
            let csl = loaded.references[0].to_csl_json();
            assert_eq!(
                csl[csl_field]["date-parts"][0][0].as_i64(),
                retained_year,
                "{field} = {value}"
            );
            assert!(
                loaded.warnings.iter().any(|warning| warning.message.contains("date")),
                "{field} = {value}"
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
        let loaded = load_str("synthetic.bib", &format!("@article{{k,date={{{value}}}}}"));
        assert_eq!(loaded.references.len(), 1);
        let issued = loaded.references[0]
            .issued
            .unwrap_or_else(|| panic!("valid date: {value}"));
        assert_eq!(issued.start.year, first);
        assert_eq!(issued.end.map(|end| end.year), last);
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
        let result = load_str("t", &text);
        assert_eq!(result.references.len(), 2, "{prefix}: {:?}", result.warnings);
        assert_eq!(
            result.references[0].year(),
            Some(2020),
            "{prefix}: crossref year not inherited"
        );
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
    for (field, year, month) in cases {
        let result = load_str("t", &format!("@article{{k, title = {{T}}, {field}}}"));
        assert_eq!(result.references.len(), 1, "{field}: {:?}", result.warnings);
        let issued = result.references[0].issued;
        assert_eq!(issued.map(|d| d.start.year), Some(year), "{field}");
        assert_eq!(issued.and_then(|d| d.start.month), month, "{field}");
    }
}

#[test]
fn deeply_nested_braces_and_long_lines_are_handled() {
    let nested = format!(
        "@article{{deep, title = {{{}x{}}}}}",
        "{".repeat(5_000),
        "}".repeat(5_000)
    );
    let _ = load_str("t", &nested);
    let long = format!("@article{{long, title = {{{}}}}}", "a".repeat(1_000_000));
    assert_eq!(load_str("t", &long).references.len(), 1);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn arbitrary_text_never_panics(text in any::<String>()) {
        let _ = load_str("t", &text);
    }

    #[test]
    fn mutated_entries_never_panic(
        prefix in "\\PC{0,40}",
        field in prop::sample::select(vec!["title", "crossref", "xdata", "author", "editor", "date", "year", "month", "journal", "pages", "langid"]),
        value in "\\PC{0,40}",
        closer in prop::sample::select(vec!["}", ")", "", "}}", "{"]),
    ) {
        let text = format!("{prefix}@article{{k, {field} = {{{value}}}{closer}\n@book(b, title={{{value}}})");
        let _ = load_str("t", &text);
    }
}
