//! What the interface and the tray are told about the library.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use ibis_core::settings::ZoteroLibrary;
use serde::Serialize;

use super::error::LoadError;
use super::source::{Source, zotero_library};
use super::state::Library;

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
        error: library.error.map(|error| {
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
pub(super) fn stale(library: &Library) -> bool {
    let (Some(error), Some(source)) = (&library.error, &library.source) else {
        return false;
    };
    library.requested.as_ref().is_none_or(|requested| requested == source)
        && retains(
            *error,
            !library.index.references().is_empty(),
            matches!(source, Source::Zotero(_)),
        )
}

/// A time for the interface, which shows it relative to its own clock.
pub(super) fn epoch_millis(time: SystemTime) -> Option<f64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|since| since.as_secs_f64() * 1000.0)
}

/// Added to a failure while the last good references stay searchable.
pub(crate) const KEPT: &str = "; keeping the last loaded references";

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
            let empty = install(&library, &requests, 0, Err((*error).into()))
                .unwrap_or_else(|| panic!("expected the current library request"));
            assert_eq!(empty.error.as_deref(), Some(*message));
            assert_eq!(empty.references, 0);
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
            LoadError::RevisionExhausted,
        ] {
            assert_eq!(error_with_snapshot_context(error, true, false), error.to_string());
        }
    }

    #[test]
    fn a_failed_switch_is_not_a_failed_update_of_the_searched_library() {
        let open = |source: Source, error: LoadError| {
            let library = Mutex::new(candidate(
                source,
                ibis_core::bibtex::load_str("synthetic", "@misc{kept, title={Kept}}"),
            ));
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
        let library = Mutex::new(candidate(
            PathBuf::from("thesis.bib"),
            ibis_core::bibtex::load_str("synthetic", "@misc{kept, title={Kept}}"),
        ));
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
