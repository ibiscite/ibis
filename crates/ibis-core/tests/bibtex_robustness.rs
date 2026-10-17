//! The BibTeX reader must never panic, whatever bytes it is given.

use ibis_core::bibtex::load_str;
use proptest::prelude::*;

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
        assert_eq!(issued.map(|d| d.year), Some(year), "{field}");
        assert_eq!(issued.and_then(|d| d.month), month, "{field}");
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
        field in prop::sample::select(vec!["title", "crossref", "author", "date", "journal", "pages"]),
        value in "\\PC{0,40}",
        closer in prop::sample::select(vec!["}", ")", "", "}}", "{"]),
    ) {
        let text = format!("{prefix}@article{{k, {field} = {{{value}}}{closer}\n@book(b, title={{{value}}})");
        let _ = load_str("t", &text);
    }
}
