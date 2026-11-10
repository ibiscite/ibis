//! Typed services retain an immutable snapshot while refresh prepares its replacement.
use crate::{Method, request::*};
use ibis_core::{
    contract::*,
    search::{MatchField, SnapshotIndex},
};
pub use ibis_sources::{LibraryKind, SourceCapability, SourceWarning, WarningCause, WarningCode};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, RwLock, Weak,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
};

#[path = "health.rs"]
pub mod health;
mod sources;
pub use sources::PreparedFileReload;

mod formatting;
pub use formatting::Cancellation;
mod prepared;
pub use prepared::{CitationCommitError, PreparedCitation};
mod project;
pub use ibis_sources::project::{Doubt, EntryRefusal, KeyCheck, ProjectProblem, ProjectRead};
pub use project::{
    EntryResult, ProjectAssessment, ProjectAssessmentState, ProjectKey, ProjectPolicy, ProjectResult,
    WritingProjectCitation, project_name,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum WatchWarning {
    LiveRefreshOff,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LibraryInfo {
    pub id: LibraryId,
    pub kind: LibraryKind,
    pub capabilities: Vec<SourceCapability>,
    pub objects: usize,
    pub warnings: Vec<ibis_sources::SourceWarning>,
    pub warnings_total: usize,
    pub refresh_error: Option<ErrorCode>,
    /// What a person can do about `refresh_error`, when the failure says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_reason: Option<ErrorReason>,
    pub updated_at_ms: u64,
    /// A readable source remains usable when automatic updates are unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watch_warning: Option<WatchWarning>,
    /// Which Zotero library this is, for a Zotero library.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zotero: Option<ZoteroLibrary>,
    /// Every repeated citation key, beyond the warnings shown, so a lookup can
    /// say why the key selects nothing.
    #[serde(skip)]
    #[schemars(skip)]
    pub duplicate_keys: std::collections::BTreeSet<String>,
    /// The read stopped at a limit, so a key past it selects nothing.
    #[serde(skip)]
    #[schemars(skip)]
    pub cut_short: bool,
}
impl LibraryInfo {
    fn clean(id: LibraryId, kind: LibraryKind, objects: usize) -> Self {
        Self {
            id,
            kind,
            capabilities: kind.capabilities().to_vec(),
            objects,
            warnings: vec![],
            warnings_total: 0,
            refresh_error: None,
            refresh_reason: None,
            updated_at_ms: now_ms(),
            watch_warning: None,
            zotero: None,
            duplicate_keys: Default::default(),
            cut_short: false,
        }
    }
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct LibrariesResult {
    pub libraries: Vec<LibraryInfo>,
    /// Answers `zotero-profiles`: each profile's safe name, never its location.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zotero_profiles: Option<Vec<String>>,
    /// Answers `zotero-profiles`: whether a Zotero library is on this computer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zotero_installed: Option<bool>,
    /// Answers `zotero-groups`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zotero_groups: Option<Vec<ZoteroGroup>>,
}
/// A Zotero group library by its public ID and a name safe to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ZoteroGroup {
    pub id: u64,
    pub name: String,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SearchHit {
    pub item: SnapshotObject,
    pub score: u32,
    pub matched: Vec<MatchField>,
    /// Some term matched only with one typo.
    pub typo: bool,
    /// Some terms did not match: one of the closest matches, given only when
    /// nothing matches every term.
    pub partial: bool,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PastedList {
    pub missing: Vec<String>,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SearchResult {
    pub snapshot: Fingerprints,
    pub hits: Vec<SearchHit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<PastedList>,
    /// The locator read off the end of the query, when one was asked for and found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locator: Option<ibis_core::locator::TrailingLocator>,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct StylesResult {
    pub styles: Vec<StyleInfo>,
    /// How many styles match the request, beyond this page.
    pub total: usize,
    /// Every language a style can be written in.
    pub locales: Vec<LocaleInfo>,
    /// Styles change only with an Ibis release; these say which release.
    pub styles_release: UpstreamRelease,
    pub locales_release: UpstreamRelease,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct StyleInfo {
    pub id: String,
    pub title: String,
    /// The style's own last-changed date: its only version marker.
    pub updated: String,
    /// One of the styles offered first.
    pub curated: bool,
    /// The independent style whose rules this one uses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// The language this style is written in unless a request names another.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_locale: Option<String>,
    /// CSL's citation format, such as `author-date`, `numeric` or `note`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct LocaleInfo {
    pub tag: String,
    pub native: String,
    pub english: String,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct UpstreamRelease {
    pub repo: String,
    pub tag: String,
    pub commit: String,
}
#[derive(Debug)]
pub struct DispatchResult {
    pub data: Value,
    pub exit_code: ExitCode,
    /// Literal terminal citation only after the canonical completeness/hash gate.
    pub text: Option<String>,
    pub source_findings: bool,
}
impl DispatchResult {
    /// Every frontend shares the same serialized result budget.
    pub fn bounded(self) -> Result<Self, EngineError> {
        crate::wire::encode(&self.data, crate::protocol::MAX_RESULT_BYTES)
            .map_err(|_| error(ErrorCode::ResourceLimit))?;
        Ok(self)
    }
    pub fn citation(result: CitationResult, expected_items: usize) -> Result<Self, EngineError> {
        let text = result
            .copy_content(expected_items)
            .map(|content| content.text.to_owned());
        let exit_code = if result.losses.is_empty() && text.is_some() {
            ExitCode::Ok
        } else {
            ExitCode::Findings
        };
        Self {
            data: value(result)?,
            exit_code,
            text,
            source_findings: false,
        }
        .bounded()
    }
}

/// A wake-up to read the current library state.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct LibraryChange;
pub struct LibrarySubscription {
    id: u64,
    owner: Weak<Shared>,
    receiver: mpsc::Receiver<LibraryChange>,
}
/// Dropping the owner's handle disconnects a blocked subscriber, even if a
/// worker still retains the engine. Pending notifications can drain first.
pub struct LibrarySubscriptionClose {
    id: u64,
    owner: Weak<Shared>,
}
impl Drop for LibrarySubscriptionClose {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.upgrade()
            && let Ok(mut subscribers) = owner.subscribers.lock()
        {
            subscribers.remove(&self.id);
        }
    }
}
impl LibrarySubscription {
    pub fn recv(&self) -> Result<LibraryChange, mpsc::RecvError> {
        self.receiver.recv()
    }
    pub fn close_handle(&self) -> LibrarySubscriptionClose {
        LibrarySubscriptionClose {
            id: self.id,
            owner: self.owner.clone(),
        }
    }
    pub fn recv_timeout(&self, timeout: std::time::Duration) -> Result<LibraryChange, mpsc::RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }
    pub fn try_recv(&self) -> Result<LibraryChange, mpsc::TryRecvError> {
        self.receiver.try_recv()
    }
}

impl Drop for LibrarySubscription {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.upgrade()
            && let Ok(mut subscribers) = owner.subscribers.lock()
        {
            subscribers.remove(&self.id);
        }
    }
}
struct State {
    /// Accepted data/diagnostic identity; read errors alone do not replace it.
    accepted: Arc<()>,
    zotero_generation: u64,
    zotero_ready: bool,
    index: Arc<SnapshotIndex>,
    libraries: BTreeMap<LibraryId, LibraryInfo>,
}
struct Shared {
    formatter: Mutex<Option<ibis_csl::Formatter>>,
    state: RwLock<Arc<State>>,
    /// Serializes publications without blocking readers during index building.
    publication: Mutex<()>,
    subscribers: Mutex<BTreeMap<u64, mpsc::SyncSender<LibraryChange>>>,
    next_subscription: AtomicU64,
    sources: Mutex<sources::Registry>,
}
#[derive(Clone)]
pub struct Engine {
    shared: Arc<Shared>,
    resources: Arc<sources::Owners>,
    watch_files: bool,
}
impl Engine {
    pub fn new() -> Result<Self, EngineError> {
        Ok(Self::from_snapshot(Arc::new(
            Snapshot::new(vec![]).map_err(|_| error(ErrorCode::Internal))?,
        )))
    }
    /// One-shot callers read files without installing background watches.
    pub fn without_file_watches(mut self) -> Self {
        self.watch_files = false;
        self
    }
    pub fn from_snapshot(snapshot: Arc<Snapshot>) -> Self {
        let mut libraries = BTreeMap::<LibraryId, LibraryInfo>::new();
        for item in snapshot.objects() {
            let id = item.object.id().library.clone();
            libraries
                .entry(id.clone())
                .or_insert_with(|| LibraryInfo::clean(id, LibraryKind::Snapshot, 0))
                .objects += 1;
        }
        Self {
            shared: Arc::new(Shared {
                formatter: Mutex::new(None),
                state: RwLock::new(Arc::new(State {
                    accepted: Arc::new(()),
                    zotero_generation: 0,
                    zotero_ready: true,
                    index: Arc::new(SnapshotIndex::new(snapshot)),
                    libraries,
                })),
                publication: Mutex::new(()),
                subscribers: Mutex::new(BTreeMap::new()),
                next_subscription: AtomicU64::new(0),
                sources: Mutex::new(sources::Registry::default()),
            }),
            resources: Arc::new(sources::Owners::default()),
            watch_files: true,
        }
    }
    /// An isolated IPv4-loopback transport for synthetic clients.
    pub fn with_zotero_port(port: u16) -> Result<Self, EngineError> {
        Self::with_zotero_profile(port, b"")
    }
    /// The same, with `profile` standing for a Zotero profile's data directory.
    pub fn with_zotero_profile(port: u16, profile: &[u8]) -> Result<Self, EngineError> {
        let mut engine = Self::new()?;
        engine.resources = Arc::new(sources::Owners::on_port(port, profile));
        Ok(engine)
    }
    fn state(&self) -> Result<Arc<State>, EngineError> {
        self.shared.state_view()
    }
    pub fn snapshot(&self) -> Result<Arc<Snapshot>, EngineError> {
        Ok(self.state()?.index.snapshot().clone())
    }
    pub fn subscribe(&self) -> Result<LibrarySubscription, EngineError> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut subscribers = self.shared.subscribers.lock().map_err(|_| error(ErrorCode::Internal))?;
        if subscribers.len() >= 64 {
            return Err(error(ErrorCode::ResourceLimit));
        }
        let id = self.shared.next_subscription.fetch_add(1, Ordering::Relaxed);
        subscribers.insert(id, sender);
        Ok(LibrarySubscription {
            id,
            owner: Arc::downgrade(&self.shared),
            receiver,
        })
    }
    pub fn publish_library(
        &self,
        library: LibraryId,
        kind: LibraryKind,
        snapshot: Arc<Snapshot>,
    ) -> Result<(), EngineError> {
        self.shared.install(
            LibraryInfo::clean(library, kind, snapshot.objects().len()),
            &snapshot,
            None,
        )
    }
    /// Apply source health to the process outcome while preserving canonical data.
    pub fn finalize(&self, mut result: DispatchResult) -> Result<DispatchResult, EngineError> {
        result.source_findings = self
            .state()?
            .libraries
            .values()
            .any(|library| library.warnings_total > 0 || library.refresh_error.is_some());
        if result.source_findings && result.exit_code == ExitCode::Ok {
            result.exit_code = ExitCode::Findings;
        }
        result.bounded()
    }
    /// Whether Zotero's references here came from its closed database, the
    /// only read a profile choice affects.
    pub fn zotero_read_closed(&self) -> bool {
        self.resources.read_closed()
    }
    pub fn libraries(&self) -> Result<LibrariesResult, EngineError> {
        Ok(LibrariesResult {
            libraries: self.state()?.libraries.values().cloned().collect(),
            zotero_profiles: None,
            zotero_installed: None,
            zotero_groups: None,
        })
    }
    pub fn search(&self, request: &SearchRequest, cancel: &Cancellation) -> Result<SearchResult, EngineError> {
        cancel.check()?;
        if request.query.len() > 64 * 1024
            || request.limit > 1000
            || request.libraries.as_ref().is_some_and(|ids| ids.len() > 1000)
        {
            return Err(error(ErrorCode::BadInput));
        }
        let state = self.state()?;
        if request
            .libraries
            .as_ref()
            .is_some_and(|libraries| libraries.iter().any(|id| !state.libraries.contains_key(id)))
        {
            return Err(error(ErrorCode::NotFound));
        }
        let pasted = state
            .index
            .pasted_list(&request.query, request.libraries.as_deref(), &|| cancel.is_cancelled())
            .ok_or_else(|| error(ErrorCode::Cancelled))?;
        // Exact lists win before locator parsing or closest-match search.
        let locator = (request.interpret_locator && pasted.is_none())
            .then(|| ibis_core::locator::parse_trailing(&request.query))
            .flatten();
        let query = locator
            .as_ref()
            .map_or(request.query.as_str(), |read| read.search.as_str());
        let (hits, list) = match pasted {
            Some(list) => (list.hits, Some(PastedList { missing: list.missing })),
            None => (
                state
                    .index
                    .search(query, request.limit, request.libraries.as_deref(), &|| {
                        cancel.is_cancelled()
                    })
                    .ok_or_else(|| error(ErrorCode::Cancelled))?,
                None,
            ),
        };
        #[derive(Serialize)]
        struct HitView<'a> {
            item: &'a SnapshotObject,
            score: u32,
            matched: &'a [MatchField],
            typo: bool,
            partial: bool,
        }
        #[derive(Serialize)]
        struct SearchView<'a> {
            snapshot: &'a Fingerprints,
            hits: Vec<HitView<'a>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            list: Option<&'a PastedList>,
            #[serde(skip_serializing_if = "Option::is_none")]
            locator: Option<&'a ibis_core::locator::TrailingLocator>,
        }
        let view = SearchView {
            snapshot: state.index.snapshot().fingerprints(),
            list: list.as_ref(),
            locator: locator.as_ref(),
            hits: hits
                .iter()
                .map(|hit| HitView {
                    item: hit.item,
                    score: hit.score,
                    matched: &hit.matched,
                    typo: hit.typo,
                    partial: hit.partial,
                })
                .collect(),
        };
        ensure_budget(&view)?;
        drop(view);
        cancel.check()?;
        Ok(SearchResult {
            snapshot: state.index.snapshot().fingerprints().clone(),
            list,
            hits: hits
                .into_iter()
                .map(|hit| SearchHit {
                    item: hit.item.clone(),
                    score: hit.score,
                    matched: hit.matched,
                    typo: hit.typo,
                    partial: hit.partial,
                })
                .collect(),
            locator,
        })
    }
    pub fn show(&self, request: &ShowRequest) -> Result<SnapshotObject, EngineError> {
        let state = self.state()?;
        let selected = select(state.index.snapshot(), &state.libraries, &request.item)?;
        ensure_budget(selected)?;
        Ok(selected.clone())
    }
    pub fn cite_keys(
        &self,
        request: &KeyCitationRequest,
        cancel: &Cancellation,
    ) -> Result<CitationResult, EngineError> {
        self.cite(&self.key_citation_request(request, cancel)?, cancel)
    }
    fn key_citation_request(
        &self,
        request: &KeyCitationRequest,
        cancel: &Cancellation,
    ) -> Result<CitationRequest, EngineError> {
        cancel.check()?;
        if request.keys.is_empty() || request.keys.len() > ibis_core::syntax::MAX_CITATION_ITEMS {
            return Err(error(ErrorCode::BadInput));
        }
        let state = self.state()?;
        let snapshot = state.index.snapshot();
        let output = crate::outputs::resolve(&request.output)?;
        let items = request
            .keys
            .iter()
            .map(|key| {
                cancel.check()?;
                let selected = select(snapshot, &state.libraries, &ItemSelector::Key(key.key.clone()))?;
                Ok(CitationItem {
                    item: selected.object.id().clone(),
                    expected_cite: Some(selected.fingerprints.cite.clone()),
                    form: None,
                    locator: key.locator.clone(),
                    prefix: key.prefix.clone(),
                    suffix: key.suffix.clone(),
                    suppress_author: key.suppress_author,
                    annotation: None,
                    expected_annotation_cite: None,
                })
            })
            .collect::<Result<Vec<_>, EngineError>>()?;
        Ok(CitationRequest {
            items,
            keep_order: request.keep_order || matches!(output, OutputSpec::Key { .. }),
            output,
            form: request.form,
            input_language: None,
            output_locale: request.output_locale.clone(),
        })
    }
    pub fn cite(&self, request: &CitationRequest, cancel: &Cancellation) -> Result<CitationResult, EngineError> {
        self.prepare_citation(request, cancel)
            .map(PreparedCitation::into_result)
    }
    fn cite_snapshot(
        &self,
        snapshot: &Arc<Snapshot>,
        request: &CitationRequest,
        cancel: &Cancellation,
    ) -> Result<CitationResult, EngineError> {
        if request.items.is_empty() || request.items.len() > ibis_core::syntax::MAX_CITATION_ITEMS {
            return Err(error(ErrorCode::BadInput));
        }
        crate::wire::serialized_size(request, crate::protocol::MAX_REQUEST_BYTES)
            .map_err(|_| error(ErrorCode::ResourceLimit))?;
        let resolved;
        let request = match &request.output {
            OutputSpec::Preset { .. } => {
                resolved = CitationRequest {
                    output: crate::outputs::resolve(&request.output)?,
                    ..request.clone()
                };
                &resolved
            }
            _ => request,
        };
        let result = match &request.output {
            OutputSpec::Key { .. } => ibis_core::syntax::cite(snapshot, request).map_err(|failure| {
                error(if failure == CitationError::OutputTooLarge {
                    ErrorCode::ResourceLimit
                } else {
                    ErrorCode::BadInput
                })
            })?,
            OutputSpec::Csl { .. } => self.formatted(snapshot, request, cancel)?,
            OutputSpec::Preset { .. } => return Err(error(ErrorCode::Internal)),
        };
        cancel.check()?;
        ensure_budget(&result)?;
        Ok(result)
    }
    pub fn dispatch(
        &self,
        method: Method,
        params: &Value,
        cancel: &Cancellation,
    ) -> Result<DispatchResult, EngineError> {
        cancel.check()?;
        crate::protocol::validate_shape(params).map_err(|_| error(ErrorCode::BadInput))?;
        let data = match method {
            Method::Libraries => match if params.is_null() {
                LibrariesRequest::List
            } else {
                parameters::<LibrariesRequest>(params)?
            } {
                LibrariesRequest::List => value(self.libraries()?)?,
                LibrariesRequest::OpenZotero {
                    libraries,
                    profile,
                    reload,
                } => {
                    self.read_zotero(reload, libraries.as_deref(), profile.as_deref(), cancel)?;
                    value(self.libraries()?)?
                }
                LibrariesRequest::ZoteroProfiles => value(LibrariesResult {
                    zotero_profiles: Some(self.zotero_profiles().unwrap_or_default()),
                    zotero_installed: Some(ibis_sources::zotero_source::installed()),
                    ..self.libraries()?
                })?,
                LibrariesRequest::ZoteroGroups => value(LibrariesResult {
                    zotero_groups: Some(self.zotero_groups()?),
                    ..self.libraries()?
                })?,
                LibrariesRequest::OpenFile { path } => {
                    self.open_file(&path, cancel)?;
                    value(self.libraries()?)?
                }
                LibrariesRequest::Reload { library } => {
                    self.reload_files(library.as_ref(), cancel)?;
                    value(self.libraries()?)?
                }
            },
            Method::Search => value(self.search(&parameters(params)?, cancel)?)?,
            Method::Show => value(self.show(&parameters(params)?)?)?,
            Method::Cite => {
                let request: CitationRequest = parameters(params)?;
                let result = self.cite(&request, cancel)?;
                return self.finalize(DispatchResult::citation(result, request.items.len())?);
            }
            Method::Project => {
                let result = self.project(&parameters(params)?, cancel)?;
                let exit_code = if result.settled() {
                    ExitCode::Ok
                } else {
                    ExitCode::Findings
                };
                return self.finalize(DispatchResult {
                    data: value(result)?,
                    exit_code,
                    text: None,
                    source_findings: false,
                });
            }
            Method::Entry => {
                let result = self.entry(&parameters(params)?, cancel)?;
                let exit_code = match result {
                    EntryResult::Entry { .. } => ExitCode::Ok,
                    EntryResult::Refused { .. } => ExitCode::Findings,
                };
                return self.finalize(DispatchResult {
                    data: value(result)?,
                    exit_code,
                    text: None,
                    source_findings: false,
                });
            }
            Method::Styles => value(self.styles(&parameters(params)?)?)?,
            Method::Outputs => {
                let OutputsRequest {} = parameters(params)?;
                value(crate::outputs::list())?
            }
            Method::Schema => {
                let request: SchemaRequest = parameters(params)?;
                let methods: Vec<_> = request
                    .method
                    .map_or_else(|| Method::ALL.to_vec(), |method| vec![method]);
                Value::Array(methods.into_iter().map(|method|json!({"method":method,"params":request_schema(method),"result":response_schema(method)})).collect())
            }
            Method::Call => {
                let request: CallRequest = parameters(params)?;
                if request.method == Method::Call {
                    return Err(error(ErrorCode::BadInput));
                }
                return self.dispatch(request.method, &request.params, cancel);
            }
        };
        cancel.check()?;
        self.finalize(DispatchResult {
            data,
            exit_code: ExitCode::Ok,
            text: None,
            source_findings: false,
        })
    }
}
impl State {
    fn with_failure(&self, library: &LibraryId, failure: EngineError) -> Result<Self, EngineError> {
        let mut libraries = self.libraries.clone();
        let info = libraries.get_mut(library).ok_or_else(|| error(ErrorCode::NotFound))?;
        info.refresh_error = Some(failure.code);
        info.refresh_reason = failure.reason;
        Ok(Self {
            accepted: self.accepted.clone(),
            zotero_generation: self.zotero_generation,
            zotero_ready: self.zotero_ready,
            index: self.index.clone(),
            libraries,
        })
    }

    fn same_publication(&self, current: &Self) -> bool {
        Arc::ptr_eq(&self.accepted, &current.accepted)
            && self.zotero_generation == current.zotero_generation
            && self.zotero_ready == current.zotero_ready
    }

    /// Build immutable publication data off the native selection lock.
    fn replacement(
        old: &Self,
        entries: Vec<(LibraryInfo, &Snapshot)>,
        replaced: impl Fn(&LibraryInfo) -> bool,
    ) -> Result<Self, EngineError> {
        if entries.iter().any(|(info, snapshot)| {
            snapshot
                .objects()
                .iter()
                .any(|item| item.object.id().library != info.id)
        }) {
            return Err(error(ErrorCode::BadInput));
        }
        let replaced: std::collections::BTreeSet<_> = old
            .libraries
            .values()
            .filter(|info| replaced(info))
            .map(|info| &info.id)
            .chain(entries.iter().map(|(info, _)| &info.id))
            .collect();
        // Unchanged libraries keep their prepared objects; nothing is re-fingerprinted.
        let combined = old
            .index
            .snapshot()
            .merged_many(
                &entries.iter().map(|(_, snapshot)| *snapshot).collect::<Vec<_>>(),
                |id| !replaced.contains(id),
            )
            .map_err(|_| error(ErrorCode::BadInput))?;
        let combined = Arc::new(combined);
        let zotero_ready = old.zotero_ready || entries.iter().any(|(info, _)| info.kind == LibraryKind::Zotero);
        let mut libraries = old.libraries.clone();
        libraries.retain(|id, _| !replaced.contains(id));
        libraries.extend(entries.into_iter().map(|(info, _)| (info.id.clone(), info)));
        let index = if combined.fingerprints().full == old.index.snapshot().fingerprints().full {
            old.index.clone()
        } else {
            Arc::new(SnapshotIndex::updated(combined, &old.index))
        };
        let next = State {
            accepted: Arc::new(()),
            zotero_generation: old.zotero_generation,
            zotero_ready,
            index,
            libraries,
        };
        Ok(next)
    }
}

impl Shared {
    fn state_view(&self) -> Result<Arc<State>, EngineError> {
        self.state
            .read()
            .map(|state| state.clone())
            .map_err(|_| error(ErrorCode::Internal))
    }
    fn notify(&self) -> Result<(), EngineError> {
        let mut subscribers = self.subscribers.lock().map_err(|_| error(ErrorCode::Internal))?;
        subscribers.retain(|_, sender| match sender.try_send(LibraryChange) {
            // A pending wake already tells the subscriber to read current state.
            Ok(()) | Err(mpsc::TrySendError::Full(_)) => true,
            Err(mpsc::TrySendError::Disconnected(_)) => false,
        });
        Ok(())
    }

    fn install(&self, info: LibraryInfo, snapshot: &Snapshot, remove: Option<LibraryId>) -> Result<(), EngineError> {
        self.publish(vec![(info, snapshot)], |old| remove.as_ref() == Some(&old.id))
    }
    /// Replace `entries`' own libraries and every one `replaced` matches, in
    /// one publication.
    fn publish(
        &self,
        entries: Vec<(LibraryInfo, &Snapshot)>,
        replaced: impl Fn(&LibraryInfo) -> bool,
    ) -> Result<(), EngineError> {
        let _publication = self.publication.lock().map_err(|_| error(ErrorCode::Internal))?;
        let old = self.state_view()?;
        let next = Arc::new(State::replacement(&old, entries, replaced)?);
        *self.state.write().map_err(|_| error(ErrorCode::Internal))? = next;
        self.notify()
    }
    fn begin_zotero_read(&self) -> Result<(), EngineError> {
        let _publication = self.publication.lock().map_err(|_| error(ErrorCode::Internal))?;
        let old = self.state_view()?;
        let generation = old
            .zotero_generation
            .checked_add(1)
            .ok_or_else(|| error(ErrorCode::ResourceLimit))?;
        *self.state.write().map_err(|_| error(ErrorCode::Internal))? = Arc::new(State {
            accepted: old.accepted.clone(),
            zotero_generation: generation,
            zotero_ready: false,
            index: old.index.clone(),
            libraries: old.libraries.clone(),
        });
        Ok(())
    }
    fn failed(&self, library: &LibraryId, failure: impl Into<EngineError>) -> Result<(), EngineError> {
        let _publication = self.publication.lock().map_err(|_| error(ErrorCode::Internal))?;
        let old = self.state_view()?;
        *self.state.write().map_err(|_| error(ErrorCode::Internal))? =
            Arc::new(old.with_failure(library, failure.into())?);
        self.notify()
    }
}
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| u64::try_from(time.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn ensure_budget(value: &impl Serialize) -> Result<(), EngineError> {
    crate::wire::serialized_size(value, crate::protocol::MAX_RESULT_BYTES)
        .map(|_| ())
        .map_err(|_| error(ErrorCode::ResourceLimit))
}
fn select<'a>(
    snapshot: &'a Snapshot,
    libraries: &BTreeMap<LibraryId, LibraryInfo>,
    selector: &ItemSelector,
) -> Result<&'a SnapshotObject, EngineError> {
    match selector {
        ItemSelector::Id(id) => snapshot.get(id).ok_or_else(|| error(ErrorCode::NotFound)),
        ItemSelector::Key(key) => {
            if key.len() > 64 * 1024 || key.is_empty() {
                return Err(error(ErrorCode::BadInput));
            }
            let mut matches = snapshot.objects().iter().filter(|item| {
                let LibraryObject::Record(record) = &item.object else {
                    return false;
                };
                record.answers_to(key)
            });
            let found = matches.next().ok_or_else(|| {
                if libraries.values().any(|library| library.duplicate_keys.contains(key)) {
                    EngineError::because(ErrorCode::NotFound, ErrorReason::DuplicateKey)
                } else if libraries.values().any(|library| library.cut_short) {
                    EngineError::because(ErrorCode::NotFound, ErrorReason::LibraryCutShort)
                } else {
                    error(ErrorCode::NotFound)
                }
            })?;
            if matches.next().is_some() {
                return Err(EngineError::because(ErrorCode::BadInput, ErrorReason::AmbiguousKey));
            }
            Ok(found)
        }
    }
}
fn parameters<T: DeserializeOwned>(params: &Value) -> Result<T, EngineError> {
    let empty = json!({});
    T::deserialize(if params.is_null() { &empty } else { params }).map_err(|_| error(ErrorCode::BadInput))
}
fn value(value: impl Serialize) -> Result<Value, EngineError> {
    serde_json::to_value(value).map_err(|_| error(ErrorCode::Internal))
}
pub(crate) fn error(code: ErrorCode) -> EngineError {
    EngineError::new(code)
}
fn response_schema(method: Method) -> Value {
    match method {
        Method::Libraries => schema_for!(LibrariesResult),
        Method::Search => schema_for!(SearchResult),
        Method::Show => schema_for!(SnapshotObject),
        Method::Cite => schema_for!(CitationResult),
        Method::Project => schema_for!(ProjectResult),
        Method::Entry => schema_for!(EntryResult),
        Method::Styles => schema_for!(StylesResult),
        Method::Outputs => schema_for!(crate::outputs::OutputsResult),
        _ => schema_for!(Value),
    }
    .into()
}
