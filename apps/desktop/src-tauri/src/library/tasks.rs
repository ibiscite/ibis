//! Background loading, watching and checking, and publishing the library
//! status to the interface and the tray.

use std::collections::BTreeMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Condvar, Mutex};

use ibis_core::settings::{LibrarySetting, ZoteroLibraries, ZoteroLibrary};
#[cfg(debug_assertions)]
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

use crate::diagnostics::{NativeIssue, application_message, report};
use crate::picker::{ChooserOpen, show_picker};
use crate::preferences::SettingsFile;
use crate::tray::{LibraryMenuItem, library_line};

use super::error::LoadError;
#[cfg(test)]
use super::load::load_source;
use super::load::{Loaded, load_source_cached};
use super::retry::{begin_check, begin_focus_retry, begin_zotero_retry, retry, zotero_choice, zotero_to_check};
use super::source::Source;
use super::state::{Library, LoadRequests, begin_open, dialog_path_error, install_automatic, install_loaded};
use super::status::{LibraryProblem, LibraryState, LibraryStatus, status};

pub(super) fn publish<R: tauri::Runtime>(app: &AppHandle<R>, status: LibraryStatus) {
    if let Some(message) = &status.error {
        application_message(message);
    }
    report(app.emit("library:changed", status), NativeIssue::LibraryEvent);
}

pub(crate) fn refresh_settings_notice<R: tauri::Runtime>(app: &AppHandle<R>, settings_file: &SettingsFile) {
    if let Some(library) = app.try_state::<Mutex<Library>>() {
        update_notices(&library, |_| true, |library| settings_file.refresh_notice(library));
        if let Ok(library) = library.lock() {
            publish(app, status(&library));
        }
    }
}

fn update_notices(
    library: &Mutex<Library>,
    current: impl Fn(&Library) -> bool,
    update: impl FnOnce(&mut Library),
) -> bool {
    let mut feedback = {
        let Ok(library) = library.lock() else { return false };
        if !current(&library) {
            return false;
        }
        Library {
            settings_notice: library.settings_notice.clone(),
            choice_save_warning: library.choice_save_warning.clone(),
            ..Library::default()
        }
    };
    let previous = (feedback.settings_notice.clone(), feedback.choice_save_warning.clone());
    // Settings reads and flushes run without the lock used by main-thread commands.
    update(&mut feedback);
    let Ok(mut library) = library.lock() else { return false };
    if !current(&library) {
        return false;
    }
    if library.settings_notice == previous.0 {
        library.settings_notice = feedback.settings_notice;
    }
    if library.choice_save_warning == previous.1 {
        library.choice_save_warning = feedback.choice_save_warning;
    }
    true
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

/// Apply an engine notification for `file`, accounting for each snapshot once.
/// Returns whether `file` is still the open library.
pub(super) fn engine_changed(library: &mut Library, file: &std::sync::Arc<super::engine_file::FileLibrary>) -> bool {
    if !library
        .file
        .as_ref()
        .is_some_and(|active| std::sync::Arc::ptr_eq(active, file))
    {
        return false;
    }
    let healthy = file
        .engine
        .libraries()
        .is_ok_and(|info| !info.libraries.is_empty() && info.libraries.iter().all(|info| info.refresh_error.is_none()));
    if healthy && library.requested == library.source && library.error != Some(LoadError::NotLocal) {
        library.error = None;
        library.retained_error = None;
    }
    let now = std::time::SystemTime::now();
    let updated = library.observe_revision(now);
    // Only a verified install establishes a check time; cached engine events cannot.
    if library.checked_at.is_some() && (updated || healthy) {
        library.checked_at = Some(now);
    }
    true
}

/// Publish each change the engine reports for the open library until
/// another library replaces it.
pub(super) fn watch_current(app: &AppHandle) {
    let Some((file, events)) = observe_current(&app.state()) else {
        return;
    };
    publish_current(app);
    let handle = app.clone();
    std::thread::spawn(move || {
        while events.recv().is_ok() {
            let current = handle
                .state::<Mutex<Library>>()
                .lock()
                .ok()
                .is_some_and(|mut library| engine_changed(&mut library, &file));
            if !current {
                break;
            }
            publish_current(&handle);
        }
    });
}

fn observe_current(
    library: &Mutex<Library>,
) -> Option<(
    std::sync::Arc<super::engine_file::FileLibrary>,
    ibis_engine::LibrarySubscription,
)> {
    let mut library = library.lock().ok()?;
    let file = library.file.clone()?;
    let events = file.observe_once().ok()??;
    library.observer = Some(events.close_handle());
    // Subscribe before reconciling: changes before subscription have no queued event.
    engine_changed(&mut library, &file);
    Some((file, events))
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
    cancel: OpenCancellation,
    #[cfg(debug_assertions)]
    started: std::time::Instant,
}

#[derive(Clone, Default)]
struct OpenCancellation {
    token: ibis_engine::Cancellation,
    wake: Arc<(Mutex<()>, Condvar)>,
}

impl OpenCancellation {
    fn cancel(&self) {
        self.token.cancel();
        let _gate = self.wake.0.lock().unwrap_or_else(|e| e.into_inner());
        self.wake.1.notify_all();
    }
}

/// One supervisor owns the active request; new choices replace pending work.
#[derive(Default)]
pub(super) struct OpenQueue {
    running: bool,
    pending: Option<OpenRequest>,
    active: Option<OpenCancellation>,
}

impl LoadRequests {
    fn enqueue(&self, open: OpenRequest) -> bool {
        let Ok(mut queue) = self.1.lock() else { return false };
        // Reservation and enqueue can race on different command threads.
        if self.0.load(Ordering::SeqCst) != open.request {
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
        let mut queue = self.1.lock().ok()?;
        let next = queue.pending.take();
        queue.active = next.as_ref().map(|open| open.cancel.clone());
        if next.is_none() {
            queue.running = false;
        }
        next
    }
}

fn supervise_load<T: Send + 'static>(
    cancel: &OpenCancellation,
    deadline: std::time::Duration,
    operation: impl FnOnce() -> Result<T, LoadError> + Send + 'static,
) -> Result<T, LoadError> {
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc;
    use std::time::Instant;
    // A driver call cannot be killed safely. Bound abandoned operations, while
    // leaving room for local B to open when remote A is stuck. Saturation fails
    // promptly, rather than queuing behind an indefinitely blocked OS call.
    static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
    struct Permit;
    impl Drop for Permit {
        fn drop(&mut self) {
            IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
        }
    }
    if cancel.token.is_cancelled() {
        return Err(LoadError::Stopped);
    }
    if IN_FLIGHT.fetch_add(1, Ordering::SeqCst) >= 4 {
        IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
        return Err(ibis_core::source::SourceError::Unavailable.into());
    }
    let permit = Permit;
    let (send, result) = mpsc::sync_channel(1);
    let wake = Arc::clone(&cancel.wake);
    std::thread::Builder::new()
        .name("ibis-library-read".into())
        .spawn(move || {
            let _permit = permit;
            let loaded =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation)).unwrap_or(Err(LoadError::Stopped));
            let _ = send.send(loaded);
            let _gate = wake.0.lock().unwrap_or_else(|e| e.into_inner());
            wake.1.notify_all();
        })
        .map_err(|_| LoadError::Stopped)?;
    let until = Instant::now() + deadline;
    let mut gate = cancel.wake.0.lock().map_err(|_| LoadError::Stopped)?;
    loop {
        if cancel.token.is_cancelled() {
            return Err(LoadError::Stopped);
        }
        let remaining = until.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            drop(gate);
            cancel.cancel();
            return Err(ibis_core::source::SourceError::Unavailable.into());
        }
        match result.try_recv() {
            Ok(result) => {
                return if cancel.token.is_cancelled() {
                    Err(LoadError::Stopped)
                } else {
                    result
                };
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => return Err(LoadError::Stopped),
        }
        gate = cancel
            .wake
            .1
            .wait_timeout(gate, remaining)
            .map_err(|_| LoadError::Stopped)?
            .0;
    }
}

impl OpenIntent {
    fn saves_choice(self, source: &Source, chosen: &LibrarySetting) -> bool {
        let changed = match (source, chosen) {
            // Path equality ignores `.`; persisted cache identity hashes the exact path bytes.
            (Source::File(previous), LibrarySetting::Bibtex(next)) => previous.as_os_str() != next.as_os_str(),
            _ => source.setting().as_ref() != Some(chosen),
        };
        matches!(self, Self::Remember) || (matches!(self, Self::Read) && changed)
    }

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
            (Self::Refresh, loaded) => install_loaded(library, requests, request, loaded),
            // An explicit request starts afresh even when the source is
            // unchanged: a remembered choice is saved again afterwards.
            (_, Ok(Loaded::Unchanged)) => {
                if let Ok(mut library) = library.lock() {
                    if requests.0.load(Ordering::SeqCst) != request {
                        return None;
                    }
                    library.choice_save_warning = None;
                }
                install_loaded(library, requests, request, Ok(Loaded::Unchanged))
            }
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
    if let Ok(path) = app.path().app_cache_dir() {
        app.state::<LoadRequests>()
            .2
            .configure(&path.join("source-generations"));
    }
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
        cancel: OpenCancellation::default(),
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
        cancel,
        ..
    } = open;
    #[cfg(debug_assertions)]
    let started = open.started;
    let previous_file = app.state::<Mutex<Library>>().lock().ok().and_then(|library| {
        (reload && library.source.as_ref() == Some(&source))
            .then(|| library.file.clone())
            .flatten()
    });
    let force = reload && !matches!(intent, OpenIntent::Refresh);
    let refresh_source = source.clone();
    let cache = app.state::<LoadRequests>().2.operation(request);
    async move {
        let loaded = tauri::async_runtime::spawn_blocking(move || {
            let file_io = matches!(source, Source::File(_) | Source::Samples(_));
            let worker_cancel = cancel.token.clone();
            let operation = move || {
                #[cfg(debug_assertions)]
                let loading = std::time::Instant::now();
                let loaded = load_source_cached(source, force, previous_file, &worker_cancel, cache, || {
                    worker_cancel.is_cancelled()
                });
                #[cfg(debug_assertions)]
                {
                    Ok((loaded, loading.elapsed().as_secs_f64() * 1000.0))
                }
                #[cfg(not(debug_assertions))]
                {
                    loaded
                }
            };
            if file_io {
                supervise_load(&cancel, std::time::Duration::from_secs(10), operation)
            } else {
                operation()
            }
        })
        .await;
        #[cfg(debug_assertions)]
        let (loaded, load_ms) = loaded
            .unwrap_or(Err(LoadError::Stopped))
            .unwrap_or_else(|error| (Err(error), 0.0));
        #[cfg(not(debug_assertions))]
        let loaded = loaded.unwrap_or(Err(LoadError::Stopped));
        let chosen = remembered_choice(&refresh_source, &loaded);
        #[cfg(debug_assertions)]
        let mut phases = LoadPhases {
            load_ms,
            load_finished_ms: started.elapsed().as_secs_f64() * 1000.0,
            load_ok: u8::from(loaded.is_ok()),
            ..LoadPhases::default()
        };
        let cached = matches!(&loaded, Ok(Loaded::New(new)) if new.pending_file.as_ref().is_some_and(ibis_engine::PreparedFileLoad::cache_hit));
        #[cfg(debug_assertions)]
        let installing = std::time::Instant::now();
        let installed = intent.install(&app.state(), &app.state(), request, loaded);
        #[cfg(debug_assertions)]
        {
            phases.install_ms = installing.elapsed().as_secs_f64() * 1000.0;
            phases.accepted = u8::from(installed.is_some());
            phases.load_ok &= u8::from(installed.as_ref().is_none_or(|status| status.error.is_none()));
        }
        if installed.is_some() {
            let mut show_note = false;
            if let Some(chosen) = chosen.filter(|chosen| intent.saves_choice(&refresh_source, chosen)) {
                // Saving and the note belong to this request, never a newer choice.
                let accepted = update_notices(
                    &app.state(),
                    |library| {
                        app.state::<LoadRequests>().0.load(Ordering::SeqCst) == request && library.error.is_none()
                    },
                    |library| {
                        // Check again under the settings lock: a newer choice
                        // then saves after this one, never before it.
                        let saved = app.state::<SettingsFile>().update(|s| {
                            if app.state::<LoadRequests>().0.load(Ordering::SeqCst) == request
                                && (matches!(intent, OpenIntent::Remember) || s.library == refresh_source.setting())
                            {
                                s.library = Some(chosen);
                            }
                        });
                        if let Err(e) = saved {
                            retire_unsaved_cache(&app.state(), &app.state(), request);
                            library.choice_save_warning =
                                Some(format!("the library opened but could not be saved for next time: {e}"));
                            show_note = true;
                        } else {
                            library.choice_save_warning = None;
                            app.state::<SettingsFile>().refresh_notice(library);
                        }
                    },
                );
                show_note &= accepted;
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
            watch_current(&app);
            if cached {
                run_open(&app, request, refresh_source, true, OpenIntent::Refresh);
            }
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

fn retire_unsaved_cache(library: &Mutex<Library>, requests: &LoadRequests, request: u64) {
    let Ok(_selection) = library.lock() else { return };
    if requests.0.load(Ordering::SeqCst) == request {
        requests.2.disable();
    }
}

pub(super) fn remembered_choice(source: &Source, loaded: &Result<Loaded, LoadError>) -> Option<LibrarySetting> {
    if let Ok(Loaded::New(new)) = loaded
        && let Some(path) = new
            .pending_file
            .as_ref()
            .and_then(ibis_engine::PreparedFileLoad::source_path)
        && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("bib"))
    {
        return Some(LibrarySetting::Bibtex(path.to_path_buf()));
    }
    source.setting()
}

// Extensions recognised by ibis-sources (including CSL-JSON's alternate suffix).
const LIBRARY_FILE_EXTENSIONS: &[&str] = &["bib", "bibtex", "json", "csljson"];

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
        .set_title("Choose a library")
        .add_filter("BibTeX, BibLaTeX and CSL-JSON", LIBRARY_FILE_EXTENSIONS);
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
                    dialog_path_error(&mut library, &handle.state());
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
    if let Some((request, source)) = begin_check(&app.state(), &app.state()) {
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
            sample: false,
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

#[cfg(test)]
mod engine_read_cancellation {
    use super::*;

    #[test]
    fn a_new_choice_cancels_the_active_engine_read_without_cancelling_its_replacement() {
        let requests = LoadRequests::default();
        let active = ibis_engine::Cancellation::default();
        let replacement = ibis_engine::Cancellation::default();
        let request = |id, cancel| OpenRequest {
            request: id,
            source: Source::Zotero(ibis_core::settings::ZoteroLibrary::User.into()),
            reload: true,
            intent: OpenIntent::Refresh,
            cancel: OpenCancellation {
                token: cancel,
                ..Default::default()
            },
            #[cfg(debug_assertions)]
            started: std::time::Instant::now(),
        };
        requests.0.store(1, Ordering::SeqCst);
        assert!(requests.enqueue(request(1, active.clone())));
        assert!(requests.next_open().is_some());
        assert!(!active.is_cancelled());
        requests.0.store(2, Ordering::SeqCst);
        assert!(!requests.enqueue(request(2, replacement.clone())));
        assert!(active.is_cancelled(), "obsolete reads must stop between pages");
        assert!(!replacement.is_cancelled());
        assert_eq!(requests.next_open().map(|open| open.request), Some(2));
        assert!(requests.next_open().is_none());
    }
}
