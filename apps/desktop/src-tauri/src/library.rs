//! The open library: loading off the main thread, the search command and
//! choosing or reloading the library file.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use ibis_core::bibtex;
use ibis_core::model::{LoadWarning, RecordId, Reference};
use ibis_core::search::OwnedIndex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::picker::{DialogOpen, show_picker};
use crate::preferences::SettingsFile;
use crate::tray::{LibraryMenuItem, library_line};

/// The library currently open in the app, indexed once when it is opened.
#[derive(Default)]
pub(crate) struct Library {
    pub(crate) path: Option<PathBuf>,
    pub(crate) index: OwnedIndex,
    pub(crate) warnings: Vec<LoadWarning>,
    /// Why the last attempt to open a library failed, shown until one opens.
    pub(crate) error: Option<String>,
    /// A problem that is not a failure to open, such as an unsaved choice.
    pub(crate) note: Option<String>,
}

/// Counts requests to open a library, so a load that finishes after a newer
/// request never replaces the newer library.
#[derive(Default)]
pub(crate) struct LoadRequests(pub(crate) AtomicU64);

/// Sent to the interface with the `library:changed` event.
#[derive(Clone, Serialize)]
pub(crate) struct LibraryStatus {
    /// File name only; the full path is never shown.
    pub(crate) name: Option<String>,
    pub(crate) references: usize,
    pub(crate) warnings: usize,
    pub(crate) error: Option<String>,
    /// Something to tell the user that is not a failure to open.
    pub(crate) note: Option<String>,
}

#[derive(Serialize)]
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
        name: library
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned()),
        references: library.index.references().len(),
        warnings: library.warnings.len(),
        error: library.error.clone(),
        note: library.note.clone(),
    }
}

/// Largest library file read; far above any real BibTeX library.
pub(crate) const MAX_LIBRARY_BYTES: u64 = 64 * 1024 * 1024;

/// Read a library file, accepting only regular files up to [`MAX_LIBRARY_BYTES`],
/// so a device or pipe can never exhaust memory or block the app.
pub(crate) fn read_library_file(path: &std::path::Path) -> Result<String, String> {
    use std::io::Read;
    let metadata = std::fs::metadata(path).map_err(|e| format!("cannot read the library file: {e}"))?;
    if !metadata.is_file() {
        return Err("the library must be a regular file".to_string());
    }
    if metadata.len() > MAX_LIBRARY_BYTES {
        return Err(format!(
            "the library file is larger than {} MiB",
            MAX_LIBRARY_BYTES / 1024 / 1024
        ));
    }
    let file = std::fs::File::open(path).map_err(|e| format!("cannot read the library file: {e}"))?;
    let mut text = String::new();
    file.take(MAX_LIBRARY_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|e| format!("cannot read the library file as UTF-8 text: {e}"))?;
    if text.len() as u64 > MAX_LIBRARY_BYTES {
        return Err("the library file grew while it was being read".to_string());
    }
    Ok(text)
}

/// Read, parse and index a BibTeX/BibLaTeX file, read-only. This is the slow
/// part of opening a library, so it never runs on the main thread.
pub(crate) fn load_library(path: PathBuf) -> Result<Library, String> {
    let text = read_library_file(&path)?;
    let source = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    let loaded = bibtex::load_str(&source.to_string_lossy(), &text);
    Ok(Library {
        path: Some(path),
        index: OwnedIndex::new(loaded.references),
        warnings: loaded.warnings,
        error: None,
        note: None,
    })
}

/// Start opening a library in the background. The request is numbered here,
/// synchronously and in the order requests arrive, so a load that finishes
/// after a newer request is discarded. The outcome reaches the interface as
/// the `library:changed` event.
///
/// With `remember`, a library that opens becomes the one opened at start.
pub(crate) fn start_open(app: &AppHandle, path: PathBuf, remember: bool) {
    let request = app.state::<LoadRequests>().0.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let chosen = path.clone();
        let loaded = tauri::async_runtime::spawn_blocking(move || load_library(path))
            .await
            .unwrap_or_else(|_| Err("loading the library stopped unexpectedly".to_string()));
        let current = install(&app.state(), &app.state(), request, loaded);
        if let Some(mut status) = current {
            if remember && status.error.is_none() {
                let saved = app.state::<SettingsFile>().update(|s| s.library = Some(chosen));
                if let Err(e) = saved {
                    let note = format!("the library opened but could not be saved for next time: {e}");
                    if let Ok(mut library) = app.state::<Mutex<Library>>().lock() {
                        library.note = Some(note.clone());
                    }
                    status.note = Some(note);
                    // The choice may have come from the tray; make sure it is seen.
                    show_picker(&app);
                }
            }
            for message in status.error.iter().chain(&status.note) {
                eprintln!("ibis: {message}");
            }
            if let Some(item) = app.try_state::<LibraryMenuItem>() {
                let _ = item.0.set_text(library_line(&status));
            }
            let _ = app.emit("library:changed", status);
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
                let _ = handle.emit(
                    "library:changed",
                    LibraryStatus {
                        error: Some("the chosen file is not on this computer".to_string()),
                        ..current_status(&handle)
                    },
                );
            }
        }
    });
}

pub(crate) fn current_status(app: &AppHandle) -> LibraryStatus {
    app.state::<Mutex<Library>>()
        .lock()
        .map(|l| status(&l))
        .unwrap_or_else(|_| LibraryStatus {
            name: None,
            references: 0,
            warnings: 0,
            error: Some("library state is unavailable".to_string()),
            note: None,
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
        Ok(new) => *library = new,
        Err(error) => library.error = Some(error),
    }
    Some(status(&library))
}

/// Choose a library with the system file dialog. The interface never passes
/// file paths; the result arrives as the `library:changed` event.
#[tauri::command]
pub(crate) fn choose_library(app: AppHandle) {
    choose_library_file(&app);
}

/// Open the current library again; the result arrives as `library:changed`.
#[tauri::command]
pub(crate) fn reload_library(app: AppHandle) {
    reload_library_file(&app);
}

/// The library as it is now, for the interface when it starts.
#[tauri::command]
pub(crate) fn library_status(app: AppHandle) -> LibraryStatus {
    current_status(&app)
}

/// Search the open library. An empty query returns no results. Async so it
/// runs off the main thread.
#[tauri::command]
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
    fn only_regular_files_are_read() {
        assert!(read_library_file(std::path::Path::new(".")).is_err());
        #[cfg(unix)]
        assert!(read_library_file(std::path::Path::new("/dev/zero")).is_err());
        let file = std::env::temp_dir().join(format!("ibis-read-{}.bib", std::process::id()));
        std::fs::write(&file, "@misc{k}").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(read_library_file(&file).as_deref(), Ok("@misc{k}"));
    }

    #[test]
    fn a_loaded_library_is_indexed_once_and_searchable() {
        let file = std::env::temp_dir().join(format!("ibis-load-{}.bib", std::process::id()));
        std::fs::write(&file, "@article{smith2020, title={Sleep and memory}}\n@article{bad,\n")
            .unwrap_or_else(|e| panic!("{e}"));
        let library = load_library(file).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(library.index.references().len(), 1);
        assert_eq!(library.warnings.len(), 1);
        assert_eq!(library.index.search("sleep", 10).len(), 1);
        assert!(load_library(std::env::temp_dir().join("ibis-missing-library.bib")).is_err());
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
