//! Key output through the shared citation contract: pasted text, losses and refusals.
#![cfg(test)]
use ibis_core::contract::*;
use ibis_core::contract::{GoldenCase, OutputSpec};
use ibis_core::syntax;

fn seed() -> GoldenCase {
    serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap()
}

/// A key output, as the key syntax and command listed in `syntax::CAPABILITIES`.
type Format = (KeySyntax, Option<&'static str>);
const PANDOC: Format = (KeySyntax::Pandoc, None);
const AUTOCITE: Format = (KeySyntax::Biblatex, Some("autocite"));
const PARENCITE: Format = (KeySyntax::Biblatex, Some("parencite"));
const TEXTCITE: Format = (KeySyntax::Biblatex, Some("textcite"));
const CITE: Format = (KeySyntax::Latex, Some("cite"));
const TYPST: Format = (KeySyntax::Typst, None);
const KEYS: Format = (KeySyntax::Keys, None);
const ALL: [Format; 7] = [PANDOC, AUTOCITE, PARENCITE, TEXTCITE, CITE, TYPST, KEYS];

/// One reference with its citation details.
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

fn typst_prefix(prefix: &str) -> String {
    text(
        TYPST,
        &[Cite {
            prefix: Some(prefix.to_owned()),
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
