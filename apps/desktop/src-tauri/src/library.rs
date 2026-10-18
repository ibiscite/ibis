//! The open library: loading off the main thread, the search command and
//! choosing or reloading the library file.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use ibis_core::model::{LoadWarning, RecordId, Reference};
use ibis_core::search::OwnedIndex;
use ibis_core::source::{SourceSnapshot, SourceWatch, load_library_snapshot};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::diagnostics::{NativeIssue, application_message, report};
use crate::picker::{DialogOpen, show_picker};
use crate::preferences::SettingsFile;
use crate::tray::{LibraryMenuItem, library_line};

/// The library currently open in the app, indexed once when it is opened.
#[derive(Default)]
pub(crate) struct Library {
    pub(crate) path: Option<PathBuf>,
    /// The requested source, including before it has produced an index.
    pub(crate) requested: Option<PathBuf>,
    pub(crate) opening: bool,
    pub(crate) index: OwnedIndex,
    pub(crate) warnings: Vec<LoadWarning>,
    /// Exact initial load observation, reused when attaching the watcher.
    snapshot: Option<SourceSnapshot>,
    /// Why the last attempt to open a library failed, shown until one opens.
    pub(crate) error: Option<String>,
    /// A problem that is not a failure to open, such as an unsaved choice.
    pub(crate) note: Option<String>,
    /// Persistent startup recovery notice; includes a path for the interface only.
    pub(crate) settings_notice: Option<String>,
}

/// Counts requests to open a library, so a load that finishes after a newer
/// request never replaces the newer library.
#[derive(Default)]
pub(crate) struct LoadRequests(pub(crate) AtomicU64);

/// The platform watcher and its worker stop when replaced or the app exits.
#[derive(Default)]
pub(crate) struct LibraryWatch(pub(crate) Mutex<Option<SourceWatch>>);

/// Sent to the interface with the `library:changed` event.
#[derive(Clone, Serialize, specta::Type)]
pub(crate) struct LibraryStatus {
    pub(crate) state: LibraryState,
    /// A successfully opened snapshot, including a valid empty library.
    pub(crate) has_snapshot: bool,
    /// Requested file name only, including during opening and after failure.
    pub(crate) name: Option<String>,
    /// File name of the searchable snapshot; may differ from the attempted file.
    pub(crate) snapshot_name: Option<String>,
    pub(crate) references: usize,
    pub(crate) warnings: usize,
    pub(crate) error: Option<String>,
    /// Something to tell the user that is not a failure to open.
    pub(crate) note: Option<String>,
    /// Startup settings recovery, separate from transient library status.
    pub(crate) settings_notice: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub(crate) enum LibraryState {
    None,
    Loading,
    Loaded,
    Reloading,
    Error,
}

#[derive(Serialize, specta::Type)]
pub(crate) struct SearchResult {
    /// Identity of the record; selection and copying use this, not the key.
    id: RecordId,
    key: String,
    title: String,
    authors: String,
    year: Option<i32>,
    matched: Vec<String>,
}

impl SearchResult {
    fn from(reference: &Reference, matched: Vec<String>) -> Self {
        SearchResult {
            id: reference.id.clone(),
            key: reference
                .citekey
                .clone()
                .unwrap_or_else(|| reference.id.native_key.clone()),
            title: reference.title_plain().unwrap_or_default(),
            authors: reference
                .authors
                .iter()
                .map(|n| {
                    if n.literal.is_some() {
                        n.display()
                    } else {
                        n.family.clone().unwrap_or_default()
                    }
                })
                .collect::<Vec<_>>()
                .join(", "),
            year: reference.year(),
            matched,
        }
    }
}

pub(crate) fn status(library: &Library) -> LibraryStatus {
    LibraryStatus {
        state: if library.opening {
            if library.path.is_some() && library.path == library.requested {
                LibraryState::Reloading
            } else {
                LibraryState::Loading
            }
        } else if library.error.is_some() {
            LibraryState::Error
        } else if library.path.is_some() {
            LibraryState::Loaded
        } else {
            LibraryState::None
        },
        has_snapshot: library.path.is_some(),
        name: library
            .requested
            .as_ref()
            .or(library.path.as_ref())
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned()),
        snapshot_name: library
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned()),
        references: library.index.references().len(),
        warnings: library.warnings.len(),
        error: library.error.clone(),
        note: library.note.clone(),
        settings_notice: library.settings_notice.clone(),
    }
}

/// Read, parse and index off the main thread. Initial loads tolerate damaged
/// entries; reloads retain the last good snapshot for demonstrably partial data.
pub(crate) fn load_library(path: PathBuf, reload: bool) -> Result<Library, String> {
    let (loaded, snapshot) = load_library_snapshot(&path, reload).map_err(|error| error.to_string())?;
    let mut library = candidate(path, loaded);
    library.snapshot = Some(snapshot);
    Ok(library)
}

fn candidate(path: PathBuf, loaded: ibis_core::model::LoadResult) -> Library {
    Library {
        path: Some(path),
        index: OwnedIndex::new(loaded.references),
        warnings: loaded.warnings,
        snapshot: None,
        error: None,
        note: None,
        ..Library::default()
    }
}

fn publish(app: &AppHandle, status: LibraryStatus) {
    for message in status.error.iter().chain(&status.note) {
        application_message(message);
    }
    report(app.emit("library:changed", status), NativeIssue::LibraryEvent);
}

fn refresh_tray(app: &AppHandle) {
    let handle = app.clone();
    // Menu setters wait for the main thread. Schedule there after releasing the
    // library lock, then read fresh state so delayed menu work cannot go stale.
    report(
        app.run_on_main_thread(move || {
            let current = current_status(&handle);
            if let Some(item) = handle.try_state::<LibraryMenuItem>() {
                report(item.0.set_text(library_line(&current)), NativeIssue::TrayLibrary);
            }
        }),
        NativeIssue::TrayLibrary,
    );
}

/// Derive and emit under the state lock: an older completion cannot publish
/// an out-of-date status after a newer request has started.
fn publish_current(app: &AppHandle) {
    publish_current_with(&app.state(), |status| publish(app, status), || refresh_tray(app));
}

fn publish_current_with(library: &Mutex<Library>, emit: impl FnOnce(LibraryStatus), refresh: impl FnOnce()) {
    if let Ok(library) = library.lock() {
        emit(status(&library));
    }
    refresh();
}

fn dialog_path_error(library: &mut Library) {
    library.opening = false;
    library.error = Some("the chosen file is not on this computer".to_string());
}

fn take_watch_source(
    library: &mut Library,
    requests: &LoadRequests,
    request: u64,
) -> Option<(PathBuf, Option<SourceSnapshot>)> {
    // Check while holding the library lock: an older open must not consume
    // the source observation belonging to a newly installed library.
    if requests.0.load(Ordering::SeqCst) != request {
        return None;
    }
    Some((library.path.clone()?, library.snapshot.take()))
}

fn watch_current(app: &AppHandle, request: u64) {
    let path = app
        .state::<Mutex<Library>>()
        .lock()
        .ok()
        .and_then(|mut library| take_watch_source(&mut library, &app.state(), request));
    let Some((path, snapshot)) = path else {
        return;
    };
    let handle = app.clone();
    let selected = path.clone();
    let changed = move |loaded: Result<ibis_core::model::LoadResult, ibis_core::source::SourceError>| {
        let loaded = loaded
            .map(|loaded| candidate(selected.clone(), loaded))
            .map_err(|e| e.to_string());
        if install(&handle.state(), &handle.state(), request, loaded).is_some() {
            publish_current(&handle);
        }
    };
    let watch = match snapshot {
        Some(snapshot) => SourceWatch::with_snapshot(&path, snapshot, changed),
        None => SourceWatch::new(&path, changed),
    };
    let state = app.state::<LibraryWatch>();
    if let Ok(mut current) = state.0.lock() {
        if app.state::<LoadRequests>().0.load(Ordering::SeqCst) != request {
            return;
        }
        match watch {
            Ok(watch) => *current = Some(watch),
            Err(error) => {
                if install(&app.state(), &app.state(), request, Err(error.to_string())).is_some() {
                    publish_current(app);
                }
            }
        }
    }
}

/// Start opening a library in the background. The request is numbered here,
/// synchronously and in the order requests arrive, so a load that finishes
/// after a newer request is discarded. The outcome reaches the interface as
/// the `library:changed` event.
///
/// With `remember`, a library that opens becomes the one opened at start.
pub(crate) fn start_open(app: &AppHandle, path: PathBuf, remember: bool) {
    let Some((request, reload, _)) = begin_open(&app.state(), &app.state(), path.clone()) else {
        return;
    };
    publish_current(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let chosen = path.clone();
        let loaded = tauri::async_runtime::spawn_blocking(move || load_library(path, reload))
            .await
            .unwrap_or_else(|_| Err("loading the library stopped unexpectedly".to_string()));
        if install(&app.state(), &app.state(), request, loaded).is_some() {
            let mut show_note = false;
            if remember && let Ok(mut library) = app.state::<Mutex<Library>>().lock() {
                // Saving and the note belong to this request, never a newer choice.
                if app.state::<LoadRequests>().0.load(Ordering::SeqCst) == request && library.error.is_none() {
                    let saved = app.state::<SettingsFile>().update(|s| s.library = Some(chosen));
                    if let Err(e) = saved {
                        library.note = Some(format!("the library opened but could not be saved for next time: {e}"));
                        show_note = true;
                    }
                }
            }
            if show_note {
                // The choice may have come from the tray; make sure it is seen.
                show_picker(&app);
            }
            publish_current(&app);
            // Reattach even after a failed choice so the retained source keeps updating.
            watch_current(&app, request);
        }
    });
}

/// Show the system file dialog and open the chosen library. Cancelling
/// changes nothing.
pub(crate) fn choose_library_file(app: &AppHandle) {
    // The dialog belongs to the picker: show the picker (the request may come
    // from the tray), attach the dialog to it, and keep the picker open while
    // the dialog has the focus.
    show_picker(app);
    if !app.state::<DialogOpen>().begin() {
        return; // a dialog is already open on the picker
    }
    let mut dialog = app
        .dialog()
        .file()
        .set_title("Choose a BibTeX or BibLaTeX library")
        .add_filter("BibTeX and BibLaTeX", &["bib"]);
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }
    let handle = app.clone();
    dialog.pick_file(move |picked| {
        handle.state::<DialogOpen>().end();
        match picked.map(|p| p.into_path()) {
            None => {}
            Some(Ok(path)) => start_open(&handle, path, true),
            Some(Err(_)) => {
                if let Ok(mut library) = handle.state::<Mutex<Library>>().lock() {
                    dialog_path_error(&mut library);
                }
                publish_current(&handle);
            }
        }
    });
}

pub(crate) fn current_status(app: &AppHandle) -> LibraryStatus {
    app.state::<Mutex<Library>>()
        .lock()
        .map(|l| status(&l))
        .unwrap_or_else(|_| LibraryStatus {
            state: LibraryState::Error,
            has_snapshot: false,
            name: None,
            snapshot_name: None,
            references: 0,
            warnings: 0,
            error: Some("library state is unavailable".to_string()),
            note: None,
            settings_notice: None,
        })
}

/// Open the current library file again, for example after editing it.
pub(crate) fn reload_library_file(app: &AppHandle) {
    let path = app.state::<Mutex<Library>>().lock().ok().and_then(|l| l.path.clone());
    match path {
        Some(path) => start_open(app, path, false),
        None => choose_library_file(app),
    }
}

/// Make a loaded library current, or record why it failed to open. Returns
/// `None` when a newer request has been made since this one.
pub(crate) fn install(
    library: &Mutex<Library>,
    requests: &LoadRequests,
    request: u64,
    loaded: Result<Library, String>,
) -> Option<LibraryStatus> {
    let mut library = library.lock().ok()?;
    if requests.0.load(Ordering::SeqCst) != request {
        return None;
    }
    match loaded {
        Ok(mut new) => {
            new.settings_notice = library.settings_notice.take();
            *library = new;
        }
        Err(error) => {
            library.opening = false;
            library.error = Some(if library.index.references().is_empty() {
                error
                    .strip_suffix("; keeping the last loaded references")
                    .unwrap_or(&error)
                    .to_string()
            } else {
                error
            });
        }
    }
    Some(status(&library))
}

/// Record the request before any background work can start or complete.
fn begin_open(library: &Mutex<Library>, requests: &LoadRequests, path: PathBuf) -> Option<(u64, bool, LibraryStatus)> {
    let mut library = library.lock().ok()?;
    let request = requests.0.fetch_add(1, Ordering::SeqCst) + 1;
    let reload = library.path.as_ref() == Some(&path);
    library.requested = Some(path);
    library.opening = true;
    library.error = None;
    Some((request, reload, status(&library)))
}

/// Choose a library with the system file dialog. The interface never passes
/// file paths; the result arrives as the `library:changed` event.
#[tauri::command]
#[specta::specta]
pub(crate) fn choose_library(app: AppHandle) {
    choose_library_file(&app);
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
/// runs off the main thread.
#[tauri::command]
#[specta::specta]
pub(crate) async fn search(
    query: String,
    limit: usize,
    library: State<'_, Mutex<Library>>,
) -> Result<Vec<SearchResult>, String> {
    let library = library.lock().map_err(|_| "library state is unavailable".to_string())?;
    if library.path.is_none() {
        return Err(match &library.error {
            Some(error) => format!("the library could not be opened: {error}"),
            None => "no library is open".to_string(),
        });
    }
    Ok(library
        .index
        .search(&query, limit.min(200))
        .into_iter()
        .map(|hit| {
            let matched = hit.matched.iter().map(|m| format!("{m:?}").to_lowercase()).collect();
            SearchResult::from(hit.reference, matched)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_emission_is_serialized_but_native_menu_work_holds_no_library_lock() {
        let library = Mutex::new(Library::default());
        publish_current_with(
            &library,
            |_| assert!(library.try_lock().is_err()),
            || assert!(library.try_lock().is_ok()),
        );
    }

    #[test]
    fn opening_is_visible_before_loading_and_stale_completions_cannot_replace_it() {
        let library = Mutex::new(Library::default());
        let requests = LoadRequests::default();
        assert_eq!(
            status(&library.lock().unwrap_or_else(|e| panic!("{e}"))).state,
            LibraryState::None
        );
        let (first, reload, opening) = begin_open(&library, &requests, PathBuf::from("synthetic/first.bib"))
            .unwrap_or_else(|| panic!("expected the current library request"));
        assert!(!reload);
        assert_eq!(opening.state, LibraryState::Loading);
        assert_eq!(opening.name.as_deref(), Some("first.bib"));
        assert!(!opening.has_snapshot);
        let (second, _, _) = begin_open(&library, &requests, PathBuf::from("synthetic/second.bib"))
            .unwrap_or_else(|| panic!("expected the current library request"));
        assert!(install(&library, &requests, first, Ok(Library::default())).is_none());
        let failed = install(&library, &requests, second, Err("unavailable".into()))
            .unwrap_or_else(|| panic!("expected the current library request"));
        assert_eq!(failed.state, LibraryState::Error);
        assert_eq!(failed.name.as_deref(), Some("second.bib"));
        assert!(!failed.has_snapshot);
    }

    #[test]
    fn reloading_keeps_even_an_empty_loaded_snapshot_available() {
        let library = Mutex::new(Library {
            path: Some(PathBuf::from("empty.bib")),
            ..Library::default()
        });
        let requests = LoadRequests::default();
        let (request, reload, loading) = begin_open(&library, &requests, PathBuf::from("empty.bib"))
            .unwrap_or_else(|| panic!("expected the current library request"));
        assert!(reload);
        assert_eq!(loading.state, LibraryState::Reloading);
        assert!(loading.has_snapshot);
        let failed = install(&library, &requests, request, Err("unavailable".into()))
            .unwrap_or_else(|| panic!("expected the current library request"));
        assert_eq!(failed.state, LibraryState::Error);
        assert!(failed.has_snapshot);
        assert_eq!(failed.references, 0);
        let loaded = install(
            &library,
            &requests,
            request,
            Ok(Library {
                path: Some(PathBuf::from("empty.bib")),
                ..Library::default()
            }),
        )
        .unwrap_or_else(|| panic!("expected the current library request"));
        assert_eq!(loaded.state, LibraryState::Loaded);
        assert!(loaded.error.is_none());
    }

    #[test]
    fn a_failed_switch_distinguishes_the_attempted_file_from_retained_search_results() {
        let library = Mutex::new(candidate(
            PathBuf::from("synthetic-parent/active.bib"),
            ibis_core::bibtex::load_str("synthetic", "@misc{active, title={Retained snapshot}}"),
        ));
        let requests = LoadRequests::default();
        let (request, _, opening) = begin_open(&library, &requests, PathBuf::from("other-parent/attempted.bib"))
            .unwrap_or_else(|| panic!("expected the current library request"));
        assert_eq!(opening.name.as_deref(), Some("attempted.bib"));
        assert_eq!(opening.snapshot_name.as_deref(), Some("active.bib"));
        let failed = install(&library, &requests, request, Err("unavailable".to_string()))
            .unwrap_or_else(|| panic!("expected the current library request"));
        assert_eq!(failed.name.as_deref(), Some("attempted.bib"));
        assert_eq!(failed.snapshot_name.as_deref(), Some("active.bib"));
        assert_eq!(failed.references, 1);
        assert_eq!(
            library
                .lock()
                .unwrap_or_else(|e| panic!("{e}"))
                .index
                .search("retained", 10)
                .len(),
            1
        );
        assert!(
            !failed
                .snapshot_name
                .unwrap_or_else(|| panic!("expected a retained snapshot name"))
                .contains("synthetic-parent")
        );
    }

    #[test]
    fn an_initial_missing_file_keeps_its_name_without_claiming_references_were_retained() {
        let library = Mutex::new(Library::default());
        let requests = LoadRequests::default();
        let missing = PathBuf::from("synthetic-parent-does-not-exist/missing.bib");
        let (request, _, _) = begin_open(&library, &requests, missing.clone())
            .unwrap_or_else(|| panic!("expected the current library request"));
        let failed = install(&library, &requests, request, load_library(missing, false))
            .unwrap_or_else(|| panic!("expected the current library request"));
        assert_eq!(failed.name.as_deref(), Some("missing.bib"));
        assert_eq!(failed.error.as_deref(), Some("the library file is unavailable"));
        assert!(!failed.has_snapshot);
        assert!(
            !failed
                .error
                .unwrap_or_else(|| panic!("expected a library failure"))
                .contains("synthetic-parent")
        );
    }

    #[test]
    fn only_populated_snapshots_claim_that_references_were_retained() {
        let library = Mutex::new(Library {
            path: Some(PathBuf::from("empty.bib")),
            ..Library::default()
        });
        let requests = LoadRequests::default();
        let empty = install(
            &library,
            &requests,
            0,
            Err(ibis_core::source::SourceError::Unavailable.to_string()),
        )
        .unwrap_or_else(|| panic!("expected the current library request"));
        assert_eq!(empty.error.as_deref(), Some("the library file is unavailable"));
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = dir.path().join("populated.bib");
        std::fs::write(&path, "@misc{synthetic}").unwrap_or_else(|e| panic!("{e}"));
        install(&library, &requests, 0, load_library(path, false));
        let retained = install(
            &library,
            &requests,
            0,
            Err(ibis_core::source::SourceError::Unavailable.to_string()),
        )
        .unwrap_or_else(|| panic!("expected the current library request"));
        assert!(
            retained
                .error
                .unwrap_or_else(|| panic!("expected a library failure"))
                .contains("keeping the last loaded references")
        );
    }

    #[test]
    fn settings_recovery_notice_survives_library_loading_success_and_failure() {
        let library = Mutex::new(Library {
            settings_notice: Some("settings.json is not valid; using defaults".to_string()),
            ..Library::default()
        });
        let requests = LoadRequests::default();
        let (request, _, opening) = begin_open(&library, &requests, PathBuf::from("synthetic.bib"))
            .unwrap_or_else(|| panic!("expected the current library request"));
        assert!(opening.settings_notice.is_some());
        let loaded = install(
            &library,
            &requests,
            request,
            Ok(candidate(
                PathBuf::from("synthetic.bib"),
                ibis_core::bibtex::load_str("synthetic", "@misc{k}"),
            )),
        )
        .unwrap_or_else(|| panic!("expected the current library request"));
        assert_eq!(loaded.settings_notice, opening.settings_notice);
        let failed = install(&library, &requests, request, Err("unavailable".to_string()))
            .unwrap_or_else(|| panic!("expected the current library request"));
        assert_eq!(failed.settings_notice, opening.settings_notice);
    }

    #[test]
    fn a_nonlocal_dialog_choice_keeps_the_open_library_and_reports_the_same_tray_error() {
        let mut library = candidate(
            PathBuf::from("synthetic.bib"),
            ibis_core::bibtex::load_str("synthetic", "@misc{k}\n@misc{bad,"),
        );
        library.note = Some("the library choice was not saved".to_string());
        let before = status(&library);
        dialog_path_error(&mut library);
        let failed = status(&library);
        assert_eq!(failed.state, LibraryState::Error);
        assert_eq!(failed.name.as_deref(), Some("synthetic.bib"));
        assert_eq!(failed.references, before.references);
        assert_eq!(failed.warnings, before.warnings);
        assert_eq!(failed.note.as_deref(), Some("the library choice was not saved"));
        assert_eq!(failed.error.as_deref(), Some("the chosen file is not on this computer"));
        assert!(
            library_line(&failed).starts_with("Library could not be opened: the chosen file is not on this computer")
        );
    }

    #[test]
    fn only_regular_files_are_read() {
        assert!(ibis_core::source::read_library_file(std::path::Path::new(".")).is_err());
        #[cfg(unix)]
        assert!(ibis_core::source::read_library_file(std::path::Path::new("/dev/zero")).is_err());
        let file = std::env::temp_dir().join(format!("ibis-read-{}.bib", std::process::id()));
        std::fs::write(&file, "@misc{k}").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ibis_core::source::read_library_file(&file).as_deref(), Ok("@misc{k}"));
    }

    #[test]
    fn a_loaded_library_is_indexed_once_and_searchable() {
        let file = std::env::temp_dir().join(format!("ibis-load-{}.bib", std::process::id()));
        std::fs::write(&file, "@article{smith2020, title={Sleep and memory}}\n@article{bad,\n")
            .unwrap_or_else(|e| panic!("{e}"));
        let library = load_library(file, false).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(library.index.references().len(), 1);
        assert_eq!(library.warnings.len(), 1);
        assert!(library.snapshot.is_some());
        assert_eq!(library.index.search("sleep", 10).len(), 1);
        assert!(load_library(std::env::temp_dir().join("ibis-missing-library.bib"), false).is_err());
    }

    #[test]
    fn a_stale_watch_attachment_preserves_the_newer_initial_snapshot() {
        let file = std::env::temp_dir().join(format!("ibis-watch-generation-{}.bib", std::process::id()));
        std::fs::write(&file, "@misc{good}\n@misc{unfinished,").unwrap_or_else(|e| panic!("{e}"));
        let mut library = load_library(file, false).unwrap_or_else(|e| panic!("{e}"));
        let requests = LoadRequests::default();
        requests.0.store(2, Ordering::SeqCst);
        assert!(take_watch_source(&mut library, &requests, 1).is_none());
        assert!(library.snapshot.is_some());
        let current = take_watch_source(&mut library, &requests, 2);
        assert!(current.is_some_and(|(_, snapshot)| snapshot.is_some()));
        assert!(library.snapshot.is_none());
    }

    #[test]
    fn failed_reloads_preserve_the_index_but_valid_empty_reloads_replace_it() {
        let file = std::env::temp_dir().join(format!("ibis-reload-{}.bib", std::process::id()));
        std::fs::write(&file, "@misc{good, title={Retained}}").unwrap_or_else(|e| panic!("{e}"));
        let initial = load_library(file.clone(), false).unwrap_or_else(|e| panic!("{e}"));
        let library = Mutex::new(initial);
        let requests = LoadRequests::default();
        for contents in ["@misc{partial,", "@string{incomplete = {Value}"] {
            std::fs::write(&file, contents).unwrap_or_else(|e| panic!("{e}"));
            let status = install(&library, &requests, 0, load_library(file.clone(), true));
            assert_eq!(status.as_ref().map(|s| s.references), Some(1));
            assert!(status.and_then(|s| s.error).is_some());
        }
        std::fs::remove_file(&file).unwrap_or_else(|e| panic!("{e}"));
        let unavailable = install(&library, &requests, 0, load_library(file.clone(), true));
        assert_eq!(unavailable.as_ref().map(|s| s.references), Some(1));
        assert!(unavailable.and_then(|s| s.error).is_some());
        std::fs::write(&file, "").unwrap_or_else(|e| panic!("{e}"));
        let empty = install(&library, &requests, 0, load_library(file.clone(), true));
        assert_eq!(empty.as_ref().map(|s| s.references), Some(0));
        assert!(empty.and_then(|s| s.error).is_none());
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn a_load_that_finishes_after_a_newer_request_is_discarded() {
        let library = Mutex::new(Library::default());
        let requests = LoadRequests::default();
        let named = |key: &str| Library {
            path: Some(PathBuf::from(format!("{key}.bib"))),
            ..Library::default()
        };
        requests.0.store(2, Ordering::SeqCst); // requests 1 and 2 were made, in that order
        assert!(install(&library, &requests, 2, Ok(named("newer"))).is_some());
        assert!(install(&library, &requests, 1, Ok(named("older"))).is_none());
        let current = library.lock().map(|l| l.path.clone()).unwrap_or_default();
        assert_eq!(current, Some(PathBuf::from("newer.bib")));
        let failed = install(&library, &requests, 2, Err("unreadable".into()));
        assert_eq!(failed.and_then(|s| s.error).as_deref(), Some("unreadable"));
    }
}
