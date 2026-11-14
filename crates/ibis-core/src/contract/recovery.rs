//! Presentation is engine data. Adding a diagnostic requires choosing its recovery here.
use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{CitationError, EngineError, ErrorCode, ErrorReason};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Message {
    pub id: String,
    pub args: BTreeMap<String, String>,
}

/// Exactly one next action; absence must have an explicit, machine-readable reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum Recovery {
    ChooseStyle,
    ChooseForm,
    LocateFile,
    ChooseLibrary,
    OpenZoteroSharingHelp,
    OpenZotero,
    Retry,
    ChooseProject,
    RenameProject,
    RemoveProject,
    ReopenProjectMenu,
    FinishEditorRequest,
    ReviewSettings,
    EditCitation,
    EditInput,
    SelectReferences,
    ReduceSelection,
    ChooseLocale,
    ReviewLibrary,
    FreeDiskSpace,
    UpgradeIbis,
    ChooseBackup,
    DownloadFile,
    ChooseShortcut,
    OpenIbis,
    RestartSession,
    None { reason: NoRecoveryReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum NoRecoveryReason {
    Cancelled,
    InProgress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum FocusTarget {
    FormatPicker,
    LibraryPicker,
    SourceFile,
    ZoteroSharingHelp,
    Search,
    Results,
    Locator,
    Citation,
    CitationForm,
    Language,
    RecoveryAction,
    ProjectName,
    ProjectMenu,
    Settings,
    Request,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecoveryInfo {
    pub message: Message,
    pub recovery: Recovery,
    pub focus: FocusTarget,
}

impl RecoveryInfo {
    pub fn new(id: &str, recovery: Recovery, focus: FocusTarget) -> Self {
        Self {
            message: Message {
                id: id.into(),
                args: BTreeMap::new(),
            },
            recovery,
            focus,
        }
    }
    pub fn argument(mut self, name: &str, value: impl Into<String>) -> Self {
        self.message.args.insert(name.into(), value.into());
        self
    }
}

/// The desktop and MCP receive this same shape, without pre-rendered English.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Problem {
    pub code: String,
    #[serde(flatten)]
    pub info: RecoveryInfo,
}

fn name(code: impl Serialize) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

impl EngineError {
    pub fn recovery_info(self) -> RecoveryInfo {
        use FocusTarget as F;
        use Recovery as R;
        if let Some(error) = self.citation_error {
            return error.problem().info;
        }
        if let Some(cause) = self.strict_cause {
            let mut info = match cause {
                super::StrictCause::Loss { .. } => {
                    RecoveryInfo::new("reason-strict-citation", R::ChooseStyle, F::FormatPicker)
                }
                super::StrictCause::Refused { error } => error.problem().info,
                super::StrictCause::Source { code, reason } => EngineError {
                    reason,
                    ..EngineError::new(code)
                }
                .recovery_info(),
                super::StrictCause::SourceWarning => {
                    RecoveryInfo::new("reason-strict-citation", R::ReviewLibrary, F::SourceFile)
                }
            };
            info.message.id = "reason-strict-citation".into();
            return info;
        }
        if let Some(reason) = self.reason {
            // No catch-all: a new reason cannot compile without a recovery decision.
            let (recovery, focus) = match reason {
                ErrorReason::StrictCitation => (R::ReviewLibrary, F::LibraryPicker),
                ErrorReason::UnknownStyle | ErrorReason::UnknownOutput | ErrorReason::MissingKey => {
                    (R::ChooseStyle, F::FormatPicker)
                }
                ErrorReason::UnknownLocale => (R::ChooseLocale, F::Language),
                ErrorReason::DuplicateKey => (R::ReviewLibrary, F::SourceFile),
                ErrorReason::AmbiguousKey => (R::SelectReferences, F::Search),
                ErrorReason::DamagedEntry => (R::ReviewLibrary, F::SourceFile),
                ErrorReason::MissingFile => (R::LocateFile, F::LibraryPicker),
                ErrorReason::AppMissing | ErrorReason::NotIbis => (R::UpgradeIbis, F::RecoveryAction),
                ErrorReason::NoAnswer => (R::Retry, F::RecoveryAction),
                ErrorReason::SettingsReadTimeout => (R::Retry, F::RecoveryAction),
                ErrorReason::ZoteroSharingOff => (R::OpenZoteroSharingHelp, F::ZoteroSharingHelp),
                ErrorReason::ZoteroProfileNeeded | ErrorReason::ZoteroNotFound => (R::ChooseLibrary, F::LibraryPicker),
                ErrorReason::ZoteroNeedsOpening => (R::OpenZotero, F::RecoveryAction),
                ErrorReason::ZoteroNoSpace => (R::FreeDiskSpace, F::RecoveryAction),
                ErrorReason::CsvTemplate | ErrorReason::UnknownFormat => (R::ChooseLibrary, F::LibraryPicker),
                ErrorReason::LibraryCutShort => (R::ReviewLibrary, F::SourceFile),
                ErrorReason::DuplicateRecord | ErrorReason::RejectedRecord => (R::EditInput, F::Request),
            };
            return RecoveryInfo::new(&format!("reason-{}", name(reason)), recovery, focus);
        }
        let (recovery, focus) = match self.code {
            ErrorCode::Usage | ErrorCode::BadInput => (R::EditInput, F::Request),
            ErrorCode::NotFound => (R::SelectReferences, F::Search),
            ErrorCode::SourceUnavailable => (R::Retry, F::RecoveryAction),
            ErrorCode::ResourceLimit => (R::ReduceSelection, F::Search),
            ErrorCode::NewerFormat => (R::UpgradeIbis, F::RecoveryAction),
            ErrorCode::Internal => (R::Retry, F::RecoveryAction),
            ErrorCode::Cancelled => (
                R::None {
                    reason: NoRecoveryReason::Cancelled,
                },
                F::Search,
            ),
        };
        RecoveryInfo::new(&format!("error-{}", name(self.code)), recovery, focus)
    }
}

impl From<EngineError> for Problem {
    fn from(error: EngineError) -> Self {
        Self {
            code: error
                .citation_error
                .map_or_else(|| error.reason.map_or_else(|| name(error.code), name), name),
            info: error.recovery_info(),
        }
    }
}

impl CitationError {
    pub fn problem(self) -> Problem {
        use FocusTarget as F;
        use Recovery as R;
        let (recovery, focus) = match self {
            Self::EmptySelection | Self::MissingItem | Self::ChangedItem | Self::AnnotationMismatch => {
                (R::SelectReferences, F::Results)
            }
            Self::TooManyItems | Self::OutputTooLarge => (R::ReduceSelection, F::Results),
            Self::InvalidLocator => (R::EditCitation, F::Locator),
            Self::InvalidRichText => (R::EditCitation, F::Citation),
            Self::UnsupportedForm => (R::ChooseForm, F::CitationForm),
            Self::MissingKey
            | Self::ConflictingKey
            | Self::InvalidKey
            | Self::UnsupportedOutput
            | Self::NotFromZotero => (R::ChooseStyle, F::FormatPicker),
            Self::AmbiguousKey => (R::ChooseStyle, F::FormatPicker),
            Self::InvalidLocale => (R::ChooseLocale, F::Language),
            Self::Cancelled => (
                R::None {
                    reason: NoRecoveryReason::Cancelled,
                },
                F::Search,
            ),
            Self::FormatterUnavailable | Self::Unknown => (R::Retry, F::RecoveryAction),
            Self::NotCitable => (R::SelectReferences, F::Results),
        };
        let code = name(self);
        let id = if self == Self::Unknown {
            "error-internal".into()
        } else {
            format!("problem-{code}")
        };
        Problem {
            code,
            info: RecoveryInfo::new(&id, recovery, focus),
        }
    }
}
