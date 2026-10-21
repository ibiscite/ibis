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
    Zotero(ZoteroError),
    /// The file dialog returned something that is not a local file.
    NotLocal,
    /// The background load ended without a result.
    Stopped,
    /// The revision counter is used up; a new index could never be told apart.
    RevisionExhausted,
}

impl LoadError {
    /// Whether this failure says the references already loaded are still
    /// searched, whatever library they came from: a file that cannot be read
    /// right now, or Zotero. A file that can never be read (not a regular
    /// file, too large) or a failure of Ibis itself does not say so.
    pub(super) fn keeps_references(self) -> bool {
        matches!(
            self,
            LoadError::Source(
                SourceError::Unavailable
                    | SourceError::Unreadable
                    | SourceError::Incomplete
                    | SourceError::LoadLimit
                    | SourceError::Changing
            ) | LoadError::Zotero(_)
        )
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
            LoadError::Zotero(error) => error.fmt(f),
            LoadError::NotLocal => f.write_str("the chosen file is not on this computer"),
            LoadError::Stopped => f.write_str("loading the library stopped unexpectedly"),
            LoadError::RevisionExhausted => f.write_str("the library could not be updated; restart Ibis"),
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
    /// Anything else, with fixed text for the user.
    Failed { message: String },
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
    fn commands_fail_with_a_stable_code() {
        let json = |error: CommandError| serde_json::to_value(error).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            json(CommandError::SnapshotChanged),
            serde_json::json!({"code": "snapshot_changed"})
        );
        assert_eq!(
            json("the clipboard could not be written".into()),
            serde_json::json!({"code": "failed", "message": "the clipboard could not be written"})
        );
    }

    #[test]
    fn retention_follows_the_kind_of_failure_not_its_wording() {
        for kept in [
            SourceError::Unavailable,
            SourceError::Unreadable,
            SourceError::Incomplete,
            SourceError::LoadLimit,
            SourceError::Changing,
        ] {
            assert!(LoadError::from(kept).keeps_references(), "{kept}");
        }
        for not_kept in [
            SourceError::NotRegular,
            SourceError::TooLarge,
            SourceError::WatchUnavailable,
        ] {
            assert!(!LoadError::from(not_kept).keeps_references(), "{not_kept}");
        }
        assert!(LoadError::from(ZoteroError::GroupNotFound).keeps_references());
        for own in [LoadError::NotLocal, LoadError::Stopped, LoadError::RevisionExhausted] {
            assert!(!own.keeps_references(), "{own}");
        }
    }
}
