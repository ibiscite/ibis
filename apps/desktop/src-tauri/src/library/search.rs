//! The rows sent to the interface for a search, and which search on a page is newest.

use std::collections::HashMap;
use std::sync::Mutex;

use ibis_core::search::MatchField;
use ibis_engine::Cancellation;
use serde::{Deserialize, Serialize};

/// The state of one emitted key in the project bibliography, for the row.
pub(crate) use ibis_engine::app_context::project::ProjectMark;

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

pub(crate) use ibis_core::contract::CitationIntent as PreviewIntent;

#[derive(Serialize, schemars::JsonSchema)]
pub(crate) struct SearchResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) recent: Option<RecentGroup>,
    /// This row's key against the project bibliography of that generation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) project: Option<ProjectMark>,
    /// This row comes from the bundled invented library, including while a replacement search is pending.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(super) sample: bool,
    /// Revision captured with the index used for this result.
    pub(super) revision: String,
    /// Identity of the record; selection and copying use this, not the key.
    pub(super) id: ibis_core::contract::ItemRef,
    pub(super) cite: String,
    pub(super) source: String,
    pub(super) key: String,
    title: String,
    authors: String,
    /// The year, or the source's own date text: `RecordSummary::year`.
    year: Option<String>,
    matched: Vec<MatchField>,
    /// Only some query words matched; shown under "Closest matches".
    partial: bool,
    /// A word matched only with one spelling change, as the engine decided.
    /// Sent only when true.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    typo: bool,
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

#[derive(Serialize, schemars::JsonSchema)]
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
        typo: bool,
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
            id: record.id.clone(),
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
            typo,
            container: summary.container,
            item_type: summary.item_type.unwrap_or_default(),
        })
    }
}

/// Query text and the explicit choice to read a trailing locator.
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SearchQuery {
    pub(super) text: String,
    pub(super) interpret_locators: bool,
}

pub(crate) use ibis_core::contract::Locator;

/// The engine's interpretation, shown so the person can search as typed: a
/// signal read off the start (`see`) and a locator read off the end (`417`).
#[derive(Serialize, schemars::JsonSchema)]
pub(crate) struct LocatorQuery {
    /// What remains to search for.
    pub(crate) search: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) locator: Option<Locator>,
    /// The text read as the locator, exactly as typed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) typed: Option<String>,
    /// The signal exactly as typed, to become the reference's prefix.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) prefix: Option<String>,
}

impl From<ibis_core::shorthand::Shorthand> for LocatorQuery {
    fn from(read: ibis_core::shorthand::Shorthand) -> Self {
        Self {
            search: read.search,
            typed: read.locator.as_ref().map(|locator| locator.typed.clone()),
            locator: read.locator.map(|locator| locator.locator),
            prefix: read.prefix,
        }
    }
}

/// A search keeps its snapshot identity even when no references match.
#[derive(Default, Serialize, schemars::JsonSchema)]
pub(crate) struct SearchReply {
    pub(super) revision: String,
    pub(super) results: Vec<SearchResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) interpretation: Option<LocatorQuery>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) list: Option<ibis_engine::PastedList>,
    /// The words as typed name something in the library, so no signal or page
    /// was read from them; the picker says so in one notice line.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(super) kept_words: bool,
    /// Nothing matched and the search has filters: the search without them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) unfiltered: Option<String>,
}

#[cfg(test)]
mod tests;
