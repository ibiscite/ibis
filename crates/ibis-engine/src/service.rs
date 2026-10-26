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
    collections::BTreeMap,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub fn check(&self) -> Result<(), EngineError> {
        if self.is_cancelled() {
            Err(error(ErrorCode::Cancelled))
        } else {
            Ok(())
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LibraryInfo {
    pub id: LibraryId,
    pub objects: usize,
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
#[derive(Debug)]
pub struct DispatchResult {
    pub data: Value,
    pub exit_code: ExitCode,
}

struct State {
    index: SnapshotIndex,
    libraries: BTreeMap<LibraryId, LibraryInfo>,
}
struct Shared {
    state: RwLock<Arc<State>>,
    /// Serializes publications without blocking readers during index building.
    publication: Mutex<()>,
}
#[derive(Clone)]
pub struct Engine {
    shared: Arc<Shared>,
}
impl Engine {
    pub fn new() -> Result<Self, EngineError> {
        Ok(Self::from_snapshot(Arc::new(
            Snapshot::new(vec![]).map_err(|_| error(ErrorCode::Internal))?,
        )))
    }
    pub fn from_snapshot(snapshot: Arc<Snapshot>) -> Self {
        let mut libraries = BTreeMap::<LibraryId, LibraryInfo>::new();
        for item in snapshot.objects() {
            let id = item.object.id().library.clone();
            libraries
                .entry(id.clone())
                .or_insert(LibraryInfo { id, objects: 0 })
                .objects += 1;
        }
        Self {
            shared: Arc::new(Shared {
                state: RwLock::new(Arc::new(State {
                    index: SnapshotIndex::new(snapshot),
                    libraries,
                })),
                publication: Mutex::new(()),
            }),
        }
    }
    fn state(&self) -> Result<Arc<State>, EngineError> {
        self.shared
            .state
            .read()
            .map(|state| state.clone())
            .map_err(|_| error(ErrorCode::Internal))
    }
    pub fn snapshot(&self) -> Result<Arc<Snapshot>, EngineError> {
        Ok(self.state()?.index.snapshot().clone())
    }
    pub fn publish_library(&self, library: LibraryId, snapshot: Arc<Snapshot>) -> Result<(), EngineError> {
        if snapshot
            .objects()
            .iter()
            .any(|item| item.object.id().library != library)
        {
            return Err(error(ErrorCode::BadInput));
        }
        let _publication = self.shared.publication.lock().map_err(|_| error(ErrorCode::Internal))?;
        let old = self.state()?;
        let mut objects: Vec<_> = old
            .index
            .snapshot()
            .objects()
            .iter()
            .filter(|item| item.object.id().library != library)
            .map(|item| item.object.clone())
            .collect();
        objects.extend(snapshot.objects().iter().map(|item| item.object.clone()));
        let combined = Arc::new(Snapshot::new(objects).map_err(|_| error(ErrorCode::BadInput))?);
        let mut libraries = old.libraries.clone();
        libraries.insert(
            library.clone(),
            LibraryInfo {
                id: library,
                objects: snapshot.objects().len(),
            },
        );
        let next = Arc::new(State {
            index: SnapshotIndex::new(combined),
            libraries,
        });
        *self.shared.state.write().map_err(|_| error(ErrorCode::Internal))? = next;
        Ok(())
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
        let hits = state
            .index
            .search(&request.query, request.limit, request.libraries.as_deref(), &|| {
                cancel.is_cancelled()
            })
            .ok_or_else(|| error(ErrorCode::Cancelled))?;
        Ok(SearchResult {
            snapshot: state.index.snapshot().fingerprints().clone(),
            hits: hits
                .into_iter()
                .map(|hit| SearchHit {
                    item: hit.item.clone(),
                    score: hit.score,
                    matched: hit.matched,
                })
                .collect(),
        })
    }
    pub fn show(&self, request: &ShowRequest) -> Result<SnapshotObject, EngineError> {
        let state = self.state()?;
        match &request.item {
            ItemSelector::Id(id) => state
                .index
                .snapshot()
                .get(id)
                .cloned()
                .ok_or_else(|| error(ErrorCode::NotFound)),
            ItemSelector::Key(key) => {
                if key.len() > 64 * 1024 || key.is_empty() {
                    return Err(error(ErrorCode::BadInput));
                }
                let mut matches = state.index.snapshot().objects().iter().filter(|item| {
                    let LibraryObject::Record(record) = &item.object else {
                        return false;
                    };
                    record.citation_keys.iter().any(|candidate| candidate.value == *key) || record.aliases.contains(key)
                });
                let found = matches.next().ok_or_else(|| error(ErrorCode::NotFound))?;
                if matches.next().is_some() {
                    return Err(error(ErrorCode::BadInput));
                }
                Ok(found.clone())
            }
        }
    }
    pub fn cite(&self, request: &CitationRequest, cancel: &Cancellation) -> Result<CitationResult, EngineError> {
        cancel.check()?;
        let snapshot = self.snapshot()?;
        let result = ibis_core::syntax::cite(&snapshot, request).map_err(|_| error(ErrorCode::BadInput))?;
        cancel.check()?;
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
        let mut exit_code = ExitCode::Ok;
        let data = match method {
            Method::Libraries => match parameters::<LibrariesRequest>(params)? {
                LibrariesRequest::List => value(self.libraries()?)?,
                _ => return Err(error(ErrorCode::SourceUnavailable)),
            },
            Method::Search => value(self.search(&parameters(params)?, cancel)?)?,
            Method::Show => value(self.show(&parameters(params)?)?)?,
            Method::Cite => {
                let request: CitationRequest = parameters(params)?;
                let result = self.cite(&request, cancel)?;
                if !result.losses.is_empty() || result.copy_content(request.items.len()).is_none() {
                    exit_code = ExitCode::Findings;
                }
                value(result)?
            }
            Method::Styles => {
                let _: StylesRequest = parameters(params)?;
                return Err(error(ErrorCode::SourceUnavailable));
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
        Ok(DispatchResult { data, exit_code })
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
        _ => schema_for!(Value),
    }
    .into()
}
