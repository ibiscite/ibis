//! Org, MyST, MultiMarkdown, AsciiDoc, Jupyter, Jekyll Scholar and Cite as You
//! Write JSON render from the shared request, with typed losses (#1207).
#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use ibis_core::syntax;

fn seed() -> GoldenCase {
    serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap()
}

fn plain(text: &str) -> RichText {
    vec![TextRun {
        text: text.into(),
        ..TextRun::default()
    }]
}

fn locator(term: LocatorTerm, value: &str) -> Option<Locator> {
    Some(Locator {
        term,
        value: value.into(),
    })
}

fn render(syntax: KeySyntax, change: impl FnOnce(&mut CitationRequest)) -> CitationResult {
    let mut case = seed();
    case.request.output = OutputSpec::Key { syntax, command: None };
    change(&mut case.request);
    syntax::cite(&case.library, &case.request).unwrap()
}

fn two(request: &mut CitationRequest) {
    request.items.push(request.items[0].clone());
}

fn noted(request: &mut CitationRequest) {
    let item = &mut request.items[0];
    item.prefix = plain("see");
    item.locator = locator(LocatorTerm::Page, "12-14");
    item.suffix = plain("emphasis added");
}

fn fields(result: &CitationResult) -> Vec<LossField> {
    result.losses.iter().map(|loss| loss.field).collect()
}

#[test]
fn every_new_syntax_renders_one_plain_reference() {
    for (syntax, expected) in [
        (KeySyntax::OrgCite, "[cite:@Example2026]"),
        (KeySyntax::Myst, "{cite:p}`Example2026`"),
        (KeySyntax::MultiMarkdown, "[#Example2026][]"),
        (KeySyntax::AsciiDoc, "cite:[Example2026]"),
        (KeySyntax::Jupyter, "<cite data-cite=\"Example2026\">Example2026</cite>"),
        (KeySyntax::JekyllScholar, "{% cite Example2026 %}"),
        (KeySyntax::CaywJson, r#"[{"citationKey":"Example2026"}]"#),
    ] {
        let result = render(syntax, |_| {});
        assert_eq!(result.text, expected, "{syntax:?}");
        assert!(result.losses.is_empty(), "{syntax:?}");
        assert!(result.copy_content(1).is_some(), "{syntax:?}");
    }
}

#[test]
fn notes_go_where_each_syntax_carries_them_and_the_rest_are_losses() {
    use LossField::{Prefix, Suffix};
    for (syntax, expected, lost) in [
        (
            KeySyntax::OrgCite,
            "[cite:see @Example2026, pp. 12–14, emphasis added]",
            vec![],
        ),
        (
            KeySyntax::Myst,
            "{cite:p}`Example2026`",
            vec![LossField::Locator, Prefix, Suffix],
        ),
        (
            KeySyntax::MultiMarkdown,
            "[pp. 12–14, emphasis added][#Example2026]",
            vec![Prefix],
        ),
        (KeySyntax::AsciiDoc, "cite:[Example2026(12–14)]", vec![Prefix, Suffix]),
        (
            KeySyntax::JekyllScholar,
            "{% cite Example2026 --locator 12-14 %}",
            vec![Prefix, Suffix],
        ),
        (
            KeySyntax::CaywJson,
            r#"[{"citationKey":"Example2026","label":"page","locator":"12-14","prefix":"see","suffix":"emphasis added"}]"#,
            vec![],
        ),
    ] {
        let result = render(syntax, noted);
        assert_eq!(result.text, expected, "{syntax:?}");
        assert_eq!(fields(&result), lost, "{syntax:?}");
    }
}

#[test]
fn several_references_stay_one_citation() {
    for (syntax, expected) in [
        (KeySyntax::OrgCite, "[cite:@Example2026; @Example2026]"),
        (KeySyntax::Myst, "{cite:p}`Example2026,Example2026`"),
        (KeySyntax::MultiMarkdown, "[#Example2026][][#Example2026][]"),
        (KeySyntax::AsciiDoc, "cite:[Example2026, Example2026]"),
        (KeySyntax::JekyllScholar, "{% cite Example2026 Example2026 %}"),
    ] {
        assert_eq!(render(syntax, two).text, expected, "{syntax:?}");
    }
    // Jekyll Scholar pairs locators with keys by position only.
    let result = render(KeySyntax::JekyllScholar, |request| {
        two(request);
        request.items[1].locator = locator(LocatorTerm::Page, "3");
    });
    assert_eq!(result.text, "{% cite Example2026 Example2026 %}");
    assert_eq!(
        result.losses,
        [Loss {
            item: Some(1),
            field: LossField::Locator,
            reason: LossReason::Unsupported
        }]
    );
}

#[test]
fn org_and_myst_forms_use_their_own_styles() {
    for (form, expected) in [
        (CitationForm::Narrative, "[cite/t:@Example2026]"),
        (CitationForm::AuthorOnly, "[cite/a:@Example2026]"),
        (CitationForm::YearOnly, "[cite/na:@Example2026]"),
    ] {
        assert_eq!(render(KeySyntax::OrgCite, |r| r.form = form).text, expected);
        let refused = render(KeySyntax::OrgCite, |r| {
            r.form = form;
            two(r);
        });
        assert!(
            refused.copy_content(2).is_none(),
            "{form:?} applies to the whole Org citation"
        );
    }
    let suppressed = render(KeySyntax::OrgCite, |r| r.items[0].suppress_author = true);
    assert_eq!(suppressed.text, "[cite/na:@Example2026]");
    let narrative = render(KeySyntax::Myst, |r| {
        r.form = CitationForm::Narrative;
        two(r);
    });
    assert_eq!(narrative.text, "{cite:t}`Example2026,Example2026`");
    assert!(
        render(KeySyntax::Myst, |r| r.form = CitationForm::YearOnly)
            .copy_content(1)
            .is_none()
    );
}

#[test]
fn other_locator_types_keep_their_label() {
    let chapter = |r: &mut CitationRequest| r.items[0].locator = locator(LocatorTerm::Chapter, "3");
    assert_eq!(render(KeySyntax::OrgCite, chapter).text, "[cite:@Example2026, chap. 3]");
    let refused = render(KeySyntax::AsciiDoc, chapter);
    assert_eq!(
        refused.outcomes,
        [ItemOutcome::Refused {
            item: 0,
            error: CitationError::InvalidLocator
        }]
    );
    assert!(refused.copy_content(1).is_none());
    assert_eq!(
        render(KeySyntax::JekyllScholar, chapter).text,
        "{% cite Example2026 --label chapter --locator 3 %}"
    );
    let sub_verbo = render(KeySyntax::CaywJson, |r| {
        r.items[0].locator = locator(LocatorTerm::SubVerbo, "lex");
    });
    assert_eq!(
        sub_verbo.text,
        r#"[{"citationKey":"Example2026","label":"sub-verbo","locator":"lex"}]"#
    );
}

#[test]
fn asciidoc_refuses_every_non_page_locator() {
    for term in LocatorTerm::ALL.into_iter().filter(|term| *term != LocatorTerm::Page) {
        let result = render(KeySyntax::AsciiDoc, |r| r.items[0].locator = locator(term, "3"));
        assert_eq!(
            result.outcomes,
            [ItemOutcome::Refused {
                item: 0,
                error: CitationError::InvalidLocator
            }]
        );
        assert!(result.copy_content(1).is_none());
    }
}

#[test]
fn text_a_syntax_would_read_as_markup_is_a_typed_loss_never_markup() {
    for hostile in [
        "see /this/ one",
        "a_b",
        "x^2",
        "a -- b",
        "wait...",
        "@other",
        "a; b",
        "[[link]]",
        "\\alpha",
        "$x$",
        "*bold*",
        "<<target>>",
    ] {
        let result = render(KeySyntax::OrgCite, |r| r.items[0].prefix = plain(hostile));
        assert_eq!(result.text, "[cite:@Example2026]", "{hostile}");
        assert_eq!(fields(&result), [LossField::Prefix], "{hostile}");
    }
    for safe in ["and/or", "cf. a+b", "1=1 ok", "l'été (2)"] {
        let result = render(KeySyntax::OrgCite, |r| r.items[0].prefix = plain(safe));
        assert_eq!(result.text, format!("[cite:{safe} @Example2026]"));
        assert!(result.losses.is_empty(), "{safe}");
    }
    for hostile in ["a [b]", "*x*", "it's", "\"q\"", "a -- b", "<b>", "x & y"] {
        let result = render(KeySyntax::MultiMarkdown, |r| r.items[0].suffix = plain(hostile));
        assert_eq!(result.text, "[#Example2026][]", "{hostile}");
        assert_eq!(fields(&result), [LossField::Suffix], "{hostile}");
    }
}

#[test]
fn an_org_suffix_that_reads_as_a_locator_is_not_carried() {
    for suffix in ["12", "12 more", "iv more", "X more", "p. 4 below", "chapter 3"] {
        for located in [false, true] {
            let result = render(KeySyntax::OrgCite, |r| {
                r.items[0].suffix = plain(suffix);
                if located {
                    r.items[0].locator = locator(LocatorTerm::Page, "4");
                }
            });
            let expected = if located {
                "[cite:@Example2026, p. 4]"
            } else {
                "[cite:@Example2026]"
            };
            assert_eq!(result.text, expected, "{suffix}");
            assert_eq!(fields(&result), [LossField::Suffix], "{suffix}");
        }
    }
}

#[test]
fn mixed_myst_forms_never_share_the_first_references_role() {
    for first in [CitationForm::Normal, CitationForm::Narrative] {
        let result = render(KeySyntax::Myst, |r| {
            r.form = first;
            two(r);
            r.items[1].form = Some(if first == CitationForm::Normal {
                CitationForm::Narrative
            } else {
                CitationForm::Normal
            });
        });
        assert_eq!(result.outcomes[0], ItemOutcome::Rendered { item: 0 });
        assert_eq!(
            result.outcomes[1],
            ItemOutcome::Refused {
                item: 1,
                error: CitationError::UnsupportedForm,
            }
        );
        assert!(result.copy_content(2).is_none());
    }
}

#[test]
fn styled_affixes_flatten_with_a_rich_text_loss() {
    let result = render(KeySyntax::CaywJson, |r| {
        r.items[0].prefix = vec![TextRun {
            text: "see".into(),
            italic: true,
            ..TextRun::default()
        }];
    });
    assert_eq!(result.text, r#"[{"citationKey":"Example2026","prefix":"see"}]"#);
    assert_eq!(fields(&result), [LossField::RichText]);
}

#[test]
fn keys_and_locators_a_syntax_cannot_hold_are_refused() {
    let mut case = seed();
    let library = |key: &str| {
        let mut objects: Vec<_> = case.library.objects().iter().map(|item| item.object.clone()).collect();
        let LibraryObject::Record(record) = &mut objects[0] else {
            panic!("synthetic seed record");
        };
        record.citation_keys[0].value = key.into();
        Snapshot::new(objects).unwrap()
    };
    for (syntax, key) in [
        (KeySyntax::Myst, "a`b"),
        (KeySyntax::MultiMarkdown, "a]b"),
        (KeySyntax::AsciiDoc, "a(b"),
        (KeySyntax::JekyllScholar, "-flag"),
        (KeySyntax::JekyllScholar, "50%off"),
        (KeySyntax::OrgCite, "a;b"),
        (KeySyntax::OrgCite, "a,b"),
        (KeySyntax::OrgCite, "a\"b"),
        (KeySyntax::OrgCite, "a\\b"),
        (KeySyntax::OrgCite, "a=b"),
    ] {
        let snapshot = library(key);
        case.request.output = OutputSpec::Key { syntax, command: None };
        let result = syntax::cite(&snapshot, &case.request).unwrap();
        assert_eq!(
            result.outcomes,
            [ItemOutcome::Refused {
                item: 0,
                error: CitationError::InvalidKey
            }],
            "{syntax:?} {key}"
        );
    }
    let jupyter = {
        let snapshot = library("a\"b<c>&'");
        case.request.output = OutputSpec::Key {
            syntax: KeySyntax::Jupyter,
            command: None,
        };
        syntax::cite(&snapshot, &case.request).unwrap()
    };
    assert_eq!(
        jupyter.text,
        "<cite data-cite=\"a&quot;b&lt;c&gt;&amp;&#39;\">a&quot;b&lt;c&gt;&amp;&#39;</cite>"
    );
    for (syntax, value) in [
        (KeySyntax::AsciiDoc, "1(2)"),
        (KeySyntax::JekyllScholar, "12 14"),
        (KeySyntax::OrgCite, "a_b"),
        (KeySyntax::MultiMarkdown, "[3]"),
    ] {
        let result = render(syntax, |r| r.items[0].locator = locator(LocatorTerm::Page, value));
        assert_eq!(
            result.outcomes,
            [ItemOutcome::Refused {
                item: 0,
                error: CitationError::InvalidLocator
            }],
            "{syntax:?} {value}"
        );
    }
}

#[test]
fn pandoc_reads_org_citations_as_the_same_picks() {
    use std::io::Write;
    let available = std::process::Command::new("pandoc")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !available {
        return;
    }
    let result = render(KeySyntax::OrgCite, |r| {
        noted(r);
        two(r);
        r.items[1].prefix = plain("and/or cf.");
    });
    let mut child = std::process::Command::new("pandoc")
        .args(["-f", "org", "-t", "json"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(result.text.as_bytes()).unwrap();
    let ast: serde_json::Value = serde_json::from_slice(&child.wait_with_output().unwrap().stdout).unwrap();
    let cite = &ast["blocks"][0]["c"][0];
    assert_eq!(cite["t"], "Cite", "{}", result.text);
    let citations = cite["c"][0].as_array().unwrap();
    assert_eq!(citations.len(), 2, "{}", result.text);
    let text = |value: &serde_json::Value| {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|inline| match inline["t"].as_str() {
                Some("Str") => inline["c"].as_str().unwrap().to_string(),
                _ => " ".into(),
            })
            .collect::<String>()
    };
    assert_eq!(citations[0]["citationId"], "Example2026");
    assert_eq!(text(&citations[0]["citationPrefix"]), "see");
    assert_eq!(
        text(&citations[0]["citationSuffix"]).trim(),
        ", pp. 12–14, emphasis added"
    );
    assert_eq!(text(&citations[1]["citationPrefix"]), "and/or cf.");
}
