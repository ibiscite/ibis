//! The file picker delegates acquisition, search, rendering and copy validation to the engine.
use super::search::{SearchQuery, SearchReply, SearchResult};
use super::{CommandError, LibraryStatus};
use ibis_core::{
    cite::{KeyFormat, Locator},
    contract::{CitationRequest, LibraryObject},
    settings::CitationStyle,
};
use ibis_engine::{Cancellation, Engine, Method, PreparedCitation, request::SearchRequest};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

static TICKETS: AtomicU64 = AtomicU64::new(1);
pub(crate) struct FileLibrary {
    pub(super) engine: Engine,
    prepared: Mutex<Option<(String, PreparedCitation)>>,
    latest_preview: AtomicU64,
    observing: AtomicBool,
}
#[derive(Deserialize, specta::Type)]
pub(crate) struct FileSelection {
    #[specta(type = specta_typescript::Unknown)]
    pub(crate) id: Value,
    pub(crate) cite: String,
    pub(crate) locator: Option<Locator>,
}
#[derive(Serialize, specta::Type)]
pub(crate) struct FilePreview {
    pub(crate) ticket: String,
    pub(crate) content_hash: String,
    pub(crate) text: String,
    pub(crate) html: Option<String>,
    pub(crate) warnings: Vec<String>,
}
impl FileLibrary {
    pub(crate) fn open(path: &Path) -> Result<Self, super::LoadError> {
        let engine = Engine::new().map_err(|_| super::LoadError::Stopped)?;
        engine
            .dispatch(
                Method::Libraries,
                &json!({"action":"open-file","path":path}),
                &Cancellation::default(),
            )
            .map_err(|error| {
                super::LoadError::Source(match error.code {
                    ibis_core::contract::ErrorCode::SourceUnavailable => ibis_core::source::SourceError::Unavailable,
                    ibis_core::contract::ErrorCode::ResourceLimit => ibis_core::source::SourceError::TooLarge,
                    _ => ibis_core::source::SourceError::Unreadable,
                })
            })?;
        Ok(Self::from_engine(engine))
    }

    fn from_engine(engine: Engine) -> Self {
        Self {
            engine,
            prepared: Mutex::new(None),
            latest_preview: AtomicU64::new(0),
            observing: AtomicBool::new(false),
        }
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
    pub(super) fn reload(&self) -> Result<(), super::LoadError> {
        self.engine
            .dispatch(Method::Libraries, &json!({"action":"reload"}), &Cancellation::default())
            .map_err(|_| super::LoadError::Stopped)?;
        if let Some(error) = self
            .engine
            .libraries()
            .map_err(|_| super::LoadError::Stopped)?
            .libraries
            .iter()
            .find_map(|info| info.refresh_error)
        {
            return Err(super::LoadError::Source(
                if error == ibis_core::contract::ErrorCode::SourceUnavailable {
                    ibis_core::source::SourceError::Unavailable
                } else {
                    ibis_core::source::SourceError::Incomplete
                },
            ));
        }
        Ok(())
    }
    pub(super) fn revision(&self) -> String {
        self.engine
            .snapshot()
            .map(|s| s.fingerprints().full.as_str().to_owned())
            .unwrap_or_default()
    }
    pub(super) fn status(&self, status: &mut LibraryStatus) {
        status.revision = self.revision();
        if let Ok(info) = self.engine.libraries() {
            status.references = info.libraries.iter().map(|i| i.objects).sum();
            status.warnings = info.libraries.iter().map(|i| i.warnings_total).sum();
            status.source_counts = info
                .libraries
                .iter()
                .map(|i| (i.id.as_str().to_owned(), i.objects))
                .collect();
            if status.error.is_none() && info.libraries.iter().any(|i| i.refresh_error.is_some()) {
                status.stale = true;
                status.error = Some("The library could not be refreshed; keeping the last loaded references.".into());
            }
            if status.note.is_none() && info.libraries.iter().any(|i| i.watch_warning.is_some()) {
                status.note = Some("Live updates are unavailable. Reload the library after editing it.".into());
            }
        }
    }
    pub(crate) fn search(
        &self,
        query: &SearchQuery,
        limit: usize,
        scope: Option<Vec<String>>,
    ) -> Result<SearchReply, String> {
        let interpretation = query
            .interpret_locators
            .then(|| ibis_core::locator_query::split(&query.text))
            .flatten();
        if interpretation
            .as_ref()
            .map_or(query.text.as_str(), |q| q.search.as_str())
            .trim()
            .is_empty()
        {
            return Ok(SearchReply {
                revision: self.revision(),
                results: vec![],
                interpretation,
            });
        }
        let reply = self
            .engine
            .search(
                &SearchRequest {
                    query: interpretation
                        .as_ref()
                        .map_or_else(|| query.text.clone(), |q| q.search.clone()),
                    limit: limit.min(200),
                    libraries: scope
                        .map(|ids| serde_json::from_value(json!(ids)))
                        .transpose()
                        .map_err(|_| "Invalid library scope.".to_string())?,
                },
                &Cancellation::default(),
            )
            .map_err(|_| "The library could not be searched.".to_string())?;
        let revision = reply.snapshot.full.as_str().to_owned();
        let results = reply
            .hits
            .into_iter()
            .filter_map(|hit| match hit.item.object {
                LibraryObject::Record(record) => Some(SearchResult::from_engine(
                    &record,
                    hit.item.fingerprints.cite.as_str(),
                    hit.matched,
                    &revision,
                )),
                _ => None,
            })
            .collect();
        Ok(SearchReply {
            revision,
            results,
            interpretation,
        })
    }
    pub(crate) fn reserve_preview(&self) -> u64 {
        let request = TICKETS.fetch_add(1, Ordering::Relaxed);
        self.latest_preview.fetch_max(request, Ordering::SeqCst);
        request
    }
    pub(crate) fn preview(
        &self,
        reservation: u64,
        items: Vec<FileSelection>,
        format: KeyFormat,
        style: Option<CitationStyle>,
    ) -> Result<FilePreview, CommandError> {
        let output = if let Some(style) = style {
            json!({"kind":"csl","style":style})
        } else {
            let (syntax, command) = match format {
                KeyFormat::Pandoc => ("pandoc", None),
                KeyFormat::Autocite => ("biblatex", Some("autocite")),
                KeyFormat::Parencite => ("biblatex", Some("parencite")),
                KeyFormat::Textcite => ("biblatex", Some("textcite")),
                KeyFormat::Cite => ("latex", Some("cite")),
                KeyFormat::Typst => ("typst", None),
                KeyFormat::Keys => ("keys", None),
            };
            json!({"kind":"key","syntax":syntax,"command":command})
        };
        let items: Vec<_> = items
            .into_iter()
            .map(|item| json!({"item":item.id,"expected_cite":item.cite,"locator":item.locator.map(|locator| json!({"term":locator.label,"value":locator.value}))}))
            .collect();
        let request: CitationRequest = serde_json::from_value(json!({"items":items,"output":output,"form":if style == Some(CitationStyle::Ieee) { "bibliography-only" } else { "normal" },"keep_order":style.is_none()}))
            .map_err(|_| CommandError::from("The selected references could not be read. Search again."))?;
        let citation = self
            .engine
            .prepare_citation(&request, &Cancellation::default())
            .map_err(CommandError::from)?;
        let result = citation.result();
        if result.copy_content(request.items.len()).is_none() {
            return Err("These references cannot be copied in this format.".into());
        }
        self.publish_preview(reservation, citation)
    }
    fn publish_preview(&self, reservation: u64, citation: PreparedCitation) -> Result<FilePreview, CommandError> {
        let mut slot = self
            .prepared
            .lock()
            .map_err(|_| CommandError::from("The preview is unavailable."))?;
        if self.latest_preview.load(Ordering::SeqCst) != reservation {
            return Err("A newer citation preview replaced this request.".into());
        }
        let result = citation.result();
        let preview = FilePreview {
            ticket: reservation.to_string(),
            content_hash: result.content_hash.as_str().to_owned(),
            text: result.text.clone(),
            html: result.html.clone(),
            warnings: if result.losses.is_empty() {
                vec![]
            } else {
                vec!["This format cannot express part of the citation.".into()]
            },
        };
        *slot = Some((preview.ticket.clone(), citation));
        Ok(preview)
    }
    pub(super) fn invalidate(&self) {
        self.reserve_preview();
        if let Ok(mut prepared) = self.prepared.lock() {
            *prepared = None;
        }
    }
    pub(crate) fn copy(
        &self,
        ticket: &str,
        hash: &str,
        write: impl FnOnce(ibis_core::contract::ClipboardContent<'_>) -> Result<(), String>,
    ) -> Result<crate::copy::Copied, CommandError> {
        let prepared = self.prepared.lock().map_err(|_| CommandError::SnapshotChanged)?;
        let (current, citation) = prepared.as_ref().ok_or(CommandError::SnapshotChanged)?;
        if current != ticket || citation.result().content_hash.as_str() != hash {
            return Err(CommandError::SnapshotChanged);
        }
        self.engine
            .commit_citation(citation, &citation.result().content_hash, write)
            .map_err(|error| match error {
                ibis_engine::CitationCommitError::Engine(_) => {
                    CommandError::from("The citation could not be copied. Try again.")
                }
                _ => CommandError::SnapshotChanged,
            })??;
        Ok(crate::copy::Copied {
            text: citation.result().text.clone(),
            warnings: if citation.result().losses.is_empty() {
                vec![]
            } else {
                vec!["This format cannot express part of the citation.".into()]
            },
        })
    }
}

#[cfg(test)]
mod review_tests;

#[cfg(test)]
mod zotero_acceptance;
