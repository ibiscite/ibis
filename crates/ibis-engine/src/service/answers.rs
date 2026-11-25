//! The engine function behind each row of the method table (decision 0057):
//! written by hand, routed by the table.
use super::{
    Cancellation, DispatchResult, Engine, EntryResult, FindInResult, LibrariesResult, OutlineResult, ProjectResult,
    ReadResult, Reply, SearchResult, ShareResult, StylesResult, error, health,
};
use crate::Method;
use crate::outputs::OutputsResult;
use crate::request::*;
use ibis_core::contract::{
    CitationRequest, CitationResult, CitationWarningCode, EngineError, ErrorCode, ExitCode, SnapshotObject,
};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;

type Answer<T> = Result<DispatchResult<T>, EngineError>;
type Checked<T> = Answer<health::Checked<T>>;

pub(crate) fn libraries(engine: &Engine, request: LibrariesRequest, cancel: &Cancellation) -> Checked<LibrariesResult> {
    let (profiles, groups) = match request {
        LibrariesRequest::List {} => (None, None),
        LibrariesRequest::OpenZotero {
            libraries,
            profile,
            reload,
        } => {
            engine.read_zotero(reload, libraries.as_deref(), profile.as_deref(), cancel)?;
            (None, None)
        }
        LibrariesRequest::ZoteroProfiles {} => (Some(engine.zotero_profiles().unwrap_or_default()), None),
        LibrariesRequest::ZoteroGroups {} => (None, Some(engine.zotero_groups()?)),
        LibrariesRequest::OpenFile { path } => {
            engine.open_file(&path, cancel)?;
            (None, None)
        }
        LibrariesRequest::OpenSource { name, library } => {
            engine.open_source(&name, library.as_deref(), cancel)?;
            (None, None)
        }
        LibrariesRequest::Reload { library } => {
            engine.reload_files(library.as_ref(), cancel)?;
            (None, None)
        }
    };
    let mut listed = engine.libraries_checked()?;
    listed.value.zotero_installed = profiles.is_some().then(ibis_sources::zotero_source::installed);
    listed.value.zotero_profiles = profiles;
    listed.value.zotero_groups = groups;
    cancel.check()?;
    checked(listed, ExitCode::Ok)
}

pub(crate) fn search(engine: &Engine, request: SearchRequest, cancel: &Cancellation) -> Answer<SearchResult> {
    let result = engine.search(&request, cancel)?;
    let source_findings = !result.source_health.is_ok();
    DispatchResult {
        data: result,
        exit_code: if source_findings {
            ExitCode::Findings
        } else {
            ExitCode::Ok
        },
        text: None,
        source_findings,
    }
    .bounded()
}

pub(crate) fn show(engine: &Engine, request: ShowRequest, _: &Cancellation) -> Checked<SnapshotObject> {
    checked(engine.show_checked(&request)?, ExitCode::Ok)
}

pub(crate) fn cite(engine: &Engine, request: CitationRequest, cancel: &Cancellation) -> Checked<CitationResult> {
    let checked = engine.prepare_citation(&request, cancel)?.into_checked();
    DispatchResult::citation(checked, request.items.len(), &request.output)
}

pub(crate) fn records(engine: &Engine, request: RecordsRequest, cancel: &Cancellation) -> Checked<CitationResult> {
    let count = request.items.as_ref().map_or(request.records.len(), Vec::len);
    // No library is read, so no library's health bears on it. A field that
    // could not be read as sent is a finding, as it is in a library file, so
    // the citation never reads as clean.
    let result = engine.cite_records(&request, cancel)?;
    let unread = result
        .warnings
        .iter()
        .any(|warning| warning.code == CitationWarningCode::SourceField);
    let mut dispatched = DispatchResult::citation(
        health::Checked {
            value: result,
            source_health: health::SourceHealth::of(std::iter::empty()),
        },
        count,
        &request.output,
    )?;
    if unread {
        dispatched.exit_code = ExitCode::Findings;
    }
    Ok(dispatched)
}

pub(crate) fn project(engine: &Engine, request: ProjectRequest, cancel: &Cancellation) -> Checked<ProjectResult> {
    let result = engine.project(&request, cancel)?;
    let exit_code = if result.value.settled() {
        ExitCode::Ok
    } else {
        ExitCode::Findings
    };
    checked(result, exit_code)
}

pub(crate) fn entry(engine: &Engine, request: EntryRequest, cancel: &Cancellation) -> Checked<EntryResult> {
    let result = engine.entry_checked(&request, cancel)?;
    let exit_code = match result.value {
        EntryResult::Entry { .. } => ExitCode::Ok,
        EntryResult::Refused { .. } => ExitCode::Findings,
    };
    checked(result, exit_code)
}

pub(crate) fn outline(engine: &Engine, request: OutlineRequest, cancel: &Cancellation) -> Checked<OutlineResult> {
    checked(engine.outline(&request, cancel)?, ExitCode::Ok)
}

pub(crate) fn read(engine: &Engine, request: ReadRequest, cancel: &Cancellation) -> Checked<ReadResult> {
    checked(engine.read_attachment(&request, cancel)?, ExitCode::Ok)
}

pub(crate) fn find_in(engine: &Engine, request: FindInRequest, cancel: &Cancellation) -> Checked<FindInResult> {
    checked(engine.find_in(&request, cancel)?, ExitCode::Ok)
}

pub(crate) fn share(engine: &Engine, request: ShareRequest, cancel: &Cancellation) -> Checked<ShareResult> {
    let result = engine.share(&request, cancel)?;
    // A reference left out of a file it was asked for is a finding.
    let exit_code = if result.value.complete() {
        ExitCode::Ok
    } else {
        ExitCode::Findings
    };
    checked(result, exit_code)
}

pub(crate) fn styles(engine: &Engine, request: StylesRequest, cancel: &Cancellation) -> Answer<StylesResult> {
    catalogue(engine.styles(&request)?, cancel)
}

pub(crate) fn outputs(_: &Engine, OutputsRequest {}: OutputsRequest, cancel: &Cancellation) -> Answer<OutputsResult> {
    catalogue(crate::outputs::list(), cancel)
}

/// One method's contract, as `schema` lists it.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct MethodSchema {
    pub method: Method,
    /// What an open library must offer for the method to have anything to work on.
    pub needs: crate::method::Needs,
    /// Whether answering may reach the network.
    pub network: crate::method::Network,
    /// The selections the picker's ⌘K list offers the method for.
    pub palette: &'static [crate::method::Selection],
    /// The contract version these schemas describe.
    pub contract: u8,
    /// The JSON Schema of the method's parameters.
    #[schemars(with = "schemars::Schema")]
    pub params: Value,
    /// The JSON Schema of the method's result.
    #[schemars(with = "schemars::Schema")]
    pub result: Value,
}

pub(crate) fn schema(_: &Engine, request: SchemaRequest, cancel: &Cancellation) -> Answer<Vec<MethodSchema>> {
    let methods = request
        .method
        .map_or_else(|| Method::ALL.to_vec(), |method| vec![method]);
    let schemas = methods
        .into_iter()
        .map(|method| MethodSchema {
            method,
            needs: method.needs(),
            network: method.network(),
            palette: method.palette(),
            contract: ibis_core::contract::CONTRACT_VERSION,
            params: (method.row().params)(),
            result: (method.row().result)(),
        })
        .collect();
    catalogue(schemas, cancel)
}

pub(crate) fn call(engine: &Engine, request: CallRequest, cancel: &Cancellation) -> Answer<Reply> {
    if request.method == Method::Call {
        return Err(error(ErrorCode::BadInput));
    }
    engine.reply(request.method, &request.params, cancel)
}

/// A result and its sources' health, as dispatch sends them.
fn checked<T: Serialize>(result: health::Checked<T>, exit_code: ExitCode) -> Checked<T> {
    let source_findings = !result.source_health.is_ok();
    DispatchResult {
        data: result,
        exit_code: if source_findings && exit_code == ExitCode::Ok {
            ExitCode::Findings
        } else {
            exit_code
        },
        text: None,
        source_findings,
    }
    .bounded()
}

/// Styles, outputs and schemas read no library, so no library's health bears on them.
fn catalogue<T: Serialize>(data: T, cancel: &Cancellation) -> Answer<T> {
    cancel.check()?;
    DispatchResult {
        data,
        exit_code: ExitCode::Ok,
        text: None,
        source_findings: false,
    }
    .bounded()
}
