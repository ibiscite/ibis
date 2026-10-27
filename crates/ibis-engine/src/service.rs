//! Typed services retain an immutable snapshot while refresh prepares its replacement.
use crate::{Method, request::*};
use ibis_core::{
    contract::*,
    search::{MatchField, SnapshotIndex},
};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    collections::BTreeMap,
    sync::{
        Arc, Mutex, RwLock, Weak,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
};

mod sources;

mod formatting;
pub use formatting::Cancellation;
mod prepared;
pub use prepared::{CitationCommitError, PreparedCitation};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum WatchWarning {
    LiveRefreshOff,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LibraryInfo {
    pub id: LibraryId,
    pub objects: usize,
    pub warnings: Vec<ibis_sources::SourceWarning>,
    pub warnings_total: usize,
    pub refresh_error: Option<ErrorCode>,
    pub updated_at_ms: u64,
    /// A readable source remains usable when automatic updates are unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watch_warning: Option<WatchWarning>,
}
impl LibraryInfo {
    fn clean(id: LibraryId, objects: usize) -> Self {
        Self {
            id,
            objects,
            warnings: vec![],
            warnings_total: 0,
            refresh_error: None,
            updated_at_ms: now_ms(),
            watch_warning: None,
        }
    }
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct LibrariesResult {
    pub libraries: Vec<LibraryInfo>,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SearchHit {
    pub item: SnapshotObject,
    pub score: u32,
    pub matched: Vec<MatchField>,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SearchResult {
    pub snapshot: Fingerprints,
    pub hits: Vec<SearchHit>,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct StylesResult {
    pub styles: Vec<StyleInfo>,
}
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct StyleInfo {
    pub id: String,
    pub title: String,
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

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct LibraryChange {
    pub sequence: u64,
    pub libraries: Vec<LibraryId>,
    pub snapshot: Fingerprints,
    /// Fetch every library when intermediate invalidations were coalesced.
    pub resync: bool,
}
pub struct LibrarySubscription {
    id: u64,
    owner: Weak<Shared>,
    receiver: mpsc::Receiver<LibraryChange>,
    last_sequence: Cell<u64>,
}
impl LibrarySubscription {
    pub fn recv_timeout(&self, timeout: std::time::Duration) -> Result<LibraryChange, mpsc::RecvTimeoutError> {
        let began = std::time::Instant::now();
        let mut remaining = timeout;
        loop {
            let event = self.receiver.recv_timeout(remaining)?;
            if let Some(event) = self.coalesce(event) {
                return Ok(event);
            }
            remaining = timeout.saturating_sub(began.elapsed());
            if remaining.is_zero() {
                return Err(mpsc::RecvTimeoutError::Timeout);
            }
        }
    }
    pub fn try_recv(&self) -> Result<LibraryChange, mpsc::TryRecvError> {
        loop {
            let event = self.receiver.try_recv()?;
            if let Some(event) = self.coalesce(event) {
                return Ok(event);
            }
        }
    }
    fn coalesce(&self, mut event: LibraryChange) -> Option<LibraryChange> {
        while let Ok(next) = self.receiver.try_recv() {
            event = next;
        }
        if let Some(latest) = self
            .owner
            .upgrade()
            .and_then(|owner| owner.latest_change.lock().ok().and_then(|latest| latest.clone()))
        {
            event = latest;
        }
        let previous = self.last_sequence.get();
        if event.sequence <= previous {
            return None;
        }
        event.resync = event.sequence != previous.saturating_add(1);
        if event.resync {
            event.libraries.clear();
        }
        self.last_sequence.set(event.sequence);
        Some(event)
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
    publication_sequence: AtomicU64,
    latest_change: Mutex<Option<LibraryChange>>,
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
                .or_insert_with(|| LibraryInfo::clean(id, 0))
                .objects += 1;
        }
        Self {
            shared: Arc::new(Shared {
                formatter: Mutex::new(None),
                state: RwLock::new(Arc::new(State {
                    index: Arc::new(SnapshotIndex::new(snapshot)),
                    libraries,
                })),
                publication: Mutex::new(()),
                subscribers: Mutex::new(BTreeMap::new()),
                next_subscription: AtomicU64::new(0),
                publication_sequence: AtomicU64::new(0),
                latest_change: Mutex::new(None),
                sources: Mutex::new(sources::Registry::default()),
            }),
            resources: Arc::new(sources::Owners::default()),
            watch_files: true,
        }
    }
    /// An isolated IPv4-loopback transport for synthetic clients.
    pub fn with_zotero_port(port: u16) -> Result<Self, EngineError> {
        let mut engine = Self::new()?;
        engine.resources = Arc::new(sources::Owners::on_port(port));
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
            last_sequence: Cell::new(self.shared.publication_sequence.load(Ordering::Acquire)),
        })
    }
    pub fn publish_library(&self, library: LibraryId, snapshot: Arc<Snapshot>) -> Result<(), EngineError> {
        self.shared
            .install(LibraryInfo::clean(library, snapshot.objects().len()), &snapshot, None)
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
    pub fn libraries(&self) -> Result<LibrariesResult, EngineError> {
        Ok(LibrariesResult {
            libraries: self.state()?.libraries.values().cloned().collect(),
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
        let hits = state
            .index
            .search(&request.query, request.limit, request.libraries.as_deref(), &|| {
                cancel.is_cancelled()
            })
            .ok_or_else(|| error(ErrorCode::Cancelled))?;
        #[derive(Serialize)]
        struct HitView<'a> {
            item: &'a SnapshotObject,
            score: u32,
            matched: &'a [MatchField],
        }
        #[derive(Serialize)]
        struct SearchView<'a> {
            snapshot: &'a Fingerprints,
            hits: Vec<HitView<'a>>,
        }
        let view = SearchView {
            snapshot: state.index.snapshot().fingerprints(),
            hits: hits
                .iter()
                .map(|hit| HitView {
                    item: hit.item,
                    score: hit.score,
                    matched: &hit.matched,
                })
                .collect(),
        };
        ensure_budget(&view)?;
        drop(view);
        cancel.check()?;
        Ok(SearchResult {
            snapshot: state.index.snapshot().fingerprints().clone(),
            hits: hits
                .into_iter()
                .map(|hit| SearchHit {
                    item: public_object(hit.item),
                    score: hit.score,
                    matched: hit.matched,
                })
                .collect(),
        })
    }
    pub fn show(&self, request: &ShowRequest) -> Result<SnapshotObject, EngineError> {
        let state = self.state()?;
        let selected = select(state.index.snapshot(), &request.item)?;
        ensure_budget(selected)?;
        Ok(public_object(selected))
    }
    pub fn cite_keys(
        &self,
        request: &KeyCitationRequest,
        cancel: &Cancellation,
    ) -> Result<CitationResult, EngineError> {
        cancel.check()?;
        if request.keys.is_empty() || request.keys.len() > ibis_core::syntax::MAX_CITATION_ITEMS {
            return Err(error(ErrorCode::BadInput));
        }
        let snapshot = self.snapshot()?;
        let items = request
            .keys
            .iter()
            .map(|key| {
                cancel.check()?;
                let selected = select(&snapshot, &ItemSelector::Key(key.clone()))?;
                Ok(CitationItem {
                    item: selected.object.id().clone(),
                    expected_cite: Some(selected.fingerprints.cite.clone()),
                    form: None,
                    locator: None,
                    prefix: vec![],
                    suffix: vec![],
                    suppress_author: false,
                    annotation: None,
                    expected_annotation_cite: None,
                })
            })
            .collect::<Result<Vec<_>, EngineError>>()?;
        self.cite_snapshot(
            &snapshot,
            &CitationRequest {
                items,
                output: request.output.clone(),
                form: request.form,
                keep_order: matches!(request.output, OutputSpec::Key { .. }),
                input_language: None,
                output_locale: request.output_locale.clone(),
            },
            cancel,
        )
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
        let result = match &request.output {
            OutputSpec::Key { .. } => ibis_core::syntax::cite(snapshot, request).map_err(|failure| {
                error(if failure == CitationError::OutputTooLarge {
                    ErrorCode::ResourceLimit
                } else {
                    ErrorCode::BadInput
                })
            })?,
            OutputSpec::Csl { .. } => self.formatted(snapshot, request, cancel)?,
            OutputSpec::Preset { .. } => formatting::refusal(request, CitationError::UnsupportedOutput),
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
                LibrariesRequest::OpenZotero => {
                    self.read_zotero(false, cancel)?;
                    value(self.libraries()?)?
                }
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
            Method::Styles => {
                let _: StylesRequest = parameters(params)?;
                value(self.styles()?)?
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
impl Shared {
    fn state_view(&self) -> Result<Arc<State>, EngineError> {
        self.state
            .read()
            .map(|state| state.clone())
            .map_err(|_| error(ErrorCode::Internal))
    }
    fn notify(&self, libraries: Vec<LibraryId>, snapshot: Fingerprints) -> Result<(), EngineError> {
        let event = LibraryChange {
            sequence: self.publication_sequence.fetch_add(1, Ordering::Relaxed) + 1,
            libraries,
            snapshot,
            resync: false,
        };
        *self.latest_change.lock().map_err(|_| error(ErrorCode::Internal))? = Some(event.clone());
        let mut subscribers = self.subscribers.lock().map_err(|_| error(ErrorCode::Internal))?;
        subscribers.retain(|_, sender| match sender.try_send(event.clone()) {
            // A queued wake pulls latest_change; gaps explicitly require full resync.
            Ok(()) | Err(mpsc::TrySendError::Full(_)) => true,
            Err(mpsc::TrySendError::Disconnected(_)) => false,
        });
        Ok(())
    }

    fn install(&self, info: LibraryInfo, snapshot: &Snapshot, remove: Option<LibraryId>) -> Result<(), EngineError> {
        let library = info.id.clone();
        if snapshot
            .objects()
            .iter()
            .any(|item| item.object.id().library != library)
        {
            return Err(error(ErrorCode::BadInput));
        }
        let _publication = self.publication.lock().map_err(|_| error(ErrorCode::Internal))?;
        let old = self.state_view()?;
        let mut objects: Vec<_> = old
            .index
            .snapshot()
            .objects()
            .iter()
            .filter(|item| {
                item.object.id().library != library && remove.as_ref().is_none_or(|id| item.object.id().library != *id)
            })
            .map(|item| item.object.clone())
            .collect();
        objects.extend(snapshot.objects().iter().map(|item| item.object.clone()));
        let combined = Arc::new(Snapshot::new(objects).map_err(|_| error(ErrorCode::BadInput))?);
        let mut libraries = old.libraries.clone();
        if let Some(remove) = &remove {
            libraries.remove(remove);
        }
        libraries.insert(library.clone(), info);
        let next = Arc::new(State {
            index: Arc::new(SnapshotIndex::new(combined)),
            libraries,
        });
        let fingerprints = next.index.snapshot().fingerprints().clone();
        *self.state.write().map_err(|_| error(ErrorCode::Internal))? = next;
        let mut changed = vec![library];
        if let Some(remove) = remove
            && !changed.contains(&remove)
        {
            changed.push(remove);
        }
        self.notify(changed, fingerprints)
    }
    fn failed(&self, library: &LibraryId, code: ErrorCode) -> Result<(), EngineError> {
        let _publication = self.publication.lock().map_err(|_| error(ErrorCode::Internal))?;
        let old = self.state_view()?;
        let mut libraries = old.libraries.clone();
        libraries
            .get_mut(library)
            .ok_or_else(|| error(ErrorCode::NotFound))?
            .refresh_error = Some(code);
        let fingerprints = old.index.snapshot().fingerprints().clone();
        *self.state.write().map_err(|_| error(ErrorCode::Internal))? = Arc::new(State {
            index: old.index.clone(),
            libraries,
        });
        self.notify(vec![library.clone()], fingerprints)
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
/// Build the public projection directly, leaving engine-only raw provenance in its snapshot.
fn public_object(item: &SnapshotObject) -> SnapshotObject {
    let object = match &item.object {
        LibraryObject::Record(record) => LibraryObject::Record(Record {
            id: record.id.clone(),
            csl: record.csl.clone(),
            citation_keys: record.citation_keys.clone(),
            aliases: record.aliases.clone(),
            identifiers: record.identifiers.clone(),
            raw: None,
        }),
        object => object.clone(),
    };
    SnapshotObject {
        object,
        fingerprints: item.fingerprints.clone(),
    }
}

fn select<'a>(snapshot: &'a Snapshot, selector: &ItemSelector) -> Result<&'a SnapshotObject, EngineError> {
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
                record.citation_keys.iter().any(|candidate| candidate.value == *key) || record.aliases.contains(key)
            });
            let found = matches.next().ok_or_else(|| error(ErrorCode::NotFound))?;
            if matches.next().is_some() {
                return Err(error(ErrorCode::BadInput));
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
    EngineError { code }
}
fn response_schema(method: Method) -> Value {
    match method {
        Method::Libraries => schema_for!(LibrariesResult),
        Method::Search => schema_for!(SearchResult),
        Method::Show => schema_for!(SnapshotObject),
        Method::Cite => schema_for!(CitationResult),
        Method::Styles => schema_for!(StylesResult),
        _ => schema_for!(Value),
    }
    .into()
}
