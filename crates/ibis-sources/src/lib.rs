//! Read-only library sources producing the shared engine snapshot contract.
mod attachments;
mod bibtex;
mod csl_json;
mod file;
mod urls;
pub mod zotero;

use std::collections::BTreeMap;
use std::path::PathBuf;

use ibis_core::contract::{ItemRef, LibraryId, Record, Snapshot};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use file::{
    MAX_LIBRARY_BYTES, SourceError, SourceSnapshot, SourceWatch, load_file, load_file_snapshot, reload_file,
};

// Real bibliographies reach 100,000 entries (#1251). Memory follows file size
// more than record count: a release build loaded 200,000 small records in
// about 0.85 GiB, and a 58 MiB file of 200,000 records in about 2.6 GiB. The
// 64 MiB file cap is the real bound.
pub const MAX_LIBRARY_RECORDS: usize = 200_000;

/// Safe source diagnostics contain a location and fixed code, never payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum WarningCode {
    Syntax,
    DuplicateKey,
    Field,
    WorkLimit,
    InvalidAttachment,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceWarning {
    pub code: WarningCode,
    pub line: Option<usize>,
}

/// One immutable library plus private source ownership. Paths never serialize.
#[derive(Clone)]
pub struct LoadedLibrary {
    pub library: LibraryId,
    pub snapshot: Snapshot,
    pub warnings: Vec<SourceWarning>,
    attachment_paths: BTreeMap<ItemRef, PathBuf>,
}
impl std::fmt::Debug for LoadedLibrary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedLibrary")
            .field("library", &self.library)
            .field("objects", &self.snapshot.objects().len())
            .field("warnings", &self.warnings)
            .finish_non_exhaustive()
    }
}
impl LoadedLibrary {
    /// Number of privately resolved attachment locations, without exporting them.
    pub fn local_attachment_count(&self) -> usize {
        self.attachment_paths.len()
    }
}

#[derive(Default)]
struct ParsedLibrary {
    records: Vec<Record>,
    warnings: Vec<SourceWarning>,
    attachments: Vec<(ItemRef, String, Option<usize>)>,
}
