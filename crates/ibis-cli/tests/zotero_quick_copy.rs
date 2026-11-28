//! Ibis against Zotero's own Quick Copy output (fixtures/zotero/quick-copy): every
//! invented item of the mapping corpus as a Zotero item, and every item type as a
//! CSL-JSON file exported from Zotero, in each curated style.
//!
//! Ibis is compared on what a person sees: the text and the HTML of one
//! citation and one bibliography entry, after the named differences in
//! [`fold`]. Every other output Ibis gives differently is listed with both
//! texts in `differences/STYLE.json`. The test fails when a list changes, so a
//! new difference is reviewed like a snapshot and a fixed one leaves its list:
//! `IBIS_BLESS=1 cargo test -p ibis --test zotero_quick_copy` rewrites them.
use ibis_core::contract::{CitationRequest, CitationResult, LibraryObject, Snapshot};
use ibis_engine::{Cancellation, Engine};
use ibis_sources::zotero::{ZoteroItem, map_item};
use ibis_sources::zotero_source::{Library, ZoteroSource};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Cursor, path::Path, sync::Arc};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
type Views = BTreeMap<&'static str, String>;

const FIXTURE: &str = "../../fixtures/zotero/quick-copy";

/// What both sides go through before they are compared. Each step is a
/// difference Ibis makes on purpose, or one it does not cover yet (marked).
fn fold(html_or_text: &str) -> String {
    let mut text = html_or_text.to_owned();
    // The same characters, which Zotero writes again through a browser DOM:
    // `&amp;` for the processor's `&#38;`, `&nbsp;` for its own character,
    // and no layout styles (those are compared in the Word evidence instead).
    text = text.replace("&#38;", "&amp;").replace("&nbsp;", "\u{a0}");
    while let Some(start) = text.find(" style=\"") {
        let Some(end) = text[start + 8..].find('"').map(|at| start + 8 + at + 1) else {
            break;
        };
        text.replace_range(start..end, "");
    }
    // Ibis writes every line break inside a field as a space, as HTML shows it.
    text.split(['\r', '\n', '\u{85}', '\u{2028}', '\u{2029}'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_owned()
}

/// One bibliography entry's own HTML, without the `csl-entry` block around it
/// and, in a numeric style, without the number in its margin.
fn entry_html(html: &str, numeric: bool) -> String {
    // The content of the first `<div class="NAME">` and its own closing tag,
    // which is the last one: these blocks hold nothing after their content.
    let inside = |html: &str, class: &str| -> Option<String> {
        let (_, rest) = html.split_once(&format!("<div class=\"{class}\""))?;
        let (_, rest) = rest.split_once('>')?;
        let (content, _) = rest.trim_end().rsplit_once("</div>")?;
        Some(content.trim().to_owned())
    };
    let html = inside(html, "csl-bib-body").unwrap_or_else(|| html.to_owned());
    let html = inside(&html, "csl-entry").unwrap_or(html);
    if numeric {
        inside(&html, "csl-right-inline").unwrap_or(html)
    } else {
        html
    }
}

/// A bibliography's layout: each block's inline style, in order. A numeric
/// entry has no number in Ibis, so only the layout around it compares.
fn layout(html: &str, numeric: bool) -> String {
    let mut styles = vec![];
    for (at, _) in html.match_indices("<div class=\"") {
        let tag = html[at + 12..].split('>').next().unwrap_or_default();
        if let Some((class, style)) = tag.split_once("\" style=\"") {
            styles.push(format!("{class}: {}", style.trim_end_matches('"')));
        }
    }
    if numeric {
        styles.truncate(1);
    }
    styles.join(" | ")
}

/// The number in a numeric bibliography's margin, as text.
fn margin_label(html: &str) -> Option<String> {
    let (_, rest) = html.split_once("<div class=\"csl-left-margin\"")?;
    let (_, rest) = rest.split_once('>')?;
    let (label, _) = rest.split_once("</div>")?;
    Some(label.replace("&#38;", "&").replace("&amp;", "&"))
}

/// What Zotero copies, view by view; `None` when it copies nothing.
fn zotero_views(case: &Value, numeric: bool) -> Option<Views> {
    let citation = &case["citation"];
    let bibliography = &case["bibliography"];
    let html = bibliography["html"].as_str()?;
    let mut text = bibliography["text"].as_str()?.trim_end_matches(['\r', '\n']);
    if numeric && let Some(label) = margin_label(html) {
        text = text.strip_prefix(label.as_str()).unwrap_or(text);
    }
    let entry = fold(text);
    if entry.is_empty() {
        return None;
    }
    let entry_html = fold(&entry_html(html, numeric));
    let mut views = BTreeMap::new();
    // Outside a document Ibis copies a numeric style's reference entry instead
    // of a number it cannot know (ROADMAP: only correct output).
    if numeric {
        views.insert("citation text", entry.clone());
        views.insert("citation html", entry_html.clone());
    } else {
        views.insert("citation text", fold(citation["text"].as_str()?));
        views.insert("citation html", fold(citation["html"].as_str()?));
    }
    views.insert("bibliography text", entry);
    views.insert("bibliography html", entry_html);
    views.insert("bibliography layout", layout(html, numeric));
    Some(views)
}

/// What Ibis copies, view by view, or why it copies nothing.
fn ibis_views(engine: &Engine, item: &Value, style: &str, numeric: bool) -> TestResult<Result<Views, String>> {
    let mut views = BTreeMap::new();
    for (form, text, html) in [
        ("normal", "citation text", "citation html"),
        ("bibliography-only", "bibliography text", "bibliography html"),
    ] {
        let request: CitationRequest = serde_json::from_value(json!({
            "items": [item], "form": form, "output": {"kind":"csl","style":style}
        }))?;
        let result: CitationResult = match engine.cite(&request, &Cancellation::default()) {
            Ok(result) => result,
            Err(error) => return Ok(Err(format!("engine error: {:?}", error.code))),
        };
        if result.copy_content(1).is_none() {
            return Ok(Err(format!("refused: {:?}", result.outcomes)));
        }
        views.insert(text, fold(&result.text));
        let copied = result.html.as_deref().unwrap_or_default();
        views.insert(html, fold(&entry_html(copied, false)));
        if form == "bibliography-only" {
            views.insert("bibliography layout", layout(copied, numeric));
        }
    }
    Ok(Ok(views))
}

/// The engine and each case's item, read one way.
struct Reader {
    name: &'static str,
    engine: Engine,
    items: BTreeMap<String, Value>,
}

fn zotero_reader(cases: &[Value]) -> TestResult<Reader> {
    let library = ZoteroSource::default().id_of(Library::User).ok_or("library id")?;
    let mut objects = vec![];
    let mut items = BTreeMap::new();
    for case in cases {
        let item: ZoteroItem = serde_json::from_value(case["item"].clone())?;
        let mapped = map_item(library.clone(), &item).map_err(|error| format!("{error:?}"))?;
        items.insert(
            case["name"].as_str().ok_or("name")?.to_owned(),
            json!({"item": mapped.record.id}),
        );
        objects.push(LibraryObject::Record(mapped.record));
    }
    let engine = Engine::from_snapshot(Arc::new(Snapshot::new(objects)?));
    Ok(Reader {
        name: "zotero",
        engine,
        items,
    })
}

/// Every item type as a CSL-JSON file exported from Zotero, through the CLI's
/// own file loader: Ibis with no Zotero at all.
fn csl_json_reader(cases: &[Value], dir: &Path) -> TestResult<Reader> {
    let cases: Vec<&Value> = cases
        .iter()
        .filter(|case| {
            case["name"]
                .as_str()
                .is_some_and(|name| name.starts_with("type-") || name.starts_with("formatted-"))
        })
        .collect();
    let records: Vec<Value> = cases
        .iter()
        .enumerate()
        .map(|(index, case)| {
            // Each record gets its own key: several invented items share one,
            // which a file reader rightly refuses to guess between.
            let mut record = case["expected"].clone();
            record["id"] = json!(format!("item{index}"));
            record["citation-key"] = json!(format!("item{index}"));
            record
        })
        .collect();
    let path = dir.join("exported.json");
    std::fs::write(&path, serde_json::to_vec(&records)?)?;
    let engine = Engine::new().map_err(|error| format!("{error:?}"))?;
    let (mut output, mut errors) = (vec![], vec![]);
    let code = ibis::run_with(
        engine.clone(),
        ["ibis", "libraries", "--file", path.to_str().ok_or("path")?, "--json"]
            .map(Into::into)
            .into(),
        Cursor::new(vec![]),
        &mut output,
        &mut errors,
        ibis::OutputMode::Plain,
    );
    // Exit 1 reports, as findings, the Zotero fields CSL-JSON readers do not use.
    assert!(code <= 1, "{}", String::from_utf8_lossy(&errors));
    let envelope: Value = serde_json::from_slice(&output)?;
    assert_eq!(
        envelope["data"]["libraries"][0]["objects"],
        records.len(),
        "every record read"
    );
    let snapshot = engine.snapshot().map_err(|error| format!("{error:?}"))?;
    let mut by_key = BTreeMap::new();
    for object in snapshot.objects() {
        if let LibraryObject::Record(record) = &object.object {
            let key = record.citation_key()?.ok_or("record without key")?.to_owned();
            by_key.insert(key, json!({"item": record.id}));
        }
    }
    let mut items = BTreeMap::new();
    for (index, case) in cases.iter().enumerate() {
        let item = by_key.remove(&format!("item{index}")).ok_or("CSL-JSON lost a record")?;
        items.insert(case["name"].as_str().ok_or("name")?.to_owned(), item);
    }
    Ok(Reader {
        name: "csl-json",
        engine,
        items,
    })
}

fn compare(style: &str) -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(root.join("expected.json"))?)?;
    let mapping: Value = serde_json::from_str(include_str!("../../../fixtures/zotero/mapping/cases.json"))?;
    let mapping = mapping.as_array().ok_or("mapping cases")?;
    let numeric = fixture["formats"][style] == "numeric";
    let dir = tempfile::tempdir()?;
    let readers = [zotero_reader(mapping)?, csl_json_reader(mapping, dir.path())?];
    let cases: Vec<&Value> = fixture["cases"]
        .as_array()
        .ok_or("cases")?
        .iter()
        .filter(|case| case["style"] == style)
        .collect();
    assert_eq!(
        cases.len(),
        mapping.len(),
        "{style}: one Quick Copy case for each invented item"
    );
    let (mut differences, mut compared, mut matched) = (vec![], 0usize, 0usize);
    // Ibis refuses to cite a record with neither title nor author (only correct
    // output) where Zotero prints what little it has, often only "(n.d.)".
    let mut missing_details = vec![];
    for case in cases {
        let name = case["name"].as_str().ok_or("name")?;
        let zotero = if case.get("error").is_some() {
            None
        } else {
            zotero_views(case, numeric)
        };
        for reader in &readers {
            let Some(item) = reader.items.get(name) else { continue };
            let ibis = ibis_views(&reader.engine, item, style, numeric)?;
            let before = differences.len();
            let mut differ = |view: &str, zotero: Value, ibis: Value| {
                differences
                    .push(json!({"name": name, "reader": reader.name, "view": view, "zotero": zotero, "ibis": ibis}));
            };
            compared += 1;
            match (&zotero, ibis) {
                (None, Err(_)) => {}
                (Some(_), Err(error)) if error.contains("MissingDetails") => {
                    missing_details.push(format!("{}:{name}", reader.name));
                    continue;
                }
                (None, Ok(ibis)) => {
                    let said = case["error"]
                        .as_str()
                        .map_or_else(|| "nothing".to_owned(), |error| format!("error: {error}"));
                    differ("outcome", json!(said), json!(ibis["bibliography text"]));
                }
                (Some(zotero), Err(error)) => differ("outcome", json!(zotero["bibliography text"]), json!(error)),
                (Some(zotero), Ok(ibis)) => {
                    for (view, expected) in zotero {
                        if ibis[view] != *expected {
                            differ(view, json!(expected), json!(ibis[view]));
                        }
                    }
                }
            }
            if differences.len() == before {
                matched += 1;
            }
        }
    }
    let report = format!(
        "{{\"style\":{style:?},\"compared\":{compared},\"matched\":{matched},\"missing_details\":{},\"differences\":[\n{}\n]}}\n",
        json!(missing_details),
        differences.iter().map(Value::to_string).collect::<Vec<_>>().join(",\n")
    );
    let path = root.join("differences").join(format!("{style}.json"));
    if std::env::var_os("IBIS_BLESS").is_some() {
        std::fs::create_dir_all(root.join("differences"))?;
        std::fs::write(&path, &report)?;
    }
    let recorded = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        recorded == report,
        "{style}: Ibis's differences from Zotero changed; review them, then rerun with IBIS_BLESS=1"
    );
    Ok(())
}

macro_rules! styles {
    ($($test:ident: $style:literal,)*) => {
        // The formatter is the same JavaScript on every system, and the other
        // golden tests replay it there; the whole corpus runs once, on Linux.
        $(#[test]
        #[cfg_attr(not(target_os = "linux"), ignore = "the corpus runs on Linux")]
        fn $test() -> TestResult {
            compare($style)
        })*

        #[test]
        fn every_curated_style_is_compared() -> TestResult {
            let fixture: Value = serde_json::from_str(&std::fs::read_to_string(
                Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE).join("expected.json"),
            )?)?;
            let mut styles: Vec<&str> = fixture["formats"].as_object().ok_or("formats")?.keys().map(String::as_str).collect();
            styles.sort_unstable();
            let mut tested = vec![$($style),*];
            tested.sort_unstable();
            assert_eq!(styles, tested);
            Ok(())
        }
    };
}

styles! {
    apa: "apa",
    chicago_author_date: "chicago-author-date",
    chicago_notes_bibliography: "chicago-notes-bibliography",
    modern_language_association: "modern-language-association",
    harvard_cite_them_right: "harvard-cite-them-right",
    ieee: "ieee",
    vancouver_nlm: "vancouver-nlm",
    nature: "nature",
    american_medical_association: "american-medical-association",
    american_chemical_society: "american-chemical-society",
    gb_t_7714_numeric: "china-national-standard-gb-t-7714-2025-numeric",
    gb_t_7714_author_date: "china-national-standard-gb-t-7714-2025-author-date",
    gb_t_7714_note: "china-national-standard-gb-t-7714-2025-note",
}
