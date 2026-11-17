use super::*;
use std::path::Path;

use crate::library::engine_library::FileLibrary;
use ibis_engine::library::Current;

fn library(directory: &Path, bib: &str) -> Current {
    let path = directory.join("synthetic.bib");
    std::fs::write(&path, bib).unwrap_or_else(|e| panic!("{e}"));
    ibis_engine::library::Current::open(
        ibis_engine::Engine::new()
            .unwrap_or_else(|_| panic!("engine"))
            .with_formatting(crate::library::engine_library::FORMATTING.clone()),
        ibis_engine::library::Source::File(path.clone()),
        None,
        &ibis_engine::Cancellation::default(),
    )
    .unwrap_or_else(|e| panic!("synthetic library: {e}"))
}

fn query(text: &str, interpret_locators: bool) -> SearchQuery {
    SearchQuery {
        text: text.into(),
        interpret_locators,
    }
}

fn search(file: &Current, text: &str, scope: Option<Vec<String>>) -> SearchReply {
    file.search(&query(text, false), usize::MAX, scope, &Cancellation::default())
        .unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn closest_matches_reach_the_picker_with_their_flag_and_snapshot() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let file = library(directory.path(), "@misc{a,title={Sleep}}");
    for (text, partial) in [("sleep", false), ("sleep photon", true)] {
        let reply = search(&file, text, None);
        assert_eq!(reply.results.len(), 1);
        let row = serde_json::to_value(&reply.results[0]).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(row["key"], "a");
        assert_eq!(row["partial"], partial);
        assert_eq!(row["revision"], reply.revision);
        assert!(!row["cite"].as_str().unwrap_or_default().is_empty());
    }
    assert!(search(&file, "photon", None).results.is_empty());
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
