//! Multi-library refreshes use the real loader and retained-state installation path.

use std::sync::{Arc, Mutex};

use ibis_core::settings::{ZoteroLibraries, ZoteroLibrary};
use ibis_core::zotero::api::{Response, Transport, TransportError};
use serde_json::json;

use super::load::load_zotero;
use super::source::Source;
use super::state::{Library, LoadRequests, begin_open, install_loaded};

struct Zotero {
    version: u64,
    group_missing: bool,
    empty: bool,
}

impl Transport for Zotero {
    fn get(&mut self, path: &str) -> Result<Response, TransportError> {
        let group = path.starts_with("groups/42/");
        if group && self.group_missing {
            return Ok(Response {
                status: 404,
                ..Response::default()
            });
        }
        let (key, title) = if group {
            ("GROUP001", "Group sleep")
        } else {
            ("USER0001", "Personal sleep")
        };
        let body = if path.contains("since=") {
            if path.ends_with(&format!("since={}", self.version)) {
                json!({})
            } else {
                json!({"CHANGED": self.version})
            }
        } else if self.empty {
            json!([])
        } else {
            json!([{
                "key": key,
                "data": {"itemType": "book"},
                "csljson": json!([{"type": "book", "title": format!("{title} version{}", self.version)}]).to_string()
            }])
        };
        Ok(Response {
            status: 200,
            headers: vec![
                ("Zotero-API-Version".into(), "3".into()),
                ("Zotero-Server-ID".into(), "synthetic-zotero".into()),
                ("Last-Modified-Version".into(), self.version.to_string()),
                ("Total-Results".into(), if self.empty { "0" } else { "1" }.into()),
            ],
            body: serde_json::to_vec(&body).unwrap_or_else(|e| panic!("{e}")),
        })
    }
}

#[test]
fn a_partial_zotero_refresh_retains_both_searchable_libraries_until_recovery() {
    for (empty, reverse) in [(false, false), (false, true), (true, false)] {
        let mut selected = vec![ZoteroLibrary::User, ZoteroLibrary::Group(42)];
        if reverse {
            selected.reverse();
        }
        let chosen = ZoteroLibraries::new(selected).unwrap_or_else(|| panic!("valid libraries"));
        let source = Source::Zotero(chosen.clone());
        let library = Mutex::new(Library::default());
        let requests = LoadRequests::default();
        let mut zotero = Zotero {
            version: 1,
            group_missing: false,
            empty,
        };
        let (request, _, _) = begin_open(&library, &requests, source.clone()).unwrap_or_else(|| panic!("current"));
        let initial = install_loaded(
            &library,
            &requests,
            request,
            load_zotero(chosen.clone(), None, &mut zotero, || false),
        )
        .unwrap_or_else(|| panic!("current"));
        let references = if empty { 0 } else { 2 };
        assert_eq!(initial.references, references);
        let (index, snapshots) = {
            let locked = library.lock().unwrap_or_else(|e| panic!("{e}"));
            (
                Arc::clone(&locked.index),
                locked.zotero.clone().unwrap_or_else(|| panic!("snapshots")),
            )
        };
        zotero.group_missing = true;
        // The other library is first unchanged, then changed. Neither partial result may install.
        for version in [1, 2] {
            zotero.version = version;
            let (request, reload, _) =
                begin_open(&library, &requests, source.clone()).unwrap_or_else(|| panic!("current"));
            assert!(reload);
            let loaded = load_zotero(chosen.clone(), Some(&snapshots), &mut zotero, || false);
            let retained = install_loaded(&library, &requests, request, loaded).unwrap_or_else(|| panic!("current"));
            assert_eq!(
                retained.references, references,
                "a failed group cannot disappear from search"
            );
            assert!(retained.has_snapshot);
            assert_eq!(retained.stale, !empty);
            assert_eq!(retained.kept_references, !empty);
            assert!(retained.error.is_some());
            assert_eq!(retained.checked_at, initial.checked_at);
            assert_eq!(retained.revision, initial.revision);
            let locked = library.lock().unwrap_or_else(|e| panic!("{e}"));
            assert!(Arc::ptr_eq(&index, &locked.index));
            assert_eq!(locked.zotero.as_ref(), Some(&snapshots));
            assert_eq!(locked.index.search("personal", 10).len(), usize::from(!empty));
            assert_eq!(locked.index.search("group", 10).len(), usize::from(!empty));
            assert!(locked.index.search("version2", 10).is_empty());
        }
        zotero.group_missing = false;
        let (request, _, _) = begin_open(&library, &requests, source).unwrap_or_else(|| panic!("current"));
        let recovered = install_loaded(
            &library,
            &requests,
            request,
            load_zotero(chosen.clone(), Some(&snapshots), &mut zotero, || false),
        )
        .unwrap_or_else(|| panic!("current"));
        assert!(!recovered.stale);
        assert!(recovered.error.is_none());
        let locked = library.lock().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(locked.index.search("version2", 10).len(), references);
        assert!(
            locked
                .zotero
                .as_ref()
                .unwrap_or_else(|| panic!("snapshots"))
                .iter()
                .all(|(_, snapshot)| snapshot.as_ref().is_some_and(|snapshot| snapshot.version == 2))
        );
        let previous = locked.zotero.clone();
        drop(locked);
        // A complete, valid empty refresh still replaces a populated snapshot.
        zotero.empty = true;
        zotero.version = 3;
        let (request, _, _) =
            begin_open(&library, &requests, Source::Zotero(chosen.clone())).unwrap_or_else(|| panic!("current"));
        let cleared = install_loaded(
            &library,
            &requests,
            request,
            load_zotero(chosen, previous.as_ref(), &mut zotero, || false),
        )
        .unwrap_or_else(|| panic!("current"));
        assert_eq!(cleared.references, 0);
        assert!(cleared.error.is_none());
        assert!(
            library
                .lock()
                .unwrap_or_else(|e| panic!("{e}"))
                .index
                .search("sleep", 10)
                .is_empty()
        );
    }
}

#[test]
fn initial_partial_loads_warn_but_repeated_failures_stay_stale_until_deselected() {
    let chosen = ZoteroLibraries::new(vec![ZoteroLibrary::User, ZoteroLibrary::Group(42)])
        .unwrap_or_else(|| panic!("valid libraries"));
    let library = Mutex::new(Library::default());
    let requests = LoadRequests::default();
    let mut zotero = Zotero {
        version: 1,
        group_missing: true,
        empty: false,
    };
    let (request, _, _) =
        begin_open(&library, &requests, Source::Zotero(chosen.clone())).unwrap_or_else(|| panic!("current"));
    let initial = install_loaded(
        &library,
        &requests,
        request,
        load_zotero(chosen.clone(), None, &mut zotero, || false),
    )
    .unwrap_or_else(|| panic!("current"));
    assert_eq!(initial.references, 1);
    assert_eq!(initial.warnings, 1);
    assert!(initial.error.is_none());
    let previous = library
        .lock()
        .unwrap_or_else(|e| panic!("{e}"))
        .zotero
        .clone()
        .unwrap_or_else(|| panic!("snapshots"));
    for version in [1, 2] {
        zotero.version = version;
        let (request, reload, _) =
            begin_open(&library, &requests, Source::Zotero(chosen.clone())).unwrap_or_else(|| panic!("current"));
        assert!(reload);
        let failed = install_loaded(
            &library,
            &requests,
            request,
            load_zotero(chosen.clone(), Some(&previous), &mut zotero, || false),
        )
        .unwrap_or_else(|| panic!("current"));
        assert!(failed.stale);
        assert_eq!(failed.checked_at, initial.checked_at);
        assert_eq!(failed.revision, initial.revision);
        assert_eq!(failed.warnings, 1);
    }
    // A different choice is an initial read: desktop's load_source passes no previous versions.
    let chosen: ZoteroLibraries = ZoteroLibrary::User.into();
    let (request, reload, _) =
        begin_open(&library, &requests, Source::Zotero(chosen.clone())).unwrap_or_else(|| panic!("current"));
    assert!(!reload);
    let selected = install_loaded(
        &library,
        &requests,
        request,
        load_zotero(chosen, None, &mut zotero, || false),
    )
    .unwrap_or_else(|| panic!("current"));
    assert_eq!(selected.references, 1);
    assert_eq!(selected.warnings, 0);
    assert!(!selected.stale);
    assert!(selected.error.is_none());
}
