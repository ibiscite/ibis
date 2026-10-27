//! Background loading, watching and checking, and publishing the library
//! status to the interface and the tray.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use ibis_core::settings::{ZoteroLibraries, ZoteroLibrary};
use ibis_core::source::SourceWatch;
#[cfg(debug_assertions)]
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

use crate::diagnostics::{NativeIssue, application_message, report};
use crate::picker::{ChooserOpen, show_picker};
use crate::preferences::SettingsFile;
use crate::tray::{LibraryMenuItem, library_line};

use super::error::LoadError;
use super::load::{Loaded, load_source};
use super::retry::{begin_check, begin_focus_retry, begin_zotero_retry, retry, zotero_choice, zotero_to_check};
use super::source::Source;
use super::state::{
    Library, LoadRequests, begin_open, candidate, dialog_path_error, install_automatic, install_loaded,
    take_watch_source,
};
use super::status::{LibraryProblem, LibraryState, LibraryStatus, status};

/// The platform watcher and its worker stop when replaced or the app exits.
#[derive(Default)]
pub(crate) struct LibraryWatch(pub(crate) Mutex<Option<SourceWatch>>);

pub(super) fn publish<R: tauri::Runtime>(app: &AppHandle<R>, status: LibraryStatus) {
    if let Some(message) = &status.error {
        application_message(message);
    }
    report(app.emit("library:changed", status), NativeIssue::LibraryEvent);
}

pub(crate) fn refresh_settings_notice<R: tauri::Runtime>(app: &AppHandle<R>, settings_file: &SettingsFile) {
    if let Some(library) = app.try_state::<Mutex<Library>>()
        && let Ok(mut library) = library.lock()
    {
        settings_file.refresh_notice(&mut library);
        publish(app, status(&library));
    }
}

pub(super) fn refresh_tray(app: &AppHandle) {
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
pub(super) fn publish_current(app: &AppHandle) {
    publish_current_with(&app.state(), |status| publish(app, status), || refresh_tray(app));
}

pub(super) fn publish_current_with(library: &Mutex<Library>, emit: impl FnOnce(LibraryStatus), refresh: impl FnOnce()) {
    if let Ok(library) = library.lock() {
        // The interface note names the retained source. Log only the original
        // allowlisted messages, never the filename added for that interface.
        if let Some(note) = &library.choice_save_warning {
            application_message(note);
        }
        if let Some(error) = library.retained_error {
            application_message(&error.to_string());
        }
        emit(status(&library));
    }
    refresh();
}

pub(super) fn engine_changed(library: &mut Library, file: &std::sync::Arc<super::engine_file::FileLibrary>) -> bool {
    if !library
        .file
        .as_ref()
        .is_some_and(|active| std::sync::Arc::ptr_eq(active, file))
    {
        return false;
    }
    if library.requested == library.source
        && file.engine.libraries().is_ok_and(|info| {
            !info.libraries.is_empty() && info.libraries.iter().all(|info| info.refresh_error.is_none())
        })
    {
        library.error = None;
        library.retained_error = None;
    }
    true
}

pub(super) fn watch_current(app: &AppHandle, request: u64) {
    let file = app
        .state::<Mutex<Library>>()
        .lock()
        .ok()
        .and_then(|library| library.file.clone());
    if let Some(file) = file {
        let Ok(Some(events)) = file.observe_once() else {
            return;
        };
        let handle = app.clone();
        std::thread::spawn(move || {
            loop {
                let changed = events.recv_timeout(std::time::Duration::from_millis(200));
                let current = handle.state::<Mutex<Library>>().lock().ok().is_some_and(|mut library| {
                    if changed.is_ok() {
                        engine_changed(&mut library, &file)
                    } else {
                        library
                            .file
                            .as_ref()
                            .is_some_and(|active| std::sync::Arc::ptr_eq(active, &file))
                    }
                });
                if !current || matches!(changed, Err(std::sync::mpsc::RecvTimeoutError::Disconnected)) {
                    break;
                }
                if changed.is_ok() {
                    publish_current(&handle);
                }
            }
        });
        return;
    }

    let path = app
        .state::<Mutex<Library>>()
        .lock()
        .ok()
        .and_then(|mut library| take_watch_source(&mut library, &app.state(), request));
    let Some((path, snapshot)) = path else {
        // Zotero has no file to watch; it is checked for changes each time
        // the picker is shown. Stop watching a previous file.
        let watch = app.state::<LibraryWatch>();
        if app.state::<LoadRequests>().0.load(Ordering::SeqCst) == request
            && let Ok(mut current) = watch.0.lock()
        {
            *current = None;
        }
        return;
    };
    let handle = app.clone();
    let selected = path.clone();
    let changed = move |loaded: Result<ibis_core::model::LoadResult, ibis_core::source::SourceError>| {
        let loaded = loaded
            .map(|loaded| candidate(selected.clone(), loaded))
            .map_err(LoadError::from);
        if install_automatic(&handle.state(), &handle.state(), request, loaded).is_some() {
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
                if install_automatic(&app.state(), &app.state(), request, Err(error.into())).is_some() {
                    publish_current(app);
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
enum OpenIntent {
    Remember,
    Read,
    Refresh,
}

#[derive(Clone)]
struct OpenRequest {
    request: u64,
    source: Source,
    reload: bool,
    intent: OpenIntent,
    #[cfg(debug_assertions)]
    started: std::time::Instant,
}

/// One worker owns the active load; new choices replace its pending request.
#[derive(Default)]
pub(super) struct OpenQueue {
    running: bool,
    pending: Option<OpenRequest>,
}

impl LoadRequests {
    fn enqueue(&self, open: OpenRequest) -> bool {
        let Ok(mut queue) = self.1.lock() else { return false };
        // Reservation and enqueue can race on different command threads.
        if self.0.load(Ordering::SeqCst) != open.request {
            return false;
        }
        queue.pending = Some(open);
        let start = !queue.running;
        queue.running = true;
        start
    }

    fn next_open(&self) -> Option<OpenRequest> {
        let mut queue = self.1.lock().ok()?;
        let next = queue.pending.take();
        if next.is_none() {
            queue.running = false;
        }
        next
    }
}

impl OpenIntent {
    fn retry(reload: bool) -> Self {
        if reload { Self::Refresh } else { Self::Remember }
    }

    fn install(
        self,
        library: &Mutex<Library>,
        requests: &LoadRequests,
        request: u64,
        loaded: Result<Loaded, LoadError>,
    ) -> Option<LibraryStatus> {
        match (self, loaded) {
            (Self::Refresh, Ok(Loaded::New(new))) => install_automatic(library, requests, request, Ok(*new)),
            (_, loaded) => install_loaded(library, requests, request, loaded),
        }
    }
}

/// Start opening a library in the background. The request is numbered here,
/// synchronously and in the order requests arrive, so a load that finishes
/// after a newer request is discarded. The outcome reaches the interface as
/// the `library:changed` event.
///
/// With `remember`, a library that opens becomes the one opened at start.
pub(crate) fn start_open(app: &AppHandle, source: Source, remember: bool) {
    let Some((request, reload, _)) = begin_open(&app.state(), &app.state(), source.clone()) else {
        return;
    };
    let intent = if remember {
        OpenIntent::Remember
    } else {
        OpenIntent::Read
    };
    run_open(app, request, source, reload, intent);
}

#[cfg(debug_assertions)]
#[derive(Default, Serialize)]
// Durations start inside run_open, not at process launch. load_ms excludes
// scheduling; load_finished_ms includes it. Publish measures emit/tray work,
// not webview receipt/rendering; completed includes watcher attachment.
// An obsolete result has accepted=0 and publish_ms=0; source errors have
// load_ok=0. A stopped worker has no returned load duration (load_ms=0).
struct LoadPhases {
    load_ms: f64,
    load_finished_ms: f64,
    install_ms: f64,
    publish_ms: f64,
    completed_ms: f64,
    accepted: u8,
    load_ok: u8,
}

fn run_open(app: &AppHandle, request: u64, source: Source, reload: bool, intent: OpenIntent) {
    #[cfg(debug_assertions)]
    let started = std::time::Instant::now();
    if !matches!(intent, OpenIntent::Refresh) {
        publish_current(app);
    }
    if !app.state::<LoadRequests>().enqueue(OpenRequest {
        request,
        source,
        reload,
        intent,
        #[cfg(debug_assertions)]
        started,
    }) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(open) = app.state::<LoadRequests>().next_open() {
            open_one(app.clone(), open).await;
        }
    });
}

fn open_one(app: AppHandle, open: OpenRequest) -> impl std::future::Future<Output = ()> {
    let OpenRequest {
        request,
        source,
        reload,
        intent,
        ..
    } = open;
    #[cfg(debug_assertions)]
    let started = open.started;
    let previous = app
        .state::<Mutex<Library>>()
        .lock()
        .ok()
        .and_then(|library| library.zotero.clone());
    let previous_file = app.state::<Mutex<Library>>().lock().ok().and_then(|library| {
        (reload && library.source.as_ref() == Some(&source))
            .then(|| library.file.clone())
            .flatten()
    });
    async move {
        let chosen = source.setting();
        let loading_app = app.clone();
        let loaded = tauri::async_runtime::spawn_blocking(move || {
            #[cfg(debug_assertions)]
            let loading = std::time::Instant::now();
            let loaded = load_source(source, reload, previous, previous_file, || {
                loading_app.state::<LoadRequests>().0.load(Ordering::SeqCst) != request
            });
            #[cfg(debug_assertions)]
            {
                (loaded, loading.elapsed().as_secs_f64() * 1000.0)
            }
            #[cfg(not(debug_assertions))]
            {
                loaded
            }
        })
        .await;
        #[cfg(debug_assertions)]
        let (loaded, load_ms) = loaded.unwrap_or((Err(LoadError::Stopped), 0.0));
        #[cfg(not(debug_assertions))]
        let loaded = loaded.unwrap_or(Err(LoadError::Stopped));
        #[cfg(debug_assertions)]
        let mut phases = LoadPhases {
            load_ms,
            load_finished_ms: started.elapsed().as_secs_f64() * 1000.0,
            load_ok: u8::from(loaded.is_ok()),
            ..LoadPhases::default()
        };
        #[cfg(debug_assertions)]
        let installing = std::time::Instant::now();
        let installed = intent.install(&app.state(), &app.state(), request, loaded);
        #[cfg(debug_assertions)]
        {
            phases.install_ms = installing.elapsed().as_secs_f64() * 1000.0;
            phases.accepted = u8::from(installed.is_some());
        }
        if installed.is_some() {
            let mut show_note = false;
            if matches!(intent, OpenIntent::Remember)
                && let Ok(mut library) = app.state::<Mutex<Library>>().lock()
            {
                // Saving and the note belong to this request, never a newer choice.
                if app.state::<LoadRequests>().0.load(Ordering::SeqCst) == request && library.error.is_none() {
                    let saved = app.state::<SettingsFile>().update(|s| s.library = Some(chosen));
                    if let Err(e) = saved {
                        library.choice_save_warning =
                            Some(format!("the library opened but could not be saved for next time: {e}"));
                        show_note = true;
                    } else {
                        app.state::<SettingsFile>().refresh_notice(&mut library);
                    }
                }
            }
            if show_note {
                // The choice may have come from the tray; make sure it is seen.
                show_picker(&app);
            }
            #[cfg(debug_assertions)]
            let publishing = std::time::Instant::now();
            publish_current(&app);
            #[cfg(debug_assertions)]
            {
                phases.publish_ms = publishing.elapsed().as_secs_f64() * 1000.0;
            }
            // Reattach even after a failed choice so the retained source keeps updating.
            watch_current(&app, request);
        }
        #[cfg(debug_assertions)]
        {
            phases.completed_ms = started.elapsed().as_secs_f64() * 1000.0;
            // No source, request ID, reference, error or filesystem data.
            if let Ok(json) = serde_json::to_string(&phases) {
                eprintln!("ibis-library-phases: {json}");
            }
        }
    }
}

/// Show the system file dialog and open the chosen library. Cancelling
/// changes nothing.
pub(crate) fn choose_library_file(app: &AppHandle) {
    // The dialog belongs to the picker: show the picker (the request may come
    // from the tray), attach the dialog to it, and keep the picker open while
    // the dialog has the focus.
    let Some(owner) = app.state::<ChooserOpen>().begin() else {
        return;
    };
    show_picker(app);
    #[cfg(target_os = "macos")]
    crate::panel::activate_for_dialog(app);
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
        owner.during(|| match picked.map(|p| p.into_path()) {
            None => {}
            Some(Ok(path)) => start_open(&handle, Source::File(path), true),
            Some(Err(_)) => {
                if let Ok(mut library) = handle.state::<Mutex<Library>>().lock() {
                    dialog_path_error(&mut library);
                }
                publish_current(&handle);
            }
        })
    });
}

/// Read Zotero from now on: the libraries already chosen if Zotero is in
/// use, otherwise the personal library.
pub(crate) fn use_zotero_library(app: &AppHandle) {
    let chosen = app
        .state::<Mutex<Library>>()
        .lock()
        .ok()
        .and_then(|library| zotero_choice(&library))
        .unwrap_or_else(|| ZoteroLibrary::User.into());
    open_zotero(app, chosen);
}

/// Read these Zotero libraries from now on.
pub(super) fn open_zotero(app: &AppHandle, chosen: ZoteroLibraries) {
    // Start first: the picker then sees a request in progress and does not
    // start a second check of Zotero.
    start_open(app, Source::Zotero(chosen), true);
    show_picker(app);
}

/// Check the open Zotero library for changes, for example when the picker
/// is shown. A file library has its own watcher, so nothing happens then.
pub(crate) fn refresh_zotero(app: &AppHandle) {
    if let Some((request, source, _)) = begin_check(&app.state(), &app.state()) {
        run_open(app, request, source, true, OpenIntent::Refresh);
        return;
    }
    if let Some((request, source, reload)) = begin_zotero_retry(&app.state(), &app.state(), false) {
        run_open(app, request, source, reload, OpenIntent::retry(reload));
    }
}

/// Returning to a visible picker must also recover a failed Zotero choice.
/// Healthy libraries are checked only on show, and no timer retries failures.
pub(crate) fn retry_zotero_on_focus(app: &AppHandle) {
    if let Some((request, source, reload)) = begin_focus_retry(&app.state(), &app.state()) {
        run_open(app, request, source, reload, OpenIntent::retry(reload));
    }
}

pub(crate) fn current_status(app: &AppHandle) -> LibraryStatus {
    app.state::<Mutex<Library>>()
        .lock()
        .map(|l| status(&l))
        .unwrap_or_else(|_| LibraryStatus {
            file_engine: false,
            revision: String::new(),
            state: LibraryState::Error,
            zotero: false,
            zotero_libraries: Vec::new(),
            has_snapshot: false,
            name: None,
            snapshot_name: None,
            references: 0,
            source_counts: BTreeMap::new(),
            warnings: 0,
            error: Some("library state is unavailable".to_string()),
            problem: Some(LibraryProblem::Other),
            error_reason: None,
            kept_references: false,
            choice_not_saved: false,
            note: None,
            settings_notice: None,
            snapshot_zotero: false,
            stale: false,
            updated_at: None,
            checked_at: None,
        })
}

/// Open the current library again, for example after editing the file.
pub(crate) fn reload_library_file(app: &AppHandle) {
    let source = app
        .state::<Mutex<Library>>()
        .lock()
        .ok()
        .and_then(|l| match zotero_to_check(&l) {
            Some(source) => Some(retry(&l, source)),
            None => l.source.clone().map(|source| (source, false)),
        });
    match source {
        Some((source, remember)) => start_open(app, source, remember),
        None => choose_library_file(app),
    }
}

#[cfg(test)]
mod tests;
