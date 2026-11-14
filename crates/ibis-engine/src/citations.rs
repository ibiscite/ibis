//! Sessions own citation ordering, cancellation, examples and commit validation.
use crate::{Cancellation, Engine, PreparedCitation};
use ibis_core::contract::*;
use std::collections::BTreeMap;
use std::sync::{
    Arc, Mutex, MutexGuard,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    Engine(EngineError),
    Presented(Problem),
    SharedCitationKey { key: String },
    SnapshotChanged,
    Cancelled,
    CitationTooLarge,
    NotPrepared,
}
impl From<ErrorCode> for SessionError {
    fn from(value: ErrorCode) -> Self {
        Self::Engine(value.into())
    }
}
impl From<EngineError> for SessionError {
    fn from(value: EngineError) -> Self {
        Self::Engine(value)
    }
}
impl From<CitationError> for SessionError {
    fn from(value: CitationError) -> Self {
        Self::Presented(value.problem())
    }
}
impl SessionError {
    pub fn problem(&self) -> Problem {
        match self {
            Self::Engine(error) => (*error).into(),
            Self::Presented(problem) => problem.clone(),
            Self::SharedCitationKey { key } => {
                let mut problem = CitationError::AmbiguousKey.problem();
                problem.info.message.id = "app-command-shared-citation-key".into();
                problem.info = problem.info.argument("key", key.clone());
                problem
            }
            Self::SnapshotChanged | Self::NotPrepared => CitationError::ChangedItem.problem(),
            Self::Cancelled => EngineError::new(ErrorCode::Cancelled).into(),
<<<<<<< HEAD
            Self::CitationTooLarge => CitationError::TooManyItems.problem(),
=======
            Self::CitationTooLarge => {
                let mut problem = CitationError::TooManyItems.problem();
                problem.info.message.id = "app-command-citation-too-large".into();
                problem
            }
>>>>>>> origin/main
        }
    }
}
impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&crate::catalogue::problem(
            &self.problem(),
            crate::catalogue::Audience::App,
        ))
    }
}
impl std::error::Error for SessionError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SessionId(u64);
impl SessionId {
    pub const PICKER: Self = Self(0);
}
const MAX_SESSIONS: usize = 65;
#[derive(Default)]
pub(crate) struct Registry {
    sessions: Mutex<BTreeMap<SessionId, Arc<SessionState>>>,
    next: AtomicU64,
}
type PreparedSlot = Option<(String, Arc<PreparedCitation>, OutputSpec)>;

#[derive(Default)]
struct SessionState {
    closed: AtomicBool,
    bibliography: Mutex<Option<BibliographyReceipt>>,
    pending: Mutex<BTreeMap<u64, Cancellation>>,
    intents: Mutex<BTreeMap<(Operation, u32), (u32, Cancellation)>>,
    prepared: Mutex<PreparedSlot>,
    latest_preview: Mutex<(u64, Cancellation)>,
    latest_details: Mutex<Cancellation>,
    latest_bibliography: Mutex<Cancellation>,
    running_bibliography: Mutex<Cancellation>,
}
struct BibliographyReceipt {
    selection: Box<[(ItemRef, Option<Fingerprint>)]>,
    style: String,
    citation: Arc<PreparedCitation>,
}
struct Worker {
    engine: Engine,
    state: Arc<SessionState>,
    sample: bool,
}
impl std::ops::Deref for Worker {
    type Target = SessionState;
    fn deref(&self) -> &SessionState {
        &self.state
    }
}
static TICKETS: AtomicU64 = AtomicU64::new(1);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Operation {
    Preview,
    Forms,
    Bibliography,
}
pub struct Citations<'a> {
    engine: &'a Engine,
}
/// Reserved synchronously by a transport before its worker receives the request.
pub(crate) struct SubmittedCitation {
    worker: Worker,
    reserved: u64,
}
impl std::fmt::Debug for SubmittedCitation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubmittedCitation").finish_non_exhaustive()
    }
}
impl SubmittedCitation {
    pub(crate) fn prepare(
        &self,
        request: &CitationRequest,
        every_library: bool,
    ) -> Result<Arc<PreparedCitation>, EngineError> {
        let cancel = self
            .worker
            .pending
            .lock()
            .map_err(|_| EngineError::new(ErrorCode::Internal))?
            .get(&self.reserved)
            .cloned()
            .ok_or_else(|| EngineError::new(ErrorCode::Cancelled))?;
        cancel.check()?;
        if self.worker.closed.load(Ordering::Acquire) {
            return Err(EngineError::new(ErrorCode::Cancelled));
        }
        let citation = Arc::new(self.worker.engine.prepare_consulting(request, every_library, &cancel)?);
        cancel.check()?;
        if self.worker.closed.load(Ordering::Acquire) {
            return Err(EngineError::new(ErrorCode::Cancelled));
        }
        Ok(citation)
    }
}
impl Drop for SubmittedCitation {
    fn drop(&mut self) {
        if let Ok(mut pending) = self.worker.pending.lock()
            && let Some(cancel) = pending.remove(&self.reserved)
        {
            cancel.cancel();
        }
    }
}
pub struct CitationTask<T>(Box<dyn FnOnce() -> Result<T, SessionError> + Send>);
impl<T> CitationTask<T> {
    pub fn run(self) -> Result<T, SessionError> {
        (self.0)()
    }
}
impl Engine {
    pub fn citations(&self) -> Citations<'_> {
        Citations { engine: self }
    }
}
impl Citations<'_> {
    fn worker(&self, id: SessionId) -> Result<Worker, SessionError> {
        let mut sessions = self
            .engine
            .shared
            .citations
            .sessions
            .lock()
            .map_err(|_| SessionError::NotPrepared)?;
        let state = if id == SessionId::PICKER {
            sessions.entry(id).or_default().clone()
        } else {
            sessions.get(&id).cloned().ok_or(SessionError::SnapshotChanged)?
        };
        if state.closed.load(Ordering::Acquire) {
            return Err(SessionError::Cancelled);
        }
        Ok(Worker {
            engine: self.engine.clone(),
            state,
            sample: false,
        })
    }
    pub fn start(
        &self,
        id: SessionId,
        operation: Operation,
        intent: CitationIntent,
    ) -> Result<Cancellation, SessionError> {
        let worker = self.worker(id)?;
        let mut intents = worker.intents.lock().map_err(|_| SessionError::NotPrepared)?;
        if !intents.contains_key(&(operation, intent.page)) && intents.len() >= 64 {
            return Err(ErrorCode::ResourceLimit.into());
        }
        let current = intents.entry((operation, intent.page)).or_default();
        if intent.request < current.0 {
            return Err(SessionError::Cancelled);
        }
        if intent.request > current.0 {
            current.1.cancel();
            *current = (intent.request, Cancellation::default());
        }
        if intent.cancel {
            current.1.cancel();
            return Err(SessionError::Cancelled);
        }
        current.1.check()?;
        if operation == Operation::Preview {
            worker
                .latest_preview
                .lock()
                .map_err(|_| SessionError::Cancelled)?
                .1
                .cancel();
        }
        Ok(current.1.clone())
    }
    pub fn open(&self) -> Result<SessionId, SessionError> {
        let mut sessions = self
            .engine
            .shared
            .citations
            .sessions
            .lock()
            .map_err(|_| SessionError::NotPrepared)?;
        // Reserve one registry slot for the picker, even before its first use.
        if sessions.keys().filter(|id| **id != SessionId::PICKER).count() >= MAX_SESSIONS - 1 {
            return Err(ErrorCode::ResourceLimit.into());
        }
        let id = SessionId(self.engine.shared.citations.next.fetch_add(1, Ordering::Relaxed) + 1);
        sessions.insert(id, Arc::default());
        Ok(id)
    }
    pub fn close(&self, id: SessionId) {
        if let Ok(mut sessions) = self.engine.shared.citations.sessions.lock()
            && let Some(state) = sessions.remove(&id)
        {
            state.closed.store(true, Ordering::Release);
            for cancel in state.pending.lock().unwrap_or_else(|error| error.into_inner()).values() {
                cancel.cancel();
            }
            state
                .latest_preview
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .1
                .cancel();
            state
                .latest_details
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .cancel();
            state
                .latest_bibliography
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .cancel();
            state
                .running_bibliography
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .cancel();
            Worker {
                engine: self.engine.clone(),
                state,
                sample: false,
            }
            .invalidate();
        }
    }
    pub fn invalidate(&self, id: SessionId) {
        if let Ok(worker) = self.worker(id) {
            worker.invalidate();
        }
    }
    /// One-shot cite requests finish independently; only previews supersede previews.
    pub(crate) fn submit(&self, id: SessionId, cancel: Cancellation) -> Result<SubmittedCitation, SessionError> {
        let worker = self.worker(id)?;
        let reserved = TICKETS.fetch_add(1, Ordering::Relaxed);
        {
            let mut pending = worker.pending.lock().map_err(|_| SessionError::NotPrepared)?;
            if worker.closed.load(Ordering::Acquire) {
                return Err(SessionError::Cancelled);
            }
            if pending.len() >= 64 {
                return Err(ErrorCode::ResourceLimit.into());
            }
            pending.insert(reserved, cancel.child());
        }
        Ok(SubmittedCitation { worker, reserved })
    }
    pub fn preview(
        &self,
        id: SessionId,
        draft: Draft,
        cancel: Cancellation,
    ) -> Result<CitationTask<Preview>, SessionError> {
        let mut worker = self.worker(id)?;
        worker.sample = draft.sample;
        let reserved = worker.reserve_preview_with(cancel);
        Ok(CitationTask(Box::new(move || worker.preview_for(reserved, draft))))
    }
    pub fn forms(
        &self,
        id: SessionId,
        ticket: &str,
        hash: &str,
        item: usize,
        cancel: Cancellation,
    ) -> Result<CitationTask<FormDetails>, SessionError> {
        let worker = self.worker(id)?;
        let ticket = ticket.to_owned();
        let hash = hash.to_owned();
        {
            let prepared = worker.prepared.lock().map_err(|_| SessionError::SnapshotChanged)?;
            let latest = worker.latest_preview.lock().map_err(|_| SessionError::Cancelled)?;
            if latest.0.to_string() != ticket
                || !prepared
                    .as_ref()
                    .is_some_and(|(id, citation, _)| id == &ticket && citation.result().content_hash.as_str() == hash)
            {
                return Err(SessionError::SnapshotChanged);
            }
            latest.1.check()?;
            Worker::replace_formatting(&worker.latest_details, &cancel)?;
        }
        Ok(CitationTask(Box::new(move || {
            worker.details(&ticket, &hash, item, &cancel)
        })))
    }
    pub fn bibliography(
        &self,
        id: SessionId,
        items: Vec<ItemRef>,
        style: String,
        cancel: Cancellation,
        sample: bool,
    ) -> Result<CitationTask<Option<SessionBibliography>>, SessionError> {
        let mut worker = self.worker(id)?;
        worker.sample = sample;
        worker.begin_bibliography(&cancel)?;
        Ok(CitationTask(Box::new(move || {
            {
                let _preview = worker.latest_preview.lock().map_err(|_| SessionError::Cancelled)?;
                cancel.check()?;
                if worker.closed.load(Ordering::Acquire) {
                    return Err(SessionError::Cancelled);
                }
                let mut queued = worker.latest_bibliography.lock().map_err(|_| SessionError::Cancelled)?;
                if queued.same(&cancel) {
                    *queued = Cancellation::default();
                }
                Worker::replace_formatting(&worker.running_bibliography, &cancel)?;
            }
            worker.session_bibliography(&items, &style, &cancel)
        })))
    }
    pub fn commit_bibliography(
        &self,
        id: SessionId,
        items: &[ItemRef],
        style: &str,
        hash: &str,
        cancel: &Cancellation,
    ) -> Result<CheckedCitation, SessionError> {
        cancel.check()?;
        let worker = self.worker(id)?;
        let cancel = cancel.child();
        {
            let _preview = worker.latest_preview.lock().map_err(|_| SessionError::Cancelled)?;
            if worker.closed.load(Ordering::Acquire) {
                return Err(SessionError::Cancelled);
            }
            Worker::replace_formatting(&worker.running_bibliography, &cancel)?;
        }
        let (citation, snapshot) = {
            let held = worker.bibliography.lock().map_err(|_| SessionError::SnapshotChanged)?;
            let held = held.as_ref().ok_or(SessionError::SnapshotChanged)?;
            if held.selection.len() != items.len()
                || held.style != style
                || held.citation.result().content_hash.as_str() != hash
            {
                return Err(SessionError::SnapshotChanged);
            }
            let snapshot = self.engine.snapshot()?;
            if held
                .selection
                .iter()
                .zip(items)
                .any(|((selected, expected), requested)| {
                    selected != requested
                        || snapshot.get(selected).map(|record| &record.fingerprints.cite) != expected.as_ref()
                })
            {
                return Err(SessionError::SnapshotChanged);
            }
            (held.citation.clone(), snapshot.fingerprints().cite.clone())
        };
        let result = self.engine.recite(&citation, &cancel).map_err(|error| match error {
            crate::CitationCommitError::Engine(error) => SessionError::from(error),
            _ => SessionError::SnapshotChanged,
        })?;
        cancel.check()?;
        if worker.closed.load(Ordering::Acquire) {
            return Err(SessionError::Cancelled);
        }
        if self.engine.snapshot()?.fingerprints().cite != snapshot {
            return Err(SessionError::SnapshotChanged);
        }
        Ok(CheckedCitation {
            ticket: hash.to_owned(),
            hash: hash.to_owned(),
            citation,
            result,
            session: worker.state.clone(),
            owner: Arc::downgrade(&self.engine.shared),
            snapshot,
            cancel,
            bibliography: true,
        })
    }
    pub fn commit(&self, id: SessionId, ticket: &str, hash: &str) -> Result<CheckedCitation, SessionError> {
        self.worker(id)?.recite(ticket, hash)
    }
    pub fn prepared(&self, id: SessionId, ticket: &str, hash: &str) -> Result<Arc<PreparedCitation>, SessionError> {
        let worker = self.worker(id)?;
        let slot = worker.prepared.lock().map_err(|_| SessionError::SnapshotChanged)?;
        let (current, citation, _) = slot.as_ref().ok_or(SessionError::SnapshotChanged)?;
        if current != ticket || citation.result().content_hash.as_str() != hash {
            return Err(SessionError::SnapshotChanged);
        }
        worker
            .preview_cancellation(ticket.parse().map_err(|_| SessionError::SnapshotChanged)?)?
            .check()?;
        Ok(citation.clone())
    }
}
/// Invalid field input retains its recovery and field focus until rendering.
fn field_failure(error: ibis_core::locator::FieldError, index: usize) -> SessionError {
    use ibis_core::locator::FieldError;
    let (FieldError::MissingValue(term) | FieldError::InvalidValue(term)) = error;
    let mut problem = CitationError::InvalidLocator.problem();
    problem.info.message.id = match (term == LocatorTerm::Page, matches!(error, FieldError::MissingValue(_))) {
        (true, true) => "app-command-page-missing",
        (true, false) => "app-command-page-invalid",
        (false, true) => "app-command-locator-missing",
        (false, false) => "app-command-locator-invalid",
    }
    .into();
    let id = crate::catalogue::id("locator", term);
    let label = crate::catalogue::text(&id, None, &[]).unwrap_or_default();
    problem.info = problem
        .info
        .argument("number", (index + 1).to_string())
        .argument("label", label);
    SessionError::Presented(problem)
}

impl Worker {
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

    fn bibliography_of(
        &self,
        items: Vec<CitationItem>,
        style: &str,
        cancel: &Cancellation,
    ) -> Result<(PreparedCitation, bool), SessionError> {
        cancel.check().map_err(SessionError::from)?;
        let count = items.len();
        let request = CitationRequest {
            items,
            form: CitationForm::BibliographyOnly,
            keep_order: false,
            output: OutputSpec::Csl { style: style.into() },
            input_language: None,
            strict: false,
            output_locale: None,
        };
        let citation = self
            .engine
            .prepare_citation(&request, cancel)
            .map_err(SessionError::from)?;
        let whole = citation.result().copy_content(count).is_some();
        Ok((citation, whole))
    }

    /// The list as a bibliography. A record the style cannot write must not
    /// cost the person all the others, so it is found and named instead.
    fn prepare_session_bibliography(
        &self,
        snapshot: &Snapshot,
        ids: &[ItemRef],
        style: &str,
        cancel: &Cancellation,
    ) -> Result<Option<(PreparedCitation, Vec<String>)>, SessionError> {
        cancel.check().map_err(SessionError::from)?;
        crate::outputs::resolve_request(&CitationRequest {
            items: vec![],
            form: CitationForm::BibliographyOnly,
            keep_order: false,
            output: OutputSpec::Csl { style: style.into() },
            input_language: None,
            strict: false,
            output_locale: None,
        })?;
        let items: Vec<_> = ids.iter().filter_map(|id| self.session_item(snapshot, id)).collect();
        if items.is_empty() {
            return Ok(None);
        }
        let first = self.bibliography_of(items.clone(), style, cancel);
        cancel.check().map_err(SessionError::from)?;
        let (citation, left_out) = if let Ok((citation, true)) = first {
            (citation, vec![])
        } else {
            let mut kept = vec![];
            let mut left_out = vec![];
            for item in items {
                cancel.check().map_err(SessionError::from)?;
                // Refused, or a failure only this record causes. The engine names it.
                match self.bibliography_of(vec![item.clone()], style, cancel) {
                    Ok((_, true)) => kept.push(item),
                    Ok((single, false)) => left_out.extend(single.item_label(0)),
                    Err(SessionError::Cancelled) => return Err(SessionError::Cancelled),
                    Err(_) => left_out.extend(session_key(snapshot, &item.item)),
                }
            }
            cancel.check().map_err(SessionError::from)?;
            if kept.is_empty() {
                let mut problem = CitationError::UnsupportedForm.problem();
                problem.info.message.id = "app-command-bibliography-unsupported".into();
                problem.info = problem.info.argument("references", left_out.join(", "));
<<<<<<< HEAD
                return Err(SessionError::Presented(problem));
            }
            let (citation, whole) = self.bibliography_of(kept, style, cancel)?;
            if !whole {
                return Err(CitationError::UnsupportedForm.into());
=======
                return Err(SessionError::Presented(
                    problem.recover_with(Recovery::RemoveReferences),
                ));
            }
            let (citation, whole) = self.bibliography_of(kept, style, cancel)?;
            if !whole {
                return Err(SessionError::Presented(
                    CitationError::UnsupportedForm
                        .problem()
                        .recover_with(Recovery::RemoveReferences)
                        .on(CitationSurface::Bibliography),
                ));
>>>>>>> origin/main
            }
            (citation, left_out)
        };
        let citation = if self.sample {
            citation.mark_sample().map_err(SessionError::from)?
        } else {
            citation
        };
        Ok(Some((citation, left_out)))
    }

    fn session_bibliography(
        &self,
        ids: &[ItemRef],
        style: &str,
        cancel: &Cancellation,
    ) -> Result<Option<SessionBibliography>, SessionError> {
        let snapshot = self.engine.snapshot()?;
        let prepared = self.prepare_session_bibliography(&snapshot, ids, style, cancel)?;
        cancel.check().map_err(SessionError::from)?;
        if self.closed.load(Ordering::Acquire) {
            return Err(SessionError::Cancelled);
        }
        let selection = ids
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    snapshot.get(id).map(|record| record.fingerprints.cite.clone()),
                )
            })
            .collect();
        let Some((citation, left_out)) = prepared else {
            return Ok(None);
        };
        let warnings = citation_warnings_named(&citation);
        let citation = Arc::new(citation);
        let result = citation.result();
        let shown = SessionBibliography {
            text: result.text.clone(),
            display: result.display.clone(),
            content_hash: result.content_hash.as_str().to_owned(),
            warnings,
            left_out,
        };
        let mut slot = self.bibliography.lock().map_err(|_| SessionError::SnapshotChanged)?;
        cancel.check()?;
        if self.closed.load(Ordering::Acquire) {
            return Err(SessionError::Cancelled);
        }
        *slot = Some(BibliographyReceipt {
            selection,
            style: style.into(),
            citation,
        });
        Ok(Some(shown))
    }

    fn reserve_preview(&self) -> u64 {
        self.reserve_preview_with(Cancellation::default())
    }
    fn reserve_preview_with(&self, cancel: Cancellation) -> u64 {
        let mut latest = self
            .latest_preview
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let request = TICKETS.fetch_add(1, Ordering::Relaxed);
        let cancel = cancel.child();
        // A superseded command can acquire the library after its replacement.
        // Give it an unpublished ticket without cancelling the current worker.
        if cancel.is_cancelled() || self.closed.load(Ordering::Acquire) {
            return request;
        }
        latest.1.cancel();
        self.latest_details
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .cancel();
        self.latest_bibliography
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .cancel();
        *latest = (request, cancel);
        request
    }
    fn preview_cancellation(&self, reservation: u64) -> Result<Cancellation, SessionError> {
        let latest = self.latest_preview.lock().map_err(|_| SessionError::NotPrepared)?;
        if latest.0 != reservation || self.closed.load(Ordering::Acquire) {
            return Err(SessionError::Cancelled);
        }
        Ok(latest.1.clone())
    }

    fn preview_for(&self, reservation: u64, draft: Draft) -> Result<Preview, SessionError> {
        let Draft {
            items,
            output,
            keep_order,
            form,
            input_language,
            output_locale,
            ..
        } = draft;
        let output = &output;
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
            return Err(SessionError::CitationTooLarge);
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
        let items: Vec<_> = items
            .into_iter()
            .enumerate()
            .map(|(index, item)| {
                let locator = match (item.locator, item.locator_input) {
                    (Some(_), Some(_)) => {
                        let mut problem = CitationError::InvalidLocator.problem();
                        problem.info.message.id = "app-command-locator-conflict".into();
                        problem.info = problem.info.argument("number", (index + 1).to_string());
                        return Err(SessionError::Presented(problem));
                    }
                    (locator, None) => locator,
                    (None, Some(input)) => Some(
                        ibis_core::locator::read_field(input.label, &input.text)
                            .map_err(|error| field_failure(error, index))?,
                    ),
                };
                Ok(CitationItem {
                    item: item.id,
                    expected_cite: Some(item.cite),
                    form: item.form.filter(|form| *form != CitationForm::Normal),
                    suppress_author: false,
                    locator,
                    prefix: literal(item.prefix),
                    suffix: literal(item.suffix),
                    annotation: None,
                    expected_annotation_cite: None,
                })
            })
            .collect::<Result<_, SessionError>>()?;
        let request = CitationRequest {
            items,
            keep_order: keep_order || matches!(output, OutputSpec::Key { .. }),
            output: output.clone(),
            form,
            input_language,
            strict: false,
            output_locale,
        };
        let citation = self
            .engine
            .prepare_citation(&request, &cancel)
            .map_err(SessionError::from)?;
        let original_output = citation.request_output().clone();
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
                return Err(SessionError::SharedCitationKey { key });
            }
            if let Some(item) = refused(CitationError::UnsupportedForm) {
                let mut failure = CitationError::UnsupportedForm.problem();
<<<<<<< HEAD
                failure.info.message.id = "app-command-unsupported-form".into();
=======
>>>>>>> origin/main
                let form = request.items[item].form.unwrap_or(request.form);
                let label = crate::catalogue::text(&crate::catalogue::id("form", form), None, &[]).unwrap_or_default();
                failure.info = failure
                    .info
<<<<<<< HEAD
                    .argument("number", (item + 1).to_string())
                    .argument("form", label);
=======
                    .argument("form", label)
                    .argument(
                        "reference",
                        crate::catalogue::text("app-reference-number", None, &[("number", &(item + 1).to_string())])
                            .unwrap_or_default(),
                    )
                    .argument(
                        "output",
                        crate::outputs::named(&request.output).map_or_else(
                            || crate::catalogue::text("mcp-output-name", None, &[]).unwrap_or_default(),
                            |output| output.name.clone(),
                        ),
                    );
>>>>>>> origin/main
                let problem = crate::catalogue::problem(&failure, crate::catalogue::Audience::App);
                let mut preview = self.publish_preview(reservation, citation, original_output)?;
                preview.problem = Some(problem);
                preview.failure = Some(failure);
                return Ok(preview);
            }
            return Err(result
                .outcomes
                .iter()
                .find_map(|outcome| match outcome {
                    ItemOutcome::Refused { error, .. } => Some(SessionError::from(*error)),
                    _ => None,
                })
                .unwrap_or_else(|| SessionError::from(CitationError::FormatterUnavailable)));
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
    ) -> Result<Preview, SessionError> {
        let citation = if self.sample
            && citation
                .result()
                .outcomes
                .iter()
                .all(|outcome| matches!(outcome, ItemOutcome::Rendered { .. }))
        {
            citation.mark_sample().map_err(SessionError::from)?
        } else {
            citation
        };
        let mut slot = self.prepared.lock().map_err(|_| SessionError::NotPrepared)?;
        self.preview_cancellation(reservation)?
            .check()
            .map_err(SessionError::from)?;
        let result = citation.result();
        let preview = Preview {
            effective_output: crate::outputs::named(citation.request_output()).map(|output| output.id.to_owned()),
            ticket: reservation.to_string(),
            content_hash: result.content_hash.as_str().to_owned(),
            text: result.text.clone(),
            warnings: citation_warnings_named(&citation),
            display: result.display.clone(),
            problem: None,
            failure: None,
            project_refusal: None,
            notices: result.notices.clone(),
        };
        *slot = Some((preview.ticket.clone(), Arc::new(citation), original_output));
        Ok(preview)
    }
    fn replace_formatting(slot: &Mutex<Cancellation>, cancel: &Cancellation) -> Result<(), SessionError> {
        let mut current = slot.lock().map_err(|_| SessionError::Cancelled)?;
        cancel.check().map_err(SessionError::from)?;
        current.cancel();
        *current = cancel.clone();
        Ok(())
    }

    /// Register before spawning a worker: a citation that replaces this work
    /// must also stop a bibliography that has not started formatting yet.
    fn begin_bibliography(&self, cancel: &Cancellation) -> Result<(), SessionError> {
        let _preview = self.latest_preview.lock().map_err(|_| SessionError::Cancelled)?;
        if self.closed.load(Ordering::Acquire) {
            return Err(SessionError::Cancelled);
        }
        Self::replace_formatting(&self.latest_bibliography, cancel)
    }

    fn details(
        &self,
        ticket: &str,
        hash: &str,
        item: usize,
        cancel: &Cancellation,
    ) -> Result<FormDetails, SessionError> {
        let (reservation, citation, original_output) = {
            let slot = self.prepared.lock().map_err(|_| SessionError::SnapshotChanged)?;
            let (current, citation, output) = slot.as_ref().ok_or(SessionError::SnapshotChanged)?;
            if current != ticket || citation.result().content_hash.as_str() != hash {
                return Err(SessionError::SnapshotChanged);
            }
            (
                ticket.parse().map_err(|_| SessionError::SnapshotChanged)?,
                Arc::clone(citation),
                output.clone(),
            )
        };
        {
            // Install under the preview's identity lock so a replacement cannot
            // pass between validation and registration of the examples' owner.
            let latest = self.latest_preview.lock().map_err(|_| SessionError::Cancelled)?;
            if latest.0 != reservation || self.closed.load(Ordering::Acquire) {
                return Err(SessionError::Cancelled);
            }
            latest.1.check().map_err(SessionError::from)?;
            cancel.check()?;
        }
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
                Some(CitationForm::Normal) if textcite => Some(CitationForm::Narrative),
                Some(form) => Some(form),
                None => None,
            }
        };
        let mut forms = Vec::new();
        for (wire, form) in CitationForm::ALL.into_iter().map(|form| (form, form)) {
            cancel.check().map_err(SessionError::from)?;
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
                .form_example(&self.engine, &original_output, item, form, cancel)
                .map_err(SessionError::from)?;
            let supported = result.outcomes.contains(&ItemOutcome::Rendered { item });
            // Refused in a group, a form may still work for this reference alone.
            let alone = !supported
                && result.outcomes.len() > 1
                && citation
                    .form_alone(&self.engine, &original_output, item, form, cancel)
                    .map_err(SessionError::from)?;
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
            .map_err(SessionError::from)?;
        cancel.check().map_err(SessionError::from)?;
        Ok(FormDetails { forms, current_form })
    }
    fn invalidate(&self) {
        self.reserve_preview();
        if let Ok(mut prepared) = self.prepared.lock() {
            *prepared = None;
        }
    }
    /// Cite the shown citation again for a copy. This can wait for the
    /// formatter, so it holds no lock and its caller keeps it off the main thread.
    fn recite(&self, ticket: &str, hash: &str) -> Result<CheckedCitation, SessionError> {
        let citation = {
            let prepared = self.prepared.lock().map_err(|_| SessionError::SnapshotChanged)?;
            let (current, citation, _) = prepared.as_ref().ok_or(SessionError::SnapshotChanged)?;
            if current != ticket || citation.result().content_hash.as_str() != hash {
                return Err(SessionError::SnapshotChanged);
            }
            Arc::clone(citation)
        };
        // A newer preview cancels this one's work, a copy's included.
        let cancel = self.preview_cancellation(ticket.parse().map_err(|_| SessionError::SnapshotChanged)?)?;
        let snapshot = self.engine.snapshot()?.fingerprints().cite.clone();
        let result = self.engine.recite(&citation, &cancel).map_err(|error| match error {
            crate::CitationCommitError::Engine(error) => SessionError::from(error),
            _ => SessionError::SnapshotChanged,
        })?;
        if self.engine.snapshot()?.fingerprints().cite != snapshot {
            return Err(SessionError::SnapshotChanged);
        }
        Ok(CheckedCitation {
            ticket: ticket.to_owned(),
            hash: hash.to_owned(),
            citation,
            result,
            session: self.state.clone(),
            owner: Arc::downgrade(&self.engine.shared),
            snapshot,
            cancel,
            bibliography: false,
        })
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
    use crate::catalogue::{self, Audience};
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

/// Checked content retains both immutable receipt identity and the owning session.
pub struct CheckedCitation {
    snapshot: Fingerprint,
    cancel: Cancellation,
    bibliography: bool,
    ticket: String,
    hash: String,
    citation: Arc<PreparedCitation>,
    result: CitationResult,
    session: Arc<SessionState>,
    owner: std::sync::Weak<crate::service::Shared>,
}
impl CheckedCitation {
    pub fn ticket(&self) -> &str {
        &self.ticket
    }
    pub fn content_hash(&self) -> &str {
        &self.hash
    }
    pub fn result(&self) -> &CitationResult {
        &self.result
    }
    pub fn citation(&self) -> &PreparedCitation {
        &self.citation
    }
    /// Hold this guard on the main thread through writing. Reservation cannot pass it.
    pub fn check<'a>(&'a self, engine: &Engine) -> Result<CheckedContent<'a>, SessionError> {
        if !self
            .owner
            .upgrade()
            .is_some_and(|owner| Arc::ptr_eq(&owner, &engine.shared))
        {
            return Err(SessionError::SnapshotChanged);
        }
        self.cancel.check()?;
        if self.session.closed.load(Ordering::Acquire) {
            return Err(SessionError::SnapshotChanged);
        }
        let mut prepared = None;
        let mut latest = None;
        let mut bibliography = None;
        if self.bibliography {
            let held = self
                .session
                .bibliography
                .lock()
                .map_err(|_| SessionError::SnapshotChanged)?;
            if !held
                .as_ref()
                .is_some_and(|held| Arc::ptr_eq(&held.citation, &self.citation))
            {
                return Err(SessionError::SnapshotChanged);
            }
            bibliography = Some(held);
        } else {
            let held = self
                .session
                .prepared
                .lock()
                .map_err(|_| SessionError::SnapshotChanged)?;
            let current = self
                .session
                .latest_preview
                .lock()
                .map_err(|_| SessionError::SnapshotChanged)?;
            if current.0.to_string() != self.ticket
                || !held
                    .as_ref()
                    .is_some_and(|(id, citation, _)| id == &self.ticket && Arc::ptr_eq(citation, &self.citation))
            {
                return Err(SessionError::SnapshotChanged);
            }
            current.1.check()?;
            prepared = Some(held);
            latest = Some(current);
        }
        self.cancel.check()?;
        engine
            .still_current(&self.citation)
            .map_err(|_| SessionError::SnapshotChanged)?;
        if self.session.closed.load(Ordering::Acquire) || engine.snapshot()?.fingerprints().cite != self.snapshot {
            return Err(SessionError::SnapshotChanged);
        }
        Ok(CheckedContent {
            checked: self,
            _prepared: prepared,
            _latest: latest,
            _bibliography: bibliography,
        })
    }
    pub fn warnings(&self) -> Vec<String> {
        citation_warnings_named(&self.citation)
    }
}
pub struct CheckedContent<'a> {
    checked: &'a CheckedCitation,
    _prepared: Option<MutexGuard<'a, PreparedSlot>>,
    _latest: Option<MutexGuard<'a, (u64, Cancellation)>>,
    _bibliography: Option<MutexGuard<'a, Option<BibliographyReceipt>>>,
}
impl CheckedContent<'_> {
    pub fn content(&self) -> ClipboardContent<'_> {
        let result = &self.checked.result;
        ClipboardContent {
            text: &result.text,
            html: result.html.as_deref(),
            rtf: result.rtf.as_deref(),
        }
    }
}
