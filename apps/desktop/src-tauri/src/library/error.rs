//! Why the library could not be opened or used, kept as values so that
//! decisions never depend on the wording shown to the user.

use std::fmt;

use ibis_core::contract::{EngineError, ErrorCode};
use serde::Serialize;

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
mod tests {
    use super::*;
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
}
