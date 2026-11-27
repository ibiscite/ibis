//! Typed services retain an immutable snapshot while refresh prepares its replacement.
use crate::sync::MutexExt;
use crate::{Method, request::*};
use ibis_core::{
    citable::{CitableFields, FieldProjection},
    contract::*,
    search::{MatchField, SnapshotIndex},
};
pub use ibis_csl::{
    LocaleEntry as LocaleInfo, Release as UpstreamRelease, StyleEntry as StyleInfo, StylePage as StylesResult,
};
pub use ibis_sources::FileLoads;
pub use ibis_sources::{LibraryKind, SourceCapability, SourceWarning, WarningCause, WarningCode};
use schemars::JsonSchema;
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
pub(crate) mod sources;
pub use sources::{PreparedFileReload, PreparedZoteroReload};

pub(crate) mod formatting;
pub use formatting::{Cancellation, Formatting};
mod prepared;
mod receipt;
pub use prepared::{CitationCommitError, PreparedCitation};
pub(crate) mod answers;
pub use answers::MethodSchema;
mod document;
mod project;
mod reading;
mod share;
pub use document::{
    CheckRequest, CheckResult, Checked as DocumentChecked, ExportRequest, ExportResult, Finding, FormatIssue,
    FormatIssueKind, FormatRefusal, FormatReport, FormatRequest, FormatResult, Problem as DocumentProblem,
    Read as DocumentRead, Which,
};
pub use ibis_sources::project::{Doubt, EntryRefusal, KeyCheck, ProjectProblem, ProjectRead};
pub use project::{
    EntryResult, ProjectAssessment, ProjectAssessmentState, ProjectKey, ProjectResult, WritingCitation, project_name,
};
pub use reading::*;
pub use share::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum WatchWarning {
    LiveRefreshOff,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LibraryInfo {
    pub id: LibraryId,
    pub kind: LibraryKind,
    /// What this library offers, as its reader declared.
    pub capabilities: Vec<SourceCapability>,
    /// Changes whenever the library's published content may have changed;
    /// a tool that mirrors the library rereads it only then.
    pub revision: Fingerprint,
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
    pub duplicate_keys: std::collections::BTreeMap<String, Vec<ibis_sources::SourceWarning>>,
    /// Each skipped entry whose header names its key, with why it was skipped.
    #[serde(skip)]
    #[schemars(skip)]
    pub damaged_keys: std::collections::BTreeMap<String, ibis_sources::SourceWarning>,
    /// The read stopped at a limit, so a key past it selects nothing.
    #[serde(skip)]
    #[schemars(skip)]
    pub cut_short: bool,
}
impl LibraryInfo {
    fn clean(id: LibraryId, kind: LibraryKind, capabilities: &[SourceCapability], revision: Fingerprint) -> Self {
        Self {
            id,
            kind,
            capabilities: capabilities.to_vec(),
            revision,
            objects: 0,
            warnings: vec![],
            warnings_total: 0,
            refresh_error: None,
            refresh_reason: None,
            updated_at_ms: now_ms(),
            watch_warning: None,
            zotero: None,
            duplicate_keys: Default::default(),
            damaged_keys: Default::default(),
            cut_short: false,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
/// Missing explicit citation keys in a recognized pasted list.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PastedList {
    pub missing: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchResult {
    pub snapshot: Fingerprints,
    pub hits: Vec<SearchHit>,
    /// True when every match is included; independent of library source health.
    pub complete: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<PastedList>,
    /// What was read around the query when asked for: a signal off its start
    /// (`see`) and a locator off its end (`417`), with what remains to search.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shorthand: Option<ibis_core::shorthand::Shorthand>,
    /// A signal or page could have been read, but the words as typed name
    /// something in the library (a title, key or number), so they were searched
    /// as words (maintainer, 2026-11-18). A comma asks for the page: `room, 22`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub kept_words: bool,
    /// The libraries searched, as they were when these hits were found.
    pub source_health: health::SourceHealth,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct DispatchResult<T = Value> {
    pub data: T,
    pub exit_code: ExitCode,
    /// Checked terminal content; record exports may report refused records beside it.
    pub text: Option<String>,
    pub source_findings: bool,
}
impl<T: Serialize> DispatchResult<T> {
    /// Every frontend shares the same serialized result budget.
    pub fn bounded(self) -> Result<Self, EngineError> {
        crate::wire::encode(&value(&self.data)?, crate::protocol::MAX_RESULT_BYTES)
            .map_err(|_| error(ErrorCode::ResourceLimit))?;
        Ok(self)
    }

    pub fn map_data<U>(self, map: impl FnOnce(T) -> U) -> DispatchResult<U> {
        DispatchResult {
            data: map(self.data),
            exit_code: self.exit_code,
            text: self.text,
            source_findings: self.source_findings,
        }
    }
    pub fn wire(self) -> Result<DispatchResult, EngineError> {
        let data = value(&self.data)?;
        Ok(self.map_data(|_| data))
    }
}

// Native callers retain the engine's result types until their output boundary.
/// One method's result, as `call` answers it.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Reply {
    Libraries(health::Checked<LibrariesResult>),
    Search(Box<SearchResult>),
    Show(Box<health::Checked<SnapshotObject>>),
    Citation(Box<health::Checked<CitationResult>>),
    Project(health::Checked<ProjectResult>),
    Entry(health::Checked<EntryResult>),
    Outline(health::Checked<OutlineResult>),
    Read(health::Checked<ReadResult>),
    FindIn(health::Checked<FindInResult>),
    Share(health::Checked<ShareResult>),
    Styles(StylesResult),
    Outputs(crate::outputs::OutputsResult),
    Schema(Vec<answers::MethodSchema>),
}
impl DispatchResult<health::Checked<CitationResult>> {
    pub fn citation(
        result: health::Checked<CitationResult>,
        expected_items: usize,
        output: &OutputSpec,
    ) -> Result<Self, EngineError> {
        let health::Checked {
            value: result,
            source_health,
        } = result;
        let source_findings = !source_health.is_ok();
        let receipt_findings = result.receipt.as_ref().is_some_and(receipt::source_findings);
        let text = result
            .copy_content(expected_items)
            .map(|content| content.text.to_owned())
            .or_else(|| {
                // Sharing can keep usable records while naming those refused.
                // A citation still requires the canonical completeness gate.
                (crate::outputs::named(output).is_some_and(|output| output.exports_records)
                    && result.outcomes.len() == expected_items
                    && result
                        .outcomes
                        .iter()
                        .enumerate()
                        .all(|(index, outcome)| match outcome {
                            ItemOutcome::Rendered { item } => *item == index,
                            ItemOutcome::Refused { item, error } => *item == index && *error != CitationError::Unknown,
                            ItemOutcome::Unknown => false,
                        })
                    && result
                        .outcomes
                        .iter()
                        .any(|outcome| matches!(outcome, ItemOutcome::Rendered { .. }))
                    && result.html.is_none()
                    && result.rtf.is_none()
                    && !result.text.trim().is_empty()
                    && !result.text.contains('\0')
                    && result.content_matches_hash())
                .then(|| result.text.clone())
            });
        let exit_code = if result.is_complete()
            && result.losses.is_empty()
            && text.is_some()
            && !source_findings
            && !receipt_findings
        {
            ExitCode::Ok
        } else {
            ExitCode::Findings
        };
        Self {
            data: health::Checked {
                value: result,
                source_health,
            },
            exit_code,
            text,
            source_findings,
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
        if let Some(owner) = self.owner.upgrade() {
            owner.subscribers.recover_lock().remove(&self.id);
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
        if let Some(owner) = self.owner.upgrade() {
            owner.subscribers.recover_lock().remove(&self.id);
        }
    }
}
#[derive(Clone)]
struct State {
    /// Accepted data/diagnostic identity; read errors alone do not replace it.
    accepted: Arc<()>,
    zotero_generation: u64,
    zotero_ready: bool,
    index: Arc<SnapshotIndex>,
    libraries: BTreeMap<LibraryId, LibraryInfo>,
}
pub(crate) struct Shared {
    pub(crate) citations: crate::citations::Registry,
    state: RwLock<Arc<State>>,
    /// Serializes publications without blocking readers during index building.
    publication: Mutex<()>,
    subscribers: Mutex<BTreeMap<u64, mpsc::SyncSender<LibraryChange>>>,
    next_subscription: AtomicU64,
    sources: Mutex<sources::Registry>,
}
impl Shared {
    fn new(state: Arc<State>) -> Self {
        Self {
            citations: crate::citations::Registry::default(),
            state: RwLock::new(state),
            publication: Mutex::new(()),
            subscribers: Mutex::new(BTreeMap::new()),
            next_subscription: AtomicU64::new(0),
            sources: Mutex::new(sources::Registry::default()),
        }
    }
}
#[derive(Clone)]
pub struct Engine {
    pub(crate) shared: Arc<Shared>,
    formatting: Formatting,
    resources: Arc<sources::Owners>,
    file_loads: Option<Arc<FileLoads>>,
    watch_files: bool,
    /// Reads PDFs in a child process; without one, PDFs stay unread.
    pdf: Option<ibis_sources::pdf::PdfReader>,
    documents: Option<ibis_sources::document::DocumentReader>,
    /// The `ibis` executable that serves built-in source adapters.
    built_in_sources: Option<std::path::PathBuf>,
    external_sources: Arc<BTreeMap<String, std::path::PathBuf>>,
    source_directory: Option<std::path::PathBuf>,
    attachment_text_outside_library: bool,
}
impl Engine {
    /// Connections with different selections can reuse the same immutable file records.
    pub fn with_file_loads(mut self, loads: Arc<FileLoads>) -> Self {
        self.file_loads = Some(loads);
        self
    }

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
    /// Read PDF attachments with `program`, an `ibis` executable started as
    /// a short-lived child for each file (decision 0055). The desktop app
    /// never parses a PDF itself.
    pub fn with_pdf_reader(mut self, program: std::path::PathBuf) -> Self {
        self.pdf = Some(ibis_sources::pdf::PdfReader::new(program));
        self
    }
    /// The PDF reader this engine was given, if any.
    pub(crate) fn pdf_reader(&self) -> Option<&ibis_sources::pdf::PdfReader> {
        self.pdf.as_ref()
    }
    /// Read documents (manuscripts for `check` and `export`) with `program`,
    /// an `ibis` executable started as `ibis __document` for each file
    /// (decision 0060). The engine never parses a document itself.
    pub fn with_document_reader(mut self, program: std::path::PathBuf) -> Self {
        self.documents = Some(ibis_sources::document::DocumentReader::new(program));
        self
    }
    /// Read documents in this process: tests only.
    #[cfg(test)]
    pub(crate) fn with_documents_in_process(mut self) -> Self {
        self.documents = Some(ibis_sources::document::DocumentReader::in_process());
        self
    }
    /// Serve built-in source adapters with `program`, an `ibis` executable
    /// started as `ibis __source NAME` for each read (decision 0056).
    pub fn with_built_in_sources(mut self, program: std::path::PathBuf) -> Self {
        self.built_in_sources = Some(program);
        self
    }
    /// Approve external adapters by name for the lifetime of this engine.
    pub fn with_external_sources(mut self, sources: BTreeMap<String, std::path::PathBuf>) -> Self {
        self.external_sources = Arc::new(sources);
        self
    }
    /// Run source adapters in the caller's working directory.
    pub fn with_source_directory(mut self, directory: Option<std::path::PathBuf>) -> Self {
        self.source_directory = directory;
        self
    }
    /// Explicit device permission for the attachment text tools to read
    /// beyond a library file's folder or Zotero's storage folder.
    pub fn with_attachment_text_outside_library(mut self, allowed: bool) -> Self {
        self.attachment_text_outside_library = allowed;
        self
    }
    /// Reuse a session's compiled style when replacing its library.
    pub fn with_formatting(mut self, formatting: Formatting) -> Self {
        self.formatting = formatting;
        self
    }
    pub fn from_snapshot(snapshot: Arc<Snapshot>) -> Self {
        let mut libraries = BTreeMap::<LibraryId, LibraryInfo>::new();
        for item in snapshot.objects() {
            let id = item.object.id().library.clone();
            libraries
                .entry(id.clone())
                .or_insert_with(|| {
                    // One snapshot holds every library here, so each takes its revision.
                    let revision = snapshot.fingerprints().full.clone();
                    LibraryInfo::clean(id, LibraryKind::Snapshot, SourceCapability::SNAPSHOT, revision)
                })
                .objects += 1;
        }
        Self {
            formatting: Formatting::default(),
            shared: Arc::new(Shared::new(Arc::new(State {
                accepted: Arc::new(()),
                zotero_generation: 0,
                zotero_ready: true,
                index: Arc::new(SnapshotIndex::new(snapshot)),
                libraries,
            }))),
            resources: Arc::new(sources::Owners::default()),
            file_loads: None,
            watch_files: true,
            pdf: None,
            documents: None,
            built_in_sources: None,
            external_sources: Arc::default(),
            source_directory: None,
            attachment_text_outside_library: false,
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
        let mut subscribers = self.shared.subscribers.recover_lock();
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
        capabilities: &[SourceCapability],
        snapshot: Arc<Snapshot>,
    ) -> Result<(), EngineError> {
        let mut info = LibraryInfo::clean(library, kind, capabilities, snapshot.fingerprints().full.clone());
        info.objects = snapshot.objects().len();
        self.shared.install(info, &snapshot, None)
    }

    /// Whether Zotero's references here came from its closed database, the
    /// only read a profile choice affects.
    pub fn zotero_read_closed(&self) -> bool {
        self.resources.read_closed()
    }
    pub fn libraries(&self) -> Result<LibrariesResult, EngineError> {
        Ok(self.libraries_checked()?.value)
    }
    /// The libraries, with the health of every one of them.
    pub fn libraries_checked(&self) -> Result<health::Checked<LibrariesResult>, EngineError> {
        let state = self.state()?;
        Ok(health::Checked {
            source_health: health::SourceHealth::of(state.libraries.values()),
            value: LibrariesResult {
                libraries: state.libraries.values().cloned().collect(),
                zotero_profiles: None,
                zotero_installed: None,
                zotero_groups: None,
            },
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
        let source_health = health::SourceHealth::of(state.libraries.values().filter(|library| {
            request
                .libraries
                .as_ref()
                .is_none_or(|scope| scope.contains(&library.id))
        }));
        let pasted = state
            .index
            .pasted_list(&request.query, request.libraries.as_deref(), &|| cancel.is_cancelled())
            .ok_or_else(|| error(ErrorCode::Cancelled))?;
        // Exact lists win before reading what is typed around the search
        // (`see vale 417`) or closest-match search.
        let read = (request.interpret_locator && pasted.is_none())
            .then(|| ibis_core::shorthand::read(&request.query))
            .flatten();
        // The words win when, as typed, they match the library whole: a title's
        // number never silently becomes a page, nor its opening "See" a signal.
        let asked_for_page = read
            .as_ref()
            .and_then(|read| read.locator.as_ref())
            .is_some_and(|locator| {
                request
                    .query
                    .trim_end()
                    .strip_suffix(locator.typed.as_str())
                    .is_some_and(|before| before.trim_end().ends_with(','))
            });
        let kept_words = read.is_some()
            && !asked_for_page
            && state
                .index
                .search(&request.query, 1, request.libraries.as_deref(), &|| {
                    cancel.is_cancelled()
                })
                .ok_or_else(|| error(ErrorCode::Cancelled))?
                .iter()
                .any(|hit| !hit.partial && !hit.typo);
        let shorthand = read.filter(|_| !kept_words);
        let query = shorthand
            .as_ref()
            .map_or(request.query.as_str(), |read| read.search.as_str());
        let (mut hits, list) = match pasted {
            Some(list) => (list.hits, Some(PastedList { missing: list.missing })),
            None => (
                state
                    .index
                    .search(query, request.limit + 1, request.libraries.as_deref(), &|| {
                        cancel.is_cancelled()
                    })
                    .ok_or_else(|| error(ErrorCode::Cancelled))?,
                None,
            ),
        };
        // Pasted lists deliberately return the whole exact selection. Ordinary
        // searches probe one extra candidate, then expose only the requested page.
        let complete = list.is_some() || hits.len() <= request.limit;
        if list.is_none() {
            hits.truncate(request.limit);
        }
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
            complete: bool,
            snapshot: &'a Fingerprints,
            hits: Vec<HitView<'a>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            list: Option<&'a PastedList>,
            #[serde(skip_serializing_if = "Option::is_none")]
            shorthand: Option<&'a ibis_core::shorthand::Shorthand>,
            source_health: &'a health::SourceHealth,
        }
        let view = SearchView {
            complete,
            source_health: &source_health,
            snapshot: state.index.snapshot().fingerprints(),
            list: list.as_ref(),
            shorthand: shorthand.as_ref(),
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
            complete,
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
            shorthand,
            kept_words,
            source_health,
        })
    }
    pub fn show(&self, request: &ShowRequest) -> Result<SnapshotObject, EngineError> {
        Ok(self.show_checked(request)?.value)
    }
    /// The object, with the health of the libraries the lookup read.
    pub fn show_checked(&self, request: &ShowRequest) -> Result<health::Checked<SnapshotObject>, EngineError> {
        let state = self.state()?;
        let selected = select(state.index.snapshot(), &state.libraries, &request.item)?;
        ensure_budget(selected)?;
        Ok(health::Checked {
            source_health: consulted(&state, &request.item, &selected.object.id().library),
            value: selected.clone(),
        })
    }
    pub fn cite_keys(
        &self,
        request: &KeyCitationRequest,
        cancel: &Cancellation,
    ) -> Result<CitationResult, EngineError> {
        Ok(self.cite_keys_checked(request, cancel)?.value)
    }
    /// The citation, with the health of every library: finding a key reads them all.
    pub fn cite_keys_checked(
        &self,
        request: &KeyCitationRequest,
        cancel: &Cancellation,
    ) -> Result<health::Checked<CitationResult>, EngineError> {
        Ok(self
            .prepare_consulting(&self.key_citation_request(request, cancel)?, true, cancel)?
            .into_checked())
    }
    fn key_citation_request(
        &self,
        request: &KeyCitationRequest,
        cancel: &Cancellation,
    ) -> Result<CitationRequest, EngineError> {
        cancel.check()?;
        if request.keys.is_empty() {
            return Err(error(ErrorCode::BadInput));
        }
        ibis_core::citation::check_item_limit(request.keys.len())
            .map_err(|_| EngineError::citation(CitationError::TooManyKeys))?;
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
            strict: request.strict,
            items,
            keep_order: request.keep_order || matches!(output, OutputSpec::Key { .. }),
            output,
            form: request.form,
            input_language: None,
            output_locale: request.output_locale.clone(),
        })
    }
    /// Cite or export records sent with the request (#1539). No library is
    /// opened or changed: the records live only for this call.
    pub fn cite_records(&self, request: &RecordsRequest, cancel: &Cancellation) -> Result<CitationResult, EngineError> {
        self.cite_records_within(request, MAX_RECORDS_BYTES, cancel)
    }
    /// [`Self::cite_records`] with records of up to `max_bytes` in all: a
    /// reference set read from a file is bounded as a file, not a request.
    pub(crate) fn cite_records_within(
        &self,
        request: &RecordsRequest,
        max_bytes: usize,
        cancel: &Cancellation,
    ) -> Result<CitationResult, EngineError> {
        cancel.check()?;
        if request.records.is_empty() {
            return Err(error(ErrorCode::BadInput));
        }
        // Records are far shallower than any transport limit allows, so the
        // same records meet the same limits however they arrive.
        ibis_core::citation::check_item_limit(request.records.len())
            .map_err(|_| EngineError::citation(CitationError::TooManyRecords))?;
        if request.records.iter().any(|record| depth(record) > MAX_RECORD_DEPTH)
            || crate::wire::serialized_size(&request.records, max_bytes).is_err()
        {
            return Err(error(ErrorCode::ResourceLimit));
        }
        let library = LibraryId::digest(b"records sent with a request");
        let read = ibis_sources::request_records(&library, &request.records)
            .map_err(|_| EngineError::because(ErrorCode::BadInput, ErrorReason::RejectedRecord))?;
        if read
            .warnings
            .iter()
            .any(|warning| warning.code == ibis_sources::WarningCode::DuplicateKey)
        {
            return Err(EngineError::because(ErrorCode::BadInput, ErrorReason::DuplicateRecord));
        }
        // Clean output never leaves a record out without saying so.
        if read.records.len() != request.records.len() {
            return Err(EngineError::because(ErrorCode::BadInput, ErrorReason::RejectedRecord));
        }
        cancel.check()?;
        let item = |id: &str| ItemRef {
            library: library.clone(),
            key: format!("record:{id}"),
        };
        let picks: Vec<RecordItem> = match &request.items {
            Some(items) => items.clone(),
            None => read
                .records
                .iter()
                .map(|record| RecordItem {
                    // Exactly one prefix: an id may itself start with `record:`.
                    id: record
                        .id
                        .key
                        .strip_prefix("record:")
                        .unwrap_or(&record.id.key)
                        .to_owned(),
                    ..RecordItem::default()
                })
                .collect(),
        };
        if picks.is_empty() {
            return Err(error(ErrorCode::BadInput));
        }
        ibis_core::citation::check_item_limit(picks.len())
            .map_err(|_| EngineError::citation(CitationError::TooManyRecordItems))?;
        let snapshot = Arc::new(
            Snapshot::new(read.records.into_iter().map(LibraryObject::Record).collect())
                .map_err(|_| error(ErrorCode::BadInput))?,
        );
        let items = picks
            .into_iter()
            .map(|pick| {
                let id = item(&pick.id);
                let expected_cite = snapshot
                    .get(&id)
                    .ok_or_else(|| error(ErrorCode::NotFound))?
                    .fingerprints
                    .cite
                    .clone();
                Ok(CitationItem {
                    item: id,
                    expected_cite: Some(expected_cite),
                    form: None,
                    locator: pick.locator,
                    prefix: pick.prefix,
                    suffix: pick.suffix,
                    suppress_author: pick.suppress_author,
                    annotation: None,
                    expected_annotation_cite: None,
                })
            })
            .collect::<Result<Vec<_>, EngineError>>()?;
        let output = crate::outputs::resolve(&request.output)?;
        let citation = CitationRequest {
            strict: request.strict,
            items,
            keep_order: matches!(output, OutputSpec::Key { .. }),
            output,
            form: request.form,
            input_language: None,
            output_locale: request.output_locale.clone(),
        };
        self.cite_snapshot(
            &receipt::CitationContext {
                source_failure: None,
                snapshot,
                sources: Vec::new(),
                zotero: BTreeMap::new(),
            },
            &citation,
            cancel,
        )
    }
    /// Why records sent with a request would not all be cited: each warning's
    /// `line` is a record's place in the list, from one. Nothing is opened.
    pub fn record_problems(records: &[Value]) -> Vec<SourceWarning> {
        ibis_sources::request_records(&LibraryId::digest(b"records sent with a request"), records).map_or_else(
            |_| vec![SourceWarning::at(ibis_sources::WarningCode::Syntax, None)],
            |read| read.warnings,
        )
    }
    pub fn cite(&self, request: &CitationRequest, cancel: &Cancellation) -> Result<CitationResult, EngineError> {
        self.prepare_citation(request, cancel)
            .map(PreparedCitation::into_result)
    }
    fn cite_snapshot(
        &self,
        context: &receipt::CitationContext,
        request: &CitationRequest,
        cancel: &Cancellation,
    ) -> Result<CitationResult, EngineError> {
        let resolved = crate::outputs::resolve_request(request)?;
        self.cite_resolved(context, &resolved, CitableFields::new, cancel)
    }
    fn cite_resolved(
        &self,
        context: &receipt::CitationContext,
        resolved: &crate::outputs::ResolvedRequest,
        project_fields: FieldProjection,
        cancel: &Cancellation,
    ) -> Result<CitationResult, EngineError> {
        let request = &resolved.request;
        let snapshot = &context.snapshot;
        crate::wire::serialized_size(request, crate::protocol::MAX_REQUEST_BYTES)
            .map_err(|_| error(ErrorCode::ResourceLimit))?;
        let key_error = EngineError::citation;
        let mut result = match &request.output {
            OutputSpec::Key {
                syntax: KeySyntax::ScannableCite,
                ..
            } => {
                // Which Zotero library each reference came from is the engine's
                // own record of what it read; an id names one library for good.
                ibis_core::syntax::scannable::cite(snapshot, request, |library| context.zotero.get(library).copied())
                    .map_err(key_error)?
            }
            OutputSpec::Key { .. } => {
                ibis_core::syntax::cite_with_fields(snapshot, request, project_fields).map_err(key_error)?
            }
            OutputSpec::Csl { .. } => self.formatted(
                snapshot,
                request,
                cancel,
                resolved.formatted.as_ref().ok_or_else(|| error(ErrorCode::Internal))?,
                project_fields,
            )?,
            OutputSpec::Preset { .. } => return Err(error(ErrorCode::Internal)),
        };
        cancel.check()?;
        receipt::attach(&mut result, request, context)?;
        ensure_budget(&result)?;
        Ok(result)
    }
    pub fn dispatch(
        &self,
        method: Method,
        params: &Value,
        cancel: &Cancellation,
    ) -> Result<DispatchResult, EngineError> {
        self.reply(method, params, cancel)?.wire()
    }

    pub fn reply(
        &self,
        method: Method,
        params: &Value,
        cancel: &Cancellation,
    ) -> Result<DispatchResult<Reply>, EngineError> {
        cancel.check()?;
        crate::protocol::validate_shape(params).map_err(|_| error(ErrorCode::BadInput))?;
        (method.row().route)(self, params, cancel)
    }

    /// Answers one method with typed values: what dispatch does once it has
    /// read the name and the parameters.
    pub fn call<M: crate::method::Row>(
        &self,
        params: M::Params,
        cancel: &Cancellation,
    ) -> Result<DispatchResult<M::Result>, EngineError> {
        cancel.check()?;
        M::answer(self, params, cancel)
    }
}
/// The libraries a lookup read: every one for a key, the record's own for an id.
fn consulted(state: &State, selector: &ItemSelector, library: &LibraryId) -> health::SourceHealth {
    health::SourceHealth::of(
        state
            .libraries
            .values()
            .filter(|info| matches!(selector, ItemSelector::Key(_)) || &info.id == library),
    )
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
        let mut subscribers = self.subscribers.recover_lock();
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
        let _publication = self.publication.recover_lock();
        let old = self.state_view()?;
        let next = Arc::new(State::replacement(&old, entries, replaced)?);
        *self.state.write().map_err(|_| error(ErrorCode::Internal))? = next;
        self.notify()
    }
    fn begin_zotero_read(&self) -> Result<(), EngineError> {
        let _publication = self.publication.recover_lock();
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
        let _publication = self.publication.recover_lock();
        let old = self.state_view()?;
        *self.state.write().map_err(|_| error(ErrorCode::Internal))? =
            Arc::new(old.with_failure(library, failure.into())?);
        self.notify()
    }
}
/// The deepest a record sent with a request may nest: CSL names and dates
/// need four levels.
const MAX_RECORD_DEPTH: usize = 16;
/// The most JSON the records of one request may take.
const MAX_RECORDS_BYTES: usize = 512 * 1024;
fn depth(value: &Value) -> usize {
    let mut deepest = 0;
    let mut pending = vec![(value, 1)];
    while let Some((value, level)) = pending.pop() {
        deepest = deepest.max(level);
        match value {
            Value::Array(values) => pending.extend(values.iter().map(|value| (value, level + 1))),
            Value::Object(values) => pending.extend(values.values().map(|value| (value, level + 1))),
            _ => {}
        }
    }
    deepest
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
                if libraries
                    .values()
                    .any(|library| library.duplicate_keys.contains_key(key))
                {
                    EngineError::because(ErrorCode::NotFound, ErrorReason::DuplicateKey)
                } else if libraries.values().any(|library| library.damaged_keys.contains_key(key)) {
                    EngineError::because(ErrorCode::NotFound, ErrorReason::DamagedEntry)
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
pub(crate) fn parameters<T: DeserializeOwned>(params: &Value) -> Result<T, EngineError> {
    let empty = json!({});
    T::deserialize(if params.is_null() { &empty } else { params }).map_err(|_| error(ErrorCode::BadInput))
}
fn value(value: impl Serialize) -> Result<Value, EngineError> {
    serde_json::to_value(value).map_err(|_| error(ErrorCode::Internal))
}
pub(crate) fn error(code: ErrorCode) -> EngineError {
    EngineError::new(code)
}
