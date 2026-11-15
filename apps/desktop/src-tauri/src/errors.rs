//! Fixed diagnostics for native infrastructure; engine failures retain their own types.
use crate::library::CommandError;
use ibis_core::contract::{Problem, Recovery as R, RecoveryInfo};

impl From<&ibis_engine::settings::SettingsError> for CommandError {
    fn from(error: &ibis_engine::settings::SettingsError) -> Self {
        Self::Presented(error.into())
    }
}
impl From<ibis_engine::settings::SettingsError> for CommandError {
    fn from(error: ibis_engine::settings::SettingsError) -> Self {
        Self::from(&error)
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativeOperation {
    OpenSettingsFile,
    CliStatus,
    SetCliInstalled,
    CopyCliProfile,
    HidePicker,
    CopyEditorEndpoint,
    SetWindowSize,
    UndoWindowPosition,
    CopyProjectEntry,
    FinishPasteGuide,
    RowContext,
    FindZotero,
    FindSources,
    OpenFoundLibrary,
    ConfigureShortcuts,
    UseSampleLibrary,
    UseZoteroLibraries,
    DragPasteApp,
}
impl NativeOperation {
    pub(crate) fn failure(self) -> CommandError {
        // Exhaustive even for native operations: never expose an OS error or
        // choose a recovery by parsing its English wording.
        let recovery = match self {
            Self::UseZoteroLibraries | Self::FindSources | Self::OpenFoundLibrary => R::ChooseLibrary,
            Self::FindZotero => R::ChooseLibrary,
            Self::RowContext => R::SearchReferences,
            Self::OpenSettingsFile
            | Self::CliStatus
            | Self::SetCliInstalled
            | Self::CopyCliProfile
            | Self::HidePicker
            | Self::CopyEditorEndpoint
            | Self::SetWindowSize
            | Self::UndoWindowPosition
            | Self::CopyProjectEntry
            | Self::FinishPasteGuide
            | Self::ConfigureShortcuts
            | Self::UseSampleLibrary
            | Self::DragPasteApp => R::Retry,
        };
        let code = serde_json::to_value(self)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default();
        CommandError::Presented(Problem {
            info: RecoveryInfo::new(&format!("app-failure-{code}"), recovery),
            code,
        })
    }
}

impl From<ibis_engine::settings::backup::BackupProblem> for CommandError {
    fn from(error: ibis_engine::settings::backup::BackupProblem) -> Self {
        use ibis_engine::settings::backup::BackupProblem as B;
        let (code, recovery) = match error {
            B::NotDownloaded => ("not-downloaded", R::DownloadFile),
            B::Unreadable => ("unreadable", R::ChooseBackup),
            B::NotABackup => ("not-a-backup", R::ChooseBackup),
            B::SettingsFile => ("settings-file", R::ChooseBackup),
            B::Newer => ("newer", R::UpgradeIbis),
            B::Invalid => ("invalid", R::ChooseBackup),
            B::SettingsUnusable => ("settings-unusable", R::Retry),
            B::Changed => ("changed", R::ChooseBackup),
            B::Busy => ("busy", R::Retry),
            B::NotSaved => ("not-saved", R::Retry),
        };
        Self::Presented(Problem {
            code: code.into(),
            info: RecoveryInfo::new(&format!("backup-problem-{code}"), recovery),
        })
    }
}

impl From<crate::shortcut_setting::ShortcutRefusal> for CommandError {
    fn from(error: crate::shortcut_setting::ShortcutRefusal) -> Self {
        use crate::shortcut_setting::ShortcutRefusal as S;
        let (code, recovery) = match error {
            S::Invalid => ("invalid", R::ChooseShortcut),
            S::NeedsModifier => ("needs-modifier", R::ChooseShortcut),
            S::AppShortcut => ("app-shortcut", R::ChooseShortcut),
            S::PickerShortcut(_) => ("picker-shortcut", R::ChooseShortcut),
            S::AltGr => ("alt-gr", R::ChooseShortcut),
            S::InUse => ("in-use", R::ChooseShortcut),
            S::Unavailable => ("unavailable", R::OpenIbis),
            S::NotSaved => ("not-saved", R::Retry),
        };
        let id = if matches!(error, S::PickerShortcut(_)) {
            "shortcuts-picker-conflict".into()
        } else {
            format!("shortcuts-refusal-{code}")
        };
        let mut info = RecoveryInfo::new(&id, recovery);
        if let S::PickerShortcut(command) = error {
            let id = ibis_engine::catalogue::id("shortcuts-command", command);
            info = info.argument(
                "command",
                ibis_engine::catalogue::text(&id, None, &[]).unwrap_or_default(),
            );
        }
        Self::Presented(Problem {
            code: code.into(),
            info,
        })
    }
}

impl From<crate::destination::AppChoiceError> for CommandError {
    fn from(error: crate::destination::AppChoiceError) -> Self {
        use crate::destination::AppChoiceError as A;
        let (code, id) = match error {
            A::NotSaved { problem } => return Self::Presented(problem),
            A::DeliveryNotSaved => ("delivery_not_saved", "app-command-delivery-not-saved"),
        };
        Self::Presented(Problem {
            code: code.into(),
            info: RecoveryInfo::new(id, R::Retry),
        })
    }
}
