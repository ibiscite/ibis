#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use ibis_csl::{FormatError, Formatter};
use serde_json::json;

// Isolate wall-clock measurements from unrelated formatter instances in this binary.
static FORMAT_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn sample() -> (Snapshot, CitationRequest) {
    let id = ItemRef {
        library: LibraryId::file(b"synthetic-format-library"),
        key: "example".into(),
    };
    let csl=serde_json::from_value(json!({"type":"book","title":"Synthetic signals","author":[{"family":"Taylor","given":"Avery"}],"issued":{"date-parts":[[2026]]},"publisher":"Imaginary Press"})).unwrap();
    let snapshot = Snapshot::new(vec![LibraryObject::Record(Record {
        id: id.clone(),
        csl,
        citation_keys: vec![],
        aliases: vec![],
        identifiers: vec![],
        raw: None,
    })])
    .unwrap();
    let request = CitationRequest {
        items: vec![CitationItem {
            item: id,
            expected_cite: None,
            form: None,
            locator: None,
            prefix: vec![],
            suffix: vec![],
            suppress_author: false,
            annotation: None,
            expected_annotation_cite: None,
        }],
        form: CitationForm::Normal,
        keep_order: false,
        output: OutputSpec::Csl { style: "apa".into() },
        input_language: None,
        output_locale: None,
    };
    (snapshot, request)
}
#[test]
fn formats_canonical_text_html_rtf_and_numeric_fallback() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    let (snapshot, mut request) = sample();
    let result = formatter.format(&snapshot, &request).unwrap();
    assert_eq!(result.text, "(Taylor, 2026)");
    assert!(result.html.as_ref().unwrap().contains("Taylor"));
    assert!(result.rtf.as_ref().unwrap().starts_with("{\\rtf1"));
    assert!(result.copy_content(1).is_some());
    request.form = CitationForm::BibliographyOnly;
    let result = formatter.format(&snapshot, &request).unwrap();
    assert_eq!(result.text, "Taylor, A. (2026). Synthetic signals. Imaginary Press.");
    assert!(
        result
            .display
            .nodes
            .iter()
            .flat_map(|node| &node.content)
            .any(|run| run.italic && run.text == "Synthetic signals")
    );
    request.form = CitationForm::Normal;
    request.output = OutputSpec::Csl { style: "ieee".into() };
    let result = formatter.format(&snapshot, &request).unwrap();
    assert_eq!(result.text, "A. Taylor, Synthetic signals. Imaginary Press, 2026.");
    assert!(result.notices.contains(&Notice::ReferenceEntry));
    assert!(!result.text.contains("[1]"));
}
#[test]
fn malformed_and_empty_requests_refuse_without_source_details() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    let (snapshot, mut request) = sample();
    request.output = OutputSpec::Csl {
        style: "untrusted/path".into(),
    };
    assert_eq!(
        formatter.format(&snapshot, &request),
        Err(FormatError::UnsupportedStyle)
    );
    request.output = OutputSpec::Csl { style: "apa".into() };
    request.items.clear();
    assert_eq!(formatter.format(&snapshot, &request), Err(FormatError::InvalidRequest));
}

fn library(items: &[serde_json::Value]) -> Snapshot {
    Snapshot::new(
        items
            .iter()
            .enumerate()
            .map(|(index, csl)| {
                LibraryObject::Record(Record {
                    id: ItemRef {
                        library: LibraryId::file(b"synthetic-format-library"),
                        // Snapshot sorts identities; preserve fixture order beyond ten records.
                        key: format!("work-{index:04}"),
                    },
                    csl: serde_json::from_value(csl.clone()).unwrap(),
                    citation_keys: vec![],
                    aliases: vec![],
                    identifiers: vec![],
                    raw: None,
                })
            })
            .collect(),
    )
    .unwrap()
}
fn selection(snapshot: &Snapshot, style: &str, bibliography: bool) -> CitationRequest {
    let (_, mut request) = sample();
    request.output = OutputSpec::Csl { style: style.into() };
    request.form = if bibliography {
        CitationForm::BibliographyOnly
    } else {
        CitationForm::Normal
    };
    request.items = snapshot
        .objects()
        .iter()
        .map(|object| CitationItem {
            item: object.object.id().clone(),
            ..request.items[0].clone()
        })
        .collect();
    request
}

#[test]
fn all_styles_match_node_for_multilingual_text_html_and_rtf() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    use std::io::Write;
    use std::process::{Command, Stdio};
    let items: Vec<serde_json::Value> = serde_json::from_str(include_str!("reference-cases.json")).unwrap();
    let snapshot = library(&items);
    let formatter = Formatter::new().unwrap();
    let mut oracle_inputs = Vec::new();
    let mut actual = Vec::new();
    for (style, _) in ibis_csl::STYLES {
        for bibliography in [false, true] {
            let request = selection(&snapshot, style, bibliography);
            actual.push(formatter.format(&snapshot, &request).unwrap());
            let oracle_items: Vec<_> = items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    let mut item = item.clone();
                    item["id"] = json!(format!("ibis-{index}"));
                    item
                })
                .collect();
            let cites:Vec<_>=(0..items.len()).map(|index|json!({"id":format!("ibis-{index}"),"prefix":"","suffix":"","suppress-author":false,"author-only":false})).collect();
            oracle_inputs.push(json!({"style":style,"items":oracle_items,"cites":cites,"bibliography":bibliography||style=="ieee","keep_order":false,"locale":null}));
        }
    }
    let mut node = Command::new(std::env::var("IBIS_NODE").unwrap_or_else(|_| "node".into()))
        .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/node-oracle.mjs"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    node.stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&oracle_inputs).unwrap())
        .unwrap();
    let expected = node.wait_with_output().unwrap();
    assert!(expected.status.success(), "Node oracle failed");
    let expected: Vec<serde_json::Value> = serde_json::from_slice(&expected.stdout).unwrap();
    for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
        assert_eq!(actual.text, expected["text"].as_str().unwrap(), "text case {index}");
        assert_eq!(actual.html.as_deref(), expected["html"].as_str(), "HTML case {index}");
        assert_eq!(actual.rtf.as_deref(), expected["rtf"].as_str(), "RTF case {index}");
        assert!(actual.copy_content(items.len()).is_some());
    }
}

#[test]
fn matches_pinned_zotero_synthetic_preview_expectation() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    // Zotero's own synthetic Preview Test asserts (Smith, 2019), including RTF:
    // https://github.com/zotero/zotero/blob/db41b99fddcc52d38897c59c161b6adaca036afb/test/tests/citeTest.js#L175-L203
    let snapshot = library(&[
        json!({"type":"book","title":"Preview Test","author":[{"family":"Smith","given":"Jane"}],"issued":{"date-parts":[[2019]]}}),
    ]);
    let formatter = Formatter::new().unwrap();
    let result = formatter
        .format(&snapshot, &selection(&snapshot, "apa", false))
        .unwrap();
    assert_eq!(result.text, "(Smith, 2019)");
    assert_eq!(result.html.as_deref(), Some("(Smith, 2019)"));
    assert!(result.rtf.as_deref().unwrap().contains("\n(Smith, 2019)\n"));
}

#[test]
fn hostile_huge_stale_and_duplicate_inputs_never_copy_wrong_output() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    let (snapshot, mut request) = sample();
    request.items[0].prefix = vec![TextRun {
        text: "<img src=x onerror=bad()> & \\input{bad}".into(),
        italic: true,
        ..TextRun::default()
    }];
    let result = formatter.format(&snapshot, &request).unwrap();
    assert!(!result.html.as_ref().unwrap().contains("<img"));
    assert_eq!(result.text, "(Taylor, 2026)");
    assert_eq!(
        result.losses,
        vec![Loss {
            item: Some(0),
            field: LossField::Prefix,
            reason: LossReason::Unsupported
        }]
    );
    assert!(result.copy_content(1).is_some());
    request.items[0].expected_cite = Some(Fingerprint::of_bytes(b"different synthetic snapshot"));
    let stale = formatter.format(&snapshot, &request).unwrap();
    assert!(stale.copy_content(1).is_none());
    assert!(matches!(
        stale.outcomes[0],
        ItemOutcome::Refused {
            error: CitationError::ChangedItem,
            ..
        }
    ));
    let (_, mut request) = sample();
    request.items[0].locator = Some(Locator {
        term: LocatorTerm::Page,
        value: "12".into(),
    });
    let mut second = request.items[0].clone();
    second.locator.as_mut().unwrap().value = "30".into();
    request.items.push(second);
    let result = formatter.format(&snapshot, &request).unwrap();
    assert!(result.text.contains("12"));
    assert!(result.text.contains("30"));
    assert!(result.copy_content(2).is_some());
    let huge = library(&[json!({"type":"book","title":"合成".repeat(1024*1024)})]);
    assert_eq!(
        formatter.format(&huge, &selection(&huge, "apa", true)),
        Err(FormatError::ResourceLimit)
    );
    let bad = library(&[json!({"type":"book","title":"Synthetic","author":[{"family":5}]})]);
    assert_eq!(
        formatter.format(&bad, &selection(&bad, "apa", true)),
        Err(FormatError::Runtime)
    );
    let (_, request) = sample();
    assert!(formatter.format(&snapshot, &request).unwrap().copy_content(1).is_some());
}

#[test]
fn citation_never_invents_document_year_suffixes_or_keeps_cached_metadata() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    let item = json!({"type":"book","title":"First synthetic book","author":[{"family":"Taylor","given":"Avery"}],"issued":{"date-parts":[[2026]]},"publisher":"Imaginary Press"});
    let mut second = item.clone();
    second["title"] = json!("Second synthetic book");
    let snapshot = library(&[item.clone(), second]);
    let request = selection(&snapshot, "apa", false);
    let result = formatter.format(&snapshot, &request).unwrap();
    assert!(!result.text.contains("2026a"));
    assert!(!result.text.contains("2026b"));
    assert!(result.notices.contains(&Notice::YearSuffixWithheld));
    let mut edited = item;
    edited["title"] = json!("Edited synthetic book");
    let snapshot = library(&[edited]);
    let result = formatter.format(&snapshot, &selection(&snapshot, "apa", true)).unwrap();
    assert!(result.text.contains("Edited synthetic book"));
    assert!(!result.text.contains("First synthetic book"));
}

#[test]
fn formatter_measurement_gate() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    use std::io::Write;
    use std::time::{Duration, Instant};
    let formatter = Formatter::new().unwrap();
    let (snapshot, request) = sample();
    let started = Instant::now();
    formatter.format(&snapshot, &request).unwrap();
    let cold = started.elapsed();
    let mut warm = Vec::new();
    for _ in 0..10 {
        let started = Instant::now();
        formatter.format(&snapshot, &request).unwrap();
        warm.push(started.elapsed());
    }
    warm.sort_unstable();
    let p95 = warm[9];
    let bases: Vec<serde_json::Value> = serde_json::from_str(include_str!("reference-cases.json")).unwrap();
    let items: Vec<_> = (0..100)
        .map(|index| {
            let mut item = bases[index % bases.len()].clone();
            item["title"] = json!(format!("Synthetic measurement {index}"));
            item
        })
        .collect();
    let bibliography = library(&items);
    let request = selection(&bibliography, "apa", true);
    let started = Instant::now();
    let result = formatter.format(&bibliography, &request).unwrap();
    let batch = started.elapsed();
    assert!(result.copy_content(100).is_some());
    // Direct stdout makes the measurement visible in existing Cargo CI logs.
    writeln!(
        std::io::stdout(),
        "CSL measurement {}: cold={}ms warm-one-p95={}ms bibliography-100={}ms",
        std::env::consts::OS,
        cold.as_millis(),
        p95.as_millis(),
        batch.as_millis()
    )
    .unwrap();
    assert!(cold <= Duration::from_secs(2), "cold formatter exceeded 2s");
    assert!(p95 <= Duration::from_millis(250), "warm preview exceeded 250ms");
    assert!(batch <= Duration::from_secs(10), "100-entry bibliography exceeded 10s");
}

#[test]
fn affixes_are_omitted_with_exact_typed_losses_until_builder_support() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    let (snapshot, mut request) = sample();
    for (text, italic, bold) in [
        ("<i>", false, false),
        ("<", false, false),
        ("&", false, false),
        ("plain text", false, false),
        ("styled text", true, true),
    ] {
        let run = TextRun {
            text: text.into(),
            italic,
            bold,
            ..TextRun::default()
        };
        request.items[0].prefix = vec![run.clone()];
        request.items[0].suffix = vec![run];
        let result = formatter.format(&snapshot, &request).unwrap();
        assert_eq!(result.text, "(Taylor, 2026)");
        assert_eq!(
            result.losses,
            vec![
                Loss {
                    item: Some(0),
                    field: LossField::Prefix,
                    reason: LossReason::Unsupported
                },
                Loss {
                    item: Some(0),
                    field: LossField::Suffix,
                    reason: LossReason::Unsupported
                }
            ]
        );
    }
}

#[test]
fn numeric_style_refuses_forms_its_reference_fallback_cannot_carry() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    let (snapshot, mut request) = sample();
    request.output = OutputSpec::Csl { style: "ieee".into() };
    request.form = CitationForm::YearOnly;
    let result = formatter.format(&snapshot, &request).unwrap();
    assert!(matches!(
        result.outcomes[0],
        ItemOutcome::Refused {
            error: CitationError::UnsupportedForm,
            ..
        }
    ));
    assert!(result.copy_content(1).is_none());
}

#[test]
fn presentation_equivalent_titles_are_valid_without_rewriting_canonical_output() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    for title in [
        "Synthetic area in m²",
        "Synthetic  area",
        "Synthetic\u{a0}area",
        "Synthetic H₂O",
        "Synthetic CO<sub>2</sub>",
        "Synthetic paragraphs\u{2028}one\u{2029}two",
    ] {
        let snapshot = library(&[
            json!({"type":"book","title":title,"author":[{"family":"Taylor","given":"Avery"}],"issued":{"date-parts":[[2026]]}}),
        ]);
        let result = formatter.format(&snapshot, &selection(&snapshot, "apa", true)).unwrap();
        assert!(result.copy_content(1).is_some());
        if title == "Synthetic area in m²" {
            assert!(result.text.contains("m²"));
            assert!(result.html.as_ref().unwrap().contains("<sup>2</sup>"));
        }
        if title == "Synthetic  area" {
            assert!(result.text.contains("Synthetic  area"));
            assert!(result.html.as_ref().unwrap().contains("\u{a0}"));
        }
    }
}

#[test]
fn bibliography_reports_document_year_suffixes_withheld() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    let items:Vec<_>=["First synthetic book","Second synthetic book"].into_iter().map(|title|json!({"type":"book","title":title,"author":[{"family":"Taylor","given":"Avery"}],"issued":{"date-parts":[[2026]]}})).collect();
    let snapshot = library(&items);
    let result = formatter.format(&snapshot, &selection(&snapshot, "apa", true)).unwrap();
    assert!(!result.text.contains("2026a"));
    assert!(!result.text.contains("2026b"));
    assert!(result.notices.contains(&Notice::YearSuffixWithheld));
}

#[test]
fn contradictory_and_missing_author_forms_never_copy_processor_sentinels() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    let (snapshot, mut request) = sample();
    request.form = CitationForm::AuthorOnly;
    request.items[0].suppress_author = true;
    let result = formatter.format(&snapshot, &request).unwrap();
    assert!(matches!(
        result.outcomes[0],
        ItemOutcome::Refused {
            error: CitationError::UnsupportedForm,
            ..
        }
    ));
    assert!(result.copy_content(1).is_none());
    let authorless = library(&[json!({"type":"book","title":"Synthetic title","issued":{"date-parts":[[2026]]}})]);
    let mut request = selection(&authorless, "apa", false);
    request.form = CitationForm::AuthorOnly;
    let result = formatter.format(&authorless, &request).unwrap();
    assert!(result.copy_content(1).is_none());
    let legitimate = library(&[
        json!({"type":"book","title":"Synthetic [NO_PRINTED_FORM] and [CSL STYLE ERROR: reference with no printed form.] title","author":[{"family":"Taylor","given":"Avery"}],"issued":{"date-parts":[[2026]]}}),
    ]);
    let result = formatter
        .format(&legitimate, &selection(&legitimate, "apa", true))
        .unwrap();
    assert!(result.copy_content(1).is_some());
    assert!(result.text.contains("[NO_PRINTED_FORM]"));
}

#[test]
fn unsupported_record_controls_are_rejected_before_copy() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    for bad in [
        json!({"type":"book","title":"Synthetic\u{0}title"}),
        json!({"type":"book","title":"Synthetic title","author":[{"family":"Taylor\u{0}bad"}]}),
        json!({"type":"book","title":"Synthetic\u{1b}title"}),
    ] {
        let snapshot = library(&[bad]);
        assert!(formatter.format(&snapshot, &selection(&snapshot, "apa", true)).is_err());
    }
}

#[test]
fn multiple_effective_author_only_items_are_explicitly_refused() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    let snapshot = library(&[
        json!({"type":"book","title":"Synthetic first","author":[{"family":"Taylor"}],"issued":{"date-parts":[[2026]]}}),
        json!({"type":"book","title":"Synthetic second","author":[{"family":"Morgan"}],"issued":{"date-parts":[[2026]]}}),
    ]);
    for (style, _) in ibis_csl::STYLES {
        for forms in [
            [CitationForm::AuthorOnly; 2],
            [CitationForm::AuthorOnly, CitationForm::Normal],
            [CitationForm::Normal, CitationForm::AuthorOnly],
        ] {
            let mut request = selection(&snapshot, style, false);
            request.form = forms[0];
            request.items[1].form = Some(forms[1]);
            request.keep_order = true;
            let result = formatter.format(&snapshot, &request).unwrap();
            assert!(result.copy_content(2).is_none());
            for (index, form) in forms.into_iter().enumerate() {
                if form == CitationForm::AuthorOnly {
                    assert_eq!(
                        result.outcomes[index],
                        ItemOutcome::Refused {
                            item: index,
                            error: CitationError::UnsupportedForm
                        }
                    );
                }
            }
        }
        let mut overridden = selection(&snapshot, style, false);
        overridden.form = CitationForm::AuthorOnly;
        for item in &mut overridden.items {
            item.form = Some(CitationForm::Normal);
        }
        assert!(
            formatter
                .format(&snapshot, &overridden)
                .unwrap()
                .copy_content(2)
                .is_some()
        );
    }
}

#[test]
fn nonprinting_mla_suppression_is_refused_before_cluster_rendering() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    let snapshot = library(&[
        json!({"type":"book","title":"Synthetic first","author":[{"family":"Taylor"}],"issued":{"date-parts":[[2026]]}}),
        json!({"type":"book","title":"Synthetic second","author":[{"family":"Morgan"}],"issued":{"date-parts":[[2026]]}}),
    ]);
    for affected in [0, 1] {
        for year_only in [false, true] {
            for keep_order in [false, true] {
                let mut request = selection(&snapshot, "modern-language-association", false);
                request.keep_order = keep_order;
                if year_only {
                    request.items[affected].form = Some(CitationForm::YearOnly);
                } else {
                    request.items[affected].suppress_author = true;
                }
                let result = formatter.format(&snapshot, &request).unwrap();
                assert_eq!(
                    result.outcomes[affected],
                    ItemOutcome::Refused {
                        item: affected,
                        error: CitationError::UnsupportedForm
                    }
                );
                assert!(result.copy_content(2).is_none());
                if year_only {
                    assert_eq!(
                        result.outcomes[1 - affected],
                        ItemOutcome::Rendered { item: 1 - affected }
                    );
                } else {
                    assert!(result.text.is_empty());
                    assert!(
                        result
                            .outcomes
                            .iter()
                            .all(|outcome| matches!(outcome, ItemOutcome::Refused { .. }))
                    );
                }
            }
        }
    }
}

#[test]
fn suppression_and_year_only_capabilities_keep_each_printed_item_explicit() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    let snapshot = library(&[
        json!({"type":"book","title":"Synthetic first","author":[{"family":"Taylor"}],"issued":{"date-parts":[[2026]]}}),
        json!({"type":"book","title":"Synthetic second","author":[{"family":"Morgan"}],"issued":{"date-parts":[[2026]]}}),
    ]);
    for (style, _) in ibis_csl::STYLES {
        for year_only in [false, true] {
            for located in [false, true] {
                let mut request = selection(&snapshot, style, false);
                request.keep_order = true;
                if year_only {
                    request.items[1].form = Some(CitationForm::YearOnly);
                } else {
                    request.items[1].suppress_author = true;
                }
                if located {
                    request.items[1].locator = Some(Locator {
                        term: LocatorTerm::Page,
                        value: "42".into(),
                    });
                }
                let result = formatter.format(&snapshot, &request).unwrap();
                let unsupported = (style == "modern-language-association" && (year_only || !located))
                    || (style == "ieee" && year_only);
                if style == "modern-language-association" && !year_only && !located {
                    assert!(result.text.is_empty());
                    assert!(
                        result
                            .outcomes
                            .iter()
                            .all(|outcome| matches!(outcome, ItemOutcome::Refused { .. }))
                    );
                } else {
                    assert_eq!(result.outcomes[0], ItemOutcome::Rendered { item: 0 });
                    assert!(result.text.contains("Taylor"));
                }
                if unsupported {
                    assert_eq!(
                        result.outcomes[1],
                        ItemOutcome::Refused {
                            item: 1,
                            error: CitationError::UnsupportedForm
                        }
                    );
                    assert!(result.copy_content(2).is_none());
                } else {
                    assert_eq!(result.outcomes[1], ItemOutcome::Rendered { item: 1 });
                    assert!(result.copy_content(2).is_some());
                    if style == "ieee" {
                        assert!(result.text.contains("Morgan"));
                        assert!(result.losses.contains(&Loss {
                            item: Some(1),
                            field: LossField::AuthorSuppression,
                            reason: LossReason::Unsupported
                        }));
                    } else {
                        assert!(!result.text.contains("Morgan"));
                        if style != "modern-language-association" {
                            assert_eq!(result.text.matches("2026").count(), 2);
                        }
                        if located {
                            assert!(result.text.contains("42"));
                        }
                    }
                }
            }
        }
        let mut bibliography = selection(&snapshot, style, true);
        bibliography.items[1].suppress_author = true;
        let result = formatter.format(&snapshot, &bibliography).unwrap();
        assert!(result.copy_content(2).is_some());
        assert!(result.losses.contains(&Loss {
            item: Some(1),
            field: LossField::AuthorSuppression,
            reason: LossReason::Unsupported
        }));
    }
    let single = library(&[
        json!({"type":"book","title":"Synthetic single","author":[{"family":"Taylor"}],"issued":{"date-parts":[[2026]]}}),
    ]);
    let mut request = selection(&single, "modern-language-association", false);
    request.items[0].suppress_author = true;
    assert!(formatter.format(&single, &request).unwrap().copy_content(1).is_none());
    request.items[0].suppress_author = false;
    request.form = CitationForm::YearOnly;
    assert!(formatter.format(&single, &request).unwrap().copy_content(1).is_none());
}

#[test]
fn sparse_members_never_become_copyable_cluster_placeholders_or_omissions() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    let complete = json!({"type":"book","title":"Synthetic complete","author":[{"family":"Taylor"}],"issued":{"date-parts":[[2026]]}});
    let sparse = json!({"type":"book"});
    let printable = library(std::slice::from_ref(&complete));
    let mut printable_request = selection(&printable, "modern-language-association", false);
    printable_request.keep_order = true;
    for items in [[sparse.clone(), complete.clone()], [complete, sparse]] {
        let snapshot = library(&items);
        let mut request = selection(&snapshot, "modern-language-association", false);
        request.keep_order = true;
        let result = formatter.format(&snapshot, &request).unwrap();
        assert!(result.copy_content(2).is_none());
        assert!(result.text.is_empty());
        assert!(result.outcomes.iter().all(|outcome| matches!(
            outcome,
            ItemOutcome::Refused {
                error: CitationError::UnsupportedForm,
                ..
            }
        )));
        let recovered = formatter.format(&printable, &printable_request).unwrap();
        assert_eq!(recovered.text, "(Taylor)");
        assert!(recovered.copy_content(1).is_some());
    }
}

#[test]
fn genuinely_printed_literal_processor_marker_text_remains_valid() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    let snapshot = library(&[
        json!({"type":"book","title":"[CSL STYLE ERROR: reference with no printed form.]","issued":{"date-parts":[[2026]]}}),
    ]);
    for (style, _) in ibis_csl::STYLES {
        for bibliography in [false, true] {
            let result = formatter
                .format(&snapshot, &selection(&snapshot, style, bibliography))
                .unwrap();
            assert!(result.copy_content(1).is_some());
            assert!(result.text.to_ascii_lowercase().contains("csl style error"));
            assert_eq!(result.outcomes, vec![ItemOutcome::Rendered { item: 0 }]);
        }
    }
}
