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
//! (`ibis __source NAME`), so they ship wherever `ibis` does. External
//! adapters run from the absolute paths approved in the person’s configuration.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ibis_core::contract::adapter::{ADAPTER_PROTOCOL, AdapterMethod, AdapterRefusal, AdapterRequest, AdapterSkipCause};
use ibis_core::contract::{LibraryObject, Snapshot};

use crate::{LibraryKind, LoadedLibrary, SourceCapability, SourceWarning, WarningCause, WarningCode};

mod response;

#[cfg(unix)]
pub use crate::child::{Termination, exit_on_termination};

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
    /// Resolve a built-in name or the executable explicitly configured for this name.
    pub fn find(name: &str, built_in: Option<&Path>, configured: Option<&Path>) -> Result<Self, AdapterError> {
        if name == ENDNOTE {
            let program = built_in
                .filter(|path| path.is_absolute())
                .ok_or(AdapterError::NotFound)?;
            return Ok(Self {
                name: name.into(),
                program: program.to_path_buf(),
                arguments: vec![CHILD_ARGUMENT.into(), name.into()],
                deadline: DEADLINE,
            });
        }
        Self::at(name, configured.ok_or(AdapterError::NotFound)?.to_path_buf())
    }

    /// Approve one external adapter, also for programs that embed Ibis.
    pub fn at(name: &str, program: PathBuf) -> Result<Self, AdapterError> {
        if !valid_name(name) || name == ENDNOTE || !program.is_absolute() {
            return Err(AdapterError::NotFound);
        }
        // An explicit .exe selects one file and excludes the implicit command interpreter.
        #[cfg(windows)]
        if !program
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        {
            return Err(AdapterError::NotFound);
        }
        Ok(Self {
            name: name.into(),
            program,
            arguments: vec![],
            deadline: DEADLINE,
        })
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
        let mut command = std::process::Command::new(&self.program);
        command.args(&self.arguments);
        let answer = crate::child::run(
            command,
            &request,
            crate::child::Limits {
                time: self.deadline,
                stdout: MAX_ANSWER_BYTES,
                stderr: None,
            },
            cancelled,
        )
        .map_err(|error| match error {
            crate::child::ChildError::TimedOut if cancelled() => AdapterError::Cancelled,
            crate::child::ChildError::Unavailable => AdapterError::NotFound,
            _ => AdapterError::Failed,
        })?;
        if !answer.status.success() {
            return Err(AdapterError::Failed);
        }
        if cancelled() {
            return Err(AdapterError::Cancelled);
        }
        self.answer(&answer.stdout)
    }

    /// Check what the program wrote and make it a library: the second half
    /// of [`Self::read`], after the program exited successfully. Any bytes
    /// give a library or a fixed error; the parsers are fuzzed through here.
    pub fn answer(&self, stdout: &[u8]) -> Result<AdapterLoad, AdapterError> {
        let response = response::decode(stdout).map_err(|_| AdapterError::Failed)?;
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
    fn accept(&self, read: response::Read<'_>) -> Result<AdapterLoad, AdapterError> {
        let library = crate::identity::library_id(LibraryKind::Adapter, &[self.name.as_bytes(), read.scope.as_bytes()]);
        let checked = crate::csl_json::request_raw_records(&library, &read.records, read.keys)
            .map_err(|_| AdapterError::Failed)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_plain_and_built_ins_take_precedence() {
        for name in ["endnote", "obsidian", "a", "a-1"] {
            assert!(valid_name(name), "{name}");
        }
        for name in ["", "EndNote", "1a", "a/b", "a.b", "-a", &"a".repeat(33)] {
            assert!(!valid_name(name), "{name}");
        }
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let ibis = directory.path().join("ibis");
        let external = directory.path().join("adapter.exe");
        let built_in =
            SourceAdapter::find(ENDNOTE, Some(&ibis), Some(&external)).unwrap_or_else(|_| panic!("built in"));
        assert_eq!(built_in.program, ibis);
        assert_eq!(built_in.arguments, [OsString::from(CHILD_ARGUMENT), ENDNOTE.into()]);
        assert_eq!(
            SourceAdapter::find(ENDNOTE, None, Some(&external)),
            Err(AdapterError::NotFound)
        );
        assert_eq!(
            SourceAdapter::find("obsidian", Some(&ibis), None),
            Err(AdapterError::NotFound)
        );
        assert_eq!(
            SourceAdapter::find("obsidian", None, Some(Path::new("adapter.exe"))),
            Err(AdapterError::NotFound),
        );
    }
}
