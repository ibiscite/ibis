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
        Err(CitationError::InvalidRichText)
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
                Escaping::Pandoc => "markdown-smart",
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
                    let format = match caps.escaping { Escaping::Pandoc => "markdown-smart", Escaping::Latex => "latex", _ => "typst" };
                    let mut child = std::process::Command::new("pandoc").args(["-f", format, "-t", "json"]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
                    child.stdin.take().unwrap().write_all(result.text.as_bytes()).unwrap();
                    let parsed = child.wait_with_output().unwrap();
                    prop_assert!(parsed.status.success(), "{caps:?}: {}", result.text);
                    let ast: Value = serde_json::from_slice(&parsed.stdout).unwrap();
                    let mut ids = Vec::new(); citations(&ast, &mut ids);
                    prop_assert_eq!(ids, vec![key.clone()], "{:?}: {}", caps, result.text);
                },
                _ => {},
            }
        }
    }
}
