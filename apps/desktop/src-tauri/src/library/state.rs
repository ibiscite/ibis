//! The open library and the numbered requests that replace it. Nothing here
//! touches the app or the platform, so every rule is tested directly.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use ibis_core::model::LoadWarning;
use ibis_core::search::OwnedIndex;
use ibis_core::source::SourceSnapshot;
use ibis_core::zotero::libraries;

use super::error::{CommandError, LoadError};
use super::load::Loaded;
use super::source::Source;
use super::status::{LibraryStatus, status};

/// The library currently open in the app, indexed once when it is opened.
#[derive(Default)]
pub(crate) struct Library {
    pub(crate) source: Option<Source>,
    /// The requested source, including before it has produced an index.
    pub(crate) requested: Option<Source>,
    pub(crate) opening: bool,
    /// A Zotero change check is running; unlike `opening`, it is not shown.
    pub(super) checking: bool,
    /// Shared so a search can run without holding the library lock.
    pub(crate) index: Arc<OwnedIndex>,
    /// In-memory index revision; transported as an opaque string for JavaScript.
    pub(super) revision: u64,
    pub(crate) warnings: Vec<LoadWarning>,
    /// Exact initial load observation, reused when attaching the watcher.
    pub(super) snapshot: Option<SourceSnapshot>,
    /// The Zotero library versions loaded, so a refresh rereads only on change.
    pub(super) zotero: Option<libraries::Snapshots>,
    /// Neutral failure reason; status derives retention context from the current index.
    /// Automatic refreshes preserve a different rejected choice until the next request.
    pub(crate) error: Option<LoadError>,
    /// An automatic update problem while a different failed choice is retained.
    pub(super) retained_error: Option<LoadError>,
    /// A failed choice save, separate from a retained-source refresh problem.
    pub(crate) choice_save_warning: Option<String>,
    /// Persistent startup recovery notice; includes a path for the interface only.
    pub(crate) settings_notice: Option<String>,
    /// When a reread of the same source last brought different references
    /// during this run; `None` after the first read, so a launch or an
    /// unchanged reload is not reported as an update.
    pub(super) changed_at: Option<SystemTime>,
    /// When the source was last found to match `index`: a read, or a Zotero
    /// check that found nothing changed. Kept when a later attempt fails.
    pub(super) checked_at: Option<SystemTime>,
}

impl Library {
    pub(crate) fn require_revision(&self, expected: &str) -> Result<(), CommandError> {
        if self.source.is_none() || self.revision.to_string() != expected {
            return Err(CommandError::SnapshotChanged);
        }
        Ok(())
    }
}

/// Counts requests to open a library, so a load that finishes after a newer
/// request never replaces the newer library.
#[derive(Default)]
pub(crate) struct LoadRequests(pub(crate) AtomicU64);

pub(super) fn candidate(source: impl Into<Source>, loaded: ibis_core::model::LoadResult) -> Library {
    let now = SystemTime::now();
    Library {
        checked_at: Some(now),
        source: Some(source.into()),
        index: Arc::new(OwnedIndex::new(loaded.references)),
        warnings: loaded.warnings,
        snapshot: None,
        error: None,
        choice_save_warning: None,
        ..Library::default()
    }
}

pub(super) fn dialog_path_error(library: &mut Library) {
    library.opening = false;
    library.error = Some(LoadError::NotLocal);
}

pub(super) fn take_watch_source(
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

/// Make a loaded library current, or record why it failed to open. Returns
/// `None` when a newer request has been made since this one.
pub(crate) fn install(
    library: &Mutex<Library>,
    requests: &LoadRequests,
    request: u64,
    loaded: Result<Library, LoadError>,
) -> Option<LibraryStatus> {
    install_result(library, requests, request, loaded, false)
}

pub(super) fn install_automatic(
    library: &Mutex<Library>,
    requests: &LoadRequests,
    request: u64,
    loaded: Result<Library, LoadError>,
) -> Option<LibraryStatus> {
    install_result(library, requests, request, loaded, true)
}

pub(super) fn install_result(
    library: &Mutex<Library>,
    requests: &LoadRequests,
    request: u64,
    loaded: Result<Library, LoadError>,
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
        new.revision = library.revision.checked_add(1).ok_or(LoadError::RevisionExhausted)?;
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
                new.choice_save_warning = library.choice_save_warning.take();
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
pub(super) fn install_loaded(
    library: &Mutex<Library>,
    requests: &LoadRequests,
    request: u64,
    loaded: Result<Loaded, LoadError>,
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
pub(super) fn begin_open(
    library: &Mutex<Library>,
    requests: &LoadRequests,
    source: impl Into<Source>,
) -> Option<(u64, bool, LibraryStatus)> {
    let source = source.into();
    let mut library = library.lock().ok()?;
    Some(record_open(&mut library, requests, source))
}

pub(super) fn record_open(
    library: &mut Library,
    requests: &LoadRequests,
    source: Source,
) -> (u64, bool, LibraryStatus) {
    let request = requests.0.fetch_add(1, Ordering::SeqCst) + 1;
    let reload = library.source.as_ref() == Some(&source);
    library.requested = Some(source);
    library.opening = true;
    library.checking = false;
    library.error = None;
    library.retained_error = None;
    (request, reload, status(library))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    use ibis_core::model::RecordId;
    use ibis_core::settings::ZoteroLibrary;
    use ibis_core::source::{SourceError, SourceWatch};

    use crate::library::error::ZOTERO_OFFLINE;
    use crate::library::load::load_library;
    use crate::library::search::search_snapshot;
    use crate::library::status::LibraryState;
    use crate::tray::library_line;

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
        let failed = install(&library, &requests, second, Err(SourceError::Unavailable.into()))
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
        let failed = install(&library, &requests, request, Err(SourceError::Unavailable.into()))
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
                .map_err(LoadError::from);
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
    fn a_retained_refresh_failure_does_not_claim_an_unsaved_choice() {
        let library = Mutex::new(candidate(
            PathBuf::from("synthetic/retained.bib"),
            ibis_core::bibtex::load_str("synthetic", ""),
        ));
        let requests = LoadRequests::default();
        let (request, _, _) =
            begin_open(&library, &requests, PathBuf::from("other/attempted.bib")).unwrap_or_else(|| panic!("open"));
        let rejected = install(&library, &requests, request, Err(SourceError::Unavailable.into()))
            .unwrap_or_else(|| panic!("rejected choice"));
        assert!(rejected.note.is_none());
        let failed = install_automatic(&library, &requests, request, Err(SourceError::WatchUnavailable.into()))
            .unwrap_or_else(|| panic!("retained refresh"));
        assert!(failed.note.as_deref().is_some_and(|note| note.contains("retained.bib")));
        assert_eq!(library_line(&failed), library_line(&rejected));
    }

    #[test]
    fn retained_source_failures_and_empty_recovery_preserve_the_choice_and_existing_note() {
        let retained = PathBuf::from("synthetic/retained.bib");
        let original_note = "the library choice could not be saved";
        let library = Mutex::new(Library {
            choice_save_warning: Some(original_note.to_string()),
            settings_notice: Some("settings recovered".to_string()),
            ..candidate(
                retained.clone(),
                ibis_core::bibtex::load_str("synthetic", "@misc{first}"),
            )
        });
        let requests = LoadRequests::default();
        let (request, _, _) =
            begin_open(&library, &requests, PathBuf::from("other/attempted.bib")).unwrap_or_else(|| panic!("open"));
        let rejected = install(&library, &requests, request, Err(SourceError::Unreadable.into()))
            .unwrap_or_else(|| panic!("install"));
        // Watch-construction failures use this same automatic policy, too.
        let unavailable = install_automatic(&library, &requests, request, Err(SourceError::WatchUnavailable.into()))
            .unwrap_or_else(|| panic!("automatic install"));
        assert_eq!(unavailable.error, rejected.error);
        assert!(unavailable.choice_not_saved);
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
        assert!(empty.choice_not_saved);
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
        let partial = install_automatic(&library, &requests, request, Err(SourceError::Incomplete.into()))
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
        assert!(install_automatic(&library, &requests, request, Err(SourceError::Unavailable.into())).is_none());
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
        assert!(!loaded.choice_not_saved);
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
        install(&library, &requests, request, Err(SourceError::Unavailable.into()));
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
        let failed = install(&library, &requests, request, Err(SourceError::Unavailable.into()))
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
        let failed = install(&library, &requests, request, Err(SourceError::Unavailable.into()))
            .unwrap_or_else(|| panic!("expected the current library request"));
        assert_eq!(failed.settings_notice, opening.settings_notice);
    }

    #[test]
    fn a_nonlocal_dialog_choice_keeps_the_open_library_and_reports_the_same_tray_error() {
        let mut library = candidate(
            PathBuf::from("synthetic.bib"),
            ibis_core::bibtex::load_str("synthetic", "@misc{k}\n@misc{bad,"),
        );
        library.choice_save_warning = Some("the library choice was not saved".to_string());
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
        let failed = install(&library, &requests, 2, Err(SourceError::Unreadable.into()));
        assert_eq!(
            failed.and_then(|s| s.error).as_deref(),
            Some("the library file is not UTF-8 text; save it as UTF-8")
        );
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
            library.choice_save_warning = Some(saved.to_string());
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
        let offline = install_loaded(&library, &requests, request, Err(ZOTERO_OFFLINE))
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
        let failed = install(&library, &requests, request, Err(ZOTERO_OFFLINE))
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
