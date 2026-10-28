use super::*;
use std::path::Path;

use crate::library::engine_file::FileLibrary;

fn library(directory: &Path, bib: &str) -> FileLibrary {
    let path = directory.join("synthetic.bib");
    std::fs::write(&path, bib).unwrap_or_else(|e| panic!("{e}"));
    FileLibrary::open(&path).unwrap_or_else(|e| panic!("synthetic library: {e}"))
}

fn query(text: &str, interpret_locators: bool) -> SearchQuery {
    SearchQuery {
        text: text.into(),
        interpret_locators,
    }
}

fn search(file: &FileLibrary, text: &str, scope: Option<Vec<String>>) -> SearchReply {
    file.search(&query(text, false), usize::MAX, scope, &Cancellation::default())
        .unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn locator_search_can_be_literal_or_interpreted_and_an_empty_query_finds_nothing() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let file = library(
        directory.path(),
        "@misc{plain,title={Reference}}\n@misc{literal,title={Reference p 12}}",
    );
    for interpret_locators in [true, false] {
        let reply = file
            .search(
                &query("Reference p 12", interpret_locators),
                20,
                None,
                &Cancellation::default(),
            )
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(reply.results.len(), if interpret_locators { 2 } else { 1 });
        assert_eq!(reply.interpretation.is_some(), interpret_locators);
        assert!(reply.results.iter().all(|result| result.revision == reply.revision));
        let json = serde_json::to_value(&reply).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(json.get("interpretation").is_some(), interpret_locators);
    }
    for text in ["", "   "] {
        let reply = file
            .search(&query(text, true), 20, None, &Cancellation::default())
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(reply.results.is_empty());
        assert_eq!(reply.revision, file.revision());
    }
}

#[test]
fn pasted_lists_precede_locator_interpretation_and_keep_missing_keys() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let file = library(
        directory.path(),
        "@misc{b,title={Second}}\n@misc{12,title={Numeric key}}\n@misc{close,title={b 12}}",
    );
    for text in ["b,12", "[@b; @12, p. 4]", "\\cite{b,12}"] {
        let reply = file
            .search(&query(text, true), 1, None, &Cancellation::default())
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            reply.results.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(),
            ["b", "12"]
        );
        assert!(reply.interpretation.is_none());
        assert!(reply.list.as_ref().is_some_and(|list| list.missing.is_empty()));
        assert!(reply.results.iter().all(|r| r.revision == reply.revision));
    }
    let reply = search(&file, "@b @absent", None);
    assert_eq!(reply.list.unwrap_or_else(|| panic!("pasted list")).missing, ["absent"]);
    let reply = search(&file, "Second", None);
    assert!(reply.list.is_none());
}

#[test]
fn closest_matches_are_left_out_until_the_picker_can_mark_them() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let file = library(directory.path(), "@misc{a,title={Sleep}}");
    assert_eq!(search(&file, "sleep", None).results.len(), 1);
    assert!(search(&file, "sleep photon", None).results.is_empty());
}

#[test]
fn a_stopped_search_answers_with_no_results_for_the_same_snapshot() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let file = library(directory.path(), "@misc{a,title={Sleep}}");
    let cancel = Cancellation::default();
    cancel.cancel();
    let reply = file
        .search(&query("sleep", false), 20, None, &cancel)
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(reply.results.is_empty());
    assert_eq!(reply.revision, file.revision());
}

#[test]
fn an_older_search_stops_even_if_it_starts_after_a_newer_one() {
    let requests = SearchRequests::default();
    let first = requests.start(1, 1).unwrap_or_else(|| panic!("first search"));
    let second = requests.start(1, 2).unwrap_or_else(|| panic!("newer search"));
    assert!(first.is_cancelled(), "a newer search stops the older one");
    assert!(!second.is_cancelled());
    assert!(
        requests.start(1, 1).is_none(),
        "an older search that starts late never runs"
    );
    let again = requests.start(1, 2).unwrap_or_else(|| panic!("same search"));
    assert!(!again.is_cancelled() && !second.is_cancelled());
    let third = requests.start(1, 3).unwrap_or_else(|| panic!("newest search"));
    assert!(second.is_cancelled() && again.is_cancelled() && !third.is_cancelled());
}

#[test]
fn a_reloaded_page_counts_its_searches_afresh() {
    let requests = SearchRequests::default();
    let old_page = requests.start(1, 500).unwrap_or_else(|| panic!("old page"));
    let new_page = requests.start(2, 1).unwrap_or_else(|| panic!("reloaded page"));
    assert!(!old_page.is_cancelled() && !new_page.is_cancelled());
    assert!(requests.start(1, 499).is_none());
}

#[test]
fn results_carry_what_tells_similar_references_apart() {
    let long = "Long ".repeat(60);
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let file = library(
        directory.path(),
        &format!(
            "@article{{a, title={{Sleep}}, author={{Garc{{\\'i}}a, Mar{{\\'i}}a and Okafor, Tobi}}, journal={{Journal of Synthetic Studies}}, year={{2021}}}}\n\
             @book{{b, title={{Sleep}}, author={{{{Example Group}}}}, publisher={{{long}}}, year={{2021}}}}\n\
             @misc{{c, title={{Sleep}}, year={{2021}}}}"
        ),
    );
    let reply = search(&file, "sleep", None);
    let row = |key: &str| {
        reply
            .results
            .iter()
            .find(|row| row.key == key)
            .unwrap_or_else(|| panic!("row {key}"))
    };
    assert_eq!(row("a").first_author, "García");
    assert_eq!(row("a").author_count, 2);
    assert_eq!(row("a").container.as_deref(), Some("Journal of Synthetic Studies"));
    assert_eq!(row("a").item_type, "article-journal");
    assert_eq!(row("b").first_author, "Example Group");
    assert_eq!(row("b").author_count, 1);
    assert_eq!(row("b").item_type, "book");
    let capped = row("b").container.as_deref().unwrap_or_default();
    assert_eq!(capped.chars().count(), CONTAINER_LIMIT);
    assert!(capped.ends_with('…') && !capped.ends_with(" …"));
    assert_eq!(
        (
            row("c").first_author.as_str(),
            row("c").author_count,
            row("c").container.as_deref()
        ),
        ("", 0, None)
    );
}

#[test]
fn the_author_count_comes_from_the_creator_list() {
    // A literal organization name with a comma is one creator, so the
    // count is not read from the comma-joined display string.
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let file = library(
        directory.path(),
        "@article{a, title={Sleep}, author={Smith, Ann and {Institute, Research}}, year={2021}}\n\
         @article{b, title={Sleep}, author={Smith, Ann and Okafor, Tobi and Lima, Ana}, year={2021}}",
    );
    let reply = search(&file, "sleep", None);
    let count = |key: &str| {
        reply
            .results
            .iter()
            .find(|row| row.key == key)
            .map(|row| row.author_count)
    };
    assert_eq!(count("a"), Some(2));
    assert_eq!(count("b"), Some(3));
}

#[test]
fn scoped_results_keep_the_desktop_result_cap() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let bib = (0..220)
        .map(|n| format!("@misc{{key{n},title={{研究}}}}\n"))
        .collect::<String>();
    let file = library(directory.path(), &bib);
    let all = search(&file, "研究", None);
    assert_eq!(all.results.len(), 200);
    let source = all.results[0].source.clone();
    let scoped = search(&file, "研究", Some(vec![source.clone(), source.clone()]));
    assert_eq!(scoped.results.len(), 200);
    assert!(scoped.results.iter().all(|row| row.source == source));
    assert!(search(&file, "研究", Some(Vec::new())).results.is_empty());
}

#[test]
fn matched_fields_reach_the_interface_under_their_serde_names() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let file = library(directory.path(), "@article{a,title={Sleep},doi={10.1000/sleep}}");
    let reply = search(&file, "10.1000/sleep", None);
    let row = serde_json::to_value(&reply.results[0]).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(row["matched"], serde_json::json!(["doi"]));
    // The names on the wire are the ones the interface already knows.
    for field in [MatchField::Citekey, MatchField::Identifier] {
        let name = serde_json::to_value(field).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(name, serde_json::json!(format!("{field:?}").to_lowercase()));
    }
}
