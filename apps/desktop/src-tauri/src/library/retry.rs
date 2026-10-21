//! When the open Zotero library is checked for changes, and when a Zotero
//! choice that failed is tried again.

use std::sync::Mutex;
use std::sync::atomic::Ordering;

use ibis_core::settings::ZoteroLibraries;
use ibis_core::zotero::libraries;

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

/// Start checking the open Zotero library for changes without showing it:
/// the picker keeps its state until Zotero has changed or cannot be reached.
/// The check is numbered like any request, so a newer choice discards it.
pub(super) fn begin_check(
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
        Source::File(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use ibis_core::settings::ZoteroLibrary;
    use ibis_core::zotero;

    use crate::library::load::Loaded;
    use crate::library::state::{begin_open, candidate, install_loaded};
    use crate::library::status::{LibraryState, status};

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
}
