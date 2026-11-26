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

/// A native step failed. It carries no words: the command that ran it fails
/// as its own [`NativeOperation`], whose code, message and recovery are fixed
/// here. `#[command]` makes that conversion, named after the command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeFailure;

#[derive(Debug, Clone, Copy, serde::Serialize, schemars::JsonSchema)]
#[cfg_attr(test, derive(serde::Deserialize))]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativeOperation {
    OpenSettingsFile,
    AgentApps,
    ConnectAgentApp,
    DisconnectAgentApp,
    WaitingProposals,
    ApplyProposal,
    RejectProposal,
    RejectAppProposals,
    CliStatus,
    SetCliInstalled,
    CopyCliProfile,
    HidePicker,
    CopyEditorEndpoint,
    CopyVersion,
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
            | Self::AgentApps
            | Self::ConnectAgentApp
            | Self::DisconnectAgentApp
            | Self::WaitingProposals
            | Self::ApplyProposal
            | Self::RejectProposal
            | Self::RejectAppProposals
            | Self::CliStatus
            | Self::SetCliInstalled
            | Self::CopyCliProfile
            | Self::HidePicker
            | Self::CopyEditorEndpoint
            | Self::CopyVersion
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
            A::DeliveryNotSaved => ("delivery-not-saved", "app-command-delivery-not-saved"),
        };
        Self::Presented(Problem {
            code: code.into(),
            info: RecoveryInfo::new(id, R::Retry),
        })
    }
}

/// Every value of a unit enum, read from its own schema.
#[cfg(test)]
fn every<T: schemars::JsonSchema + serde::de::DeserializeOwned>() -> Vec<T> {
    let schema = schemars::schema_for!(T).to_value();
    let names: Vec<serde_json::Value> = match schema.get("enum") {
        Some(names) => names.as_array().cloned().unwrap_or_default(),
        None => schema["oneOf"]
            .as_array()
            .unwrap_or_else(|| panic!("{} is not a unit enum", T::schema_name()))
            .iter()
            .flat_map(|variant| match variant.get("enum") {
                Some(names) => names.as_array().cloned().unwrap_or_default(),
                None => vec![variant["const"].clone()],
            })
            .collect(),
    };
    names
        .into_iter()
        .map(|name| serde_json::from_value(name).unwrap_or_else(|e| panic!("{}: {e}", T::schema_name())))
        .collect()
}

/// One problem for every code the desktop can send. The bindings close
/// `Problem.code` over exactly these codes, so the interface switches on a
/// closed set, and each one must have words in the catalogue.
///
/// Families with an `ALL` or a schema are read whole. A family listed here by
/// hand is followed by an exhaustive match: a new variant does not compile
/// until it is listed too.
#[cfg(test)]
pub(crate) fn every_problem() -> Vec<Problem> {
    use crate::{destination::AppChoiceError, shortcut_setting::ShortcutRefusal};
    use ibis_core::contract::{CitationError, EngineError, ErrorCode, ErrorReason};
    use ibis_engine::{
        app_context::Error as Context,
        citations::SessionError as Session,
        library::LoadError as Load,
        settings::{SettingsError as Settings, backup::BackupProblem as Backup, writings::WritingError as Writing},
    };
    let mut problems: Vec<Problem> = ErrorCode::ALL.map(|code| EngineError::new(code).into()).to_vec();
    problems.extend(
        every::<ErrorReason>()
            .into_iter()
            .map(|reason| EngineError::because(ErrorCode::BadInput, reason).into()),
    );
    problems.extend(CitationError::ALL.map(CitationError::problem));
    problems.extend(
        every::<NativeOperation>()
            .into_iter()
            .map(|operation| operation.failure().problem()),
    );

    // Proposal refusals reach Settings as problems too, with and without a field.
    for refusal in every::<ibis_engine::write::WriteProblem>() {
        problems.push(refusal.problem());
        problems.push(
            ibis_engine::write::WriteError {
                problem: refusal,
                field: Some(String::new()),
            }
            .problem(),
        );
    }

    let session = [
        Session::SharedCitationKey { key: String::new() },
        Session::SnapshotChanged,
        Session::Cancelled,
        Session::CitationTooLarge,
        Session::NotPrepared,
    ];
    session.iter().for_each(|error| match error {
        Session::SharedCitationKey { .. }
        | Session::SnapshotChanged
        | Session::Cancelled
        | Session::CitationTooLarge
        | Session::NotPrepared
        | Session::Engine(_)
        | Session::Presented(_) => {}
    });
    problems.extend(session.iter().map(Session::problem));

    let settings = || {
        [
            Settings::Changed,
            Settings::Busy,
            Settings::Unreadable(std::io::ErrorKind::PermissionDenied.into()),
            Settings::Unreadable(std::io::ErrorKind::NotFound.into()),
            Settings::Invalid(String::new()),
            Settings::InvalidZoteroLibraryEntry { position: 1 },
            Settings::NewerVersion(0),
        ]
    };
    settings().iter().for_each(|error| match error {
        Settings::Changed
        | Settings::Busy
        | Settings::Unreadable(_)
        | Settings::Invalid(_)
        | Settings::InvalidZoteroLibraryEntry { .. }
        | Settings::NewerVersion(_)
        | Settings::Backup(_) => {}
    });
    problems.extend(settings().iter().map(Problem::from));

    let writings = [
        Writing::InvalidName,
        Writing::DuplicateName,
        Writing::NotFound,
        Writing::TooMany,
        Writing::Invalid,
        Writing::ContextUnavailable,
        Writing::InvalidContext,
        Writing::AmbiguousContext,
    ];
    writings.iter().for_each(|error| match error {
        Writing::InvalidName
        | Writing::DuplicateName
        | Writing::NotFound
        | Writing::TooMany
        | Writing::Invalid
        | Writing::ContextUnavailable
        | Writing::InvalidContext
        | Writing::AmbiguousContext
        | Writing::Settings(_) => {}
    });
    problems.extend(writings.iter().map(Problem::from));

    let context = [
        Context::Stale,
        Context::Destination,
        Context::Editor,
        Context::Unsupported,
        Context::SnapshotChanged,
        Context::Cancelled,
    ];
    context.iter().for_each(|error| match error {
        Context::Stale
        | Context::Destination
        | Context::Editor
        | Context::Unsupported
        | Context::SnapshotChanged
        | Context::Cancelled
        | Context::Project(_)
        | Context::Session(_)
        | Context::Presented(_) => {}
    });
    problems.extend(context.into_iter().map(Problem::from));

    let load = [Load::NotLocal, Load::Stopped];
    load.iter().for_each(|error| match error {
        Load::NotLocal | Load::Stopped | Load::File(_) | Load::FileRefresh(_) | Load::Zotero(_) => {}
    });
    problems.extend(load.into_iter().map(Load::problem));

    let backup = [
        Backup::NotDownloaded,
        Backup::Unreadable,
        Backup::NotABackup,
        Backup::SettingsFile,
        Backup::Newer,
        Backup::Invalid,
        Backup::SettingsUnusable,
        Backup::Busy,
        Backup::Changed,
        Backup::NotSaved,
    ];
    backup.iter().for_each(|error| match error {
        Backup::NotDownloaded
        | Backup::Unreadable
        | Backup::NotABackup
        | Backup::SettingsFile
        | Backup::Newer
        | Backup::Invalid
        | Backup::SettingsUnusable
        | Backup::Busy
        | Backup::Changed
        | Backup::NotSaved => {}
    });
    problems.extend(backup.into_iter().map(|error| CommandError::from(error).problem()));

    let mut shortcut = vec![
        ShortcutRefusal::Invalid,
        ShortcutRefusal::NeedsModifier,
        ShortcutRefusal::AppShortcut,
        ShortcutRefusal::AltGr,
        ShortcutRefusal::InUse,
        ShortcutRefusal::Unavailable,
        ShortcutRefusal::NotSaved,
    ];
    shortcut.extend(
        every::<crate::shortcut_setting::PickerCommand>()
            .into_iter()
            .map(ShortcutRefusal::PickerShortcut),
    );
    shortcut.iter().for_each(|error| match error {
        ShortcutRefusal::Invalid
        | ShortcutRefusal::NeedsModifier
        | ShortcutRefusal::AppShortcut
        | ShortcutRefusal::PickerShortcut(_)
        | ShortcutRefusal::AltGr
        | ShortcutRefusal::InUse
        | ShortcutRefusal::Unavailable
        | ShortcutRefusal::NotSaved => {}
    });
    problems.extend(shortcut.into_iter().map(|error| CommandError::from(error).problem()));

    let choice = [AppChoiceError::DeliveryNotSaved];
    choice.iter().for_each(|error| match error {
        AppChoiceError::DeliveryNotSaved | AppChoiceError::NotSaved { .. } => {}
    });
    problems.extend(choice.into_iter().map(|error| CommandError::from(error).problem()));

    let command = [
        CommandError::SnapshotChanged,
        CommandError::LibraryLoading,
        CommandError::Cancelled,
        CommandError::NotPrepared,
        CommandError::BibliographyUnavailable,
        CommandError::BibliographyNotSaved,
        CommandError::BibliographyNameTaken,
        CommandError::BibliographyFormat,
        CommandError::ClipboardUnavailable,
        CommandError::SettingsUnavailable,
        CommandError::RecentHistoryNotSaved,
        CommandError::ProjectConflict { keys: Vec::new() },
        CommandError::SourceNotOpened,
        CommandError::SearchFailed,
        CommandError::RecentReferencesFailed,
        CommandError::NoLibrary,
        CommandError::DocumentFormat,
        CommandError::KeyFormat,
        CommandError::OutputNotSaved,
    ];
    command.iter().for_each(|error| match error {
        CommandError::SnapshotChanged
        | CommandError::LibraryLoading
        | CommandError::Cancelled
        | CommandError::NotPrepared
        | CommandError::BibliographyUnavailable
        | CommandError::BibliographyNotSaved
        | CommandError::BibliographyNameTaken
        | CommandError::BibliographyFormat
        | CommandError::ClipboardUnavailable
        | CommandError::SettingsUnavailable
        | CommandError::RecentHistoryNotSaved
        | CommandError::ProjectConflict { .. }
        | CommandError::SourceNotOpened
        | CommandError::SearchFailed
        | CommandError::RecentReferencesFailed
        | CommandError::NoLibrary
        | CommandError::DocumentFormat
        | CommandError::KeyFormat
        | CommandError::OutputNotSaved
        | CommandError::Engine(_)
        | CommandError::Refused(_)
        | CommandError::Presented(_) => {}
    });
    problems.extend(command.iter().map(CommandError::problem));
    problems
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_problem_has_a_code_words_and_one_recovery() {
        for problem in super::every_problem() {
            assert!(!problem.code.is_empty(), "{problem:?} has no code");
            // One case for every code, as the engine's own serde names are.
            assert!(
                problem
                    .code
                    .split('-')
                    .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())),
                "{} is not kebab-case",
                problem.code
            );
            let id = &problem.info.message.id;
            assert!(
                ibis_engine::catalogue::text(id, None, &[]).is_some_and(|text| !text.is_empty()),
                "{} uses {id}, which the catalogue lacks",
                problem.code
            );
            let wire = serde_json::to_value(&problem).unwrap_or_default();
            assert!(
                wire["recovery"]["action"].is_string(),
                "{} has no recovery",
                problem.code
            );
        }
    }
}
