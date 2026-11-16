//! Why a library could not be opened or used, kept as values so that
//! decisions never depend on the wording shown to the user.

use std::fmt;

use ibis_core::contract::{
    EngineError, ErrorCode, ErrorReason, NoRecoveryReason, Problem, Recovery as R, RecoveryInfo,
};

/// Why a library could not be read or installed: the engine's own error, kept
/// with what was asked of it. Text is made only for the interface and the log,
/// from fixed messages without paths or content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadError {
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
    pub const FILE_UNAVAILABLE: Self = Self::File(EngineError::new(ErrorCode::SourceUnavailable));
    const ZOTERO_UNAVAILABLE: Self = Self::Zotero(EngineError::new(ErrorCode::SourceUnavailable));

    pub fn problem(self) -> Problem {
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
            Self::Stopped => Problem {
                code: "cancelled".into(),
                info: RecoveryInfo::new(
                    "app-command-cancelled",
                    R::None {
                        reason: NoRecoveryReason::Cancelled,
                    },
                ),
            },
        }
    }

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
    pub fn retains(self, has_references: bool, zotero: bool) -> bool {
        has_references
            && (zotero
                || match self {
                    Self::File(error) => crate::health::retains(&error, false),
                    Self::FileRefresh(error) => crate::health::retains(&error, true),
                    Self::Zotero(_) => true,
                    Self::NotLocal | Self::Stopped => false,
                })
    }

    pub(super) fn with_snapshot_context(self, has_references: bool, zotero: bool) -> String {
        if self.retains(has_references, zotero) {
            let cause = match self {
                Self::File(error) | Self::FileRefresh(error) if error.reason == Some(ErrorReason::MissingFile) => {
                    words("app-library-file-unavailable", &[])
                }
                _ => self.to_string(),
            };
            // A reason is a whole sentence; here it leads into the rest.
            words("app-library-kept", &[("error", cause.trim_end_matches('.'))])
        } else {
            self.to_string()
        }
    }

    /// The catalogue message that says what went wrong.
    pub fn message_id(self) -> &'static str {
        match self {
            Self::File(error) | Self::FileRefresh(error) => match (self, error.code) {
                _ if error.reason == Some(ErrorReason::MissingFile) => "reason-missing-file",
                (_, ErrorCode::SourceUnavailable) => "app-library-file-unavailable",
                _ if error.reason == Some(ErrorReason::CsvTemplate) => "reason-csv-template",
                _ if error.reason == Some(ErrorReason::UnknownFormat) => "reason-unknown-format",
                (Self::File(_), ErrorCode::BadInput) => "app-library-file-invalid",
                (Self::File(_), ErrorCode::ResourceLimit) => "app-library-file-too-large",
                (_, ErrorCode::BadInput) => "app-library-file-unfinished",
                (_, ErrorCode::ResourceLimit) => "app-library-work-limit",
                _ => "app-library-file-not-utf8",
            },
            Self::Zotero(error) => match (error.reason, error.code) {
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
            Self::NotLocal => "app-library-not-local",
            Self::Stopped => "app-library-stopped",
        }
    }

    /// The failure a source without an answer in time reports.
    pub(super) fn unavailable(zotero: bool) -> Self {
        if zotero {
            Self::ZOTERO_UNAVAILABLE
        } else {
            Self::FILE_UNAVAILABLE
        }
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&words(self.message_id(), &[]))
    }
}

/// One of the app's own sentences from the catalogue. A missing message reads
/// as its id, which the tests make impossible.
pub fn words(id: &str, args: &[(&str, &str)]) -> String {
    crate::catalogue::text(id, None, args).unwrap_or_else(|| id.to_owned())
}

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
    fn every_library_failure_has_catalogue_words() {
        let mut loads = vec![LoadError::NotLocal, LoadError::Stopped];
        for code in every::<ErrorCode>() {
            let error = EngineError::new(code);
            loads.extend([
                LoadError::File(error),
                LoadError::FileRefresh(error),
                LoadError::Zotero(error),
            ]);
            loads.extend(every::<ErrorReason>().into_iter().flat_map(|reason| {
                [LoadError::Zotero, LoadError::File].map(|load| load(EngineError::because(code, reason)))
            }));
        }
        for error in loads {
            for text in [error.to_string(), error.with_snapshot_context(true, true)] {
                assert!(!text.is_empty() && !text.contains("app-"), "{text:?} is worded");
            }
        }
    }
}
