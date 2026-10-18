//! Ibis desktop app: a thin Tauri shell over `ibis-core`.
//!
//! The window is the picker. All behaviour that can be tested without a
//! window lives in `ibis-core`; this file only translates between the web
//! interface and the core.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use ibis_core::bibtex;
use ibis_core::model::{LoadWarning, Reference};
use ibis_core::search::OwnedIndex;
use ibis_core::settings;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// The library currently open in the app, indexed once when it is opened.
#[derive(Default)]
struct Library {
    path: Option<PathBuf>,
    index: OwnedIndex,
    warnings: Vec<LoadWarning>,
    /// Why the last attempt to open a library failed, shown until one opens.
    error: Option<String>,
}

/// Counts requests to open a library, so a load that finishes after a newer
/// request never replaces the newer library.
#[derive(Default)]
struct LoadRequests(AtomicU64);

/// Sent to the interface with the `library:changed` event.
#[derive(Clone, Serialize)]
struct LibraryStatus {
    path: Option<String>,
    references: usize,
    warnings: usize,
    error: Option<String>,
}

#[derive(Serialize)]
struct SearchResult {
    key: String,
    title: String,
    authors: String,
    year: Option<i32>,
    matched: Vec<String>,
}

impl SearchResult {
    fn from(reference: &Reference, matched: Vec<String>) -> Self {
        SearchResult {
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

fn status(library: &Library) -> LibraryStatus {
    LibraryStatus {
        path: library.path.as_ref().map(|p| p.display().to_string()),
        references: library.index.references().len(),
        warnings: library.warnings.len(),
        error: library.error.clone(),
    }
}

/// Largest library file read; far above any real BibTeX library.
const MAX_LIBRARY_BYTES: u64 = 64 * 1024 * 1024;

/// Read a library file, accepting only regular files up to [`MAX_LIBRARY_BYTES`],
/// so a device or pipe can never exhaust memory or block the app.
fn read_library_file(path: &std::path::Path) -> Result<String, String> {
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
fn load_library(path: PathBuf) -> Result<Library, String> {
    let text = read_library_file(&path)?;
    let source = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    let loaded = bibtex::load_str(&source.to_string_lossy(), &text);
    Ok(Library {
        path: Some(path),
        index: OwnedIndex::new(loaded.references),
        warnings: loaded.warnings,
        error: None,
    })
}

/// Start opening a library in the background. The request is numbered here,
/// synchronously and in the order requests arrive, so a load that finishes
/// after a newer request is discarded. The outcome reaches the interface as
/// the `library:changed` event.
fn start_open(app: &AppHandle, path: PathBuf) {
    let request = app.state::<LoadRequests>().0.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let loaded = tauri::async_runtime::spawn_blocking(move || load_library(path))
            .await
            .unwrap_or_else(|_| Err("loading the library stopped unexpectedly".to_string()));
        let current = install(&app.state(), &app.state(), request, loaded);
        if let Some(status) = current {
            if let Some(error) = &status.error {
                eprintln!("ibis: {error}");
            }
            let _ = app.emit("library:changed", status);
        }
    });
}

/// Make a loaded library current, or record why it failed to open. Returns
/// `None` when a newer request has been made since this one.
fn install(
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

/// Open a library file. Not async, so requests are numbered in the order
/// they arrive; the result arrives as the `library:changed` event.
#[tauri::command]
fn open_library(path: String, app: AppHandle) {
    start_open(&app, PathBuf::from(path));
}

/// Search the open library. An empty query returns no results. Async so it
/// runs off the main thread.
#[tauri::command]
async fn search(query: String, limit: usize, library: State<'_, Mutex<Library>>) -> Result<Vec<SearchResult>, String> {
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

/// Whether the global shortcut is working, shown in the interface.
#[derive(Default, Clone, Serialize)]
struct ShortcutStatus {
    shortcut: String,
    registered: bool,
    error: Option<String>,
}

/// Default shortcut: Cmd+Shift+Space on macOS, Ctrl+Shift+Space elsewhere.
fn default_shortcut() -> Shortcut {
    let primary = if cfg!(target_os = "macos") {
        Modifiers::SUPER
    } else {
        Modifiers::CONTROL
    };
    Shortcut::new(Some(primary | Modifiers::SHIFT), Code::Space)
}

fn shortcut_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd+Shift+Space"
    } else {
        "Ctrl+Shift+Space"
    }
}

/// The shortcut from settings (for example `"Control+Command+R"`), or the
/// default. An invalid setting falls back to the default with an explanation.
fn configured_shortcut(setting: Option<&str>) -> (Shortcut, String, Option<String>) {
    match setting {
        Some(text) => match text.parse::<Shortcut>() {
            Ok(shortcut) => (shortcut, text.to_string(), None),
            Err(e) => (
                default_shortcut(),
                shortcut_label().to_string(),
                Some(format!(
                    "the shortcut \"{text}\" in settings is not valid ({e}); using {}",
                    shortcut_label()
                )),
            ),
        },
        None => (default_shortcut(), shortcut_label().to_string(), None),
    }
}

/// Show the picker, focused, and tell the interface to focus the search box.
fn show_picker(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.emit("picker:shown", ());
    }
}

/// Toggle the picker: hide it if it is visible and focused, show it otherwise.
fn toggle_picker(app: &AppHandle) {
    match app.get_webview_window("main") {
        Some(w) if w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false) => {
            let _ = w.hide();
        }
        _ => show_picker(app),
    }
}

/// Hide the picker (Escape in the interface).
#[tauri::command]
fn hide_picker(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

/// End the app. Closing the window only hides it, so this is the way out on
/// Windows and Linux, which have no application menu with Quit.
#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

/// Current state of the global shortcut.
#[tauri::command]
fn shortcut_status(status: State<'_, Mutex<ShortcutStatus>>) -> ShortcutStatus {
    status.lock().map(|s| s.clone()).unwrap_or_default()
}

fn main() {
    let app = tauri::Builder::default()
        // Must be registered first: a second launch brings the running picker forward.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_picker(app)))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle_picker(app);
                    }
                })
                .build(),
        )
        .manage(Mutex::new(Library::default()))
        .manage(LoadRequests::default())
        .manage(Mutex::new(ShortcutStatus::default()))
        .invoke_handler(tauri::generate_handler![
            open_library,
            search,
            hide_picker,
            quit,
            shortcut_status
        ])
        .on_window_event(|window, event| match event {
            // Closing the picker hides it; the app keeps running for the shortcut.
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            // Like other launchers, the picker hides when it loses focus.
            WindowEvent::Focused(false) if !cfg!(debug_assertions) => {
                let _ = window.hide();
            }
            _ => {}
        })
        .setup(|app| {
            // Settings live in the app's configuration folder, e.g. on macOS
            // ~/Library/Application Support/io.github.ibis-cite.ibis/settings.json
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let loaded = settings::load(&settings_path);
            if let Some(problem) = &loaded.problem {
                eprintln!("ibis: {problem}");
            }
            let (shortcut, label, setting_error) = configured_shortcut(loaded.settings.shortcut.as_deref());
            let result = app.global_shortcut().register(shortcut);
            let registration_error = result
                .err()
                .map(|e| format!("the shortcut could not be registered ({e}); another app may be using it"));
            let status = ShortcutStatus {
                shortcut: label,
                registered: registration_error.is_none(),
                error: registration_error.or(setting_error),
            };
            if let Some(error) = &status.error {
                eprintln!("ibis: {error}");
            }
            if let Ok(mut current) = app.state::<Mutex<ShortcutStatus>>().lock() {
                *current = status;
            }
            // The library from settings; IBIS_LIBRARY overrides it for development.
            let library = std::env::var("IBIS_LIBRARY")
                .ok()
                .or_else(|| loaded.settings.library.as_ref().map(|p| p.display().to_string()));
            if let Some(path) = library {
                // Opened in the background so the window appears at once.
                start_open(app.handle(), PathBuf::from(path));
            }
            Ok(())
        })
        .build(tauri::generate_context!());
    match app {
        Ok(app) => app.run(|app, event| {
            // macOS: clicking the Dock icon brings back the hidden picker.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                show_picker(app);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        }),
        Err(err) => {
            eprintln!("ibis: the app could not start: {err}");
            std::process::exit(1);
        }
    }
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
    fn shortcuts_from_settings_are_parsed_and_invalid_ones_fall_back() {
        let (_, label, error) = configured_shortcut(Some("Control+Command+R"));
        assert_eq!(label, "Control+Command+R");
        assert!(error.is_none());
        let (_, label, error) = configured_shortcut(Some("Control+Nonsense"));
        assert_eq!(label, shortcut_label());
        assert!(error.is_some());
        let (_, label, error) = configured_shortcut(None);
        assert_eq!(label, shortcut_label());
        assert!(error.is_none());
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
