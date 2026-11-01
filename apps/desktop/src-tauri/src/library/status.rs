//! What the interface and the tray are told about the library.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use ibis_core::settings::ZoteroLibrary;
use serde::Serialize;

use super::LoadError;
use super::source::Source;
use super::state::Library;

/// Sent to the interface with the `library:changed` event.
#[derive(Clone, Serialize, specta::Type)]
pub(crate) struct LibraryStatus {
    pub(crate) state: LibraryState,
    pub(crate) revision: String,
    pub(crate) source_generation: String,
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
    /// What kind of failure `error` is, so the picker can offer the fix.
    pub(crate) problem: Option<LibraryProblem>,
    // Native presentation facts do not alter the serialized picker contract.
    #[serde(skip)]
    pub(crate) error_reason: Option<LoadError>,
    #[serde(skip)]
    pub(crate) kept_references: bool,
    #[serde(skip)]
    pub(crate) choice_not_saved: bool,
    /// Something to tell the user that is not a failure to open.
    pub(crate) note: Option<String>,
    /// Startup settings recovery, separate from transient library status.
    pub(crate) settings_notice: Option<String>,
    /// Whether the searchable references are Zotero's, which can differ from
    /// `zotero` (the requested source) after a failed switch.
    pub(crate) snapshot_zotero: bool,
    /// The searchable snapshot contains only bundled invented references.
    #[specta(optional)]
    pub(crate) sample: bool,
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

/// A library failure grouped by what the person can do about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LibraryProblem {
    /// The file is not there: moved, renamed or on a disconnected drive.
    Missing,
    /// The file is there but cannot be used as a library.
    Unreadable,
    /// Zotero did not answer: it is closed or still starting.
    ZoteroClosed,
    /// Zotero answered but does not let other apps read it.
    ZoteroAccessOff,
    Other,
}

impl From<LoadError> for LibraryProblem {
    fn from(error: LoadError) -> Self {
        use ibis_core::source::SourceError;
        use ibis_core::zotero::api::{TransportError, ZoteroError};
        match error {
            LoadError::Source(SourceError::Unavailable) => LibraryProblem::Missing,
            LoadError::InvalidData
            | LoadError::Source(
                SourceError::NotRegular | SourceError::TooLarge | SourceError::Unreadable | SourceError::Incomplete,
            ) => LibraryProblem::Unreadable,
            LoadError::Zotero(ZoteroError::Transport(TransportError::Unreachable | TransportError::TimedOut)) => {
                LibraryProblem::ZoteroClosed
            }
            LoadError::Zotero(ZoteroError::NotAllowed) => LibraryProblem::ZoteroAccessOff,
            _ => LibraryProblem::Other,
        }
    }
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

fn has_references(library: &Library) -> bool {
    library
        .file
        .as_ref()
        .and_then(|file| file.engine.snapshot().ok())
        .is_some_and(|snapshot| {
            snapshot
                .objects()
                .iter()
                .any(|item| matches!(item.object, ibis_core::contract::LibraryObject::Record(_)))
        })
}

pub(crate) fn status(library: &Library) -> LibraryStatus {
    let has_references = has_references(library);
    let mut result = LibraryStatus {
        revision: String::new(),
        source_generation: library.source_generation.to_string(),
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
        references: 0,
        source_counts: BTreeMap::new(),
        warnings: 0,
        error: library.error.map(|error| {
            let zotero = matches!(library.source, Some(Source::Zotero(_)));
            error_with_snapshot_context(error, has_references, zotero)
        }),
        problem: library.error.map(LibraryProblem::from),
        error_reason: library.error,
        kept_references: library
            .error
            .is_some_and(|error| retains(error, has_references, matches!(library.source, Some(Source::Zotero(_))))),
        choice_not_saved: library.choice_save_warning.is_some(),
        note: match &library.retained_error {
            Some(error) => {
                let name = library.source.as_ref().and_then(Source::name).unwrap_or_default();
                let update = format!("the retained library {name} could not be updated: {error}");
                Some(match &library.choice_save_warning {
                    Some(note) => format!("{note}\n{update}"),
                    None => update,
                })
            }
            None => library.choice_save_warning.clone(),
        },
        settings_notice: library.settings_notice.clone(),
        snapshot_zotero: matches!(library.source, Some(Source::Zotero(_))),
        sample: matches!(library.source, Some(Source::Samples(_))),
        stale: stale(library),
        updated_at: library.changed_at.and_then(epoch_millis),
        checked_at: library.checked_at.and_then(epoch_millis),
    };
    if let Some(file) = &library.file {
        file.status(&mut result);
    }
    if result.stale && library.error.is_some() && library.requested.is_some() && library.requested != library.source {
        let note = "The retained library could not be refreshed; keeping the last loaded references.";
        result.note = Some(match result.note.take() {
            Some(previous) => format!("{previous}\n{note}"),
            None => note.into(),
        });
    }
    result
}

/// The searched library's own update failed: the error is about the source
/// that produced the references, not a different library that was chosen.
pub(super) fn stale(library: &Library) -> bool {
    let (Some(error), Some(source)) = (&library.error, &library.source) else {
        return false;
    };
    library.requested.as_ref().is_none_or(|requested| requested == source)
        && retains(*error, has_references(library), matches!(source, Source::Zotero(_)))
}

/// A time for the interface, which shows it relative to its own clock.
pub(super) fn epoch_millis(time: SystemTime) -> Option<f64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|since| since.as_secs_f64() * 1000.0)
}

/// Added to a failure while the last good references stay searchable.
const KEPT: &str = "; keeping the last loaded references";

/// Retention belongs to the installed desktop snapshot, not the source error.
/// Any failure to refresh Zotero keeps the references already read.
pub(super) fn error_with_snapshot_context(error: LoadError, has_references: bool, zotero: bool) -> String {
    if retains(error, has_references, zotero) {
        format!("{error}{KEPT}")
    } else {
        error.to_string()
    }
}

/// Whether this failure leaves the loaded references searchable.
pub(super) fn retains(error: LoadError, has_references: bool, zotero: bool) -> bool {
    has_references && (zotero || error.keeps_references())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Mutex;

    use ibis_core::source::SourceError;

    use crate::library::error::ZOTERO_OFFLINE;
    use crate::library::load::load_library;
    use crate::library::state::{LoadRequests, begin_open, candidate, install};
    use crate::tray::library_line;

    #[test]
    fn reference_counts_exclude_attachments_in_library_status() -> Result<(), Box<dyn std::error::Error>> {
        use super::super::engine_file::FileLibrary;
        use ibis_engine::{Cancellation, Engine};
        use std::sync::Arc;

        let directory = tempfile::tempdir()?;
        let path = directory.path().join("invented.bib");
        std::fs::write(directory.path().join("invented.pdf"), b"invented attachment")?;
        std::fs::write(&path, "@book{one,title={Invented rivers},file={invented.pdf}}")?;
        let engine = Engine::new().map_err(|_| "engine")?.without_file_watches();
        engine
            .open_file_observed(&path, &Cancellation::default())
            .map_err(|_| "open library")?;
        assert_eq!(engine.libraries().map_err(|_| "libraries")?.libraries[0].objects, 2);
        let mut library = Library {
            source: Some(Source::File(path.clone())),
            file: Some(Arc::new(FileLibrary::from_engine(engine))),
            ..Library::default()
        };
        let loaded = status(&library);
        assert_eq!(loaded.references, 1);
        assert_eq!(loaded.source_counts.values().copied().collect::<Vec<_>>(), [1]);
        assert_eq!(library_line(&loaded), "invented.bib: 1 references");
        let payload = serde_json::to_value(&loaded)?;
        assert_eq!(payload["references"], 1, "the footer receives the reference count");

        std::fs::remove_file(&path)?;
        let file = library.file.as_ref().ok_or("file")?;
        library.error = Some(file.reload().err().ok_or("deleted file reloaded")?);
        let retained = status(&library);
        assert_eq!(retained.references, 1);
        assert_eq!(retained.source_counts, loaded.source_counts);
        assert!(retained.stale && retained.kept_references);
        Ok(())
    }

    #[test]
    fn non_record_sources_count_as_empty_and_do_not_claim_retained_references() -> Result<(), Box<dyn std::error::Error>>
    {
        use super::super::engine_file::FileLibrary;
        use ibis_core::contract::{LibraryId, LibraryObject, Snapshot};
        use ibis_engine::Engine;
        use serde_json::json;
        use std::sync::Arc;

        let non_records = LibraryId::file(b"invented non-record objects");
        let records = LibraryId::file(b"invented references");
        let empty = LibraryId::file(b"invented empty library");
        let id = |key| json!({"library":non_records,"key":key});
        let objects: Vec<LibraryObject> = serde_json::from_value(json!([
            {"kind":"attachment","data":{"id":id("attachment"),"parent":null,"title":"Invented PDF",
                "media_type":"application/pdf","availability":"unknown","content_hash":null}},
            {"kind":"note","data":{"id":id("note"),"parent":null,"text":"Invented note"}},
            {"kind":"annotation","data":{"id":id("annotation"),"parent":id("attachment"),
                "text":"Invented passage","comment":"","page_label":null,"page_index":null}},
            {"kind":"collection","data":{"id":id("collection"),"parent":null,"name":"Invented","items":[]}},
            {"kind":"tag","data":{"id":id("tag"),"name":"Invented","items":[]}},
            {"kind":"saved-search","data":{"id":id("search"),"name":"Invented","conditions":[]}}
        ]))?;
        let engine = Engine::from_snapshot(Arc::new(Snapshot::new(objects)?));
        engine
            .publish_library(empty.clone(), Arc::new(Snapshot::new(vec![])?))
            .map_err(|_| "publish empty library")?;
        let mut library = Library {
            source: Some(Source::File(PathBuf::from("invented.bib"))),
            file: Some(Arc::new(FileLibrary::from_engine(engine.clone()))),
            error: Some(SourceError::Unavailable.into()),
            ..Library::default()
        };
        let failed = status(&library);
        assert!(
            !failed.kept_references,
            "non-record objects are not searchable references"
        );
        assert!(!failed.stale);
        assert_eq!(
            failed.error.as_deref(),
            Some("the library file is unavailable; download any online-only file to this computer first")
        );
        assert_eq!(failed.references, 0);
        assert_eq!(
            failed.source_counts,
            BTreeMap::from([(non_records.as_str().into(), 0), (empty.as_str().into(), 0)])
        );

        let objects = serde_json::from_value(json!([
            {"kind":"record","data":{"id":{"library":records,"key":"one"},"csl":{"title":"Invented one"}}},
            {"kind":"record","data":{"id":{"library":records,"key":"two"},"csl":{"title":"Invented two"}}}
        ]))?;
        engine
            .publish_library(records.clone(), Arc::new(Snapshot::new(objects)?))
            .map_err(|_| "publish references")?;
        library.error = None;
        let populated = status(&library);
        assert_eq!(populated.references, 2);
        assert_eq!(
            populated.source_counts,
            BTreeMap::from([
                (non_records.as_str().into(), 0),
                (empty.as_str().into(), 0),
                (records.as_str().into(), 2)
            ])
        );
        Ok(())
    }

    #[test]
    fn a_failed_choice_does_not_hide_failure_or_recovery_of_the_retained_file() -> Result<(), Box<dyn std::error::Error>>
    {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("retained.bib");
        std::fs::write(&path, "@misc{retained}")?;
        let library = Mutex::new(load_library(path.clone(), false).map_err(|e| e.to_string())?);
        let requests = LoadRequests::default();
        let (request, _, _) = begin_open(&library, &requests, directory.path().join("missing.bib")).ok_or("open")?;
        let rejected = install(&library, &requests, request, Err(SourceError::Unavailable.into())).ok_or("install")?;
        let file = library.lock().map_err(|_| "library")?.file.clone().ok_or("file")?;
        std::fs::remove_file(&path)?;
        assert!(file.reload().is_err());
        let failed = status(&*library.lock().map_err(|_| "library")?);
        assert!(failed.stale, "the references being searched are now stale too");
        assert_eq!(failed.error, rejected.error, "keep the failed choice visible");
        assert!(failed.note.is_some(), "also report the retained source's failure");
        std::fs::write(&path, "@misc{retained}")?;
        file.reload().map_err(|e| e.to_string())?;
        let recovered = status(&*library.lock().map_err(|_| "library")?);
        assert!(!recovered.stale);
        assert!(recovered.note.is_none());
        assert_eq!(recovered.error, rejected.error);
        Ok(())
    }

    #[test]
    fn native_presentation_facts_do_not_change_the_picker_payload() {
        let mut library = candidate(PathBuf::from("synthetic/retained.bib"), "@misc{kept}");
        library.error = Some(SourceError::Unavailable.into());
        library.choice_save_warning = Some("the choice could not be saved".into());
        let current = status(&library);
        assert!(current.error_reason.is_some());
        assert!(current.kept_references && current.choice_not_saved);
        let payload = serde_json::to_value(&current).unwrap_or_else(|e| panic!("{e}"));
        let expected = [
            "state",
            "revision",
            "source_generation",
            "zotero",
            "zotero_libraries",
            "has_snapshot",
            "name",
            "snapshot_name",
            "references",
            "source_counts",
            "warnings",
            "error",
            "problem",
            "note",
            "settings_notice",
            "snapshot_zotero",
            "sample",
            "stale",
            "updated_at",
            "checked_at",
        ];
        let keys = payload.as_object().unwrap_or_else(|| panic!("status object"));
        assert_eq!(payload["source_generation"], library.source_generation.to_string());
        assert_eq!(keys.len(), expected.len());
        assert!(expected.iter().all(|key| keys.contains_key(*key)));
        assert_eq!(
            payload["error"],
            "the library file is unavailable; download any online-only file to this computer first; keeping the last loaded references"
        );
        assert_eq!(payload["note"], "the choice could not be saved");
        assert_eq!(payload["problem"], "missing");
        assert_eq!(payload["sample"], false);
    }

    #[test]
    fn problems_group_failures_by_the_fix_they_need() {
        use ibis_core::zotero::api::ZoteroError;
        let cases: [(LoadError, LibraryProblem); 6] = [
            (SourceError::Unavailable.into(), LibraryProblem::Missing),
            (SourceError::Unreadable.into(), LibraryProblem::Unreadable),
            (SourceError::TooLarge.into(), LibraryProblem::Unreadable),
            (ZOTERO_OFFLINE, LibraryProblem::ZoteroClosed),
            (ZoteroError::NotAllowed.into(), LibraryProblem::ZoteroAccessOff),
            (LoadError::Stopped, LibraryProblem::Other),
        ];
        for (error, problem) in cases {
            assert_eq!(LibraryProblem::from(error), problem, "{error}");
        }
    }

    #[test]
    fn only_populated_snapshots_claim_that_references_were_retained() {
        let failures = [
            (
                SourceError::Unavailable,
                "the library file is unavailable; download any online-only file to this computer first",
            ),
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
            let empty = install(&library, &requests, 0, Err((*error).into()))
                .unwrap_or_else(|| panic!("expected the current library request"));
            assert_eq!(empty.error.as_deref(), Some(*message));
            assert_eq!(empty.references, 0);
            assert!(!empty.kept_references);
            assert_eq!(empty.error_reason, Some((*error).into()));
        }
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = dir.path().join("populated.bib");
        std::fs::write(&path, "@misc{synthetic}").unwrap_or_else(|e| panic!("{e}"));
        install(&library, &requests, 0, load_library(path, false));
        for (error, message) in &failures {
            let retained = install(&library, &requests, 0, Err((*error).into()))
                .unwrap_or_else(|| panic!("expected the current library request"));
            assert_eq!(
                retained.error,
                Some(format!("{message}; keeping the last loaded references"))
            );
            assert_eq!(retained.references, 1);
            assert!(retained.kept_references);
            assert_eq!(retained.error_reason, Some((*error).into()));
        }
    }

    #[test]
    fn unrelated_errors_keep_their_original_context() {
        for error in [
            SourceError::NotRegular.into(),
            SourceError::TooLarge.into(),
            SourceError::WatchUnavailable.into(),
            LoadError::Stopped,
            LoadError::NotLocal,
        ] {
            assert_eq!(error_with_snapshot_context(error, true, false), error.to_string());
        }
    }

    #[test]
    fn a_failed_switch_is_not_a_failed_update_of_the_searched_library() {
        let open = |source: Source, error: LoadError| {
            let library = Mutex::new(candidate(source, "@misc{kept, title={Kept}}"));
            let requests = LoadRequests::default();
            let (request, _, _) = begin_open(&library, &requests, PathBuf::from("missing B.bib"))
                .unwrap_or_else(|| panic!("the choice starts"));
            install(&library, &requests, request, Err(error)).unwrap_or_else(|| panic!("current"))
        };
        for failed in [
            open(PathBuf::from("A.bib").into(), SourceError::Unavailable.into()),
            open(
                Source::Zotero(ZoteroLibrary::User.into()),
                SourceError::Unreadable.into(),
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
        let library = Mutex::new(candidate(PathBuf::from("thesis.bib"), "@misc{kept, title={Kept}}"));
        let requests = LoadRequests::default();
        let (request, _, _) = begin_open(&library, &requests, Source::Zotero(ZoteroLibrary::User.into()))
            .unwrap_or_else(|| panic!("the choice starts"));
        let failed = install(&library, &requests, request, Err(ZOTERO_OFFLINE)).unwrap_or_else(|| panic!("current"));
        // The file's references are kept, as after a failed switch to another file.
        assert_eq!(
            failed.error.as_deref(),
            Some("Zotero is not running, or it is not accepting connections; keeping the last loaded references")
        );
        assert!(failed.zotero, "Zotero was asked for");
        assert!(!failed.snapshot_zotero, "but the file is what is searched");
        assert!(!failed.stale);
        assert_eq!(failed.snapshot_name.as_deref(), Some("thesis.bib"));
    }
}
