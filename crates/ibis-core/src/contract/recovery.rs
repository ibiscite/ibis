//! Presentation is engine data. Adding a diagnostic requires choosing its recovery here.
use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{EngineError, ErrorCode, ErrorReason};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Message {
    pub id: String,
    pub args: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<CitationSurface>,
}

/// Exactly one next action; absence must have an explicit, machine-readable reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum Recovery {
    ChooseOutput,
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
    EditLocator,
    EditInput,
    SearchReferences,
    SelectReferences,
    ReduceSelection,
    RemoveReferences,
    Close,
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
    SessionReferences,
    Language,
    RecoveryAction,
    ProjectName,
    ProjectMenu,
    Settings,
    Request,
}

/// Focus is derived from the action, never independently chosen by a caller.
impl Recovery {
    pub fn focus(&self) -> FocusTarget {
        use FocusTarget as F;
        match self {
            Self::ChooseOutput | Self::ChooseStyle => F::FormatPicker,
            Self::ChooseForm => F::CitationForm,
            Self::LocateFile | Self::ChooseLibrary => F::LibraryPicker,
            Self::OpenZoteroSharingHelp => F::ZoteroSharingHelp,
            Self::ChooseProject | Self::RemoveProject | Self::ReopenProjectMenu => F::ProjectMenu,
            Self::RenameProject => F::ProjectName,
            Self::FinishEditorRequest => F::Citation,
            Self::ReviewSettings => F::Settings,
            Self::EditCitation => F::Citation,
            Self::EditLocator => F::Locator,
            Self::EditInput | Self::RestartSession => F::Request,
            Self::SearchReferences => F::Search,
            Self::SelectReferences | Self::ReduceSelection => F::Results,
            Self::RemoveReferences => F::SessionReferences,
            Self::ChooseLocale => F::Language,
            Self::ReviewLibrary => F::SourceFile,
            Self::None { .. } => F::RecoveryAction,
            Self::OpenZotero
            | Self::Retry
            | Self::FreeDiskSpace
            | Self::UpgradeIbis
            | Self::ChooseBackup
            | Self::DownloadFile
            | Self::ChooseShortcut
            | Self::OpenIbis
            | Self::Close => F::RecoveryAction,
        }
    }
}

/// Wording context only; operations choose their recovery before presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CitationSurface {
    Bibliography,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "PresentedRecovery", into = "PresentedRecovery")]
pub struct RecoveryInfo {
    pub message: Message,
    recovery: Recovery,
}

#[derive(Serialize, Deserialize, JsonSchema)]
struct PresentedRecovery {
    message: Message,
    recovery: Recovery,
    focus: FocusTarget,
}

impl From<RecoveryInfo> for PresentedRecovery {
    fn from(info: RecoveryInfo) -> Self {
        let recovery = info.recovery();
        Self {
            focus: recovery.focus(),
            message: info.message,
            recovery,
        }
    }
}
impl From<PresentedRecovery> for RecoveryInfo {
    fn from(info: PresentedRecovery) -> Self {
        Self {
            message: info.message,
            recovery: info.recovery,
        }
    }
}
impl JsonSchema for RecoveryInfo {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "RecoveryInfo".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        PresentedRecovery::json_schema(generator)
    }
}

impl RecoveryInfo {
    pub fn recovery(&self) -> Recovery {
        self.recovery
    }
    pub fn new(id: &str, recovery: Recovery) -> Self {
        Self {
            message: Message {
                id: id.into(),
                args: BTreeMap::new(),
                context: None,
            },
            recovery,
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
        use Recovery as R;
        if let Some(error) = self.citation_error {
            return error.problem().info;
        }
        if let Some(cause) = self.strict_cause {
            let mut info = match cause {
                super::StrictCause::Loss { .. } => RecoveryInfo::new("reason-strict-citation", R::ChooseStyle),
                super::StrictCause::Refused { error } => error.problem().info,
                super::StrictCause::Source { code, reason } => EngineError {
                    reason,
                    ..EngineError::new(code)
                }
                .recovery_info(),
                super::StrictCause::SourceWarning => RecoveryInfo::new("reason-strict-citation", R::ReviewLibrary),
            };
            info.message.id = "reason-strict-citation".into();
            return info;
        }
        if let Some(reason) = self.reason {
            // No catch-all: a new reason cannot compile without a recovery decision.
            let recovery = match reason {
                ErrorReason::StrictCitation => R::ReviewLibrary,
                ErrorReason::UnknownStyle | ErrorReason::MissingKey => R::ChooseStyle,
                ErrorReason::UnknownOutput => R::ChooseOutput,
                ErrorReason::UnknownLocale => R::ChooseLocale,
                ErrorReason::DuplicateKey => R::ReviewLibrary,
                ErrorReason::AmbiguousKey => R::SearchReferences,
                ErrorReason::DamagedEntry => R::ReviewLibrary,
                ErrorReason::MissingFile => R::LocateFile,
                ErrorReason::AppMissing | ErrorReason::NotIbis => R::UpgradeIbis,
                ErrorReason::NoAnswer => R::Retry,
                ErrorReason::SettingsReadTimeout => R::Retry,
                ErrorReason::ZoteroSharingOff => R::OpenZoteroSharingHelp,
                ErrorReason::ZoteroProfileNeeded | ErrorReason::ZoteroNotFound => R::ChooseLibrary,
                ErrorReason::ZoteroNeedsOpening => R::OpenZotero,
                ErrorReason::ZoteroNoSpace => R::FreeDiskSpace,
                ErrorReason::CsvTemplate | ErrorReason::UnknownFormat => R::ChooseLibrary,
                ErrorReason::LibraryCutShort => R::ReviewLibrary,
                ErrorReason::DuplicateRecord | ErrorReason::RejectedRecord => R::EditInput,
            };
            return RecoveryInfo::new(&format!("reason-{}", name(reason)), recovery);
        }
        let recovery = match self.code {
            ErrorCode::Usage | ErrorCode::BadInput => R::EditInput,
            ErrorCode::NotFound => R::SearchReferences,
            ErrorCode::SourceUnavailable => R::Retry,
            ErrorCode::ResourceLimit => R::ReduceSelection,
            ErrorCode::NewerFormat => R::UpgradeIbis,
            ErrorCode::Internal => R::Retry,
            ErrorCode::Cancelled => R::None {
                reason: NoRecoveryReason::Cancelled,
            },
        };
        RecoveryInfo::new(&format!("error-{}", name(self.code)), recovery)
    }
}

impl Problem {
    pub fn recover_with(mut self, recovery: Recovery) -> Self {
        self.info.recovery = recovery;
        self
    }
    pub fn on(mut self, surface: CitationSurface) -> Self {
        self.info.message.context = Some(surface);
        self
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

// One definition owns each wire code, message id and recovery. Audience and
// named-reference wording are attributes of that same catalogue message.
macro_rules! citation_diagnostics {
    ($($variant:ident => $code:literal, $recovery:expr;)*) => {
        /// Fixed diagnostics never contain source data, paths or request text.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        pub enum CitationError {
            $(#[serde(rename = $code)] $variant,)*
            /// A code from a newer engine is never treated as success.
            #[serde(other)]
            #[schemars(skip)]
            Unknown,
        }
        impl CitationError {
            pub const ALL: [Self; [$(stringify!($variant)),*].len()] = [$(Self::$variant,)*];
            pub fn message_id(self) -> &'static str {
                match self {
                    $(Self::$variant => concat!("problem-", $code),)*
                    Self::Unknown => "error-internal",
                }
            }
            pub fn problem(self) -> Problem {
                let (code, recovery) = match self {
                    $(Self::$variant => ($code, $recovery),)*
                    Self::Unknown => ("unknown", Recovery::Retry),
                };
                Problem { code: code.into(), info: RecoveryInfo::new(self.message_id(), recovery) }
            }
        }
    }
}
citation_diagnostics! {
    EmptySelection => "empty-selection", Recovery::SelectReferences;
    TooManyItems => "too-many-items", Recovery::ReduceSelection;
    InvalidLocator => "invalid-locator", Recovery::EditLocator;
    InvalidRichText => "invalid-rich-text", Recovery::EditCitation;
    OutputTooLarge => "output-too-large", Recovery::ReduceSelection;
    UnsupportedForm => "unsupported-form", Recovery::ChooseForm;
    MissingItem => "missing-item", Recovery::SelectReferences;
    ChangedItem => "changed-item", Recovery::SelectReferences;
    MissingKey => "missing-key", Recovery::ChooseStyle;
    MissingDoi => "missing-doi", Recovery::ChooseOutput;
    AmbiguousKey => "ambiguous-key", Recovery::ChooseStyle;
    ConflictingKey => "conflicting-key", Recovery::ChooseStyle;
    InvalidKey => "invalid-key", Recovery::ChooseOutput;
    UnsupportedOutput => "unsupported-output", Recovery::ChooseOutput;
    InvalidLocale => "invalid-locale", Recovery::ChooseLocale;
    Cancelled => "cancelled", Recovery::None { reason: NoRecoveryReason::Cancelled };
    FormatterUnavailable => "formatter-unavailable", Recovery::Retry;
    AnnotationMismatch => "annotation-mismatch", Recovery::SelectReferences;
    NotCitable => "not-citable", Recovery::SelectReferences;
    NotFromZotero => "not-from-zotero", Recovery::ChooseOutput;
}
