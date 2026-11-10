//! The app's row of "same results from every front end" (#1200). The picker
//! exposes two engine methods: `search` (its result rows) and `cite` (its
//! preview and copy). Each golden case the picker can express runs through
//! them and must give what the engine gives for the same request.
use super::*;
use ibis_core::contract::{GoldenCase, Snapshot};
use std::sync::Arc;

/// The golden cases the picker can ask for: an output the engine lists, in
/// the normal form, with plain items. Page and form combinations have their
/// own matrix below; literal affixes also run through the real CLI.
fn picker_choice(request: &CitationRequest) -> Option<(Vec<FileSelection>, &'static str)> {
    let output = ibis_engine::outputs::named(&request.output)?;
    if request.form != CitationForm::Normal || request.items.is_empty() {
        return None;
    }
    let items = request
        .items
        .iter()
        .map(|item| {
            let plain = item.prefix.is_empty()
                && item.suffix.is_empty()
                && !item.suppress_author
                && item.form.is_none()
                && item.annotation.is_none();
            plain.then_some(())?;
            Some(FileSelection {
                prefix: None,
                suffix: None,
                id: item.item.clone(),
                cite: item.expected_cite.clone()?,
                form: None,
                locator_input: None,
                locator: item.locator.clone().map(Into::into),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some((items, output.id.as_str()))
}

fn engine_for(case: &GoldenCase) -> Result<Engine, Box<dyn std::error::Error>> {
    let engine = Engine::new().map_err(|_| "engine")?.without_file_watches();
    let mut libraries: Vec<_> = case
        .library
        .objects()
        .iter()
        .map(|item| item.object.id().library.clone())
        .collect();
    libraries.dedup();
    for library in libraries {
        let objects = case
            .library
            .objects()
            .iter()
            .filter(|item| item.object.id().library == library)
            .map(|item| item.object.clone())
            .collect();
        engine
            .publish_library(
                library,
                ibis_engine::LibraryKind::Snapshot,
                Arc::new(Snapshot::new(objects)?),
            )
            .map_err(|_| "publish")?;
    }
    Ok(engine)
}

/// The picker's preview and copy of `request` against the engine's result
/// for it: `None` when the picker cannot ask for the request, otherwise
/// whether both copied it (rather than both refusing).
fn same_through_the_picker(
    case: &GoldenCase,
    mut request: CitationRequest,
) -> Result<Option<bool>, Box<dyn std::error::Error>> {
    let engine = engine_for(case)?;
    // The picker always sends the fingerprint it showed.
    for item in &mut request.items {
        item.expected_cite = case
            .library
            .get(&item.item)
            .map(|object| object.fingerprints.cite.clone());
    }
    let Some((items, output)) = picker_choice(&request) else {
        return Ok(None);
    };
    // The picker keeps key order; formatted styles sort as the style says.
    // It names no output language, so a style writes in its own.
    request.output_locale = None;
    request.keep_order = matches!(request.output, OutputSpec::Key { .. });
    let expected = engine.cite(&request, &Cancellation::default());
    let file = FileLibrary::from_engine(engine);
    let preview = file.preview(file.reserve_preview(), items, output);
    let name = format!("{} {output}", case.name);
    match (expected, preview) {
        (Ok(expected), Ok(preview)) if expected.copy_content(request.items.len()).is_some() => {
            assert!(!preview.text.is_empty(), "{name}");
            assert_eq!(preview.text, expected.text, "{name}");
            assert_eq!(preview.html, expected.html, "{name}");
            assert_eq!(preview.content_hash, expected.content_hash.as_str(), "{name}");
            let copied = file
                .copy(&preview.ticket, &preview.content_hash, |content| {
                    assert_eq!(content.text, expected.text, "{name}");
                    assert_eq!(content.html, expected.html.as_deref(), "{name}");
                    Ok(())
                })
                .map_err(|_| "copy")?;
            assert_eq!(copied.text, expected.text, "{name}");
            Ok(Some(true))
        }
        (Ok(expected), Ok(preview))
            if expected.copy_content(request.items.len()).is_none() && preview.problem.is_some() =>
        {
            Ok(Some(false))
        }
        // What the engine will not copy, the picker refuses too.
        (Ok(_) | Err(_), Err(_)) => Ok(Some(false)),
        (expected, preview) => panic!(
            "{name}: engine {:?}, picker {:?}",
            expected.map(|result| result.text),
            preview.map(|preview| preview.text).map_err(|_| "refused")
        ),
    }
}

fn cases() -> Result<Vec<GoldenCase>, Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/cases");
    let mut cases = vec![];
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "json") {
            cases.push(serde_json::from_slice(&std::fs::read(path)?)?);
        }
    }
    Ok(cases)
}

#[test]
fn golden_cases_the_picker_can_ask_for_give_the_engines_result() -> Result<(), Box<dyn std::error::Error>> {
    let mut copied = 0;
    for case in cases()? {
        if same_through_the_picker(&case, case.request.clone())? == Some(true) {
            copied += 1;
        }
    }
    assert!(
        copied >= 5,
        "the picker's row covers the golden cases it can express ({copied})"
    );
    Ok(())
}

#[test]
fn every_bundled_style_previews_and_copies_what_the_engine_formats() -> Result<(), Box<dyn std::error::Error>> {
    let mut copied = 0;
    for case in cases()? {
        // Every common style the engine lists, in the normal form: a numbered
        // style writes its entry by itself.
        for output in ibis_engine::outputs::outputs() {
            if !matches!(output.output, OutputSpec::Csl { .. }) {
                continue;
            }
            let mut request = case.request.clone();
            request.output = output.output.clone();
            request.form = CitationForm::Normal;
            if same_through_the_picker(&case, request)? == Some(true) {
                copied += 1;
            }
        }
    }
    assert!(copied >= 25, "every style through several cases ({copied})");
    Ok(())
}

#[test]
fn page_and_range_with_each_picker_form_match_canonical_output() -> Result<(), Box<dyn std::error::Error>> {
    use ibis_core::contract::{CitationForm, Locator as EngineLocator, LocatorTerm};
    let case = cases()?
        .into_iter()
        .find(|case| case.name == "02-pandoc")
        .ok_or("case")?;
    let engine = engine_for(&case)?;
    let file = FileLibrary::from_engine(engine.clone());
    let outputs = [
        "pandoc",
        "autocite",
        "parencite",
        "textcite",
        "typst",
        "apa",
        "chicago-author-date",
        "harvard-cite-them-right",
        "ieee",
        "modern-language-association",
    ];
    for output in outputs {
        let spec = crate::output::spec(output);
        let style = matches!(spec, OutputSpec::Csl { .. });
        let biblatex = matches!(
            spec,
            OutputSpec::Key {
                syntax: KeySyntax::Biblatex,
                ..
            }
        );
        let entries = crate::output::listed(output).is_some_and(|listed| listed.info().entries);
        for form in ["normal", "narrative", "author-only", "year-only", "full"] {
            for page in ["12", "12–15"] {
                let mut request = case.request.clone();
                let pick = &mut request.items[0];
                pick.expected_cite = Some(case.library.objects()[0].fingerprints.cite.clone());
                pick.form = match form {
                    "narrative" => Some(CitationForm::Narrative),
                    "author-only" => Some(CitationForm::AuthorOnly),
                    "year-only" => Some(CitationForm::YearOnly),
                    "full" => Some(CitationForm::Full),
                    _ => None,
                };
                pick.suppress_author = false;
                pick.locator = Some(EngineLocator {
                    term: LocatorTerm::Page,
                    value: page.into(),
                });
                let selection = serde_json::from_value(json!({"id":pick.item,"cite":pick.expected_cite,
                    "locator":{"label":"page","value":page},"form":form}))?;
                // The engine's own output: it writes a narrative BibLaTeX citation as \\textcite.
                request.output = spec.clone();
                request.form = CitationForm::Normal;
                request.keep_order = !style;
                let expected = engine.cite(&request, &Cancellation::default()).map_err(|_| "cite")?;
                let preview = file.preview(file.reserve_preview(), vec![selection], output);
                if expected.copy_content(1).is_none() {
                    let preview = preview.map_err(|error| error.to_string())?;
                    assert!(
                        preview
                            .problem
                            .as_deref()
                            .is_some_and(|problem| problem.contains("cannot be expressed"))
                    );
                    assert!(file.copy(&preview.ticket, &preview.content_hash, |_| Ok(())).is_err());
                    continue;
                }
                let preview = preview.map_err(|e| e.to_string())?;
                assert_eq!(preview.text, expected.text, "{output} {form} {page}");
                assert_eq!(preview.html, expected.html);
                assert_eq!(preview.content_hash, expected.content_hash.as_str());
                // The one cited reference is named by its key.
                let expected_warnings = citation_warnings(&expected, &[Some("Example2026".into())]);
                assert_eq!(preview.warnings, expected_warnings, "{output} {form} {page}");
                if !expected.losses.is_empty() {
                    assert!(preview.warnings.iter().any(|warning| warning.contains("Example2026")));
                }
                file.copy(&preview.ticket, &preview.content_hash, |content| {
                    assert_eq!(content.text, expected.text);
                    assert_eq!(content.html, expected.html.as_deref());
                    assert_eq!(content.rtf, expected.rtf.as_deref());
                    Ok(())
                })
                .map_err(|e| e.to_string())?;
                // A reference entry or the name alone has no place for a page, and says so.
                if entries || matches!(form, "author-only" | "full") && !preview.text.contains("12") {
                    assert!(
                        preview
                            .warnings
                            .iter()
                            .any(|warning| warning.contains("page or location")),
                        "{output} {form} {page}: {:?}",
                        preview.warnings
                    );
                } else {
                    assert!(preview.text.contains("12"), "{output} {form} {page}: {}", preview.text);
                    if page.contains('–') {
                        assert!(preview.text.contains("15"));
                    }
                    if style {
                        assert_eq!(preview.text.contains("Taylor"), form != "year-only", "{output} {form}");
                    }
                }
                if biblatex && form == "narrative" {
                    assert!(preview.text.starts_with("\\textcite["));
                }
                if output == "pandoc" && form == "narrative" {
                    assert!(preview.text.starts_with("@Example2026 ["));
                }
            }
        }
    }
    Ok(())
}

#[test]
fn labelled_pages_use_the_shared_parser_before_preview_and_canonical_copy() -> Result<(), Box<dyn std::error::Error>> {
    let case = cases()?
        .into_iter()
        .find(|case| case.name == "02-pandoc")
        .ok_or("case")?;
    let file = FileLibrary::from_engine(engine_for(&case)?);
    let pick = &case.library.objects()[0];
    let selection = |label: &str, value: &str| {
        serde_json::from_value::<FileSelection>(json!({
            "id":pick.object.id(), "cite":pick.fingerprints.cite, "locator":{"label":label,"value":value}
        }))
    };
    for output in [
        "pandoc",
        "typst",
        "autocite",
        "apa",
        "chicago-author-date",
        "harvard-cite-them-right",
        "ieee",
        "modern-language-association",
    ] {
        for (typed, value) in [
            ("p. 12", "12"),
            ("pp. 12–15", "12–15"),
            (" PP.  iv–vi ", "iv–vi"),
            ("pp. \"١٢–١٥\"", "١٢–١٥"),
            ("pp. ١٢–١٥", "١٢–١٥"),
            ("pp. ۱۲–۱۵", "۱۲–۱۵"),
            ("pp. १२–१५", "१२–१५"),
            ("pp. １２–１５", "１２–１５"),
            ("pp. ๑๒–๑๕", "๑๒–๑๕"),
            ("pp. 𝟙𝟚–𝟙𝟝", "𝟙𝟚–𝟙𝟝"),
            ("S.12", "12"),
            ("pp. \"S.12\"", "S.12"),
            ("pp. \"pages of notes\"", "pages of notes"),
            ("pp. \"pp. 12\"", "pp. 12"),
        ] {
            let expected = file
                .preview(file.reserve_preview(), vec![selection("page", value)?], output)
                .map_err(|e| e.to_string())?;
            let actual = file
                .preview(
                    file.reserve_preview(),
                    vec![serde_json::from_value(json!({
                        "id":pick.object.id(), "cite":pick.fingerprints.cite, "locator_input":{"label":"page","text":typed}
                    }))?],
                    output,
                )
                .map_err(|e| e.to_string())?;
            assert_eq!(actual.text, expected.text, "{output}: {typed}");
            assert_eq!(actual.html, expected.html);
            assert_eq!(actual.content_hash, expected.content_hash);
            assert_eq!(actual.warnings, expected.warnings);
            file.copy(&actual.ticket, &actual.content_hash, |content| {
                assert_eq!(content.text, expected.text);
                assert_eq!(content.html, expected.html.as_deref());
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        }
    }
    // Parsing is whole-value and page-only: other locator types and literal
    // labels such as p53 keep their existing meaning.
    for (label, value) in [("chapter", "pp. 12–15"), ("page", "p53")] {
        let mut request = case.request.clone();
        request.items[0].locator = Some(serde_json::from_value(json!({"term":label,"value":value}))?);
        let expected = file
            .engine
            .cite(&request, &Cancellation::default())
            .map_err(|_| "cite")?;
        let preview = file
            .preview(file.reserve_preview(), vec![selection(label, value)?], "pandoc")
            .map_err(|e| e.to_string())?;
        assert_eq!(preview.text, expected.text);
        assert_eq!(preview.content_hash, expected.content_hash.as_str());
    }
    Ok(())
}

#[test]
fn invalid_page_editor_input_refuses_preview_and_copy_with_an_actionable_message()
-> Result<(), Box<dyn std::error::Error>> {
    let case = cases()?
        .into_iter()
        .find(|case| case.name == "02-pandoc")
        .ok_or("case")?;
    let file = FileLibrary::from_engine(engine_for(&case)?);
    let pick = &case.library.objects()[0];
    for output in ["pandoc", "typst", "autocite", "apa", "ieee"] {
        for value in [
            "",
            " ",
            "pp.",
            " p. ",
            "PP",
            "page",
            "pages",
            "pg.",
            "S.",
            "pp. \"\"",
            "pp. 12 appendix",
        ] {
            let selection = serde_json::from_value(json!({"id":pick.object.id(), "cite":pick.fingerprints.cite,
                "locator_input":{"label":"page","text":value}}))?;
            let mut writes = 0;
            let outcome = file
                .preview(file.reserve_preview(), vec![selection], output)
                .and_then(|preview| {
                    file.copy(&preview.ticket, &preview.content_hash, |_| {
                        writes += 1;
                        Ok(())
                    })
                });
            assert_eq!(writes, 0, "invalid Page {value:?} must never write clipboard content");
            let message = outcome.err().ok_or("invalid Page was copied")?.to_string();
            assert!(message.contains("Reference 1: Page"), "{message}");
            assert!(
                message.contains("page or range") && message.contains("clear Page"),
                "{message}"
            );
        }
    }
    Ok(())
}

#[test]
fn typed_locator_input_cannot_override_a_canonical_locator() -> Result<(), Box<dyn std::error::Error>> {
    let case = cases()?
        .into_iter()
        .find(|case| case.name == "02-pandoc")
        .ok_or("case")?;
    let file = FileLibrary::from_engine(engine_for(&case)?);
    let pick = &case.library.objects()[0];
    let selection = serde_json::from_value(json!({"id":pick.object.id(),"cite":pick.fingerprints.cite,
        "locator":{"label":"page","value":"S.12"},"locator_input":{"label":"page","text":"12"}}))?;
    let error = file
        .preview(file.reserve_preview(), vec![selection], "pandoc")
        .err()
        .ok_or("ambiguous page input was accepted")?;
    assert_eq!(
        error.to_string(),
        "Reference 1: a typed locator and a captured locator cannot be combined."
    );
    Ok(())
}

#[test]
fn unsupported_group_forms_refuse_and_field_losses_name_the_reference() -> Result<(), Box<dyn std::error::Error>> {
    let case = cases()?
        .into_iter()
        .find(|case| case.name == "02-pandoc")
        .ok_or("case")?;
    let file = FileLibrary::from_engine(engine_for(&case)?);
    let pick = &case.library.objects()[0];
    let selection = |form: &str| {
        serde_json::from_value::<FileSelection>(json!({"id":pick.object.id(),
        "cite":pick.fingerprints.cite,"locator":{"label":"page","value":"12"},"form":form}))
    };
    for output in ["pandoc", "autocite", "apa"] {
        let preview = file
            .preview(
                file.reserve_preview(),
                vec![selection("normal")?, selection("narrative")?],
                output,
            )
            .map_err(|e| e.to_string())?;
        assert!(
            preview
                .problem
                .as_deref()
                .is_some_and(|reason| reason.contains("Reference 2:"))
        );
        assert!(file.copy(&preview.ticket, &preview.content_hash, |_| Ok(())).is_err());
    }
    let refused = file
        .preview(
            file.reserve_preview(),
            vec![selection("normal")?, selection("year-only")?],
            "autocite",
        )
        .map_err(|e| e.to_string())?;
    assert!(
        refused
            .problem
            .as_deref()
            .is_some_and(|reason| reason.contains("Year only"))
    );
    let preview = file
        .preview(file.reserve_preview(), vec![selection("normal")?], "keys")
        .map_err(|e| e.to_string())?;
    assert!(
        preview
            .warnings
            .iter()
            .any(|warning| warning.contains("the page or location for "))
    );
    assert!(
        serde_json::from_value::<FileSelection>(
            json!({"id":pick.object.id(),"cite":pick.fingerprints.cite,"form":"invented"})
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn quoted_page_search_preserves_canonical_text_through_preview_and_copy() -> Result<(), Box<dyn std::error::Error>> {
    let case = cases()?
        .into_iter()
        .find(|case| case.name == "02-pandoc")
        .ok_or("case")?;
    let file = FileLibrary::from_engine(engine_for(&case)?);
    for value in ["S.12", "pages of notes", "pp.", "pp. 12"] {
        let reply = file.search(
            &SearchQuery {
                text: format!("Example pp. \"{value}\""),
                interpret_locators: true,
            },
            20,
            None,
            &Cancellation::default(),
        )?;
        let read = reply
            .interpretation
            .and_then(|read| read.locator)
            .ok_or("missing quoted page")?;
        assert_eq!(read.label, LocatorTerm::Page);
        assert_eq!(read.value, value);
        assert_eq!(reply.results.len(), 1);
        let row = &reply.results[0];
        for output in ["pandoc", "typst", "autocite", "apa"] {
            let mut request = case.request.clone();
            request.items[0].locator = Some(read.clone().into());
            request.output = crate::output::spec(output).clone();
            request.keep_order = matches!(request.output, OutputSpec::Key { .. });
            let expected = file
                .engine
                .cite(&request, &Cancellation::default())
                .map_err(|_| "cite")?;
            assert!(expected.copy_content(1).is_some());
            let preview = file
                .preview(
                    file.reserve_preview(),
                    vec![serde_json::from_value(
                        json!({"id":row.id,"cite":row.cite,"locator":read}),
                    )?],
                    output,
                )
                .map_err(|e| e.to_string())?;
            assert_eq!(preview.text, expected.text, "{output}: {value}");
            assert_eq!(preview.html, expected.html);
            assert_eq!(preview.content_hash, expected.content_hash.as_str());
            let mut writes = 0;
            file.copy(&preview.ticket, &preview.content_hash, |content| {
                writes += 1;
                assert_eq!(content.text, expected.text);
                assert_eq!(content.html, expected.html.as_deref());
                assert_eq!(content.rtf, expected.rtf.as_deref());
                Ok(())
            })
            .map_err(|e| e.to_string())?;
            assert_eq!(writes, 1);
        }
    }
    Ok(())
}

#[test]
fn picker_search_preserves_unicode_page_intent_through_preview_and_copy() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("synthetic.bib");
    std::fs::write(&path, "@misc{taylor,title={Synthetic study},author={Taylor, Alex}}")?;
    let engine = Engine::new().map_err(|_| "engine")?.without_file_watches();
    engine
        .dispatch(
            Method::Libraries,
            &json!({"action":"open-file","path":path}),
            &Cancellation::default(),
        )
        .map_err(|_| "open")?;
    let file = FileLibrary::from_engine(engine);
    for value in ["١٢–١٥", "۱۲–۱۵", "१२–१५", "１２–１５", "๑๒–๑๕", "𝟙𝟚–𝟙𝟝", "XII–XV"]
    {
        let reply = file.search(
            &SearchQuery {
                text: format!("Taylor pp. {value}"),
                interpret_locators: true,
            },
            20,
            None,
            &Cancellation::default(),
        )?;
        let interpretation = reply.interpretation.ok_or("picker lost page intent")?;
        assert_eq!(interpretation.search, "Taylor");
        assert_eq!(interpretation.typed, Some(format!("pp. {value}")));
        let locator = interpretation.locator.ok_or("picker lost page")?;
        assert_eq!(locator.value, value);
        assert_eq!(reply.results.len(), 1);
        let row = &reply.results[0];
        let preview = file
            .preview(
                file.reserve_preview(),
                vec![serde_json::from_value(
                    json!({"id":row.id,"cite":row.cite,"locator":locator}),
                )?],
                "pandoc",
            )
            .map_err(|e| e.to_string())?;
        assert_eq!(preview.text, format!("[@taylor, pp. {}]", value.replace('–', "-")));
        file.copy(&preview.ticket, &preview.content_hash, |content| {
            assert_eq!(content.text, preview.text);
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[test]
fn picker_rows_list_the_engines_search_hits_in_order() -> Result<(), Box<dyn std::error::Error>> {
    #[derive(Deserialize)]
    struct LocatorCase {
        query: String,
    }
    let locators: Vec<LocatorCase> = serde_json::from_str(include_str!("../../../../../../fixtures/locators.json"))?;
    for case in cases()? {
        let engine = engine_for(&case)?;
        let words: Vec<String> = case
            .library
            .objects()
            .iter()
            .filter_map(|item| match &item.object {
                LibraryObject::Record(record) => record.csl.get("title")?.as_str().map(str::to_owned),
                _ => None,
            })
            .flat_map(|title| title.split_whitespace().map(str::to_owned).collect::<Vec<_>>())
            .chain(["no such synthetic words".to_owned()])
            .chain(locators.iter().map(|case| case.query.clone()))
            .collect();
        let file = FileLibrary::from_engine(engine.clone());
        for word in words {
            for (text, interpret_locators) in [
                (word.clone(), false),
                (word.clone(), true),
                (format!("{word} pp. ١٢–١٥"), true),
                (format!("{word} page123"), true),
                (format!("{word} appendix B"), true),
                (format!("{word}, ٢٠٢٦"), true),
                (format!("{word} p."), true),
            ] {
                let reply = engine
                    .search(
                        &SearchRequest {
                            interpret_locator: interpret_locators,
                            query: text.clone(),
                            limit: 50,
                            libraries: None,
                        },
                        &Cancellation::default(),
                    )
                    .map_err(|_| "engine search")?;
                let hits: Vec<Value> = reply
                    .hits
                    .iter()
                    .map(|hit| serde_json::to_value(hit.item.object.id()))
                    .collect::<Result<_, _>>()?;
                let picker = file
                    .search(
                        &SearchQuery {
                            text: text.clone(),
                            interpret_locators,
                        },
                        50,
                        None,
                        &Cancellation::default(),
                    )
                    .map_err(|_| "picker search")?;
                assert_eq!(picker.revision, reply.snapshot.full.as_str());
                let interpretation = picker.interpretation.map(|read| {
                    (
                        read.search,
                        read.prefix,
                        read.typed,
                        read.locator.map(ibis_core::contract::Locator::from),
                    )
                });
                assert_eq!(
                    interpretation,
                    reply.shorthand.map(|read| (
                        read.search,
                        read.prefix,
                        read.locator.as_ref().map(|locator| locator.typed.clone()),
                        read.locator.map(|locator| locator.locator)
                    )),
                    "{}: {text}",
                    case.name
                );
                let rows: Vec<Value> = picker.results.iter().map(|row| serde_json::json!(row.id)).collect();
                assert_eq!(rows, hits, "{}: {text}", case.name);
            }
        }
    }
    Ok(())
}

/// The source-file goldens (fixtures/formatted/source-files, generated from
/// pinned Zotero) through the app: open the file, pick the references from
/// search rows in the file's order, and preview and copy every case the
/// picker can ask for.
#[test]
fn source_file_goldens_preview_and_copy_as_pinned_zotero_formats_them() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/formatted/source-files");
    let golden: Value = serde_json::from_slice(&std::fs::read(root.join("expected.json"))?)?;
    let keys = golden["keys"].as_array().ok_or("keys")?;
    for source in ["synthetic.bib", "synthetic.json"] {
        let file = FileLibrary::open(&root.join(source)).map_err(|_| "open")?;
        let mut items = vec![];
        for key in keys {
            let key = key.as_str().ok_or("key")?;
            let rows = file
                .search(
                    &serde_json::from_value(json!({"text":key,"interpret_locators":false}))?,
                    50,
                    None,
                    &Cancellation::default(),
                )?
                .results;
            let row = rows.into_iter().find(|row| row.key == key).ok_or(key.to_owned())?;
            items.push(json!({"id":row.id,"cite":row.cite,"locator":null}));
        }
        let mut replayed = 0;
        for case in golden["cases"].as_array().ok_or("cases")? {
            let style: String = serde_json::from_value(case["style"].clone())?;
            // A numbered style writes reference entries, as the pinned list did.
            let entries = crate::output::listed(&style).is_some_and(|output| output.info().entries);
            let form = if entries { "bibliography-only" } else { "normal" };
            if case["form"] != form {
                continue;
            }
            let name = format!("{source} {style}");
            let preview = file
                .preview(file.reserve_preview(), serde_json::from_value(json!(items))?, &style)
                .map_err(|_| name.clone())?;
            assert_eq!(preview.text, case["text"].as_str().ok_or("text")?, "{name}");
            assert_eq!(preview.html.as_deref(), case["html"].as_str(), "{name}");
            let copied = file
                .copy(&preview.ticket, &preview.content_hash, |content| {
                    assert_eq!(content.text, case["text"].as_str().unwrap_or_default(), "{name}");
                    assert_eq!(content.html, case["html"].as_str(), "{name}");
                    Ok(())
                })
                .map_err(|_| "copy")?;
            assert_eq!(copied.text, preview.text, "{name}");
            replayed += 1;
        }
        // Four styles in their normal form and a numbered one as a reference list.
        assert_eq!(replayed, 5, "{source}");
    }
    Ok(())
}

#[test]
fn typed_locators_of_every_type_and_kept_order_match_the_engine() -> Result<(), Box<dyn std::error::Error>> {
    use ibis_core::contract::{Locator as EngineLocator, LocatorTerm};
    let case = cases()?
        .into_iter()
        .find(|case| case.name == "02-pandoc")
        .ok_or("case")?;
    let engine = engine_for(&case)?;
    let file = FileLibrary::from_engine(engine.clone());
    let pick = &case.library.objects()[0];
    let cite = |locator: EngineLocator, output: &str| -> Result<CitationResult, Box<dyn std::error::Error>> {
        let mut request = case.request.clone();
        request.items.truncate(1);
        request.items[0].expected_cite = Some(pick.fingerprints.cite.clone());
        request.items[0].locator = Some(locator);
        request.output = crate::output::spec(output).clone();
        request.keep_order = matches!(request.output, OutputSpec::Key { .. });
        engine
            .cite(&request, &Cancellation::default())
            .map_err(|_| "cite".into())
    };
    let typed = |label: LocatorTerm, text: &str, output: &str| {
        let selection = serde_json::from_value(json!({"id":pick.object.id(),"cite":pick.fingerprints.cite,
            "locator_input":{"label":label,"text":text}}))?;
        file.preview(file.reserve_preview(), vec![selection], output)
            .map_err(|error| -> Box<dyn std::error::Error> { error.to_string().into() })
    };
    for output in ["pandoc", "apa", "chicago-author-date"] {
        // Every CSL locator type, with a range, as the locator field sends it.
        for term in LocatorTerm::ALL {
            let expected = cite(
                EngineLocator {
                    term,
                    value: "3–5".into(),
                },
                output,
            )?;
            let preview = typed(term, "3–5", output)?;
            assert_eq!(preview.text, expected.text, "{output} {term:?}");
            assert_eq!(
                preview.content_hash,
                expected.content_hash.as_str(),
                "{output} {term:?}"
            );
        }
        // A typed label names the type, whatever the field's pop-up said.
        let expected = cite(
            EngineLocator {
                term: LocatorTerm::Chapter,
                value: "3".into(),
            },
            output,
        )?;
        assert_eq!(
            typed(LocatorTerm::Page, "ch. 3", output)?.text,
            expected.text,
            "{output}"
        );
    }
    let refused = typed(LocatorTerm::Page, "chap.", "pandoc")
        .err()
        .ok_or("an empty chapter was cited")?;
    assert_eq!(
        refused.to_string(),
        "Reference 1: Chapter needs a value. Enter one after the label, or clear the locator."
    );
    Ok(())
}

#[test]
fn kept_order_gives_the_engines_text_and_a_style_sorts_without_it() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("invented.bib");
    std::fs::write(
        &path,
        "@book{taylor2026,title={Synthetic signals},author={Taylor, Avery},year={2026}}\n\
         @book{morgan2025,title={Synthetic noise},author={Morgan, Ann},year={2025}}\n",
    )?;
    let engine = Engine::new().map_err(|_| "engine")?.without_file_watches();
    engine
        .dispatch(
            Method::Libraries,
            &json!({"action":"open-file","path":path}),
            &Cancellation::default(),
        )
        .map_err(|_| "open library")?;
    let picked = ["taylor2026", "morgan2025"].map(|key| {
        engine.show(&ibis_engine::request::ShowRequest {
            item: ibis_engine::request::ItemSelector::Key(key.into()),
        })
    });
    let file = FileLibrary::from_engine(engine.clone());
    for (keep, text) in [
        (false, "(Morgan, 2025; Taylor, 2026)"),
        (true, "(Taylor, 2026; Morgan, 2025)"),
    ] {
        let mut items = vec![];
        let mut selections = vec![];
        for shown in &picked {
            let shown = shown.as_ref().map_err(|_| "show")?;
            items.push(json!({"item":shown.object.id(),"expected_cite":shown.fingerprints.cite}));
            selections.push(serde_json::from_value::<FileSelection>(
                json!({"id":shown.object.id(),"cite":shown.fingerprints.cite}),
            )?);
        }
        let request: CitationRequest =
            serde_json::from_value(json!({"items":items,"output":{"kind":"csl","style":"apa"},"keep_order":keep}))?;
        let expected = engine.cite(&request, &Cancellation::default()).map_err(|_| "cite")?;
        let preview = file
            .preview_for(
                file.reserve_preview(),
                selections,
                crate::output::spec("apa"),
                keep,
                None,
            )
            .map_err(|error| error.to_string())?;
        assert_eq!(
            (preview.text.as_str(), expected.text.as_str()),
            (text, text),
            "keep order {keep}"
        );
        assert_eq!(preview.content_hash, expected.content_hash.as_str());
    }
    Ok(())
}
