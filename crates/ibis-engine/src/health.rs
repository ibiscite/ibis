//! Source health over the engine's published state; inspection is read-only.
use super::{Engine, LibraryInfo};
use crate::Cancellation;
use ibis_core::{contract::*, settings};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

/// Whether the sources behind one result were healthy, read from the same
/// library state the result came from: a later refresh never changes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SourceHealth {
    pub status: HealthStatus,
    /// One entry per problem, in library order; empty when `status` is `ok`.
    pub findings: Vec<SourceFinding>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum HealthStatus {
    Ok,
    Findings,
    /// A status from a newer engine: it needs review.
    #[serde(other)]
    #[schemars(skip)]
    Unknown,
}
/// A problem with one library a result consulted. Codes only: no source text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "code", rename_all = "kebab-case")]
pub enum SourceFinding {
    /// The library changed and could not be read again; the result used the
    /// references from its last successful read.
    RefreshFailed {
        library: LibraryId,
        error: ErrorCode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<ErrorReason>,
        last_successful_read_ms: u64,
    },
    /// Entries were skipped or read with problems; `ibis libraries` lists them.
    SourceWarnings { library: LibraryId, count: usize },
    /// A finding from a newer engine: it needs review.
    #[serde(other)]
    #[schemars(skip)]
    Unknown,
}
impl SourceHealth {
    /// The findings of `libraries`, the ones a result consulted.
    pub fn of<'a>(libraries: impl IntoIterator<Item = &'a LibraryInfo>) -> Self {
        let mut findings = Vec::new();
        for library in libraries {
            if let Some(error) = library.refresh_error {
                findings.push(SourceFinding::RefreshFailed {
                    library: library.id.clone(),
                    error,
                    reason: library.refresh_reason,
                    last_successful_read_ms: library.updated_at_ms,
                });
            }
            if library.warnings_total > 0 {
                findings.push(SourceFinding::SourceWarnings {
                    library: library.id.clone(),
                    count: library.warnings_total,
                });
            }
        }
        Self {
            status: if findings.is_empty() {
                HealthStatus::Ok
            } else {
                HealthStatus::Findings
            },
            findings,
        }
    }
    /// Clean only when it says so and lists nothing; anything unknown needs review.
    pub fn is_ok(&self) -> bool {
        self.status == HealthStatus::Ok && self.findings.is_empty()
    }
}

/// A result with the health of the sources it consulted.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Checked<T> {
    #[serde(flatten)]
    pub value: T,
    pub source_health: SourceHealth,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InspectionPolicy {
    ReadOnly,
    Setup,
}

/// Stable verdicts, without client wording or a second library state model.
pub fn classify(info: &LibraryInfo) -> (&'static str, &'static str) {
    match (info.refresh_error, info.warnings_total) {
        (Some(_), _) => ("findings", "refresh-failed"),
        (_, 0) => ("ok", "source-ok"),
        _ => ("findings", "source-warnings"),
    }
}

/// Whether a failed read leaves the references already read in use. Opening
/// something that is no library, or is too large, replaces them; reading the
/// same library again keeps them through any failure but a cancelled read.
pub fn retains(error: &EngineError, refresh: bool) -> bool {
    match error.code {
        ErrorCode::Cancelled => false,
        ErrorCode::BadInput | ErrorCode::ResourceLimit => refresh,
        _ => true,
    }
}
impl Engine {
    /// A coherent observation, using the published snapshot and source metadata.
    pub fn health(&self) -> Result<(Arc<Snapshot>, Vec<LibraryInfo>), EngineError> {
        let state = self.state()?;
        Ok((
            state.index.snapshot().clone(),
            state.libraries.values().cloned().collect(),
        ))
    }
}

pub fn counts(snapshot: &Snapshot) -> std::collections::BTreeMap<&LibraryId, usize> {
    let mut counts = std::collections::BTreeMap::new();
    for item in snapshot.objects() {
        if let LibraryObject::Record(record) = &item.object {
            *counts.entry(&record.id.library).or_default() += 1;
        }
    }
    counts
}

pub fn inspect_identity(cancel: &Cancellation) -> Result<bool, EngineError> {
    let result = bounded(cancel, |_| crate::identity::inspect_saved_key());
    cancel.check()?;
    result.ok_or(ErrorCode::SourceUnavailable.into())
}

pub fn inspect_config(path: PathBuf, cancel: &Cancellation) -> Option<settings::Loaded> {
    bounded(cancel, move |_| settings::load(&path))
}

/// No discovery, Zotero, writes, watches, or mutation of a host engine. Fixed
/// diagnostic codes retain metadata distinctions that EngineError cannot carry.
pub fn inspect_file(policy: InspectionPolicy, path: &Path, cancel: &Cancellation) -> Result<LibraryInfo, &'static str> {
    if policy != InspectionPolicy::ReadOnly {
        return Err("bad-input");
    }
    inspect_identity(cancel).map_err(|e| {
        if e.code == ErrorCode::Cancelled {
            "cancelled"
        } else {
            "library-unavailable"
        }
    })?;
    if path.to_str().is_none() {
        return Err("path-not-utf8");
    }
    let owned = path.to_owned();
    match bounded(cancel, move |_| std::fs::metadata(owned)) {
        Some(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => return Err("library-missing"),
        Some(Err(_)) => return Err("library-permission"),
        Some(Ok(metadata)) if !metadata.is_file() => return Err("library-not-regular"),
        None => {
            return Err(if cancel.is_cancelled() {
                "cancelled"
            } else {
                "library-unavailable"
            });
        }
        _ => {}
    }
    let engine = Engine::new().map_err(|_| "internal")?.without_file_watches();
    engine
        .open_file_observed(path, cancel)
        .map_err(|error| match error.code {
            ErrorCode::BadInput => "library-invalid",
            ErrorCode::ResourceLimit => "resource-limit",
            ErrorCode::NewerFormat => "newer-format",
            ErrorCode::Internal => "internal",
            ErrorCode::Cancelled => "cancelled",
            _ => "library-unavailable",
        })?;
    engine
        .libraries()
        .map_err(|_| "internal")?
        .libraries
        .pop()
        .ok_or("internal")
}

/// Native waits share source reads' eight-call pool. Expiry retires the child,
/// so a blocked call cannot start another discovery stage when it returns.
pub(crate) fn bounded<T: Send + 'static>(
    cancel: &Cancellation,
    operation: impl FnOnce(Cancellation) -> T + Send + 'static,
) -> Option<T> {
    cancel.check().ok()?;
    let child = cancel.child();
    let worker = child.clone();
    let result = ibis_sources::inspect_io(move || (!worker.is_cancelled()).then(|| operation(worker))).flatten();
    child.cancel();
    result.filter(|_| !cancel.is_cancelled())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn retention_follows_the_kind_of_failure_not_its_wording() {
        let failed = |code| EngineError::new(code);
        for code in [
            ErrorCode::SourceUnavailable,
            ErrorCode::Internal,
            ErrorCode::NewerFormat,
        ] {
            assert!(retains(&failed(code), false), "{code:?}");
            assert!(retains(&failed(code), true), "{code:?}");
        }
        // Opening something that is no library, or too large, replaces the
        // last references; reading the same library again keeps them.
        for code in [ErrorCode::BadInput, ErrorCode::ResourceLimit] {
            assert!(!retains(&failed(code), false), "{code:?}");
            assert!(retains(&failed(code), true), "{code:?}");
        }
        assert!(!retains(&failed(ErrorCode::Cancelled), true));
    }
    #[test]
    fn a_stalled_filesystem_returns_no_result_and_late_work_is_retired() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("invented.json");
        std::fs::write(&path, b"{invented invalid settings").unwrap_or_else(|e| panic!("{e}"));
        let (release, blocked) = std::sync::mpsc::channel();
        let (finished, done) = std::sync::mpsc::channel();
        let source = path.clone();
        let result = bounded(&Cancellation::default(), move |cancel| {
            let _ = blocked.recv();
            let loaded = settings::load(&source);
            let _ = finished.send(cancel.is_cancelled());
            loaded
        });
        assert!(result.is_none());
        let _ = release.send(());
        assert_eq!(done.recv_timeout(Duration::from_secs(5)), Ok(true));
        assert_eq!(std::fs::read(path).unwrap_or_default(), b"{invented invalid settings");
        assert_eq!(
            std::fs::read_dir(directory.path())
                .unwrap_or_else(|e| panic!("{e}"))
                .count(),
            1
        );
        assert_eq!(bounded(&Cancellation::default(), |_| 7), Some(7));
    }
    #[test]
    fn health_from_a_newer_engine_parses_and_needs_review() {
        let newer: SourceHealth = serde_json::from_value(serde_json::json!({
            "status": "invented-status", "findings": [{"code": "invented-finding", "library": "x"}], "added": 1
        }))
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(newer.status, HealthStatus::Unknown);
        assert_eq!(newer.findings, [SourceFinding::Unknown]);
        assert!(!newer.is_ok());
        let odd: SourceHealth =
            serde_json::from_value(serde_json::json!({"status": "ok", "findings": [{"code": "invented-finding"}]}))
                .unwrap_or_else(|e| panic!("{e}"));
        assert!(!odd.is_ok());
    }
}
