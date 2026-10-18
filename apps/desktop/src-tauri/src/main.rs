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
use ibis_core::cite::{self, CiteItem, KeyFormat};
use ibis_core::model::{LoadWarning, RecordId, Reference};
use ibis_core::search::OwnedIndex;
use ibis_core::settings;
use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// The library currently open in the app, indexed once when it is opened.
#[derive(Default)]
struct Library {
    path: Option<PathBuf>,
    index: OwnedIndex,
    warnings: Vec<LoadWarning>,
    /// Why the last attempt to open a library failed, shown until one opens.
    error: Option<String>,
    /// A problem that is not a failure to open, such as an unsaved choice.
    note: Option<String>,
}

/// Counts requests to open a library, so a load that finishes after a newer
/// request never replaces the newer library.
#[derive(Default)]
struct LoadRequests(AtomicU64);

/// Sent to the interface with the `library:changed` event.
#[derive(Clone, Serialize)]
struct LibraryStatus {
    /// File name only; the full path is never shown.
    name: Option<String>,
    references: usize,
    warnings: usize,
    error: Option<String>,
    /// Something to tell the user that is not a failure to open.
    note: Option<String>,
}

#[derive(Serialize)]
struct SearchResult {
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

fn status(library: &Library) -> LibraryStatus {
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
        note: None,
    })
}

/// Start opening a library in the background. The request is numbered here,
/// synchronously and in the order requests arrive, so a load that finishes
/// after a newer request is discarded. The outcome reaches the interface as
/// the `library:changed` event.
///
/// With `remember`, a library that opens becomes the one opened at start.
fn start_open(app: &AppHandle, path: PathBuf, remember: bool) {
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

/// One line describing the library, for the menu bar or tray menu.
fn library_line(status: &LibraryStatus) -> String {
    let line = match (&status.error, &status.name) {
        (Some(error), _) => format!("Library could not be opened: {error}"),
        (None, Some(name)) => {
            let problems = match status.warnings {
                0 => String::new(),
                1 => ", 1 problem".to_string(),
                n => format!(", {n} problems"),
            };
            format!("{name}: {} references{problems}", status.references)
        }
        (None, None) => "No library chosen".to_string(),
    };
    let line = match status.note {
        Some(_) => format!("{line} (not saved for next time)"),
        None => line,
    };
    // Menus do not wrap; keep long errors readable.
    if line.chars().count() > 90 {
        format!("{}…", line.chars().take(89).collect::<String>())
    } else {
        line
    }
}

/// The disabled menu item that shows the library line.
struct LibraryMenuItem(MenuItem<tauri::Wry>);

/// Show the system file dialog and open the chosen library. Cancelling
/// changes nothing.
fn choose_library_file(app: &AppHandle) {
    let handle = app.clone();
    app.dialog()
        .file()
        .set_title("Choose a BibTeX or BibLaTeX library")
        .add_filter("BibTeX and BibLaTeX", &["bib"])
        .pick_file(move |picked| match picked.map(|p| p.into_path()) {
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
        });
}

fn current_status(app: &AppHandle) -> LibraryStatus {
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
fn reload_library_file(app: &AppHandle) {
    let path = app.state::<Mutex<Library>>().lock().ok().and_then(|l| l.path.clone());
    match path {
        Some(path) => start_open(app, path, false),
        None => choose_library_file(app),
    }
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

/// Choose a library with the system file dialog. The interface never passes
/// file paths; the result arrives as the `library:changed` event.
#[tauri::command]
fn choose_library(app: AppHandle) {
    choose_library_file(&app);
}

/// Open the current library again; the result arrives as `library:changed`.
#[tauri::command]
fn reload_library(app: AppHandle) {
    reload_library_file(&app);
}

/// The library as it is now, for the interface when it starts.
#[tauri::command]
fn library_status(app: AppHandle) -> LibraryStatus {
    current_status(&app)
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

/// Where the settings file is, for saving choices made in the picker.
struct SettingsFile {
    path: PathBuf,
    /// Held for each whole read, change and save, so two saves never interleave.
    writing: Mutex<()>,
}

impl SettingsFile {
    fn new(path: PathBuf) -> Self {
        SettingsFile {
            path,
            writing: Mutex::new(()),
        }
    }

    fn update(&self, change: impl FnOnce(&mut settings::Settings)) -> Result<settings::Settings, String> {
        let _writing = self
            .writing
            .lock()
            .map_err(|_| "settings are unavailable".to_string())?;
        settings::update(&self.path, change).map_err(|e| e.to_string())
    }
}

#[derive(Serialize)]
struct Copied {
    text: String,
    /// What the chosen syntax could not express, and other problems the user should see.
    warnings: Vec<String>,
}

/// Render the selected records as one citation in `format`.
fn render_citation(references: &[Reference], ids: &[RecordId], format: KeyFormat) -> Result<cite::Rendered, String> {
    let items = ids
        .iter()
        .map(|id| {
            let reference = references
                .iter()
                .find(|r| &r.id == id)
                .ok_or_else(|| "a selected reference is no longer in the library".to_string())?;
            let key = reference.citekey.clone().ok_or_else(|| {
                format!(
                    "\"{}\" has no citation key, so it cannot be cited in this format",
                    reference.title_plain().unwrap_or_else(|| id.native_key.clone())
                )
            })?;
            Ok(CiteItem {
                key,
                ..CiteItem::default()
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    cite::render(format, &items).map_err(|e| e.to_string())
}

/// Copy the selected records as a citation to the clipboard. The format is
/// remembered for next time.
#[tauri::command]
fn copy_citation(
    ids: Vec<RecordId>,
    format: KeyFormat,
    app: AppHandle,
    library: State<'_, Mutex<Library>>,
    settings_file: State<'_, SettingsFile>,
) -> Result<Copied, String> {
    let rendered = {
        let library = library.lock().map_err(|_| "library state is unavailable".to_string())?;
        render_citation(library.index.references(), &ids, format)?
    };
    app.clipboard()
        .write_text(rendered.text.clone())
        .map_err(|e| format!("the clipboard could not be written ({e})"))?;
    let mut warnings = rendered.warnings;
    // Read and saved each time, so edits made to the file meanwhile are kept
    // and a failed save is reported on every copy.
    if let Err(e) = settings_file.update(|s| s.copy_format = Some(format)) {
        warnings.push(format!("the format choice could not be saved: {e}"));
    }
    Ok(Copied {
        text: rendered.text,
        warnings,
    })
}

/// The format the picker starts with: the last one used, or Pandoc.
#[tauri::command]
fn copy_format(settings_file: State<'_, SettingsFile>) -> KeyFormat {
    settings::load(&settings_file.path)
        .settings
        .copy_format
        .unwrap_or(KeyFormat::Pandoc)
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

/// The menu bar (macOS) or tray (Windows, Linux) item and its menu.
fn create_tray(app: &AppHandle, shortcut: Option<&str>) -> tauri::Result<()> {
    let open_label = match shortcut {
        Some(shortcut) => format!("Open Picker ({shortcut})"),
        None => "Open Picker".to_string(),
    };
    let line = MenuItem::with_id(app, "library", library_line(&current_status(app)), false, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open", open_label, true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &line,
            &MenuItem::with_id(app, "choose", "Choose Library File…", true, None::<&str>)?,
            &MenuItem::with_id(app, "reload", "Reload Library", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", "Quit Ibis", true, None::<&str>)?,
        ],
    )?;
    app.manage(LibraryMenuItem(line));
    let mut tray = TrayIconBuilder::with_id("ibis")
        .tooltip("Ibis")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_picker(app),
            "choose" => choose_library_file(app),
            "reload" => reload_library_file(app),
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

fn main() {
    let app = tauri::Builder::default()
        // Must be registered first: a second launch brings the running picker forward.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_picker(app)))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
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
            choose_library,
            reload_library,
            library_status,
            search,
            hide_picker,
            copy_citation,
            copy_format,
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
            app.manage(SettingsFile::new(settings_path.clone()));
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
            let shown_shortcut = app
                .state::<Mutex<ShortcutStatus>>()
                .lock()
                .ok()
                .filter(|s| s.registered)
                .map(|s| s.shortcut.clone());
            // A missing tray library on Linux panics inside the tray crate; the
            // picker's own library controls still work without the tray.
            let tray = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                create_tray(app.handle(), shown_shortcut.as_deref())
            }));
            match tray {
                Ok(Ok(())) => {}
                Ok(Err(e)) => eprintln!("ibis: the menu bar item could not be created ({e})"),
                Err(_) => eprintln!("ibis: the menu bar item is not available on this system"),
            }
            // The library from settings; IBIS_LIBRARY overrides it for development.
            let library = std::env::var("IBIS_LIBRARY")
                .ok()
                .or_else(|| loaded.settings.library.as_ref().map(|p| p.display().to_string()));
            if let Some(path) = library {
                // Opened in the background so the window appears at once.
                start_open(app.handle(), PathBuf::from(path), false);
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

    #[test]
    fn citations_are_rendered_by_record_identity_not_by_key() {
        let loaded = bibtex::load_str(
            "a.bib",
            "@article{same, title={From A}}\n@article{other, title={Other}}\n",
        );
        let ids: Vec<RecordId> = loaded.references.iter().rev().map(|r| r.id.clone()).collect();
        let rendered = render_citation(&loaded.references, &ids, KeyFormat::Pandoc).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(rendered.text, "[@other; @same]");
        let missing = RecordId {
            source: "b.bib".into(),
            native_key: "same".into(),
        };
        assert!(render_citation(&loaded.references, &[missing], KeyFormat::Pandoc).is_err());
    }

    #[test]
    fn the_library_line_explains_failures_and_stays_short() {
        let status = |name: Option<&str>, warnings, error: Option<&str>| LibraryStatus {
            name: name.map(String::from),
            references: 12,
            warnings,
            error: error.map(String::from),
            note: None,
        };
        assert_eq!(library_line(&status(None, 0, None)), "No library chosen");
        assert_eq!(
            library_line(&status(Some("refs.bib"), 0, None)),
            "refs.bib: 12 references"
        );
        assert_eq!(
            library_line(&status(Some("refs.bib"), 2, None)),
            "refs.bib: 12 references, 2 problems"
        );
        let unsaved = LibraryStatus {
            note: Some("could not be saved".into()),
            ..status(Some("refs.bib"), 0, None)
        };
        assert_eq!(
            library_line(&unsaved),
            "refs.bib: 12 references (not saved for next time)"
        );
        let long = "x".repeat(200);
        let line = library_line(&status(Some("refs.bib"), 0, Some(&long)));
        assert!(line.starts_with("Library could not be opened: x"));
        assert_eq!(line.chars().count(), 90);
    }

    #[test]
    fn settings_saves_from_several_threads_never_lose_each_other() {
        let dir = std::env::temp_dir().join(format!("ibis-concurrent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = std::sync::Arc::new(SettingsFile::new(dir.join("settings.json")));
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let file = file.clone();
                std::thread::spawn(move || {
                    for _ in 0..20 {
                        let saved = if i % 2 == 0 {
                            file.update(|s| s.library = Some(PathBuf::from("refs.bib")))
                        } else {
                            file.update(|s| s.copy_format = Some(KeyFormat::Typst))
                        };
                        saved.unwrap_or_else(|e| panic!("{e}"));
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap_or_else(|_| panic!("a saving thread panicked"));
        }
        let loaded = settings::load(&file.path);
        assert!(loaded.problem.is_none());
        assert_eq!(loaded.settings.library, Some(PathBuf::from("refs.bib")));
        assert_eq!(loaded.settings.copy_format, Some(KeyFormat::Typst));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
