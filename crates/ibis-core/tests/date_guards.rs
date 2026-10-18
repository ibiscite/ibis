//! Synthetic malformed dates must behave identically in debug and release.
use ibis_core::{bibtex::load_str, model::WarningKind};

#[test]
fn minimized_signed_year_fuzz_input_keeps_its_reference_and_field_warnings() {
    let source = include_str!("../../../fixtures/bibtex/fuzz-signed-year.bib");
    assert_eq!(source.as_bytes(), b"@s{h u={},year={- 3},e=c}");
    let loaded = load_str("synthetic.bib", source);
    assert_eq!(loaded.references.len(), 1, "{:?}", loaded.warnings);
    assert_eq!(loaded.references[0].id.native_key, "h");
    assert!(loaded.references[0].issued.is_none());
    assert!(!loaded.warnings.is_empty());
    assert!(loaded.warnings.iter().all(|warning| warning.kind == WarningKind::Field));
}

#[test]
fn separated_year_signs_do_not_become_caught_parser_panics() {
    for prefix in ["", "url", "orig", "event"] {
        for sign in ["-", "+"] {
            let text = format!("@article{{kept,title={{Retained}},{prefix}year={{{sign} 2024}}}}");
            let loaded = load_str("synthetic.bib", &text);
            assert_eq!(loaded.references.len(), 1, "{prefix}year");
            assert!(loaded.warnings.iter().any(|warning| warning.kind == WarningKind::Field));
            assert!(loaded.warnings.iter().all(|warning| warning.kind == WarningKind::Field));
        }
    }
}

#[test]
fn malformed_date_parts_keep_the_reference_and_warn_for_each_prefix() {
    for (prefix, csl) in [
        ("", "issued"),
        ("url", "accessed"),
        ("orig", "original-date"),
        ("event", "event-date"),
    ] {
        for parts in [
            format!("{prefix}year=2020,{prefix}month={{0}}"),
            format!("{prefix}year=2020,{prefix}month={{Ⅻ}}"),
            format!("{prefix}year=2020,{prefix}month={{123}}"),
            format!("{prefix}year={{- 2024}}"),
            format!("{prefix}year={{+ 2024}}"),
        ] {
            let loaded = load_str("synthetic.bib", &format!("@article{{kept,title={{Retained}},{parts}}}"));
            assert_eq!(loaded.references.len(), 1, "{parts}");
            assert_eq!(loaded.references[0].title.as_deref(), Some("Retained"));
            assert!(!loaded.warnings.is_empty(), "{parts}: missing warning");
            assert!(
                loaded.warnings.iter().all(|warning| warning.kind == WarningKind::Field),
                "{parts}: {:?}",
                loaded.warnings
            );
            if parts.contains("month") {
                assert_eq!(
                    loaded.references[0].to_csl_json()[csl]["date-parts"],
                    serde_json::json!([[2020]]),
                    "{parts}"
                );
            }
        }
        if !prefix.is_empty() {
            for month in ["٣", "0001", "001"] {
                let text = format!("@article{{kept,title={{Retained}},{prefix}year=2020,{prefix}month={{{month}}}}}");
                let loaded = load_str("synthetic.bib", &text);
                assert_eq!(loaded.references.len(), 1);
                assert!(loaded.warnings.iter().any(|warning| warning.kind == WarningKind::Field));
                assert_eq!(
                    loaded.references[0].to_csl_json()[csl]["date-parts"],
                    serde_json::json!([[2020]])
                );
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
            let loaded = load_str(
                "synthetic.bib",
                &format!("@article{{kept,title={{Retained}},{field}={{{value}}}}}"),
            );
            assert_eq!(loaded.references.len(), 1, "{field}");
            assert_eq!(loaded.references[0].title.as_deref(), Some("Retained"));
            assert!(!loaded.warnings.is_empty(), "{field}");
            assert!(
                loaded.warnings.iter().all(|warning| warning.kind == WarningKind::Field),
                "{:?}",
                loaded.warnings
            );
        }
    }
}

#[test]
fn unsafe_months_preserve_safe_signed_short_and_era_years() {
    for (prefix, csl) in [
        ("", "issued"),
        ("url", "accessed"),
        ("orig", "original-date"),
        ("event", "event-date"),
    ] {
        for (year, expected) in [("2024 BC", -2023), ("-2024", -2024), ("+2024", 2024), ("24", 24)] {
            for month in ["Ⅻ", "123"] {
                let source = format!("@article{{kept,{prefix}year={{{year}}},{prefix}month={{{month}}}}}");
                let loaded = load_str("synthetic.bib", &source);
                assert_eq!(loaded.references.len(), 1);
                assert!(loaded.warnings.iter().any(|warning| warning.kind == WarningKind::Field));
                assert!(loaded.warnings.iter().all(|warning| warning.kind == WarningKind::Field));
                assert_eq!(
                    loaded.references[0].to_csl_json()[csl]["date-parts"],
                    serde_json::json!([[expected]]),
                    "{prefix}year={year}; month={month}"
                );
            }
        }
    }
}

#[test]
fn ordinary_signed_years_and_named_months_keep_their_precision() {
    for (prefix, csl) in [
        ("", "issued"),
        ("url", "accessed"),
        ("orig", "original-date"),
        ("event", "event-date"),
    ] {
        for (year, expected) in [("-2024", -2024), ("+2024", 2024)] {
            let loaded = load_str(
                "synthetic.bib",
                &format!("@article{{kept,{prefix}year={{{year}}},{prefix}month={{March 2}}}}"),
            );
            assert_eq!(loaded.references.len(), 1);
            assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
            assert_eq!(
                loaded.references[0].to_csl_json()[csl]["date-parts"],
                serde_json::json!([[expected, 3, 2]])
            );
        }
    }
}
