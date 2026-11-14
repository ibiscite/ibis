//! Read-only library sources producing the shared engine snapshot contract.
mod attachments;
mod bibtex;
mod csl_json;
mod exported;
mod file;
pub mod identity;
pub mod project;
mod urls;
pub mod zotero;

use std::collections::BTreeMap;
use std::path::PathBuf;

use ibis_core::contract::{ItemRef, LibraryId, Record, Snapshot};
use ibis_core::source::IdentityClaims;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use csl_json::{RequestRecords, request_records};
pub use file::{
    MAX_LIBRARY_BYTES, SourceError, SourceSnapshot, SourceWatch, inspect_io, library_file_extensions, load_file,
    load_file_snapshot, names_a_library, original_entry, reload_file, reload_file_reusing,
    reload_file_snapshot_reusing,
};

// Real bibliographies reach 100,000 entries (#1251). Memory follows file size
// more than record count: a release build loaded 200,000 small records in
// about 0.85 GiB, and a 58 MiB file of 200,000 records in about 2.6 GiB. The
// 64 MiB file cap is the real bound.
pub const MAX_LIBRARY_RECORDS: usize = 200_000;

/// Days in a month of the proleptic Gregorian calendar; months outside 1–12 have 31.
pub(crate) fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Safe source diagnostics contain a location and fixed code, never payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum WarningCode {
    /// Validated newer Zotero layout; a source status, not a record problem.
    ZoteroNewerThanTested,
    CloudSyncedDirectory,
    /// Better BibTeX keeps keys this read could not get, so items without a
    /// key of their own have none rather than a guessed one.
    CitationKeysUnavailable,
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
    /// The repeated citation key or CSL `id`, for `duplicate-key` only. It is the person's
    /// own data and is shown only in their library diagnostics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// The entry's citation key, or its title when no key is available. Like
    /// `key`, this is source text for the person's diagnostics, never MCP.
    /// Display labels are bounded to 256 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    /// Why the record was skipped or needs checking, as a fixed code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<WarningCause>,
    /// The field to fix, named from the source format's vocabulary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

impl SourceWarning {
    pub fn at(code: WarningCode, line: Option<usize>) -> Self {
        Self {
            code,
            line,
            key: None,
            entry: None,
            cause: None,
            field: None,
        }
    }

    pub fn for_entry(mut self, label: Option<&str>) -> Self {
        self.entry = label
            .filter(|label| !label.trim().is_empty())
            .map(|label| match label.char_indices().nth(256) {
                Some((end, _)) => format!("{}…", &label[..end]),
                None => label.into(),
            });
        self
    }

    /// Supply a field only after recognizing it in the format's vocabulary.
    /// Omit names the format does not define, even if they have valid syntax.
    pub fn because(mut self, cause: WarningCause, field: Option<&str>) -> Self {
        self.cause = Some(cause);
        self.field = field
            .map(str::trim)
            .filter(|name| {
                !name.is_empty()
                    && name.len() <= 64
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            })
            .map(str::to_ascii_lowercase);
        self
    }
}

/// Why a record was skipped (the first group) or a field needs checking (the
/// second). Each has one sentence in `locales/en-US.ftl` (`cause-…`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum WarningCause {
    /// The entry or record never ends.
    NotClosed,
    /// A BibTeX entry has no citation key.
    NoKey,
    /// A field has no `=` or no value.
    MissingValue,
    /// A field's braces or quotes do not pair up.
    Unbalanced,
    /// Text the format does not allow, where an entry's parts belong.
    UnexpectedText,
    /// A type Ibis does not know.
    UnknownType,
    /// Repeated values or values that disagree.
    Conflicting,
    /// A CSV row with more or fewer cells than its header.
    WrongCells,
    /// A required field is empty.
    MissingField,
    /// RIS fields before any `TY` line.
    NoStart,
    /// An `@string` definition could not be read.
    StringDefinition,
    /// The entry could not be read at all.
    Unreadable,
    /// A field uses an abbreviation (`@string`) that is not defined.
    Abbreviation,
    /// A date could not be read in full.
    Date,
    /// A list of names could not be read.
    Names,
    /// A role CSL has no word for.
    Role,
    /// A link that is private or not a web address.
    Link,
    /// A cross-reference or inherited entry that is missing or loops.
    Inheritance,
    /// A chain of linked entries reached the resolver's work limit.
    InheritanceLimit,
    /// A field too large to read.
    TooLarge,
    /// An identifier (DOI, ISBN, ISSN) that is not valid.
    Identifier,
    /// A language Ibis does not recognise.
    Language,
    /// A TeX command Ibis does not understand, kept as written.
    TexCommand,
    /// A field the format does not have.
    UnknownField,
    /// A value that could not be read as its field expects.
    Value,
}

/// What a library is. Front ends read this, never the library's id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum LibraryKind {
    /// A BibTeX, BibLaTeX, CSL-JSON, RIS or CSV file.
    File,
    Zotero,
    /// Data handed to the engine whole, with no source behind it.
    Snapshot,
}

/// A fixed list per kind, not a plugin system. Zotero's two read paths share
/// one kind, so running and closed Zotero cannot offer different lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SourceCapability {
    BibliographicRecords,
    NativeCitationKeys,
    ExtraCitationKeys,
    KeyAliases,
    KeyConflicts,
}

impl LibraryKind {
    pub fn capabilities(self) -> &'static [SourceCapability] {
        use SourceCapability::*;
        match self {
            Self::File => &[BibliographicRecords, NativeCitationKeys],
            Self::Zotero => &[
                BibliographicRecords,
                NativeCitationKeys,
                ExtraCitationKeys,
                KeyAliases,
                KeyConflicts,
            ],
            Self::Snapshot => &[BibliographicRecords],
        }
    }
}

/// One immutable library plus private source ownership. Paths never serialize.
#[derive(Clone)]
pub struct LoadedLibrary {
    pub library: LibraryId,
    pub kind: LibraryKind,
    pub snapshot: Snapshot,
    pub warnings: Vec<SourceWarning>,
    /// The read stopped at a limit and left later records unread, so a key
    /// that is not found may be among them. A limit that only skipped one
    /// record or its attachments does not count.
    pub cut_short: bool,
    /// Each skipped entry whose header names a citation key, with its warning,
    /// so a lookup can say why the key selects nothing. Warnings themselves
    /// carry no source text beyond duplicate keys; this never serializes.
    pub damaged_keys: BTreeMap<String, SourceWarning>,
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
    reused: Vec<usize>,
    warnings: Vec<SourceWarning>,
    attachments: Vec<(ItemRef, String, Option<usize>)>,
    /// Records after a limit were left unread: `LoadedLibrary::cut_short`.
    cut_short: bool,
    /// `LoadedLibrary::damaged_keys`.
    damaged_keys: BTreeMap<String, SourceWarning>,
}

impl ParsedLibrary {
    fn quarantine(&mut self, claims: &IdentityClaims<String, Option<usize>>, previous: Option<&Snapshot>) {
        self.records.retain(|record| claims.is_unique(&record.id.key));
        self.reused.retain(|&position| {
            previous
                .and_then(|snapshot| snapshot.objects().get(position))
                .is_some_and(|item| claims.is_unique(&item.object.id().key))
        });
        self.attachments.retain(|(item, ..)| claims.is_unique(&item.key));
        let mut warnings: Vec<_> = claims
            .duplicates()
            .map(|(key, line)| SourceWarning {
                key: Some(key.strip_prefix("record:").unwrap_or(key).to_owned()),
                ..SourceWarning::at(WarningCode::DuplicateKey, *line)
            })
            .collect();
        warnings.sort_by(|a, b| (&a.line, &a.key).cmp(&(&b.line, &b.key)));
        self.warnings.extend(warnings);
    }

    /// Warn that an entry was skipped, remembering the key its header names.
    fn skip(&mut self, key: Option<&str>, warning: SourceWarning) {
        // A header without its comma can run on; real keys are short.
        if let Some(key) = key.filter(|key| key.len() <= 1024) {
            self.damaged_keys
                .entry(key.to_owned())
                .or_insert_with(|| warning.clone());
        }
        self.warnings.push(warning);
    }
    fn push_record(&mut self, record: Record, previous: Option<&Snapshot>) {
        if let Some(position) = previous.and_then(|snapshot| snapshot.matching_record(&record)) {
            self.reused.push(position);
        } else {
            self.records.push(record);
        }
    }
}

pub mod zotero_source;
