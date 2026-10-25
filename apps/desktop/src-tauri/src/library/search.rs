//! Searching an index snapshot, and the rows sent to the interface for it.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use ibis_core::locator_query::{self, LocatorQuery};
use ibis_core::model::{RecordId, Reference};
use ibis_core::search::{MatchField, OwnedIndex};
use serde::{Deserialize, Serialize};

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

/// Query text and the explicit choice to read a trailing locator.
#[derive(Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub(crate) struct SearchQuery {
    pub(super) text: String,
    pub(super) interpret_locators: bool,
}

/// A search keeps its snapshot identity even when no references match.
#[derive(Serialize, specta::Type)]
pub(crate) struct SearchReply {
    pub(super) revision: String,
    pub(super) results: Vec<SearchResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub(super) interpretation: Option<LocatorQuery>,
}

pub(super) fn search_snapshot(
    index: &OwnedIndex,
    revision: String,
    query: &SearchQuery,
    limit: usize,
    request: u32,
    newest: &AtomicU32,
    scope: Option<&[String]>,
) -> SearchReply {
    let interpretation = query
        .interpret_locators
        .then(|| locator_query::split(&query.text))
        .flatten();
    let text = interpretation
        .as_ref()
        .map_or(query.text.as_str(), |query| &query.search);
    let results = search_newest(index, &revision, text, limit, request, newest, scope);
    SearchReply {
        revision,
        results,
        interpretation,
    }
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
    fn locator_search_can_be_literal_scoped_cancelled_and_empty() {
        let index = OwnedIndex::new(
            ibis_core::bibtex::load_str(
                "synthetic",
                "@misc{plain,title={Reference}}\n@misc{literal,title={Reference p 12}}",
            )
            .references,
        );
        let newest = AtomicU32::default();
        for interpret_locators in [true, false] {
            let query = SearchQuery {
                text: "Reference p 12".into(),
                interpret_locators,
            };
            let reply = search_snapshot(&index, "snapshot".into(), &query, 20, 1, &newest, None);
            assert_eq!(reply.results.len(), if interpret_locators { 2 } else { 1 });
            assert_eq!(reply.interpretation.is_some(), interpret_locators);
            assert!(reply.results.iter().all(|result| result.revision == reply.revision));
            let json = serde_json::to_value(&reply).unwrap_or_else(|error| panic!("{error}"));
            assert_eq!(json.get("interpretation").is_some(), interpret_locators);
            for scope in [&[][..], &["absent".into()][..]] {
                let scoped = search_snapshot(&index, "snapshot".into(), &query, 20, 1, &newest, Some(scope));
                assert!(scoped.results.is_empty());
                assert_eq!(scoped.revision, "snapshot");
            }
        }
        newest.store(2, Ordering::SeqCst);
        let query = SearchQuery {
            text: "Reference p 12".into(),
            interpret_locators: true,
        };
        let cancelled = search_snapshot(&index, "snapshot".into(), &query, 20, 1, &newest, None);
        assert!(cancelled.results.is_empty());
        assert_eq!(cancelled.revision, "snapshot");
        let empty = SearchQuery {
            text: String::new(),
            interpret_locators: true,
        };
        assert!(
            search_snapshot(&index, "snapshot".into(), &empty, 20, 3, &newest, None)
                .results
                .is_empty()
        );
    }

    #[test]
    fn interpreted_unicode_queries_with_long_words_keep_search_and_snapshot_intact() {
        for space in [
            '\u{2003}', '\u{0085}', '\u{00a0}', '\u{1680}', '\u{2028}', '\u{2029}', '\u{202f}', '\u{205f}', '\u{3000}',
        ] {
            for (prefix, token) in [("x", "a".repeat(70)), ("研究", "界".repeat(24))] {
                let search = format!("{prefix}{space}{token}");
                let source = format!("@misc{{a,title={{{search}}}}}");
                let index = OwnedIndex::new(ibis_core::bibtex::load_str("synthetic", &source).references);
                for interpret_locators in [true, false] {
                    let query = SearchQuery {
                        text: format!("{search} p 12"),
                        interpret_locators,
                    };
                    let reply = search_snapshot(&index, "snapshot".into(), &query, 20, 1, &AtomicU32::default(), None);
                    assert_eq!(reply.revision, "snapshot");
                    assert_eq!(reply.results.len(), usize::from(interpret_locators));
                    if interpret_locators {
                        let interpreted = reply.interpretation.unwrap_or_else(|| panic!("expected locator"));
                        assert_eq!(interpreted.search, search);
                        assert_eq!(interpreted.typed, "p 12");
                        assert_eq!(interpreted.locator.value, "12");
                        assert_eq!(reply.results[0].revision, reply.revision);
                    } else {
                        assert!(reply.interpretation.is_none());
                    }
                }
            }
        }
    }

    #[test]
    fn separate_searches_keep_each_selected_locator_through_preview_and_copy() {
        use ibis_core::cite::KeyFormat;
        use ibis_core::library::CitationSelection;
        let mut loaded = ibis_core::bibtex::load_str("a", "@misc{same,title={Alpha}}");
        loaded
            .references
            .extend(ibis_core::bibtex::load_str("b", "@misc{same,title={Beta}}").references);
        let library = Mutex::new(candidate(PathBuf::from("synthetic.bib"), loaded));
        let (index, revision) = {
            let locked = library.lock().unwrap_or_else(|error| panic!("{error}"));
            (Arc::clone(&locked.index), locked.revision.to_string())
        };
        let mut selected = Vec::new();
        for (request, text) in [(1, "Alpha p 12"), (2, "Beta ch. 3")] {
            let query = SearchQuery {
                text: text.into(),
                interpret_locators: true,
            };
            let reply = search_snapshot(
                &index,
                revision.clone(),
                &query,
                20,
                request,
                &AtomicU32::default(),
                None,
            );
            assert_eq!(reply.results.len(), 1);
            selected.push(CitationSelection {
                id: reply.results[0].id.clone(),
                locator: reply.interpretation.map(|query| query.locator),
            });
        }
        assert_ne!(selected[0].id.source, selected[1].id.source);
        for (format, expected) in [
            (KeyFormat::Pandoc, "[@same, p. 12; @same, chap. 3]"),
            (KeyFormat::Autocite, r"\autocites[12]{same}[chap.~3]{same}"),
            (KeyFormat::Parencite, r"\parencites[12]{same}[chap.~3]{same}"),
            (KeyFormat::Textcite, r"\textcites[12]{same}[chap.~3]{same}"),
            (KeyFormat::Cite, r"\cite[p.~12]{same}, \cite[chap.~3]{same}"),
            (KeyFormat::Typst, "@same[p. 12] @same[chap. 3]"),
            (KeyFormat::Keys, "same, same"),
        ] {
            let preview = crate::copy::prepare_citation(&library, &revision, &selected, format)
                .unwrap_or_else(|error| panic!("{error}"));
            let preview = serde_json::to_value(preview).unwrap_or_else(|error| panic!("{error}"));
            let copied = crate::copy::prepare_snapshot(&library, &revision, |locked| {
                crate::copy::render_citation(locked.index.references(), &selected, format)
            })
            .unwrap_or_else(|error| panic!("{error}"));
            assert_eq!(copied.text, expected);
            assert_eq!(preview["text"], copied.text);
            assert_eq!(preview["warnings"], serde_json::json!(copied.warnings));
            assert_eq!(
                copied.warnings.is_empty(),
                !matches!(format, KeyFormat::Cite | KeyFormat::Keys)
            );
            let segments = preview["segments"]
                .as_array()
                .unwrap_or_else(|| panic!("preview segments"));
            assert_eq!(
                segments
                    .iter()
                    .filter_map(|part| part["text"].as_str())
                    .collect::<String>(),
                expected
            );
            assert_eq!(
                segments
                    .iter()
                    .filter_map(|part| part["item"].as_u64())
                    .collect::<Vec<_>>(),
                [0, 1]
            );
            assert!(matches!(
                crate::copy::prepare_citation(&library, "retired", &selected, format),
                Err(crate::library::CommandError::SnapshotChanged)
            ));
        }
    }

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
        let scoped = search_snapshot(
            &index,
            captured.revision.clone(),
            &SearchQuery {
                text: "研究".into(),
                interpret_locators: true,
            },
            20,
            1,
            &newest,
            Some(&scope),
        );
        assert_eq!(scoped.revision, captured.revision);
        assert_eq!(scoped.results.len(), 1);
        assert_eq!(scoped.results[0].id.source, "zotero:group:7");
        assert_eq!(scoped.results[0].revision, captured.revision);
        let all = search_snapshot(
            &index,
            captured.revision.clone(),
            &SearchQuery {
                text: "研究".into(),
                interpret_locators: true,
            },
            20,
            2,
            &newest,
            None,
        );
        assert_eq!(all.results.len(), 2);
        for (request, scope) in [
            (3, Vec::new()),
            (4, vec!["zotero:group:8".into()]),
            (5, vec!["unknown".into()]),
        ] {
            let empty = search_snapshot(
                &index,
                captured.revision.clone(),
                &SearchQuery {
                    text: "研究".into(),
                    interpret_locators: true,
                },
                20,
                request,
                &newest,
                Some(&scope),
            );
            assert!(empty.results.is_empty());
            assert_eq!(empty.revision, captured.revision);
        }
        let late = search_snapshot(
            &index,
            captured.revision.clone(),
            &SearchQuery {
                text: "研究".into(),
                interpret_locators: true,
            },
            20,
            1,
            &newest,
            None,
        );
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
