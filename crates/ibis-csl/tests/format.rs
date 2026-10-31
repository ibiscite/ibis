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
        added_at: None,
        id: id.clone(),
        csl,
        citation_keys: vec![],
        aliases: vec![],
        warnings: vec![],
        identifiers: vec![],
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
fn shelf_spans_follow_processor_order_without_changing_citation_meaning() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let snapshot = library(&[
        json!({"type":"book","title":"Synthetic Z","author":[{"family":"Zulu"}],"issued":{"date-parts":[[2024]]}}),
        json!({"type":"book","title":"Synthetic A","author":[{"family":"Alpha"}],"issued":{"date-parts":[[2021]]}}),
    ]);
    let formatter = Formatter::new().unwrap();
    for (style, _) in ibis_csl::STYLES {
        let request = selection(&snapshot, style, false);
        let result = formatter.format(&snapshot, &request).unwrap();
        let plain: String = result
            .display
            .nodes
            .iter()
            .flat_map(|node| &node.content)
            .map(|run| run.text.as_str())
            .collect();
        assert_eq!(plain, result.text, "{style}: every character belongs to the display");
        for item in 0..2 {
            let owned: String = result
                .display
                .nodes
                .iter()
                .filter(|node| node.item == Some(item))
                .flat_map(|node| &node.content)
                .map(|run| run.text.as_str())
                .collect();
            let id = &request.items[item].item;
            let LibraryObject::Record(record) = &snapshot.get(id).unwrap().object else {
                panic!("record")
            };
            let family = record.csl["author"][0]["family"].as_str().unwrap();
            assert!(owned.contains(family), "{style}: item {item} owns its rendered name");
        }
        assert!(result.copy_content(2).is_some());
    }
}

#[test]
fn shelf_multi_reference_spans_preserve_passive_title_styles() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let snapshot = library(&[
        json!({"type":"book","title":"Synthetic <span style=\"font-variant: small-caps;\">Caps</span>","issued":{"date-parts":[[2026]]}}),
        json!({"type":"book","title":"Other invented title","issued":{"date-parts":[[2024]]}}),
    ]);
    let result = Formatter::new()
        .unwrap()
        .format(&snapshot, &selection(&snapshot, "modern-language-association", false))
        .unwrap();
    assert!(result.html.as_deref().unwrap().contains("small-caps"));
    assert!(result.copy_content(2).is_some());
    let display = serde_json::to_value(&result.display).unwrap();
    assert!(
        display["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|node| node["content"].as_array().unwrap())
            .any(|run| run["text"] == "Caps" && run["small_caps"] == true)
    );
}

#[test]
fn shelf_repeated_bibliography_occurrences_keep_deduplicated_output_and_each_owner() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let (snapshot, mut request) = sample();
    request.output = OutputSpec::Csl { style: "ieee".into() };
    let formatter = Formatter::new().unwrap();
    let single = formatter.format(&snapshot, &request).unwrap();
    request.items[0].locator = Some(Locator {
        term: LocatorTerm::Page,
        value: "12".into(),
    });
    let mut second = request.items[0].clone();
    second.locator.as_mut().unwrap().value = "30".into();
    request.items.push(second);
    let result = formatter.format(&snapshot, &request).unwrap();
    assert_eq!(result.text, single.text);
    assert_eq!(result.html, single.html);
    assert_eq!(result.rtf, single.rtf);
    assert!(result.copy_content(2).is_some());
    for item in 0..2 {
        assert!(result.display.nodes.iter().any(|node| node.item == Some(item)));
    }
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
                    added_at: None,
                    id: ItemRef {
                        library: LibraryId::file(b"synthetic-format-library"),
                        // Snapshot sorts identities; preserve fixture order beyond ten records.
                        key: format!("work-{index:04}"),
                    },
                    csl: serde_json::from_value(csl.clone()).unwrap(),
                    citation_keys: vec![],
                    aliases: vec![],
                    warnings: vec![],
                    identifiers: vec![],
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
        if !matches!(style, "ieee" | "modern-language-association") {
            for issued in [
                Some(json!({"date-parts":[[2026]]})),
                None,
                Some(json!({"date-parts":[[0]]})),
            ] {
                let mut item = json!({"type":"book","title":"Synthetic signals","author":[{"family":"Taylor"}]});
                if let Some(issued) = issued {
                    item["issued"] = issued;
                }
                let snapshot = library(&[item.clone()]);
                let mut request = selection(&snapshot, style, false);
                request.form = CitationForm::YearOnly;
                actual.push(formatter.format(&snapshot, &request).unwrap());
                item["id"] = json!("ibis-0");
                oracle_inputs.push(json!({"style":style,"items":[item],"cites":[{"id":"ibis-0","suppress-author":true,"year-only":true}],"bibliography":false,"keep_order":false,"locale":null}));
            }
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
        assert!(actual.copy_content(actual.outcomes.len()).is_some());
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
    let html = result.html.as_deref().unwrap();
    assert!(!html.contains("<img"));
    assert!(html.contains("<i>&#60;img src=x onerror=bad()&#62; &#38; \\input{bad}</i>"));
    assert!(!result.rtf.as_deref().unwrap().contains("\\input{bad}"));
    assert_eq!(result.text, "(<img src=x onerror=bad()> & \\input{bad}Taylor, 2026)");
    assert!(result.losses.is_empty());
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
    let original = library(std::slice::from_ref(&item));
    let mut edited = item;
    edited["title"] = json!("Edited synthetic book");
    edited["author"] = json!([{"family":"Morgan","given":"Ann"}]);
    let snapshot = library(&[edited]);
    for (style, _) in ibis_csl::STYLES {
        for bibliography in [false, true] {
            let request = selection(&original, style, bibliography);
            assert!(formatter.format(&original, &request).unwrap().text.contains("Taylor"));
            let result = formatter.format(&snapshot, &request).unwrap();
            assert!(result.text.contains("Morgan"));
            assert!(!result.text.contains("Taylor"));
            if bibliography || style == "ieee" {
                assert!(result.text.to_lowercase().contains("edited synthetic book"));
                assert!(!result.text.to_lowercase().contains("first synthetic book"));
            }
        }
    }
}

#[test]
fn explicitly_literal_year_suffixes_keep_era_names_in_date_order() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    for date_kind in ["literal", "quoted-raw", "curly-quoted-raw"] {
        let items: Vec<_> = [
            ("2020ae", "Invented first"),
            ("2020ad", "Invented second"),
            ("2020ac", "Invented third"),
        ]
        .into_iter()
        .map(|(year, title)| {
            let issued = match date_kind {
                "literal" => json!({"literal": year}),
                "quoted-raw" => json!({"raw": format!("\"{year}\"")}),
                _ => json!({"raw": format!("“{year}”")}),
            };
            json!({"type":"article-journal","title":title,
                "author":[{"family":"Okafor","given":"Ada"}],
                "container-title":"Invented Journal","issued":issued})
        })
        .collect();
        let snapshot = library(&items);
        for bibliography in [false, true] {
            let result = formatter
                .format(&snapshot, &selection(&snapshot, "apa", bibliography))
                .unwrap();
            let (text, html, rtf) = if bibliography {
                (
                    "Okafor, A. (2020ac). Invented third. Invented Journal.\nOkafor, A. (2020ad). Invented second. Invented Journal.\nOkafor, A. (2020ae). Invented first. Invented Journal.",
                    "<div class=\"csl-entry\">Okafor, A. (2020ac). Invented third. <i>Invented Journal</i>.</div>\n<div class=\"csl-entry\">Okafor, A. (2020ad). Invented second. <i>Invented Journal</i>.</div>\n<div class=\"csl-entry\">Okafor, A. (2020ae). Invented first. <i>Invented Journal</i>.</div>",
                    "Okafor, A. (2020ac). Invented third. {\\i{}Invented Journal}.\\par\nOkafor, A. (2020ad). Invented second. {\\i{}Invented Journal}.\\par\nOkafor, A. (2020ae). Invented first. {\\i{}Invented Journal}.",
                )
            } else {
                (
                    "(Okafor, 2020ac, 2020ad, 2020ae)",
                    "(Okafor, 2020ac, 2020ad, 2020ae)",
                    "(Okafor, 2020ac, 2020ad, 2020ae)",
                )
            };
            assert_eq!(result.text, text, "{date_kind}, bibliography={bibliography}");
            assert_eq!(result.html.as_deref(), Some(html));
            assert_eq!(
                result.rtf,
                Some(format!(
                    "{{\\rtf1\\ansi\\deff0{{\\fonttbl{{\\f0 Times New Roman;}}}}\\f0\n{rtf}\n}}"
                ))
            );
            assert!(result.copy_content(3).is_some());
        }
    }
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
fn year_only_preserves_bundled_support_for_dated_undated_and_zero_year_items() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    for (issued, dated) in [
        (Some(json!({"date-parts":[[2026]]})), true),
        (None, false),
        (Some(json!({"date-parts":[[0]]})), false),
    ] {
        let mut item = json!({"type":"book","title":"Synthetic signals","author":[{"family":"Taylor"}]});
        if let Some(issued) = &issued {
            item["issued"] = issued.clone();
        }
        let snapshot = library(&[item]);
        for (style, _) in ibis_csl::STYLES {
            let mut request = selection(&snapshot, style, false);
            request.form = CitationForm::YearOnly;
            let result = formatter.format(&snapshot, &request).unwrap();
            // Match main's supported forms: IEEE's fallback and MLA have no year-only form.
            if matches!(style, "ieee" | "modern-language-association") {
                assert!(result.copy_content(1).is_none(), "{style}");
                assert_eq!(
                    result.outcomes,
                    vec![ItemOutcome::Refused {
                        item: 0,
                        error: CitationError::UnsupportedForm,
                    }]
                );
            } else {
                let expected = if dated {
                    "2026"
                } else if style == "harvard-cite-them-right" {
                    "no date"
                } else {
                    "n.d."
                };
                assert_eq!(result.text, format!("({expected})"), "{style}");
                assert!(result.copy_content(1).is_some(), "{style}");
            }
        }
    }
}

#[test]
fn year_only_undated_titles_and_locators_do_not_prove_no_date_output() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    for (title, locator) in [
        ("Synthetic n.d. observations", "42"),
        ("Synthetic observations", "n.d."),
        ("Synthetic observations", "42"),
    ] {
        let snapshot = library(&[json!({"type":"classic","title":title,"author":[{"family":"Taylor"}]})]);
        let mut request = selection(&snapshot, "chicago-author-date", false);
        request.form = CitationForm::YearOnly;
        request.items[0].locator = Some(Locator {
            term: LocatorTerm::Page,
            value: locator.into(),
        });
        let result = formatter.format(&snapshot, &request).unwrap();
        assert_eq!(
            result.outcomes,
            vec![ItemOutcome::Refused {
                item: 0,
                error: CitationError::UnsupportedForm,
            }],
            "title={title}, locator={locator}"
        );
        assert!(result.text.is_empty());
        assert!(result.html.is_none());
        assert!(result.rtf.is_none());
        assert!(result.copy_content(1).is_none());
    }
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
fn record_controls_do_not_discard_valid_items() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    for bad in [
        json!({"type":"book","title":"Synthetic\u{0}title"}),
        json!({"type":"book","title":"Synthetic title","author":[{"family":"Taylor\u{0}bad"}]}),
        json!({"type":"book","title":"Synthetic\u{1b}title"}),
    ] {
        let snapshot = library(&[
            bad,
            json!({"type":"book","title":"Other valid work","abstract":"unused\u{b}\u{c}abstract"}),
        ]);
        let result = formatter.format(&snapshot, &selection(&snapshot, "apa", true)).unwrap();
        assert!(result.copy_content(2).is_some());
        assert!(result.text.contains("Other valid work"));
        assert!(!result.text.chars().any(|ch| ch.is_control() && ch != '\n'));
    }
}

#[test]
fn line_breaks_preserve_every_title_word_in_all_representations() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    for style in ["chicago-author-date", "modern-language-association"] {
        for separator in [
            "\n",
            "\r",
            "\u{85}",
            "\u{2028}",
            "\u{2029}",
            "\r\n\u{85}\u{2028}\u{2029}",
        ] {
            let snapshot = library(&[
                json!({"type":"book", "title":format!("alpha{separator}beta"), "author":[{"family":"Zeta","given":"Ann"}], "issued":{"date-parts":[[2020]]}}),
            ]);
            let request = selection(&snapshot, style, true);
            let result = formatter.format(&snapshot, &request).unwrap();
            for output in [
                &result.text,
                result.html.as_ref().unwrap(),
                result.rtf.as_ref().unwrap(),
            ] {
                assert!(output.contains("Alpha Beta"), "{style}: {output:?}");
            }
            assert!(result.copy_content(1).is_some());
        }
    }
}

#[test]
fn ieee_lowercase_given_name_preserves_processor_bytes() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    let snapshot = library(&[
        json!({"type":"book","title":"Synthetic signals","author":[{"family":"Taylor","given":"avery"}],"issued":{"date-parts":[[2026]]},"publisher":"Imaginary Press"}),
    ]);
    for bibliography in [false, true] {
        let result = formatter
            .format(&snapshot, &selection(&snapshot, "ieee", bibliography))
            .unwrap();
        assert_eq!(result.text, "avery Taylor, Synthetic signals. Imaginary Press, 2026.");
        assert!(
            result
                .html
                .as_ref()
                .unwrap()
                .contains("<div class=\"csl-entry\"> avery Taylor")
        );
        assert!(result.rtf.as_ref().unwrap().contains("avery Taylor"));
        assert!(result.copy_content(1).is_some());
    }
}

#[test]
fn reused_formatter_handles_bibliographies_across_locale_order_and_styles() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let items: Vec<_> = (0..100).map(|index| json!({"type":"book","title":format!("Synthetic title {index}"),"author":[{"family":format!("Invented{index}"),"given":"Avery"}],"issued":{"date-parts":[[2026]]},"publisher":"Imaginary Press"})).collect();
    let snapshot = library(&items);
    let formatter = Formatter::new().unwrap();
    for keep_order in [false, true] {
        for locale in [None, Some("en"), Some("en-US")] {
            let mut request = selection(&snapshot, "chicago-author-date", true);
            request.output_locale = locale.map(String::from);
            request.keep_order = keep_order;
            let result = formatter.format(&snapshot, &request).unwrap();
            assert!(result.copy_content(100).is_some());
        }
    }
    // Reuse both workers with enough distinct engine keys to turn over the cache.
    let small = library(&items[..3]);
    for round in 0..3 {
        for (style, _) in ibis_csl::STYLES {
            for bibliography in [false, true] {
                let mut request = selection(&small, style, bibliography);
                request.output_locale = [None, Some("en"), Some("en-US")][round].map(String::from);
                request.keep_order = round % 2 == 0;
                assert!(formatter.format(&small, &request).unwrap().copy_content(3).is_some());
            }
        }
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
fn run(text: &str, italic: bool) -> TextRun {
    TextRun {
        text: text.into(),
        italic,
        bold: false,
        script: Script::Normal,
    }
}
#[test]
fn affixes_keep_their_text_and_styling_in_every_representation() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    let (snapshot, mut request) = sample();
    request.items[0].locator = Some(Locator {
        term: LocatorTerm::Page,
        value: "12".into(),
    });
    request.items[0].prefix = vec![run("cf. ", false), run("passim", true), run(" ", false)];
    request.items[0].suffix = vec![run(", emphasis added", false)];
    let result = formatter.format(&snapshot, &request).unwrap();
    assert_eq!(result.text, "(cf. passim Taylor, 2026, p. 12, emphasis added)");
    assert_eq!(
        result.html.as_deref(),
        Some("(cf. <i>passim</i> Taylor, 2026, p. 12, emphasis added)")
    );
    assert!(result.rtf.as_deref().unwrap().contains("{\\i{}passim}"));
    assert!(result.losses.is_empty());
    assert!(result.copy_content(1).is_some());
    // A prefix without a trailing space gets one, as citeproc-js gives it.
    request.items[0].prefix = vec![run("see", false)];
    request.items[0].suffix = vec![];
    assert_eq!(
        formatter.format(&snapshot, &request).unwrap().text,
        "(see Taylor, 2026, p. 12)"
    );
}
#[test]
fn affix_text_is_literal_never_markup_or_typography() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    let (snapshot, mut request) = sample();
    request.items[0].prefix = vec![run("say \"it\" & <b>x</b> it's &#38; ", false)];
    request.items[0].suffix = vec![run(" -- 'q' {\\b}", false)];
    let result = formatter.format(&snapshot, &request).unwrap();
    assert_eq!(
        result.text,
        "(say \"it\" & <b>x</b> it's &#38; Taylor, 2026 -- 'q' {\\b})"
    );
    assert_eq!(
        result.html.as_deref(),
        Some("(say \"it\" &#38; &#60;b&#62;x&#60;/b&#62; it's &#38;#38; Taylor, 2026 -- 'q' {\\b})")
    );
    assert!(result.rtf.as_deref().unwrap().contains("-- 'q' \\{\\\\b\\})"));
    // The placeholder sentinel is refused wherever it appears.
    for sentinel in ['\u{E000}', '\u{E001}'] {
        request.items[0].suffix = vec![run(&format!("x{sentinel}"), false)];
        assert_eq!(formatter.format(&snapshot, &request), Err(FormatError::InvalidRequest));
    }
}
#[test]
fn affixes_in_a_bibliography_are_reported_as_losses() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    let (snapshot, mut request) = sample();
    request.form = CitationForm::BibliographyOnly;
    request.items[0].prefix = vec![run("see ", false)];
    let result = formatter.format(&snapshot, &request).unwrap();
    assert!(!result.text.contains("see"));
    assert!(result.losses.iter().any(|loss| loss.field == LossField::Prefix));
}
#[test]
fn narrative_citations_name_the_author_then_cite_the_rest() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    let (snapshot, mut request) = sample();
    request.form = CitationForm::Narrative;
    request.items[0].locator = Some(Locator {
        term: LocatorTerm::Page,
        value: "12".into(),
    });
    for (style, expected) in [
        ("apa", "Taylor (2026, p. 12)"),
        ("chicago-author-date", "Taylor (2026, 12)"),
        ("modern-language-association", "Taylor (12)"),
    ] {
        request.output = OutputSpec::Csl { style: style.into() };
        let result = formatter.format(&snapshot, &request).unwrap();
        assert_eq!(result.text, expected, "{style}");
        assert!(result.copy_content(1).is_some());
    }
    request.output = OutputSpec::Csl { style: "ieee".into() };
    let result = formatter.format(&snapshot, &request).unwrap();
    assert!(result.copy_content(1).is_none());
    // One citation can have only one sentence subject.
    request.output = OutputSpec::Csl { style: "apa".into() };
    request.items.push(request.items[0].clone());
    let result = formatter.format(&snapshot, &request).unwrap();
    assert!(result.copy_content(2).is_none());
}

#[test]
fn styled_dollar_sequences_stay_literal_in_prefixes_and_suffixes() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    let (snapshot, mut request) = sample();
    for token in ["$$", "$&", "$`", "$'"] {
        for prefix in [false, true] {
            request.items[0].prefix.clear();
            request.items[0].suffix.clear();
            let runs = if prefix {
                &mut request.items[0].prefix
            } else {
                &mut request.items[0].suffix
            };
            runs.push(run(&format!(" {token} "), true));
            let result = formatter.format(&snapshot, &request).unwrap();
            assert!(result.text.contains(token), "{token}");
            assert!(
                result
                    .html
                    .as_deref()
                    .unwrap()
                    .contains(&format!("<i> {} </i>", token.replace('&', "&#38;")))
            );
            assert!(
                result
                    .rtf
                    .as_deref()
                    .unwrap()
                    .contains(&format!("{{\\i{{}} {token} }}"))
            );
            assert!(result.losses.is_empty());
            assert!(result.copy_content(1).is_some());
        }
    }
}

#[test]
fn narrative_citations_refuse_author_suppression() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    for per_item in [false, true] {
        let (snapshot, mut request) = sample();
        if per_item {
            request.items[0].form = Some(CitationForm::Narrative);
        } else {
            request.form = CitationForm::Narrative;
        }
        request.items[0].suppress_author = true;
        let result = formatter.format(&snapshot, &request).unwrap();
        assert_eq!(
            result.outcomes,
            [ItemOutcome::Refused {
                item: 0,
                error: CitationError::UnsupportedForm,
            }]
        );
        assert!(result.text.is_empty());
        assert!(result.copy_content(1).is_none());
    }
}

#[test]
fn apa_narrative_refuses_name_lists_that_need_a_different_conjunction() {
    let _guard = FORMAT_TESTS.lock().unwrap();
    let formatter = Formatter::new().unwrap();
    let author = json!([{"family":"Taylor"}]);
    let multiple = json!([{"family":"Taylor"},{"family":"Dale"}]);
    for creators in [
        json!({"author":multiple}),
        json!({"author":author,"composer":multiple}),
        json!({"author":[{"family":"Taylor"},{"literal":" "}]}),
    ] {
        let mut csl = json!({"type":"book","title":"Synthetic signals","issued":{"date-parts":[[2026]]}});
        csl.as_object_mut()
            .unwrap()
            .extend(creators.as_object().unwrap().clone());
        let snapshot = library(&[csl]);
        let mut request = selection(&snapshot, "apa", false);
        request.form = CitationForm::Narrative;
        let result = formatter.format(&snapshot, &request).unwrap();
        assert_eq!(
            result.outcomes,
            [ItemOutcome::Refused {
                item: 0,
                error: CitationError::UnsupportedForm
            }]
        );
        assert!(result.copy_content(1).is_none());
        request.form = CitationForm::Normal;
        assert!(formatter.format(&snapshot, &request).unwrap().copy_content(1).is_some());
    }
}
