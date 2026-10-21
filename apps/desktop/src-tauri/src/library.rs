//! The open library: loading off the main thread, the search command and
//! choosing or reloading a library file or the Zotero library.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use ibis_core::model::{LoadWarning, RecordId, Reference};
use ibis_core::search::OwnedIndex;
use ibis_core::settings::{LibrarySetting, ZoteroLibraries, ZoteroLibrary};
use ibis_core::source::{SourceError, SourceSnapshot, SourceWatch, load_library_snapshot};
use ibis_core::zotero::{self, libraries};
use ibis_zotero_local::LocalZotero;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::diagnostics::{NativeIssue, application_message, report};
use crate::picker::{DialogOpen, show_picker};
use crate::preferences::SettingsFile;
use crate::tray::{LibraryMenuItem, library_line};

/// Where references come from: a BibTeX or BibLaTeX file, or Zotero
/// libraries read through Zotero's local API while Zotero runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    File(PathBuf),
    Zotero(ZoteroLibraries),
}

impl From<PathBuf> for Source {
    fn from(path: PathBuf) -> Self {
        Source::File(path)
    }
}

impl From<LibrarySetting> for Source {
    fn from(setting: LibrarySetting) -> Self {
        match setting {
            LibrarySetting::Bibtex(path) => Source::File(path),
            LibrarySetting::Zotero(library) => Source::Zotero(library),
        }
    }
}

impl Source {
    fn setting(&self) -> LibrarySetting {
        match self {
            Source::File(path) => LibrarySetting::Bibtex(path.clone()),
            Source::Zotero(libraries) => LibrarySetting::Zotero(libraries.clone()),
        }
    }

    /// The name shown to the user: a file name only, never a full path.
    fn name(&self) -> Option<String> {
        match self {
            Source::File(path) => path.file_name().map(|n| n.to_string_lossy().into_owned()),
            Source::Zotero(libraries) => Some(match libraries.libraries() {
                [ZoteroLibrary::User] => "Zotero library".to_string(),
                [ZoteroLibrary::Group(id)] => format!("Zotero group {id}"),
                several => format!("{} Zotero libraries", several.len()),
            }),
        }
    }
}

fn zotero_library(library: ZoteroLibrary) -> zotero::Library {
    match library {
        ZoteroLibrary::User => zotero::Library::User,
        ZoteroLibrary::Group(id) => zotero::Library::Group(id),
    }
}

/// The library currently open in the app, indexed once when it is opened.
#[derive(Default)]
pub(crate) struct Library {
    pub(crate) source: Option<Source>,
    /// The requested source, including before it has produced an index.
    pub(crate) requested: Option<Source>,
    pub(crate) opening: bool,
    /// A Zotero change check is running; unlike `opening`, it is not shown.
    checking: bool,
    /// Shared so a search can run without holding the library lock.
    pub(crate) index: Arc<OwnedIndex>,
    /// In-memory index revision; transported as an opaque string for JavaScript.
    revision: u64,
    pub(crate) warnings: Vec<LoadWarning>,
    /// Exact initial load observation, reused when attaching the watcher.
    snapshot: Option<SourceSnapshot>,
    /// The Zotero library versions loaded, so a refresh rereads only on change.
    zotero: Option<libraries::Snapshots>,
    /// Neutral failure reason; status derives retention context from the current index.
    /// Automatic refreshes preserve a different rejected choice until the next request.
    pub(crate) error: Option<String>,
    /// An automatic update problem while a different failed choice is retained.
    retained_error: Option<String>,
    /// A problem that is not a failure to open, such as an unsaved choice.
    pub(crate) note: Option<String>,
    /// Persistent startup recovery notice; includes a path for the interface only.
    pub(crate) settings_notice: Option<String>,
    /// When a reread of the same source last brought different references
    /// during this run; `None` after the first read, so a launch or an
    /// unchanged reload is not reported as an update.
    changed_at: Option<SystemTime>,
    /// When the source was last found to match `index`: a read, or a Zotero
    /// check that found nothing changed. Kept when a later attempt fails.
    checked_at: Option<SystemTime>,
}

impl Library {
    pub(crate) fn require_revision(&self, expected: &str) -> Result<(), String> {
        if self.source.is_none() || self.revision.to_string() != expected {
            return Err("library snapshot changed".into());
        }
        Ok(())
    }
}

/// Counts requests to open a library, so a load that finishes after a newer
/// request never replaces the newer library.
#[derive(Default)]
pub(crate) struct LoadRequests(pub(crate) AtomicU64);

/// The newest search each interface page has sent, so a search for a query
/// the person has already typed past stops early instead of delaying the
/// newer one. The interface numbers its searches, because commands can start
/// in a different order than they were sent, and names its page, because a
/// reloaded page counts from the start again.
#[derive(Default)]
pub(crate) struct SearchRequests(Mutex<HashMap<u32, Arc<AtomicU32>>>);

impl SearchRequests {
    fn newest(&self, page: u32) -> Arc<AtomicU32> {
        let mut pages = self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        Arc::clone(pages.entry(page).or_default())
    }
}

/// The platform watcher and its worker stop when replaced or the app exits.
#[derive(Default)]
pub(crate) struct LibraryWatch(pub(crate) Mutex<Option<SourceWatch>>);

/// Sent to the interface with the `library:changed` event.
#[derive(Clone, Serialize, specta::Type)]
pub(crate) struct LibraryStatus {
    pub(crate) state: LibraryState,
    pub(crate) revision: String,
    /// Whether the requested library is Zotero rather than a file.
    pub(crate) zotero: bool,
    /// The Zotero libraries requested, in order; empty for a file.
    pub(crate) zotero_libraries: Vec<ZoteroLibrary>,
    /// A successfully opened snapshot, including a valid empty library.
    pub(crate) has_snapshot: bool,
    /// Requested file name only, including during opening and after failure.
    pub(crate) name: Option<String>,
    /// File name of the searchable snapshot; may differ from the attempted file.
    pub(crate) snapshot_name: Option<String>,
    pub(crate) references: usize,
    /// Counts in the searchable snapshot by exact source ID; includes empty loaded Zotero libraries.
    pub(crate) source_counts: BTreeMap<String, usize>,
    pub(crate) warnings: usize,
    pub(crate) error: Option<String>,
    /// Something to tell the user that is not a failure to open.
    pub(crate) note: Option<String>,
    /// Startup settings recovery, separate from transient library status.
    pub(crate) settings_notice: Option<String>,
    /// Whether the searchable references are Zotero's, which can differ from
    /// `zotero` (the requested source) after a failed switch.
    pub(crate) snapshot_zotero: bool,
    /// An update of the searched library failed and its earlier references
    /// are still searched; not set when a switch to another library failed.
    pub(crate) stale: bool,
    /// When a reread of the same library last changed its references during
    /// this run, in milliseconds since 1970 (UTC); `null` if it has not.
    pub(crate) updated_at: Option<f64>,
    /// When the library was last read or found unchanged, in the same unit;
    /// when `stale`, the time of the copy still being searched.
    pub(crate) checked_at: Option<f64>,
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
    /// Revision captured with the index used for this result.
    revision: String,
    /// Identity of the record; selection and copying use this, not the key.
    id: RecordId,
    key: String,
    title: String,
    authors: String,
    year: Option<i32>,
    matched: Vec<String>,
    /// Family name of the first author (or the whole name of an
    /// organization); empty when the record names no authors.
    first_author: String,
    /// How many creators the record names (an organization counts as one),
    /// so a row can name two authors and shorten three or more.
    author_count: u32,
    /// Journal, book or proceedings title, else the publisher; capped at
    /// 120 characters.
    container: Option<String>,
    /// CSL item type, such as `article-journal`.
    item_type: String,
}

/// Longest container sent to the interface, in characters, ellipsis included.
const CONTAINER_LIMIT: usize = 120;

/// Shorten `text` to at most `limit` characters, ending with an ellipsis.
fn capped(text: String, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text;
    }
    let kept: String = text.chars().take(limit.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

fn surname(name: &ibis_core::model::Name) -> String {
    if name.literal.is_some() {
        name.display()
    } else {
        name.family.clone().unwrap_or_default()
    }
}

impl SearchResult {
    fn from(reference: &Reference, matched: Vec<String>, revision: &str) -> Self {
        SearchResult {
            revision: revision.into(),
            first_author: reference.authors.first().map(surname).unwrap_or_default(),
            author_count: u32::try_from(reference.authors.len()).unwrap_or(u32::MAX),
            container: reference.container_plain().map(|text| capped(text, CONTAINER_LIMIT)),
            item_type: reference.item_type.clone(),
            id: reference.id.clone(),
            key: reference
                .citekey
                .clone()
                .unwrap_or_else(|| reference.id.native_key.clone()),
            title: reference.title_plain().unwrap_or_default(),
            authors: reference.authors.iter().map(surname).collect::<Vec<_>>().join(", "),
            year: reference.year(),
            matched,
        }
    }
}

pub(crate) fn status(library: &Library) -> LibraryStatus {
    let mut source_counts = library.index.source_counts().clone();
    if let Some(Source::Zotero(chosen)) = &library.source {
        for library in chosen.libraries() {
            source_counts.entry(zotero_library(*library).to_string()).or_default();
        }
    }
    LibraryStatus {
        revision: library.revision.to_string(),
        state: if library.opening {
            if library.source.is_some() && library.source == library.requested {
                LibraryState::Reloading
            } else {
                LibraryState::Loading
            }
        } else if library.error.is_some() {
            LibraryState::Error
        } else if library.source.is_some() {
            LibraryState::Loaded
        } else {
            LibraryState::None
        },
        zotero: matches!(
            library.requested.as_ref().or(library.source.as_ref()),
            Some(Source::Zotero(_))
        ),
        zotero_libraries: match library.requested.as_ref().or(library.source.as_ref()) {
            Some(Source::Zotero(chosen)) => chosen.libraries().to_vec(),
            _ => Vec::new(),
        },
        has_snapshot: library.source.is_some(),
        name: library
            .requested
            .as_ref()
            .or(library.source.as_ref())
            .and_then(Source::name),
        snapshot_name: library.source.as_ref().and_then(Source::name),
        references: library.index.references().len(),
        source_counts,
        warnings: library.warnings.len(),
        error: library.error.clone().map(|error| {
            let zotero = matches!(library.source, Some(Source::Zotero(_)));
            error_with_snapshot_context(error, !library.index.references().is_empty(), zotero)
        }),
        note: match &library.retained_error {
            Some(error) => {
                let name = library.source.as_ref().and_then(Source::name).unwrap_or_default();
                let update = format!("the retained library {name} could not be updated: {error}");
                Some(match &library.note {
                    Some(note) => format!("{note}\n{update}"),
                    None => update,
                })
            }
            None => library.note.clone(),
        },
        settings_notice: library.settings_notice.clone(),
        snapshot_zotero: matches!(library.source, Some(Source::Zotero(_))),
        stale: stale(library),
        updated_at: library.changed_at.and_then(epoch_millis),
        checked_at: library.checked_at.and_then(epoch_millis),
    }
}

/// The searched library's own update failed: the error is about the source
/// that produced the references, not a different library that was chosen.
fn stale(library: &Library) -> bool {
    let (Some(error), Some(source)) = (&library.error, &library.source) else {
        return false;
    };
    library.requested.as_ref().is_none_or(|requested| requested == source)
        && retains(
            error,
            !library.index.references().is_empty(),
            matches!(source, Source::Zotero(_)),
        )
}

/// A time for the interface, which shows it relative to its own clock.
fn epoch_millis(time: SystemTime) -> Option<f64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|since| since.as_secs_f64() * 1000.0)
}

/// Read, parse and index off the main thread. Initial loads tolerate damaged
/// entries; reloads retain the last good snapshot for demonstrably partial data.
pub(crate) fn load_library(path: PathBuf, reload: bool) -> Result<Library, String> {
    let (loaded, snapshot) = load_library_snapshot(&path, reload).map_err(|error| error.to_string())?;
    let mut library = candidate(path, loaded);
    library.snapshot = Some(snapshot);
    Ok(library)
}

/// What a background load produced.
pub(crate) enum Loaded {
    New(Box<Library>),
    /// A Zotero refresh found nothing changed; keep the current references.
    Unchanged,
}

/// Read the Zotero libraries, or only check for changes when `previous`
/// holds the versions already loaded.
fn load_zotero(chosen: ZoteroLibraries, previous: Option<&libraries::Snapshots>) -> Result<Loaded, String> {
    let mut zotero = LocalZotero::default();
    let wanted: Vec<zotero::Library> = chosen.libraries().iter().copied().map(zotero_library).collect();
    match libraries::refresh(&mut zotero, &wanted, previous).map_err(|e| e.to_string())? {
        libraries::Refresh::Unchanged => Ok(Loaded::Unchanged),
        libraries::Refresh::Loaded { result, snapshots } => {
            let mut new = candidate(Source::Zotero(chosen), result);
            new.zotero = Some(snapshots);
            Ok(Loaded::New(Box::new(new)))
        }
    }
}

fn load_source(source: Source, reload: bool, previous: Option<libraries::Snapshots>) -> Result<Loaded, String> {
    match source {
        Source::File(path) => load_library(path, reload).map(|library| Loaded::New(Box::new(library))),
        Source::Zotero(chosen) => load_zotero(chosen, previous.as_ref().filter(|_| reload)),
    }
}

fn candidate(source: impl Into<Source>, loaded: ibis_core::model::LoadResult) -> Library {
    let now = SystemTime::now();
    Library {
        checked_at: Some(now),
        source: Some(source.into()),
        index: Arc::new(OwnedIndex::new(loaded.references)),
        warnings: loaded.warnings,
        snapshot: None,
        error: None,
        note: None,
        ..Library::default()
    }
}

fn publish(app: &AppHandle, status: LibraryStatus) {
    if let Some(message) = &status.error {
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
        // The interface note names the retained source. Log only the original
        // allowlisted messages, never the filename added for that interface.
        for message in library.note.iter().chain(&library.retained_error) {
            application_message(message);
        }
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
    match library.source.clone()? {
        Source::File(path) => Some((path, library.snapshot.take())),
        Source::Zotero(_) => None,
    }
}

fn watch_current(app: &AppHandle, request: u64) {
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
            .map_err(|e| e.to_string());
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
                if install_automatic(&app.state(), &app.state(), request, Err(error.to_string())).is_some() {
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
pub(crate) fn start_open(app: &AppHandle, source: Source, remember: bool) {
    let Some((request, reload, _)) = begin_open(&app.state(), &app.state(), source.clone()) else {
        return;
    };
    run_open(app, request, source, reload, remember);
}

fn run_open(app: &AppHandle, request: u64, source: Source, reload: bool, remember: bool) {
    publish_current(app);
    let previous = app
        .state::<Mutex<Library>>()
        .lock()
        .ok()
        .and_then(|library| library.zotero.clone());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let chosen = source.setting();
        let loaded = tauri::async_runtime::spawn_blocking(move || load_source(source, reload, previous))
            .await
            .unwrap_or_else(|_| Err("loading the library stopped unexpectedly".to_string()));
        if install_loaded(&app.state(), &app.state(), request, loaded).is_some() {
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
        handle.state::<DialogOpen>().end();
        match picked.map(|p| p.into_path()) {
            None => {}
            Some(Ok(path)) => start_open(&handle, Source::File(path), true),
            Some(Err(_)) => {
                if let Ok(mut library) = handle.state::<Mutex<Library>>().lock() {
                    dialog_path_error(&mut library);
                }
                publish_current(&handle);
            }
        }
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

/// The Zotero libraries asked for last, or still shown after another choice
/// failed, so going back to Zotero keeps the groups that were on.
fn zotero_choice(library: &Library) -> Option<ZoteroLibraries> {
    [library.requested.as_ref(), library.source.as_ref()]
        .into_iter()
        .flatten()
        .find_map(|source| match source {
            Source::Zotero(chosen) => Some(chosen.clone()),
            _ => None,
        })
}

/// Read these Zotero libraries from now on.
fn open_zotero(app: &AppHandle, chosen: ZoteroLibraries) {
    // Start first: the picker then sees a request in progress and does not
    // start a second check of Zotero.
    start_open(app, Source::Zotero(chosen), true);
    show_picker(app);
}

/// Check the open Zotero library for changes, for example when the picker
/// is shown. A file library has its own watcher, so nothing happens then.
pub(crate) fn refresh_zotero(app: &AppHandle) {
    if let Some((request, source, previous)) = begin_check(&app.state(), &app.state()) {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let loaded = tauri::async_runtime::spawn_blocking(move || load_source(source, true, previous))
                .await
                .unwrap_or_else(|_| Err("loading the library stopped unexpectedly".to_string()));
            if install_loaded(&app.state(), &app.state(), request, loaded).is_some() {
                publish_current(&app);
                watch_current(&app, request);
            }
        });
        return;
    }
    if let Some((request, source, reload)) = begin_zotero_retry(&app.state(), &app.state(), false) {
        run_open(app, request, source, reload, !reload);
    }
}

/// Start checking the open Zotero library for changes without showing it:
/// the picker keeps its state until Zotero has changed or cannot be reached.
/// The check is numbered like any request, so a newer choice discards it.
fn begin_check(
    library: &Mutex<Library>,
    requests: &LoadRequests,
) -> Option<(u64, Source, Option<libraries::Snapshots>)> {
    let mut library = library.lock().ok()?;
    let source = library.source.clone()?;
    // An installed library has no separate request; a different request is a
    // choice in progress or a failed one that must stay visible.
    let other_request = library.requested.as_ref().is_some_and(|requested| requested != &source);
    if library.opening || library.checking || other_request {
        return None;
    }
    let Source::Zotero(_) = source else {
        return None;
    };
    let request = requests.0.fetch_add(1, Ordering::SeqCst) + 1;
    library.checking = true;
    Some((request, source, library.zotero.clone()))
}

/// Returning to a visible picker must also recover a failed Zotero choice.
/// Healthy libraries are checked only on show, and no timer retries failures.
pub(crate) fn retry_zotero_on_focus(app: &AppHandle) {
    if let Some((request, source, reload)) = begin_focus_retry(&app.state(), &app.state()) {
        run_open(app, request, source, reload, !reload);
    }
}

fn begin_focus_retry(library: &Mutex<Library>, requests: &LoadRequests) -> Option<(u64, Source, bool)> {
    begin_zotero_retry(library, requests, true)
}

fn begin_zotero_retry(
    library: &Mutex<Library>,
    requests: &LoadRequests,
    only_failed: bool,
) -> Option<(u64, Source, bool)> {
    let mut library = library.lock().ok()?;
    if library.opening || library.checking || (only_failed && library.error.is_none()) {
        return None;
    }
    let source = zotero_to_check(&library)?;
    let reload = library.source.as_ref() == Some(&source);
    let request = if reload {
        // A retained library stays visible, with its failure, until Zotero
        // answers. This is the same quiet check used when showing the picker.
        library.checking = true;
        requests.0.fetch_add(1, Ordering::SeqCst) + 1
    } else {
        // Eligibility and request reservation share the lock: focus and show
        // cannot both start recovery of a choice that has never opened.
        record_open(&mut library, requests, source.clone()).0
    };
    Some((request, source, reload))
}

/// The Zotero library to read again: the one asked for, even if it has not
/// opened yet (Zotero was closed). A failed file choice stays visible until
/// the next request, so a retained Zotero library is not checked then.
/// A choice that has not opened yet is saved once it does, like any other
/// choice; rereading the open library changes no setting.
fn retry(library: &Library, source: Source) -> (Source, bool) {
    let remember = library.source.as_ref() != Some(&source);
    (source, remember)
}

fn zotero_to_check(library: &Library) -> Option<Source> {
    match library.requested.as_ref().or(library.source.as_ref())? {
        source @ Source::Zotero(_) => Some(source.clone()),
        Source::File(_) => None,
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

/// Added to a failure while the last good references stay searchable.
pub(crate) const KEPT: &str = "; keeping the last loaded references";

/// Retention belongs to the installed desktop snapshot, not the source error.
/// Any failure to refresh Zotero keeps the references already read.
fn error_with_snapshot_context(error: String, has_references: bool, zotero: bool) -> String {
    if retains(&error, has_references, zotero) {
        format!("{error}{KEPT}")
    } else {
        error
    }
}

/// Whether this failure leaves the loaded references searchable.
fn retains(error: &str, has_references: bool, zotero: bool) -> bool {
    has_references
        && (zotero
            || [
                SourceError::Unavailable,
                SourceError::Unreadable,
                SourceError::Incomplete,
                SourceError::LoadLimit,
                SourceError::Changing,
            ]
            .iter()
            .any(|source| source.to_string() == error))
}

/// Make a loaded library current, or record why it failed to open. Returns
/// `None` when a newer request has been made since this one.
pub(crate) fn install(
    library: &Mutex<Library>,
    requests: &LoadRequests,
    request: u64,
    loaded: Result<Library, String>,
) -> Option<LibraryStatus> {
    install_result(library, requests, request, loaded, false)
}

fn install_automatic(
    library: &Mutex<Library>,
    requests: &LoadRequests,
    request: u64,
    loaded: Result<Library, String>,
) -> Option<LibraryStatus> {
    install_result(library, requests, request, loaded, true)
}

fn install_result(
    library: &Mutex<Library>,
    requests: &LoadRequests,
    request: u64,
    loaded: Result<Library, String>,
    automatic: bool,
) -> Option<LibraryStatus> {
    // Compare references outside the lock, so searches and status reads do
    // not wait on a large library; the installed index is checked again below.
    let compared = match &loaded {
        Ok(new) if new.source.is_some() => {
            let (source, index) = library
                .lock()
                .ok()
                .map(|current| (current.source.clone(), Arc::clone(&current.index)))?;
            (new.source == source).then(|| (index.references() == new.index.references(), index))
        }
        _ => None,
    };
    let mut library = library.lock().ok()?;
    if requests.0.load(Ordering::SeqCst) != request {
        return None;
    }
    // A source refresh is not a successful opening of the rejected choice.
    // Keep that choice visible while continuing to refresh its retained source.
    let failed_choice = automatic
        && library.error.is_some()
        && library.source.is_some()
        && library.requested.is_some()
        && library.requested != library.source;
    // Never reuse a revision: even an exhausted counter retains the old index.
    let loaded = loaded.and_then(|mut new| {
        new.revision = library
            .revision
            .checked_add(1)
            .ok_or_else(|| "the library could not be updated; restart Ibis".to_string())?;
        Ok(new)
    });
    match loaded {
        Ok(mut new) => {
            new.settings_notice = library.settings_notice.take();
            // Only a reread of the same library that brought different
            // references is an update worth telling.
            if let Some((same, index)) = compared
                && new.source == library.source
            {
                // Unchanged only if the compared index is still the installed one.
                new.changed_at = if same && Arc::ptr_eq(&index, &library.index) {
                    library.changed_at
                } else {
                    new.checked_at
                };
            }
            if failed_choice {
                new.requested = library.requested.take();
                new.error = library.error.take();
                new.note = library.note.take();
            }
            *library = new;
        }
        Err(error) => {
            library.opening = false;
            library.checking = false;
            if failed_choice {
                library.retained_error = Some(error);
            } else {
                library.error = Some(error);
                library.retained_error = None;
            }
        }
    }
    Some(status(&library))
}

/// [`install`], where a Zotero refresh that found no change only ends the
/// request and keeps everything as it is.
fn install_loaded(
    library: &Mutex<Library>,
    requests: &LoadRequests,
    request: u64,
    loaded: Result<Loaded, String>,
) -> Option<LibraryStatus> {
    match loaded {
        Ok(Loaded::New(new)) => install(library, requests, request, Ok(*new)),
        Err(error) => install(library, requests, request, Err(error)),
        Ok(Loaded::Unchanged) => {
            let mut library = library.lock().ok()?;
            if requests.0.load(Ordering::SeqCst) != request {
                return None;
            }
            library.opening = false;
            library.checking = false;
            library.error = None;
            library.checked_at = Some(SystemTime::now());
            // Keep the note: a "not saved for next time" warning must outlive
            // the check that showing the picker starts right after it.
            library.requested = library.source.clone();
            Some(status(&library))
        }
    }
}

/// Record the request before any background work can start or complete.
fn begin_open(
    library: &Mutex<Library>,
    requests: &LoadRequests,
    source: impl Into<Source>,
) -> Option<(u64, bool, LibraryStatus)> {
    let source = source.into();
    let mut library = library.lock().ok()?;
    Some(record_open(&mut library, requests, source))
}

fn record_open(library: &mut Library, requests: &LoadRequests, source: Source) -> (u64, bool, LibraryStatus) {
    let request = requests.0.fetch_add(1, Ordering::SeqCst) + 1;
    let reload = library.source.as_ref() == Some(&source);
    library.requested = Some(source);
    library.opening = true;
    library.checking = false;
    library.error = None;
    library.retained_error = None;
    (request, reload, status(library))
}

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

/// A search keeps its snapshot identity even when no references match.
#[derive(Serialize, specta::Type)]
pub(crate) struct SearchReply {
    revision: String,
    results: Vec<SearchResult>,
}

fn search_snapshot(
    index: &OwnedIndex,
    revision: String,
    query: &str,
    limit: usize,
    request: u32,
    newest: &AtomicU32,
    scope: Option<&[String]>,
) -> SearchReply {
    let results = search_newest(index, &revision, query, limit, request, newest, scope);
    SearchReply { revision, results }
}

/// Run a search unless a newer request has started, stopping if one does.
fn search_newest(
    index: &OwnedIndex,
    revision: &str,
    query: &str,
    limit: usize,
    request: u32,
    newest: &AtomicU32,
    scope: Option<&[String]>,
) -> Vec<SearchResult> {
    newest.fetch_max(request, Ordering::SeqCst);
    let outdated = || newest.load(Ordering::SeqCst) > request;
    let results = index
        .search_scoped_until(query, limit.min(200), scope, &outdated)
        .unwrap_or_default()
        .into_iter()
        .map(|hit| {
            let matched = hit.matched.iter().map(|m| format!("{m:?}").to_lowercase()).collect();
            SearchResult::from(hit.reference, matched, revision)
        })
        .collect();
    if outdated() { Vec::new() } else { results }
}

#[cfg(test)]
mod tests {
    #[test]
    fn results_carry_what_tells_similar_references_apart() {
        let long = "Long ".repeat(60);
        let bib = format!(
            "@article{{a, title={{Sleep}}, author={{Garc{{\\'i}}a, Mar{{\\'i}}a and Okafor, Tobi}}, journal={{Journal of Synthetic Studies}}, year={{2021}}}}\n\
             @book{{b, title={{Sleep}}, author={{{{Example Group}}}}, publisher={{{long}}}, year={{2021}}}}\n\
             @misc{{c, title={{Sleep}}, year={{2021}}}}"
        );
        let loaded = ibis_core::bibtex::load_str("t", &bib);
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let results: Vec<SearchResult> = loaded
            .references
            .iter()
            .map(|r| SearchResult::from(r, Vec::new(), "synthetic"))
            .collect();
        assert_eq!(results[0].first_author, "García");
        assert_eq!(results[0].author_count, 2);
        assert_eq!(results[0].container.as_deref(), Some("Journal of Synthetic Studies"));
        assert_eq!(results[0].item_type, "article-journal");
        assert_eq!(results[1].first_author, "Example Group");
        assert_eq!(results[1].author_count, 1);
        assert_eq!(results[1].item_type, "book");
        let capped = results[1].container.as_deref().unwrap_or_default();
        assert_eq!(capped.chars().count(), CONTAINER_LIMIT);
        assert!(capped.ends_with('…') && !capped.ends_with(" …"));
        assert_eq!(
            (
                results[2].first_author.as_str(),
                results[2].author_count,
                results[2].container.as_deref()
            ),
            ("", 0, None)
        );
    }

    #[test]
    fn the_author_count_comes_from_the_creator_list() {
        // A literal organization name with a comma is one creator, so the
        // count is not read from the comma-joined display string.
        let bib = "@article{a, title={Sleep}, author={Smith, Ann and {Institute, Research}}, year={2021}}\n\
                   @article{b, title={Sleep}, author={Smith, Ann and Okafor, Tobi and Lima, Ana}, year={2021}}";
        let loaded = ibis_core::bibtex::load_str("t", bib);
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let results: Vec<SearchResult> = loaded
            .references
            .iter()
            .map(|r| SearchResult::from(r, Vec::new(), "synthetic"))
            .collect();
        assert_eq!(
            (results[0].author_count, results[0].authors.as_str()),
            (2, "Smith, Institute, Research")
        );
        assert_eq!(results[1].author_count, 3);
    }

    #[test]
    fn scopes_limit_the_snapshot_without_reopening_and_keep_page_requests_separate() {
        let chosen = ZoteroLibraries::new(vec![
            ZoteroLibrary::User,
            ZoteroLibrary::Group(7),
            ZoteroLibrary::Group(8),
        ])
        .unwrap_or_else(|| panic!("valid libraries"));
        let mut loaded = ibis_core::bibtex::load_str("zotero:user", "@misc{same,title={研究},doi={10.1000/scope}}");
        loaded.references.extend(
            ibis_core::bibtex::load_str("zotero:group:7", "@misc{same,title={研究},doi={10.1000/scope}}").references,
        );
        let mut library = candidate(Source::Zotero(chosen), loaded);
        let index = Arc::clone(&library.index);
        let requests = SearchRequests::default();
        let scope = vec!["zotero:group:7".into()];
        let results = search_newest(&index, "synthetic", "研究", 1, 2, &requests.newest(1), Some(&scope));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id.source, "zotero:group:7");
        assert!(search_newest(&index, "synthetic", "10.1000/scope", 20, 1, &requests.newest(1), None).is_empty());
        assert_eq!(
            search_newest(&index, "synthetic", "10.1000/scope", 20, 1, &requests.newest(2), None).len(),
            2
        );
        assert!(search_newest(&index, "synthetic", "研究", 20, 3, &requests.newest(1), Some(&[])).is_empty());
        assert!(Arc::ptr_eq(&index, &library.index));
        let counts = status(&library).source_counts;
        assert_eq!(counts.get("zotero:user"), Some(&1));
        assert_eq!(counts.get("zotero:group:7"), Some(&1));
        assert_eq!(counts.get("zotero:group:8"), Some(&0));
        assert_eq!(counts.values().sum::<usize>(), status(&library).references);
        // Counts describe the retained searchable snapshot, not a failed new selection.
        library.requested = Some(Source::Zotero(ZoteroLibrary::Group(99).into()));
        library.error = Some("unavailable".into());
        assert_eq!(status(&library).source_counts, counts);
        assert!(status(&Library::default()).source_counts.is_empty());
        let file = candidate(
            PathBuf::from("refs.bib"),
            ibis_core::bibtex::load_str("synthetic-file", "@misc{a}"),
        );
        assert_eq!(status(&file).source_counts.get("synthetic-file"), Some(&1));
    }

    #[test]
    fn scoped_snapshot_replies_keep_revision_and_retained_source_counts_together() {
        let chosen = ZoteroLibraries::new(vec![
            ZoteroLibrary::User,
            ZoteroLibrary::Group(7),
            ZoteroLibrary::Group(8),
        ])
        .unwrap_or_else(|| panic!("valid synthetic libraries"));
        let mut loaded = ibis_core::bibtex::load_str("zotero:user", "@misc{same,title={研究}}");
        loaded
            .references
            .extend(ibis_core::bibtex::load_str("zotero:group:7", "@misc{same,title={研究}}").references);
        let mut library = candidate(Source::Zotero(chosen), loaded);
        library.revision = u64::MAX - 1;
        let captured = status(&library);
        let index = Arc::clone(&library.index);
        let scope = vec!["zotero:group:7".into()];
        let newest = AtomicU32::default();
        let scoped = search_snapshot(&index, captured.revision.clone(), "研究", 20, 1, &newest, Some(&scope));
        assert_eq!(scoped.revision, captured.revision);
        assert_eq!(scoped.results.len(), 1);
        assert_eq!(scoped.results[0].id.source, "zotero:group:7");
        assert_eq!(scoped.results[0].revision, captured.revision);
        let all = search_snapshot(&index, captured.revision.clone(), "研究", 20, 2, &newest, None);
        assert_eq!(all.results.len(), 2);
        for (request, scope) in [
            (3, Vec::new()),
            (4, vec!["zotero:group:8".into()]),
            (5, vec!["unknown".into()]),
        ] {
            let empty = search_snapshot(
                &index,
                captured.revision.clone(),
                "研究",
                20,
                request,
                &newest,
                Some(&scope),
            );
            assert!(empty.results.is_empty());
            assert_eq!(empty.revision, captured.revision);
        }
        let late = search_snapshot(&index, captured.revision.clone(), "研究", 20, 1, &newest, None);
        assert!(late.results.is_empty());
        assert_eq!(late.revision, captured.revision);
        library.requested = Some(Source::Zotero(ZoteroLibrary::Group(99).into()));
        library.error = Some("synthetic reload failure".into());
        let retained = status(&library);
        assert_eq!(retained.revision, captured.revision);
        assert_eq!(retained.source_counts, captured.source_counts);
        assert_eq!(retained.source_counts.get("zotero:group:8"), Some(&0));
        assert_eq!(retained.source_counts.values().sum::<usize>(), retained.references);
        assert!(Arc::ptr_eq(&index, &library.index));
    }

    #[test]
    fn scoped_results_keep_the_desktop_result_cap() {
        let bib = (0..220)
            .map(|n| format!("@misc{{key{n},title={{研究}}}}\n"))
            .collect::<String>();
        let index = OwnedIndex::new(ibis_core::bibtex::load_str("file", &bib).references);
        let scope = vec!["file".into(), "file".into()];
        let results = search_newest(
            &index,
            "synthetic",
            "研究",
            usize::MAX,
            1,
            &AtomicU32::new(0),
            Some(&scope),
        );
        assert_eq!(results.len(), 200);
        assert!(results.iter().all(|r| r.id.source == "file"));
        assert_eq!(index.source_counts().get("file"), Some(&220));
    }

    #[test]
    fn an_older_search_stops_even_if_it_starts_after_a_newer_one() {
        let refs = ibis_core::bibtex::load_str("t", "@article{a,title={Sleep},year={2020}}").references;
        let index = OwnedIndex::new(refs);
        let newest = AtomicU32::new(0);
        assert_eq!(
            search_newest(&index, "synthetic", "sleep", 20, 2, &newest, None).len(),
            1
        );
        assert!(search_newest(&index, "synthetic", "sleep", 20, 1, &newest, None).is_empty());
        assert_eq!(
            search_newest(&index, "synthetic", "sleep", 20, 2, &newest, None).len(),
            1
        );
        assert_eq!(
            search_newest(&index, "synthetic", "sleep", 20, 3, &newest, None).len(),
            1
        );
    }

    #[test]
    fn a_reloaded_page_counts_its_searches_afresh() {
        let refs = ibis_core::bibtex::load_str("t", "@article{a,title={Sleep},year={2020}}").references;
        let index = OwnedIndex::new(refs);
        let requests = SearchRequests::default();
        assert_eq!(
            search_newest(&index, "synthetic", "sleep", 20, 500, &requests.newest(1), None).len(),
            1
        );
        assert_eq!(
            search_newest(&index, "synthetic", "sleep", 20, 1, &requests.newest(2), None).len(),
            1
        );
        assert!(search_newest(&index, "synthetic", "sleep", 20, 499, &requests.newest(1), None).is_empty());
    }

    use super::*;

    #[test]
    fn going_back_to_zotero_after_a_failed_file_keeps_the_groups_that_were_on() {
        let groups = ZoteroLibraries::new(vec![ZoteroLibrary::User, ZoteroLibrary::Group(42)])
            .unwrap_or_else(|| panic!("a valid choice"));
        let library = Library {
            source: Some(Source::Zotero(groups.clone())),
            requested: Some(Source::File(PathBuf::from("/synthetic/missing.bib"))),
            ..Library::default()
        };
        assert_eq!(zotero_choice(&library), Some(groups));
        assert_eq!(zotero_choice(&Library::default()), None);
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
            source: Some(Source::File(PathBuf::from("empty.bib"))),
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
                source: Some(Source::File(PathBuf::from("empty.bib"))),
                ..Library::default()
            }),
        )
        .unwrap_or_else(|| panic!("expected the current library request"));
        assert_eq!(loaded.state, LibraryState::Loaded);
        assert!(loaded.error.is_none());
    }

    #[test]
    fn a_failed_choice_survives_retained_source_reattachment_and_real_updates() {
        use std::sync::{Arc, mpsc};
        use std::time::Duration;

        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let retained = dir.path().join("retained.bib");
        let attempted = dir.path().join("invalid-utf8.bib");
        std::fs::write(&retained, "@misc{first,title={Retained first}}").unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(&attempted, [0xff, 0xfe]).unwrap_or_else(|e| panic!("{e}"));
        let library = Arc::new(Mutex::new(
            load_library(retained.clone(), false).unwrap_or_else(|e| panic!("{e}")),
        ));
        let requests = Arc::new(LoadRequests::default());
        // Initial attachment consumes the observation; later retained-source
        // reattachment takes the real SourceWatch::new registration gap path.
        let initial = take_watch_source(&mut library.lock().unwrap_or_else(|e| panic!("{e}")), &requests, 0);
        assert!(initial.is_some_and(|(_, snapshot)| snapshot.is_some()));
        let (request, _, _) = begin_open(&library, &requests, attempted.clone()).unwrap_or_else(|| panic!("open"));
        let failed =
            install(&library, &requests, request, load_library(attempted, false)).unwrap_or_else(|| panic!("install"));
        let expected_error = failed.error.clone();
        assert!(
            expected_error
                .as_deref()
                .is_some_and(|message| message.contains("not UTF-8"))
        );
        let (path, snapshot) = take_watch_source(
            &mut library.lock().unwrap_or_else(|e| panic!("{e}")),
            &requests,
            request,
        )
        .unwrap_or_else(|| panic!("watch source"));
        assert!(snapshot.is_none());
        let (send, receive) = mpsc::channel();
        let watched_library = Arc::clone(&library);
        let watched_requests = Arc::clone(&requests);
        let selected = path.clone();
        let _watch = SourceWatch::new(&path, move |loaded| {
            let loaded = loaded
                .map(|loaded| candidate(selected.clone(), loaded))
                .map_err(|error| error.to_string());
            if let Some(status) = install_automatic(&watched_library, &watched_requests, request, loaded) {
                let _ = send.send(status);
            }
        })
        .unwrap_or_else(|e| panic!("{e}"));
        let unchanged = receive
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            unchanged.error, expected_error,
            "unchanged retained source cleared the failed choice"
        );
        assert_eq!(unchanged.name.as_deref(), Some("invalid-utf8.bib"));
        assert_eq!(unchanged.snapshot_name.as_deref(), Some("retained.bib"));
        assert_eq!(unchanged.references, 1);

        std::fs::write(
            &retained,
            "@misc{first,title={Retained first}}\n@misc{second,title={Actual update}}",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let updated = receive
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(updated.references, 2, "real retained-source changes must still load");
        assert_eq!(updated.error, expected_error);
        assert_eq!(updated.name.as_deref(), Some("invalid-utf8.bib"));
        assert_eq!(
            library
                .lock()
                .unwrap_or_else(|e| panic!("{e}"))
                .index
                .search("actual", 10)
                .len(),
            1
        );
    }

    #[test]
    fn retained_source_failures_and_empty_recovery_preserve_the_choice_and_existing_note() {
        let retained = PathBuf::from("synthetic/retained.bib");
        let original_note = "the library choice could not be saved";
        let library = Mutex::new(Library {
            note: Some(original_note.to_string()),
            settings_notice: Some("settings recovered".to_string()),
            ..candidate(
                retained.clone(),
                ibis_core::bibtex::load_str("synthetic", "@misc{first}"),
            )
        });
        let requests = LoadRequests::default();
        let (request, _, _) =
            begin_open(&library, &requests, PathBuf::from("other/attempted.bib")).unwrap_or_else(|| panic!("open"));
        let rejected = install(&library, &requests, request, Err(SourceError::Unreadable.to_string()))
            .unwrap_or_else(|| panic!("install"));
        // Watch-construction failures use this same automatic policy, too.
        let unavailable = install_automatic(
            &library,
            &requests,
            request,
            Err(SourceError::WatchUnavailable.to_string()),
        )
        .unwrap_or_else(|| panic!("automatic install"));
        assert_eq!(unavailable.error, rejected.error);
        let note = unavailable.note.unwrap_or_else(|| panic!("retained source note"));
        assert!(note.starts_with(original_note));
        assert!(note.contains("retained.bib could not be updated"));
        assert!(note.contains("automatic library updates are unavailable"));
        assert!(!note.contains("synthetic/"));
        assert_eq!(unavailable.name.as_deref(), Some("attempted.bib"));

        let empty = install_automatic(
            &library,
            &requests,
            request,
            Ok(candidate(
                retained.clone(),
                ibis_core::bibtex::load_str("synthetic", ""),
            )),
        )
        .unwrap_or_else(|| panic!("empty update"));
        assert_eq!(empty.references, 0);
        assert!(empty.has_snapshot);
        assert_eq!(
            empty.error.as_deref(),
            Some("the library file is not UTF-8 text; save it as UTF-8")
        );
        assert_eq!(
            empty.note.as_deref(),
            Some(original_note),
            "only the automatic note should clear"
        );
        assert_eq!(empty.settings_notice.as_deref(), Some("settings recovered"));
        let partial = install_automatic(&library, &requests, request, Err(SourceError::Incomplete.to_string()))
            .unwrap_or_else(|| panic!("partial update"));
        assert_eq!(partial.error, empty.error);
        assert!(
            partial
                .note
                .as_deref()
                .is_some_and(|note| note.contains("retained.bib") && note.contains("unfinished"))
        );

        let (new_request, reload, opening) =
            begin_open(&library, &requests, retained.clone()).unwrap_or_else(|| panic!("reload"));
        assert!(reload);
        assert!(opening.error.is_none());
        assert_eq!(opening.note.as_deref(), Some(original_note));
        assert!(
            install_automatic(
                &library,
                &requests,
                request,
                Ok(candidate(
                    retained.clone(),
                    ibis_core::bibtex::load_str("synthetic", "@misc{stale}")
                ))
            )
            .is_none()
        );
        assert!(install_automatic(&library, &requests, request, Err(SourceError::Unavailable.to_string())).is_none());
        let loaded = install(
            &library,
            &requests,
            new_request,
            Ok(candidate(retained, ibis_core::bibtex::load_str("synthetic", ""))),
        )
        .unwrap_or_else(|| panic!("reload completion"));
        assert_eq!(loaded.state, LibraryState::Loaded);
        assert_eq!(loaded.references, 0);
        assert!(loaded.error.is_none());
        assert!(loaded.note.is_none());
        assert_eq!(loaded.settings_notice.as_deref(), Some("settings recovered"));
    }

    #[test]
    fn automatic_recovery_clears_a_failure_of_the_selected_source_itself() {
        let path = PathBuf::from("synthetic/selected.bib");
        let library = Mutex::new(candidate(
            path.clone(),
            ibis_core::bibtex::load_str("synthetic", "@misc{first}"),
        ));
        let requests = LoadRequests::default();
        let (request, reload, _) = begin_open(&library, &requests, path.clone()).unwrap_or_else(|| panic!("open"));
        assert!(reload);
        install(&library, &requests, request, Err(SourceError::Unavailable.to_string()));
        let recovered = install_automatic(
            &library,
            &requests,
            request,
            Ok(candidate(
                path,
                ibis_core::bibtex::load_str("synthetic", "@misc{first}"),
            )),
        )
        .unwrap_or_else(|| panic!("recovery"));
        assert_eq!(recovered.state, LibraryState::Loaded);
        assert_eq!(recovered.name.as_deref(), Some("selected.bib"));
        assert!(recovered.error.is_none());
        assert!(recovered.note.is_none());
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
        let failures = [
            (SourceError::Unavailable, "the library file is unavailable"),
            (
                SourceError::Unreadable,
                "the library file is not UTF-8 text; save it as UTF-8",
            ),
            (SourceError::Incomplete, "the library file contains an unfinished entry"),
            (SourceError::LoadLimit, "the library exceeded the load work limit"),
            (SourceError::Changing, "the library file is still changing"),
        ];
        let library = Mutex::new(Library {
            source: Some(Source::File(PathBuf::from("empty.bib"))),
            ..Library::default()
        });
        let requests = LoadRequests::default();
        for (error, message) in &failures {
            let empty = install(&library, &requests, 0, Err(error.to_string()))
                .unwrap_or_else(|| panic!("expected the current library request"));
            assert_eq!(empty.error.as_deref(), Some(*message));
            assert_eq!(empty.references, 0);
        }
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = dir.path().join("populated.bib");
        std::fs::write(&path, "@misc{synthetic}").unwrap_or_else(|e| panic!("{e}"));
        install(&library, &requests, 0, load_library(path, false));
        for (error, message) in &failures {
            let retained = install(&library, &requests, 0, Err(error.to_string()))
                .unwrap_or_else(|| panic!("expected the current library request"));
            assert_eq!(
                retained.error,
                Some(format!("{message}; keeping the last loaded references"))
            );
            assert_eq!(retained.references, 1);
        }
    }

    #[test]
    fn unrelated_errors_keep_their_original_context() {
        for error in [
            SourceError::NotRegular.to_string(),
            SourceError::TooLarge.to_string(),
            SourceError::WatchUnavailable.to_string(),
            "loading the library stopped unexpectedly".to_string(),
        ] {
            assert_eq!(error_with_snapshot_context(error.clone(), true, false), error);
        }
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
            source: Some(Source::File(PathBuf::from(format!("{key}.bib")))),
            ..Library::default()
        };
        requests.0.store(2, Ordering::SeqCst); // requests 1 and 2 were made, in that order
        assert!(install(&library, &requests, 2, Ok(named("newer"))).is_some());
        assert!(install(&library, &requests, 1, Ok(named("older"))).is_none());
        let current = library.lock().map(|l| l.source.clone()).unwrap_or_default();
        assert_eq!(current, Some(Source::File(PathBuf::from("newer.bib"))));
        let failed = install(&library, &requests, 2, Err("unreadable".into()));
        assert_eq!(failed.and_then(|s| s.error).as_deref(), Some("unreadable"));
    }

    #[test]
    fn only_a_reread_that_changes_the_references_is_an_update() {
        let file = PathBuf::from("synthetic.bib");
        let read = |text: &str| candidate(file.clone(), ibis_core::bibtex::load_str("synthetic", text));
        let library = Mutex::new(Library::default());
        let requests = LoadRequests::default();
        let install_now = |new: Library| {
            let (request, _, _) = begin_open(&library, &requests, file.clone()).unwrap_or_else(|| panic!("opens"));
            install(&library, &requests, request, Ok(new)).unwrap_or_else(|| panic!("current"))
        };
        let first = install_now(read("@misc{a, title={A}}"));
        assert_eq!(first.updated_at, None, "opening is not an update");
        let same = install_now(read("@misc{a, title={A}}"));
        assert_eq!(same.updated_at, None, "rereading the same references is not an update");
        let changed = install_now(read("@misc{a, title={A}}\n@misc{b, title={B}}"));
        assert!(changed.updated_at.is_some(), "new references are an update");
        assert_eq!(
            install_now(read("@misc{a, title={A}}\n@misc{b, title={B}}")).updated_at,
            changed.updated_at
        );
    }

    #[test]
    fn a_failed_switch_is_not_a_failed_update_of_the_searched_library() {
        let open = |source: Source, error: &str| {
            let library = Mutex::new(candidate(
                source,
                ibis_core::bibtex::load_str("synthetic", "@misc{kept, title={Kept}}"),
            ));
            let requests = LoadRequests::default();
            let (request, _, _) = begin_open(&library, &requests, PathBuf::from("missing B.bib"))
                .unwrap_or_else(|| panic!("the choice starts"));
            install(&library, &requests, request, Err(error.to_string())).unwrap_or_else(|| panic!("current"))
        };
        let unavailable = SourceError::Unavailable.to_string();
        for failed in [
            open(PathBuf::from("A.bib").into(), &unavailable),
            open(
                Source::Zotero(ZoteroLibrary::User.into()),
                "the library file is not UTF-8 text",
            ),
        ] {
            assert!(!failed.stale, "{}", library_line(&failed));
            assert!(
                !library_line(&failed).contains("not updated"),
                "{}",
                library_line(&failed)
            );
        }

        // From a file to Zotero while Zotero is closed: the file is still searched.
        let library = Mutex::new(candidate(
            PathBuf::from("thesis.bib"),
            ibis_core::bibtex::load_str("synthetic", "@misc{kept, title={Kept}}"),
        ));
        let requests = LoadRequests::default();
        let (request, _, _) = begin_open(&library, &requests, Source::Zotero(ZoteroLibrary::User.into()))
            .unwrap_or_else(|| panic!("the choice starts"));
        let failed = install(&library, &requests, request, Err("Zotero is not running".into()))
            .unwrap_or_else(|| panic!("current"));
        assert!(failed.zotero, "Zotero was asked for");
        assert!(!failed.snapshot_zotero, "but the file is what is searched");
        assert!(!failed.stale);
        assert_eq!(failed.snapshot_name.as_deref(), Some("thesis.bib"));
    }

    #[test]
    fn several_zotero_libraries_are_named_listed_and_saved_in_order() {
        let chosen = ZoteroLibraries::new(vec![ZoteroLibrary::Group(7), ZoteroLibrary::User])
            .unwrap_or_else(|| panic!("valid choice"));
        let source = Source::Zotero(chosen.clone());
        assert_eq!(source.name().as_deref(), Some("2 Zotero libraries"));
        assert_eq!(source.setting(), LibrarySetting::Zotero(chosen.clone()));
        assert_eq!(Source::from(source.setting()), source);

        let library = Mutex::new(Library::default());
        let requests = LoadRequests::default();
        let (_, _, opening) =
            begin_open(&library, &requests, source).unwrap_or_else(|| panic!("the choice should start"));
        assert!(opening.zotero);
        assert_eq!(opening.zotero_libraries, [ZoteroLibrary::Group(7), ZoteroLibrary::User]);
        let file = candidate(PathBuf::from("refs.bib"), ibis_core::bibtex::load_str("synthetic", ""));
        assert!(status(&file).zotero_libraries.is_empty());
    }

    #[test]
    fn a_zotero_library_is_named_without_paths_and_keeps_its_references_when_unchanged_or_unreachable() {
        let library = Mutex::new(candidate(
            Source::Zotero(ZoteroLibrary::Group(7).into()),
            ibis_core::bibtex::load_str("synthetic", "@misc{kept, title={Kept}}"),
        ));
        let requests = LoadRequests::default();
        assert_eq!(status(&Library::default()).updated_at, None, "nothing read, no time");
        let (request, reload, refreshing) =
            begin_open(&library, &requests, Source::Zotero(ZoteroLibrary::Group(7).into()))
                .unwrap_or_else(|| panic!("the refresh should start"));
        assert!(reload);
        assert!(refreshing.zotero);
        assert_eq!(refreshing.state, LibraryState::Reloading);
        assert_eq!(refreshing.name.as_deref(), Some("Zotero group 7"));
        let unchanged = install_loaded(&library, &requests, request, Ok(Loaded::Unchanged))
            .unwrap_or_else(|| panic!("the refresh is current"));
        assert_eq!(unchanged.state, LibraryState::Loaded);
        assert_eq!(unchanged.references, 1);
        assert_eq!(unchanged.updated_at, None, "nothing changed since it was opened");
        assert!(unchanged.checked_at.is_some(), "an unchanged check confirms the copy");
        assert!(!unchanged.stale);

        let saved = "the library opened but could not be saved for next time: synthetic";
        if let Ok(mut library) = library.lock() {
            library.note = Some(saved.to_string());
        }
        let (request, _, _) = begin_open(&library, &requests, Source::Zotero(ZoteroLibrary::Group(7).into()))
            .unwrap_or_else(|| panic!("the refresh should start"));
        let kept = install_loaded(&library, &requests, request, Ok(Loaded::Unchanged))
            .unwrap_or_else(|| panic!("the refresh is current"));
        assert_eq!(
            kept.note.as_deref(),
            Some(saved),
            "an unchanged check keeps the warning"
        );

        let (request, _, _) = begin_open(&library, &requests, Source::Zotero(ZoteroLibrary::Group(7).into()))
            .unwrap_or_else(|| panic!("the refresh should start"));
        let offline = install_loaded(
            &library,
            &requests,
            request,
            Err("Zotero is not running, or it is not accepting connections".into()),
        )
        .unwrap_or_else(|| panic!("the refresh is current"));
        assert_eq!(offline.references, 1);
        assert_eq!(
            (offline.updated_at, offline.checked_at),
            (unchanged.updated_at, kept.checked_at),
            "a failed update keeps the time of the copy still searched"
        );
        assert!(offline.stale, "the searched library itself could not be updated");
        assert!(offline.snapshot_zotero);
        assert_eq!(
            offline.error.as_deref(),
            Some("Zotero is not running, or it is not accepting connections; keeping the last loaded references")
        );
        assert_eq!(
            Source::Zotero(ZoteroLibrary::User.into()).name().as_deref(),
            Some("Zotero library")
        );
    }

    #[test]
    fn a_zotero_change_check_is_not_shown_and_a_newer_choice_discards_it() {
        let zotero = Source::Zotero(ZoteroLibrary::User.into());
        let library = Mutex::new(Library::default());
        let requests = LoadRequests::default();
        // Open Zotero the ordinary way, as at start or after "Use Zotero".
        let (opened, _, _) = begin_open(&library, &requests, zotero.clone()).unwrap_or_else(|| panic!("opens"));
        let loaded = candidate(
            zotero.clone(),
            ibis_core::bibtex::load_str("synthetic", "@misc{kept, title={Kept}}"),
        );
        install_loaded(&library, &requests, opened, Ok(Loaded::New(Box::new(loaded))));
        if let Ok(mut library) = library.lock() {
            library.error = Some("Zotero is not running".into());
        }
        let shown = library.lock().ok().map(|l| status(&l)).map(|s| (s.state, s.error));
        let (request, source, _) = begin_check(&library, &requests).unwrap_or_else(|| panic!("the check starts"));
        assert_eq!(source, zotero);
        assert_eq!(
            library.lock().ok().map(|l| status(&l)).map(|s| (s.state, s.error)),
            shown
        );
        assert!(begin_check(&library, &requests).is_none(), "one check at a time");

        let unchanged = install_loaded(&library, &requests, request, Ok(Loaded::Unchanged))
            .unwrap_or_else(|| panic!("the check is current"));
        assert_eq!(unchanged.state, LibraryState::Loaded);
        assert!(unchanged.error.is_none(), "Zotero answered, so the old error goes");

        let (stale, _, _) = begin_check(&library, &requests).unwrap_or_else(|| panic!("the next check starts"));
        let (choice, _, _) = begin_open(&library, &requests, PathBuf::from("synthetic.bib"))
            .unwrap_or_else(|| panic!("the choice starts"));
        assert!(install_loaded(&library, &requests, stale, Err("Zotero is not running".into())).is_none());
        let failed = install_loaded(&library, &requests, choice, Err("the file could not be read".into()))
            .unwrap_or_else(|| panic!("the choice is current"));
        assert_eq!(
            failed.error.as_deref(),
            Some("the file could not be read; keeping the last loaded references")
        );

        let file = Mutex::new(candidate(
            PathBuf::from("synthetic.bib"),
            ibis_core::bibtex::load_str("synthetic", ""),
        ));
        assert!(begin_check(&file, &requests).is_none(), "a file has its own watcher");
    }

    #[test]
    fn a_zotero_choice_that_has_not_opened_yet_is_what_reload_and_the_picker_retry() {
        let zotero = Source::Zotero(ZoteroLibrary::User.into());
        let file = Source::File(PathBuf::from("synthetic.bib"));
        // Zotero was closed when Ibis started: nothing opened yet.
        let library = Mutex::new(Library::default());
        let requests = LoadRequests::default();
        let (request, _, _) = begin_open(&library, &requests, zotero.clone()).unwrap_or_else(|| panic!("starts"));
        install_loaded(&library, &requests, request, Err("Zotero is not running".into()));
        assert_eq!(
            library.lock().ok().and_then(|l| zotero_to_check(&l)),
            Some(zotero.clone())
        );

        // A failed Zotero choice over a loaded file retries Zotero, not the file.
        let mut over_file = candidate(file.clone(), ibis_core::bibtex::load_str("synthetic", ""));
        over_file.requested = Some(zotero.clone());
        over_file.error = Some("Zotero is not running".into());
        assert_eq!(zotero_to_check(&over_file), Some(zotero.clone()));
        assert!(retry(&over_file, zotero.clone()).1, "the choice is saved once it opens");

        // A failed file choice over a loaded Zotero library stays visible.
        let mut over_zotero = candidate(zotero, ibis_core::bibtex::load_str("synthetic", ""));
        over_zotero.requested = Some(file.clone());
        over_zotero.error = Some("the file could not be read".into());
        assert_eq!(zotero_to_check(&over_zotero), None);

        assert!(
            !retry(&over_zotero, Source::Zotero(ZoteroLibrary::User.into())).1,
            "rereading saves nothing"
        );
        let loaded_file = candidate(file, ibis_core::bibtex::load_str("synthetic", ""));
        assert_eq!(zotero_to_check(&loaded_file), None);
    }
    #[test]
    fn focus_return_recovers_the_failed_zotero_choice_without_reopening_the_picker() {
        let groups = ZoteroLibraries::new(vec![ZoteroLibrary::Group(42), ZoteroLibrary::Group(7)])
            .unwrap_or_else(|| panic!("valid chosen groups"));
        let chosen = Source::Zotero(groups.clone());
        for retained in [
            None,
            Some(Source::File(PathBuf::from("synthetic.bib"))),
            Some(Source::Zotero(ZoteroLibrary::User.into())),
        ] {
            let library = Mutex::new(
                retained
                    .map(|source| candidate(source, ibis_core::bibtex::load_str("synthetic", "@misc{old}")))
                    .unwrap_or_default(),
            );
            let requests = LoadRequests::default();
            let (first, _, _) = begin_open(&library, &requests, chosen.clone()).unwrap_or_else(|| panic!("starts"));
            install_loaded(&library, &requests, first, Err("Zotero is not running".into()));

            // The synthetic loader becomes ready. Returning focus, without
            // showing the picker again, must retry the choice that failed.
            let (recovery, source, reload) = begin_focus_retry(&library, &requests)
                .unwrap_or_else(|| panic!("focus must recover the failed chosen groups"));
            assert_eq!(source, chosen);
            assert!(!reload, "this choice has not opened yet");
            assert!(
                begin_focus_retry(&library, &requests).is_none(),
                "only one recovery at a time"
            );
            assert!(
                begin_zotero_retry(&library, &requests, false).is_none(),
                "show cannot duplicate focus"
            );
            let loaded = zotero::load_items(
                zotero::Library::Group(42),
                &[serde_json::json!({
                    "key": "SYN00001", "data": {"itemType": "book"},
                    "csljson": {"type": "book", "title": "Synthetic recovered record"}
                })],
            );
            let recovered = install_loaded(
                &library,
                &requests,
                recovery,
                Ok(Loaded::New(Box::new(candidate(source, loaded)))),
            )
            .unwrap_or_else(|| panic!("recovery is current"));
            assert_eq!(recovered.state, LibraryState::Loaded);
            assert!(recovered.error.is_none());
            assert_eq!(recovered.zotero_libraries, groups.libraries());
            assert_eq!(recovered.references, 1);
            assert_eq!(recovered.source_counts.get("zotero:group:42"), Some(&1));
            assert_eq!(recovered.source_counts.get("zotero:group:7"), Some(&0));
            assert!(
                begin_focus_retry(&library, &requests).is_none(),
                "healthy focus does not check again"
            );
        }
    }

    #[test]
    fn focus_retries_of_a_retained_zotero_snapshot_are_quiet_and_guarded() {
        let source = Source::Zotero(ZoteroLibrary::Group(42).into());
        let library = Mutex::new(candidate(
            source.clone(),
            ibis_core::bibtex::load_str("synthetic", "@misc{kept, title={Synthetic retained record}}"),
        ));
        let requests = LoadRequests::default();
        assert!(
            begin_focus_retry(&library, &requests).is_none(),
            "healthy focus does not check"
        );
        let (request, _, _) = begin_check(&library, &requests).unwrap_or_else(|| panic!("show checks"));
        assert!(
            begin_focus_retry(&library, &requests).is_none(),
            "checking is already in progress"
        );
        install_loaded(&library, &requests, request, Err("Zotero is not running".into()));
        let before = library.lock().map(|l| status(&l)).unwrap_or_else(|e| panic!("{e}"));
        let (request, retried, reload) = begin_focus_retry(&library, &requests).unwrap_or_else(|| panic!("retry"));
        assert_eq!(retried, source);
        assert!(reload, "a retained source keeps its versions and is not saved again");
        let during = library.lock().map(|l| status(&l)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((during.state, during.error), (before.state, before.error));
        assert_eq!(during.references, before.references);
        assert!(begin_focus_retry(&library, &requests).is_none());
        assert!(begin_zotero_retry(&library, &requests, false).is_none());
        let recovered = install_loaded(&library, &requests, request, Ok(Loaded::Unchanged))
            .unwrap_or_else(|| panic!("current recovery"));
        assert_eq!(recovered.state, LibraryState::Loaded);
        assert!(recovered.error.is_none());
        assert_eq!(recovered.revision, before.revision);
        assert_eq!(recovered.references, 1);
        assert!(begin_focus_retry(&library, &requests).is_none());

        let file = Source::File(PathBuf::from("missing-synthetic.bib"));
        let (request, _, _) = begin_open(&library, &requests, file).unwrap_or_else(|| panic!("file choice"));
        assert!(
            begin_focus_retry(&library, &requests).is_none(),
            "opening is already in progress"
        );
        install_loaded(&library, &requests, request, Err("synthetic file missing".into()));
        let before = requests.0.load(Ordering::SeqCst);
        assert!(
            begin_focus_retry(&library, &requests).is_none(),
            "a failed file choice stays visible"
        );
        assert_eq!(requests.0.load(Ordering::SeqCst), before);
        assert!(
            library
                .lock()
                .ok()
                .is_some_and(|l| l.error.as_deref() == Some("synthetic file missing"))
        );
        assert!(
            begin_focus_retry(&Mutex::new(Library::default()), &requests).is_none(),
            "setup selects nothing"
        );
    }

    #[test]
    fn simultaneous_focus_and_show_reserve_only_one_failed_choice_retry() {
        let library = Mutex::new(Library::default());
        let requests = LoadRequests::default();
        let (request, _, _) = begin_open(&library, &requests, Source::Zotero(ZoteroLibrary::Group(42).into()))
            .unwrap_or_else(|| panic!("initial choice"));
        install_loaded(&library, &requests, request, Err("Zotero is not running".into()));
        let retries = std::thread::scope(|scope| {
            let focus = scope.spawn(|| begin_focus_retry(&library, &requests));
            let show = scope.spawn(|| begin_zotero_retry(&library, &requests, false));
            [
                focus.join().unwrap_or_else(|e| panic!("{e:?}")),
                show.join().unwrap_or_else(|e| panic!("{e:?}")),
            ]
        });
        assert_eq!(retries.into_iter().flatten().count(), 1);
        assert_eq!(requests.0.load(Ordering::SeqCst), request + 1);
    }

    #[test]
    fn later_focus_can_retry_an_unready_endpoint_but_old_completion_cannot_replace_a_new_choice() {
        let source = Source::Zotero(ZoteroLibrary::Group(42).into());
        let library = Mutex::new(Library::default());
        let requests = LoadRequests::default();
        let (request, _, _) = begin_open(&library, &requests, source).unwrap_or_else(|| panic!("starts"));
        install_loaded(&library, &requests, request, Err("Zotero is not running".into()));
        let (unready, _, _) = begin_focus_retry(&library, &requests).unwrap_or_else(|| panic!("first return"));
        install_loaded(
            &library,
            &requests,
            unready,
            Err("Zotero did not answer in time".into()),
        );
        assert_eq!(
            requests.0.load(Ordering::SeqCst),
            unready,
            "failure starts no autonomous retry"
        );
        let (late, source, _) = begin_focus_retry(&library, &requests).unwrap_or_else(|| panic!("later return"));
        let file = Source::File(PathBuf::from("synthetic-new.bib"));
        let (newer, _, _) = begin_open(&library, &requests, file.clone()).unwrap_or_else(|| panic!("new choice"));
        assert!(
            install_loaded(
                &library,
                &requests,
                late,
                Ok(Loaded::New(Box::new(candidate(
                    source,
                    ibis_core::bibtex::load_str("synthetic", "@misc{old}"),
                )))),
            )
            .is_none()
        );
        assert!(install_loaded(&library, &requests, late, Err("old failure".into())).is_none());
        let current = install_loaded(
            &library,
            &requests,
            newer,
            Ok(Loaded::New(Box::new(candidate(
                file,
                ibis_core::bibtex::load_str("synthetic", "@misc{new}"),
            )))),
        )
        .unwrap_or_else(|| panic!("new choice is current"));
        assert!(!current.zotero);
        assert_eq!(current.name.as_deref(), Some("synthetic-new.bib"));
        assert_eq!(current.references, 1);
        assert!(begin_focus_retry(&library, &requests).is_none());
    }

    #[test]
    fn snapshot_revisions_bind_search_and_all_copy_paths_to_the_installed_index() {
        let source = Source::Zotero(ZoteroLibrary::User.into());
        let make = |key: &str, title: &str| {
            let mut loaded = ibis_core::bibtex::load_str("synthetic", &format!("@misc{{{key}, title={{{title}}}}}"));
            loaded.references[0].id = RecordId {
                source: "zotero:user".into(),
                native_key: "ITEM0001".into(),
            };
            candidate(source.clone(), loaded)
        };
        let library = Mutex::new(Library::default());
        let requests = LoadRequests::default();
        let (request, _, _) =
            begin_open(&library, &requests, source.clone()).unwrap_or_else(|| panic!("synthetic request is current"));
        let first = install(&library, &requests, request, Ok(make("oldkey", "Sleep old")))
            .unwrap_or_else(|| panic!("synthetic request is current"));
        let (old_index, old_revision) = {
            let locked = library.lock().unwrap_or_else(|e| panic!("{e}"));
            (Arc::clone(&locked.index), first.revision.clone())
        };
        let (request, _, loading) =
            begin_open(&library, &requests, source.clone()).unwrap_or_else(|| panic!("synthetic request is current"));
        assert_eq!(loading.revision, old_revision);
        let unchanged = install_loaded(&library, &requests, request, Ok(Loaded::Unchanged))
            .unwrap_or_else(|| panic!("synthetic request is current"));
        assert_eq!(unchanged.revision, old_revision);
        let (request, _, _) =
            begin_open(&library, &requests, source.clone()).unwrap_or_else(|| panic!("synthetic request is current"));
        let failed = install(&library, &requests, request, Err("Zotero is unavailable".into()))
            .unwrap_or_else(|| panic!("synthetic request is current"));
        assert_eq!(failed.revision, old_revision);
        let (request, _, _) =
            begin_open(&library, &requests, source.clone()).unwrap_or_else(|| panic!("synthetic request is current"));
        let replaced = install(&library, &requests, request, Ok(make("newkey", "Sleep new")))
            .unwrap_or_else(|| panic!("synthetic request is current"));
        assert_ne!(replaced.revision, old_revision);
        // The index has changed, but no event has been published to JavaScript.
        let late_reply = search_snapshot(
            &old_index,
            old_revision.clone(),
            "sleep",
            20,
            1,
            &AtomicU32::default(),
            None,
        );
        assert_eq!(late_reply.revision, old_revision);
        let empty = search_snapshot(
            &old_index,
            old_revision.clone(),
            "no-match",
            20,
            2,
            &AtomicU32::default(),
            None,
        );
        assert!(empty.results.is_empty());
        assert_eq!(empty.revision, old_revision);
        let late = late_reply.results;
        assert_eq!(late[0].key, "oldkey");
        assert_eq!(late[0].revision, old_revision);
        let ids = vec![late[0].id.clone()];
        let looked_up = std::cell::Cell::new(false);
        let retired = crate::copy::prepare_snapshot(&library, &old_revision, |locked| {
            looked_up.set(true);
            crate::copy::render_citation(locked.index.references(), &ids, ibis_core::cite::KeyFormat::Pandoc)
        });
        assert!(retired.is_err());
        assert!(!looked_up.get());
        assert!(
            crate::copy::prepare_snapshot(&library, &old_revision, |locked| crate::copy::csl_json(
                locked.index.references(),
                &ids
            ))
            .is_err()
        );
        let wrote = std::cell::Cell::new(false);
        assert!(
            crate::copy::write_formatted(
                &library,
                &old_revision,
                "old preview".into(),
                "old preview".into(),
                |_, _| {
                    wrote.set(true);
                    Ok(())
                }
            )
            .is_err()
        );
        assert!(!wrote.get());
        let current = crate::copy::prepare_snapshot(&library, &replaced.revision, |locked| {
            crate::copy::render_citation(locked.index.references(), &ids, ibis_core::cite::KeyFormat::Pandoc)
        })
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(current.text, "[@newkey]");
        let json = crate::copy::prepare_snapshot(&library, &replaced.revision, |locked| {
            crate::copy::csl_json(locked.index.references(), &ids)
        })
        .unwrap_or_else(|e| panic!("{e}"));
        assert!(json.contains("Sleep new"));
        crate::copy::write_formatted(
            &library,
            &replaced.revision,
            "new preview".into(),
            "new preview".into(),
            |text, html| {
                assert!(
                    library.try_lock().is_ok(),
                    "clipboard I/O does not hold the library mutex"
                );
                assert_eq!(text, "new preview");
                assert_eq!(html, "new preview");
                wrote.set(true);
                Ok(())
            },
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert!(wrote.get());
        let (request, _, _) =
            begin_open(&library, &requests, source.clone()).unwrap_or_else(|| panic!("synthetic request is current"));
        let empty = install(
            &library,
            &requests,
            request,
            Ok(candidate(source, ibis_core::bibtex::load_str("synthetic", ""))),
        )
        .unwrap_or_else(|| panic!("synthetic request is current"));
        assert!(empty.has_snapshot);
        assert_eq!(empty.references, 0);
        assert_ne!(empty.revision, replaced.revision);
        let locked = library.lock().unwrap_or_else(|e| panic!("{e}"));
        assert!(locked.require_revision(&replaced.revision).is_err());
        assert!(locked.require_revision(&empty.revision).is_ok());
        let reply = search_snapshot(
            &locked.index,
            empty.revision.clone(),
            "",
            20,
            3,
            &AtomicU32::default(),
            None,
        );
        assert!(reply.results.is_empty());
        assert_eq!(reply.revision, empty.revision);
    }
}
