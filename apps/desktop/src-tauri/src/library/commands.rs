//! The library commands the interface calls.

use super::engine_library::FileLibrary;
use ibis_engine::library::{ChoiceStore, Intent, LibraryProblem, LibraryStatus, Owner};

use ibis_engine::settings::{ZoteroLibraries, ZoteroLibrary};
use serde::Serialize;
use tauri::{AppHandle, Manager, State, path::BaseDirectory};

use super::Source;
use super::search::{SearchQuery, SearchReply, SearchRequests};
use super::{choose_library_file, current_status, reload_library_file, start_open, use_zotero_library};
use crate::project::ProjectBibliography;
use ibis_engine::discovery::{self, ZoteroPresence};
use ibis_engine::{Cancellation, ZoteroGroup, health::InspectionPolicy};

/// Choose a library with the system file dialog. The interface never passes
/// file paths; the result arrives as the `library:changed` event.
#[tauri::command]
#[ibis_desktop_macros::command(main)]
pub(crate) fn choose_library(app: AppHandle) {
    choose_library_file(&app);
}

/// Read the personal Zotero library while Zotero runs; the result arrives as
/// `library:changed`.
#[tauri::command]
#[ibis_desktop_macros::command(main)]
pub(crate) fn use_zotero(app: AppHandle) {
    use_zotero_library(&app);
}

/// Samples are an explicit, temporary source, never a saved library preference.
#[tauri::command]
#[ibis_desktop_macros::command(main)]
pub(crate) fn use_sample_library(app: AppHandle, enabled: bool) -> Result<(), crate::errors::NativeFailure> {
    if enabled {
        let path = app
            .path()
            .resolve("resources/samples.csljson", BaseDirectory::Resource)
            .map_err(|_| crate::errors::NativeFailure)?;
        start_open(&app, Source::Samples(path), false);
    } else {
        app.state::<Owner>().submit(Intent::LeaveSamples);
    }
    Ok(())
}

/// Read these Zotero libraries together from now on, in this order; the
/// result arrives as `library:changed`. An empty list or a repeated library
/// is refused.
#[tauri::command]
#[ibis_desktop_macros::command(main)]
pub(crate) fn use_zotero_libraries(
    app: AppHandle,
    libraries: Vec<ZoteroLibrary>,
) -> Result<(), crate::errors::NativeFailure> {
    let chosen = ZoteroLibraries::new(libraries).ok_or(crate::errors::NativeFailure)?;
    // Chosen from the open picker: showing it again would move focus out of
    // the menu.
    start_open(&app, Source::Zotero(chosen), true);
    Ok(())
}

/// The group libraries Zotero knows: asked of Zotero while it runs, or as
/// listed by the closed read of the open library. Starts no read of its own.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn zotero_groups(library: State<'_, Owner>) -> Result<Vec<ZoteroGroup>, super::CommandError> {
    let file = library.current();
    tauri::async_runtime::spawn_blocking(move || match file {
        Some(file) => file.zotero_groups(),
        None => ibis_engine::Engine::new().and_then(|engine| super::engine_library::zotero_groups(&engine)),
    })
    .await
    .map_err(|_| super::CommandError::NotPrepared)?
    .map_err(super::CommandError::from)
}

/// The Zotero profiles on this computer and the one chosen, by name.
#[derive(Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct ZoteroProfiles {
    pub(crate) names: Vec<String>,
    pub(crate) chosen: Option<String>,
    /// Zotero is running, so it answers for the profile it opened and a choice
    /// here would change nothing on screen.
    pub(crate) running: bool,
}

/// The profiles to choose from for reading while Zotero is closed. Their
/// locations stay native.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn zotero_profiles(app: AppHandle) -> Result<ZoteroProfiles, super::CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let chosen = app.state::<crate::preferences::SettingsFile>().zotero_profile()?;
        let running = discovery::find_zotero(InspectionPolicy::Setup, false, &Cancellation::default())?
            != discovery::ZoteroPresence::Absent;
        super::engine_library::zotero_profiles().map(|names| ZoteroProfiles { names, chosen, running })
    })
    .await
    .map_err(|_| super::CommandError::NotPrepared)?
    .map_err(super::CommandError::from)
}

/// Read this Zotero profile from now on; the result arrives as
/// `library:changed`, and only a closed read that used it saves the choice.
#[tauri::command]
#[ibis_desktop_macros::command(main)]
pub(crate) fn use_zotero_profile(app: AppHandle, name: String) {
    super::use_zotero_profile(&app, name);
}

/// Whether to read Zotero again after asking it: it now shares, or it opened
/// without sharing and the screen waiting on it still says something else.
fn zotero_answered(found: ZoteroPresence, problem: Option<LibraryProblem>) -> bool {
    match found {
        ZoteroPresence::Available => true,
        ZoteroPresence::AccessOff => problem != Some(LibraryProblem::ZoteroAccessOff),
        ZoteroPresence::Closed | ZoteroPresence::Absent => false,
    }
}

/// Ask Zotero while a screen waiting on it is shown. Once it answers, the
/// failed choice is read again and the result arrives as `library:changed`.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn check_zotero_sharing(app: AppHandle) -> Result<ZoteroPresence, super::CommandError> {
    let found = tauri::async_runtime::spawn_blocking(|| {
        discovery::find_zotero(InspectionPolicy::Setup, false, &Cancellation::default())
    })
    .await
    .map_err(|_| super::CommandError::NotPrepared)?
    .map_err(super::CommandError::from)?;
    let problem = app.state::<Owner>().view().error.map(LibraryProblem::from);
    if zotero_answered(found, problem) {
        super::retry_zotero_on_focus(&app);
    }
    Ok(found)
}

/// Open the current library again; the result arrives as `library:changed`.
#[tauri::command]
#[ibis_desktop_macros::command(main)]
pub(crate) fn reload_library(app: AppHandle) {
    reload_library_file(&app);
}

/// The library as it is now, for the interface when it starts.
#[tauri::command]
#[ibis_desktop_macros::command(main)]
pub(crate) fn library_status(app: AppHandle) -> LibraryStatus {
    current_status(&app)
}

/// A possibly retired identity arriving from a desktop action.
pub(crate) struct RecentIdentity(pub(crate) Option<ibis_core::contract::ItemRef>);

/// Any value: the web view sends back whatever identity it was given.
impl schemars::JsonSchema for RecentIdentity {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "RecentIdentity".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!(true)
    }
}

/// History outlives identities: one that is no longer valid resolves to
/// nothing, the same as one whose library is gone.
impl<'de> serde::Deserialize<'de> for RecentIdentity {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(Self(serde_json::from_value(value).ok()))
    }
}

/// Current destination's recents; Forget retires in-flight recording.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn recent_references(
    forget: bool,
    app: AppHandle,
    library: State<'_, Owner>,
    project: State<'_, ProjectBibliography>,
    editor: State<'_, crate::editor_endpoint::Pending>,
) -> Result<(SearchReply, bool, bool), super::CommandError> {
    let captured = crate::paste::last_app(&app).map(|(id, _, _)| id.normalized());
    let captured = editor.app(captured);
    let file = library.current();
    let project = project.current();
    tauri::async_runtime::spawn_blocking(move || {
        let session = app.state::<crate::session_references::SessionReferences>();
        if forget {
            session
                .forget()
                .map_err(|_| super::CommandError::RecentHistoryNotSaved)?;
        }
        recent_rows(session.inner(), captured.as_deref(), file.as_deref(), project.as_ref())
    })
    .await
    .map_err(|_| super::CommandError::RecentReferencesFailed)?
}

/// Search the open library. An empty query returns no results. Async so it
/// runs off the main thread. `page` identifies the interface page and
/// `request` increases with every search it sends; a search older than that
/// page's newest one returns no results, which the interface ignores because
/// it belongs to an outdated query. `scope` contains exact loaded source IDs:
/// omitted/null searches all, an empty list searches none. It never reloads sources.
// Each managed state is its own argument: Tauri hands them to commands that way.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn search(
    query: SearchQuery,
    limit: usize,
    page: u32,
    request: u32,
    scope: Option<Vec<String>>,
    library: State<'_, Owner>,
    requests: State<'_, SearchRequests>,
    project: State<'_, ProjectBibliography>,
) -> Result<SearchReply, super::CommandError> {
    let view = library.view();
    let file = view.current.ok_or_else(|| {
        view.error.map_or(super::CommandError::NoLibrary, |error| {
            super::CommandError::Presented(error.problem())
        })
    })?;
    let Some(cancel) = requests.start(page, request) else {
        return Ok(SearchReply {
            revision: file.revision(),
            ..SearchReply::default()
        });
    };
    let project = project.current();
    tauri::async_runtime::spawn_blocking(move || {
        file.search(&query, limit, scope, &cancel)
            .map(|reply| file.mark_project(reply, project.as_ref()))
    })
    .await
    .map_err(|_| super::CommandError::SearchFailed)?
}

/// Resolve the destination's recents from the captured library and bibliography.
pub(crate) fn recent_rows(
    session: &crate::session_references::SessionReferences,
    captured: Option<&str>,
    file: Option<&ibis_engine::library::Current>,
    project: Option<&crate::project::Current>,
) -> Result<(SearchReply, bool, bool), super::CommandError> {
    let (ids, has_history, forget_failed) = {
        let history = session.lock();
        (
            history.ids(captured).to_vec(),
            history.has_history(),
            history.forget_failed,
        )
    };
    let reply = match file {
        Some(file) => file
            .recents(&ids, has_history)
            .map(|reply| file.mark_project(reply, project))?,
        None => SearchReply::default(),
    };
    Ok((reply, has_history, forget_failed))
}
