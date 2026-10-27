//! The new renderer consumes the shared contract without front-end formatting.
#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use ibis_core::contract::{GoldenCase, OutputSpec};
use ibis_core::syntax::{self, CAPABILITIES, Escaping};
use proptest::prelude::*;
use serde_json::{Value, json};

fn seed() -> GoldenCase {
    serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap()
}

fn output(caps: &syntax::SyntaxCapabilities) -> OutputSpec {
    OutputSpec::Key {
        syntax: caps.syntax,
        command: caps.command.map(str::to_string),
    }
}

fn changed(case: &GoldenCase, change: impl FnOnce(&mut Record)) -> Snapshot {
    let mut objects: Vec<_> = case.library.objects().iter().map(|item| item.object.clone()).collect();
    let LibraryObject::Record(record) = &mut objects[0] else {
        panic!("synthetic seed record");
    };
    change(record);
    Snapshot::new(objects).unwrap()
}

#[test]
fn key_golden_cases_render_exactly() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cases");
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let case: GoldenCase = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        if !matches!(case.request.output, OutputSpec::Key { .. }) || case.request.items.is_empty() {
            continue;
        }
        let actual = ibis_core::syntax::cite(&case.library, &case.request)
            .map_err(|error| format!("{}: {error:?}", case.name))?;
        assert_eq!(Some(&actual), case.expected.data(), "{}", case.name);
    }
    Ok(())
}

#[test]
fn capabilities_drive_forms_and_report_each_dropped_field() {
    let mut case = seed();
    for caps in &CAPABILITIES {
        case.request.output = output(caps);
        assert_eq!(syntax::capabilities(&case.request.output).unwrap(), caps);
        for form in [
            CitationForm::Normal,
            CitationForm::Narrative,
            CitationForm::AuthorOnly,
            CitationForm::YearOnly,
            CitationForm::Full,
            CitationForm::BibliographyOnly,
        ] {
            case.request.form = form;
            let result = syntax::cite(&case.library, &case.request).unwrap();
            assert_eq!(
                result.copy_content(1).is_some(),
                caps.forms.contains(&form),
                "{caps:?} {form:?}"
            );
            if !caps.forms.contains(&form) {
                assert_eq!(
                    result.outcomes,
                    [ItemOutcome::Refused {
                        item: 0,
                        error: CitationError::UnsupportedForm
                    }]
                );
            }
        }
    }
    case.request.form = CitationForm::Normal;
    case.request.output = OutputSpec::Key {
        syntax: KeySyntax::Keys,
        command: None,
    };
    let item = &mut case.request.items[0];
    item.locator = Some(Locator {
        term: LocatorTerm::Page,
        value: "12".into(),
    });
    item.prefix = vec![TextRun {
        text: "see".into(),
        italic: true,
        ..TextRun::default()
    }];
    item.suffix = vec![TextRun {
        text: "note".into(),
        ..TextRun::default()
    }];
    item.suppress_author = true;
    let result = syntax::cite(&case.library, &case.request).unwrap();
    assert_eq!(result.text, "Example2026");
    assert_eq!(
        result.losses.iter().map(|loss| loss.field).collect::<Vec<_>>(),
        [
            LossField::Locator,
            LossField::Prefix,
            LossField::Suffix,
            LossField::AuthorSuppression
        ]
    );
}

#[test]
fn commands_forms_and_partial_outcomes_never_change_intent_silently() {
    let mut case = seed();
    for (syntax, command) in [
        (KeySyntax::Biblatex, "autocite}\\input{bad"),
        (KeySyntax::Pandoc, "cite"),
        (KeySyntax::Latex, "input"),
    ] {
        case.request.output = OutputSpec::Key {
            syntax,
            command: Some(command.into()),
        };
        assert_eq!(
            syntax::cite(&case.library, &case.request),
            Err(CitationError::UnsupportedOutput)
        );
    }
    case.request.output = OutputSpec::Key {
        syntax: KeySyntax::Biblatex,
        command: Some("textcite".into()),
    };
    case.request.items[0].suppress_author = true;
    let result = syntax::cite(&case.library, &case.request).unwrap();
    assert_eq!(result.text, "\\textcite{Example2026}");
    assert_eq!(
        result.losses,
        [Loss {
            item: Some(0),
            field: LossField::AuthorSuppression,
            reason: LossReason::Unsupported
        }]
    );
    case.request.items[0].suppress_author = false;
    case.request.items.push(case.request.items[0].clone());
    case.request.items[0].expected_cite = Some(Fingerprint::of_bytes(b"invented stale fingerprint"));
    let result = syntax::cite(&case.library, &case.request).unwrap();
    assert_eq!(
        result.outcomes,
        [
            ItemOutcome::Refused {
                item: 0,
                error: CitationError::ChangedItem
            },
            ItemOutcome::Rendered { item: 1 }
        ]
    );
    assert_eq!(result.display.nodes[0].item, Some(1));
    assert!(result.copy_content(2).is_none());
    case.request.items[0].expected_cite = None;
    case.request.form = CitationForm::YearOnly;
    case.request.output = OutputSpec::Key {
        syntax: KeySyntax::Biblatex,
        command: Some("autocite".into()),
    };
    assert!(
        syntax::cite(&case.library, &case.request)
            .unwrap()
            .outcomes
            .iter()
            .all(|outcome| matches!(
                outcome,
                ItemOutcome::Refused {
                    error: CitationError::UnsupportedForm,
                    ..
                }
            ))
    );
}

#[test]
fn snapshot_identity_aliases_and_missing_keys_are_checked_before_rendering() {
    let case = seed();
    let first = case.library.objects()[0].object.clone();
    let LibraryObject::Record(mut other) = first.clone() else {
        panic!("seed");
    };
    other.id.library = LibraryId::zotero(b"invented second library");
    other.citation_keys[0].value = "Other2026".into();
    other.aliases.push("Example2026".into());
    let snapshot = Snapshot::new(vec![first, LibraryObject::Record(other)]).unwrap();
    assert_eq!(
        syntax::cite(&snapshot, &case.request).unwrap().outcomes,
        [ItemOutcome::Refused {
            item: 0,
            error: CitationError::AmbiguousKey
        }]
    );
    for (change, expected) in [
        (0, CitationError::MissingKey),
        (1, CitationError::InvalidKey),
        (2, CitationError::ConflictingKey),
    ] {
        let snapshot = changed(&case, |record| match change {
            0 => record.citation_keys.clear(),
            1 => record.citation_keys[0].value = "a b".into(),
            _ => record.citation_keys.push(CitationKey {
                value: "Other2026".into(),
                source: CitationKeySource::Extra,
            }),
        });
        assert_eq!(
            syntax::cite(&snapshot, &case.request).unwrap().outcomes,
            [ItemOutcome::Refused {
                item: 0,
                error: expected
            }]
        );
    }
}

#[test]
fn annotation_parent_page_and_fingerprint_are_part_of_citation_intent() {
    let mut case = seed();
    let mut objects: Vec<_> = case.library.objects().iter().map(|item| item.object.clone()).collect();
    let record = case.request.items[0].item.clone();
    let attachment = ItemRef {
        library: record.library.clone(),
        key: "attachment:invented".into(),
    };
    let annotation = ItemRef {
        library: record.library.clone(),
        key: "annotation:invented".into(),
    };
    objects.push(LibraryObject::Attachment(Attachment {
        id: attachment.clone(),
        parent: Some(record),
        title: "Synthetic attachment".into(),
        media_type: "application/pdf".into(),
        availability: Availability::Unknown,
        content_hash: None,
    }));
    objects.push(LibraryObject::Annotation(Annotation {
        id: annotation.clone(),
        parent: attachment,
        text: "Synthetic text".into(),
        comment: String::new(),
        page_label: Some("iv".into()),
        page_index: Some(3),
    }));
    let snapshot = Snapshot::new(objects).unwrap();
    case.request.items[0].annotation = Some(annotation.clone());
    case.request.items[0].expected_annotation_cite = Some(snapshot.get(&annotation).unwrap().fingerprints.cite.clone());
    case.request.items[0].locator = Some(Locator {
        term: LocatorTerm::Page,
        value: "iv".into(),
    });
    assert!(
        syntax::cite(&snapshot, &case.request)
            .unwrap()
            .copy_content(1)
            .is_some()
    );
    case.request.items[0].locator.as_mut().unwrap().value = "99".into();
    assert_eq!(
        syntax::cite(&snapshot, &case.request).unwrap().outcomes,
        [ItemOutcome::Refused {
            item: 0,
            error: CitationError::AnnotationMismatch
        }]
    );
    case.request.items[0].locator.as_mut().unwrap().value = "iv".into();
    case.request.items[0].expected_annotation_cite = Some(Fingerprint::of_bytes(b"old annotation"));
    assert_eq!(
        syntax::cite(&snapshot, &case.request).unwrap().outcomes,
        [ItemOutcome::Refused {
            item: 0,
            error: CitationError::ChangedItem
        }]
    );
}

#[test]
fn hostile_empty_and_large_inputs_have_bounded_fixed_diagnostics() {
    let mut case = seed();
    let mut request = case.request.clone();
    request.items.clear();
    assert_eq!(
        syntax::cite(&case.library, &request),
        Err(CitationError::EmptySelection)
    );
    request.items = vec![case.request.items[0].clone(); syntax::MAX_CITATION_ITEMS + 1];
    assert_eq!(syntax::cite(&case.library, &request), Err(CitationError::TooManyItems));
    case.request.items[0].prefix = vec![TextRun {
        text: "x".repeat(syntax::MAX_CITATION_BYTES + 1),
        ..TextRun::default()
    }];
    assert_eq!(
        syntax::cite(&case.library, &case.request),
        Err(CitationError::InvalidRichText)
    );
    case.request.items[0].prefix[0].text = "private synthetic\u{7}".into();
    assert_eq!(
        syntax::cite(&case.library, &case.request).unwrap().outcomes,
        [ItemOutcome::Refused {
            item: 0,
            error: CitationError::InvalidRichText
        }]
    );
    case.request.items[0].prefix.clear();
    let snapshot = changed(&case, |record| {
        record.csl.insert("abstract".into(), json!("x".repeat(1024 * 1024)));
    });
    case.request.output = OutputSpec::Key {
        syntax: KeySyntax::CslJson,
        command: None,
    };
    case.request.items = vec![case.request.items[0].clone(); 20];
    assert_eq!(
        syntax::cite(&snapshot, &case.request),
        Err(CitationError::OutputTooLarge)
    );
}

#[test]
fn rich_affixes_and_latex_prefixes_retain_their_position() {
    let mut case = seed();
    case.request.output = OutputSpec::Key {
        syntax: KeySyntax::Latex,
        command: None,
    };
    case.request.items[0].prefix = vec![TextRun {
        text: "see &".into(),
        italic: true,
        ..TextRun::default()
    }];
    case.request.items[0].suffix = vec![TextRun {
        text: "note".into(),
        bold: true,
        ..TextRun::default()
    }];
    assert_eq!(
        syntax::cite(&case.library, &case.request).unwrap().text,
        "\\emph{see \\&} \\cite[\\textbf{note}]{Example2026}"
    );
    case.request.output = OutputSpec::Key {
        syntax: KeySyntax::Pandoc,
        command: None,
    };
    case.request.items[0].prefix[0].script = Script::Superscript;
    let result = syntax::cite(&case.library, &case.request).unwrap();
    assert_eq!(
        result.losses,
        [Loss {
            item: Some(0),
            field: LossField::RichText,
            reason: LossReason::Flattened
        }]
    );
}

#[test]
fn typst_colon_keys_use_an_explicit_label_without_truncation() {
    let mut case = seed();
    case.request.output = OutputSpec::Key {
        syntax: KeySyntax::Typst,
        command: None,
    };
    let snapshot = changed(&case, |record| record.citation_keys[0].value = "a:0".into());
    assert_eq!(
        syntax::cite(&snapshot, &case.request).unwrap().text,
        "#cite(label(\"a:0\"))"
    );
}

fn citations(value: &Value, ids: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            if object.get("t").and_then(Value::as_str) == Some("Cite")
                && let Some(items) = value["c"][0].as_array()
            {
                for item in items {
                    ids.push(item["citationId"].as_str().unwrap_or_default().to_string());
                }
            }
            for child in object.values() {
                citations(child, ids);
            }
        }
        Value::Array(array) => {
            for child in array {
                citations(child, ids);
            }
        }
        _ => {}
    }
}

fn pandoc_available() -> bool {
    let available = std::process::Command::new("pandoc")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    assert!(
        available || std::env::var_os("IBIS_REQUIRE_PANDOC").is_none(),
        "Pandoc is required"
    );
    available
}

fn consumer_ast(format: &str, text: &str) -> Value {
    consumer_ast_with(format, text, &[])
}

fn consumer_ast_with(format: &str, text: &str, extra: &[&str]) -> Value {
    use std::io::Write;
    let mut child = std::process::Command::new("pandoc")
        .args(["-f", format, "-t", "json"])
        .args(extra)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(text.as_bytes()).unwrap();
    let parsed = child.wait_with_output().unwrap();
    assert!(parsed.status.success(), "{text}");
    serde_json::from_slice(&parsed.stdout).unwrap()
}

fn first_citation(value: &Value) -> Option<&Value> {
    match value {
        Value::Object(object) if object.get("t").and_then(Value::as_str) == Some("Cite") => {
            value["c"][0].as_array()?.first()
        }
        Value::Object(object) => object.values().find_map(first_citation),
        Value::Array(array) => array.iter().find_map(first_citation),
        _ => None,
    }
}

#[test]
fn prefix_support_matches_every_supported_output_and_form() {
    use syntax::FieldSupport::{Flattened, Native, Unsupported};
    let mut case = seed();
    case.request.items[0].locator = None;
    case.request.items[0].suffix.clear();
    case.request.items[0].prefix = vec![TextRun {
        text: "see".into(),
        ..TextRun::default()
    }];
    let consumer_available = pandoc_available();
    for caps in &CAPABILITIES {
        case.request.output = output(caps);
        assert_eq!(caps.prefix.len(), caps.forms.len(), "{caps:?}");
        for &form in caps.forms {
            case.request.form = form;
            let expected = match (caps.syntax, form) {
                (KeySyntax::Pandoc, CitationForm::Narrative) | (KeySyntax::Latex | KeySyntax::Typst, _) => Flattened,
                (KeySyntax::Keys | KeySyntax::CslJson, _) => Unsupported,
                _ => Native,
            };
            assert_eq!(caps.prefix_support(form), expected, "{caps:?} {form:?}");
            let result = syntax::cite(&case.library, &case.request).unwrap();
            let expected_losses = if expected == Native {
                vec![]
            } else {
                vec![Loss {
                    item: Some(0),
                    field: LossField::Prefix,
                    reason: if expected == Flattened {
                        LossReason::Flattened
                    } else {
                        LossReason::Unsupported
                    },
                }]
            };
            assert_eq!(result.losses, expected_losses, "{caps:?} {form:?}");
            if consumer_available && expected != Unsupported {
                let ast = consumer_ast(
                    match caps.escaping {
                        Escaping::Pandoc => "markdown",
                        Escaping::Latex => "latex",
                        _ => "typst",
                    },
                    &result.text,
                );
                let citation = first_citation(&ast).unwrap();
                let native = inline_text(&citation["citationPrefix"]);
                let outside = ast["blocks"][0]["c"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .take_while(|inline| inline["t"] != "Cite")
                    .map(inline_text)
                    .collect::<String>();
                assert_eq!(
                    (native.trim(), outside.trim()),
                    if expected == Native { ("see", "") } else { ("", "see") },
                    "{caps:?} {form:?}: {}",
                    result.text
                );
            }
        }
    }
}

#[test]
fn default_consumers_preserve_literal_affix_text() {
    if !pandoc_available() {
        return;
    }
    let mut case = seed();
    for note in [
        "a -- \"b\" ...",
        "`single` and ``double''",
        "[] {} & % $ # _ ~ ^ \\ @; |<>",
        "l'été Ελληνικά العربية हिन्दी 中文 😀",
        "a - b; 12-14. (x), z: value!",
    ] {
        case.request.items[0].prefix = vec![TextRun {
            text: note.into(),
            ..TextRun::default()
        }];
        case.request.items[0].suffix = case.request.items[0].prefix.clone();
        for caps in &CAPABILITIES {
            if !matches!(caps.escaping, Escaping::Pandoc | Escaping::Latex) {
                continue;
            }
            case.request.output = output(caps);
            let result = syntax::cite(&case.library, &case.request).unwrap();
            let ast = consumer_ast(
                if caps.escaping == Escaping::Pandoc {
                    "markdown"
                } else {
                    "latex"
                },
                &result.text,
            );
            let citation = first_citation(&ast).unwrap();
            let prefix = if caps.syntax == KeySyntax::Latex {
                ast["blocks"][0]["c"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .take_while(|inline| inline["t"] != "Cite")
                    .map(inline_text)
                    .collect::<String>()
            } else {
                inline_text(&citation["citationPrefix"])
            };
            assert_eq!(prefix.trim(), note, "{caps:?}: {}", result.text);
            let expected = if caps.escaping == Escaping::Pandoc {
                format!(", {{}}{note}")
            } else {
                note.to_string()
            };
            assert_eq!(
                inline_text(&citation["citationSuffix"]),
                expected,
                "{caps:?}: {}",
                result.text
            );
        }
    }
}

#[test]
fn mixed_pandoc_forms_are_explicitly_refused_in_one_request() {
    let mut case = seed();
    case.request.items.push(case.request.items[0].clone());
    case.request.items[0].form = Some(CitationForm::Narrative);
    let result = syntax::cite(&case.library, &case.request).unwrap();
    assert_eq!(
        result.outcomes[0],
        ItemOutcome::Refused {
            item: 0,
            error: CitationError::UnsupportedForm
        }
    );
    assert!(result.copy_content(2).is_none());
    if pandoc_available() {
        let ast = consumer_ast("markdown", &result.text);
        let mut ids = vec![];
        citations(&ast, &mut ids);
        assert_eq!(ids.len(), 1);
    }
}

#[test]
fn literal_punctuation_across_runs_cannot_create_smart_typography() {
    if !pandoc_available() {
        return;
    }
    let mut case = seed();
    case.request.items[0].suffix = "--...\"'`"
        .chars()
        .map(|c| TextRun {
            text: c.to_string(),
            ..TextRun::default()
        })
        .collect();
    for caps in &CAPABILITIES {
        if !matches!(caps.escaping, Escaping::Pandoc | Escaping::Latex) {
            continue;
        }
        case.request.output = output(caps);
        let result = syntax::cite(&case.library, &case.request).unwrap();
        let ast = consumer_ast(
            if caps.escaping == Escaping::Pandoc {
                "markdown"
            } else {
                "latex"
            },
            &result.text,
        );
        let suffix = inline_text(&first_citation(&ast).unwrap()["citationSuffix"]);
        let suffix = suffix.strip_prefix(", ").unwrap_or(&suffix);
        let suffix = if caps.escaping == Escaping::Pandoc {
            suffix.strip_prefix("{}").unwrap()
        } else {
            suffix
        };
        assert_eq!(suffix, "--...\"'`", "{}", result.text);
    }
}

#[test]
fn pandoc_superscript_affixes_stay_inside_the_citation() {
    if !pandoc_available() {
        return;
    }
    let plain = |text: &str| TextRun {
        text: text.into(),
        ..TextRun::default()
    };
    let sup = |text: &str, italic| TextRun {
        text: text.into(),
        script: Script::Superscript,
        italic,
        ..TextRun::default()
    };
    for (form, prefix, suffix, field, expected) in [
        (
            CitationForm::Normal,
            vec![],
            vec![plain("x"), sup("2", false), plain("y")],
            "citationSuffix",
            ", {}x2y",
        ),
        (
            CitationForm::Normal,
            vec![sup("th", false), plain(" see")],
            vec![],
            "citationPrefix",
            "th see",
        ),
        (
            CitationForm::Normal,
            vec![sup("a", true), plain("b")],
            vec![],
            "citationPrefix",
            "ab",
        ),
        (
            CitationForm::Narrative,
            vec![],
            vec![sup("2", false)],
            "citationSuffix",
            "{}2",
        ),
    ] {
        let mut case = seed();
        case.request.form = form;
        case.request.items[0].prefix = prefix;
        case.request.items[0].suffix = suffix;
        let result = syntax::cite(&case.library, &case.request).unwrap();
        assert!(!result.text.contains("[^"), "{}", result.text);
        let ast = consumer_ast("markdown", &result.text);
        let affix = &first_citation(&ast).unwrap()[field];
        assert!(
            affix.to_string().contains("\"t\":\"Superscript\""),
            "{}: {affix}",
            result.text
        );
        assert_eq!(inline_text(affix), expected, "{}", result.text);
        if result.text.contains("*a*") {
            assert!(affix.to_string().contains("\"t\":\"Emph\""));
        }
    }
}

fn processed_pandoc_citation(text: &str) -> String {
    let style = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/desktop/vendor/csl/styles/apa.csl");
    let header = "---\nreferences:\n- id: Example2026\n  type: book\n  title: Synthetic\n  author:\n  - family: Taylor\n  issued:\n    date-parts: [[2026]]\n---\n\n";
    let ast = consumer_ast_with(
        "markdown",
        &format!("{header}{text}"),
        &["--citeproc", "--csl", style.to_str().unwrap()],
    );
    inline_text(&ast["blocks"][0]["c"])
}

#[test]
fn suffixes_never_create_empty_punctuation_or_invented_locators() {
    if !pandoc_available() {
        return;
    }
    let mut case = seed();
    for form in [CitationForm::Normal, CitationForm::Narrative] {
        case.request.form = form;
        for (suffix, page, expected) in [
            (
                "  ",
                None,
                if form == CitationForm::Normal {
                    "(Taylor, 2026)"
                } else {
                    "Taylor (2026)"
                },
            ),
            (
                "12 special",
                None,
                if form == CitationForm::Normal {
                    "(Taylor, 2026, 12 special)"
                } else {
                    "Taylor (2026, 12 special)"
                },
            ),
            (
                "12",
                Some("33"),
                if form == CitationForm::Normal {
                    "(Taylor, 2026, p. 33, 12)"
                } else {
                    "Taylor (2026, p. 33, 12)"
                },
            ),
        ] {
            case.request.items[0].suffix = vec![TextRun {
                text: suffix.into(),
                ..TextRun::default()
            }];
            case.request.items[0].locator = page.map(|value| Locator {
                term: LocatorTerm::Page,
                value: value.into(),
            });
            let result = syntax::cite(&case.library, &case.request).unwrap();
            assert_eq!(processed_pandoc_citation(&result.text), expected, "{}", result.text);
        }
    }
}

#[test]
fn pandoc_locator_braces_cannot_swallow_a_literal_suffix() {
    if !pandoc_available() {
        return;
    }
    let mut case = seed();
    case.request.items[0].suffix = vec![TextRun {
        text: "34".into(),
        ..TextRun::default()
    }];
    for value in [
        "{12", "12}", "{12}", "{{12}}", r"\{12\}", "{[12]}", "[{12}]", "12]", "[12", "[12]", "12\\",
    ] {
        case.request.items[0].locator = Some(Locator {
            term: LocatorTerm::Page,
            value: value.into(),
        });
        let result = syntax::cite(&case.library, &case.request).unwrap();
        assert_eq!(
            result.outcomes,
            [ItemOutcome::Refused {
                item: 0,
                error: CitationError::InvalidLocator
            }]
        );
        assert!(result.copy_content(1).is_none());
    }
    case.request.items[0].locator.as_mut().unwrap().value = "12".into();
    let result = syntax::cite(&case.library, &case.request).unwrap();
    assert_eq!(processed_pandoc_citation(&result.text), "(Taylor, 2026, p. 12, 34)");
}

#[test]
fn biblatex_suffix_only_postnotes_never_acquire_page_labels() {
    let mut case = seed();
    for command in ["autocite", "parencite", "textcite"] {
        case.request.output = OutputSpec::Key {
            syntax: KeySyntax::Biblatex,
            command: Some(command.into()),
        };
        for (suffix, escaped) in [
            ("12", "12"),
            ("12-14", "12{-}14"),
            ("XIV", "XIV"),
            ("12, 14", "12, 14"),
            ("12 special", "12 special"),
        ] {
            case.request.items.truncate(1);
            case.request.items[0].locator = None;
            case.request.items[0].prefix.clear();
            case.request.items[0].suffix = vec![TextRun {
                text: suffix.into(),
                ..TextRun::default()
            }];
            let result = syntax::cite(&case.library, &case.request).unwrap();
            assert_eq!(result.text, format!("\\{command}[{escaped}\\nopp]{{Example2026}}"));
            assert!(result.losses.is_empty());
            assert!(result.copy_content(1).is_some());

            case.request.items[0].prefix = vec![TextRun {
                text: "see".into(),
                ..TextRun::default()
            }];
            case.request.items.push(case.request.items[0].clone());
            let result = syntax::cite(&case.library, &case.request).unwrap();
            let note = format!("[see][{escaped}\\nopp]{{Example2026}}");
            assert_eq!(result.text, format!("\\{command}s{note}{note}"));
            assert!(result.copy_content(2).is_some());
        }
        case.request.items.truncate(1);
        case.request.items[0].prefix.clear();
        case.request.items[0].suffix = vec![TextRun {
            text: "12".into(),
            italic: true,
            ..TextRun::default()
        }];
        assert_eq!(
            syntax::cite(&case.library, &case.request).unwrap().text,
            format!("\\{command}[\\emph{{12}}\\nopp]{{Example2026}}")
        );
        case.request.items[0].suffix.clear();
        assert_eq!(
            syntax::cite(&case.library, &case.request).unwrap().text,
            format!("\\{command}{{Example2026}}")
        );
    }
}

#[test]
fn natbib_suffixes_are_refused_until_the_syntax_is_supported() {
    let mut case = seed();
    case.request.output = OutputSpec::Key {
        syntax: KeySyntax::Natbib,
        command: None,
    };
    case.request.items[0].locator = None;
    case.request.items[0].suffix = vec![TextRun {
        text: "12".into(),
        ..TextRun::default()
    }];
    assert_eq!(
        syntax::cite(&case.library, &case.request),
        Err(CitationError::UnsupportedOutput)
    );
}

#[test]
fn biblatex_page_labels_survive_a_literal_suffix() {
    let mut case = seed();
    case.request.output = OutputSpec::Key {
        syntax: KeySyntax::Biblatex,
        command: None,
    };
    case.request.items[0].suffix = vec![TextRun {
        text: "note".into(),
        ..TextRun::default()
    }];
    for (value, label) in [("33", "page"), ("33-35", "pages")] {
        case.request.items[0].locator = Some(Locator {
            term: LocatorTerm::Page,
            value: value.into(),
        });
        let result = syntax::cite(&case.library, &case.request).unwrap();
        assert!(
            result.text.contains(&format!("\\bibstring{{{label}}}~")),
            "{}",
            result.text
        );
        assert!(result.losses.is_empty());
        assert!(!result.text.contains("\\nopp"));
    }
}

#[test]
fn locator_quotes_remain_literal_in_latex_consumers() {
    if !pandoc_available() {
        return;
    }
    let mut case = seed();
    case.request.items[0].locator = Some(Locator {
        term: LocatorTerm::Chapter,
        value: "``quoted''".into(),
    });
    for syntax in [KeySyntax::Biblatex, KeySyntax::Latex] {
        case.request.output = OutputSpec::Key { syntax, command: None };
        let result = syntax::cite(&case.library, &case.request).unwrap();
        let ast = consumer_ast("latex", &result.text);
        assert_eq!(
            inline_text(&first_citation(&ast).unwrap()["citationSuffix"]),
            "chap.\u{a0}``quoted''",
            "{}",
            result.text
        );
    }
}

#[test]
fn control_characters_are_refused_in_every_affix_target() {
    let mut case = seed();
    for control in ['\0', '\n', '\r', '\t', '\u{7f}'] {
        case.request.items[0].suffix = vec![TextRun {
            text: format!("synthetic{control}text"),
            ..TextRun::default()
        }];
        for caps in &CAPABILITIES {
            case.request.output = output(caps);
            assert_eq!(
                syntax::cite(&case.library, &case.request).unwrap().outcomes,
                [ItemOutcome::Refused {
                    item: 0,
                    error: CitationError::InvalidRichText
                }]
            );
        }
    }
}

fn inline_text(value: &Value) -> String {
    match value.get("t").and_then(Value::as_str) {
        Some("Str") => value["c"].as_str().unwrap_or_default().into(),
        Some("Space") | Some("SoftBreak") => " ".into(),
        Some("Span") => inline_text(&value["c"][1]),
        Some(_) => inline_text(&value["c"]),
        None => value
            .as_array()
            .map_or_else(String::new, |items| items.iter().map(inline_text).collect()),
    }
}

#[test]
fn typst_literal_prefix_does_not_turn_into_shorthands_or_smart_quotes() {
    if !pandoc_available() {
        return;
    }
    use std::io::Write;
    let mut case = seed();
    case.request.output = OutputSpec::Key {
        syntax: KeySyntax::Typst,
        command: None,
    };
    let prefix = "literal ~ -1 -- --- ... \"quoted\" http://example.invalid";
    case.request.items[0].prefix = vec![TextRun {
        text: prefix.into(),
        ..TextRun::default()
    }];
    let result = syntax::cite(&case.library, &case.request).unwrap();
    let mut child = std::process::Command::new("pandoc")
        .args(["-f", "typst", "-t", "json"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(result.text.as_bytes()).unwrap();
    let parsed = child.wait_with_output().unwrap();
    let ast: Value = serde_json::from_slice(&parsed.stdout).unwrap();
    let before: String = ast["blocks"][0]["c"]
        .as_array()
        .unwrap()
        .iter()
        .take_while(|inline| inline["t"] != "Cite")
        .map(inline_text)
        .collect();
    assert_eq!(before.trim(), prefix, "{}", result.text);
}

#[test]
fn unusual_keys_are_preserved_or_explicitly_refused_for_each_target() {
    if !pandoc_available() {
        return;
    }
    use std::io::Write;
    for key in [
        "参考文献",
        "mark\u{0345}2026",
        "back\\slash",
        "quote\"key",
        "a@b",
        "a[b]",
        "50%off",
        "a#b",
        "a$b",
        "a&b",
        "til~de",
        "<key>",
        "a^b",
    ] {
        let mut case = seed();
        let snapshot = changed(&case, |record| record.citation_keys[0].value = key.into());
        for caps in &CAPABILITIES {
            case.request.output = output(caps);
            let result = syntax::cite(&snapshot, &case.request).unwrap();
            if result.copy_content(1).is_none() {
                assert_eq!(
                    result.outcomes,
                    [ItemOutcome::Refused {
                        item: 0,
                        error: CitationError::InvalidKey
                    }]
                );
                continue;
            }
            // Pandoc's LaTeX reader does not accept the complete BibLaTeX key
            // vocabulary (including combining marks and quotes). Its parser
            // property below covers safe keys and hostile annotation text;
            // these unusual keys additionally retain their exact UTF-8 bytes.
            if caps.escaping == Escaping::Latex {
                assert!(result.text.contains(key));
                continue;
            }
            let format = match caps.escaping {
                Escaping::Pandoc => "markdown",
                Escaping::Latex => "latex",
                Escaping::Typst => "typst",
                _ => continue,
            };
            let mut child = std::process::Command::new("pandoc")
                .args(["-f", format, "-t", "json"])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            child.stdin.take().unwrap().write_all(result.text.as_bytes()).unwrap();
            let parsed = child.wait_with_output().unwrap();
            assert!(parsed.status.success(), "{}", result.text);
            let ast: Value = serde_json::from_slice(&parsed.stdout).unwrap();
            let mut ids = Vec::new();
            citations(&ast, &mut ids);
            assert_eq!(ids, [key], "{caps:?}: {}", result.text);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]
    #[test]
    fn escaping_properties_for_every_current_syntax(key in "[a-z][a-zA-Z0-9_:./-]{0,18}", fragments in prop::collection::vec(prop::sample::select(vec!["*", "_", "\\", "[", "]", ";", "@injected", "#", "%", "$", "&", "<", ">", "^", "~", "\"", "'", "//", "/*", "中", "é", "😀", " " ]), 0..15)) {
        let mut case = seed();
        let snapshot = changed(&case, |record| record.citation_keys[0].value = key.clone());
        let note = fragments.concat();
        case.request.items[0].prefix = vec![TextRun { text: note.clone(), ..TextRun::default() }];
        case.request.items[0].suffix = case.request.items[0].prefix.clone();
        let available = pandoc_available();
        for caps in &CAPABILITIES {
            case.request.output = output(caps);
            let result = syntax::cite(&snapshot, &case.request).unwrap();
            prop_assert!(result.content_matches_hash());
            prop_assert!(result.copy_content(1).is_some());
            match caps.escaping {
                Escaping::BareKeys => prop_assert_eq!(&result.text, &key),
                Escaping::Json => {
                    let records: Vec<Value> = serde_json::from_str(&result.text).unwrap();
                    prop_assert_eq!(records[0]["id"].as_str(), Some(key.as_str()));
                    prop_assert_eq!(records[0]["title"].as_str(), Some("Synthetic signals"));
                },
                _ if available => {
                    use std::io::Write;
                    let format = match caps.escaping { Escaping::Pandoc => "markdown", Escaping::Latex => "latex", _ => "typst" };
                    let mut child = std::process::Command::new("pandoc").args(["-f", format, "-t", "json"]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
                    child.stdin.take().unwrap().write_all(result.text.as_bytes()).unwrap();
                    let parsed = child.wait_with_output().unwrap();
                    prop_assert!(parsed.status.success(), "{caps:?}: {}", result.text);
                    let ast: Value = serde_json::from_slice(&parsed.stdout).unwrap();
                    let mut ids = Vec::new(); citations(&ast, &mut ids);
                    prop_assert_eq!(ids, vec![key.clone()], "{:?}: {}", caps, result.text);
                    if matches!(caps.escaping, Escaping::Pandoc | Escaping::Latex) {
                        let citation = first_citation(&ast).unwrap();
                        let prefix = if caps.syntax == KeySyntax::Latex {
                            ast["blocks"][0]["c"].as_array().unwrap().iter().take_while(|inline| inline["t"] != "Cite").map(inline_text).collect::<String>()
                        } else { inline_text(&citation["citationPrefix"]) };
                        let expected = note.split_whitespace().collect::<Vec<_>>().join(" ");
                        prop_assert_eq!(prefix.split_whitespace().collect::<Vec<_>>().join(" "), expected.clone(), "{:?}: {}", caps, result.text);
                        let suffix = inline_text(&citation["citationSuffix"]);
                        let suffix = if caps.escaping == Escaping::Pandoc && !note.trim().is_empty() { suffix.strip_prefix(",").unwrap_or(&suffix).trim_start().strip_prefix("{}").unwrap() } else { &suffix };
                        prop_assert_eq!(suffix.split_whitespace().collect::<Vec<_>>().join(" "), expected, "{:?}: {}", caps, result.text);
                    }
                },
                _ => {},
            }
        }
    }
}
