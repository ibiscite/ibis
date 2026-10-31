//! The open library and the numbered requests that replace it. Nothing here
//! touches the app or the platform, so every rule is tested directly.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use super::engine_file::FileLibrary;
use super::error::LoadError;
use super::load::{FileReload, Loaded};
use super::source::Source;
use super::status::{LibraryStatus, status};

/// The library currently open in the app: the engine that read it and how
/// the request to open it went.
#[derive(Default)]
pub(crate) struct Library {
    /// Shared so a search or copy can run without holding the library lock.
    pub(crate) file: Option<Arc<FileLibrary>>,
    /// Disconnect the blocked notification worker when its library is replaced.
    pub(super) observer: Option<ibis_engine::LibrarySubscriptionClose>,
    pub(crate) source: Option<Source>,
    /// The requested source, including before it has been read.
    pub(crate) requested: Option<Source>,
    pub(crate) opening: bool,
    /// A Zotero change check is running; unlike `opening`, it is not shown.
    pub(super) checking: bool,
    /// Neutral failure reason; status derives retention context from the current library.
    /// Automatic refreshes preserve a different rejected choice until the next request.
    pub(crate) error: Option<LoadError>,
    /// An automatic update problem while a different failed choice is retained.
    pub(super) retained_error: Option<LoadError>,
    /// A failed choice save, separate from a retained-source refresh problem.
    pub(crate) choice_save_warning: Option<String>,
    /// Settings recovery notice, refreshed after saves; includes a path for the interface only.
    pub(crate) settings_notice: Option<String>,
    /// When a reread of the same source last brought different references
    /// during this run; `None` after the first read, so a launch or an
    /// unchanged reload is not reported as an update.
    pub(super) changed_at: Option<SystemTime>,
    /// When the source was last found to match the library: a read, or a
    /// check that found nothing changed. Kept when a later attempt fails.
    pub(super) checked_at: Option<SystemTime>,
    /// Last snapshot accounted for by either a load completion or an engine event.
    pub(super) observed_revision: Option<String>,
}

impl Library {
    pub(super) fn observe_revision(&mut self, now: SystemTime) -> bool {
        let Some(revision) = self.file.as_ref().map(|file| file.revision()).filter(|s| !s.is_empty()) else {
            return false;
        };
        let changed = self
            .observed_revision
            .as_ref()
            .is_some_and(|previous| previous != &revision);
        self.observed_revision = Some(revision);
        if changed {
            self.changed_at = Some(now);
        }
        changed
    }
}

/// Counts requests to open a library, so a load that finishes after a newer
/// request never replaces the newer library.
#[derive(Default)]
pub(crate) struct LoadRequests(pub(crate) AtomicU64, pub(super) Mutex<super::tasks::OpenQueue>);

pub(super) fn dialog_path_error(library: &mut Library, requests: &LoadRequests) {
    requests.0.fetch_add(1, Ordering::SeqCst);
    library.opening = false;
    library.checking = false;
    library.error = Some(LoadError::NotLocal);
}

/// Leave a sample session without touching the saved real-library choice.
pub(super) fn leave_samples(library: &Mutex<Library>, requests: &LoadRequests) {
    let Ok(mut library) = library.lock() else { return };
    if !matches!(library.source, Some(Source::Samples(_))) && !matches!(library.requested, Some(Source::Samples(_))) {
        return;
    }
    requests.0.fetch_add(1, Ordering::SeqCst);
    if let Some(file) = &library.file {
        file.invalidate();
    }
    *library = Library {
        settings_notice: library.settings_notice.take(),
        ..Library::default()
    };
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
    match loaded {
        Ok(mut new) => {
            if new
                .file
                .as_ref()
                .zip(library.file.as_ref())
                .is_some_and(|(new, old)| Arc::ptr_eq(new, old))
            {
                new.observer = library.observer.take();
            }
            new.settings_notice = library.settings_notice.take();
            if new.source.is_some() && new.source == library.source {
                new.changed_at = library.changed_at;
                new.observed_revision = library.observed_revision.clone();
            }
            new.observe_revision(new.checked_at.unwrap_or_else(SystemTime::now));
            if failed_choice {
                new.requested = library.requested.take();
                new.error = library.error.take();
                new.choice_save_warning = library.choice_save_warning.take();
            } else if automatic && new.source == library.source {
                // A refresh never saves the choice again, so a "not saved for
                // next time" warning still holds after it brings changes.
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

/// [`install`], where a reread that found no change only ends the request
/// and keeps everything as it is.
pub(super) fn install_loaded(
    library: &Mutex<Library>,
    requests: &LoadRequests,
    request: u64,
    loaded: Result<Loaded, LoadError>,
) -> Option<LibraryStatus> {
    match loaded {
        Ok(Loaded::New(new)) => install(library, requests, request, Ok(*new)),
        Ok(Loaded::Reload(reload)) => install_reload(library, requests, request, *reload),
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
            library.observe_revision(SystemTime::now());
            // Keep the note: a "not saved for next time" warning must outlive
            // the check that showing the picker starts right after it.
            library.requested = library.source.clone();
            Some(status(&library))
        }
    }
}

/// Publication and status reconciliation share the immutable request's selection
/// guard. No cancelled completion can alter the engine retained by a failed choice.
fn install_reload(
    library: &Mutex<Library>,
    requests: &LoadRequests,
    request: u64,
    reload: FileReload,
) -> Option<LibraryStatus> {
    let mut library = library.lock().ok()?;
    if requests.0.load(Ordering::SeqCst) != request
        || library.source.as_ref() != Some(&reload.source)
        || !library
            .file
            .as_ref()
            .is_some_and(|file| Arc::ptr_eq(file, &reload.file))
    {
        return None;
    }
    match reload.file.accept_reload(reload.prepared) {
        Ok(()) => {
            library.opening = false;
            library.checking = false;
            library.error = None;
            library.retained_error = None;
            library.requested = library.source.clone();
            let now = SystemTime::now();
            library.checked_at = Some(now);
            library.observe_revision(now);
            library.choice_save_warning = None;
        }
        Err(error) => {
            library.opening = false;
            library.checking = false;
            library.error = Some(error);
        }
    }
    Some(status(&library))
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
    library.observe_revision(SystemTime::now());
    if let Some(file) = &library.file {
        file.invalidate();
    }
    library.requested = Some(source);
    library.opening = true;
    library.checking = false;
    library.error = None;
    library.retained_error = None;
    (request, reload, status(library))
}

/// A library read by the engine from `bib`, installed as `source`, for tests.
/// The file stays on disk so the engine can keep reading it.
#[cfg(test)]
pub(crate) fn candidate(source: impl Into<Source>, bib: &str) -> Library {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}")).keep();
    let path = directory.join("synthetic.bib");
    std::fs::write(&path, bib).unwrap_or_else(|e| panic!("{e}"));
    let mut library = Library {
        checked_at: Some(SystemTime::now()),
        source: Some(source.into()),
        file: Some(Arc::new(
            FileLibrary::open(&path).unwrap_or_else(|e| panic!("synthetic library: {e}")),
        )),
        ..Library::default()
    };
    library.observe_revision(SystemTime::now());
    library
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use ibis_core::settings::ZoteroLibrary;
    use ibis_core::source::SourceError;

    use crate::library::error::ZOTERO_OFFLINE;
    use crate::library::load::{load_library, load_source};
    use crate::library::status::LibraryState;
    use crate::tray::library_line;

    #[test]
    #[allow(clippy::unwrap_used)]
    fn startup_parses_fresh_and_can_cite_immediately_even_with_obsolete_cache_files() {
        use tauri::Manager;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invented.bib");
        let cache = dir.path().join("source-generations");
        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
            std::fs::DirBuilder::new().mode(0o700).create(&cache).unwrap();
            for (name, bytes) in [
                (".owner", "Ibis prepared source storage\n"),
                (".lease", ""),
                ("generation-invented", "stale saved references"),
                (
                    "manifest-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.json",
                    "untrusted old manifest",
                ),
            ] {
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(cache.join(name))
                    .unwrap()
                    .write_all(bytes.as_bytes())
                    .unwrap();
            }
        }
        // This is the startup cleanup; source loading never reads the old files.
        ibis_zotero_local::closed::remove_obsolete_file_cache(dir.path()).unwrap();
        assert!(!cache.join("generation-invented").exists());
        assert!(
            !cache
                .join("manifest-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.json")
                .exists()
        );
        for key in ["first", "reopened"] {
            #[cfg(unix)]
            let busy = (key == "reopened").then(|| {
                use std::io::Write;
                use std::os::unix::fs::OpenOptionsExt;
                let lease = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(cache.join(".lease"))
                    .unwrap();
                lease.try_lock().unwrap();
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(cache.join("generation-invented"))
                    .unwrap()
                    .write_all(b"stale saved references")
                    .unwrap();
                assert_eq!(
                    ibis_zotero_local::closed::remove_obsolete_file_cache(dir.path()),
                    Err(ibis_zotero_local::closed::PrivateStorageError::Busy)
                );
                lease
            });
            std::fs::write(&path, format!("@book{{{key},title={{Synthetic stars}}}}")).unwrap();
            let app = tauri::test::mock_builder()
                .manage(Mutex::new(Library::default()))
                .manage(super::super::PreviewRequests::default())
                .build(tauri::test::mock_context(tauri::test::noop_assets()))
                .unwrap();
            let library = app.state::<Mutex<Library>>();
            let requests = LoadRequests::default();
            let (request, reload, _) = begin_open(&library, &requests, path.clone()).unwrap();
            assert!(!reload);
            let loaded = load_source(
                Source::File(path.clone()),
                false,
                None,
                &ibis_engine::Cancellation::default(),
                || false,
            );
            let ready = install_loaded(&library, &requests, request, loaded).unwrap();
            assert_eq!(ready.state, LibraryState::Loaded);
            assert!(ready.checked_at.is_some() && ready.has_snapshot && ready.error.is_none());
            let active = library.lock().unwrap().file.clone().unwrap();
            let query =
                serde_json::from_value(serde_json::json!({"text":"synthetic","interpret_locators":false})).unwrap();
            let rows = serde_json::to_value(
                active
                    .search(&query, 10, None, &ibis_engine::Cancellation::default())
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(rows["results"].as_array().unwrap().len(), 1);
            let selection = serde_json::from_value(serde_json::json!({
                "id":rows["results"][0]["id"],"cite":rows["results"][0]["cite"]
            }))
            .unwrap();
            let preview = tauri::async_runtime::block_on(crate::copy::preview_file_citation(
                vec![selection],
                ibis_core::cite::KeyFormat::Pandoc,
                None,
                active.revision(),
                super::super::PreviewIntent {
                    page: 1,
                    request: 1,
                    cancel: false,
                },
                app.state(),
                app.state(),
            ))
            .unwrap();
            let expected = format!("[@{key}]");
            assert_eq!(preview.text, expected);
            let mut written = String::new();
            active
                .copy(&preview.ticket, &preview.content_hash, |content| {
                    written = content.text.to_owned();
                    Ok(())
                })
                .unwrap();
            assert_eq!(written, expected);
            #[cfg(unix)]
            if let Some(lease) = busy {
                lease.unlock().unwrap();
                ibis_zotero_local::closed::remove_obsolete_file_cache(dir.path()).unwrap();
                assert!(!cache.join("generation-invented").exists());
            }
        }
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
    fn a_failed_choice_survives_engine_retained_source_updates() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let retained = dir.path().join("retained.bib");
        let attempted = dir.path().join("invalid-utf8.bib");
        std::fs::write(&retained, "@misc{first,title={Retained first}}").unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(&attempted, [0xff, 0xfe]).unwrap_or_else(|e| panic!("{e}"));
        let library = Mutex::new(load_library(retained.clone(), false).unwrap_or_else(|e| panic!("{e}")));
        let file = library
            .lock()
            .unwrap_or_else(|e| panic!("{e}"))
            .file
            .clone()
            .unwrap_or_else(|| panic!("engine file"));
        let requests = LoadRequests::default();
        let (request, _, _) = begin_open(&library, &requests, attempted.clone()).unwrap_or_else(|| panic!("open"));
        let failed =
            install(&library, &requests, request, load_library(attempted, false)).unwrap_or_else(|| panic!("install"));
        let expected_error = failed.error;
        assert!(expected_error.is_some());
        file.reload().unwrap_or_else(|e| panic!("{e}"));
        let unchanged = status(&library.lock().unwrap_or_else(|e| panic!("{e}")));
        assert_eq!(unchanged.error, expected_error);
        assert_eq!(unchanged.name.as_deref(), Some("invalid-utf8.bib"));
        assert_eq!(unchanged.snapshot_name.as_deref(), Some("retained.bib"));
        assert_eq!(unchanged.references, 1);
        std::fs::write(
            &retained,
            "@misc{first,title={Retained first}}\n@misc{second,title={Actual update}}",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        file.reload().unwrap_or_else(|e| panic!("{e}"));
        let updated = status(&library.lock().unwrap_or_else(|e| panic!("{e}")));
        assert_eq!(updated.references, 2);
        assert_eq!(updated.error, expected_error);
        assert_eq!(updated.name.as_deref(), Some("invalid-utf8.bib"));
        let query = serde_json::from_value(serde_json::json!({"text":"actual","interpret_locators":false}))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            file.search(&query, 10, None, &ibis_engine::Cancellation::default())
                .unwrap_or_else(|e| panic!("{e}"))
                .results
                .len(),
            1
        );
    }

    #[test]
    fn a_retained_refresh_failure_does_not_claim_an_unsaved_choice() {
        let library = Mutex::new(candidate(PathBuf::from("synthetic/retained.bib"), ""));
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
            ..candidate(retained.clone(), "@misc{first}")
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

        let empty = install_automatic(&library, &requests, request, Ok(candidate(retained.clone(), "")))
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
                Ok(candidate(retained.clone(), "@misc{stale}"))
            )
            .is_none()
        );
        assert!(install_automatic(&library, &requests, request, Err(SourceError::Unavailable.into())).is_none());
        let loaded = install(&library, &requests, new_request, Ok(candidate(retained, "")))
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
        let library = Mutex::new(candidate(path.clone(), "@misc{first}"));
        let requests = LoadRequests::default();
        let (request, reload, _) = begin_open(&library, &requests, path.clone()).unwrap_or_else(|| panic!("open"));
        assert!(reload);
        install(&library, &requests, request, Err(SourceError::Unavailable.into()));
        let recovered = install_automatic(&library, &requests, request, Ok(candidate(path, "@misc{first}")))
            .unwrap_or_else(|| panic!("recovery"));
        assert_eq!(recovered.state, LibraryState::Loaded);
        assert_eq!(recovered.name.as_deref(), Some("selected.bib"));
        assert!(recovered.error.is_none());
        assert!(recovered.note.is_none());
    }

    #[test]
    fn an_automatic_refresh_that_brings_changes_keeps_the_unsaved_choice_warning() {
        let saved = "the library opened but could not be saved for next time: synthetic";
        for source in [
            Source::File(PathBuf::from("synthetic/selected.bib")),
            Source::Zotero(ZoteroLibrary::Group(7).into()),
        ] {
            let library = Mutex::new(Library::default());
            let requests = LoadRequests::default();
            let (request, _, _) = begin_open(&library, &requests, source.clone()).unwrap_or_else(|| panic!("open"));
            let first = candidate(source.clone(), "@misc{first}");
            install(&library, &requests, request, Ok(first)).unwrap_or_else(|| panic!("opened"));
            if let Ok(mut library) = library.lock() {
                library.choice_save_warning = Some(saved.to_string());
            }
            let changed = candidate(source.clone(), "@misc{first}\n@misc{second}");
            let refreshed = install_automatic(&library, &requests, request, Ok(changed))
                .unwrap_or_else(|| panic!("the refresh is current"));
            assert_eq!(refreshed.references, 2);
            assert!(refreshed.choice_not_saved);
            assert_eq!(
                refreshed.note.as_deref(),
                Some(saved),
                "a refresh never saves the choice"
            );
        }
    }

    #[test]
    fn a_failed_switch_distinguishes_the_attempted_file_from_retained_search_results() {
        let library = Mutex::new(candidate(
            PathBuf::from("synthetic-parent/active.bib"),
            "@misc{active, title={Retained snapshot}}",
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
        let file = library
            .lock()
            .unwrap_or_else(|e| panic!("{e}"))
            .file
            .clone()
            .unwrap_or_else(|| panic!("engine file"));
        let query = serde_json::from_value(serde_json::json!({"text":"retained","interpret_locators":false}))
            .unwrap_or_else(|e| panic!("{e}"));
        let found = file
            .search(&query, 10, None, &ibis_engine::Cancellation::default())
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(found.results.len(), 1);
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
            Ok(candidate(PathBuf::from("synthetic.bib"), "@misc{k}")),
        )
        .unwrap_or_else(|| panic!("expected the current library request"));
        assert_eq!(loaded.settings_notice, opening.settings_notice);
        let failed = install(&library, &requests, request, Err(SourceError::Unavailable.into()))
            .unwrap_or_else(|| panic!("expected the current library request"));
        assert_eq!(failed.settings_notice, opening.settings_notice);
    }

    #[test]
    fn a_nonlocal_dialog_choice_keeps_the_open_library_and_reports_the_same_tray_error() {
        let mut library = candidate(PathBuf::from("synthetic.bib"), "@misc{k}\n@misc{bad,");
        library.choice_save_warning = Some("the library choice was not saved".to_string());
        let requests = LoadRequests::default();
        let (pending, _, _) = record_open(&mut library, &requests, PathBuf::from("synthetic.bib").into());
        let before = status(&library);
        dialog_path_error(&mut library, &requests);
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
        let file = library.file.clone().unwrap_or_else(|| panic!("file"));
        super::super::tasks::engine_changed(&mut library, &file);
        assert_eq!(library.error, Some(LoadError::NotLocal));
        assert!(
            install(&Mutex::new(library), &requests, pending, Ok(Library::default())).is_none(),
            "the rejected dialog choice supersedes the pending load"
        );
    }

    #[test]
    fn failed_rereads_keep_the_references_but_valid_empty_rereads_replace_them() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = directory.path().join("reload.bib");
        std::fs::write(&file, "@misc{good, title={Retained}}").unwrap_or_else(|e| panic!("{e}"));
        let library = Mutex::new(load_library(file.clone(), false).unwrap_or_else(|e| panic!("{e}")));
        let requests = LoadRequests::default();
        let reread = || {
            let previous = library.lock().unwrap_or_else(|e| panic!("{e}")).file.clone();
            let loaded = load_source(
                Source::File(file.clone()),
                true,
                previous,
                &ibis_engine::Cancellation::default(),
                || false,
            );
            install_loaded(&library, &requests, 0, loaded).unwrap_or_else(|| panic!("current"))
        };
        for contents in ["@misc{partial,", "@string{incomplete = {Value}"] {
            std::fs::write(&file, contents).unwrap_or_else(|e| panic!("{e}"));
            let status = reread();
            assert_eq!(status.references, 1);
            assert!(status.error.is_some());
        }
        std::fs::remove_file(&file).unwrap_or_else(|e| panic!("{e}"));
        let unavailable = reread();
        assert_eq!(unavailable.references, 1);
        assert!(unavailable.error.is_some());
        std::fs::write(&file, "").unwrap_or_else(|e| panic!("{e}"));
        let empty = reread();
        assert_eq!(empty.references, 0);
        assert!(empty.error.is_none());
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
        let library = Mutex::new(Library::default());
        let requests = LoadRequests::default();
        let install_now = |new: Result<Loaded, LoadError>| {
            let (request, _, _) = begin_open(&library, &requests, file.clone()).unwrap_or_else(|| panic!("opens"));
            install_loaded(&library, &requests, request, new).unwrap_or_else(|| panic!("current"))
        };
        let first = install_now(Ok(Loaded::New(Box::new(candidate(
            file.clone(),
            "@misc{a, title={A}}",
        )))));
        assert_eq!(first.updated_at, None, "opening is not an update");
        let same = install_now(Ok(Loaded::Unchanged));
        assert_eq!(same.updated_at, None, "rereading the same references is not an update");
        let changed = install_now(Ok(Loaded::New(Box::new(candidate(
            file.clone(),
            "@misc{a, title={A}}\n@misc{b, title={B}}",
        )))));
        assert!(changed.updated_at.is_some(), "new references are an update");
        assert_eq!(install_now(Ok(Loaded::Unchanged)).updated_at, changed.updated_at);
    }

    #[test]
    fn a_zotero_library_is_named_without_paths_and_keeps_its_references_when_unchanged_or_unreachable() {
        let library = Mutex::new(candidate(
            Source::Zotero(ZoteroLibrary::Group(7).into()),
            "@misc{kept, title={Kept}}",
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
    fn the_status_revision_is_the_engine_snapshot_and_survives_a_failed_reread() {
        let source = Source::File(PathBuf::from("synthetic.bib"));
        let library = Mutex::new(Library::default());
        let requests = LoadRequests::default();
        let (request, _, _) = begin_open(&library, &requests, source.clone()).unwrap_or_else(|| panic!("current"));
        let opened = install(
            &library,
            &requests,
            request,
            Ok(candidate(source.clone(), "@misc{a, title={Sleep}}")),
        )
        .unwrap_or_else(|| panic!("current"));
        let file = library
            .lock()
            .unwrap_or_else(|e| panic!("{e}"))
            .file
            .clone()
            .unwrap_or_else(|| panic!("engine file"));
        assert!(!opened.revision.is_empty());
        assert_eq!(opened.revision, file.revision());
        let query = serde_json::from_value(serde_json::json!({"text":"sleep","interpret_locators":false}))
            .unwrap_or_else(|e| panic!("{e}"));
        let reply = file
            .search(&query, 20, None, &ibis_engine::Cancellation::default())
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(reply.revision, opened.revision);
        assert!(reply.results.iter().all(|row| row.revision == opened.revision));
        let (request, _, loading) =
            begin_open(&library, &requests, source.clone()).unwrap_or_else(|| panic!("current"));
        assert_eq!(loading.revision, opened.revision);
        let failed = install(&library, &requests, request, Err(SourceError::Unavailable.into()))
            .unwrap_or_else(|| panic!("current"));
        assert_eq!(failed.revision, opened.revision);
        assert_eq!(failed.references, 1);
    }
}
