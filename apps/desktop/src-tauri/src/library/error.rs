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
    pub(crate) fn problem(self) -> ibis_core::contract::Problem {
        use ibis_core::contract::{Problem, Recovery as R, RecoveryInfo};
        match self {
            Self::File(error) | Self::FileRefresh(error) => {
                let mut problem: Problem = error.into();
                if error.reason.is_none() && matches!(error.code, ErrorCode::BadInput | ErrorCode::ResourceLimit) {
                    problem.info = RecoveryInfo::new(&problem.info.message.id, R::ChooseLibrary);
                }
                problem
            }
            Self::Zotero(error) => {
                let mut problem: Problem = error.into();
                if error.reason.is_none() && error.code == ErrorCode::NotFound {
                    problem.info = RecoveryInfo::new(&problem.info.message.id, R::ChooseLibrary);
                }
                problem
            }
            Self::NotLocal => Problem {
                code: "not-local".into(),
                info: RecoveryInfo::new("app-library-not-local", R::ChooseLibrary),
            },
            Self::Stopped => CommandError::Cancelled.problem(),
        }
    }
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
            // A reason is a whole sentence; here it leads into the rest.
            words("app-library-kept", &[("error", self.to_string().trim_end_matches('.'))])
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

/// Native command failures preserve engine facts until the client renders them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(into = "ibis_core::contract::Problem")]
pub(crate) enum CommandError {
    SnapshotChanged,
    LibraryLoading,
    Cancelled,
    Engine(EngineError),
    Refused(ibis_core::contract::CitationError),
    Presented(ibis_core::contract::Problem),
    NotPrepared,
    BibliographyUnavailable,
    BibliographyNotSaved,
    BibliographyNameTaken,
    BibliographyFormat,
    ClipboardUnavailable,
    SettingsUnavailable,
    RecentHistoryNotSaved,
    ProjectConflict { keys: Vec<String> },
    SourceNotOpened,
    SearchFailed,
    RecentReferencesFailed,
}

impl schemars::JsonSchema for CommandError {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "CommandError".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        ibis_core::contract::Problem::json_schema(generator)
    }
}
impl From<EngineError> for CommandError {
    fn from(error: EngineError) -> Self {
        if error.code == ErrorCode::Cancelled {
            Self::Cancelled
        } else {
            Self::Engine(error)
        }
    }
}
impl From<ibis_core::contract::CitationError> for CommandError {
    fn from(error: ibis_core::contract::CitationError) -> Self {
        Self::Refused(error)
    }
}
impl CommandError {
    pub(crate) fn problem(&self) -> ibis_core::contract::Problem {
        use ibis_core::contract::{NoRecoveryReason, Problem, Recovery as R, RecoveryInfo};
        let (code, info) = match self {
            Self::Engine(error) => return ibis_core::contract::Problem::from(*error),
            Self::Refused(error) => return error.problem(),
            Self::Presented(problem) => return problem.clone(),
            Self::SnapshotChanged => (
                "snapshot_changed",
                RecoveryInfo::new("app-command-snapshot-changed", R::SelectReferences),
            ),
            Self::LibraryLoading => (
                "library_loading",
                RecoveryInfo::new(
                    "app-command-library-loading",
                    R::None {
                        reason: NoRecoveryReason::InProgress,
                    },
                ),
            ),
            Self::Cancelled => (
                "cancelled",
                RecoveryInfo::new(
                    "app-command-cancelled",
                    R::None {
                        reason: NoRecoveryReason::Cancelled,
                    },
                ),
            ),
            Self::BibliographyUnavailable => (
                "bibliography-unavailable",
                RecoveryInfo::new("app-bibliography-unavailable", R::Retry),
            ),
            Self::BibliographyNotSaved => (
                "bibliography-not-saved",
                RecoveryInfo::new("app-bibliography-not-saved", R::Retry),
            ),
            Self::BibliographyNameTaken => (
                "bibliography-name-taken",
                RecoveryInfo::new("app-bibliography-name-taken", R::Retry),
            ),
            Self::BibliographyFormat => (
                "bibliography-format",
                RecoveryInfo::new("app-bibliography-format", R::Retry),
            ),
            Self::SettingsUnavailable => (
                "settings-unreadable",
                RecoveryInfo::new("app-settings-error-unreadable", R::Retry),
            ),
            Self::RecentHistoryNotSaved => (
                "recent-history-not-saved",
                RecoveryInfo::new("app-recent-history-not-saved", R::Retry),
            ),
            Self::ProjectConflict { keys } => (
                "project-conflict",
                RecoveryInfo::new("app-project-citation-conflict", R::ReviewLibrary).argument("keys", keys.join(", ")),
            ),
            Self::ClipboardUnavailable => (
                "clipboard-unavailable",
                RecoveryInfo::new("app-clipboard-unavailable", R::Retry),
            ),
            Self::NotPrepared => ("not-prepared", RecoveryInfo::new("app-command-not-prepared", R::Retry)),
            Self::SourceNotOpened => (
                "source-not-opened",
                RecoveryInfo::new("app-command-source-not-opened", R::Retry),
            ),
            Self::SearchFailed => (
                "search-failed",
                RecoveryInfo::new("app-failure-search", R::SearchReferences),
            ),
            Self::RecentReferencesFailed => (
                "recent-references-failed",
                RecoveryInfo::new("app-failure-recent-references", R::SearchReferences),
            ),
        };
        Problem {
            code: code.into(),
            info,
        }
    }
}
impl From<CommandError> for ibis_core::contract::Problem {
    fn from(error: CommandError) -> Self {
        error.problem()
    }
}
impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&ibis_engine::catalogue::problem(
            &self.problem(),
            ibis_engine::catalogue::Audience::App,
        ))
    }
}

/// Preserve the session's complete problem at the native boundary.
impl From<ibis_engine::citations::SessionError> for CommandError {
    fn from(error: ibis_engine::citations::SessionError) -> Self {
        Self::Presented(error.problem())
    }
}
impl std::error::Error for CommandError {}

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
    fn native_errors_preserve_the_engine_code_and_required_recovery() {
        assert_eq!(
            CommandError::from(EngineError::new(ErrorCode::Cancelled)),
            CommandError::Cancelled
        );
        for code in [
            ErrorCode::ResourceLimit,
            ErrorCode::SourceUnavailable,
            ErrorCode::Internal,
        ] {
            let error = serde_json::to_value(CommandError::from(EngineError::new(code))).unwrap_or_default();
            assert_eq!(error["code"], serde_json::to_value(code).unwrap_or_default());
            assert!(error["message"]["id"].is_string());
            assert!(error["recovery"]["action"].is_string());
            assert!(error["focus"].is_string());
        }
    }

    #[test]
    fn app_refusals_have_one_recovery_and_catalogue_arguments() {
        let error = serde_json::to_value(CommandError::from(
            ibis_engine::citations::SessionError::SharedCitationKey { key: "same".into() },
        ))
        .unwrap_or_default();
        assert_eq!(error["code"], "ambiguous-key");
        assert_eq!(error["message"]["args"]["key"], "same");
        assert_eq!(error["recovery"]["action"], "choose-style");
        assert_eq!(error["focus"], "format-picker");
        for error in [
            CommandError::SnapshotChanged,
            CommandError::LibraryLoading,
            CommandError::Cancelled,
        ] {
            let error = serde_json::to_value(error).unwrap_or_default();
            assert!(error["recovery"]["action"].is_string());
            assert!(error["focus"].is_string());
        }
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
            "app-command-citation-too-large",
            "app-command-no-library",
            "app-command-locator-missing",
            "app-command-locator-invalid",
            "app-command-page-missing",
            "app-command-page-invalid",
            "app-command-output-not-saved",
            "app-command-delivery-not-saved",
            "app-command-locator-conflict",
            "app-command-bibliography-unsupported",
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
            CommandError::from(ibis_engine::citations::SessionError::SharedCitationKey { key: "same2026".into() }),
            CommandError::from(EngineError::new(ErrorCode::Internal)),
            CommandError::SourceNotOpened,
        ];
        let shown = loads
            .iter()
            .flat_map(|error| [error.to_string(), error.with_snapshot_context(true, true)])
            .chain(commands.iter().map(CommandError::to_string));
        for text in shown {
            assert!(!text.is_empty() && !text.contains("app-"), "{text:?} is worded");
        }
        assert_eq!(
            CommandError::from(ibis_engine::citations::SessionError::SharedCitationKey { key: "same2026".into() })
                .to_string(),
            "the citation key `same2026` belongs to more than one reference"
        );
    }
}
