//! Why the library could not be opened or used, kept as values so that
//! decisions never depend on the wording shown to the user.

use std::fmt;

use ibis_core::source::SourceError;
use ibis_core::zotero::api::ZoteroError;
use serde::Serialize;

/// Why a library could not be read or installed. Text is made only for the
/// interface and the log, from fixed messages without paths or content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LoadError {
    Source(SourceError),
    /// The engine rejected the file's reference format.
    InvalidData,
    Zotero(ZoteroError),
    EngineZotero(ibis_core::contract::EngineError),
    /// The file dialog returned something that is not a local file.
    NotLocal,
    /// The background load ended without a result.
    Stopped,
}

impl LoadError {
    /// Convert native failure evidence for the engine's retention classification.
    pub(super) fn source_error(self) -> Option<SourceError> {
        match self {
            Self::Source(error) => Some(error),
            Self::Zotero(_) | Self::EngineZotero(_) => Some(SourceError::Unavailable),
            _ => None,
        }
    }
    pub(super) fn with_snapshot_context(self, has_references: bool, zotero: bool) -> String {
        if ibis_engine::health::retains(self.source_error(), has_references, zotero) {
            format!("{self}; keeping the last loaded references")
        } else {
            self.to_string()
        }
    }
}

impl From<SourceError> for LoadError {
    fn from(error: SourceError) -> Self {
        LoadError::Source(error)
    }
}

impl From<ZoteroError> for LoadError {
    fn from(error: ZoteroError) -> Self {
        LoadError::Zotero(error)
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Source(error) => error.fmt(f),
            LoadError::InvalidData => f.write_str("the library file does not contain valid reference data"),
            LoadError::Zotero(error) => error.fmt(f),
            LoadError::EngineZotero(error) => {
                use ibis_core::contract::{ErrorCode, ErrorReason};
                f.write_str(match (error.reason, error.code) {
                    (Some(ErrorReason::ZoteroSharingOff), _) => {
                        "Zotero is open but is not sharing your library with other applications"
                    }
                    (Some(ErrorReason::ZoteroProfileNeeded), _) => "choose which Zotero profile to read",
                    (Some(ErrorReason::ZoteroNeedsOpening), _) => {
                        "Zotero's library cannot be read while it is closed"
                    }
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
                })
            }
            LoadError::NotLocal => f.write_str("the chosen file is not on this computer"),
            LoadError::Stopped => f.write_str("loading the library stopped unexpectedly"),
        }
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

/// Zotero is closed, for tests.
#[cfg(test)]
pub(super) const ZOTERO_OFFLINE: LoadError = LoadError::Zotero(ZoteroError::Transport(
    ibis_core::zotero::api::TransportError::Unreachable,
));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_failures_supply_the_existing_retention_evidence() {
        assert!(ibis_engine::health::retains(
            LoadError::Zotero(ZoteroError::GroupNotFound).source_error(),
            true,
            false
        ));
        for error in [LoadError::NotLocal, LoadError::Stopped] {
            assert!(!ibis_engine::health::retains(error.source_error(), true, false));
        }
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
