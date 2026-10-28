//! Legacy key-output examples replayed through the shared citation contract.
//! Each case is what a writer would paste into a document.
//! The cases come from the old `cite::render` tests; current contract spelling,
//! typed losses and refusals are asserted explicitly.
#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use ibis_core::contract::{GoldenCase, OutputSpec};
use ibis_core::syntax;
use proptest::prelude::*;

fn seed() -> GoldenCase {
    serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap()
}

/// An old `KeyFormat`, as the key syntax and command listed in `syntax::CAPABILITIES`.
type Format = (KeySyntax, Option<&'static str>);
const PANDOC: Format = (KeySyntax::Pandoc, None);
const AUTOCITE: Format = (KeySyntax::Biblatex, Some("autocite"));
const PARENCITE: Format = (KeySyntax::Biblatex, Some("parencite"));
const TEXTCITE: Format = (KeySyntax::Biblatex, Some("textcite"));
const CITE: Format = (KeySyntax::Latex, Some("cite"));
const TYPST: Format = (KeySyntax::Typst, None);
const KEYS: Format = (KeySyntax::Keys, None);
const ALL: [Format; 7] = [PANDOC, AUTOCITE, PARENCITE, TEXTCITE, CITE, TYPST, KEYS];

/// One cited reference, spelled as the old `CiteItem`.
#[derive(Debug, Clone, Default)]
struct Cite {
    key: String,
    locator: Option<Locator>,
    prefix: Option<String>,
    suffix: Option<String>,
    suppress_author: bool,
}

fn item(key: &str) -> Cite {
    Cite {
        key: key.to_string(),
        ..Cite::default()
    }
}

fn pages(key: &str, value: &str) -> Cite {
    Cite {
        locator: Some(Locator {
            term: LocatorTerm::Page,
            value: value.to_string(),
        }),
        ..item(key)
    }
}

fn runs(text: &Option<String>) -> RichText {
    text.iter()
        .map(|text| TextRun {
            text: text.clone(),
            ..TextRun::default()
        })
        .collect()
}

/// One synthetic seed record per distinct key; repeated keys cite the same record.
fn cite(format: Format, items: &[Cite]) -> Result<CitationResult, CitationError> {
    let case = seed();
    let LibraryObject::Record(template) = &case.library.objects()[0].object else {
        panic!("synthetic seed record");
    };
    let mut records: Vec<Record> = Vec::new();
    let mut request = case.request.clone();
    request.output = OutputSpec::Key {
        syntax: format.0,
        command: format.1.map(str::to_string),
    };
    request.items.clear();
    for cited in items {
        let index = match records
            .iter()
            .position(|record| record.citation_keys[0].value == cited.key)
        {
            Some(index) => index,
            None => {
                let mut record = template.clone();
                record.id.key = format!("work-{}", records.len() + 1);
                record.citation_keys[0].value = cited.key.clone();
                records.push(record);
                records.len() - 1
            }
        };
        request.items.push(CitationItem {
            item: records[index].id.clone(),
            locator: cited.locator.clone(),
            prefix: runs(&cited.prefix),
            suffix: runs(&cited.suffix),
            suppress_author: cited.suppress_author,
            ..case.request.items[0].clone()
        });
    }
    let snapshot = Snapshot::new(records.into_iter().map(LibraryObject::Record).collect()).unwrap();
    syntax::cite(&snapshot, &request)
}

fn rendered(format: Format, items: &[Cite]) -> CitationResult {
    let result = cite(format, items).unwrap();
    assert!(
        result.copy_content(items.len()).is_some(),
        "{format:?}: {:?}",
        result.outcomes
    );
    result
}

fn text(format: Format, items: &[Cite]) -> String {
    rendered(format, items).text
}

fn refused(format: Format, items: &[Cite]) -> Vec<ItemOutcome> {
    let result = cite(format, items).unwrap();
    assert!(
        result.copy_content(items.len()).is_none(),
        "{format:?}: {}",
        result.text
    );
    result.outcomes
}

#[test]
fn empty_locators_are_refused_before_any_syntax_can_drop_them() {
    for format in ALL {
        for value in ["", " ", "\u{a0}", "\u{2003}"] {
            assert_eq!(
                refused(format, &[pages("example", value)]),
                [ItemOutcome::Refused {
                    item: 0,
                    error: CitationError::InvalidLocator
                }],
                "{format:?} {value:?}"
            );
        }
    }
}

#[test]
fn repeated_and_unusual_keys_keep_syntax_and_input_identity() {
    // Repeated keys remain distinct choices; Unicode and punctuation must not
    // be decorated before each syntax makes its escaping decisions.
    let items = [item("same"), item("same"), item("é.")];
    for (format, expected) in [
        (PANDOC, "[@same; @same; @{é.}]"),
        (AUTOCITE, "\\autocites{same}{same}{é.}"),
        (PARENCITE, "\\parencites{same}{same}{é.}"),
        (TEXTCITE, "\\textcites{same}{same}{é.}"),
        (CITE, "\\cite{same,same,é.}"),
        (TYPST, "@same @same #cite(label(\"é.\"))"),
        (KEYS, "same, same, é."),
    ] {
        let result = rendered(format, &items);
        assert_eq!(result.text, expected);
        assert_eq!(
            result.outcomes,
            [0, 1, 2].map(|item| ItemOutcome::Rendered { item }),
            "{format:?}"
        );
        assert_eq!(
            result.display.nodes,
            [DisplayNode {
                item: None,
                content: vec![TextRun {
                    text: expected.into(),
                    ..TextRun::default()
                }],
            }]
        );
    }
}

#[test]
fn biblatex_multicite_keeps_a_prefix_inside_its_reference() {
    let items = [
        Cite {
            prefix: Some("see &".into()),
            ..pages("a", "12")
        },
        pages("b", "24–26"),
    ];
    assert_eq!(text(AUTOCITE, &items), "\\autocites[see \\&][12]{a}[24--26]{b}");
}

#[test]
fn pandoc_separate_locators_prefix_and_suppressed_author() {
    let see = Cite {
        prefix: Some("see".into()),
        ..pages("smith2020", "12")
    };
    let year_only = Cite {
        suppress_author: true,
        ..pages("doe:2019", "24–26")
    };
    assert_eq!(
        text(PANDOC, &[see, year_only]),
        "[see @smith2020, p. 12; -@doe:2019, pp. 24-26]"
    );
}

#[test]
fn pandoc_wraps_unusual_keys_in_braces() {
    assert_eq!(text(PANDOC, &[item("Smith_2020.")]), "[@{Smith_2020.}]");
    assert_eq!(text(PANDOC, &[item("a--b")]), "[@{a--b}]");
}

#[test]
fn pandoc_quotes_alphabetic_combining_marks_in_keys() {
    // Rust's Unicode Alphabetic property includes these marks and symbols, but
    // Pandoc's bare citation-key parser stops before them. With `[@key]`, the
    // remaining text becomes a suffix and silently cites a different key.
    for key in [
        "synthetic\u{0345}2024",
        "synthetic\u{05b0}2024",
        "synthetic\u{093e}2024",
        "synthetic\u{24b6}2024",  // Circled alphabetic symbol, not a mark.
        "synthetic\u{1f130}2024", // Squared alphabetic symbol, not a mark.
    ] {
        let result = rendered(PANDOC, &[item(key)]);
        assert_eq!(result.text, format!("[@{{{key}}}]"), "key {key:?}");
        assert!(result.losses.is_empty());
        assert!(result.notices.is_empty());
    }
}

#[test]
fn biblatex_multicite_keeps_one_locator_per_reference() {
    let items = [pages("a", "12"), pages("b", "24-26")];
    assert_eq!(text(AUTOCITE, &items), "\\autocites[12]{a}[24--26]{b}");
}

#[test]
fn biblatex_prefix_needs_both_brackets() {
    let see = Cite {
        prefix: Some("see".into()),
        ..item("a")
    };
    assert_eq!(text(PARENCITE, &[see]), "\\parencite[see][]{a}");
    let chapter = Cite {
        locator: Some(Locator {
            term: LocatorTerm::Chapter,
            value: "3".into(),
        }),
        ..item("a")
    };
    assert_eq!(text(TEXTCITE, &[chapter]), "\\textcite[chap.~3]{a}");
}

#[test]
fn biblatex_suppressed_author_uses_star_form_for_one_reference() {
    let one = Cite {
        suppress_author: true,
        ..item("a")
    };
    assert_eq!(text(AUTOCITE, std::slice::from_ref(&one)), "\\autocite*{a}");
    assert_eq!(
        rendered(AUTOCITE, &[one, item("b")]).losses,
        [Loss {
            item: Some(0),
            field: LossField::AuthorSuppression,
            reason: LossReason::Unsupported
        }]
    );
}

#[test]
fn keys_that_break_latex_are_refused_for_latex_but_allowed_elsewhere() {
    for key in ["50%off", "a#b", "back\\slash", "til~de"] {
        for format in [AUTOCITE, CITE] {
            assert_eq!(
                refused(format, &[item(key)]),
                [ItemOutcome::Refused {
                    item: 0,
                    error: CitationError::InvalidKey
                }],
                "{format:?} {key}"
            );
        }
        for format in [PANDOC, TYPST] {
            rendered(format, &[item(key)]);
        }
    }
}

#[test]
fn plain_cite_groups_keys_or_splits_when_locators_differ() {
    assert_eq!(text(CITE, &[item("a"), item("b")]), "\\cite{a,b}");
    assert_eq!(
        text(CITE, &[pages("a", "12"), pages("b", "24-26")]),
        "\\cite[p.~12]{a}, \\cite[pp.~24--26]{b}"
    );
}

#[test]
fn latex_special_characters_in_notes_are_escaped() {
    let note = Cite {
        suffix: Some("50% & more_".into()),
        ..item("a")
    };
    assert_eq!(text(AUTOCITE, &[note]), "\\autocite[50\\% \\& more\\_\\nopp]{a}");
}

#[test]
fn typst_simple_and_complex_labels() {
    assert_eq!(
        text(TYPST, &[pages("smith2020", "12"), item("doe")]),
        "@smith2020[p. 12] @doe"
    );
    assert_eq!(text(TYPST, &[item("odd/key")]), "#cite(label(\"odd/key\"))");
    let year_only = Cite {
        suppress_author: true,
        ..item("a")
    };
    assert_eq!(text(TYPST, &[year_only]), "#cite(<a>, form: \"year\")");
}

#[test]
/// A key with a dot uses an explicit label: Typst reads `@a.b[...]` as key `a`.
fn typst_notes_escape_comment_markers() {
    let notes = Cite {
        prefix: Some("see // note".into()),
        suffix: Some("and/or // x /* y */".into()),
        ..pages("a.b", "12")
    };
    let complex = Cite {
        prefix: Some("/* see */".into()),
        suffix: Some("/// end".into()),
        suppress_author: true,
        ..pages("odd/key", "12//13")
    };
    assert_eq!(
        text(TYPST, &[notes, complex]),
        r#"see \// note #cite(label("a.b"), supplement: [p. 12, and/or \// x \/\* y \*/]) \/\* see \*/ #cite(label("odd/key"), supplement: [p. 12\//13, \/\// end], form: "year")"#
    );
}

#[test]
fn typst_quotes_keys_outside_the_safe_literal_subset() {
    let keys = ["x²", "x½", ".key", ":key", "café", "key\"quote", "back\\slash"];
    let items: Vec<_> = keys.into_iter().map(item).collect();
    assert_eq!(
        text(TYPST, &items),
        r#"#cite(label("x²")) #cite(label("x½")) #cite(label(".key")) #cite(label(":key")) #cite(label("café")) #cite(label("key\"quote")) #cite(label("back\\slash"))"#
    );
}

fn typst_prefix(prefix: &str) -> String {
    text(
        TYPST,
        &[Cite {
            prefix: Some(prefix.to_owned()),
            ..item("synthetic2024")
        }],
    )
}

fn typst_suffix(suffix: &str) -> String {
    text(
        TYPST,
        &[Cite {
            suffix: Some(suffix.to_owned()),
            ..item("synthetic2024")
        }],
    )
}

#[test]
fn typst_prefix_escapes_heading_and_list_markers() {
    // Pandoc 3.1.11.1's Typst reader parses the unescaped forms as a Header,
    // OrderedList, BulletList and DefinitionList, respectively. The prefix is
    // user text, not a request to turn the citation into one of those blocks.
    for (prefix, escaped) in [
        ("= synthetic note", "\\= synthetic note"),
        ("+ synthetic note", "\\+ synthetic note"),
        ("/ synthetic: note", "\\/ synthetic: note"),
        ("1. synthetic note", "1\\. synthetic note"),
        ("note\u{2028}= heading", "note\u{2028}\\= heading"),
    ] {
        assert_eq!(typst_prefix(prefix), format!("{escaped} @synthetic2024"));
    }
}

#[test]
/// Text with a hyphen becomes a Typst string, so it can never start a list.
fn typst_prefix_escapes_hyphen_list_markers() {
    let (actual, expected): (Vec<_>, Vec<_>) = [
        ("- synthetic note", "#text(\"- synthetic note\")"),
        ("  - synthetic note", "#text(\"  - synthetic note\")"),
        ("note\u{2029}- item", "#text(\"note\u{2029}- item\")"),
    ]
    .into_iter()
    .map(|(prefix, escaped)| (typst_prefix(prefix), format!("{escaped} @synthetic2024")))
    .unzip();
    assert_eq!(actual, expected);
}

#[test]
fn typst_supplements_escape_block_markers() {
    for (note, escaped) in [
        ("= note", "\\= note"),
        ("+ note", "\\+ note"),
        ("/ term: note", "\\/ term: note"),
        ("12. note", "12\\. note"),
    ] {
        assert_eq!(typst_suffix(note), format!("@synthetic2024[{escaped}]"));
    }
}

#[test]
fn typst_supplements_keep_hyphen_text_literal() {
    let (actual, expected): (Vec<_>, Vec<_>) = [
        ("- note", "#text(\"- note\")"),
        (
            "pre-existing and/or 2024-10-19",
            "#text(\"pre-existing and/or 2024-10-19\")",
        ),
    ]
    .into_iter()
    .map(|(note, escaped)| (typst_suffix(note), format!("@synthetic2024[{escaped}]")))
    .unzip();
    assert_eq!(actual, expected);
}

#[test]
fn invalid_input_is_rejected_not_guessed() {
    assert_eq!(
        refused(CITE, &[item("a b")]),
        [ItemOutcome::Refused {
            item: 0,
            error: CitationError::InvalidKey
        }]
    );
    let control = Cite {
        suffix: Some("x\u{7}".into()),
        ..item("a")
    };
    assert_eq!(
        refused(PANDOC, &[control]),
        [ItemOutcome::Refused {
            item: 0,
            error: CitationError::InvalidRichText
        }]
    );
}

proptest! {
    /// Any valid key appears in every output and rendering never panics.
    #[test]
    fn every_valid_key_is_present(key in "[A-Za-z0-9_][A-Za-z0-9_:./-]{0,20}", page in "[0-9]{1,4}(-[0-9]{1,4})?") {
        for format in [PANDOC, AUTOCITE, CITE, TYPST, KEYS] {
            let result = cite(format, &[pages(&key, &page)]);
            prop_assert!(
                result.as_ref().is_ok_and(|result| result.copy_content(1).is_some() && result.text.contains(&key)),
                "{:?} lost key {}: {:?}", format, key, result
            );
        }
    }
}
