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
            words("app-library-kept", &[("error", &self.to_string())])
        } else {
            self.to_string()
        }
    }
}

impl LoadError {
    /// The catalogue message that says what went wrong.
    fn words_id(self) -> &'static str {
        match self {
            LoadError::File(error) | LoadError::FileRefresh(error) => match (self, error.code) {
                (_, ErrorCode::SourceUnavailable) => "app-library-file-unavailable",
                _ if error.reason == Some(ErrorReason::CsvTemplate) => "reason-csv-template",
                _ if error.reason == Some(ErrorReason::UnknownFormat) => "reason-unknown-format",
                (LoadError::File(_), ErrorCode::BadInput) => "app-library-file-invalid",
                (LoadError::File(_), ErrorCode::ResourceLimit) => "app-library-file-too-large",
                (_, ErrorCode::BadInput) => "app-library-file-unfinished",
                (_, ErrorCode::ResourceLimit) => "app-library-work-limit",
                _ => "app-library-file-not-utf8",
            },
            LoadError::Zotero(error) => match (error.reason, error.code) {
                (Some(ErrorReason::ZoteroSharingOff), _) => "app-library-zotero-sharing-off",
                (Some(ErrorReason::ZoteroProfileNeeded), _) => "app-library-zotero-profile-needed",
                (Some(ErrorReason::ZoteroNeedsOpening), _) => "app-library-zotero-closed",
                (Some(ErrorReason::ZoteroNotFound), _) => "app-library-zotero-not-found",
                (Some(ErrorReason::ZoteroNoSpace), _) => "app-library-zotero-no-space",
                (_, ErrorCode::Cancelled) => "app-library-zotero-cancelled",
                (_, ErrorCode::ResourceLimit) => "app-library-zotero-work-limit",
                (_, ErrorCode::NewerFormat) => "app-library-zotero-newer",
                (_, ErrorCode::NotFound) => "app-library-zotero-group-not-found",
                _ => "app-library-zotero-try-again",
            },
            LoadError::NotLocal => "app-library-not-local",
            LoadError::Stopped => "app-library-stopped",
        }
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&words(self.words_id(), &[]))
    }
}

/// One of the app's own sentences from the catalogue. A missing message reads
/// as its id, which the tests below make impossible.
pub(crate) fn words(id: &str, args: &[(&str, &str)]) -> String {
    ibis_engine::catalogue::text(id, None, args).unwrap_or_else(|| id.to_owned())
}

/// A failed command as the interface receives it: the interface decides
/// from `code`, never from the text in `message`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
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
            _ => Self::failed("app-command-not-prepared"),
        }
    }
}

impl CommandError {
    /// A failure worded by this catalogue message.
    pub(crate) fn failed(id: &str) -> Self {
        Self::Failed {
            message: words(id, &[]),
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
        f.write_str(&match self {
            CommandError::SnapshotChanged => words("app-command-snapshot-changed", &[]),
            CommandError::LibraryLoading => words("app-command-library-loading", &[]),
            CommandError::Cancelled => words("app-command-cancelled", &[]),
            CommandError::SharedCitationKey { key } => words("app-command-shared-citation-key", &[("key", key)]),
            CommandError::Failed { message } => return f.write_str(message),
        })
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

    /// Every value a code can take, from its own schema.
    fn every<T: schemars::JsonSchema + serde::de::DeserializeOwned>() -> Vec<T> {
        let schema = serde_json::to_value(schemars::schema_for!(T)).unwrap_or_default();
        let values = schema["enum"].as_array().cloned().or_else(|| {
            schema["oneOf"]
                .as_array()
                .map(|variants| variants.iter().map(|variant| variant["const"].clone()).collect())
        });
        let values: Vec<T> = values
            .unwrap_or_default()
            .into_iter()
            .filter_map(|value| serde_json::from_value(value).ok())
            .collect();
        assert!(!values.is_empty(), "the schema lists its values");
        values
    }

    #[test]
    fn every_library_and_command_failure_has_catalogue_words_and_none_is_unused() {
        use std::collections::BTreeSet;
        let codes = every::<ErrorCode>();
        let reasons = every::<ErrorReason>();
        let mut loads = vec![LoadError::NotLocal, LoadError::Stopped];
        for &code in &codes {
            let error = EngineError::new(code);
            loads.extend([
                LoadError::File(error),
                LoadError::FileRefresh(error),
                LoadError::Zotero(error),
            ]);
            loads.extend(reasons.iter().flat_map(|&reason| {
                [LoadError::Zotero, LoadError::File].map(|load| load(EngineError::because(code, reason)))
            }));
        }
        let mut used: BTreeSet<&str> = loads.iter().map(|error| error.words_id()).collect();
        used.extend([
            "app-library-kept",
            "app-library-retained-not-updated",
            "app-library-not-refreshed",
            "app-library-retained-not-refreshed",
            "app-library-state-unavailable",
            "app-command-snapshot-changed",
            "app-command-library-loading",
            "app-command-cancelled",
            "app-command-shared-citation-key",
            "app-command-not-prepared",
            "app-command-source-not-opened",
        ]);
        let catalogue: BTreeSet<&str> = ibis_engine::catalogue::SOURCE
            .lines()
            .filter_map(|line| line.split_once(" =").map(|(id, _)| id))
            .filter(|id| id.starts_with("app-library-") || id.starts_with("app-command-"))
            .collect();
        // A reason the engine words for everyone is the catalogue's `reason-` message.
        let own: BTreeSet<&str> = used.iter().copied().filter(|id| !id.starts_with("reason-")).collect();
        assert_eq!(
            catalogue, own,
            "the catalogue's library and command words and the app's differ"
        );
        for id in used {
            let text = ibis_engine::catalogue::text(id, None, &[]);
            assert!(text.is_some_and(|text| !text.is_empty()), "{id} has a sentence");
        }

        let commands = [
            CommandError::SnapshotChanged,
            CommandError::LibraryLoading,
            CommandError::Cancelled,
            CommandError::SharedCitationKey { key: "same2026".into() },
            CommandError::from(EngineError::new(ErrorCode::Internal)),
            CommandError::failed("app-command-source-not-opened"),
        ];
        let shown = loads
            .iter()
            .flat_map(|error| [error.to_string(), error.with_snapshot_context(true, true)])
            .chain(commands.iter().map(CommandError::to_string));
        for text in shown {
            assert!(!text.is_empty() && !text.contains("app-"), "{text:?} is worded");
        }
        assert_eq!(
            CommandError::SharedCitationKey { key: "same2026".into() }.to_string(),
            "the citation key `same2026` belongs to more than one reference"
        );
    }
}
