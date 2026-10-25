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

pub(super) fn watch_current(app: &AppHandle, request: u64) {
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
    async move {
        let chosen = source.setting();
        let loading_app = app.clone();
        let loaded = tauri::async_runtime::spawn_blocking(move || {
            #[cfg(debug_assertions)]
            let loading = std::time::Instant::now();
            let loaded = load_source(source, reload, previous, || {
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
mod tests {
    use super::*;

    #[test]
    fn three_rapid_choices_run_only_first_and_last_loads_and_the_last_wins() {
        use std::sync::mpsc;
        use std::time::Duration;

        let library = Mutex::new(Library::default());
        let requests = LoadRequests::default();
        let choose = |id| {
            let source = Source::Zotero(ZoteroLibrary::Group(id).into());
            let (request, reload, _) =
                begin_open(&library, &requests, source.clone()).unwrap_or_else(|| panic!("choice starts"));
            OpenRequest {
                request,
                source,
                reload,
                intent: OpenIntent::Read,
                #[cfg(debug_assertions)]
                started: std::time::Instant::now(),
            }
        };
        assert!(requests.enqueue(choose(1)), "the first choice starts one worker");
        let (started, active) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let ran = std::thread::scope(|scope| {
            let (requests, library) = (&requests, &library);
            let worker = scope.spawn(move || {
                let mut ran = Vec::new();
                while let Some(open) = requests.next_open() {
                    ran.push(open.request);
                    if ran.len() == 1 {
                        started.send(()).unwrap_or_else(|e| panic!("{e}"));
                        resume
                            .recv_timeout(Duration::from_secs(5))
                            .unwrap_or_else(|e| panic!("{e}"));
                    }
                    let loaded = candidate(open.source, ibis_core::bibtex::load_str("synthetic", "@misc{key}"));
                    let installed =
                        open.intent
                            .install(library, requests, open.request, Ok(Loaded::New(Box::new(loaded))));
                    assert_eq!(installed.is_some(), open.request == 3, "stale work cannot install");
                }
                ran
            });
            active
                .recv_timeout(Duration::from_secs(5))
                .unwrap_or_else(|e| panic!("{e}"));
            let second = choose(2);
            assert!(
                !requests.enqueue(second.clone()),
                "the second choice waits for the active load"
            );
            assert!(
                !requests.enqueue(choose(3)),
                "a pending choice cannot start another worker"
            );
            assert!(
                !requests.enqueue(second),
                "a delayed older enqueue cannot replace the latest choice"
            );
            release.send(()).unwrap_or_else(|e| panic!("{e}"));
            worker.join().unwrap_or_else(|e| panic!("{e:?}"))
        });
        assert_eq!(ran, [1, 3], "the middle choice must never load");
        assert_eq!(
            library.lock().unwrap_or_else(|e| panic!("{e}")).source,
            Some(Source::Zotero(ZoteroLibrary::Group(3).into()))
        );
        assert!(requests.enqueue(choose(4)), "an idle worker can restart");
    }

    #[test]
    fn automatic_focus_recovery_keeps_the_unsaved_choice_but_explicit_reload_clears_it() {
        use super::super::error::ZOTERO_OFFLINE;
        use super::super::state::install;

        let source = Source::Zotero(ZoteroLibrary::Group(7).into());
        let mut opened = candidate(source.clone(), ibis_core::bibtex::load_str("synthetic", "@misc{first}"));
        let note = "the library opened but could not be saved for next time: synthetic";
        opened.choice_save_warning = Some(note.to_string());
        let library = Mutex::new(opened);
        let requests = LoadRequests::default();
        let (check, _, _) = begin_check(&library, &requests).unwrap_or_else(|| panic!("quiet check"));
        install(&library, &requests, check, Err(ZOTERO_OFFLINE)).unwrap_or_else(|| panic!("failed check"));

        let (request, retried, reload) =
            begin_focus_retry(&library, &requests).unwrap_or_else(|| panic!("focus recovery"));
        assert!(reload);
        let changed = || {
            Ok(Loaded::New(Box::new(candidate(
                source.clone(),
                ibis_core::bibtex::load_str("synthetic", "@misc{first}\n@misc{second}"),
            ))))
        };
        let recovered = OpenIntent::retry(reload)
            .install(&library, &requests, request, changed())
            .unwrap_or_else(|| panic!("recovery is current"));
        assert_eq!(retried, source);
        assert_eq!(recovered.references, 2);
        assert!(recovered.choice_not_saved);
        assert_eq!(recovered.note.as_deref(), Some(note));

        let (request, _, _) = begin_open(&library, &requests, source.clone()).unwrap_or_else(|| panic!("reload"));
        let reloaded = OpenIntent::Read
            .install(&library, &requests, request, changed())
            .unwrap_or_else(|| panic!("explicit reload is current"));
        assert_eq!(reloaded.references, 2);
        assert!(!reloaded.choice_not_saved);
        assert!(reloaded.note.is_none(), "explicit Reload retains its existing behavior");
    }

    #[cfg(debug_assertions)]
    #[test]
    fn load_phase_json_contains_only_fixed_numeric_fields_for_all_outcomes() {
        for (accepted, load_ok) in [(1, 1), (1, 0), (0, 1), (0, 0)] {
            let phases = super::LoadPhases {
                load_ms: 1.25,
                accepted,
                load_ok,
                ..Default::default()
            };
            let value = serde_json::to_value(phases).unwrap_or_else(|e| panic!("{e}"));
            let fields = value.as_object().unwrap_or_else(|| panic!("numeric object"));
            let mut expected = vec![
                "load_ms",
                "load_finished_ms",
                "install_ms",
                "publish_ms",
                "completed_ms",
                "accepted",
                "load_ok",
            ];
            expected.sort_unstable();
            let mut actual = fields.keys().map(String::as_str).collect::<Vec<_>>();
            actual.sort_unstable();
            assert_eq!(actual, expected);
            assert!(
                fields
                    .values()
                    .all(|value| value.as_f64().is_some_and(|n| n.is_finite() && n >= 0.0))
            );
            assert_eq!(fields["accepted"].as_u64(), Some(u64::from(accepted)));
            assert_eq!(fields["load_ok"].as_u64(), Some(u64::from(load_ok)));
        }
    }

    #[test]
    fn status_emission_is_serialized_but_native_menu_work_holds_no_library_lock() {
        let library = Mutex::new(Library::default());
        publish_current_with(
            &library,
            |_| assert!(library.try_lock().is_err()),
            || assert!(library.try_lock().is_ok()),
        );
    }
}
