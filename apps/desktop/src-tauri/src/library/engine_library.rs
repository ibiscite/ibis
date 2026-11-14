//! The picker delegates acquisition, search, rendering and copy validation to the engine.
use super::CommandError;
use super::search::{SearchQuery, SearchReply, SearchResult};
use ibis_core::contract::{ItemRef, LibraryId};
use ibis_engine::settings::{ZoteroLibraries, ZoteroLibrary};
use ibis_engine::{
    Cancellation, Engine, Method,
    request::{SearchRequest, ZoteroLibrary as EngineZoteroLibrary},
};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) struct FileLibrary {
    pub(crate) engine: Engine,
    /// Completed initial read identity; Reload carries its own immutable observation.
    source_path: Option<PathBuf>,
    pub(crate) sample: bool,
    observing: AtomicBool,
}
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

impl FileLibrary {
    pub(crate) fn recents(&self, ids: &[ItemRef], include_added: bool) -> Result<SearchReply, CommandError> {
        use super::search::RecentGroup;
        let snapshot = self.engine.snapshot().map_err(CommandError::from)?;
        let revision = snapshot.fingerprints().full.as_str().to_owned();
        let sources = self.engine.libraries().map_err(CommandError::from)?.libraries;
        let picked = ibis_engine::recents::resolve(&snapshot, ids, include_added);
        let results = picked
            .into_iter()
            .filter_map(|(object, cited)| {
                let mut row = SearchResult::from_engine(object, vec![], false, false, &revision, self.sample)?;
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
    pub(crate) fn session_rows(&self, ids: &[ItemRef]) -> Result<Vec<SearchResult>, CommandError> {
        let snapshot = self.engine.snapshot().map_err(CommandError::from)?;
        let revision = snapshot.fingerprints().full.as_str();
        Ok(ids
            .iter()
            .filter_map(|id| SearchResult::from_engine(snapshot.get(id)?, vec![], false, false, revision, self.sample))
            .collect())
    }

<<<<<<< HEAD
    pub(crate) fn open(path: &Path) -> Result<Self, super::LoadError> {
        Self::open_file(
=======
    pub(crate) fn open(path: &Path, cancel: &Cancellation) -> Result<Self, super::LoadError> {
        Self::open_file_with(
>>>>>>> origin/main
            path,
            Engine::new()
                .map(|engine| engine.with_formatting(FORMATTING.clone()))
                .map_err(|_| super::LoadError::Stopped)?,
<<<<<<< HEAD
        )
    }

    pub(super) fn open_samples(path: &Path) -> Result<Self, super::LoadError> {
        let mut file = Self::open_file(
=======
            cancel,
            || {},
        )
    }

    pub(super) fn open_samples(path: &Path, cancel: &Cancellation) -> Result<Self, super::LoadError> {
        let mut file = Self::open_file_with(
>>>>>>> origin/main
            path,
            Engine::new()
                .map(|engine| engine.with_formatting(FORMATTING.clone()))
                .map_err(|_| super::LoadError::Stopped)?
                .without_file_watches(),
<<<<<<< HEAD
=======
            cancel,
            || {},
>>>>>>> origin/main
        )?;
        file.sample = true;
        Ok(file)
    }

<<<<<<< HEAD
    fn open_file(path: &Path, engine: Engine) -> Result<Self, super::LoadError> {
        Self::open_file_with(path, engine, || {})
    }

    fn open_file_with(path: &Path, engine: Engine, before_read: impl FnOnce()) -> Result<Self, super::LoadError> {
        before_read();
        let source_path = engine
            .open_file_observed(path, &Cancellation::default())
=======
    fn open_file_with(
        path: &Path,
        engine: Engine,
        cancel: &Cancellation,
        before_read: impl FnOnce(),
    ) -> Result<Self, super::LoadError> {
        before_read();
        let source_path = engine
            .open_file_observed(path, cancel)
>>>>>>> origin/main
            .map_err(super::LoadError::file)?;
        let mut file = Self::from_engine(engine);
        file.source_path = Some(source_path);
        Ok(file)
    }

    pub(super) fn source_path(&self) -> Option<&Path> {
        self.source_path.as_deref()
    }

    pub(super) fn from_engine(engine: Engine) -> Self {
        Self {
            engine,
            source_path: None,
            sample: false,
            observing: AtomicBool::new(false),
        }
    }
    pub(super) fn open_zotero(
        chosen: &ZoteroLibraries,
        profile: Option<&str>,
        cancel: &Cancellation,
    ) -> Result<Self, super::LoadError> {
        let engine = Engine::new()
            .map(|engine| engine.with_formatting(FORMATTING.clone()))
            .map_err(super::LoadError::Zotero)?;
        Self::open_zotero_with(engine, chosen, profile, cancel)
    }

    fn open_zotero_with(
        engine: Engine,
        chosen: &ZoteroLibraries,
        profile: Option<&str>,
        cancel: &Cancellation,
    ) -> Result<Self, super::LoadError> {
        let bridge = Self::from_engine(engine);
        bridge.refresh_zotero(chosen, profile, false, cancel)?;
        Ok(bridge)
    }

    /// Whether this library's references came from Zotero's closed database.
    pub(super) fn read_zotero_closed(&self) -> bool {
        self.engine.zotero_read_closed()
    }

    /// The group libraries this engine knows, Zotero running or not.
    pub(super) fn zotero_groups(&self) -> Result<Vec<ibis_engine::ZoteroGroup>, ibis_core::contract::EngineError> {
        zotero_groups(&self.engine)
    }

    pub(super) fn refresh_zotero(
        &self,
        chosen: &ZoteroLibraries,
        profile: Option<&str>,
        force: bool,
        cancel: &Cancellation,
    ) -> Result<bool, super::LoadError> {
        let libraries: Vec<_> = chosen
            .libraries()
            .iter()
            .map(|library| match library {
                ZoteroLibrary::User => EngineZoteroLibrary::Personal,
                ZoteroLibrary::Group(id) => EngineZoteroLibrary::Group(*id),
            })
            .collect();
        let loaded: Vec<_> = self
            .engine
            .libraries()
            .map_err(super::LoadError::Zotero)?
            .libraries
            .iter()
            .filter_map(|library| library.zotero)
            .collect();
        // The listing is sorted by opaque ID, not by the user's selection order.
        let same = loaded.len() == libraries.len() && loaded.iter().all(|library| libraries.contains(library));
        let request = json!({"action":"open-zotero","libraries":libraries,"profile":profile,"reload":force && same});
        self.engine
            .dispatch(Method::Libraries, &request, cancel)
            .map_err(super::LoadError::Zotero)?;
        let info = self.engine.libraries().map_err(super::LoadError::Zotero)?;
        let failure = info.libraries.iter().find_map(|info| {
            info.refresh_error.map(|code| ibis_core::contract::EngineError {
                code,
                reason: info.refresh_reason,
                detail: None,
                strict_cause: None,
                citation_error: None,
            })
        });
        if let Some(failure) = failure {
            return Err(super::LoadError::Zotero(failure));
        }
        // Empty libraries change the choice even when the object fingerprint stays equal.
        Ok(same)
    }

    pub(super) fn prepare_zotero_reload(
        &self,
        chosen: &ZoteroLibraries,
        profile: Option<&str>,
        force: bool,
        cancel: &Cancellation,
        guard: impl FnMut(
            &mut dyn FnMut() -> Result<(), ibis_core::contract::EngineError>,
        ) -> Result<(), ibis_core::contract::EngineError>,
    ) -> Result<ibis_engine::PreparedZoteroReload, super::LoadError> {
        let libraries: Vec<_> = chosen
            .libraries()
            .iter()
            .map(|library| match library {
                ZoteroLibrary::User => EngineZoteroLibrary::Personal,
                ZoteroLibrary::Group(id) => EngineZoteroLibrary::Group(*id),
            })
            .collect();
        self.engine
            .prepare_zotero_reload_guarded(force, Some(&libraries), profile, cancel, guard)
            .map_err(super::LoadError::Zotero)
    }

    pub(super) fn observe_once(
        &self,
    ) -> Result<Option<ibis_engine::LibrarySubscription>, ibis_core::contract::EngineError> {
        if self
            .observing
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Ok(None);
        }
        match self.engine.subscribe() {
            Ok(events) => Ok(Some(events)),
            Err(error) => {
                self.observing.store(false, Ordering::SeqCst);
                Err(error)
            }
        }
    }
    pub(super) fn prepare_reload(
        &self,
        path: &Path,
        cancel: &Cancellation,
    ) -> Result<ibis_engine::PreparedFileReload, super::LoadError> {
        self.engine
            .prepare_file_reload(path, cancel)
            .map_err(super::LoadError::file_refresh)
    }

    pub(super) fn accept_reload(&self, reload: ibis_engine::PreparedFileReload) -> Result<(), super::LoadError> {
        match self.engine.accept_file_reload(reload) {
            // The caller holds the current request, so a newer accepted read
            // (usually the watcher's) retired this one: the library is fresh.
            Err(error) if error.code == ibis_core::contract::ErrorCode::Cancelled => Ok(()),
            result => result.map_err(super::LoadError::file_refresh),
        }
    }

    #[cfg(test)]
    pub(super) fn reload(&self) -> Result<(), super::LoadError> {
        // A live watcher can accept the same change first, which retires this
        // reload. The newer publication is the result, so it is not a failure.
        match self
            .engine
            .dispatch(Method::Libraries, &json!({"action":"reload"}), &Cancellation::default())
        {
            Err(failure) if failure.code != ibis_core::contract::ErrorCode::Cancelled => {
                return Err(super::LoadError::Stopped);
            }
            _ => {}
        }
        if let Some(error) = self
            .engine
            .libraries()
            .map_err(|_| super::LoadError::Stopped)?
            .libraries
            .iter()
            .find_map(|info| {
                info.refresh_error.map(|code| ibis_core::contract::EngineError {
                    code,
                    reason: info.refresh_reason,
                    detail: None,
                    strict_cause: None,
                    citation_error: None,
                })
            })
        {
            return Err(super::LoadError::file_refresh(error));
        }
        Ok(())
    }
    pub(super) fn revision(&self) -> String {
        self.engine
            .snapshot()
            .map(|s| s.fingerprints().full.as_str().to_owned())
            .unwrap_or_default()
    }
    pub(crate) fn search(
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
                    SearchResult::from_engine(&hit.item, hit.matched, hit.partial, hit.typo, &revision, self.sample)?;
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
            list: reply
                .list
                .map(|list| super::search::PastedList { missing: list.missing }),
            unfiltered,
        })
    }
    /// Mark each row by the key a citation of it would emit. A row whose key
    /// cannot be emitted has no mark; the citation refuses it as before.
    pub(crate) fn mark_project(
        &self,
        mut reply: SearchReply,
        project: Option<&crate::project::Current>,
    ) -> SearchReply {
        let Some(project) = project else { return reply };
        for row in &mut reply.results {
            let check = self.engine.project_check_item(&project.read, &row.id).ok().flatten();
            row.project = project.mark(check.map(|(_, check)| check));
        }
        reply
    }

    pub(crate) fn original_entry(&self, id: &ItemRef) -> Result<ibis_engine::EntryResult, CommandError> {
        self.engine
            .original_entry(id, &Cancellation::default())
            .map_err(CommandError::from)
    }
}

pub(super) fn native_source(info: &ibis_engine::LibraryInfo) -> String {
    match info.zotero {
        Some(EngineZoteroLibrary::Personal) => "zotero:user".into(),
        Some(EngineZoteroLibrary::Group(id)) => format!("zotero:group:{id}"),
        None => info.id.as_str().to_owned(),
    }
}

#[cfg(test)]
mod library_tests;
#[cfg(test)]
mod zotero_acceptance;

// Source changes share the desktop's lazy formatter; its policy stays in the engine.
static FORMATTING: std::sync::LazyLock<ibis_engine::Formatting> =
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
