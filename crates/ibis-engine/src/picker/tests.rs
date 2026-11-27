use super::*;
use serde_json::json;

fn library(records: Vec<serde_json::Value>) -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invented.csljson");
    std::fs::write(&path, serde_json::to_vec(&records).unwrap()).unwrap();
    let engine = Engine::new().unwrap();
    engine.open_file_id(&path, &Cancellation::default()).unwrap();
    (dir, engine)
}

fn find(engine: &Engine, query: &str, shorthand: bool, recent: &[ItemRef], language: Language) -> PickerSearchResult {
    search(
        engine,
        PickerSearchRequest {
            query: query.into(),
            shorthand,
        },
        recent,
        language,
        &Cancellation::default(),
    )
    .unwrap()
}

#[test]
fn sections_count_the_untruncated_search_and_fill_recent_sections() {
    let (_dir, engine) = library(
        (0..53)
            .map(|n| {
                json!({
                    "id": format!("invented-{n:02}"), "title": format!("Imaginary lantern {n}"),
                    "type": "article-journal", "author": [{"family": "Vale", "given": "Mira"}],
                    "container-title": "Journal of Imaginary Lanterns", "date-added": "2026-01-05"
                })
            })
            .collect(),
    );
    let result = find(&engine, "lantern", false, &[], Language::English);
    assert_eq!(result.total, Some(53));
    assert_eq!(result.count_label.as_deref(), Some("53 results"));
    assert_eq!(result.sections[0].kind, PickerSectionKind::Best);
    assert_eq!(result.sections[0].rows.len(), 50);
    let selected = &result.sections[0].rows[0];
    assert_eq!(selected.authors, ["Mira Vale"]);
    assert_eq!(selected.first_author.as_deref(), Some("Vale"));
    assert_eq!(selected.item_type.as_deref(), Some("article-journal"));
    assert_eq!(
        selected.bylines,
        [
            "Mira Vale · Journal of Imaginary Lanterns",
            "Vale · Journal of Imaginary Lanterns",
            "Mira Vale",
            "Vale",
        ]
    );
    let recents = find(&engine, "", false, std::slice::from_ref(&selected.id), Language::German);
    assert_eq!(recents.total, None);
    assert_eq!(recents.sections[0].kind, PickerSectionKind::CitedHere);
    assert_eq!(recents.sections[0].rows[0].id, selected.id);
    assert_eq!(recents.sections[1].kind, PickerSectionKind::RecentlyAdded);
    assert_eq!(recents.sections[1].rows.len(), 12);
    assert!(recents.sections[1].rows.iter().all(|row| row.id != selected.id));
    assert_ne!(recents.sections[0].rows[0].full_byline, selected.full_byline);
}

#[test]
fn search_explains_unexpected_matches_without_labelling_ordinary_rows() {
    let (_dir, engine) = library(vec![json!({
        "id": "invented", "title": "Invented lanterns", "type": "article-journal",
        "abstract": "A fictional catalogue of peregrinations.", "keyword": "pernoctation", "date-added": "2026-01-05"
    })]);
    let reasons = |query, language| {
        find(&engine, query, false, &[], language)
            .sections
            .into_iter()
            .flat_map(|section| section.rows)
            .next()
            .unwrap_or_else(|| panic!("no result for {query:?}"))
            .match_reasons
    };
    assert_eq!(
        reasons("peregrinations", Language::English),
        ["Matched in the abstract"]
    );
    assert_eq!(reasons("peregrinations", Language::German), ["Treffer im Abstract"]);
    assert_eq!(reasons("lantrns", Language::English), ["Spelling match"]);
    assert_eq!(
        reasons("pernoctation unobtainium", Language::English),
        ["Closest match", "Matched in the keywords"]
    );
    assert!(reasons("lanterns peregrinations", Language::English).is_empty());
    assert!(reasons("", Language::English).is_empty());
}

#[test]
fn locator_recovery_names_the_reference_that_needs_editing() {
    let (_dir, engine) = library(vec![
        json!({"id":"vale", "citation-key":"vale2024", "title":"Imaginary lanterns", "type":"book"}),
        json!({"id":"fen", "citation-key":"fen2023", "title":"Invented bridges", "type":"book"}),
    ]);
    let entries: Vec<_> = find(&engine, "invent", false, &[], Language::English)
        .sections
        .into_iter()
        .flat_map(|section| section.rows)
        .map(|row| PickerPreviewEntry {
            item: row.id,
            cite: row.cite,
            form: None,
            locator: None,
            prefix: None,
            suffix: None,
        })
        .collect();
    // Use separate title searches so the two item identities are unambiguous.
    assert_eq!(entries.len(), 1);
    let mut other = find(&engine, "lanterns", false, &[], Language::English).sections;
    let row = other.remove(0).rows.remove(0);
    let owner = row.id.clone();
    let mut draft = vec![PickerPreviewEntry {
        item: row.id,
        cite: row.cite,
        form: None,
        locator: Some(Locator {
            term: ibis_core::contract::LocatorTerm::Page,
            value: "17 [appendix]".into(),
        }),
        prefix: None,
        suffix: Some("and compare".into()),
    }];
    draft.extend(entries);
    let result = preview(
        &engine,
        PickerPreviewRequest {
            entries: draft,
            format: "pandoc".into(),
            keep_order: false,
        },
        Language::English,
        Cancellation::default(),
    );
    let status = result.problem.unwrap();
    assert!(matches!(status.fix, Some(PickerFix::EditLocator)));
    assert_eq!(status.item, Some(owner));
}

#[test]
fn explicit_segments_build_the_same_group_as_the_rich_preview() {
    let (_dir, engine) = library(vec![
        json!({"id":"vale", "title":"Imaginary lanterns", "type":"article-journal",
            "author":[{"family":"Vale","given":"Mira"}], "issued":{"date-parts":[[2024]]}}),
        json!({"id":"fen", "title":"Invented bridges", "type":"book",
            "author":[{"family":"Fen","given":"Tavi"}], "issued":{"date-parts":[[2023]]}}),
    ]);
    let query = "vale p12; fen ch3;";
    let plain = find(&engine, query, false, &[], Language::English);
    assert!(plain.segments.is_empty());
    let parsed = find(&engine, query, true, &[], Language::English);
    assert_eq!(parsed.segments.len(), 2);
    let german = find(&engine, "vale Seite12; fen Kapitel3;", true, &[], Language::German);
    assert!(
        german
            .segments
            .iter()
            .all(|part| part.row.is_some() && part.locator.is_some())
    );
    let entries = parsed
        .segments
        .into_iter()
        .map(|part| {
            assert!(part.complete);
            let row = part.row.unwrap();
            PickerPreviewEntry {
                item: row.id,
                cite: row.cite,
                locator: part.locator,
                prefix: part.prefix,
                suffix: None,
                form: None,
            }
        })
        .collect();
    let preview = preview(
        &engine,
        PickerPreviewRequest {
            entries,
            format: "apa".into(),
            keep_order: true,
        },
        Language::English,
        Cancellation::default(),
    );
    assert!(preview.problem.is_none());
    assert!(preview.text.contains("12"));
    assert!(preview.text.contains("3"));
    let nodes = preview.display.nodes;
    assert_eq!(nodes.iter().filter_map(|node| node.item).collect::<Vec<_>>(), [0, 1]);
    assert_eq!(
        nodes
            .into_iter()
            .flat_map(|node| node.content)
            .map(|run| run.text)
            .collect::<String>(),
        preview.text
    );
}
