//! What the interface and the tray are told about the library: one whole
//! snapshot, derived from the owner's state each time it changes.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use ibis_core::contract::{EngineError, ErrorCode, ErrorReason, LibraryId, Problem, Recovery};
use schemars::JsonSchema;
use serde::Serialize;

use super::error::words;
use super::{LoadError, Source, State};
use crate::LibraryKind;
use crate::settings::ZoteroLibrary;

/// Sent to the interface with the `library:changed` event.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct LibraryStatus {
    pub state: LibraryState,
    pub revision: String,
    pub source_generation: String,
    /// Whether no switch was requested, or its exact native source is the searchable source.
    pub source_is_requested: bool,
    /// Whether the requested library is Zotero rather than a file.
    pub zotero: bool,
    /// The Zotero libraries requested, in order; empty for a file.
    pub zotero_libraries: Vec<ZoteroLibrary>,
    /// A successfully opened snapshot, including a valid empty library.
    pub has_snapshot: bool,
    /// Requested file name only, including during opening and after failure.
    pub name: Option<String>,
    /// File name of the searchable snapshot; may differ from the attempted file.
    pub snapshot_name: Option<String>,
    pub references: usize,
    /// Counts in the searchable snapshot by exact source ID; includes empty loaded Zotero libraries.
    pub source_counts: BTreeMap<String, usize>,
    pub warnings: usize,
    #[serde(skip)]
    pub problem_library: Option<LibraryId>,
    /// What reading found, at most the engine's detail limit, so the footer
    /// can say which entries were skipped and where to look.
    pub reading: Vec<ReadingNotice>,
    /// A searched source cannot be watched, so edits show only after a reload.
    pub live_updates_off: bool,
    pub error: Option<String>,
    /// What kind of failure `error` is, so the picker can offer the fix.
    pub problem: Option<LibraryProblem>,
    /// Canonical engine failure. Rendering and recovery use these facts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<Problem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retained_failure: Option<Problem>,
    #[serde(skip)]
    pub error_reason: Option<LoadError>,
    #[serde(skip)]
    pub kept_references: bool,
    /// Something to tell the user that is not a failure to open.
    pub note: Option<String>,
    /// Startup settings recovery, separate from transient library status.
    pub settings_notice: Option<String>,
    /// Whether the searchable references are Zotero's, which can differ from
    /// `zotero` (the requested source) after a failed switch.
    pub snapshot_zotero: bool,
    /// The searchable snapshot contains only bundled invented references.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub sample: bool,
    /// An update of the searched library failed and its earlier references
    /// are still searched; not set when a switch to another library failed.
    pub stale: bool,
    /// When a reread of the same library last changed its references during
    /// this run, in milliseconds since 1970 (UTC); `null` if it has not.
    pub updated_at: Option<f64>,
    /// When the library was last read or found unchanged, in the same unit;
    /// when `stale`, the time of the copy still being searched.
    pub checked_at: Option<f64>,
    /// The chosen library opened but could not be saved for next time, just
    /// now: the app shows itself so the note is seen.
    #[serde(skip)]
    pub reveal: bool,
    /// Fixed messages for the app's log: never a path or a reference.
    #[serde(skip)]
    pub log: Vec<String>,
}

/// One thing reading the library found: an entry skipped or one to check.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ReadingNotice {
    /// The engine's warning code, such as `duplicate-key` or `syntax`.
    pub code: String,
    pub line: Option<u32>,
    /// The repeated key, for `duplicate-key` only: the person's own data.
    pub key: Option<String>,
    /// Why, in the engine's words, naming the field to fix when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

/// A library failure grouped by what the person can do about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LibraryProblem {
    /// The file is not there: moved, renamed or on a disconnected drive.
    Missing,
    /// The file is there but cannot be used as a library.
    Unreadable,
    /// Zotero did not answer: it is closed or still starting.
    ZoteroClosed,
    /// Zotero answered but does not let other apps read it.
    ZoteroAccessOff,
    /// Zotero is closed and which profile to read is not settled.
    ZoteroProfiles,
    /// No Zotero library is on this computer.
    ZoteroMissing,
    Other,
}

impl From<LoadError> for LibraryProblem {
    fn from(error: LoadError) -> Self {
        match error.problem().info.recovery() {
            Recovery::LocateFile => Self::Missing,
            Recovery::OpenZoteroSharingHelp => Self::ZoteroAccessOff,
            Recovery::OpenZotero => Self::ZoteroClosed,
            Recovery::ChooseLibrary => match error {
                LoadError::Zotero(error) if error.reason == Some(ErrorReason::ZoteroProfileNeeded) => {
                    Self::ZoteroProfiles
                }
                LoadError::Zotero(error) if error.reason == Some(ErrorReason::ZoteroNotFound) => Self::ZoteroMissing,
                LoadError::File(_) | LoadError::FileRefresh(_) => Self::Unreadable,
                _ => Self::Other,
            },
            Recovery::ReviewLibrary => Self::Unreadable,
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LibraryState {
    None,
    Loading,
    Loaded,
    Reloading,
    Error,
}

impl LibraryStatus {
    /// What to show when the owner itself cannot be read.
    pub fn unavailable() -> Self {
        let mut status = status(&State::default());
        status.state = LibraryState::Error;
        status.error = Some(words("app-library-state-unavailable", &[]));
        status.problem = Some(LibraryProblem::Other);
        status.failure = Some(Problem {
            code: "not-prepared".into(),
            info: ibis_core::contract::RecoveryInfo::new("app-command-not-prepared", Recovery::Retry),
        });
        status
    }
}

/// How a source is named for the engine's listing: its Zotero library, or its opaque ID.
pub fn native_source(info: &crate::LibraryInfo) -> String {
    info.zotero
        .map_or_else(|| info.id.as_str().to_owned(), |library| library.to_string())
}

pub(super) fn status(library: &State) -> LibraryStatus {
    let source = library.current.as_ref().map(|current| &current.source);
    let observed = library
        .current
        .as_ref()
        .and_then(|current| current.engine.health().ok());
    let (snapshot, sources) = observed
        .as_ref()
        .map(|(snapshot, sources)| (Some(snapshot), sources.as_slice()))
        .unwrap_or_default();
    let counts = snapshot
        .map(|snapshot| crate::health::counts(snapshot))
        .unwrap_or_default();
    let requested = library.requested.as_ref().or(source);
    let references = counts.values().sum();
    let source_is_requested = library
        .requested
        .as_ref()
        .is_none_or(|requested| source == Some(requested));
    let zotero = matches!(source, Some(Source::Zotero(_)));
    let kept_references = library.error.is_some_and(|error| error.retains(references > 0, zotero));
    let mut result = LibraryStatus {
        revision: snapshot.map_or_else(String::new, |snapshot| snapshot.fingerprints().full.as_str().to_owned()),
        source_generation: library.generation.to_string(),
        source_is_requested,
        state: if library.opening {
            if source.is_some() && source == library.requested.as_ref() {
                LibraryState::Reloading
            } else {
                LibraryState::Loading
            }
        } else if library.error.is_some() {
            LibraryState::Error
        } else if source.is_some() {
            LibraryState::Loaded
        } else {
            LibraryState::None
        },
        zotero: matches!(requested, Some(Source::Zotero(_))),
        zotero_libraries: match requested {
            Some(Source::Zotero(chosen)) => chosen.libraries().to_vec(),
            _ => Vec::new(),
        },
        has_snapshot: source.is_some(),
        name: requested.and_then(Source::name),
        snapshot_name: source.and_then(Source::name),
        references,
        source_counts: sources
            .iter()
            .map(|info| (native_source(info), counts.get(&info.id).copied().unwrap_or_default()))
            .collect(),
        warnings: sources.iter().map(|info| info.warnings_total).sum(),
        problem_library: sources
            .iter()
            .find(|info| info.warnings_total > 0)
            .map(|info| info.id.clone()),
        reading: sources
            .iter()
            .flat_map(|info| &info.warnings)
            .filter_map(|warning| {
                Some(ReadingNotice {
                    code: serde_json::to_value(warning.code).ok()?.as_str()?.to_owned(),
                    line: warning.line.and_then(|line| u32::try_from(line).ok()),
                    key: warning.key.clone(),
                    why: crate::catalogue::cause(warning),
                })
            })
            .collect(),
        live_updates_off: sources.iter().any(|info| info.watch_warning.is_some()),
        error: library
            .error
            .map(|error| error.with_snapshot_context(references > 0, zotero)),
        problem: library.error.map(LibraryProblem::from),
        failure: library.error.map(LoadError::problem),
        retained_failure: library.retained_error.map(LoadError::problem),
        error_reason: library.error,
        kept_references,
        note: match &library.retained_error {
            Some(error) => {
                let name = source.and_then(Source::name).unwrap_or_default();
                let update = words(
                    "app-library-retained-not-updated",
                    &[("name", &name), ("error", &error.to_string())],
                );
                Some(match &library.choice_save_warning {
                    Some(note) => format!("{note}\n{update}"),
                    None => update,
                })
            }
            None => library.choice_save_warning.clone(),
        },
        settings_notice: None,
        snapshot_zotero: zotero,
        sample: matches!(source, Some(Source::Samples(_))),
        stale: source.is_some() && source_is_requested && kept_references,
        updated_at: library.changed_at.and_then(epoch_millis),
        checked_at: library.checked_at.and_then(epoch_millis),
        reveal: false,
        log: [
            library.choice_save_warning.clone(),
            library.retained_error.map(|error| error.to_string()),
        ]
        .into_iter()
        .flatten()
        .collect(),
    };
    for info in sources {
        if crate::health::classify(info).1 == "refresh-failed" {
            result.stale = true;
            if result.error.is_none() {
                result.kept_references = references > 0;
                result.error = Some(refresh_failure(info.kind, info.refresh_error, info.refresh_reason));
                result.error_reason = refresh_error(info.kind, info.refresh_error, info.refresh_reason);
                result.failure = result.error_reason.map(LoadError::problem);
                result.problem = result.error_reason.map(LibraryProblem::from);
            }
        }
    }
    if result.stale && library.error.is_some() && !source_is_requested {
        let note = words("app-library-retained-not-refreshed", &[]);
        result.note = Some(match result.note.take() {
            Some(previous) => format!("{previous}\n{note}"),
            None => note,
        });
    }
    result.log.extend(result.error.clone());
    result
}

/// Why a watched refresh failed, in the engine's words when it gave a reason,
/// with the references kept.
fn refresh_failure(kind: LibraryKind, code: Option<ErrorCode>, reason: Option<ErrorReason>) -> String {
    match (kind, code, reason) {
        (LibraryKind::File, Some(code), Some(reason)) => {
            LoadError::file_refresh(EngineError::because(code, reason)).with_snapshot_context(true, false)
        }
        _ => words("app-library-not-refreshed", &[]),
    }
}

fn refresh_error(kind: LibraryKind, code: Option<ErrorCode>, reason: Option<ErrorReason>) -> Option<LoadError> {
    let error = EngineError {
        code: code?,
        reason,
        detail: None,
        strict_cause: None,
        citation_error: None,
    };
    Some(match kind {
        LibraryKind::File => LoadError::file_refresh(error),
        LibraryKind::Zotero => LoadError::Zotero(error),
        LibraryKind::Snapshot => return None,
    })
}

/// A time for the interface, which shows it relative to its own clock.
fn epoch_millis(time: SystemTime) -> Option<f64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|since| since.as_secs_f64() * 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_failures_say_why_and_keep_the_references() {
        assert_eq!(
            refresh_failure(
                LibraryKind::File,
                Some(ErrorCode::BadInput),
                Some(ErrorReason::UnknownFormat)
            ),
            "This file is not in a format Ibis reads: BibTeX, BibLaTeX, CSL-JSON, RIS or CSV; keeping the last loaded references"
        );
        assert_eq!(
            refresh_failure(LibraryKind::File, Some(ErrorCode::SourceUnavailable), None),
            "The library could not be refreshed; keeping the last loaded references."
        );
        assert_eq!(
            refresh_error(LibraryKind::Snapshot, Some(ErrorCode::Internal), None),
            None
        );
    }

    #[test]
    fn problems_group_failures_by_the_fix_they_need() {
        use LibraryProblem as P;
        let zotero = |reason| LoadError::Zotero(EngineError::because(ErrorCode::SourceUnavailable, reason));
        for (error, problem) in [
            (
                LoadError::File(EngineError::because(
                    ErrorCode::SourceUnavailable,
                    ErrorReason::MissingFile,
                )),
                P::Missing,
            ),
            (LoadError::File(EngineError::new(ErrorCode::BadInput)), P::Unreadable),
            (
                LoadError::File(EngineError::new(ErrorCode::SourceUnavailable)),
                P::Other,
            ),
            (zotero(ErrorReason::ZoteroNeedsOpening), P::ZoteroClosed),
            (zotero(ErrorReason::ZoteroSharingOff), P::ZoteroAccessOff),
            (zotero(ErrorReason::ZoteroProfileNeeded), P::ZoteroProfiles),
            (zotero(ErrorReason::ZoteroNotFound), P::ZoteroMissing),
            (LoadError::NotLocal, P::Other),
        ] {
            assert_eq!(P::from(error), problem, "{error:?}");
        }
    }
}
