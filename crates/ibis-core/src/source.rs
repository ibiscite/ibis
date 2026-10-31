//! Shared file-source errors used by clients. File loading lives in `ibis-sources`.

use thiserror::Error;

/// Allowlisted diagnostics: never expose paths, contents or operating system errors.
/// Front ends add any context about their installed references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SourceError {
    #[error("the library file is unavailable; download any online-only file to this computer first")]
    Unavailable,
    #[error("the library must be a regular file")]
    NotRegular,
    #[error("the library file is larger than 64 MiB")]
    TooLarge,
    #[error("the library file is not UTF-8 text; save it as UTF-8")]
    Unreadable,
    #[error("the library file contains an unfinished entry")]
    Incomplete,
    #[error("the library exceeded the load work limit")]
    LoadLimit,
    #[error("the library file is still changing")]
    Changing,
    #[error("automatic library updates are unavailable; use Reload to try again")]
    WatchUnavailable,
}
