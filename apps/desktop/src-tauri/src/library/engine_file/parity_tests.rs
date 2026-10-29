//! The app's row of "same results from every front end" (#1200). The picker
//! exposes two engine methods: `search` (its result rows) and `cite` (its
//! preview and copy). Each golden case the picker can express runs through
//! them and must give what the engine gives for the same request.
use super::*;
use ibis_core::contract::{GoldenCase, Snapshot};
use std::sync::Arc;

/// The original normal-form golden subset. Page/form combinations have their
/// own matrix below; other syntaxes and affixes remain outside the picker.
fn picker_choice(request: &CitationRequest) -> Option<(Vec<FileSelection>, KeyFormat, Option<CitationStyle>)> {
    let (format, style) = match serde_json::to_value(&request.output).ok()? {
        output if output["kind"] == "csl" => (
            KeyFormat::Pandoc,
            Some(serde_json::from_value(output["style"].clone()).ok()?),
        ),
        output => (
            match (output["syntax"].as_str()?, output["command"].as_str()) {
                ("pandoc", None) => KeyFormat::Pandoc,
                ("biblatex", Some("autocite")) => KeyFormat::Autocite,
                ("biblatex", Some("parencite")) => KeyFormat::Parencite,
                ("biblatex", Some("textcite")) => KeyFormat::Textcite,
                ("latex", Some("cite")) => KeyFormat::Cite,
                ("typst", None) => KeyFormat::Typst,
                ("keys", None) => KeyFormat::Keys,
                _ => return None,
            },
            None,
        ),
    };
    let form = if style == Some(CitationStyle::Ieee) {
        "bibliography-only"
    } else {
        "normal"
    };
    if serde_json::to_value(request.form).ok()? != form || request.items.is_empty() {
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
                id: item.item.clone(),
                cite: item.expected_cite.clone()?,
                form: None,
                locator: item.locator.clone().map(Into::into),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some((items, format, style))
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
            .publish_library(library, Arc::new(Snapshot::new(objects)?))
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
    let Some((items, format, style)) = picker_choice(&request) else {
        return Ok(None);
    };
    // The picker keeps key order; formatted styles sort as the style says.
    request.keep_order = style.is_none();
    let expected = engine.cite(&request, &Cancellation::default());
    let file = FileLibrary::from_engine(engine);
    let preview = file.preview(file.reserve_preview(), items, format, style);
    let name = format!("{} {:?}", case.name, style);
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
        for style in [
            "apa",
            "chicago-author-date",
            "harvard-cite-them-right",
            "ieee",
            "modern-language-association",
        ] {
            let mut request = case.request.clone();
            request.output = serde_json::from_value(json!({"kind":"csl","style":style}))?;
            request.form = serde_json::from_value(json!(if style == "ieee" { "bibliography-only" } else { "normal" }))?;
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
        (KeyFormat::Pandoc, None, "pandoc", None),
        (KeyFormat::Autocite, None, "biblatex", Some("autocite")),
        (KeyFormat::Parencite, None, "biblatex", Some("parencite")),
        (KeyFormat::Textcite, None, "biblatex", Some("textcite")),
        (KeyFormat::Typst, None, "typst", None),
        (KeyFormat::Pandoc, Some(CitationStyle::Apa), "", None),
        (KeyFormat::Pandoc, Some(CitationStyle::ChicagoAuthorDate), "", None),
        (KeyFormat::Pandoc, Some(CitationStyle::HarvardCiteThemRight), "", None),
        (KeyFormat::Pandoc, Some(CitationStyle::Ieee), "", None),
        (
            KeyFormat::Pandoc,
            Some(CitationStyle::ModernLanguageAssociation),
            "",
            None,
        ),
    ];
    for (format, style, syntax, command) in outputs {
        for form in ["normal", "narrative", "author-omitted"] {
            for page in ["12", "12–15"] {
                let mut request = case.request.clone();
                let pick = &mut request.items[0];
                pick.expected_cite = Some(case.library.objects()[0].fingerprints.cite.clone());
                pick.form = (form == "narrative").then_some(CitationForm::Narrative);
                pick.suppress_author = form == "author-omitted";
                pick.locator = Some(EngineLocator {
                    term: LocatorTerm::Page,
                    value: page.into(),
                });
                let selection = serde_json::from_value(json!({"id":pick.item,"cite":pick.expected_cite,
                    "locator":{"label":"page","value":page},"form":form}))?;
                let command = if syntax == "biblatex" && form == "narrative" {
                    Some("textcite")
                } else {
                    command
                };
                request.output = serde_json::from_value(if let Some(style) = style {
                    json!({"kind":"csl","style":style})
                } else {
                    json!({"kind":"key","syntax":syntax,"command":command})
                })?;
                request.form = if style == Some(CitationStyle::Ieee) {
                    CitationForm::BibliographyOnly
                } else {
                    CitationForm::Normal
                };
                request.keep_order = style.is_none();
                let expected = engine.cite(&request, &Cancellation::default()).map_err(|_| "cite")?;
                let preview = file.preview(file.reserve_preview(), vec![selection], format, style);
                if style == Some(CitationStyle::Ieee) && form == "narrative" {
                    assert!(
                        preview
                            .err()
                            .ok_or("numeric narrative accepted")?
                            .to_string()
                            .contains("cannot be expressed")
                    );
                    continue;
                }
                let preview = preview.map_err(|e| e.to_string())?;
                assert_eq!(preview.text, expected.text, "{format:?} {style:?} {form} {page}");
                assert_eq!(preview.html, expected.html);
                assert_eq!(preview.content_hash, expected.content_hash.as_str());
                assert_eq!(preview.warnings, citation_warnings(&expected));
                file.copy(&preview.ticket, &preview.content_hash, |content| {
                    assert_eq!(content.text, expected.text);
                    assert_eq!(content.html, expected.html.as_deref());
                    assert_eq!(content.rtf, expected.rtf.as_deref());
                    Ok(())
                })
                .map_err(|e| e.to_string())?;
                if style == Some(CitationStyle::Ieee) {
                    assert!(preview.warnings.iter().any(|warning| warning.contains("Page")));
                } else {
                    assert!(preview.text.contains("12"));
                    if page.contains('–') {
                        assert!(preview.text.contains("15"));
                    }
                    if style.is_some() {
                        assert_eq!(preview.text.contains("Taylor"), form != "author-omitted");
                    }
                }
                if form == "author-omitted" && (format == KeyFormat::Textcite || style == Some(CitationStyle::Ieee)) {
                    assert!(
                        preview
                            .warnings
                            .iter()
                            .any(|warning| warning.contains("Reference 1: Omit author"))
                    );
                }
                if syntax == "biblatex" && form == "narrative" {
                    assert!(preview.text.starts_with("\\textcite["));
                }
                if syntax == "pandoc" && form == "narrative" {
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
    for (format, style) in [
        (KeyFormat::Pandoc, None),
        (KeyFormat::Typst, None),
        (KeyFormat::Autocite, None),
        (KeyFormat::Pandoc, Some(CitationStyle::Apa)),
        (KeyFormat::Pandoc, Some(CitationStyle::ChicagoAuthorDate)),
        (KeyFormat::Pandoc, Some(CitationStyle::HarvardCiteThemRight)),
        (KeyFormat::Pandoc, Some(CitationStyle::Ieee)),
        (KeyFormat::Pandoc, Some(CitationStyle::ModernLanguageAssociation)),
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
        ] {
            let expected = file
                .preview(file.reserve_preview(), vec![selection("page", value)?], format, style)
                .map_err(|e| e.to_string())?;
            let actual = file
                .preview(file.reserve_preview(), vec![selection("page", typed)?], format, style)
                .map_err(|e| e.to_string())?;
            assert_eq!(actual.text, expected.text, "{format:?} {style:?}: {typed}");
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
            .preview(
                file.reserve_preview(),
                vec![selection(label, value)?],
                KeyFormat::Pandoc,
                None,
            )
            .map_err(|e| e.to_string())?;
        assert_eq!(preview.text, expected.text);
        assert_eq!(preview.content_hash, expected.content_hash.as_str());
    }
    Ok(())
}

#[test]
fn label_only_pages_refuse_preview_and_copy_with_an_actionable_message() -> Result<(), Box<dyn std::error::Error>> {
    let case = cases()?
        .into_iter()
        .find(|case| case.name == "02-pandoc")
        .ok_or("case")?;
    let file = FileLibrary::from_engine(engine_for(&case)?);
    let pick = &case.library.objects()[0];
    for (format, style) in [
        (KeyFormat::Pandoc, None),
        (KeyFormat::Typst, None),
        (KeyFormat::Autocite, None),
        (KeyFormat::Pandoc, Some(CitationStyle::Apa)),
        (KeyFormat::Pandoc, Some(CitationStyle::Ieee)),
    ] {
        for value in [
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
                "locator":{"label":"page","value":value}}))?;
            let mut writes = 0;
            let outcome = file
                .preview(file.reserve_preview(), vec![selection], format, style)
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
    for (format, style) in [
        (KeyFormat::Pandoc, None),
        (KeyFormat::Autocite, None),
        (KeyFormat::Pandoc, Some(CitationStyle::Apa)),
    ] {
        let error = file
            .preview(
                file.reserve_preview(),
                vec![selection("normal")?, selection("narrative")?],
                format,
                style,
            )
            .err()
            .ok_or("mixed narrative accepted")?;
        assert!(error.to_string().contains("Reference 2:"));
    }
    let preview = file
        .preview(
            file.reserve_preview(),
            vec![selection("normal")?, selection("author-omitted")?],
            KeyFormat::Autocite,
            None,
        )
        .map_err(|e| e.to_string())?;
    assert!(
        preview
            .warnings
            .iter()
            .any(|warning| warning.contains("Reference 2: Omit author"))
    );
    let preview = file
        .preview(
            file.reserve_preview(),
            vec![selection("author-omitted")?],
            KeyFormat::Keys,
            None,
        )
        .map_err(|e| e.to_string())?;
    assert!(preview.warnings.iter().any(|warning| warning.contains("Page")));
    assert!(preview.warnings.iter().any(|warning| warning.contains("Omit author")));
    assert!(
        serde_json::from_value::<FileSelection>(
            json!({"id":pick.object.id(),"cite":pick.fingerprints.cite,"form":"invented"})
        )
        .is_err()
    );
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
        assert_eq!(interpretation.typed, format!("pp. {value}"));
        assert_eq!(interpretation.locator.value, value);
        assert_eq!(reply.results.len(), 1);
        let row = &reply.results[0];
        let preview = file
            .preview(
                file.reserve_preview(),
                vec![serde_json::from_value(
                    json!({"id":row.id,"cite":row.cite,"locator":interpretation.locator}),
                )?],
                KeyFormat::Pandoc,
                None,
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
                        read.typed,
                        ibis_core::contract::Locator::from(read.locator),
                    )
                });
                assert_eq!(
                    interpretation,
                    reply.locator.map(|read| (read.search, read.typed, read.locator)),
                    "{}: {text}",
                    case.name
                );
                let rows: Vec<Value> = picker.results.iter().map(|row| row.id.clone()).collect();
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
            let style: CitationStyle = serde_json::from_value(case["style"].clone())?;
            let form = if style == CitationStyle::Ieee {
                "bibliography-only"
            } else {
                "normal"
            };
            if case["form"] != form {
                continue;
            }
            let name = format!("{source} {style:?}");
            let preview = file
                .preview(
                    file.reserve_preview(),
                    serde_json::from_value(json!(items))?,
                    KeyFormat::Pandoc,
                    Some(style),
                )
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
        // Four styles in their normal form and IEEE as a reference list.
        assert_eq!(replayed, 5, "{source}");
    }
    Ok(())
}
