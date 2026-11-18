//! Libraries read by a source adapter: a separate program that speaks the
//! adapter protocol (`ibis_core::contract::adapter`, decision 0056 step 4).
//!
//! The adapter only says what its manager holds. Everything Ibis promises is
//! kept here: the library id is a keyed digest of the adapter's name and its
//! `scope`, so a changed scope retires every earlier handle; each record is
//! checked as records sent to `cite --records` are; `citation-key` is
//! dropped unless the adapter declares keys; answers are bounded in size and
//! time. A failed read changes nothing: the engine keeps the last good one.
//!
//! Built-in adapters are served by the `ibis` executable itself
//! (`ibis __source NAME`), so they ship wherever `ibis` does; others are
//! `ibis-source-NAME` programs on PATH. A built-in name is never looked up
//! on PATH.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::time::Duration;

use ibis_core::contract::adapter::{
    ADAPTER_PROTOCOL, AdapterMethod, AdapterRead, AdapterRefusal, AdapterRequest, AdapterResponse, AdapterSkipCause,
};
use ibis_core::contract::{LibraryObject, Snapshot};
use serde_json::Value;

use crate::{LibraryKind, LoadedLibrary, SourceCapability, SourceWarning, WarningCause, WarningCode};

/// The argument that makes `ibis` serve a built-in adapter: `ibis __source NAME`.
pub const CHILD_ARGUMENT: &str = "__source";
/// The built-in EndNote adapter, served by the CLI.
pub const ENDNOTE: &str = "endnote";
/// How long one read may take. EndNote answers a 10,000-record library in
/// well under a minute.
pub const DEADLINE: Duration = Duration::from_secs(120);
/// The largest answer: the same bound as a library file.
pub const MAX_ANSWER_BYTES: u64 = crate::MAX_LIBRARY_BYTES;
/// The longest `scope` or `revision`.
pub const MAX_TOKEN_CHARS: usize = 256;

const RECORDS: &[SourceCapability] = &[SourceCapability::BibliographicRecords];
const RECORDS_AND_KEYS: &[SourceCapability] = &[
    SourceCapability::BibliographicRecords,
    SourceCapability::NativeCitationKeys,
];

/// Why a source adapter gave no library: fixed reasons, never its output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterError {
    /// No adapter has this name.
    NotFound,
    ManagerClosed,
    LibraryNeeded,
    LibraryNotFound,
    PermissionDenied,
    /// It could not read, it stopped, it took too long, or its answer broke
    /// the protocol.
    Failed,
    Cancelled,
}

/// One adapter, found by name.
#[derive(Clone, PartialEq, Eq)]
pub struct SourceAdapter {
    name: String,
    program: PathBuf,
    arguments: Vec<OsString>,
    deadline: Duration,
}
impl std::fmt::Debug for SourceAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SourceAdapter({})", self.name)
    }
}

/// A read the engine can publish, with the adapter's own change token.
pub struct AdapterLoad {
    pub loaded: LoadedLibrary,
    /// The adapter's `revision`, for `ibis sources check`.
    pub token: String,
}

/// A name an adapter may have: a lowercase letter, then up to 31 lowercase
/// letters, digits or hyphens.
pub fn valid_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && name.len() <= 32
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

impl SourceAdapter {
    /// The adapter called `name`: built in when `ibis` (`built_in`, the
    /// running `ibis` executable) serves it, otherwise the first
    /// `ibis-source-NAME` program in `search`, a PATH value.
    pub fn find(name: &str, built_in: Option<&Path>, search: Option<&OsStr>) -> Result<Self, AdapterError> {
        if !valid_name(name) {
            return Err(AdapterError::NotFound);
        }
        if name == ENDNOTE {
            let program = built_in.ok_or(AdapterError::NotFound)?;
            return Ok(Self {
                name: name.into(),
                program: program.to_path_buf(),
                arguments: vec![CHILD_ARGUMENT.into(), name.into()],
                deadline: DEADLINE,
            });
        }
        let file = format!("ibis-source-{name}{}", std::env::consts::EXE_SUFFIX);
        std::env::split_paths(search.ok_or(AdapterError::NotFound)?)
            // A relative PATH entry would mean a different program in every folder.
            .filter(|directory| directory.is_absolute())
            .map(|directory| directory.join(&file))
            .find(|candidate| runnable(candidate))
            .map(|program| Self::at(name, program))
            .ok_or(AdapterError::NotFound)
    }

    /// The adapter at `program`, for tests and programs that embed Ibis.
    pub fn at(name: &str, program: PathBuf) -> Self {
        Self {
            name: name.into(),
            program,
            arguments: vec![],
            deadline: DEADLINE,
        }
    }

    /// The same adapter with a shorter deadline than [`DEADLINE`], for tests.
    pub fn with_deadline(mut self, deadline: Duration) -> Self {
        self.deadline = deadline.min(DEADLINE);
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Read `library` once, within the deadline.
    pub fn read(&self, library: Option<&str>, cancelled: &dyn Fn() -> bool) -> Result<AdapterLoad, AdapterError> {
        let request = serde_json::to_vec(&AdapterRequest {
            ibis: ADAPTER_PROTOCOL,
            method: AdapterMethod::Read,
            library: library.map(str::to_owned),
        })
        .map_err(|_| AdapterError::Failed)?;
        let arguments: Vec<&OsStr> = self.arguments.iter().map(OsString::as_os_str).collect();
        let answer = crate::child::run(
            &self.program,
            &arguments,
            request,
            self.deadline,
            MAX_ANSWER_BYTES,
            cancelled,
        )
        .map_err(|error| match error {
            crate::child::ChildError::TimedOut if cancelled() => AdapterError::Cancelled,
            crate::child::ChildError::Unavailable => AdapterError::NotFound,
            _ => AdapterError::Failed,
        })?;
        if cancelled() {
            return Err(AdapterError::Cancelled);
        }
        let response: AdapterResponse = serde_json::from_slice(&answer).map_err(|_| AdapterError::Failed)?;
        if response.ibis != ADAPTER_PROTOCOL {
            return Err(AdapterError::Failed);
        }
        match (response.read, response.refused) {
            (Some(read), None) => self.accept(read),
            (None, Some(refusal)) => Err(match refusal {
                AdapterRefusal::ManagerClosed => AdapterError::ManagerClosed,
                AdapterRefusal::LibraryNeeded => AdapterError::LibraryNeeded,
                AdapterRefusal::LibraryNotFound => AdapterError::LibraryNotFound,
                AdapterRefusal::PermissionDenied => AdapterError::PermissionDenied,
                AdapterRefusal::Unreadable | AdapterRefusal::Unsupported => AdapterError::Failed,
            }),
            _ => Err(AdapterError::Failed),
        }
    }

    /// Check a read and make it a library.
    fn accept(&self, read: AdapterRead) -> Result<AdapterLoad, AdapterError> {
        let token = |text: &str| {
            !text.is_empty() && text.chars().count() <= MAX_TOKEN_CHARS && !text.chars().any(char::is_control)
        };
        if !token(&read.scope) || !token(&read.revision) || read.records.len() > crate::MAX_LIBRARY_RECORDS {
            return Err(AdapterError::Failed);
        }
        let library = crate::identity::library_id(LibraryKind::Adapter, &[self.name.as_bytes(), read.scope.as_bytes()]);
        let mut records = read.records;
        if !read.keys {
            for record in &mut records {
                if let Value::Object(fields) = record {
                    fields.remove("citation-key");
                }
            }
        }
        let checked = crate::request_records(&library, &records).map_err(|_| AdapterError::Failed)?;
        let mut warnings = checked.warnings;
        warnings.extend(read.skipped.into_iter().map(|skip| {
            let cause = match skip.cause {
                AdapterSkipCause::UnsupportedType => WarningCause::UnknownType,
                AdapterSkipCause::MissingField => WarningCause::MissingField,
                AdapterSkipCause::Conflicting => WarningCause::Conflicting,
                AdapterSkipCause::Unreadable => WarningCause::Unreadable,
            };
            SourceWarning::at(WarningCode::Syntax, None)
                .for_entry(skip.entry.as_deref())
                .because(cause, skip.field.as_deref())
        }));
        let snapshot = Snapshot::new(checked.records.into_iter().map(LibraryObject::Record).collect())
            .map_err(|_| AdapterError::Failed)?;
        Ok(AdapterLoad {
            loaded: LoadedLibrary {
                library,
                kind: LibraryKind::Adapter,
                capabilities: if read.keys { RECORDS_AND_KEYS } else { RECORDS },
                snapshot,
                warnings,
                cut_short: false,
                duplicate_keys: Default::default(),
                damaged_keys: Default::default(),
            },
            token: read.revision,
        })
    }
}

/// A regular file this account may run.
#[allow(
    clippy::disallowed_methods,
    reason = "A PATH lookup checks metadata only; the adapter's own bounds apply once it runs."
)]
fn runnable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        metadata.is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_plain_and_built_ins_never_come_from_path() {
        for name in ["endnote", "obsidian", "a", "a-1"] {
            assert!(valid_name(name), "{name}");
        }
        for name in ["", "EndNote", "1a", "a/b", "a.b", "-a", &"a".repeat(33)] {
            assert!(!valid_name(name), "{name}");
        }
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let search = std::env::join_paths([directory.path()]).unwrap_or_else(|error| panic!("{error}"));
        let ibis = Path::new("/invented/ibis");
        let built_in = SourceAdapter::find("endnote", Some(ibis), Some(&search)).unwrap_or_else(|_| panic!("built in"));
        assert_eq!(built_in.program, ibis);
        assert_eq!(built_in.arguments, [OsString::from(CHILD_ARGUMENT), "endnote".into()]);
        assert_eq!(
            SourceAdapter::find("endnote", None, Some(&search)),
            Err(AdapterError::NotFound)
        );
        assert_eq!(
            SourceAdapter::find("obsidian", Some(ibis), Some(&search)),
            Err(AdapterError::NotFound)
        );
    }
}
