//! Why the library could not be opened or used, kept as values so that
//! decisions never depend on the wording shown to the user.

use std::fmt;

use ibis_core::contract::{EngineError, ErrorCode, ErrorReason};
use serde::Serialize;

/// Why a library could not be read or installed: the engine's own error, kept
/// with what was asked of it. Text is made only for the interface and the log,
/// from fixed messages without paths or content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LoadError {
    /// Opening a library file failed.
    File(EngineError),
    /// Reading the open library file again failed.
    FileRefresh(EngineError),
    /// Reading Zotero failed.
    Zotero(EngineError),
    /// The file dialog returned something that is not a local file.
    NotLocal,
    /// The background load ended without a result.
    Stopped,
}

impl LoadError {
    /// A file could not be reached in time, or too many reads are still running.
    pub(crate) const FILE_UNAVAILABLE: Self = Self::File(EngineError::new(ErrorCode::SourceUnavailable));

    pub(super) fn file(error: EngineError) -> Self {
        Self::stopped_or(error, Self::File)
    }
    pub(super) fn file_refresh(error: EngineError) -> Self {
        Self::stopped_or(error, Self::FileRefresh)
    }
    fn stopped_or(error: EngineError, failed: fn(EngineError) -> Self) -> Self {
        if error.code == ErrorCode::Cancelled {
            Self::Stopped
        } else {
            failed(error)
        }
    }
    /// Whether the references already loaded are still searched after this
    /// failure. Zotero's references stay through any failure.
    pub(super) fn retains(self, has_references: bool, zotero: bool) -> bool {
        has_references
            && (zotero
                || match self {
                    Self::File(error) => ibis_engine::health::retains(&error, false),
                    Self::FileRefresh(error) => ibis_engine::health::retains(&error, true),
                    Self::Zotero(_) => true,
                    Self::NotLocal | Self::Stopped => false,
                })
    }
    pub(super) fn with_snapshot_context(self, has_references: bool, zotero: bool) -> String {
        if self.retains(has_references, zotero) {
            format!("{self}; keeping the last loaded references")
        } else {
            self.to_string()
        }
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            LoadError::File(error) | LoadError::FileRefresh(error) => match (self, error.code) {
                (_, ErrorCode::SourceUnavailable) => {
                    "the library file is unavailable; download any online-only file to this computer first"
                }
                (LoadError::File(_), ErrorCode::BadInput) => "the library file does not contain valid reference data",
                (LoadError::File(_), ErrorCode::ResourceLimit) => "the library file is larger than 64 MiB",
                (_, ErrorCode::BadInput) => "the library file contains an unfinished entry",
                (_, ErrorCode::ResourceLimit) => "the library exceeded the load work limit",
                _ => "the library file is not UTF-8 text; save it as UTF-8",
            },
            LoadError::Zotero(error) => match (error.reason, error.code) {
                (Some(ErrorReason::ZoteroSharingOff), _) => {
                    "Zotero is open but is not sharing your library with other applications"
                }
                (Some(ErrorReason::ZoteroProfileNeeded), _) => "choose which Zotero profile to read",
                (Some(ErrorReason::ZoteroNeedsOpening), _) => "Zotero's library cannot be read while it is closed",
                (Some(ErrorReason::ZoteroNotFound), _) => "no Zotero library was found on this computer",
                (Some(ErrorReason::ZoteroNoSpace), _) => {
                    "there is not enough free space to read Zotero while it is closed; free some space or open Zotero"
                }
                (_, ErrorCode::Cancelled) => "reading Zotero was cancelled",
                (_, ErrorCode::ResourceLimit) => "reading Zotero exceeded its work limit",
                (_, ErrorCode::NewerFormat) => "this Zotero database is newer than Ibis can read",
                (_, ErrorCode::NotFound) => {
                    "a chosen Zotero group library was not found; check that you are still a member"
                }
                _ => "Zotero could not be read just now; try again",
            },
            LoadError::NotLocal => "the chosen file is not on this computer",
            LoadError::Stopped => "loading the library stopped unexpectedly",
        })
    }
}

/// A failed command as the interface receives it: the interface decides
/// from `code`, never from the text in `message`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "code", rename_all = "snake_case")]
pub(crate) enum CommandError {
    /// The references the interface chose from are no longer the open
    /// library; search again and choose again.
    SnapshotChanged,
    /// The library is being read; ask again when it has opened.
    LibraryLoading,
    /// The work stopped because it was cancelled or replaced. Nothing failed.
    Cancelled,
    /// Another loaded record has this citation key, so citing by key would be ambiguous.
    SharedCitationKey { key: String },
    /// Anything else, with fixed text for the user.
    Failed { message: String },
}

// Engine faults (including formatter capacity/timeouts) are not source revisions.
// Cancellation is not a fault: it keeps its own code and never gets failure text.
impl From<ibis_core::contract::EngineError> for CommandError {
    fn from(error: ibis_core::contract::EngineError) -> Self {
        match error.code {
            ibis_core::contract::ErrorCode::Cancelled => Self::Cancelled,
            _ => Self::Failed {
                message: "The citation could not be prepared. Try again.".into(),
            },
        }
    }
}

impl From<String> for CommandError {
    fn from(message: String) -> Self {
        CommandError::Failed { message }
    }
}

impl From<&str> for CommandError {
    fn from(message: &str) -> Self {
        CommandError::Failed {
            message: message.to_string(),
        }
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::SnapshotChanged => f.write_str("library snapshot changed"),
            CommandError::LibraryLoading => f.write_str("library is loading"),
            CommandError::Cancelled => f.write_str("cancelled"),
            CommandError::SharedCitationKey { key } => {
                write!(f, "the citation key `{key}` belongs to more than one reference")
            }
            CommandError::Failed { message } => f.write_str(message),
        }
    }
}

/// Failures as the engine reports them, for tests.
#[cfg(test)]
pub(crate) const FILE_UNREADABLE: LoadError = LoadError::File(EngineError::new(ErrorCode::Internal));
#[cfg(test)]
pub(crate) const FILE_TOO_LARGE: LoadError = LoadError::File(EngineError::new(ErrorCode::ResourceLimit));
#[cfg(test)]
pub(crate) const REFRESH_INCOMPLETE: LoadError = LoadError::FileRefresh(EngineError::new(ErrorCode::BadInput));
#[cfg(test)]
pub(crate) const REFRESH_LIMIT: LoadError = LoadError::FileRefresh(EngineError::new(ErrorCode::ResourceLimit));

/// Zotero is closed, for tests.
#[cfg(test)]
pub(super) const ZOTERO_OFFLINE: LoadError = LoadError::Zotero(EngineError::because(
    ErrorCode::SourceUnavailable,
    ErrorReason::ZoteroNeedsOpening,
));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_read_keeps_loaded_references_unless_it_is_no_library() {
        let file = |code| LoadError::file(EngineError::new(code));
        let refresh = |code| LoadError::file_refresh(EngineError::new(code));
        for kept in [
            file(ErrorCode::SourceUnavailable),
            file(ErrorCode::Internal),
            refresh(ErrorCode::BadInput),
            refresh(ErrorCode::ResourceLimit),
            refresh(ErrorCode::SourceUnavailable),
            LoadError::Zotero(EngineError::new(ErrorCode::NotFound)),
        ] {
            assert!(kept.retains(true, false), "{kept}");
            assert!(!kept.retains(false, true), "{kept}");
        }
        for lost in [
            file(ErrorCode::BadInput),
            file(ErrorCode::ResourceLimit),
            file(ErrorCode::Cancelled),
            refresh(ErrorCode::Cancelled),
            LoadError::NotLocal,
        ] {
            assert!(!lost.retains(true, false), "{lost}");
            assert!(lost.retains(true, true), "Zotero keeps its references: {lost}");
        }
        assert_eq!(file(ErrorCode::Cancelled), LoadError::Stopped);
    }

    #[test]
    fn only_cancellation_escapes_the_engine_fault_text() {
        use ibis_core::contract::{EngineError, ErrorCode};
        assert_eq!(
            CommandError::from(EngineError::new(ErrorCode::Cancelled)),
            CommandError::Cancelled
        );
        for code in [
            ErrorCode::ResourceLimit,
            ErrorCode::SourceUnavailable,
            ErrorCode::Internal,
        ] {
            assert_eq!(
                CommandError::from(EngineError::new(code)),
                CommandError::from("The citation could not be prepared. Try again.")
            );
        }
    }

    #[test]
    fn commands_fail_with_a_stable_code() {
        let json = |error: CommandError| serde_json::to_value(error).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            json(CommandError::SnapshotChanged),
            serde_json::json!({"code": "snapshot_changed"})
        );
        assert_eq!(
            json(CommandError::LibraryLoading),
            serde_json::json!({"code": "library_loading"})
        );
        assert_eq!(json(CommandError::Cancelled), serde_json::json!({"code": "cancelled"}));
        assert_eq!(
            json(CommandError::SharedCitationKey { key: "same".into() }),
            serde_json::json!({"code": "shared_citation_key", "key": "same"})
        );
        assert_eq!(
            json("the clipboard could not be written".into()),
            serde_json::json!({"code": "failed", "message": "the clipboard could not be written"})
        );
    }
}
