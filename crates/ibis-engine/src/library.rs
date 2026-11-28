//! The library owner (decision 0052): which library is open, reading it off
//! the caller's thread, checking it again and saving the choice. A front end
//! submits intents, takes the open library for searching and citing, and
//! draws the status the owner publishes after every change.
//!
//! Requests are numbered when they are submitted, so a read that finishes
//! after a newer request never replaces it. A failed switch to B keeps A
//! searchable and names B as the failed attempt.

use ibis_os::sync::MutexExt;
mod error;
mod status;
#[cfg(test)]
mod tests;

pub use error::{LoadError, words};
pub use status::{LibraryProblem, LibraryState, LibraryStatus, ReadingNotice, native_source};

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use ibis_core::contract::{EngineError, ErrorCode};
use ibis_os::file_io::{Admission, Pool, Scope};
use serde_json::json;

use crate::citations::SessionId;
use crate::settings::{LibrarySetting, Settings, ZoteroLibraries, ZoteroLibrary};
use crate::{Cancellation, Engine, LibrarySubscriptionClose, Method, PreparedFileReload, PreparedZoteroReload};

/// Where references come from: a library file, Zotero libraries, or the
/// bundled invented samples.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    File(PathBuf),
    Zotero(ZoteroLibraries),
    Samples(PathBuf),
}

impl From<PathBuf> for Source {
    fn from(path: PathBuf) -> Self {
        Self::File(path)
    }
}

impl From<LibrarySetting> for Source {
    fn from(setting: LibrarySetting) -> Self {
        match setting {
            LibrarySetting::Bibtex(path) => Self::File(path),
            LibrarySetting::Zotero(library) => Self::Zotero(library),
        }
    }
}

impl Source {
    pub fn setting(&self) -> Option<LibrarySetting> {
        match self {
            Self::File(path) => Some(LibrarySetting::Bibtex(path.clone())),
            Self::Zotero(libraries) => Some(LibrarySetting::Zotero(libraries.clone())),
            Self::Samples(_) => None,
        }
    }

    /// The name shown to the user: a file name only, never a full path.
    pub fn name(&self) -> Option<String> {
        match self {
            Self::Samples(_) => Some("Sample references".to_string()),
            Self::File(path) => path.file_name().map(|n| n.to_string_lossy().into_owned()),
            Self::Zotero(libraries) => Some(match libraries.libraries() {
                [ZoteroLibrary::User] => "Zotero library".to_string(),
                [ZoteroLibrary::Group(id)] => format!("Zotero group {id}"),
                several => format!("{} Zotero libraries", several.len()),
            }),
        }
    }
}

/// The open library: the engine that read it and where it came from.
pub struct Current {
    pub engine: Engine,
    pub source: Source,
    /// The path the first read resolved, saved as the choice.
    source_path: Option<PathBuf>,
    observing: AtomicBool,
}

impl Current {
    fn new(engine: Engine, source: Source, source_path: Option<PathBuf>) -> Self {
        Self {
            engine,
            source,
            source_path,
            observing: AtomicBool::new(false),
        }
    }
    /// Read `source` into `engine`, for a client that opens one library
    /// itself; the owner opens every library it shows this way.
    pub fn open(
        engine: Engine,
        source: Source,
        profile: Option<&str>,
        cancel: &Cancellation,
    ) -> Result<Self, LoadError> {
        match source {
            Source::File(path) => {
                let read = engine.open_file_observed(&path, cancel).map_err(LoadError::file)?;
                Ok(Self::new(engine, Source::File(path), Some(read)))
            }
            Source::Samples(path) => {
                // The bundled immutable file needs neither a watch nor a saved choice.
                let engine = engine.without_file_watches();
                engine.open_file_observed(&path, cancel).map_err(LoadError::file)?;
                Ok(Self::new(engine, Source::Samples(path), None))
            }
            Source::Zotero(chosen) => {
                open_zotero(&engine, &chosen, profile, cancel)?;
                Ok(Self::new(engine, Source::Zotero(chosen), None))
            }
        }
    }
    /// Only bundled invented references.
    pub fn sample(&self) -> bool {
        matches!(self.source, Source::Samples(_))
    }
    /// The open snapshot's identity; empty when it cannot be read.
    pub fn revision(&self) -> String {
        self.engine
            .snapshot()
            .map(|snapshot| snapshot.fingerprints().full.as_str().to_owned())
            .unwrap_or_default()
    }
}

/// When a library that opens becomes the one opened next time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Save {
    /// Always: the person chose it.
    Always,
    /// Only when it differs from the saved choice.
    IfChanged,
    /// Never: the choice came from the settings file.
    Never,
}

/// What a front end asks of the library.
#[derive(Clone, Debug)]
pub enum Intent {
    /// Open `source`. `profile` is a Zotero profile to read in place of the
    /// saved one; only a closed read that used it saves it.
    Open {
        source: Source,
        save: Save,
        profile: Option<String>,
    },
    /// The picker was shown: check Zotero, and a file that cannot be watched
    /// or failed to read, without a busy state; retry a Zotero choice that
    /// has not opened.
    Check,
    /// The person came back: retry a Zotero choice that failed.
    RetryFailed,
    /// Read the open library again, for example after editing it.
    Reload,
    /// The saved choice was removed: no library is open.
    Close,
    /// Leave the samples without touching the saved choice.
    LeaveSamples,
    /// A file dialog returned something that is not on this computer.
    RefuseNotLocal,
}

/// Where the library choice is read and saved.
pub trait ChoiceStore: Send + Sync + 'static {
    /// The Zotero profile chosen for reading while Zotero is closed.
    fn zotero_profile(&self) -> Result<Option<String>, EngineError>;
    /// Apply `change` in one settings turn.
    fn save(&self, change: &mut dyn FnMut(&mut Settings)) -> Result<(), String>;
}

impl ChoiceStore for crate::settings::Owner {
    fn zotero_profile(&self) -> Result<Option<String>, EngineError> {
        self.read()
            .cites()
            .map(|settings| settings.device.zotero_profile.clone())
            .map_err(|_| EngineError::new(ErrorCode::SourceUnavailable))
    }
    fn save(&self, change: &mut dyn FnMut(&mut Settings)) -> Result<(), String> {
        self.change(|settings| {
            change(settings);
            Ok::<_, crate::settings::SettingsError>(())
        })
        .map(|_| ())
        .map_err(|error| error.to_string())
    }
}

/// What a command needs from the library at one moment.
#[derive(Clone, Default)]
pub struct View {
    pub current: Option<Arc<Current>>,
    /// The source asked for, which may not have opened yet or have failed.
    pub requested: Option<Source>,
    /// Changes with every request and every installed library.
    pub generation: u64,
    /// A library is being opened; its status shows that.
    pub opening: bool,
    /// The open library is being checked, without a busy state.
    pub checking: bool,
    pub error: Option<LoadError>,
}

impl View {
    pub fn source(&self) -> Option<&Source> {
        self.current.as_ref().map(|current| &current.source)
    }
    /// The Zotero libraries asked for last, or still shown after another
    /// choice failed, so going back to Zotero keeps the groups that were on.
    pub fn zotero_choice(&self) -> Option<ZoteroLibraries> {
        [self.requested.as_ref(), self.source()]
            .into_iter()
            .flatten()
            .find_map(|source| match source {
                Source::Zotero(chosen) => Some(chosen.clone()),
                _ => None,
            })
    }
}

type Publish = Box<dyn Fn(LibraryStatus) + Send + Sync>;

/// One owner per process; clones share it.
#[derive(Clone)]
pub struct Owner {
    shared: Arc<Shared>,
}

struct Shared {
    state: Mutex<State>,
    /// Counts requests, so a read that finishes after a newer one is discarded.
    request: AtomicU64,
    queue: Mutex<Queue>,
    /// Signalled when the queue runs dry.
    idle: Condvar,
    store: Arc<dyn ChoiceStore>,
    engine: Box<dyn Fn() -> Result<Engine, EngineError> + Send + Sync>,
    publish: Publish,
    /// This owner's reads still running, abandoned or not, per kind of source.
    file_reads: Pool,
    zotero_reads: Pool,
}

#[derive(Default)]
struct State {
    current: Option<Arc<Current>>,
    /// Disconnects the blocked notification worker when its library is replaced.
    observer: Option<LibrarySubscriptionClose>,
    generation: u64,
    /// The requested source, including before it has been read.
    requested: Option<Source>,
    opening: bool,
    /// A quiet check is running; unlike `opening`, it is not shown.
    checking: bool,
    error: Option<LoadError>,
    /// An automatic update problem while a different failed choice is retained.
    retained_error: Option<LoadError>,
    /// A failed choice save, separate from a retained-source refresh problem.
    choice_save_warning: Option<String>,
    /// When a reread of the same source last brought different references
    /// during this run; `None` after the first read.
    changed_at: Option<SystemTime>,
    /// When the source was last found to match the library.
    checked_at: Option<SystemTime>,
    /// Last snapshot accounted for by a completion or an engine event.
    observed_revision: Option<String>,
}

impl State {
    fn advance_generation(&mut self) {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        self.generation = NEXT.fetch_add(1, Ordering::SeqCst);
    }

    fn observe_revision(&mut self, now: SystemTime) -> bool {
        let Some(revision) = self
            .current
            .as_ref()
            .map(|current| current.revision())
            .filter(|revision| !revision.is_empty())
        else {
            return false;
        };
        let changed = self
            .observed_revision
            .as_ref()
            .is_some_and(|previous| previous != &revision);
        self.observed_revision = Some(revision);
        if changed {
            self.changed_at = Some(now);
        }
        changed
    }

    fn retire_citations(&self) {
        if let Some(current) = &self.current {
            current.engine.citations().invalidate(SessionId::PICKER);
        }
    }

    /// Nothing open; a settings notice is the app's and is kept by it.
    fn clear(&mut self) {
        self.retire_citations();
        *self = Self::default();
        self.advance_generation();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OpenIntent {
    Remember,
    Read,
    ReadOnly,
    Refresh,
}

impl From<Save> for OpenIntent {
    fn from(save: Save) -> Self {
        match save {
            Save::Always => Self::Remember,
            Save::IfChanged => Self::Read,
            Save::Never => Self::ReadOnly,
        }
    }
}

impl OpenIntent {
    fn rereads_source(self, source: &Source, reload: bool) -> bool {
        reload && (matches!(source, Source::File(_)) || self != Self::Refresh)
    }

    fn saves_choice(self, source: &Source, chosen: &LibrarySetting) -> bool {
        let changed = match (source, chosen) {
            // Path equality ignores `.`; remember the canonical spelling captured by the read.
            (Source::File(previous), LibrarySetting::Bibtex(next)) => previous.as_os_str() != next.as_os_str(),
            _ => source.setting().as_ref() != Some(chosen),
        };
        self == Self::Remember || (self == Self::Read && changed)
    }

    fn retry(reload: bool) -> Self {
        if reload { Self::Refresh } else { Self::Remember }
    }
}

struct OpenRequest {
    request: u64,
    source: Source,
    reload: bool,
    intent: OpenIntent,
    profile: Option<String>,
    cancel: OpenCancellation,
}

#[derive(Clone, Default)]
struct OpenCancellation {
    token: Cancellation,
}

impl OpenCancellation {
    fn cancel(&self) {
        self.token.cancel();
    }
}

/// One supervisor runs the active request; a new choice replaces pending work.
#[derive(Default)]
struct Queue {
    running: bool,
    pending: Option<OpenRequest>,
    active: Option<OpenCancellation>,
}

/// What a background read produced.
enum Loaded {
    New(Arc<Current>),
    /// A reread found nothing changed; keep the current references.
    Unchanged,
    /// A strict read outcome, still private until it is accepted.
    Reload(Arc<Current>, Box<Prepared>),
}

enum Prepared {
    File(PreparedFileReload),
    Zotero(PreparedZoteroReload),
}

/// The whole load, parsing included: a slow machine with a large library must
/// finish. A newer choice cancels at once, so this only limits a lone stuck read.
const LOAD_DEADLINE: Duration = Duration::from_secs(120);

impl Owner {
    /// The owner of this process's library. `engine` makes the engine each
    /// newly opened library is read into. `publish` receives every status,
    /// in order; it runs while the owner is locked, so it must not call back.
    pub fn new(
        store: Arc<dyn ChoiceStore>,
        engine: impl Fn() -> Result<Engine, EngineError> + Send + Sync + 'static,
        publish: impl Fn(LibraryStatus) + Send + Sync + 'static,
    ) -> Self {
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State::default()),
                request: AtomicU64::new(0),
                queue: Mutex::new(Queue::default()),
                idle: Condvar::new(),
                store,
                engine: Box::new(engine),
                publish: Box::new(publish),
                file_reads: Pool::new(4, 0),
                zotero_reads: Pool::new(4, 0),
            }),
        }
    }

    /// Ask for something; the outcome arrives as a published status. Returns
    /// false when there was nothing to do: `Reload` with no library.
    pub fn submit(&self, intent: Intent) -> bool {
        let shared = &self.shared;
        match intent {
            Intent::Open { source, save, profile } => {
                let (request, reload) = shared.with_state(|state| shared.record_open(state, source.clone()));
                shared.run(request, source, reload, save.into(), profile);
            }
            Intent::Check => {
                if let Some((request, source)) = shared.with_state(|state| shared.begin_check(state)) {
                    shared.run(request, source, true, OpenIntent::Refresh, None);
                } else if let Some((request, source, reload)) =
                    shared.with_state(|state| shared.begin_zotero_retry(state, false))
                {
                    shared.run(request, source, reload, OpenIntent::retry(reload), None);
                }
            }
            Intent::RetryFailed => {
                if let Some((request, source, reload)) =
                    shared.with_state(|state| shared.begin_zotero_retry(state, true))
                {
                    shared.run(request, source, reload, OpenIntent::retry(reload), None);
                }
            }
            Intent::Reload => {
                let source = shared.with_state(|state| match zotero_to_check(state) {
                    // A choice that has not opened yet is saved once it does.
                    Some(source) => Some((source.clone(), state.source() != Some(&source))),
                    None => state.source().cloned().map(|source| (source, false)),
                });
                let Some((source, remember)) = source else {
                    return false;
                };
                let save = if remember { Save::Always } else { Save::IfChanged };
                return self.submit(Intent::Open {
                    source,
                    save,
                    profile: None,
                });
            }
            Intent::Close => shared.publish_after(|state| {
                shared.request.fetch_add(1, Ordering::SeqCst);
                state.clear();
            }),
            Intent::LeaveSamples => shared.publish_after(|state| {
                if matches!(state.source(), Some(Source::Samples(_)))
                    || matches!(state.requested, Some(Source::Samples(_)))
                {
                    shared.request.fetch_add(1, Ordering::SeqCst);
                    state.clear();
                }
            }),
            Intent::RefuseNotLocal => shared.publish_after(|state| {
                shared.request.fetch_add(1, Ordering::SeqCst);
                state.opening = false;
                state.checking = false;
                state.error = Some(LoadError::NotLocal);
            }),
        }
        true
    }

    /// The library as it is now, for a command.
    pub fn view(&self) -> View {
        self.shared.with_state(|state| View {
            current: state.current.clone(),
            requested: state.requested.clone(),
            generation: state.generation,
            opening: state.opening,
            checking: state.checking,
            error: state.error,
        })
    }

    /// The open library, for searching and citing.
    pub fn current(&self) -> Option<Arc<Current>> {
        self.shared.with_state(|state| state.current.clone())
    }

    /// Status and the searchable library captured under the same owner lock.
    pub fn status_with_current(&self) -> (LibraryStatus, Option<Arc<Current>>) {
        self.shared
            .with_state(|state| (status::status(state), state.current.clone()))
    }

    /// The status as it is now, for an interface that has just started.
    pub fn status(&self) -> LibraryStatus {
        self.shared.with_state(|state| status::status(state))
    }

    /// Wait until no read is running or waiting, at most `timeout`, for a
    /// client that needs the outcome before going on. Returns whether it is.
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let queue = self.shared.queue.recover_lock();
        let (queue, _) = ibis_os::sync::recover(
            self.shared
                .idle
                .wait_timeout_while(queue, timeout, |queue| queue.running),
        );
        !queue.running
    }

    /// How many requests have been made, so a caller can tell whether a
    /// newer one arrived while it waited.
    pub fn requests(&self) -> u64 {
        self.shared.request.load(Ordering::SeqCst)
    }

    /// Publish the status again, for example when the app's own notice changed.
    pub fn republish(&self) {
        self.shared.publish_after(|_| {});
    }
}

impl State {
    fn source(&self) -> Option<&Source> {
        self.current.as_ref().map(|current| &current.source)
    }
}

/// The Zotero library to read again: the one asked for, even if it has not
/// opened yet. A failed file choice stays visible until the next request, so
/// a retained Zotero library is not checked then.
fn zotero_to_check(state: &State) -> Option<Source> {
    match state.requested.as_ref().or(state.source())? {
        source @ Source::Zotero(_) => Some(source.clone()),
        Source::File(_) | Source::Samples(_) => None,
    }
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.recover_lock()
    }

    fn with_state<T>(&self, work: impl FnOnce(&mut State) -> T) -> T {
        work(&mut self.lock())
    }

    fn is_current(&self, request: u64) -> bool {
        self.request.load(Ordering::SeqCst) == request
    }

    /// Change the state and publish the result under the same lock, so an
    /// older completion cannot publish after a newer request started.
    fn publish_after(&self, change: impl FnOnce(&mut State)) {
        let mut state = self.lock();
        change(&mut state);
        (self.publish)(status::status(&state));
    }

    fn publish_locked(&self, state: &State, reveal: bool) {
        let mut status = status::status(state);
        status.reveal = reveal;
        (self.publish)(status);
    }

    /// Record the request before any background work can start or complete.
    fn record_open(&self, state: &mut State, source: Source) -> (u64, bool) {
        let request = self.request.fetch_add(1, Ordering::SeqCst) + 1;
        state.advance_generation();
        let reload = state.source() == Some(&source);
        state.observe_revision(SystemTime::now());
        state.retire_citations();
        state.requested = Some(source);
        state.opening = true;
        state.checking = false;
        state.error = None;
        state.retained_error = None;
        (request, reload)
    }

    /// Start checking the open library without a busy state: the picker keeps
    /// its state until the source answers. A newer choice discards the check.
    fn begin_check(&self, state: &mut State) -> Option<(u64, Source)> {
        let current = state.current.clone()?;
        // A different request is a choice in progress or a failed one that must stay visible.
        let other_request = state
            .requested
            .as_ref()
            .is_some_and(|requested| requested != &current.source);
        if state.opening || state.checking || other_request {
            return None;
        }
        // A supervised read may fail before the engine records source health.
        // A nonlocal dialog refusal says nothing about the installed source.
        let failed_read = state.error.is_some_and(|error| error != LoadError::NotLocal);
        let eligible = match current.source {
            Source::Zotero(_) => true,
            Source::File(_) => failed_read || needs_check(&current.engine),
            Source::Samples(_) => false,
        };
        if !eligible {
            return None;
        }
        state.checking = true;
        Some((self.request.fetch_add(1, Ordering::SeqCst) + 1, current.source.clone()))
    }

    fn begin_zotero_retry(&self, state: &mut State, only_failed: bool) -> Option<(u64, Source, bool)> {
        if state.opening || state.checking || (only_failed && state.error.is_none()) {
            return None;
        }
        let source = zotero_to_check(state)?;
        let reload = state.source() == Some(&source);
        let request = if reload {
            // A retained library stays visible, with its failure, until Zotero answers.
            state.checking = true;
            self.request.fetch_add(1, Ordering::SeqCst) + 1
        } else {
            // Eligibility and reservation share the lock: focus and show
            // cannot both start recovery of a choice that never opened.
            self.record_open(state, source.clone()).0
        };
        Some((request, source, reload))
    }

    fn run(self: &Arc<Self>, request: u64, source: Source, reload: bool, intent: OpenIntent, profile: Option<String>) {
        if intent != OpenIntent::Refresh {
            self.publish_after(|_| {});
        }
        let open = OpenRequest {
            request,
            source,
            reload,
            intent,
            profile,
            cancel: OpenCancellation::default(),
        };
        if !self.enqueue(open) {
            return;
        }
        let shared = Arc::clone(self);
        let started = std::thread::Builder::new().name("ibis-library".into()).spawn(move || {
            while let Some(open) = shared.next_open() {
                shared.open_one(open);
            }
        });
        if started.is_err() {
            let mut queue = self.queue.recover_lock();
            queue.running = false;
            queue.pending = None;
            self.idle.notify_all();
        }
    }

    fn enqueue(&self, open: OpenRequest) -> bool {
        let mut queue = self.queue.recover_lock();
        // Reservation and enqueue can race on different threads.
        if !self.is_current(open.request) {
            return false;
        }
        if let Some(cancel) = &queue.active {
            cancel.cancel();
        }
        queue.pending = Some(open);
        let start = !queue.running;
        queue.running = true;
        start
    }

    fn next_open(&self) -> Option<OpenRequest> {
        let mut queue = self.queue.recover_lock();
        let next = queue.pending.take();
        queue.active = next.as_ref().map(|open| open.cancel.clone());
        if next.is_none() {
            queue.running = false;
            self.idle.notify_all();
        }
        next
    }

    fn open_one(self: &Arc<Self>, open: OpenRequest) {
        let OpenRequest {
            request,
            source,
            reload,
            intent,
            profile,
            cancel,
        } = open;
        let previous = self.with_state(|state| {
            (reload && state.source() == Some(&source))
                .then(|| state.current.clone())
                .flatten()
        });
        let shared = Arc::clone(self);
        let read_source = source.clone();
        let read_profile = profile.clone();
        let token = cancel.token.clone();
        let in_flight = if matches!(source, Source::Zotero(_)) {
            &self.zotero_reads
        } else {
            &self.file_reads
        };
        let loaded = supervise(&source, &cancel, LOAD_DEADLINE, in_flight, move || {
            let profile = match (&read_source, read_profile) {
                (Source::Zotero(_), Some(profile)) => Some(profile),
                (Source::Zotero(_), None) if intent != OpenIntent::ReadOnly => shared
                    .store
                    .zotero_profile()
                    .map_err(|_| LoadError::unavailable(true))?,
                _ => None,
            };
            let force = intent.rereads_source(&read_source, reload);
            shared.load(read_source, force, previous, profile.as_deref(), &token, request)
        });
        let chosen = remembered_choice(&source, &loaded);
        if !self.install(intent, request, loaded) {
            return;
        }
        // A running Zotero answers for whichever profile it opened, so only a
        // closed read proves the name. Nothing else may save it.
        let profile = profile.filter(|_| {
            self.with_state(|state| state.current.clone())
                .is_some_and(|current| current.engine.zotero_read_closed())
        });
        let mut reveal = false;
        if let Some(chosen) = chosen.filter(|chosen| intent.saves_choice(&source, chosen)) {
            reveal = self.save_choice(request, intent, &source, chosen, profile);
        }
        {
            let state = self.lock();
            self.publish_locked(&state, reveal);
        }
        // Reattach even after a failed choice so the retained source keeps updating.
        self.watch_current();
    }

    /// Save the choice for this request only, never for a newer one. Returns
    /// whether it could not be saved, so the note must be seen.
    fn save_choice(
        &self,
        request: u64,
        intent: OpenIntent,
        source: &Source,
        chosen: LibrarySetting,
        profile: Option<String>,
    ) -> bool {
        let accepted = |state: &State| self.is_current(request) && state.error.is_none();
        if !self.with_state(|state| accepted(state)) {
            return false;
        }
        // Settings are written without the lock used by commands; the request
        // is checked again inside the settings turn, so a newer choice saves after this one.
        let saved = self.store.save(&mut |settings| {
            if self.is_current(request)
                && (intent == OpenIntent::Remember || settings.portable.library == source.setting())
            {
                settings.portable.library = Some(chosen.clone());
                if profile.is_some() {
                    settings.device.zotero_profile.clone_from(&profile);
                }
            }
        });
        self.with_state(|state| {
            if !accepted(state) {
                return false;
            }
            match saved {
                Ok(()) => {
                    state.choice_save_warning = None;
                    false
                }
                Err(error) => {
                    state.choice_save_warning = Some(words("app-library-not-saved", &[("error", &error)]));
                    true
                }
            }
        })
    }

    /// Read `source` into a new library, or check the open one again.
    fn load(
        &self,
        source: Source,
        force: bool,
        previous: Option<Arc<Current>>,
        profile: Option<&str>,
        cancel: &Cancellation,
        request: u64,
    ) -> Result<Loaded, LoadError> {
        let stopped = || cancel.is_cancelled() || !self.is_current(request);
        if stopped() {
            return Err(LoadError::Stopped);
        }
        let engine = || (self.engine)().map_err(|_| LoadError::Stopped);
        let loaded = match (source, previous) {
            (Source::File(path), Some(current)) => {
                if !force {
                    // An unreadable snapshot has no revision and never counts as unchanged.
                    return if current.revision().is_empty() {
                        Err(LoadError::FILE_UNAVAILABLE)
                    } else {
                        Ok(Loaded::Unchanged)
                    };
                }
                let prepared = current
                    .engine
                    .prepare_file_reload(&path, cancel)
                    .map_err(LoadError::file_refresh)?;
                Loaded::Reload(current, Box::new(Prepared::File(prepared)))
            }
            (Source::Zotero(chosen), Some(current)) => {
                let expected = Arc::clone(&current);
                let prepared = current
                    .engine
                    .prepare_zotero_reload_guarded(force, Some(chosen.libraries()), profile, cancel, |retire| {
                        // Detecting a full read retires old citations at once,
                        // but only while this request still owns the library.
                        let owned = self.is_current(request)
                            && self.with_state(|state| {
                                state.current.as_ref().is_some_and(|open| Arc::ptr_eq(open, &expected))
                            });
                        if !owned {
                            return Err(EngineError::new(ErrorCode::Cancelled));
                        }
                        retire()
                    })
                    .map_err(LoadError::Zotero)?;
                Loaded::Reload(current, Box::new(Prepared::Zotero(prepared)))
            }
            (source, _) => Loaded::New(Arc::new(Current::open(engine()?, source, profile, cancel)?)),
        };
        if stopped() {
            return Err(LoadError::Stopped);
        }
        Ok(loaded)
    }

    /// Make the outcome current, or record why it failed. Returns false when
    /// a newer request has been made since this one.
    fn install(&self, intent: OpenIntent, request: u64, loaded: Result<Loaded, LoadError>) -> bool {
        let mut state = self.lock();
        if !self.is_current(request) {
            return false;
        }
        let automatic = intent == OpenIntent::Refresh;
        let now = SystemTime::now();
        match loaded {
            Ok(Loaded::Unchanged) => {
                // An explicit request starts afresh: a remembered choice is saved again afterwards.
                if !automatic {
                    state.choice_save_warning = None;
                }
                state.opening = false;
                state.checking = false;
                state.error = None;
                state.checked_at = Some(now);
                state.observe_revision(now);
                state.requested = state.source().cloned();
            }
            Ok(Loaded::Reload(current, prepared)) => {
                if !state.current.as_ref().is_some_and(|open| Arc::ptr_eq(open, &current)) {
                    return false;
                }
                state.opening = false;
                state.checking = false;
                match accept(&current, *prepared) {
                    Ok(()) => {
                        state.error = None;
                        state.retained_error = None;
                        state.requested = state.source().cloned();
                        state.checked_at = Some(now);
                        state.observe_revision(now);
                        if !automatic {
                            state.choice_save_warning = None;
                        }
                    }
                    Err(error) => state.error = Some(error),
                }
            }
            loaded => {
                // A source refresh is not a successful opening of the rejected
                // choice: keep that choice visible while its retained source refreshes.
                let failed_choice = automatic
                    && state.error.is_some()
                    && state.current.is_some()
                    && state.requested.is_some()
                    && state.requested.as_ref() != state.source();
                match loaded {
                    Ok(Loaded::New(current)) => {
                        let same_source = state.source() == Some(&current.source);
                        if !state.current.as_ref().is_some_and(|open| Arc::ptr_eq(open, &current)) {
                            state.observer = None;
                        }
                        if !same_source {
                            state.changed_at = None;
                            state.observed_revision = None;
                        }
                        state.current = Some(current);
                        state.opening = false;
                        state.checking = false;
                        state.checked_at = Some(now);
                        state.observe_revision(now);
                        if !failed_choice {
                            state.requested = None;
                            state.error = None;
                            // A refresh never saves the choice again, so a "not
                            // saved for next time" warning still holds after it.
                            if !(automatic && same_source) {
                                state.choice_save_warning = None;
                            }
                        }
                        state.retained_error = None;
                    }
                    Err(error) => {
                        state.opening = false;
                        state.checking = false;
                        if failed_choice {
                            state.retained_error = Some(error);
                        } else {
                            state.error = Some(error);
                            state.retained_error = None;
                        }
                    }
                    Ok(Loaded::Unchanged | Loaded::Reload(..)) => unreachable!("matched above"),
                }
                state.advance_generation();
            }
        }
        true
    }

    /// Publish each change the engine reports for the open library until
    /// another library replaces it.
    fn watch_current(self: &Arc<Self>) {
        let Some((current, events)) = self.with_state(|state| {
            let current = state.current.clone()?;
            if current
                .observing
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                return None;
            }
            let Ok(events) = current.engine.subscribe() else {
                current.observing.store(false, Ordering::SeqCst);
                return None;
            };
            state.observer = Some(events.close_handle());
            // Subscribe before reconciling: earlier changes have no queued event.
            engine_changed(state, &current);
            self.publish_locked(state, false);
            Some((current, events))
        }) else {
            return;
        };
        let shared = Arc::clone(self);
        let _ = std::thread::Builder::new()
            .name("ibis-library-events".into())
            .spawn(move || {
                while events.recv().is_ok() {
                    let mut state = shared.lock();
                    if !engine_changed(&mut state, &current) {
                        return;
                    }
                    shared.publish_locked(&state, false);
                }
            });
    }
}

/// Whether a file library cannot be watched, or its last refresh failed.
fn needs_check(engine: &Engine) -> bool {
    engine.libraries().is_ok_and(|info| {
        info.libraries
            .iter()
            .any(|info| info.watch_warning.is_some() || info.refresh_error.is_some())
    })
}

/// Apply an engine notification for `current`, accounting for each snapshot
/// once. Returns whether `current` is still the open library.
fn engine_changed(state: &mut State, current: &Arc<Current>) -> bool {
    if !state.current.as_ref().is_some_and(|open| Arc::ptr_eq(open, current)) {
        return false;
    }
    let healthy = current
        .engine
        .libraries()
        .is_ok_and(|info| !info.libraries.is_empty() && info.libraries.iter().all(|info| info.refresh_error.is_none()));
    if healthy
        && state
            .requested
            .as_ref()
            .is_none_or(|requested| Some(requested) == state.source())
        && state.error != Some(LoadError::NotLocal)
    {
        state.error = None;
        state.retained_error = None;
    }
    let now = SystemTime::now();
    if state.observe_revision(now) || healthy {
        state.checked_at = Some(now);
    }
    true
}

fn accept(current: &Current, prepared: Prepared) -> Result<(), LoadError> {
    match prepared {
        Prepared::File(prepared) => match current.engine.accept_file_reload(prepared) {
            // The caller holds the current request, so a newer accepted read
            // (usually the watcher's) retired this one: the library is fresh.
            Err(error) if error.code == ErrorCode::Cancelled => Ok(()),
            result => result.map_err(LoadError::file_refresh),
        },
        Prepared::Zotero(prepared) => current.engine.accept_zotero_reload(prepared).map_err(LoadError::Zotero),
    }
}

/// Read the chosen Zotero libraries into a new engine.
fn open_zotero(
    engine: &Engine,
    chosen: &ZoteroLibraries,
    profile: Option<&str>,
    cancel: &Cancellation,
) -> Result<(), LoadError> {
    let request = json!({"action":"open-zotero","libraries":chosen.libraries(),"profile":profile});
    engine
        .dispatch(Method::Libraries, &request, cancel)
        .map_err(LoadError::Zotero)?;
    let info = engine.libraries().map_err(LoadError::Zotero)?;
    match info.libraries.iter().find_map(|info| {
        info.refresh_error.map(|code| EngineError {
            code,
            reason: info.refresh_reason,
            detail: None,
            strict_cause: None,
            citation_error: None,
        })
    }) {
        Some(failure) => Err(LoadError::Zotero(failure)),
        None => Ok(()),
    }
}

/// The choice a successful read saves: the path the read resolved, so the
/// saved choice names what was read even if the chosen path later moves.
fn remembered_choice(source: &Source, loaded: &Result<Loaded, LoadError>) -> Option<LibrarySetting> {
    if let Ok(Loaded::Reload(_, prepared)) = loaded
        && let Prepared::File(prepared) = &**prepared
    {
        return prepared
            .source_path()
            .map(|path| LibrarySetting::Bibtex(path.to_path_buf()));
    }
    if let Ok(Loaded::New(current)) = loaded
        && matches!(current.source, Source::File(_))
        && let Some(path) = current.source_path.as_deref()
        && is_bib(path)
    {
        return Some(LibrarySetting::Bibtex(path.to_path_buf()));
    }
    source.setting()
}

fn is_bib(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("bib"))
}

/// Run `operation` on its own thread until it answers, `cancel` fires or the
/// deadline passes. A driver call cannot be killed safely, so abandoned reads
/// are bounded instead: at most four of each kind per owner, leaving room for a
/// local file to open while a remote one is stuck.
fn supervise<T: Send + 'static>(
    source: &Source,
    cancel: &OpenCancellation,
    deadline: Duration,
    in_flight: &Pool,
    operation: impl FnOnce() -> Result<T, LoadError> + Send + 'static,
) -> Result<T, LoadError> {
    let zotero = matches!(source, Source::Zotero(_));
    supervise_with(cancel, deadline, in_flight, LoadError::unavailable(zotero), operation)
}

fn supervise_with<T: Send + 'static>(
    cancel: &OpenCancellation,
    deadline: Duration,
    in_flight: &Pool,
    unavailable: LoadError,
    operation: impl FnOnce() -> Result<T, LoadError> + Send + 'static,
) -> Result<T, LoadError> {
    let check = || cancel.token.is_cancelled();
    in_flight
        .run(&Scope::new(&check, deadline), Admission::Immediate, operation)
        .map_err(|error| match error {
            ibis_os::file_io::Error::TimedOut => {
                cancel.cancel();
                unavailable
            }
            ibis_os::file_io::Error::Capacity => unavailable,
            _ => LoadError::Stopped,
        })?
}
