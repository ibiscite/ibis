//! The library commands the interface calls.

use std::sync::{Arc, Mutex};

use ibis_core::settings::{ZoteroLibraries, ZoteroLibrary};
use ibis_core::zotero::libraries;
use ibis_zotero_local::LocalZotero;
use serde::Serialize;
use tauri::{AppHandle, State};

use super::search::{SearchReply, SearchRequests, search_snapshot};
use super::source::Source;
use super::state::Library;
use super::status::LibraryStatus;
use super::tasks::{choose_library_file, current_status, reload_library_file, start_open, use_zotero_library};

/// Choose a library with the system file dialog. The interface never passes
/// file paths; the result arrives as the `library:changed` event.
#[tauri::command]
#[specta::specta]
pub(crate) fn choose_library(app: AppHandle) {
    choose_library_file(&app);
}

/// Read the personal Zotero library while Zotero runs; the result arrives as
/// `library:changed`.
#[tauri::command]
#[specta::specta]
pub(crate) fn use_zotero(app: AppHandle) {
    use_zotero_library(&app);
}

/// Read these Zotero libraries together from now on, in this order; the
/// result arrives as `library:changed`. An empty list or a repeated library
/// is refused.
#[tauri::command]
#[specta::specta]
pub(crate) fn use_zotero_libraries(app: AppHandle, libraries: Vec<ZoteroLibrary>) -> Result<(), String> {
    let chosen = ZoteroLibraries::new(libraries).ok_or("Choose at least one Zotero library, each once")?;
    // Chosen from the open picker: showing it again would move focus out of
    // the menu.
    start_open(&app, Source::Zotero(chosen), true);
    Ok(())
}

/// A Zotero group library the user can choose.
#[derive(Clone, Serialize, specta::Type)]
pub(crate) struct ZoteroGroup {
    pub(crate) id: u64,
    pub(crate) name: String,
}

/// The group libraries Zotero knows, while Zotero runs.
#[tauri::command]
#[specta::specta]
pub(crate) async fn zotero_groups() -> Result<Vec<ZoteroGroup>, String> {
    tauri::async_runtime::spawn_blocking(|| libraries::groups(&mut LocalZotero::default()))
        .await
        .map_err(|_| "listing Zotero groups stopped unexpectedly".to_string())?
        .map(|groups| {
            groups
                .into_iter()
                .map(|g| ZoteroGroup { id: g.id, name: g.name })
                .collect()
        })
        .map_err(|e| e.to_string())
}

/// Open the current library again; the result arrives as `library:changed`.
#[tauri::command]
#[specta::specta]
pub(crate) fn reload_library(app: AppHandle) {
    reload_library_file(&app);
}

/// The library as it is now, for the interface when it starts.
#[tauri::command]
#[specta::specta]
pub(crate) fn library_status(app: AppHandle) -> LibraryStatus {
    current_status(&app)
}

/// Search the open library. An empty query returns no results. Async so it
/// runs off the main thread. `page` identifies the interface page and
/// `request` increases with every search it sends; a search older than that
/// page's newest one returns no results, which the interface ignores because
/// it belongs to an outdated query. `scope` contains exact loaded source IDs:
/// omitted/null searches all, an empty list searches none. It never reloads sources.
#[tauri::command]
#[specta::specta]
pub(crate) async fn search(
    query: String,
    limit: usize,
    page: u32,
    request: u32,
    scope: Option<Vec<String>>,
    library: State<'_, Mutex<Library>>,
    requests: State<'_, SearchRequests>,
) -> Result<SearchReply, String> {
    // The library lock is held only to take the index.
    let (index, revision) = {
        let library = library.lock().map_err(|_| "library state is unavailable".to_string())?;
        if library.source.is_none() {
            return Err(match &library.error {
                Some(error) => format!("the library could not be opened: {error}"),
                None => "no library is open".to_string(),
            });
        }
        (Arc::clone(&library.index), library.revision.to_string())
    };
    Ok(search_snapshot(
        &index,
        revision,
        &query,
        limit,
        request,
        &requests.newest(page),
        scope.as_deref(),
    ))
}
