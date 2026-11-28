use super::*;
use crate::format::FormatFrom;
use serde_json::json;

fn library(records: Vec<serde_json::Value>) -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invented.csljson");
    std::fs::write(&path, serde_json::to_vec(&records).unwrap()).unwrap();
    let engine = Engine::new().unwrap();
    engine.open_file_id(&path, &Cancellation::default()).unwrap();
    (dir, engine)
}

fn find(engine: &Engine, query: &str, recent: &[ItemRef], language: Language) -> PickerSearchResult {
    search(engine, query, recent, language, &Cancellation::default()).unwrap()
}

fn scope(format: &str) -> PickerFormatScope {
    PickerFormatScope {
        format: Some(format.into()),
        from: FormatFrom::Yours,
        usual_format: Some(format.into()),
        remembers_for: None,
        remembered_for: None,
        project: None,
        requires_bibliography: false,
    }
}

fn session(engine: &Engine, format: &str) -> PickerSession {
    let mut session = PickerSession::default();
    session.open(engine, true, scope(format), Vec::new(), Language::English, false);
    session
}

fn act(session: &mut PickerSession, engine: &Engine, intent: serde_json::Value) -> PickerState {
    session.act(engine, serde_json::from_value(intent).unwrap()).unwrap()
}

fn two_books() -> (tempfile::TempDir, Engine) {
    library(vec![
        json!({"id":"vale", "citation-key":"vale2024", "title":"Imaginary lanterns", "type":"article-journal",
            "author":[{"family":"Vale","given":"Mira"}], "issued":{"date-parts":[[2024]]}}),
        json!({"id":"fen", "citation-key":"fen2023", "title":"Invented bridges", "type":"book",
            "author":[{"family":"Fen","given":"Tavi"}], "issued":{"date-parts":[[2023]]}}),
    ])
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
    let result = find(&engine, "lantern", &[], Language::English);
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
    let recents = find(&engine, "", std::slice::from_ref(&selected.id), Language::German);
    assert_eq!(recents.count_label, None);
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
        find(&engine, query, &[], language)
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
    let (_dir, engine) = two_books();
    let mut picker = session(&engine, "pandoc");
    act(&mut picker, &engine, json!({"intent":"type","query":"lanterns"}));
    let state = act(&mut picker, &engine, json!({"intent":"add"}));
    let owner = state.citation[0].row.id.clone();
    let state = act(
        &mut picker,
        &engine,
        json!({"intent":"details","item":owner,"details":{"locator":{"term":"page","value":"17 [appendix]"},
            "prefix":null,"suffix":"and compare","form":null}}),
    );
    let status = state.preview.unwrap().problem.unwrap();
    assert!(matches!(status.fix, Some(PickerFix::EditLocator)));
    assert_eq!(status.item, Some(owner));
}

#[test]
fn typed_references_read_as_the_citation_that_will_be_delivered() {
    let (_dir, engine) = two_books();
    let mut picker = session(&engine, "apa");
    let state = act(
        &mut picker,
        &engine,
        json!({"intent":"type","query":"vale p12; fen ch3"}),
    );
    assert_eq!(state.query, "vale p12; fen ch3");
    assert_eq!(state.citation.len(), 2);
    assert!(
        state
            .citation
            .iter()
            .all(|entry| !entry.added && entry.details.locator.is_some())
    );
    assert_eq!(state.citation[0].note.as_deref(), Some("p. 12"));
    assert_eq!(state.reads_as.as_deref(), Some("Chapter 3 · then 1 more reference"));
    let preview = state.preview.unwrap();
    assert!(preview.problem.is_none());
    assert!(preview.text.contains("12") && preview.text.contains('3'));
    let nodes = preview.display.nodes;
    let mut owners: Vec<_> = nodes.iter().filter_map(|node| node.item).collect();
    owners.sort_unstable();
    assert_eq!(owners, [0, 1]);
    assert_eq!(
        nodes
            .into_iter()
            .flat_map(|node| node.content)
            .map(|run| run.text)
            .collect::<String>(),
        preview.text
    );
    let german = {
        let mut picker = PickerSession::default();
        picker.open(&engine, true, scope("apa"), Vec::new(), Language::German, false);
        act(
            &mut picker,
            &engine,
            json!({"intent":"type","query":"vale Seite12; fen Kapitel3"}),
        )
    };
    assert!(german.citation.iter().all(|entry| entry.details.locator.is_some()));
    // A signal is read only before more words; a plain title search is left alone.
    let state = act(&mut picker, &engine, json!({"intent":"type","query":"see vale"}));
    assert_eq!(state.citation[0].details.prefix.as_deref(), Some("see"));
    let state = act(
        &mut picker,
        &engine,
        json!({"intent":"type","query":"invented bridges"}),
    );
    assert_eq!(state.reads_as, None);
}

#[test]
fn tab_adds_and_the_citation_keeps_its_own_order_with_undo() {
    let (_dir, engine) = two_books();
    let mut picker = session(&engine, "apa");
    act(&mut picker, &engine, json!({"intent":"type","query":"vale 12"}));
    let state = act(&mut picker, &engine, json!({"intent":"add"}));
    assert_eq!(state.query, "");
    assert_eq!(
        state.citation.len(),
        1,
        "the recent list beside an added citation is not cited"
    );
    assert!(state.citation[0].added);
    assert_eq!(state.citation[0].note.as_deref(), Some("p. 12"));
    let vale = state.citation[0].row.id.clone();
    act(&mut picker, &engine, json!({"intent":"type","query":"fen"}));
    let typing = act(&mut picker, &engine, json!({"intent":"type","query":"fen"}));
    assert_eq!(
        typing.citation.len(),
        2,
        "Return would also cite the highlighted reference"
    );
    let state = act(&mut picker, &engine, json!({"intent":"add"}));
    let fen = state.citation[1].row.id.clone();
    assert!(state.citation.iter().all(|entry| entry.added));
    let state = act(&mut picker, &engine, json!({"intent":"reorder","item":fen,"step":-1}));
    assert_eq!(state.citation[0].row.id, fen);
    assert!(state.keep_order);
    let state = act(&mut picker, &engine, json!({"intent":"remove","item":vale}));
    assert_eq!(state.citation.len(), 1);
    let state = act(&mut picker, &engine, json!({"intent":"undo"}));
    assert_eq!(state.citation.len(), 2);
    assert!(!state.can_undo);
    let first = state.revision;
    let state = act(&mut picker, &engine, json!({"intent":"move","step":1}));
    assert!(state.revision > first);
    picker.delivered();
    let state = picker.open(&engine, true, scope("apa"), Vec::new(), Language::English, true);
    assert!(state.citation.len() <= 1 && state.citation.iter().all(|entry| !entry.added));
}

#[test]
fn a_format_used_once_drops_forms_it_cannot_write() {
    let (_dir, engine) = two_books();
    let mut picker = session(&engine, "apa");
    act(&mut picker, &engine, json!({"intent":"type","query":"vale"}));
    let state = act(&mut picker, &engine, json!({"intent":"add"}));
    let vale = state.citation[0].row.id.clone();
    let state = act(
        &mut picker,
        &engine,
        json!({"intent":"details","item":vale,"details":{"locator":null,"prefix":null,"suffix":null,"form":"narrative"}}),
    );
    assert_eq!(state.citation[0].details.form, Some(CitationForm::Narrative));
    let state = act(&mut picker, &engine, json!({"intent":"use-format","id":"cite"}));
    assert!(state.format_held);
    assert_eq!(state.format.as_deref(), Some("cite"));
    assert!(state.citation[0].details.form.is_none() || state.forms.contains(&CitationForm::Narrative));
    assert!(
        picker
            .act(
                &engine,
                serde_json::from_value(json!({"intent":"use-format","id":"nothing"})).unwrap()
            )
            .is_err()
    );
}

#[test]
fn a_name_of_many_words_offers_a_few_short_bylines() {
    let name = "a ".repeat(20_000);
    let (_dir, engine) = library(vec![
        json!({"id":"long", "title":"Invented wordy body", "type":"report",
        "author":[{"literal": name}], "issued":{"date-parts":[[2020]]}}),
    ]);
    let found = find(&engine, "wordy", &[], Language::English);
    let row = &found.sections[0].rows[0];
    assert!(row.bylines.len() <= 12, "{} bylines", row.bylines.len());
    let shortened: Vec<_> = row.bylines.iter().filter(|line| line.ends_with('…')).collect();
    assert!(!shortened.is_empty());
    assert!(shortened.iter().all(|line| line.len() <= 200));
}

fn ids(engine: &Engine) -> (ItemRef, ItemRef) {
    let id = |query| {
        find(engine, query, &[], Language::English).sections[0].rows[0]
            .id
            .clone()
    };
    (id("vale"), id("fen"))
}

fn with_recent(engine: &Engine, recent: Vec<ItemRef>) -> PickerSession {
    let mut session = PickerSession::default();
    session.open(engine, true, scope("apa"), recent, Language::English, false);
    session
}

#[test]
fn a_trailing_semicolon_cites_only_what_was_typed() {
    let (_dir, engine) = two_books();
    let (vale, fen) = ids(&engine);
    let mut picker = with_recent(&engine, vec![fen.clone()]);
    let state = act(&mut picker, &engine, json!({"intent":"type","query":"vale;"}));
    assert_eq!(
        state.highlight.as_ref(),
        Some(&fen),
        "the recent list shows after the semicolon"
    );
    let cited: Vec<_> = state.citation.iter().map(|entry| entry.row.id.clone()).collect();
    assert_eq!(cited, std::slice::from_ref(&vale));
    let state = act(&mut picker, &engine, json!({"intent":"add"}));
    let added: Vec<_> = state.citation.iter().map(|entry| entry.row.id.clone()).collect();
    assert_eq!(added, [vale], "Tab adds what was typed, not the recent row beside it");
}

#[test]
fn tab_adds_a_second_reference_from_the_recent_list() {
    let (_dir, engine) = two_books();
    let (vale, fen) = ids(&engine);
    let mut picker = with_recent(&engine, vec![vale.clone(), fen.clone()]);
    let state = act(&mut picker, &engine, json!({"intent":"highlight","item":vale}));
    assert_eq!(state.highlight.as_ref(), Some(&vale));
    act(&mut picker, &engine, json!({"intent":"add"}));
    let state = act(&mut picker, &engine, json!({"intent":"highlight","item":fen}));
    assert_eq!(state.citation.len(), 1, "Return cites only what was added");
    let state = act(&mut picker, &engine, json!({"intent":"add"}));
    let cited: Vec<_> = state.citation.iter().map(|entry| entry.row.id.clone()).collect();
    assert_eq!(cited, [vale, fen]);
}

#[test]
fn editing_one_typed_reference_keeps_the_others() {
    let (_dir, engine) = two_books();
    let (vale, fen) = ids(&engine);
    let mut picker = session(&engine, "apa");
    let typed = act(&mut picker, &engine, json!({"intent":"type","query":"vale 12; fen"}));
    assert_eq!(typed.citation.len(), 2);
    let state = act(
        &mut picker,
        &engine,
        json!({"intent":"details","item":fen,"details":{"locator":{"term":"chapter","value":"3"},
            "prefix":null,"suffix":null,"form":null}}),
    );
    assert_eq!(state.query, "");
    let cited: Vec<_> = state
        .citation
        .iter()
        .map(|entry| (entry.row.id.clone(), entry.added))
        .collect();
    assert_eq!(cited, [(vale, true), (fen, true)]);
    assert_eq!(state.citation[0].note.as_deref(), Some("p. 12"));
}

#[test]
fn an_unmatched_reference_blocks_the_citation() {
    let (_dir, engine) = two_books();
    let (_, fen) = ids(&engine);
    let mut picker = with_recent(&engine, vec![fen]);
    let state = act(
        &mut picker,
        &engine,
        json!({"intent":"type","query":"nobody-invented;"}),
    );
    assert!(state.citation.is_empty(), "the recent row is not cited in its place");
    assert!(state.preview.is_none(), "nothing can be delivered");
    assert!(!state.status.is_empty(), "the person is told what to fix");
    let state = act(&mut picker, &engine, json!({"intent":"add"}));
    assert!(state.citation.is_empty(), "Tab adds nothing either");
}

#[test]
fn a_pasted_list_cites_every_key_in_order() {
    let (_dir, engine) = two_books();
    let (vale, fen) = ids(&engine);
    let mut picker = session(&engine, "apa");
    let state = act(
        &mut picker,
        &engine,
        json!({"intent":"type","query":"fen2023, vale2024"}),
    );
    let cited: Vec<_> = state.citation.iter().map(|entry| entry.row.id.clone()).collect();
    assert_eq!(cited, [fen, vale]);
    let state = act(
        &mut picker,
        &engine,
        json!({"intent":"type","query":"@fen2023 @nobody2020"}),
    );
    assert!(state.preview.is_none(), "a missing key is never dropped silently");
    assert!(!state.status.is_empty());
}
