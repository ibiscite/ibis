//! The picker delegates acquisition, search, rendering and copy validation to the engine.
use super::CommandError;
use super::search::{SearchQuery, SearchReply, SearchResult};
use ibis_core::contract::{ItemRef, LibraryId};
use ibis_engine::library::{Current, native_source};
use ibis_engine::{Cancellation, Engine, Method, request::SearchRequest};
use serde_json::json;
use std::collections::HashMap;

/// One answer from an engine's `libraries` method, by its field.
fn engine_list<T: serde::de::DeserializeOwned>(
    engine: &Engine,
    action: &str,
    field: &str,
) -> Result<T, ibis_core::contract::EngineError> {
    let answer = engine.dispatch(Method::Libraries, &json!({"action":action}), &Cancellation::default())?;
    answer
        .data
        .get(field)
        .cloned()
        .and_then(|list| serde_json::from_value(list).ok())
        .ok_or(ibis_core::contract::ErrorCode::Internal.into())
}

pub(super) fn zotero_groups(
    engine: &Engine,
) -> Result<Vec<ibis_engine::ZoteroGroup>, ibis_core::contract::EngineError> {
    engine_list(engine, "zotero-groups", "zotero_groups")
}

/// The Zotero profiles on this computer, by name.
pub(super) fn zotero_profiles() -> Result<Vec<String>, ibis_core::contract::EngineError> {
    let names: Vec<String> = engine_list(&Engine::new()?, "zotero-profiles", "zotero_profiles")?;
    if names.is_empty() {
        return Err(ibis_core::contract::ErrorCode::SourceUnavailable.into());
    }
    Ok(names)
}

pub(crate) trait FileLibrary {
    fn recents(&self, ids: &[ItemRef], include_added: bool) -> Result<SearchReply, CommandError>;
    fn session_rows(&self, ids: &[ItemRef]) -> Result<Vec<SearchResult>, CommandError>;
    fn search(
        &self,
        query: &SearchQuery,
        limit: usize,
        scope: Option<Vec<String>>,
        cancel: &Cancellation,
    ) -> Result<SearchReply, CommandError>;
    fn mark_project(&self, reply: SearchReply, project: Option<&crate::project::Current>) -> SearchReply;
    fn original_entry(&self, id: &ItemRef) -> Result<ibis_engine::EntryResult, CommandError>;
    fn zotero_groups(&self) -> Result<Vec<ibis_engine::ZoteroGroup>, ibis_core::contract::EngineError>;
}

impl FileLibrary for Current {
    fn recents(&self, ids: &[ItemRef], include_added: bool) -> Result<SearchReply, CommandError> {
        use super::search::RecentGroup;
        let snapshot = self.engine.snapshot().map_err(CommandError::from)?;
        let revision = snapshot.fingerprints().full.as_str().to_owned();
        let sources = self.engine.libraries().map_err(CommandError::from)?.libraries;
        let picked = ibis_engine::recents::resolve(&snapshot, ids, include_added);
        let results = picked
            .into_iter()
            .filter_map(|(object, cited)| {
                let mut row = SearchResult::from_engine(object, vec![], false, false, &revision, self.sample())?;
                if let Some(source) = sources.iter().find(|info| info.id == object.object.id().library) {
                    row.source = native_source(source);
                }
                row.recent = Some(if cited { RecentGroup::Cited } else { RecentGroup::Added });
                Some(row)
            })
            .collect();
        Ok(SearchReply {
            revision,
            results,
            ..SearchReply::default()
        })
    }
    /// Rows for listed records in the open library, in the list's order. Each
    /// comes from the current snapshot; the list itself holds identities only.
    fn session_rows(&self, ids: &[ItemRef]) -> Result<Vec<SearchResult>, CommandError> {
        let snapshot = self.engine.snapshot().map_err(CommandError::from)?;
        let revision = snapshot.fingerprints().full.as_str();
        Ok(ids
            .iter()
            .filter_map(|id| {
                SearchResult::from_engine(snapshot.get(id)?, vec![], false, false, revision, self.sample())
            })
            .collect())
    }

    fn search(
        &self,
        query: &SearchQuery,
        limit: usize,
        scope: Option<Vec<String>>,
        cancel: &Cancellation,
    ) -> Result<SearchReply, CommandError> {
        if query.text.trim().is_empty() {
            return Ok(SearchReply {
                revision: self.revision(),
                ..SearchReply::default()
            });
        }
        // The existing native boundary names Zotero sources for the picker;
        // ItemRef remains the engine's opaque identity for preview and copy.
        let sources: HashMap<_, _> = self
            .engine
            .libraries()
            .map_err(CommandError::from)?
            .libraries
            .iter()
            .map(|info| (info.id.as_str().to_owned(), native_source(info)))
            .collect();
        let scope = scope.map(|ids| {
            ids.into_iter()
                .map(|id| {
                    sources
                        .iter()
                        .find(|(_, name)| **name == id)
                        .map(|(opaque, _)| opaque.clone())
                        .unwrap_or(id)
                })
                .collect::<Vec<_>>()
        });
        let libraries: Option<Vec<LibraryId>> = scope
            .map(|ids| ids.iter().map(|id| LibraryId::new(id)).collect::<Result<_, _>>())
            .transpose()
            .map_err(|_| {
                CommandError::from(ibis_core::contract::EngineError::new(
                    ibis_core::contract::ErrorCode::BadInput,
                ))
            })?;
        let outcome = self.engine.search(
            &SearchRequest {
                interpret_locator: query.interpret_locators,
                query: query.text.clone(),
                limit: limit.min(200),
                libraries,
            },
            cancel,
        );
        let reply = match outcome {
            Ok(reply) => reply,
            // A newer search replaced this one; the interface ignores the empty reply.
            Err(error) if error.code == ibis_core::contract::ErrorCode::Cancelled => {
                return Ok(SearchReply {
                    revision: self.revision(),
                    ..SearchReply::default()
                });
            }
            Err(error) => return Err(error.into()),
        };
        let revision = reply.snapshot.full.as_str().to_owned();
        let results: Vec<_> = reply
            .hits
            .into_iter()
            .filter_map(|hit| {
                let mut result =
                    SearchResult::from_engine(&hit.item, hit.matched, hit.partial, hit.typo, &revision, self.sample())?;
                if let Some(source) = sources.get(hit.item.object.id().library.as_str()) {
                    result.source.clone_from(source);
                }
                Some(result)
            })
            .collect();
        let unfiltered = results
            .is_empty()
            .then(|| ibis_engine::without_filters(&query.text))
            .flatten();
        Ok(SearchReply {
            revision,
            results,
            interpretation: reply.shorthand.map(Into::into),
            list: reply.list,
            unfiltered,
        })
    }
    /// Mark each row by the key a citation of it would emit. A row whose key
    /// cannot be emitted has no mark; the citation refuses it as before.
    fn mark_project(&self, mut reply: SearchReply, project: Option<&crate::project::Current>) -> SearchReply {
        let Some(project) = project else { return reply };
        for row in &mut reply.results {
            let check = self.engine.project_check_item(&project.read, &row.id).ok().flatten();
            row.project = project.mark(check.map(|(_, check)| check));
        }
        reply
    }

    fn original_entry(&self, id: &ItemRef) -> Result<ibis_engine::EntryResult, CommandError> {
        self.engine
            .original_entry(id, &Cancellation::default())
            .map_err(CommandError::from)
    }
    fn zotero_groups(&self) -> Result<Vec<ibis_engine::ZoteroGroup>, ibis_core::contract::EngineError> {
        zotero_groups(&self.engine)
    }
}
#[cfg(test)]
mod library_tests;
#[cfg(test)]
mod zotero_acceptance;

// Source changes share the desktop's lazy formatter; its policy stays in the engine.
pub(crate) static FORMATTING: std::sync::LazyLock<ibis_engine::Formatting> =
    std::sync::LazyLock::new(ibis_engine::Formatting::default);

pub(crate) fn warm_formatter(output: Option<String>) {
    let formatting = FORMATTING.clone();
    let _ = std::thread::Builder::new()
        .name("ibis-format-startup".into())
        .spawn(move || {
            let output = crate::output::stored(output.as_deref());
            let _ = formatting.warm(crate::output::default_style(output), None);
        });
}
