//! Specification by example for every key-based output syntax.
//! Each case is what a writer would paste into their document.

use ibis_core::cite::{CiteError, CiteItem, KeyFormat, Locator, LocatorLabel, render};
use proptest::prelude::*;

fn item(key: &str) -> CiteItem {
    CiteItem {
        key: key.to_string(),
        ..CiteItem::default()
    }
}

fn pages(key: &str, value: &str) -> CiteItem {
    CiteItem {
        locator: Some(Locator {
            label: LocatorLabel::Page,
            value: value.to_string(),
        }),
        ..item(key)
    }
}

fn text(format: KeyFormat, items: &[CiteItem]) -> String {
    match render(format, items) {
        Ok(r) => r.text,
        Err(e) => panic!("render failed: {e}"),
    }
}

#[test]
fn pandoc_separate_locators_prefix_and_suppressed_author() {
    let see = CiteItem {
        prefix: Some("see".into()),
        ..pages("smith2020", "12")
    };
    let year_only = CiteItem {
        suppress_author: true,
        ..pages("doe:2019", "24–26")
    };
    assert_eq!(
        text(KeyFormat::Pandoc, &[see, year_only]),
        "[see @smith2020, p. 12; -@doe:2019, pp. 24-26]"
    );
}

#[test]
fn pandoc_wraps_unusual_keys_in_braces() {
    assert_eq!(text(KeyFormat::Pandoc, &[item("Smith_2020.")]), "[@{Smith_2020.}]");
    assert_eq!(text(KeyFormat::Pandoc, &[item("a--b")]), "[@{a--b}]");
}

#[test]
fn biblatex_multicite_keeps_one_locator_per_reference() {
    let items = [pages("a", "12"), pages("b", "24-26")];
    assert_eq!(text(KeyFormat::Autocite, &items), "\\autocites[12]{a}[24--26]{b}");
}

#[test]
fn biblatex_prefix_needs_both_brackets() {
    let see = CiteItem {
        prefix: Some("see".into()),
        ..item("a")
    };
    assert_eq!(text(KeyFormat::Parencite, &[see]), "\\parencite[see][]{a}");
    let chapter = CiteItem {
        locator: Some(Locator {
            label: LocatorLabel::Chapter,
            value: "3".into(),
        }),
        ..item("a")
    };
    assert_eq!(text(KeyFormat::Textcite, &[chapter]), "\\textcite[chap.~3]{a}");
}

#[test]
fn biblatex_suppressed_author_uses_star_form_for_one_reference() {
    let one = CiteItem {
        suppress_author: true,
        ..item("a")
    };
    assert_eq!(text(KeyFormat::Autocite, std::slice::from_ref(&one)), "\\autocite*{a}");
    let many = render(KeyFormat::Autocite, &[one, item("b")]).map(|r| r.warnings.len());
    assert_eq!(many, Ok(1));
}

#[test]
fn textcite_cannot_hide_the_author_and_says_so() {
    let year_only = CiteItem {
        suppress_author: true,
        ..item("a")
    };
    let rendered = render(KeyFormat::Textcite, &[year_only]).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(rendered.text, "\\textcite{a}");
    assert_eq!(rendered.warnings.len(), 1);
}

#[test]
fn keys_that_break_latex_are_refused_for_latex_but_allowed_elsewhere() {
    for key in ["50%off", "a#b", "back\\slash", "til~de"] {
        assert!(
            matches!(
                render(KeyFormat::Autocite, &[item(key)]),
                Err(CiteError::KeyNotValidFor(_, "LaTeX"))
            ),
            "{key}"
        );
        assert!(
            matches!(
                render(KeyFormat::Cite, &[item(key)]),
                Err(CiteError::KeyNotValidFor(_, "LaTeX"))
            ),
            "{key}"
        );
        assert!(render(KeyFormat::Pandoc, &[item(key)]).is_ok(), "{key}");
        assert!(render(KeyFormat::Typst, &[item(key)]).is_ok(), "{key}");
    }
}

#[test]
fn plain_cite_groups_keys_or_splits_when_locators_differ() {
    assert_eq!(text(KeyFormat::Cite, &[item("a"), item("b")]), "\\cite{a,b}");
    let rendered = render(KeyFormat::Cite, &[pages("a", "12"), pages("b", "24-26")]);
    let rendered = rendered.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(rendered.text, "\\cite[p.~12]{a}, \\cite[pp.~24--26]{b}");
    assert_eq!(rendered.warnings.len(), 1);
}

#[test]
fn latex_special_characters_in_notes_are_escaped() {
    let note = CiteItem {
        suffix: Some("50% & more_".into()),
        ..item("a")
    };
    assert_eq!(text(KeyFormat::Autocite, &[note]), "\\autocite[50\\% \\& more\\_]{a}");
}

#[test]
fn typst_simple_and_complex_labels() {
    assert_eq!(
        text(KeyFormat::Typst, &[pages("smith2020", "12"), item("doe")]),
        "@smith2020[p. 12] @doe"
    );
    assert_eq!(text(KeyFormat::Typst, &[item("odd/key")]), "#cite(label(\"odd/key\"))");
    let year_only = CiteItem {
        suppress_author: true,
        ..item("a")
    };
    assert_eq!(text(KeyFormat::Typst, &[year_only]), "#cite(<a>, form: \"year\")");
}

#[test]
fn invalid_input_is_rejected_not_guessed() {
    assert_eq!(render(KeyFormat::Pandoc, &[]), Err(CiteError::Empty));
    assert!(matches!(
        render(KeyFormat::Cite, &[item("a b")]),
        Err(CiteError::InvalidKey(_))
    ));
    let control = CiteItem {
        suffix: Some("x\u{7}".into()),
        ..item("a")
    };
    assert_eq!(render(KeyFormat::Pandoc, &[control]), Err(CiteError::ControlCharacter));
}

proptest! {
    /// Any valid key appears in every output and rendering never panics.
    #[test]
    fn every_valid_key_is_present(key in "[A-Za-z0-9_][A-Za-z0-9_:./-]{0,20}", page in "[0-9]{1,4}(-[0-9]{1,4})?") {
        for format in [KeyFormat::Pandoc, KeyFormat::Autocite, KeyFormat::Cite, KeyFormat::Typst, KeyFormat::Keys] {
            let rendered = render(format, &[pages(&key, &page)]);
            prop_assert!(rendered.as_ref().is_ok_and(|r| r.text.contains(&key)), "{:?} lost key {}: {:?}", format, key, rendered);
        }
    }
}
