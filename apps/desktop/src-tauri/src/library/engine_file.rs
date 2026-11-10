//! The picker delegates acquisition, search, rendering and copy validation to the engine.
use super::CommandError;
use super::search::{Locator, SearchQuery, SearchReply, SearchResult};
use ibis_core::{
    contract::{
        CitationError, CitationForm, CitationItem, CitationRequest, CitationResult, Fingerprint, ItemOutcome, ItemRef,
        KeySyntax, LibraryId, LibraryObject, LocatorTerm, Notice, OutputSpec, Snapshot, TextRun,
    },
    settings::{ZoteroLibraries, ZoteroLibrary},
};
use ibis_engine::{
    Cancellation, Engine, Method, PreparedCitation,
    request::{SearchRequest, ZoteroLibrary as EngineZoteroLibrary},
};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde_json::Value;
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

static TICKETS: AtomicU64 = AtomicU64::new(1);
pub(crate) struct FileLibrary {
    pub(super) engine: Engine,
    /// Completed initial read identity; Reload carries its own immutable observation.
    source_path: Option<PathBuf>,
    pub(crate) sample: bool,
    prepared: Mutex<Option<(String, Arc<PreparedCitation>, OutputSpec)>>,
    latest_preview: Mutex<(u64, Cancellation)>,
    observing: AtomicBool,
}
#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct FileSelection {
    pub(crate) id: ItemRef,
    #[schemars(with = "String")]
    pub(crate) cite: Fingerprint,
    /// A canonical locator captured from search; its value is never parsed again.
    pub(crate) locator: Option<Locator>,
    /// Raw locator field text and its chosen type, mutually exclusive with a
    /// captured locator.
    pub(crate) locator_input: Option<LocatorInput>,
    pub(crate) form: Option<ReferenceForm>,
    /// Literal text; only the engine escapes and formats it.
    pub(crate) prefix: Option<String>,
    pub(crate) suffix: Option<String>,
}
/// A formatted bibliography, shown before it is copied. The picker's own
/// citation receipt is untouched.
#[derive(Serialize, schemars::JsonSchema)]
pub(crate) struct SessionBibliography {
    pub(crate) text: String,
    pub(crate) html: Option<String>,
    pub(crate) content_hash: String,
    pub(crate) warnings: Vec<String>,
    /// References this style cannot write, by key or title. They are named so
    /// that the rest can still be used.
    pub(crate) left_out: Vec<String>,
}
/// What a person typed in a reference's locator field, with the type its pop-up
/// shows. Only the engine reads it: a typed label (`ch. 3`) names the type.
#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct LocatorInput {
    pub(crate) label: LocatorTerm,
    pub(crate) text: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ReferenceForm {
    Normal,
    Narrative,
    AuthorOnly,
    YearOnly,
    Full,
}
impl ReferenceForm {
    const ALL: [(Self, CitationForm); 5] = [
        (Self::Normal, CitationForm::Normal),
        (Self::Narrative, CitationForm::Narrative),
        (Self::AuthorOnly, CitationForm::AuthorOnly),
        (Self::YearOnly, CitationForm::YearOnly),
        (Self::Full, CitationForm::Full),
    ];
    fn citation(self) -> CitationForm {
        Self::ALL
            .iter()
            .find(|(wire, _)| *wire == self)
            .map_or(CitationForm::Normal, |(_, form)| *form)
    }
}
/// How to correct a locator field the engine could not read.
fn field_hint(error: ibis_core::locator::FieldError) -> String {
    use ibis_core::locator::FieldError;
    let (FieldError::MissingValue(term) | FieldError::InvalidValue(term)) = error;
    let missing = matches!(error, FieldError::MissingValue(_));
    if term == LocatorTerm::Page {
        return if missing {
            "Page needs a page or range. Enter one after the label, or clear Page."
        } else {
            "Page needs a valid page or range after its label. Correct it, or clear Page."
        }
        .into();
    }
    let name = serde_json::to_value(term)
        .ok()
        .and_then(|value| value.as_str().map(|name| name.replace('-', " ")))
        .unwrap_or_default();
    let mut chars = name.chars();
    let name: String = chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default();
    if missing {
        format!("{name} needs a value. Enter one after the label, or clear the locator.")
    } else {
        format!("{name} needs a valid value after its label. Correct it, or clear the locator.")
    }
}
#[derive(Serialize, schemars::JsonSchema)]
pub(crate) struct CitationSpan {
    pub(crate) item: Option<usize>,
    pub(crate) text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) runs: Option<Vec<ibis_core::contract::DisplayRun>>,
}
#[derive(Serialize, schemars::JsonSchema)]
pub(crate) struct FormOption {
    pub(crate) form: ReferenceForm,
    pub(crate) example: Option<String>,
    pub(crate) reason: Option<String>,
}
#[derive(Serialize, schemars::JsonSchema)]
pub(crate) struct ReferenceDetails {
    pub(crate) forms: Vec<FormOption>,
    pub(crate) current_form: Option<ReferenceForm>,
}
#[derive(Serialize, schemars::JsonSchema)]
pub(crate) struct FilePreview {
    pub(crate) ticket: String,
    pub(crate) content_hash: String,
    pub(crate) text: String,
    pub(crate) html: Option<String>,
    pub(crate) warnings: Vec<String>,
    pub(crate) spans: Vec<CitationSpan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) problem: Option<String>,
    /// Why this key citation is refused: a key in the project bibliography
    /// names a different work.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) project_refusal: Option<String>,
    /// What the engine says about this citation that is no warning, such as a
    /// full first note; the web view words each from the catalogue.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) notices: Vec<Notice>,
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
    pub(crate) fn recents(&self, ids: &[ItemRef], include_added: bool) -> Result<SearchReply, String> {
        use super::search::RecentGroup;
        let snapshot = self
            .engine
            .snapshot()
            .map_err(|_| "Recent references are unavailable.")?;
        let revision = snapshot.fingerprints().full.as_str().to_owned();
        let sources = self
            .engine
            .libraries()
            .map_err(|_| "Recent references are unavailable.")?
            .libraries;
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
    pub(crate) fn session_rows(&self, ids: &[ItemRef]) -> Result<Vec<SearchResult>, String> {
        let snapshot = self
            .engine
            .snapshot()
            .map_err(|_| "The references could not be read. Try again.")?;
        let revision = snapshot.fingerprints().full.as_str();
        Ok(ids
            .iter()
            .filter_map(|id| SearchResult::from_engine(snapshot.get(id)?, vec![], false, false, revision, self.sample))
            .collect())
    }

    fn session_item(&self, snapshot: &Snapshot, id: &ItemRef) -> Option<CitationItem> {
        let object = snapshot.get(id)?;
        matches!(object.object, LibraryObject::Record(_)).then(|| CitationItem {
            item: id.clone(),
            expected_cite: Some(object.fingerprints.cite.clone()),
            form: None,
            suppress_author: false,
            locator: None,
            prefix: vec![],
            suffix: vec![],
            annotation: None,
            expected_annotation_cite: None,
        })
    }

    fn bibliography_of(&self, items: Vec<CitationItem>, style: &str) -> Result<(PreparedCitation, bool), CommandError> {
        let count = items.len();
        let request = CitationRequest {
            items,
            form: CitationForm::BibliographyOnly,
            keep_order: false,
            output: OutputSpec::Csl { style: style.into() },
            input_language: None,
            output_locale: None,
        };
        let citation = self
            .engine
            .prepare_citation(&request, &Cancellation::default())
            .map_err(CommandError::from)?;
        let whole = citation.result().copy_content(count).is_some();
        Ok((citation, whole))
    }

    /// The list as a bibliography. A record the style cannot write must not
    /// cost the person all the others, so it is found and named instead.
    fn prepare_session_bibliography(
        &self,
        ids: &[ItemRef],
        style: &str,
    ) -> Result<Option<(PreparedCitation, Vec<String>)>, CommandError> {
        let snapshot = self.engine.snapshot().map_err(CommandError::from)?;
        let items: Vec<_> = ids.iter().filter_map(|id| self.session_item(&snapshot, id)).collect();
        if items.is_empty() {
            return Ok(None);
        }
        let first = self.bibliography_of(items.clone(), style);
        let (citation, left_out) = if let Ok((citation, true)) = first {
            (citation, vec![])
        } else {
            let mut kept = vec![];
            let mut left_out = vec![];
            for item in items {
                // Refused, or a failure only this record causes. The engine names it.
                match self.bibliography_of(vec![item.clone()], style) {
                    Ok((_, true)) => kept.push(item),
                    Ok((single, false)) => left_out.extend(single.item_label(0)),
                    Err(_) => left_out.extend(session_key(&snapshot, &item.item)),
                }
            }
            if kept.is_empty() {
                return Err(format!("This style can't write these references: {}.", left_out.join(", ")).into());
            }
            let (citation, whole) = self.bibliography_of(kept, style)?;
            if !whole {
                return Err("These references cannot be written as a bibliography in this style.".into());
            }
            (citation, left_out)
        };
        let citation = if self.sample {
            citation.mark_sample().map_err(CommandError::from)?
        } else {
            citation
        };
        Ok(Some((citation, left_out)))
    }

    pub(crate) fn session_bibliography(
        &self,
        ids: &[ItemRef],
        style: &str,
    ) -> Result<Option<SessionBibliography>, CommandError> {
        Ok(self
            .prepare_session_bibliography(ids, style)?
            .map(|(citation, left_out)| {
                let warnings = citation_warnings_named(&citation);
                let result = citation.into_result();
                SessionBibliography {
                    text: result.text,
                    html: result.html,
                    content_hash: result.content_hash.as_str().to_owned(),
                    warnings,
                    left_out,
                }
            }))
    }

    /// Hand `use_it` exactly what was shown, or refuse: the list, the library
    /// or the style changed since. Checked like a citation's copy.
    pub(crate) fn commit_session_bibliography<T>(
        &self,
        ids: &[ItemRef],
        style: &str,
        hash: &str,
        use_it: impl FnOnce(ibis_core::contract::ClipboardContent<'_>) -> Result<T, String>,
    ) -> Result<T, CommandError> {
        let (citation, _) = self
            .prepare_session_bibliography(ids, style)?
            .ok_or(CommandError::SnapshotChanged)?;
        if citation.result().content_hash.as_str() != hash {
            return Err(CommandError::SnapshotChanged);
        }
        let copied = self
            .engine
            .recite(&citation, &Cancellation::default())
            .map_err(|error| match error {
                ibis_engine::CitationCommitError::Engine(_) => {
                    CommandError::from("The bibliography could not be copied. Try again.")
                }
                _ => CommandError::SnapshotChanged,
            })?;
        let content = copied
            .copy_content(citation.items().len())
            .ok_or(CommandError::SnapshotChanged)?;
        use_it(content).map_err(CommandError::from)
    }

    pub(crate) fn open(path: &Path) -> Result<Self, super::LoadError> {
        Self::open_file(path, Engine::new().map_err(|_| super::LoadError::Stopped)?)
    }

    pub(super) fn open_samples(path: &Path) -> Result<Self, super::LoadError> {
        let mut file = Self::open_file(
            path,
            Engine::new()
                .map_err(|_| super::LoadError::Stopped)?
                .without_file_watches(),
        )?;
        file.sample = true;
        Ok(file)
    }

    fn open_file(path: &Path, engine: Engine) -> Result<Self, super::LoadError> {
        Self::open_file_with(path, engine, || {})
    }

    fn open_file_with(path: &Path, engine: Engine, before_read: impl FnOnce()) -> Result<Self, super::LoadError> {
        before_read();
        let source_path = engine
            .open_file_observed(path, &Cancellation::default())
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
            prepared: Mutex::new(None),
            latest_preview: Mutex::new((0, Cancellation::default())),
            observing: AtomicBool::new(false),
        }
    }
    pub(super) fn open_zotero(
        chosen: &ZoteroLibraries,
        profile: Option<&str>,
        cancel: &Cancellation,
    ) -> Result<Self, super::LoadError> {
        let engine = Engine::new().map_err(super::LoadError::Zotero)?;
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
            })
        });
        if let Some(failure) = failure {
            return Err(super::LoadError::Zotero(failure));
        }
        // Empty libraries change the choice even when the object fingerprint stays equal.
        Ok(same)
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
    ) -> Result<SearchReply, String> {
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
            .map_err(|_| "The library could not be searched.".to_string())?
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
            .map_err(|_| "Invalid library scope.".to_string())?;
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
            Err(_) => return Err("The library could not be searched.".to_string()),
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

    /// The exact records retained by this receipt supply both its emitted keys
    /// and its bibliography checks. A concurrent reload cannot change either.
    pub(crate) fn prepared_project_warnings(
        &self,
        ticket: &str,
        hash: &str,
        project: Option<&crate::project::Current>,
    ) -> Result<Vec<String>, CommandError> {
        let prepared = self.prepared.lock().map_err(|_| CommandError::SnapshotChanged)?;
        let (current, citation, _) = prepared.as_ref().ok_or(CommandError::SnapshotChanged)?;
        if current != ticket || citation.result().content_hash.as_str() != hash {
            return Err(CommandError::SnapshotChanged);
        }
        let Some(project) = project.filter(|_| matches!(citation.request_output(), OutputSpec::Key { .. })) else {
            return Ok(vec![]);
        };
        Ok(citation
            .project_checks(&project.read)
            .into_iter()
            .filter_map(|key| project.warning(&key.key, &key.check))
            .collect())
    }

    pub(crate) fn prepared_project_evidence(
        &self,
        ticket: &str,
        hash: &str,
        project: Option<&crate::project::Current>,
    ) -> Result<Option<ibis_engine::ProjectAssessment>, CommandError> {
        let prepared = self.prepared.lock().map_err(|_| CommandError::SnapshotChanged)?;
        let (current, citation, _) = prepared.as_ref().ok_or(CommandError::SnapshotChanged)?;
        if current != ticket || citation.result().content_hash.as_str() != hash {
            return Err(CommandError::SnapshotChanged);
        }
        Ok(project
            .filter(|_| citation.uses_citation_keys())
            .map(|project| citation.project_evidence(&project.read, ibis_engine::ProjectPolicy::Normal)))
    }

    pub(crate) fn original_entry(&self, id: &ItemRef) -> Result<ibis_engine::EntryResult, String> {
        self.engine
            .original_entry(id, &Cancellation::default())
            .map_err(|_| "The entry could not be read.".to_owned())
    }

    pub(crate) fn reserve_preview(&self) -> u64 {
        self.reserve_preview_with(Cancellation::default())
    }
    pub(crate) fn reserve_preview_with(&self, cancel: Cancellation) -> u64 {
        let mut latest = self
            .latest_preview
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let request = TICKETS.fetch_add(1, Ordering::Relaxed);
        // A superseded command can acquire the library after its replacement.
        // Give it an unpublished ticket without cancelling the current worker.
        if cancel.is_cancelled() {
            return request;
        }
        latest.1.cancel();
        *latest = (request, cancel);
        request
    }
    fn preview_cancellation(&self, reservation: u64) -> Result<Cancellation, CommandError> {
        let latest = self
            .latest_preview
            .lock()
            .map_err(|_| CommandError::from("The preview is unavailable."))?;
        if latest.0 != reservation {
            return Err(CommandError::Cancelled);
        }
        Ok(latest.1.clone())
    }
    #[cfg(test)]
    pub(crate) fn preview(
        &self,
        reservation: u64,
        items: Vec<FileSelection>,
        output: &str,
    ) -> Result<FilePreview, CommandError> {
        self.preview_for(reservation, items, crate::output::spec(output), false, None)
    }

    /// As [`Self::preview`], with a waiting editor's output in place of the
    /// picker's, so the preview is what that editor receives.
    pub(crate) fn preview_for(
        &self,
        reservation: u64,
        items: Vec<FileSelection>,
        output: &OutputSpec,
        keep_order: bool,
        editor: Option<&crate::editor_endpoint::EditorRequest>,
    ) -> Result<FilePreview, CommandError> {
        // A late-starting worker stops before building a request. A running
        // worker shares this token with the engine and QuickJS interrupt handler.
        let cancel = self.preview_cancellation(reservation)?;
        // Bound raw native field input before parsing or constructing rich text.
        // The engine owns its own rich-text accounting and validation.
        let bytes = items
            .iter()
            .flat_map(|item| {
                [
                    item.locator_input.as_ref().map(|input| input.text.as_str()),
                    item.locator.as_ref().map(|locator| locator.value.as_str()),
                    item.prefix.as_deref(),
                    item.suffix.as_deref(),
                ]
            })
            .flatten()
            .fold(0_usize, |sum, text| sum.saturating_add(text.len()));
        if items.len() > ibis_core::syntax::MAX_CITATION_ITEMS || bytes > ibis_core::syntax::MAX_CITATION_BYTES {
            return Err(CommandError::failed("app-command-citation-too-large"));
        }
        let literal = |text: Option<String>| {
            text.filter(|text| !text.is_empty())
                .into_iter()
                .map(|text| TextRun {
                    text,
                    ..TextRun::default()
                })
                .collect()
        };
        // The engine writes each form in the output's own way, a narrative
        // BibLaTeX citation as \textcite included; the picker only asks.
        let mut original_output = output.clone();
        let items: Vec<_> = items
            .into_iter()
            .enumerate()
            .map(|(index, item)| {
                let locator = match (item.locator, item.locator_input) {
                    (Some(_), Some(_)) => {
                        return Err(CommandError::from(format!(
                            "Reference {}: a typed locator and a captured locator cannot be combined.",
                            index + 1
                        )));
                    }
                    (locator, None) => locator.map(Into::into),
                    (None, Some(input)) => Some(ibis_core::locator::read_field(input.label, &input.text).map_err(
                        |error| CommandError::from(format!("Reference {}: {}", index + 1, field_hint(error))),
                    )?),
                };
                Ok(CitationItem {
                    item: item.id,
                    expected_cite: Some(item.cite),
                    form: item
                        .form
                        .map(ReferenceForm::citation)
                        .filter(|form| *form != CitationForm::Normal),
                    suppress_author: false,
                    locator,
                    prefix: literal(item.prefix),
                    suffix: literal(item.suffix),
                    annotation: None,
                    expected_annotation_cite: None,
                })
            })
            .collect::<Result<_, CommandError>>()?;
        let mut request = CitationRequest {
            items,
            keep_order: keep_order || matches!(output, OutputSpec::Key { .. }),
            output: output.clone(),
            form: CitationForm::Normal,
            input_language: None,
            output_locale: None,
        };
        if let Some(editor) = editor {
            request.output = editor.output.clone();
            request.form = editor.form;
            request.keep_order = keep_order || matches!(editor.output, OutputSpec::Key { .. });
            original_output = editor.output.clone();
        }
        let citation = self
            .engine
            .prepare_citation(&request, &cancel)
            .map_err(CommandError::from)?;
        let result = citation.result();
        if result.copy_content(request.items.len()).is_none() {
            let refused = |wanted: CitationError| {
                result.outcomes.iter().find_map(|outcome| match outcome {
                    ItemOutcome::Refused { item, error } if *error == wanted => Some(*item),
                    _ => None,
                })
            };
            // A key another record also has is named, so the picker can say what to rename.
            if let Some(key) = refused(CitationError::AmbiguousKey).and_then(|item| self.citation_key(&request, item)) {
                return Err(CommandError::SharedCitationKey { key });
            }
            if let Some(item) = refused(CitationError::UnsupportedForm) {
                let form = if request.items[item].form == Some(ibis_core::contract::CitationForm::Narrative) {
                    "Name in the sentence"
                } else if request.items[item].form == Some(CitationForm::YearOnly) {
                    "Year only"
                } else {
                    "This citation form"
                };
                let problem = format!(
                    "Reference {}: {form} cannot be expressed in this output or group. Choose Name in brackets, cite it alone, or choose another output.",
                    item + 1
                );
                let mut preview = self.publish_preview(reservation, citation, original_output)?;
                preview.problem = Some(problem);
                return Ok(preview);
            }
            return Err(if refused(CitationError::ConflictingKey).is_some() {
                "These references have conflicting citation keys. Resolve them in your library or choose a formatted style."
            } else {
                "These references cannot be copied in this format."
            }
            .into());
        }
        self.publish_preview(reservation, citation, original_output)
    }
    fn citation_key(&self, request: &CitationRequest, item: usize) -> Option<String> {
        let snapshot = self.engine.snapshot().ok()?;
        let LibraryObject::Record(record) = &snapshot.get(&request.items.get(item)?.item)?.object else {
            return None;
        };
        record.citation_key().ok().flatten().map(str::to_owned)
    }
    fn publish_preview(
        &self,
        reservation: u64,
        citation: PreparedCitation,
        original_output: OutputSpec,
    ) -> Result<FilePreview, CommandError> {
        let citation = if self.sample
            && citation
                .result()
                .outcomes
                .iter()
                .all(|outcome| matches!(outcome, ItemOutcome::Rendered { .. }))
        {
            citation.mark_sample().map_err(CommandError::from)?
        } else {
            citation
        };
        let mut slot = self
            .prepared
            .lock()
            .map_err(|_| CommandError::from("The preview is unavailable."))?;
        self.preview_cancellation(reservation)?
            .check()
            .map_err(CommandError::from)?;
        let result = citation.result();
        let preview = FilePreview {
            ticket: reservation.to_string(),
            content_hash: result.content_hash.as_str().to_owned(),
            text: result.text.clone(),
            html: result.html.clone(),
            warnings: citation_warnings_named(&citation),
            spans: result
                .display
                .nodes
                .iter()
                .map(|node| CitationSpan {
                    item: node.item,
                    text: node.content.iter().map(|run| run.text.as_str()).collect(),
                    runs: node
                        .content
                        .iter()
                        .any(|run| {
                            run.italic
                                || run.bold
                                || run.small_caps
                                || run.underline
                                || run.script != ibis_core::contract::Script::Normal
                        })
                        .then(|| node.content.clone()),
                })
                .collect(),
            problem: None,
            project_refusal: None,
            notices: result.notices.clone(),
        };
        *slot = Some((preview.ticket.clone(), Arc::new(citation), original_output));
        Ok(preview)
    }
    pub(crate) fn details(&self, ticket: &str, hash: &str, item: usize) -> Result<ReferenceDetails, CommandError> {
        let (reservation, citation, original_output) = {
            let slot = self.prepared.lock().map_err(|_| CommandError::SnapshotChanged)?;
            let (current, citation, output) = slot.as_ref().ok_or(CommandError::SnapshotChanged)?;
            if current != ticket || citation.result().content_hash.as_str() != hash {
                return Err(CommandError::SnapshotChanged);
            }
            (
                ticket.parse().map_err(|_| CommandError::SnapshotChanged)?,
                Arc::clone(citation),
                output.clone(),
            )
        };
        let cancel = self.preview_cancellation(reservation)?;
        // A numbered style writes reference entries, which have no name forms.
        let entry = citation.result().notices.contains(&Notice::ReferenceEntry);
        let bare_keys = matches!(
            &original_output,
            OutputSpec::Key {
                syntax: KeySyntax::Keys,
                ..
            }
        );
        let textcite = matches!(&original_output, OutputSpec::Key { syntax: KeySyntax::Biblatex, command } if command.as_deref() == Some("textcite"));
        let current_form = if entry || bare_keys {
            None
        } else {
            match citation.item_form(item) {
                Some(CitationForm::Normal) if textcite => Some(ReferenceForm::Narrative),
                Some(form) => ReferenceForm::ALL
                    .iter()
                    .find(|(_, citation)| *citation == form)
                    .map(|(wire, _)| *wire),
                None => None,
            }
        };
        let mut forms = Vec::new();
        for (wire, form) in ReferenceForm::ALL {
            cancel.check().map_err(CommandError::from)?;
            let semantic_reason = if entry {
                Some("Reference entries have no name forms. Choose another style.")
            } else if bare_keys {
                Some("Keys only has no name forms. Choose another output.")
            } else if textcite && form == CitationForm::Normal {
                Some("Not with \\textcite")
            } else {
                None
            };
            if let Some(reason) = semantic_reason {
                forms.push(FormOption {
                    form: wire,
                    example: None,
                    reason: Some(reason.into()),
                });
                continue;
            }
            let result = citation
                .form_example(&self.engine, &original_output, item, form, &cancel)
                .map_err(CommandError::from)?;
            let supported = result.outcomes.contains(&ItemOutcome::Rendered { item });
            // Refused in a group, a form may still work for this reference alone.
            let alone = !supported
                && result.outcomes.len() > 1
                && citation
                    .form_alone(&self.engine, &original_output, item, form, &cancel)
                    .map_err(CommandError::from)?;
            forms.push(FormOption {
                form: wire,
                example: supported.then(|| {
                    let example = if result.outcomes.len() == 1 {
                        result.text
                    } else {
                        result
                            .display
                            .nodes
                            .iter()
                            .filter(|node| node.item == Some(item))
                            .flat_map(|node| &node.content)
                            .map(|run| run.text.as_str())
                            .collect()
                    };
                    if example.chars().count() > 256 {
                        format!("{}…", example.chars().take(255).collect::<String>())
                    } else {
                        example
                    }
                }),
                reason: (!supported).then(|| {
                    if alone {
                        "Only when cited alone"
                    } else {
                        "Not in this output"
                    }
                    .into()
                }),
            });
        }
        self.preview_cancellation(reservation)?
            .check()
            .map_err(CommandError::from)?;
        Ok(ReferenceDetails { forms, current_form })
    }
    /// Whether the receipt `ticket` was prepared for this editor's request.
    pub(crate) fn prepared_for(&self, ticket: &str, editor: &crate::editor_endpoint::EditorRequest) -> bool {
        self.prepared.lock().is_ok_and(|prepared| {
            prepared.as_ref().is_some_and(|(current, citation, _)| {
                current == ticket
                    && citation.request_output() == &editor.output
                    && citation.request_form() == editor.form
            })
        })
    }

    pub(crate) fn invalidate_preview(&self) {
        self.invalidate();
    }

    pub(super) fn invalidate(&self) {
        self.reserve_preview();
        if let Ok(mut prepared) = self.prepared.lock() {
            *prepared = None;
        }
    }
    /// Cite the shown citation again for a copy. This can wait for the
    /// formatter, so it holds no lock and its caller keeps it off the main thread.
    pub(crate) fn recite(&self, ticket: &str, hash: &str) -> Result<Recited, CommandError> {
        let citation = {
            let prepared = self.prepared.lock().map_err(|_| CommandError::SnapshotChanged)?;
            let (current, citation, _) = prepared.as_ref().ok_or(CommandError::SnapshotChanged)?;
            if current != ticket || citation.result().content_hash.as_str() != hash {
                return Err(CommandError::SnapshotChanged);
            }
            Arc::clone(citation)
        };
        // A newer preview cancels this one's work, a copy's included.
        let cancel = self.preview_cancellation(ticket.parse().map_err(|_| CommandError::SnapshotChanged)?)?;
        let result = self.engine.recite(&citation, &cancel).map_err(|error| match error {
            ibis_engine::CitationCommitError::Engine(_) => {
                CommandError::from("The citation could not be copied. Try again.")
            }
            _ => CommandError::SnapshotChanged,
        })?;
        Ok(Recited {
            ticket: ticket.to_owned(),
            hash: hash.to_owned(),
            citation,
            result,
        })
    }

    /// Hand `write` what `recite` confirmed, while that preview is still the
    /// shown one; a newer preview or another library refuses it.
    pub(crate) fn copy_recited(
        &self,
        recited: &Recited,
        write: impl FnOnce(ibis_core::contract::ClipboardContent<'_>) -> Result<(), String>,
    ) -> Result<crate::copy::Copied, CommandError> {
        let prepared = self.prepared.lock().map_err(|_| CommandError::SnapshotChanged)?;
        let (current, citation, _) = prepared.as_ref().ok_or(CommandError::SnapshotChanged)?;
        if current != &recited.ticket || !Arc::ptr_eq(citation, &recited.citation) {
            return Err(CommandError::SnapshotChanged);
        }
        // A preview asked for since, even one that failed, retires this copy.
        let latest = self.latest_preview.lock().map_err(|_| CommandError::SnapshotChanged)?.0;
        if latest.to_string() != recited.ticket {
            return Err(CommandError::SnapshotChanged);
        }
        // Zotero may have begun a full read while the copy waited for this thread.
        self.engine
            .still_current(citation)
            .map_err(|_| CommandError::SnapshotChanged)?;
        let copied = &recited.result;
        write(
            copied
                .copy_content(citation.items().len())
                .ok_or(CommandError::SnapshotChanged)?,
        )?;
        Ok(crate::copy::Copied {
            summary: Some(crate::copy::ReceiptSummary::citation(copied)),
            text: copied.text.clone(),
            warnings: citation_warnings_named(citation),
            delivery: None,
            cited: citation.items(),
        })
    }

    /// Both steps of a copy at once.
    #[cfg(test)]
    pub(crate) fn copy(
        &self,
        ticket: &str,
        hash: &str,
        write: impl FnOnce(ibis_core::contract::ClipboardContent<'_>) -> Result<(), String>,
    ) -> Result<crate::copy::Copied, CommandError> {
        self.copy_recited(&self.recite(ticket, hash)?, write)
    }
}

/// A shown citation, cited again for a copy.
pub(crate) struct Recited {
    ticket: String,
    hash: String,
    citation: Arc<PreparedCitation>,
    result: CitationResult,
}

impl Recited {
    pub(crate) fn ticket(&self) -> &str {
        &self.ticket
    }
    pub(crate) fn content_hash(&self) -> &str {
        &self.hash
    }
}

pub(super) fn native_source(info: &ibis_engine::LibraryInfo) -> String {
    match info.zotero {
        Some(EngineZoteroLibrary::Personal) => "zotero:user".into(),
        Some(EngineZoteroLibrary::Group(id)) => format!("zotero:group:{id}"),
        None => info.id.as_str().to_owned(),
    }
}

fn session_key(snapshot: &Snapshot, id: &ItemRef) -> Option<String> {
    let LibraryObject::Record(record) = &snapshot.get(id)?.object else {
        return None;
    };
    Some(
        record
            .citation_key()
            .ok()
            .flatten()
            .unwrap_or("a reference without a key")
            .to_owned(),
    )
}

/// What the citation could not say in full, each naming its reference by key or title.
fn citation_warnings_named(citation: &PreparedCitation) -> Vec<String> {
    let labels: Vec<_> = (0..citation.items().len())
        .map(|item| citation.item_label(item))
        .collect();
    citation_warnings(citation.result(), &labels)
}
/// `labels` names each cited reference by key or title. Two references can
/// share a key across libraries, so a shared name also says which one it is.
fn citation_warnings(result: &CitationResult, labels: &[Option<String>]) -> Vec<String> {
    use ibis_engine::catalogue::{self, Audience};
    let name = |item: usize| match labels.get(item).cloned().flatten() {
        Some(label) if labels.iter().filter(|other| other.as_ref() == Some(&label)).count() > 1 => {
            format!("{label} (reference {})", item + 1)
        }
        Some(label) => label,
        None => format!("Reference {}", item + 1),
    };
    let mut warnings = vec![];
    for warning in &result.warnings {
        let reference = name(warning.item);
        warnings.push(catalogue::sentence(
            "warning",
            warning.code,
            Audience::App,
            &[("reference", &reference)],
        ));
    }
    if result.notices.contains(&Notice::YearSuffixWithheld) {
        warnings.push(catalogue::sentence(
            "notice",
            Notice::YearSuffixWithheld,
            Audience::App,
            &[],
        ));
    }
    for loss in &result.losses {
        let what = catalogue::text(&catalogue::id("loss", loss.field), None, &[]).unwrap_or_default();
        let target = loss.item.map(|item| format!(" for {}", name(item))).unwrap_or_default();
        warnings.extend(catalogue::text(
            "app-loss",
            None,
            &[("what", &what), ("target", &target)],
        ));
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::contract::{CitationWarning, CitationWarningCode, DisplayTree};

    #[test]
    fn a_key_two_references_share_still_says_which_one_a_warning_means() {
        let result = CitationResult::new(
            "(Doe, 2026a; Doe, 2026b)".into(),
            None,
            None,
            DisplayTree::default(),
            vec![ItemOutcome::Rendered { item: 0 }, ItemOutcome::Rendered { item: 1 }],
            vec![],
            vec![],
        )
        .with_warnings(vec![CitationWarning {
            item: 1,
            code: CitationWarningCode::SourceField,
        }]);
        let shared = [Some("Doe2026".to_owned()), Some("Doe2026".to_owned())];
        assert_eq!(
            citation_warnings(&result, &shared),
            ["A field of Doe2026 (reference 2) could not be fully converted. Check it in your library."]
        );
        let distinct = [Some("Doe2026".to_owned()), Some("Roe2026".to_owned())];
        assert_eq!(
            citation_warnings(&result, &distinct),
            ["A field of Roe2026 could not be fully converted. Check it in your library."]
        );
    }
}

#[cfg(test)]
mod parity_tests;

#[cfg(test)]
mod review_tests;

#[cfg(test)]
mod zotero_acceptance;
