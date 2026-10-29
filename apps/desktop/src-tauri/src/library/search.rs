//! The rows sent to the interface for a search, and which search on a page is newest.

use std::collections::HashMap;
use std::sync::Mutex;

use ibis_core::contract::LocatorTerm;
use ibis_core::search::MatchField;
use ibis_engine::Cancellation;
use serde::{Deserialize, Serialize};

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
    year: Option<i32>,
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

impl SearchResult {
    pub(super) fn from_engine(
        record: &ibis_core::contract::Record,
        cite: &str,
        matched: Vec<MatchField>,
        partial: bool,
        revision: &str,
    ) -> Self {
        let text = |field: &str| {
            record
                .csl
                .get(field)
                .and_then(serde_json::Value::as_str)
                .map(ibis_core::model::strip_markup)
                .unwrap_or_default()
        };
        let authors: Vec<_> = record
            .csl
            .get("author")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|author| {
                author
                    .get("literal")
                    .or_else(|| author.get("family"))
                    .and_then(serde_json::Value::as_str)
            })
            .map(ibis_core::model::strip_markup)
            .collect();
        let container = ["container-title", "publisher"]
            .into_iter()
            .map(text)
            .map(|text| text.trim().to_owned())
            .find(|text| !text.is_empty())
            .map(|text| capped(text, CONTAINER_LIMIT));
        Self {
            revision: revision.into(),
            id: serde_json::json!(&record.id),
            source: record.id.library.as_str().to_owned(),
            cite: cite.into(),
            key: record.citation_key().ok().flatten().unwrap_or_default().into(),
            title: text("title"),
            first_author: authors.first().cloned().unwrap_or_default(),
            author_count: u32::try_from(authors.len()).unwrap_or(u32::MAX),
            authors: authors.join(", "),
            year: record
                .csl
                .get("issued")
                .and_then(|issued| issued.get("date-parts"))
                .and_then(|parts| parts.get(0))
                .and_then(|parts| parts.get(0))
                .and_then(serde_json::Value::as_i64)
                .and_then(|year| i32::try_from(year).ok()),
            matched,
            partial,
            container,
            item_type: text("type"),
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
    /// As typed by the user, e.g. "12", "24-26", "3, 7".
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
#[derive(Serialize, specta::Type)]
pub(crate) struct SearchReply {
    pub(super) revision: String,
    pub(super) results: Vec<SearchResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub(super) interpretation: Option<LocatorQuery>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub(super) list: Option<PastedList>,
}

#[cfg(test)]
mod tests;
