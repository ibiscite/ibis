//! The picker delegates acquisition, search, rendering and copy validation to the engine.
use super::search::{Locator, SearchQuery, SearchReply, SearchResult};
use super::{CommandError, LibraryStatus};
use ibis_core::{
    cite::KeyFormat,
    contract::{
        CitationError, CitationForm, CitationItem, CitationRequest, CitationResult, Fingerprint, ItemOutcome, ItemRef,
        KeySyntax, LibraryId, LibraryObject, LocatorTerm, Notice, OutputSpec,
    },
    settings::{CitationStyle, ZoteroLibraries, ZoteroLibrary},
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
use std::path::Path;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

static TICKETS: AtomicU64 = AtomicU64::new(1);
pub(crate) struct FileLibrary {
    pub(super) engine: Engine,
    prepared: Mutex<Option<(String, PreparedCitation)>>,
    latest_preview: Mutex<(u64, Cancellation)>,
    observing: AtomicBool,
}
#[derive(Deserialize, specta::Type)]
pub(crate) struct FileSelection {
    #[specta(type = specta_typescript::Unknown)]
    pub(crate) id: ItemRef,
    #[specta(type = String)]
    pub(crate) cite: Fingerprint,
    /// A canonical locator captured from search; its value is never parsed again.
    pub(crate) locator: Option<Locator>,
    /// Raw Page editor text, mutually exclusive with a captured locator.
    #[specta(optional)]
    pub(crate) page_input: Option<String>,
    #[specta(optional)]
    pub(crate) form: Option<ReferenceForm>,
}
#[derive(Clone, Copy, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ReferenceForm {
    Normal,
    Narrative,
    AuthorOmitted,
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
            .map_err(|error| match error.code {
                ibis_core::contract::ErrorCode::BadInput => super::LoadError::InvalidData,
                ibis_core::contract::ErrorCode::SourceUnavailable => ibis_core::source::SourceError::Unavailable.into(),
                ibis_core::contract::ErrorCode::ResourceLimit => ibis_core::source::SourceError::TooLarge.into(),
                _ => ibis_core::source::SourceError::Unreadable.into(),
            })?;
        Ok(Self::from_engine(engine))
    }

    fn from_engine(engine: Engine) -> Self {
        Self {
            engine,
            prepared: Mutex::new(None),
            latest_preview: Mutex::new((0, Cancellation::default())),
            observing: AtomicBool::new(false),
        }
    }
    pub(super) fn open_zotero(chosen: &ZoteroLibraries, cancel: &Cancellation) -> Result<Self, super::LoadError> {
        let engine = Engine::new().map_err(|e| super::LoadError::EngineZotero(e.code))?;
        Self::open_zotero_with(engine, chosen, cancel)
    }

    fn open_zotero_with(
        engine: Engine,
        chosen: &ZoteroLibraries,
        cancel: &Cancellation,
    ) -> Result<Self, super::LoadError> {
        let bridge = Self::from_engine(engine);
        bridge.refresh_zotero(chosen, false, cancel)?;
        Ok(bridge)
    }

    pub(super) fn refresh_zotero(
        &self,
        chosen: &ZoteroLibraries,
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
            .map_err(|e| super::LoadError::EngineZotero(e.code))?
            .libraries
            .iter()
            .filter_map(|library| library.zotero)
            .collect();
        // The listing is sorted by opaque ID, not by the user's selection order.
        let same = loaded.len() == libraries.len() && loaded.iter().all(|library| libraries.contains(library));
        let request = if force && same {
            json!({"action":"reload"})
        } else {
            json!({"action":"open-zotero","libraries":libraries})
        };
        self.engine
            .dispatch(Method::Libraries, &request, cancel)
            .map_err(|e| super::LoadError::EngineZotero(e.code))?;
        let info = self
            .engine
            .libraries()
            .map_err(|e| super::LoadError::EngineZotero(e.code))?;
        if let Some(code) = info.libraries.iter().find_map(|info| info.refresh_error) {
            return Err(super::LoadError::EngineZotero(code));
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
            status.source_counts = info.libraries.iter().map(|i| (native_source(i), i.objects)).collect();
            if info.libraries.iter().any(|i| i.refresh_error.is_some()) {
                status.stale = true;
                if status.error.is_none() {
                    status.error =
                        Some("The library could not be refreshed; keeping the last loaded references.".into());
                }
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
        cancel: &Cancellation,
    ) -> Result<SearchReply, String> {
        if query.text.trim().is_empty() {
            return Ok(SearchReply {
                revision: self.revision(),
                results: vec![],
                interpretation: None,
                list: None,
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
                    results: vec![],
                    interpretation: None,
                    list: None,
                });
            }
            Err(_) => return Err("The library could not be searched.".to_string()),
        };
        let revision = reply.snapshot.full.as_str().to_owned();
        let results = reply
            .hits
            .into_iter()
            .filter_map(|hit| match hit.item.object {
                LibraryObject::Record(record) => {
                    let mut result = SearchResult::from_engine(
                        &record,
                        hit.item.fingerprints.cite.as_str(),
                        hit.matched,
                        hit.partial,
                        &revision,
                    );
                    if let Some(source) = sources.get(record.id.library.as_str()) {
                        result.source.clone_from(source);
                    }
                    Some(result)
                }
                _ => None,
            })
            .collect();
        Ok(SearchReply {
            revision,
            results,
            interpretation: reply.locator.map(Into::into),
            list: reply
                .list
                .map(|list| super::search::PastedList { missing: list.missing }),
        })
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
            return Err("A newer citation preview replaced this request.".into());
        }
        Ok(latest.1.clone())
    }
    pub(crate) fn preview(
        &self,
        reservation: u64,
        items: Vec<FileSelection>,
        format: KeyFormat,
        style: Option<CitationStyle>,
    ) -> Result<FilePreview, CommandError> {
        // A late-starting worker stops before building a request. A running
        // worker shares this token with the engine and QuickJS interrupt handler.
        let cancel = self.preview_cancellation(reservation)?;
        // Narrative is the textcite member of the BibLaTeX output family.
        // Mixed command clusters remain outside the picker's small editor.
        let format = if items.len() == 1
            && items[0].form == Some(ReferenceForm::Narrative)
            && matches!(format, KeyFormat::Autocite | KeyFormat::Parencite)
        {
            KeyFormat::Textcite
        } else {
            format
        };
        let output = if let Some(style) = style {
            OutputSpec::Csl {
                style: match style {
                    CitationStyle::Apa => "apa",
                    CitationStyle::ChicagoAuthorDate => "chicago-author-date",
                    CitationStyle::HarvardCiteThemRight => "harvard-cite-them-right",
                    CitationStyle::Ieee => "ieee",
                    CitationStyle::ModernLanguageAssociation => "modern-language-association",
                }
                .into(),
            }
        } else {
            let (syntax, command) = match format {
                KeyFormat::Pandoc => (KeySyntax::Pandoc, None),
                KeyFormat::Autocite => (KeySyntax::Biblatex, Some("autocite")),
                KeyFormat::Parencite => (KeySyntax::Biblatex, Some("parencite")),
                KeyFormat::Textcite => (KeySyntax::Biblatex, Some("textcite")),
                KeyFormat::Cite => (KeySyntax::Latex, Some("cite")),
                KeyFormat::Typst => (KeySyntax::Typst, None),
                KeyFormat::Keys => (KeySyntax::Keys, None),
            };
            OutputSpec::Key {
                syntax,
                command: command.map(str::to_owned),
            }
        };
        let items: Vec<_> = items
            .into_iter()
            .enumerate()
            .map(|(index, item)| {
                let locator = match (item.locator, item.page_input) {
                    (Some(_), Some(_)) => {
                        return Err(CommandError::from(format!(
                            "Reference {}: Page editor input and a captured locator cannot be combined.",
                            index + 1
                        )));
                    }
                    (locator, None) => locator.map(Into::into),
                    (None, Some(input)) => Some(ibis_core::contract::Locator {
                        term: LocatorTerm::Page,
                        value: ibis_core::locator::page_value(&input).map_err(|error| {
                            let hint = match error {
                                ibis_core::locator::PageValueError::MissingValue => {
                                    "Page needs a page or range. Enter one after the label, or clear Page."
                                }
                                ibis_core::locator::PageValueError::InvalidValue => {
                                    "Page needs a valid page or range after its label. Correct it, or clear Page."
                                }
                            };
                            CommandError::from(format!("Reference {}: {hint}", index + 1))
                        })?,
                    }),
                };
                Ok(CitationItem {
                    item: item.id,
                    expected_cite: Some(item.cite),
                    form: (item.form == Some(ReferenceForm::Narrative)).then_some(CitationForm::Narrative),
                    suppress_author: item.form == Some(ReferenceForm::AuthorOmitted),
                    locator,
                    prefix: vec![],
                    suffix: vec![],
                    annotation: None,
                    expected_annotation_cite: None,
                })
            })
            .collect::<Result<_, CommandError>>()?;
        let request = CitationRequest {
            items,
            output,
            form: if style == Some(CitationStyle::Ieee) {
                CitationForm::BibliographyOnly
            } else {
                CitationForm::Normal
            },
            keep_order: style.is_none(),
            input_language: None,
            output_locale: None,
        };
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
                    "Narrative"
                } else {
                    "This citation form"
                };
                return Err(format!("Reference {}: {form} cannot be expressed in this output or group. Choose Normal, cite it alone, or choose another output.", item + 1).into());
            }
            return Err(if refused(CitationError::ConflictingKey).is_some() {
                "These references have conflicting citation keys. Resolve them in your library or choose a formatted style."
            } else {
                "These references cannot be copied in this format."
            }
            .into());
        }
        self.publish_preview(reservation, citation)
    }
    fn citation_key(&self, request: &CitationRequest, item: usize) -> Option<String> {
        let snapshot = self.engine.snapshot().ok()?;
        let LibraryObject::Record(record) = &snapshot.get(&request.items.get(item)?.item)?.object else {
            return None;
        };
        record.citation_key().ok().flatten().map(str::to_owned)
    }
    fn publish_preview(&self, reservation: u64, citation: PreparedCitation) -> Result<FilePreview, CommandError> {
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
            warnings: citation_warnings(result),
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
            warnings: citation_warnings(citation.result()),
            delivery: None,
        })
    }
}

fn native_source(info: &ibis_engine::LibraryInfo) -> String {
    match info.zotero {
        Some(EngineZoteroLibrary::Personal) => "zotero:user".into(),
        Some(EngineZoteroLibrary::Group(id)) => format!("zotero:group:{id}"),
        None => info.id.as_str().to_owned(),
    }
}

fn citation_warnings(result: &CitationResult) -> Vec<String> {
    let mut warnings = vec![];
    if result.notices.contains(&Notice::YearSuffixWithheld) {
        warnings.push("Year suffixes require the whole document; Ibis leaves them out.".into());
    }
    for loss in &result.losses {
        use ibis_core::contract::LossField;
        let detail = match loss.field {
            LossField::Locator => "Page or location",
            LossField::AuthorSuppression => "Omit author",
            _ => "Part of the citation",
        };
        let owner = loss
            .item
            .map_or_else(String::new, |item| format!("Reference {}: ", item + 1));
        warnings.push(format!("{owner}{detail} cannot be fully expressed in this output."));
    }
    warnings
}

#[cfg(test)]
mod parity_tests;

#[cfg(test)]
mod review_tests;

#[cfg(test)]
mod zotero_acceptance;
