//! Native command intents shared by the socket client and the in-process CLI.
//! Public RPC connections go straight to the engine's method dispatcher.
use crate::{Failure, app_libraries, daemon, keys, settings_backup, sources, writings};
use ibis_core::contract::{CitationResult, EngineError, ErrorCode, ErrorReason, LibraryId, Problem, SnapshotObject};
use ibis_engine::health::Checked;
use ibis_engine::request::{KeyCitationRequest, ShowRequest, ZoteroLibrary};
use ibis_engine::settings::{Owner, Watching, backup, writings::Writing};
use ibis_engine::{
    Cancellation, DispatchResult, Engine, FileLoads, Formatting, LibrariesResult, LibraryInfo, Method, Reply,
    SourceWarning, WritingCitation,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

pub(super) fn engine() -> Result<Engine, EngineError> {
    let engine = Engine::new()?;
    Ok(match std::env::current_exe() {
        Ok(program) => engine
            .with_pdf_reader(program.clone())
            .with_document_reader(program.clone())
            .with_built_in_sources(program),
        Err(_) => engine,
    })
}

/// Clients that ask for the same selection reuse its index. Different
/// selections have independent lookup/health state and share parsed files.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Selection {
    pub files: Vec<PathBuf>,
    pub adapters: Vec<String>,
    pub adapter_directory: Option<PathBuf>,
    pub zotero: Vec<ZoteroLibrary>,
    pub profile: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "kind")]
pub(super) enum Intent {
    Select {
        selection: Selection,
        transient: bool,
        read_text: bool,
    },
    Method {
        method: Method,
        params: Value,
    },
    OpenFile {
        path: PathBuf,
    },
    OpenSource {
        name: String,
        library: Option<String>,
    },
    KeepFolder {
        nightly: bool,
        folder: PathBuf,
        skip: BTreeSet<String>,
    },
    CiteKeys {
        request: KeyCitationRequest,
        writing: Option<Writing>,
    },
    KeyAdvice {
        key: String,
        reason: Option<ErrorReason>,
    },
    Edit {
        nightly: bool,
        edit: writings::Edit,
    },
    Restore {
        nightly: bool,
        plan: Box<backup::Plan>,
    },
    StartMcp {
        allow_writes: bool,
        library_problem: Option<EngineError>,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Citation {
    Plain(Box<Checked<CitationResult>>),
    Project(Box<WritingCitation>),
}

#[derive(Serialize, Deserialize)]
struct MethodReply {
    result: DispatchResult<Reply>,
    citation_text: bool,
}

impl MethodReply {
    fn new(mut result: DispatchResult<Reply>) -> Self {
        // One-shot CLI output has never described the ability to watch a file.
        // The stdio and socket servers still publish that status to clients.
        if let Reply::Libraries(libraries) = &mut result.data {
            for library in &mut libraries.value.libraries {
                library.watch_warning = None;
            }
        }
        let citation_text =
            matches!(&result.data, Reply::Citation(citation) if result.text.as_ref() == Some(&citation.value.text));
        if citation_text {
            result.text = None;
        }
        Self { result, citation_text }
    }

    fn into_result(mut self) -> DispatchResult<Reply> {
        if self.citation_text
            && let Reply::Citation(citation) = &self.result.data
        {
            self.result.text = Some(citation.value.text.clone());
        }
        self.result
    }
}

#[derive(Default, Serialize, Deserialize)]
pub(super) struct KeyAdvice {
    pub warnings: Vec<SourceWarning>,
    pub suggestions: Vec<String>,
}

fn key_advice(engine: &Engine, key: &str, reason: Option<ErrorReason>) -> KeyAdvice {
    let libraries = engine.libraries().map(|result| result.libraries).unwrap_or_default();
    let warnings = match reason {
        Some(ErrorReason::DuplicateKey) => libraries
            .iter()
            .filter_map(|library| library.duplicate_keys.get(key))
            .flatten()
            .cloned()
            .collect(),
        Some(ErrorReason::DamagedEntry) => libraries
            .iter()
            .filter_map(|library| library.damaged_keys.get(key))
            .cloned()
            .collect(),
        _ => vec![],
    };
    let suggestions = if reason.is_none() {
        keys::suggestions(engine, key)
    } else {
        Vec::new()
    };
    KeyAdvice { warnings, suggestions }
}

fn cite_keys(
    engine: &Engine,
    request: &KeyCitationRequest,
    writing: Option<&Writing>,
    cancel: &Cancellation,
) -> Result<Citation, EngineError> {
    match writing {
        Some(writing) => engine
            .cite_writing_keys(writing, request, cancel)
            .map(|result| Citation::Project(Box::new(result))),
        None => engine
            .cite_keys_checked(request, cancel)
            .map(|result| Citation::Plain(Box::new(result))),
    }
}

pub(super) struct Host {
    rpc: Engine,
    formatting: Formatting,
    files: Arc<FileLoads>,
    scopes: Mutex<Vec<(Selection, Engine)>>,
    settings: Mutex<BTreeMap<PathBuf, Profile>>,
    adapter_open: Mutex<()>,
    pub(crate) picker_history: std::sync::OnceLock<crate::daemon::picker::History>,
}

struct Profile {
    owner: Arc<Owner>,
    _watch: Option<Watching>,
}

impl Host {
    pub fn new() -> Result<Self, EngineError> {
        let formatting = Formatting::default();
        let files = Arc::new(FileLoads::default());
        let rpc = engine()?
            .with_formatting(formatting.clone())
            .with_file_loads(files.clone())
            .with_external_sources(sources::configuration().unwrap_or_default());
        let host = Self {
            rpc,
            formatting,
            files,
            scopes: Mutex::default(),
            settings: Mutex::default(),
            adapter_open: Mutex::default(),
            picker_history: std::sync::OnceLock::new(),
        };
        for nightly in [false, true] {
            if let Ok((_, path)) = settings_backup::profile_file(nightly)
                && crate::bounded(move || path.is_file() || path.with_extension("json.bak").is_file()) == Some(true)
            {
                let _ = host.settings(nightly);
            }
        }
        Ok(host)
    }

    pub fn rpc_engine(&self) -> Engine {
        self.rpc.clone()
    }

    pub fn cli_engine(&self) -> Result<Engine, EngineError> {
        Ok(engine()?
            .with_formatting(self.formatting.clone())
            .with_file_loads(self.files.clone()))
    }

    pub fn library_loads(&self) -> u64 {
        self.files.parses()
    }

    fn scope(&self, selection: Selection) -> Result<Engine, EngineError> {
        let mut scopes = locked(&self.scopes);
        let engine = if let Some(index) = scopes.iter().position(|(key, _)| *key == selection) {
            scopes.remove(index).1
        } else {
            self.cli_engine()?
        };
        if scopes.len() >= 4 {
            scopes.remove(0);
        }
        scopes.push((selection, engine.clone()));
        drop(scopes);
        Ok(engine)
    }

    fn settings(&self, nightly: bool) -> Result<Arc<Owner>, EngineError> {
        let (_, path) = settings_backup::profile_file(nightly).map_err(|failure| EngineError::new(failure.code()))?;
        Ok(self.settings_for(path, |owner| {
            let path = owner.path().to_path_buf();
            let clear = move |loaded: &ibis_engine::settings::Loaded| {
                if loaded
                    .cites()
                    .is_ok_and(|settings| !settings.portable.recent_citations())
                {
                    let _ = ibis_engine::recents::clear_disabled(&path);
                }
            };
            clear(&owner.view());
            owner.on_publish(clear);
        }))
    }

    pub(super) fn settings_for(&self, path: PathBuf, initialize: impl FnOnce(&Owner)) -> Arc<Owner> {
        locked(&self.settings)
            .entry(path.clone())
            .or_insert_with(|| {
                let owner = Arc::new(Owner::open(path));
                initialize(&owner);
                let watch = owner.watch(|_, _| {});
                Profile { owner, _watch: watch }
            })
            .owner
            .clone()
    }

    pub fn answer(&self, engine: &mut Engine, intent: Intent, cancel: &Cancellation) -> Result<Value, EngineError> {
        cancel.check()?;
        match intent {
            Intent::Select {
                selection,
                transient,
                read_text,
            } => {
                let adapters = !selection.adapters.is_empty();
                let directory = selection.adapter_directory.clone();
                let mut selected = if transient {
                    self.cli_engine()?.without_file_watches()
                } else {
                    self.scope(selection)?
                };
                if adapters || transient {
                    selected = selected
                        .with_external_sources(sources::configuration().unwrap_or_default())
                        .with_source_directory(directory);
                }
                if read_text {
                    selected = for_reading(selected);
                }
                *engine = selected;
                Ok(Value::Null)
            }
            Intent::Method { method, params } => encode(MethodReply::new(engine.reply(method, &params, cancel)?)),
            Intent::OpenFile { path } => encode(engine.open_file_id(&path, cancel)?),
            Intent::OpenSource { name, library } => {
                let _open = locked(&self.adapter_open);
                encode(engine.open_source(&name, library.as_deref(), cancel)?)
            }
            Intent::KeepFolder { nightly, folder, skip } => {
                let (_, settings) =
                    settings_backup::profile_file(nightly).map_err(|failure| EngineError::new(failure.code()))?;
                encode(keep_folder(engine, &settings, &folder, &skip, cancel))
            }
            Intent::CiteKeys { request, writing } => encode(cite_keys(engine, &request, writing.as_ref(), cancel)?),
            Intent::KeyAdvice { key, reason } => encode(key_advice(engine, &key, reason)),
            Intent::Edit { nightly, edit } => encode(
                edit.save(self.settings(nightly)?.as_ref())
                    .map_err(writings::SaveProblem::from),
            ),
            Intent::Restore { nightly, plan } => encode(
                self.settings(nightly)?
                    .restore(&plan)
                    .map(|_| ())
                    .map_err(settings_backup::RestoreProblem::from),
            ),
            Intent::StartMcp { .. } => Err(EngineError::new(ErrorCode::Usage)),
        }
    }
}

fn keep_folder(
    engine: &Engine,
    settings: &Path,
    folder: &Path,
    skip: &BTreeSet<String>,
    cancel: &Cancellation,
) -> Result<ibis_engine::write::Kept, Problem> {
    engine
        .keep_folder(&ibis_engine::write::Proposals::beside(settings), folder, skip, cancel)
        .map_err(|error| error.problem().clone())
}

fn encode(value: impl Serialize) -> Result<Value, EngineError> {
    serde_json::to_value(value).map_err(|_| EngineError::new(ErrorCode::Internal))
}

pub(super) enum Backend<'a> {
    Local(&'a Engine),
    Remote(Mutex<Remote>),
}

pub(super) struct Remote {
    pub connection: Option<daemon::Connection>,
    pub setup: Vec<Intent>,
}

fn locked<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn offline() -> EngineError {
    EngineError::new(ErrorCode::SourceUnavailable)
}

impl Remote {
    pub fn connect(&mut self) -> Result<&mut daemon::Connection, EngineError> {
        if self.connection.is_none() {
            let mut connection = daemon::Endpoint::client().map_err(|_| offline())?;
            for intent in &self.setup {
                request::<Value>(&mut connection, intent).map_err(|_| offline())??;
            }
            self.connection = Some(connection);
        }
        self.connection.as_mut().ok_or_else(offline)
    }

    fn call<T: DeserializeOwned>(&mut self, intent: &Intent, repeat: bool) -> Result<T, EngineError> {
        match request(self.connect()?, intent) {
            Ok(result) => result,
            Err(_) => {
                self.connection = None;
                if !repeat {
                    return Err(offline());
                }
                request(self.connect()?, intent).map_err(|_| offline())?
            }
        }
    }

    fn setup<T: DeserializeOwned>(&mut self, intent: Intent, repeat: bool) -> Result<T, EngineError> {
        let selected = self.call(&intent, repeat)?;
        self.setup.push(intent);
        Ok(selected)
    }
}

fn request<T: DeserializeOwned>(
    connection: &mut daemon::Connection,
    intent: &Intent,
) -> std::io::Result<Result<T, EngineError>> {
    connection.exchange(intent)
}

impl<'a> Backend<'a> {
    pub fn new(engine: &'a Engine, daemon: bool) -> Self {
        if daemon && let Ok(connection) = daemon::Endpoint::client() {
            return Self::Remote(Mutex::new(Remote {
                connection: Some(connection),
                setup: Vec::new(),
            }));
        }
        Self::Local(engine)
    }

    pub fn select(&self, selection: Selection, transient: bool, read_text: bool) -> Result<(), EngineError> {
        match self {
            Self::Local(_) => Ok(()),
            Self::Remote(remote) => locked(remote).setup(
                Intent::Select {
                    selection,
                    transient,
                    read_text,
                },
                true,
            ),
        }
    }

    pub fn reply(
        &self,
        method: Method,
        params: &Value,
        cancel: &Cancellation,
    ) -> Result<DispatchResult<Reply>, EngineError> {
        match self {
            Self::Local(engine) => engine.reply(method, params, cancel),
            Self::Remote(remote) => {
                let mut params = params.clone();
                absolute_params(method, &mut params)?;
                if method == Method::Libraries && params["action"] == "open-zotero" {
                    return locked(remote)
                        .setup(Intent::Method { method, params }, true)
                        .map(MethodReply::into_result);
                }
                let repeat = matches!(
                    method,
                    Method::Search
                        | Method::Show
                        | Method::Cite
                        | Method::Records
                        | Method::Styles
                        | Method::Outputs
                        | Method::Schema
                );
                locked(remote)
                    .call(&Intent::Method { method, params }, repeat)
                    .map(MethodReply::into_result)
            }
        }
    }

    pub fn libraries(&self) -> Result<LibrariesResult, EngineError> {
        match self {
            Self::Local(engine) => engine.libraries(),
            Self::Remote(_) => match self
                .reply(Method::Libraries, &Value::Null, &Cancellation::default())?
                .data
            {
                Reply::Libraries(result) => Ok(result.value),
                _ => Err(EngineError::new(ErrorCode::Internal)),
            },
        }
    }

    pub fn show(&self, request: &ShowRequest) -> Result<SnapshotObject, EngineError> {
        match self {
            Self::Local(engine) => engine.show(request),
            Self::Remote(_) => match self
                .reply(Method::Show, &encode(request)?, &Cancellation::default())?
                .data
            {
                Reply::Show(result) => Ok(result.value),
                _ => Err(EngineError::new(ErrorCode::Internal)),
            },
        }
    }

    pub fn open_file(&self, path: &Path, cancel: &Cancellation) -> Result<LibraryId, EngineError> {
        match self {
            Self::Local(engine) => engine.open_file_id(path, cancel),
            Self::Remote(remote) => locked(remote).setup(Intent::OpenFile { path: absolute(path)? }, true),
        }
    }

    pub fn open_source(
        &self,
        name: &str,
        library: Option<&str>,
        cancel: &Cancellation,
    ) -> Result<(LibraryInfo, String), EngineError> {
        match self {
            Self::Local(engine) => engine.open_source(name, library, cancel),
            Self::Remote(remote) => locked(remote).setup(
                Intent::OpenSource {
                    name: name.to_owned(),
                    library: library.map(str::to_owned),
                },
                false,
            ),
        }
    }

    pub fn keep_folder(
        &self,
        settings: &Path,
        folder: &Path,
        skip: &BTreeSet<String>,
        cancel: &Cancellation,
    ) -> Result<ibis_engine::write::Kept, Failure> {
        let result = match self {
            Self::Local(engine) => keep_folder(engine, settings, folder, skip, cancel),
            Self::Remote(remote) => locked(remote).call(
                &Intent::KeepFolder {
                    nightly: settings_backup::is_nightly(settings),
                    folder: absolute(folder)?,
                    skip: skip.clone(),
                },
                false,
            )?,
        };
        result.map_err(|problem| {
            Failure::new(
                ErrorCode::SourceUnavailable,
                ibis_engine::catalogue::problem(&problem, ibis_engine::catalogue::Audience::Cli),
            )
        })
    }

    pub fn cite_keys(
        &self,
        request: &KeyCitationRequest,
        writing: Option<&Writing>,
        cancel: &Cancellation,
    ) -> Result<Citation, EngineError> {
        match self {
            Self::Local(engine) => cite_keys(engine, request, writing, cancel),
            Self::Remote(remote) => locked(remote).call(
                &Intent::CiteKeys {
                    request: request.clone(),
                    writing: writing.cloned(),
                },
                true,
            ),
        }
    }

    pub fn key_advice(&self, key: &str, reason: Option<ErrorReason>) -> KeyAdvice {
        match self {
            Self::Local(engine) => key_advice(engine, key, reason),
            Self::Remote(remote) => locked(remote)
                .call(
                    &Intent::KeyAdvice {
                        key: key.to_owned(),
                        reason,
                    },
                    true,
                )
                .unwrap_or_default(),
        }
    }

    pub fn edit(&self, path: PathBuf, edit: writings::Edit) -> Result<Writing, Failure> {
        let nightly = settings_backup::is_nightly(&path);
        match self {
            Self::Local(_) => edit.save(&Owner::open(path)).map_err(writings::failure),
            Self::Remote(remote) => locked(remote)
                .call::<Result<Writing, writings::SaveProblem>>(&Intent::Edit { nightly, edit }, false)?
                .map_err(|error| writings::failure(error.into())),
        }
    }

    pub fn restore(&self, path: PathBuf, plan: &backup::Plan) -> Result<(), Failure> {
        let nightly = settings_backup::is_nightly(&path);
        match self {
            Self::Local(_) => Owner::open(path)
                .restore(plan)
                .map(|_| ())
                .map_err(|error| settings_backup::backup_failure(&error)),
            Self::Remote(remote) => locked(remote)
                .call::<Result<(), settings_backup::RestoreProblem>>(
                    &Intent::Restore {
                        nightly,
                        plan: Box::new(plan.clone()),
                    },
                    false,
                )?
                .map_err(|error| settings_backup::backup_failure(&error.into())),
        }
    }
}

fn absolute(path: &Path) -> Result<PathBuf, EngineError> {
    std::path::absolute(path).map_err(|_| EngineError::new(ErrorCode::BadInput))
}

pub(super) fn selection_paths(files: &[PathBuf]) -> Vec<PathBuf> {
    let mut paths: Vec<_> = files
        .iter()
        .map(|file| absolute(file).unwrap_or_else(|_| file.clone()))
        .collect();
    paths.sort_unstable();
    paths.dedup();
    paths
}

fn absolute_params(method: Method, params: &mut Value) -> Result<(), EngineError> {
    let fields: &[&str] = match method {
        Method::Call => {
            if let Some(method) = params["method"].as_str().and_then(|name| Method::parse(name).ok())
                && method != Method::Call
                && let Some(params) = params.get_mut("params")
            {
                absolute_params(method, params)?;
            }
            return Ok(());
        }
        Method::Libraries if params["action"] == "open-file" => &["path"],
        Method::Project => &["path"],
        Method::Check => &["document", "set"],
        Method::Share => &["destination"],
        _ => return Ok(()),
    };
    for &field in fields {
        if let Some(path) = params[field].as_str().filter(|path| !path.is_empty()) {
            params[field] = encode(absolute(Path::new(path))?)?;
        }
    }
    Ok(())
}

pub(super) fn for_reading(engine: Engine) -> Engine {
    let allowed = crate::bounded(|| app_libraries::format_settings(false))
        .and_then(Result::ok)
        .is_some_and(|settings| settings.device.attachment_text_outside_library);
    engine.with_attachment_text_outside_library(allowed)
}
