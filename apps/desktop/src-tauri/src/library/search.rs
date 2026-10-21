//! Searching an index snapshot, and the rows sent to the interface for it.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use ibis_core::model::{RecordId, Reference};
use ibis_core::search::{MatchField, OwnedIndex};
use serde::Serialize;

/// The newest search each interface page has sent, so a search for a query
/// the person has already typed past stops early instead of delaying the
/// newer one. The interface numbers its searches, because commands can start
/// in a different order than they were sent, and names its page, because a
/// reloaded page counts from the start again.
#[derive(Default)]
pub(crate) struct SearchRequests(Mutex<HashMap<u32, Arc<AtomicU32>>>);

impl SearchRequests {
    pub(super) fn newest(&self, page: u32) -> Arc<AtomicU32> {
        let mut pages = self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        Arc::clone(pages.entry(page).or_default())
    }
}

#[derive(Serialize, specta::Type)]
pub(crate) struct SearchResult {
    /// Revision captured with the index used for this result.
    pub(super) revision: String,
    /// Identity of the record; selection and copying use this, not the key.
    pub(super) id: RecordId,
    pub(super) key: String,
    title: String,
    authors: String,
    year: Option<i32>,
    matched: Vec<MatchField>,
    /// Family name of the first author (or the whole name of an
    /// organization); empty when the record names no authors.
    first_author: String,
    /// How many creators the record names (an organization counts as one),
    /// so a row can name two authors and shorten three or more.
    author_count: u32,
    /// Journal, book or proceedings title, else the publisher; capped at
    /// 120 characters.
    container: Option<String>,
    /// CSL item type, such as `article-journal`.
    item_type: String,
}

/// Longest container sent to the interface, in characters, ellipsis included.
const CONTAINER_LIMIT: usize = 120;

/// Shorten `text` to at most `limit` characters, ending with an ellipsis.
pub(super) fn capped(text: String, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text;
    }
    let kept: String = text.chars().take(limit.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

pub(super) fn surname(name: &ibis_core::model::Name) -> String {
    if name.literal.is_some() {
        name.display()
    } else {
        name.family.clone().unwrap_or_default()
    }
}

impl SearchResult {
    fn from(reference: &Reference, matched: Vec<MatchField>, revision: &str) -> Self {
        SearchResult {
            revision: revision.into(),
            first_author: reference.authors.first().map(surname).unwrap_or_default(),
            author_count: u32::try_from(reference.authors.len()).unwrap_or(u32::MAX),
            container: reference.container_plain().map(|text| capped(text, CONTAINER_LIMIT)),
            item_type: reference.item_type.clone(),
            id: reference.id.clone(),
            key: reference
                .citekey
                .clone()
                .unwrap_or_else(|| reference.id.native_key.clone()),
            title: reference.title_plain().unwrap_or_default(),
            authors: reference.authors.iter().map(surname).collect::<Vec<_>>().join(", "),
            year: reference.year(),
            matched,
        }
    }
}

/// A search keeps its snapshot identity even when no references match.
#[derive(Serialize, specta::Type)]
pub(crate) struct SearchReply {
    pub(super) revision: String,
    pub(super) results: Vec<SearchResult>,
}

pub(super) fn search_snapshot(
    index: &OwnedIndex,
    revision: String,
    query: &str,
    limit: usize,
    request: u32,
    newest: &AtomicU32,
    scope: Option<&[String]>,
) -> SearchReply {
    let results = search_newest(index, &revision, query, limit, request, newest, scope);
    SearchReply { revision, results }
}

/// Run a search unless a newer request has started, stopping if one does.
pub(super) fn search_newest(
    index: &OwnedIndex,
    revision: &str,
    query: &str,
    limit: usize,
    request: u32,
    newest: &AtomicU32,
    scope: Option<&[String]>,
) -> Vec<SearchResult> {
    newest.fetch_max(request, Ordering::SeqCst);
    let outdated = || newest.load(Ordering::SeqCst) > request;
    let results = index
        .search_scoped_until(query, limit.min(200), scope, &outdated)
        .unwrap_or_default()
        .into_iter()
        .map(|hit| SearchResult::from(hit.reference, hit.matched, revision))
        .collect();
    if outdated() { Vec::new() } else { results }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use ibis_core::settings::{ZoteroLibraries, ZoteroLibrary};

    use ibis_core::source::SourceError;

    use crate::library::error::ZOTERO_OFFLINE;
    use crate::library::source::Source;
    use crate::library::state::{Library, candidate};
    use crate::library::status::status;

    #[test]
    fn results_carry_what_tells_similar_references_apart() {
        let long = "Long ".repeat(60);
        let bib = format!(
            "@article{{a, title={{Sleep}}, author={{Garc{{\\'i}}a, Mar{{\\'i}}a and Okafor, Tobi}}, journal={{Journal of Synthetic Studies}}, year={{2021}}}}\n\
             @book{{b, title={{Sleep}}, author={{{{Example Group}}}}, publisher={{{long}}}, year={{2021}}}}\n\
             @misc{{c, title={{Sleep}}, year={{2021}}}}"
        );
        let loaded = ibis_core::bibtex::load_str("t", &bib);
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let results: Vec<SearchResult> = loaded
            .references
            .iter()
            .map(|r| SearchResult::from(r, Vec::new(), "synthetic"))
            .collect();
        assert_eq!(results[0].first_author, "García");
        assert_eq!(results[0].author_count, 2);
        assert_eq!(results[0].container.as_deref(), Some("Journal of Synthetic Studies"));
        assert_eq!(results[0].item_type, "article-journal");
        assert_eq!(results[1].first_author, "Example Group");
        assert_eq!(results[1].author_count, 1);
        assert_eq!(results[1].item_type, "book");
        let capped = results[1].container.as_deref().unwrap_or_default();
        assert_eq!(capped.chars().count(), CONTAINER_LIMIT);
        assert!(capped.ends_with('…') && !capped.ends_with(" …"));
        assert_eq!(
            (
                results[2].first_author.as_str(),
                results[2].author_count,
                results[2].container.as_deref()
            ),
            ("", 0, None)
        );
    }

    #[test]
    fn the_author_count_comes_from_the_creator_list() {
        // A literal organization name with a comma is one creator, so the
        // count is not read from the comma-joined display string.
        let bib = "@article{a, title={Sleep}, author={Smith, Ann and {Institute, Research}}, year={2021}}\n\
                   @article{b, title={Sleep}, author={Smith, Ann and Okafor, Tobi and Lima, Ana}, year={2021}}";
        let loaded = ibis_core::bibtex::load_str("t", bib);
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let results: Vec<SearchResult> = loaded
            .references
            .iter()
            .map(|r| SearchResult::from(r, Vec::new(), "synthetic"))
            .collect();
        assert_eq!(
            (results[0].author_count, results[0].authors.as_str()),
            (2, "Smith, Institute, Research")
        );
        assert_eq!(results[1].author_count, 3);
    }

    #[test]
    fn scopes_limit_the_snapshot_without_reopening_and_keep_page_requests_separate() {
        let chosen = ZoteroLibraries::new(vec![
            ZoteroLibrary::User,
            ZoteroLibrary::Group(7),
            ZoteroLibrary::Group(8),
        ])
        .unwrap_or_else(|| panic!("valid libraries"));
        let mut loaded = ibis_core::bibtex::load_str("zotero:user", "@misc{same,title={研究},doi={10.1000/scope}}");
        loaded.references.extend(
            ibis_core::bibtex::load_str("zotero:group:7", "@misc{same,title={研究},doi={10.1000/scope}}").references,
        );
        let mut library = candidate(Source::Zotero(chosen), loaded);
        let index = Arc::clone(&library.index);
        let requests = SearchRequests::default();
        let scope = vec!["zotero:group:7".into()];
        let results = search_newest(&index, "synthetic", "研究", 1, 2, &requests.newest(1), Some(&scope));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id.source, "zotero:group:7");
        assert!(search_newest(&index, "synthetic", "10.1000/scope", 20, 1, &requests.newest(1), None).is_empty());
        assert_eq!(
            search_newest(&index, "synthetic", "10.1000/scope", 20, 1, &requests.newest(2), None).len(),
            2
        );
        assert!(search_newest(&index, "synthetic", "研究", 20, 3, &requests.newest(1), Some(&[])).is_empty());
        assert!(Arc::ptr_eq(&index, &library.index));
        let counts = status(&library).source_counts;
        assert_eq!(counts.get("zotero:user"), Some(&1));
        assert_eq!(counts.get("zotero:group:7"), Some(&1));
        assert_eq!(counts.get("zotero:group:8"), Some(&0));
        assert_eq!(counts.values().sum::<usize>(), status(&library).references);
        // Counts describe the retained searchable snapshot, not a failed new selection.
        library.requested = Some(Source::Zotero(ZoteroLibrary::Group(99).into()));
        library.error = Some(SourceError::Unavailable.into());
        assert_eq!(status(&library).source_counts, counts);
        assert!(status(&Library::default()).source_counts.is_empty());
        let file = candidate(
            PathBuf::from("refs.bib"),
            ibis_core::bibtex::load_str("synthetic-file", "@misc{a}"),
        );
        assert_eq!(status(&file).source_counts.get("synthetic-file"), Some(&1));
    }

    #[test]
    fn scoped_snapshot_replies_keep_revision_and_retained_source_counts_together() {
        let chosen = ZoteroLibraries::new(vec![
            ZoteroLibrary::User,
            ZoteroLibrary::Group(7),
            ZoteroLibrary::Group(8),
        ])
        .unwrap_or_else(|| panic!("valid synthetic libraries"));
        let mut loaded = ibis_core::bibtex::load_str("zotero:user", "@misc{same,title={研究}}");
        loaded
            .references
            .extend(ibis_core::bibtex::load_str("zotero:group:7", "@misc{same,title={研究}}").references);
        let mut library = candidate(Source::Zotero(chosen), loaded);
        library.revision = u64::MAX - 1;
        let captured = status(&library);
        let index = Arc::clone(&library.index);
        let scope = vec!["zotero:group:7".into()];
        let newest = AtomicU32::default();
        let scoped = search_snapshot(&index, captured.revision.clone(), "研究", 20, 1, &newest, Some(&scope));
        assert_eq!(scoped.revision, captured.revision);
        assert_eq!(scoped.results.len(), 1);
        assert_eq!(scoped.results[0].id.source, "zotero:group:7");
        assert_eq!(scoped.results[0].revision, captured.revision);
        let all = search_snapshot(&index, captured.revision.clone(), "研究", 20, 2, &newest, None);
        assert_eq!(all.results.len(), 2);
        for (request, scope) in [
            (3, Vec::new()),
            (4, vec!["zotero:group:8".into()]),
            (5, vec!["unknown".into()]),
        ] {
            let empty = search_snapshot(
                &index,
                captured.revision.clone(),
                "研究",
                20,
                request,
                &newest,
                Some(&scope),
            );
            assert!(empty.results.is_empty());
            assert_eq!(empty.revision, captured.revision);
        }
        let late = search_snapshot(&index, captured.revision.clone(), "研究", 20, 1, &newest, None);
        assert!(late.results.is_empty());
        assert_eq!(late.revision, captured.revision);
        library.requested = Some(Source::Zotero(ZoteroLibrary::Group(99).into()));
        library.error = Some(ZOTERO_OFFLINE);
        let retained = status(&library);
        assert_eq!(retained.revision, captured.revision);
        assert_eq!(retained.source_counts, captured.source_counts);
        assert_eq!(retained.source_counts.get("zotero:group:8"), Some(&0));
        assert_eq!(retained.source_counts.values().sum::<usize>(), retained.references);
        assert!(Arc::ptr_eq(&index, &library.index));
    }

    #[test]
    fn scoped_results_keep_the_desktop_result_cap() {
        let bib = (0..220)
            .map(|n| format!("@misc{{key{n},title={{研究}}}}\n"))
            .collect::<String>();
        let index = OwnedIndex::new(ibis_core::bibtex::load_str("file", &bib).references);
        let scope = vec!["file".into(), "file".into()];
        let results = search_newest(
            &index,
            "synthetic",
            "研究",
            usize::MAX,
            1,
            &AtomicU32::new(0),
            Some(&scope),
        );
        assert_eq!(results.len(), 200);
        assert!(results.iter().all(|r| r.id.source == "file"));
        assert_eq!(index.source_counts().get("file"), Some(&220));
    }

    #[test]
    fn matched_fields_reach_the_interface_under_their_serde_names() {
        let refs = ibis_core::bibtex::load_str("t", "@article{a,title={Sleep},doi={10.1000/sleep}}").references;
        let index = OwnedIndex::new(refs);
        let results = search_newest(&index, "synthetic", "10.1000/sleep", 20, 1, &AtomicU32::new(0), None);
        let row = serde_json::to_value(&results[0]).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(row["matched"], serde_json::json!(["doi"]));
        // The names on the wire are the ones the interface already knows.
        for field in [MatchField::Citekey, MatchField::Identifier] {
            let name = serde_json::to_value(field).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(name, serde_json::json!(format!("{field:?}").to_lowercase()));
        }
    }

    #[test]
    fn an_older_search_stops_even_if_it_starts_after_a_newer_one() {
        let refs = ibis_core::bibtex::load_str("t", "@article{a,title={Sleep},year={2020}}").references;
        let index = OwnedIndex::new(refs);
        let newest = AtomicU32::new(0);
        assert_eq!(
            search_newest(&index, "synthetic", "sleep", 20, 2, &newest, None).len(),
            1
        );
        assert!(search_newest(&index, "synthetic", "sleep", 20, 1, &newest, None).is_empty());
        assert_eq!(
            search_newest(&index, "synthetic", "sleep", 20, 2, &newest, None).len(),
            1
        );
        assert_eq!(
            search_newest(&index, "synthetic", "sleep", 20, 3, &newest, None).len(),
            1
        );
    }

    #[test]
    fn a_reloaded_page_counts_its_searches_afresh() {
        let refs = ibis_core::bibtex::load_str("t", "@article{a,title={Sleep},year={2020}}").references;
        let index = OwnedIndex::new(refs);
        let requests = SearchRequests::default();
        assert_eq!(
            search_newest(&index, "synthetic", "sleep", 20, 500, &requests.newest(1), None).len(),
            1
        );
        assert_eq!(
            search_newest(&index, "synthetic", "sleep", 20, 1, &requests.newest(2), None).len(),
            1
        );
        assert!(search_newest(&index, "synthetic", "sleep", 20, 499, &requests.newest(1), None).is_empty());
    }
}
