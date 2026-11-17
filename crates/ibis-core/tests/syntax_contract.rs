//! The new renderer consumes the shared contract without front-end formatting.
#![cfg(test)]
use ibis_core::contract::*;
use ibis_core::contract::{GoldenCase, OutputSpec};
use ibis_core::syntax::{self, CAPABILITIES, Escaping};

/// The outputs written from citation keys. Zotero's markers name Zotero items
/// instead and have their own tests (`syntax::scannable`).
fn key_path() -> impl Iterator<Item = &'static syntax::SyntaxCapabilities> {
    CAPABILITIES.iter().filter(|caps| !caps.zotero_items)
}
use proptest::prelude::*;
use serde_json::{Value, json};

fn seed() -> GoldenCase {
    let mut case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap();
    case.library = changed(&case, |record| {
        record.csl.insert("DOI".into(), json!("10.1000/invented"));
    });
    case
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
fn reference_segments_and_source_warnings_survive_together() {
    let mut case = seed();
    let LibraryObject::Record(first) = &case.library.objects()[0].object else {
        panic!("synthetic seed record");
    };
    let mut first = first.clone();
    first.warnings = vec![CitationWarningCode::SourceField];
    let mut second = first.clone();
    second.id.key = "work-2".into();
    second.citation_keys[0].value = "Other2024".into();
    second.warnings.clear();
    case.request.items.push(CitationItem {
        item: second.id.clone(),
        ..case.request.items[0].clone()
    });
    let snapshot = Snapshot::new(vec![LibraryObject::Record(first), LibraryObject::Record(second)]).unwrap();
    let result = syntax::cite(&snapshot, &case.request).unwrap();
    assert_eq!(result.text, "[@Example2026; @Other2024]");
    assert_eq!(
        result.warnings,
        [CitationWarning {
            item: 0,
            code: CitationWarningCode::SourceField
        }]
    );
    assert_eq!(
        result
            .display
            .nodes
            .iter()
            .filter_map(|node| node.item)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(
        result
            .display
            .nodes
            .iter()
            .flat_map(|node| &node.content)
            .map(|run| run.text.as_str())
            .collect::<String>(),
        result.text
    );
    assert!(result.copy_content(2).is_some());
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
    for caps in key_path() {
        case.request.output = output(caps);
        assert_eq!(syntax::capabilities(&case.request.output).unwrap(), caps);
        // A narrative citation in a command without one is written by its sibling.
        let forms = caps.citable_forms();
        for form in CitationForm::ALL {
            case.request.form = form;
            let result = syntax::cite(&case.library, &case.request).unwrap();
            assert_eq!(
                result.copy_content(1).is_some(),
                forms.contains(&form),
                "{caps:?} {form:?}"
            );
            if !forms.contains(&form) {
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
    other.id.library = LibraryId::digest(b"invented second library");
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
        record
            .csl
            .insert("abstract".into(), json!("x".repeat(20 * 1024 * 1024)));
    });
    for syntax in [KeySyntax::CslJson, KeySyntax::Bibtex] {
        case.request.output = OutputSpec::Key { syntax, command: None };
        assert_eq!(
            syntax::cite(&snapshot, &case.request),
            Err(CitationError::OutputTooLarge)
        );
    }
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
        for caps in key_path() {
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

fn processed_pandoc_citation(text: &str) -> String {
    let style = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/synthetic-author-date.csl");
    let header = "---\nreferences:\n- id: Example2026\n  type: book\n  title: Synthetic\n  author:\n  - family: Taylor\n  issued:\n    date-parts: [[2026]]\n---\n\n";
    let ast = consumer_ast_with(
        "markdown",
        &format!("{header}{text}"),
        &["--citeproc", "--csl", style.to_str().unwrap()],
    );
    inline_text(&ast["blocks"][0]["c"])
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
fn control_characters_are_refused_in_every_affix_target() {
    let mut case = seed();
    for control in ['\0', '\n', '\r', '\t', '\u{7f}'] {
        case.request.items[0].suffix = vec![TextRun {
            text: format!("synthetic{control}text"),
            ..TextRun::default()
        }];
        for caps in key_path() {
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
        for caps in key_path() {
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
        for caps in key_path() {
            case.request.output = output(caps);
            let result = syntax::cite(&snapshot, &case.request).unwrap();
            prop_assert!(result.content_matches_hash());
            // ConTeXt reads `a::b` as a dataset, so it refuses such a key
            // rather than cite another entry.
            if caps.escaping == Escaping::Context && key.contains("::") {
                prop_assert_eq!(&result.outcomes, &[ItemOutcome::Refused { item: 0, error: CitationError::InvalidKey }]);
                continue;
            }
            prop_assert!(result.copy_content(1).is_some());
            match caps.escaping {
                Escaping::BareKeys => prop_assert_eq!(&result.text, &key),
                Escaping::Json => {
                    let records: Vec<Value> = serde_json::from_str(&result.text).unwrap();
                    prop_assert_eq!(records[0]["id"].as_str(), Some(key.as_str()));
                    prop_assert_eq!(records[0]["title"].as_str(), Some("Synthetic signals"));
                },
                Escaping::Pandoc | Escaping::Latex | Escaping::Typst | Escaping::Org if available => {
                    use std::io::Write;
                    let format = match caps.escaping { Escaping::Pandoc => "markdown", Escaping::Latex => "latex", Escaping::Org => "org", _ => "typst" };
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
                Escaping::Context => {
                    let (settings, keys) = result.text.rsplit_once("][").unwrap_or(("", result.text.strip_prefix("\\cite[").unwrap()));
                    prop_assert_eq!(keys.strip_suffix(']'), Some(key.as_str()), "{}", result.text);
                    // Every special is written as an escape, so nothing can end the setting.
                    let mut rest = settings.to_owned();
                    for escape in ["\\&", "\\%", "\\#", "\\$", "\\_", "\\char92{}", "\\char94{}", "\\char123{}", "\\char124{}", "\\char125{}", "\\char126{}"] {
                        rest = rest.replace(escape, "");
                    }
                    let rest = ["\\cite", "\\space", "\\removeunwantedspaces", "\\removepunctuation"]
                        .into_iter()
                        .fold(rest, |rest, command| rest.replace(command, ""));
                    prop_assert!(!rest.contains(['\\', '&', '%', '#', '$', '_', '^', '~', '|']), "{}", result.text);
                    prop_assert_eq!(rest.matches('{').count(), rest.matches('}').count(), "{}", result.text);
                },
                _ => {},
            }
        }
    }
}
