//! The rows sent to the interface for a search, and which search on a page is newest.

use std::collections::HashMap;
use std::sync::Mutex;

use ibis_core::contract::LocatorTerm;
use ibis_core::search::MatchField;
use ibis_engine::Cancellation;
use serde::{Deserialize, Serialize};

/// The state of one emitted key in the project bibliography, for the row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ProjectKeyState {
    In,
    Missing,
    CouldNotCheck,
    Repeated,
    MayNameAnother,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
pub(crate) struct ProjectMark {
    pub(crate) generation: u32,
    pub(crate) state: ProjectKeyState,
    /// Where to look in the file, when the state asks a person to look.
    pub(crate) lines: Vec<u32>,
}

impl ProjectMark {
    pub(crate) fn new(generation: u32, check: &ibis_engine::KeyCheck) -> Self {
        use ibis_engine::KeyCheck;
        let (state, lines) = match check {
            KeyCheck::In { .. } => (ProjectKeyState::In, vec![]),
            KeyCheck::Missing => (ProjectKeyState::Missing, vec![]),
            KeyCheck::CouldNotCheck => (ProjectKeyState::CouldNotCheck, vec![]),
            KeyCheck::Repeated { lines } => (ProjectKeyState::Repeated, lines.clone()),
            KeyCheck::MayNameAnother { line, .. } => (ProjectKeyState::MayNameAnother, vec![*line]),
        };
        Self {
            generation,
            state,
            lines: lines.into_iter().take(5).map(|line| line as u32).collect(),
        }
    }
}

/// The newest search each interface page has sent, so a search for a query
/// the person has already typed past stops early instead of delaying the
/// newer one. The interface numbers its searches, because commands can start
/// in a different order than they were sent, and names its page, because a
/// reloaded page counts from the start again.
#[derive(Default)]
pub(crate) struct SearchRequests(Mutex<HashMap<u32, (u32, Cancellation)>>);

impl SearchRequests {
    /// Start search `request` on `page`, stopping that page's older searches.
    /// `None` when a newer search on the page has already started.
    pub(super) fn start(&self, page: u32, request: u32) -> Option<Cancellation> {
        let mut pages = self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let newest = pages.entry(page).or_default();
        if request < newest.0 {
            return None;
        }
        if request > newest.0 {
            newest.1.cancel();
            *newest = (request, Cancellation::default());
        }
        Some(newest.1.clone())
    }
}

/// Preview requests have their own counter; late aborts name the exact request.
#[derive(Default)]
pub(crate) struct PreviewRequests(SearchRequests);

#[derive(Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub(crate) struct PreviewIntent {
    pub(crate) page: u32,
    pub(crate) request: u32,
    pub(crate) cancel: bool,
    #[serde(default)]
    #[specta(optional)]
    pub(crate) context: Option<(u32, u32)>,
}

impl PreviewRequests {
    pub(crate) fn start(&self, intent: PreviewIntent) -> Option<Cancellation> {
        let token = self.0.start(intent.page, intent.request)?;
        if intent.cancel {
            // Also remembers an abort received before the worker's start message.
            token.cancel();
            None
        } else {
            Some(token)
        }
    }
}

#[derive(Serialize, specta::Type)]
pub(crate) struct SearchResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub(super) recent: Option<RecentGroup>,
    /// This row's key against the project bibliography of that generation.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub(super) project: Option<ProjectMark>,
    /// This row comes from the bundled invented library, including while a replacement search is pending.
    #[specta(optional)]
    pub(super) sample: bool,
    /// Revision captured with the index used for this result.
    pub(super) revision: String,
    /// Identity of the record; selection and copying use this, not the key.
    #[specta(type = specta_typescript::Unknown)]
    pub(super) id: serde_json::Value,
    #[specta(optional)]
    pub(super) cite: String,
    #[specta(optional)]
    pub(super) source: String,
    pub(super) key: String,
    title: String,
    authors: String,
    /// The year, or the source's own date text: `RecordSummary::year`.
    year: Option<String>,
    matched: Vec<MatchField>,
    /// Only some query words matched; shown under "Closest matches".
    partial: bool,
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

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RecentGroup {
    Cited,
    Added,
}

impl SearchResult {
    pub(super) fn from_engine(
        object: &ibis_core::contract::SnapshotObject,
        matched: Vec<MatchField>,
        partial: bool,
        revision: &str,
        sample: bool,
    ) -> Option<Self> {
        let ibis_core::contract::LibraryObject::Record(record) = &object.object else {
            return None;
        };
        let summary = record.summary();
        Some(Self {
            recent: None,
            project: None,
            sample,
            revision: revision.into(),
            id: serde_json::json!(&record.id),
            source: record.id.library.as_str().to_owned(),
            cite: object.fingerprints.cite.as_str().into(),
            key: record.citation_key().ok().flatten().unwrap_or_default().into(),
            title: summary.title.clone().unwrap_or_default(),
            first_author: summary
                .authors
                .first()
                .map(|author| author.short.clone())
                .unwrap_or_default(),
            author_count: u32::try_from(summary.authors.len()).unwrap_or(u32::MAX),
            authors: summary
                .authors
                .iter()
                .map(|author| author.short.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            year: summary.year.clone(),
            matched,
            partial,
            container: summary.container,
            item_type: summary.item_type.unwrap_or_default(),
        })
    }
}

/// Query text and the explicit choice to read a trailing locator.
#[derive(Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub(crate) struct SearchQuery {
    pub(super) text: String,
    pub(super) interpret_locators: bool,
}

/// Missing explicit citation keys in a recognized pasted list.
#[derive(Serialize, specta::Type)]
pub(crate) struct PastedList {
    pub(super) missing: Vec<String>,
}

/// Picker wire names over the engine's canonical locator vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub(crate) struct Locator {
    pub(crate) label: LocatorTerm,
    /// Canonical value, including literal text preserved by the engine.
    pub(crate) value: String,
}

impl From<ibis_core::contract::Locator> for Locator {
    fn from(locator: ibis_core::contract::Locator) -> Self {
        Self {
            label: locator.term,
            value: locator.value,
        }
    }
}

impl From<Locator> for ibis_core::contract::Locator {
    fn from(locator: Locator) -> Self {
        Self {
            term: locator.label,
            value: locator.value,
        }
    }
}

/// The engine's interpretation, shown so the person can search as typed.
#[derive(Serialize, specta::Type)]
pub(crate) struct LocatorQuery {
    /// What remains to search for.
    pub(crate) search: String,
    pub(crate) locator: Locator,
    /// The text read as the locator, exactly as typed.
    pub(crate) typed: String,
}

impl From<ibis_core::locator::TrailingLocator> for LocatorQuery {
    fn from(read: ibis_core::locator::TrailingLocator) -> Self {
        Self {
            search: read.search,
            locator: read.locator.into(),
            typed: read.typed,
        }
    }
}

/// A search keeps its snapshot identity even when no references match.
#[derive(Default, Serialize, specta::Type)]
pub(crate) struct SearchReply {
    pub(super) revision: String,
    pub(super) results: Vec<SearchResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub(super) interpretation: Option<LocatorQuery>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub(super) list: Option<PastedList>,
    /// Nothing matched and the search has filters: the search without them.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub(super) unfiltered: Option<String>,
}

#[cfg(test)]
mod tests;
