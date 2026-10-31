//! Picker-open checks for Zotero and files whose notifications are unavailable,
//! and recovery of a Zotero choice that failed.

use std::sync::Mutex;
use std::sync::atomic::Ordering;

use ibis_core::settings::ZoteroLibraries;

use super::source::Source;
use super::state::{Library, LoadRequests, record_open};

/// The Zotero libraries asked for last, or still shown after another choice
/// failed, so going back to Zotero keeps the groups that were on.
pub(super) fn zotero_choice(library: &Library) -> Option<ZoteroLibraries> {
    [library.requested.as_ref(), library.source.as_ref()]
        .into_iter()
        .flatten()
        .find_map(|source| match source {
            Source::Zotero(chosen) => Some(chosen.clone()),
            _ => None,
        })
}

/// Start checking the open library for changes without showing it:
/// the picker keeps its state until Zotero has changed or cannot be reached.
/// The check is numbered like any request, so a newer choice discards it.
pub(super) fn begin_check(library: &Mutex<Library>, requests: &LoadRequests) -> Option<(u64, Source)> {
    begin_check_with(library, requests, |file| {
        file.engine.libraries().is_ok_and(|info| {
            info.libraries
                .iter()
                .any(|info| info.watch_warning.is_some() || info.refresh_error.is_some())
        })
    })
}

fn begin_check_with(
    library: &Mutex<Library>,
    requests: &LoadRequests,
    file_needs_check: impl FnOnce(&super::engine_file::FileLibrary) -> bool,
) -> Option<(u64, Source)> {
    let mut library = library.lock().ok()?;
    let source = library.source.clone()?;
    // An installed library has no separate request; a different request is a
    // choice in progress or a failed one that must stay visible.
    let other_request = library.requested.as_ref().is_some_and(|requested| requested != &source);
    if library.opening || library.checking || other_request {
        return None;
    }
    let eligible = matches!(source, Source::Zotero(_))
        || matches!(source, Source::File(_)) && library.file.as_ref().is_some_and(|file| file_needs_check(file));
    if !eligible {
        return None;
    }
    let request = requests.0.fetch_add(1, Ordering::SeqCst) + 1;
    if matches!(source, Source::File(_)) {
        requests.2.invalidate(request);
    }
    library.checking = true;
    Some((request, source))
}

pub(super) fn begin_focus_retry(library: &Mutex<Library>, requests: &LoadRequests) -> Option<(u64, Source, bool)> {
    begin_zotero_retry(library, requests, true)
}

pub(super) fn begin_zotero_retry(
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
pub(super) fn retry(library: &Library, source: Source) -> (Source, bool) {
    let remember = library.source.as_ref() != Some(&source);
    (source, remember)
}

pub(super) fn zotero_to_check(library: &Library) -> Option<Source> {
    match library.requested.as_ref().or(library.source.as_ref())? {
        source @ Source::Zotero(_) => Some(source.clone()),
        Source::File(_) | Source::Samples(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use ibis_core::settings::ZoteroLibrary;

    use ibis_core::source::SourceError;
    use ibis_core::zotero::api::{TransportError, ZoteroError};

    use crate::library::error::{LoadError, ZOTERO_OFFLINE};
    use crate::library::load::Loaded;
    use crate::library::state::{begin_open, candidate, install_loaded};
    use crate::library::status::{LibraryState, status};

    #[test]
    fn eventless_files_revalidate_once_on_each_picker_open_and_keep_newer_choices() {
        let source = Source::File(PathBuf::from("synthetic-network.bib"));
        let library = Mutex::new(candidate(source.clone(), "@misc{before}"));
        let requests = LoadRequests::default();
        // The fake mount delivers no events. Only showing the picker asks again.
        for _ in 0..2 {
            let (request, checked) = begin_check_with(&library, &requests, |_| true)
                .unwrap_or_else(|| panic!("manual-refresh source must be checked on open"));
            assert_eq!(checked, source);
            assert!(begin_check_with(&library, &requests, |_| true).is_none());
            let loaded = candidate(source.clone(), "@misc{after,title={New references}}");
            let visible = install_loaded(&library, &requests, request, Ok(Loaded::New(Box::new(loaded))))
                .unwrap_or_else(|| panic!("current open"));
            assert_eq!(visible.references, 1);
            assert!(visible.error.is_none());
            assert!(!library.lock().unwrap_or_else(|e| panic!("{e}")).checking);
            assert_eq!(
                requests.0.load(Ordering::SeqCst),
                request,
                "closing starts no poll or retry"
            );
        }
        let (old, _) = begin_check_with(&library, &requests, |_| true).unwrap_or_else(|| panic!("open"));
        let newer = Source::File(PathBuf::from("synthetic-local.bib"));
        let (request, _, _) = begin_open(&library, &requests, newer.clone()).unwrap_or_else(|| panic!("newer choice"));
        assert!(install_loaded(&library, &requests, old, Err(SourceError::Unavailable.into())).is_none());
        let visible = install_loaded(
            &library,
            &requests,
            request,
            Ok(Loaded::New(Box::new(candidate(newer, "@misc{local}")))),
        )
        .unwrap_or_else(|| panic!("newer choice completes"));
        assert_eq!(visible.name.as_deref(), Some("synthetic-local.bib"));
        assert!(visible.error.is_none());
    }

    #[test]
    fn picker_open_file_checks_reserve_the_cache_operation_and_parse_current_bytes()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("synthetic.bib");
        std::fs::write(&path, "@misc{before}")?;
        let source = Source::File(path.clone());
        let library = Mutex::new(crate::library::load::load_library(path.clone(), false).map_err(|e| e.to_string())?);
        let requests = LoadRequests::default();
        std::fs::write(&path, "@misc{before}\n@misc{fresh,title={Fresh reference}}")?;
        let (request, checked) = begin_check_with(&library, &requests, |_| true).ok_or("picker-open check")?;
        let file = library.lock().map_err(|_| "library")?.file.clone();
        let loaded = crate::library::load::load_source_cached(
            checked,
            false,
            file,
            &ibis_engine::Cancellation::default(),
            requests.2.operation(request),
            || false,
        )
        .map_err(|e| e.to_string())?;
        let visible = install_loaded(&library, &requests, request, Ok(loaded)).ok_or("current result")?;
        assert_eq!(visible.references, 2);
        assert!(visible.error.is_none());
        assert!(visible.checked_at.is_some());
        assert_eq!(library.lock().map_err(|_| "library")?.source, Some(source));
        Ok(())
    }

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
    fn a_zotero_change_check_is_not_shown_and_a_newer_choice_discards_it() {
        let zotero = Source::Zotero(ZoteroLibrary::User.into());
        let library = Mutex::new(Library::default());
        let requests = LoadRequests::default();
        // Open Zotero the ordinary way, as at start or after "Use Zotero".
        let (opened, _, _) = begin_open(&library, &requests, zotero.clone()).unwrap_or_else(|| panic!("opens"));
        let loaded = candidate(zotero.clone(), "@misc{kept, title={Kept}}");
        install_loaded(&library, &requests, opened, Ok(Loaded::New(Box::new(loaded))));
        if let Ok(mut library) = library.lock() {
            library.error = Some(ZOTERO_OFFLINE);
        }
        let shown = library.lock().ok().map(|l| status(&l)).map(|s| (s.state, s.error));
        let (request, source) = begin_check(&library, &requests).unwrap_or_else(|| panic!("the check starts"));
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

        let (stale, _) = begin_check(&library, &requests).unwrap_or_else(|| panic!("the next check starts"));
        let (choice, _, _) = begin_open(&library, &requests, PathBuf::from("synthetic.bib"))
            .unwrap_or_else(|| panic!("the choice starts"));
        assert!(install_loaded(&library, &requests, stale, Err(ZOTERO_OFFLINE)).is_none());
        let failed = install_loaded(&library, &requests, choice, Err(SourceError::Unavailable.into()))
            .unwrap_or_else(|| panic!("the choice is current"));
        assert_eq!(
            failed.error.as_deref(),
            Some(
                "the library file is unavailable; download any online-only file to this computer first; keeping the last loaded references"
            )
        );

        let file = Mutex::new(candidate(PathBuf::from("synthetic.bib"), ""));
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
        install_loaded(&library, &requests, request, Err(ZOTERO_OFFLINE));
        assert_eq!(
            library.lock().ok().and_then(|l| zotero_to_check(&l)),
            Some(zotero.clone())
        );

        // A failed Zotero choice over a loaded file retries Zotero, not the file.
        let mut over_file = candidate(file.clone(), "");
        over_file.requested = Some(zotero.clone());
        over_file.error = Some(ZOTERO_OFFLINE);
        assert_eq!(zotero_to_check(&over_file), Some(zotero.clone()));
        assert!(retry(&over_file, zotero.clone()).1, "the choice is saved once it opens");

        // A failed file choice over a loaded Zotero library stays visible.
        let mut over_zotero = candidate(zotero, "");
        over_zotero.requested = Some(file.clone());
        over_zotero.error = Some(SourceError::Unavailable.into());
        assert_eq!(zotero_to_check(&over_zotero), None);

        assert!(
            !retry(&over_zotero, Source::Zotero(ZoteroLibrary::User.into())).1,
            "rereading saves nothing"
        );
        let loaded_file = candidate(file, "");
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
                    .map(|source| candidate(source, "@misc{old}"))
                    .unwrap_or_default(),
            );
            let requests = LoadRequests::default();
            let (first, _, _) = begin_open(&library, &requests, chosen.clone()).unwrap_or_else(|| panic!("starts"));
            install_loaded(&library, &requests, first, Err(ZOTERO_OFFLINE));

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
            let loaded = "@misc{recovered, title={Synthetic recovered record}}";
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
            "@misc{kept, title={Synthetic retained record}}",
        ));
        let requests = LoadRequests::default();
        assert!(
            begin_focus_retry(&library, &requests).is_none(),
            "healthy focus does not check"
        );
        let (request, _) = begin_check(&library, &requests).unwrap_or_else(|| panic!("show checks"));
        assert!(
            begin_focus_retry(&library, &requests).is_none(),
            "checking is already in progress"
        );
        install_loaded(&library, &requests, request, Err(ZOTERO_OFFLINE));
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
        install_loaded(&library, &requests, request, Err(SourceError::Unavailable.into()));
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
                .is_some_and(|l| l.error == Some(LoadError::Source(SourceError::Unavailable)))
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
        install_loaded(&library, &requests, request, Err(ZOTERO_OFFLINE));
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
        install_loaded(&library, &requests, request, Err(ZOTERO_OFFLINE));
        let (unready, _, _) = begin_focus_retry(&library, &requests).unwrap_or_else(|| panic!("first return"));
        install_loaded(
            &library,
            &requests,
            unready,
            Err(LoadError::Zotero(ZoteroError::Transport(TransportError::TimedOut))),
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
                Ok(Loaded::New(Box::new(candidate(source, "@misc{old}",)))),
            )
            .is_none()
        );
        assert!(install_loaded(&library, &requests, late, Err(LoadError::Stopped)).is_none());
        let current = install_loaded(
            &library,
            &requests,
            newer,
            Ok(Loaded::New(Box::new(candidate(file, "@misc{new}")))),
        )
        .unwrap_or_else(|| panic!("new choice is current"));
        assert!(!current.zotero);
        assert_eq!(current.name.as_deref(), Some("synthetic-new.bib"));
        assert_eq!(current.references, 1);
        assert!(begin_focus_retry(&library, &requests).is_none());
    }
}
