//! The app's row of "same results from every front end" (#1200). The picker
//! exposes two engine methods: `search` (its result rows) and `cite` (its
//! preview and copy). Each golden case the picker can express runs through
//! them and must give what the engine gives for the same request.
use super::*;
use ibis_core::contract::{GoldenCase, Snapshot};
use std::sync::Arc;

/// The picker's choice for a request, or `None` when the picker cannot ask
/// for it: other key syntaxes, affixes, omitted authors or another form.
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
                id: serde_json::to_value(&item.item).ok()?,
                cite: item.expected_cite.as_ref()?.as_str().to_owned(),
                locator: match &item.locator {
                    Some(locator) => {
                        let locator = serde_json::to_value(locator).ok()?;
                        Some(serde_json::from_value(json!({"label":locator["term"],"value":locator["value"]})).ok()?)
                    }
                    None => None,
                },
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
fn picker_rows_list_the_engines_search_hits_in_order() -> Result<(), Box<dyn std::error::Error>> {
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
            .collect();
        let file = FileLibrary::from_engine(engine.clone());
        for word in words {
            let hits: Vec<Value> = engine
                .search(
                    &SearchRequest {
                        query: word.clone(),
                        limit: 50,
                        libraries: None,
                    },
                    &Cancellation::default(),
                )
                .map_err(|_| "engine search")?
                .hits
                .iter()
                .map(|hit| serde_json::to_value(hit.item.object.id()))
                .collect::<Result<_, _>>()?;
            let rows: Vec<Value> = file
                .search(
                    &serde_json::from_value(json!({"text":word,"interpret_locators":false}))?,
                    50,
                    None,
                )
                .map_err(|_| "picker search")?
                .results
                .iter()
                .map(|row| row.id.clone())
                .collect();
            assert_eq!(rows, hits, "{}: {word}", case.name);
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
