//! The document pipeline (decision 0060): scan → reference set → render →
//! sink. A manuscript is read once, its citations become the set of records
//! it cites, and that set is checked or written out.
//!
//! - **Check** names every citation that cannot be built: damaged markers,
//!   keys no library holds or holds twice, and, against a reference set,
//!   keys it lacks and records that changed since it was written.
//! - **Export** writes the cited records in a record format, through the same
//!   share-safe exports as `cite`. Its CSL-JSON is a reference set.
//!
//! Only the document child parses a file; this module reads its bytes under
//! the local-file rules and checks what the child answers. A text read only
//! in part says so, and never passes.

use super::{Cancellation, Engine, health, select};
use crate::request::{ItemSelector, KeyCitationRequest, KeyItem, RecordsRequest};
use ibis_core::contract::{
    CitationError, CitationForm, CitationResult, EngineError, ErrorCode, ErrorReason, KeySyntax, OutputSpec,
};
use ibis_core::contract::{
    Cite, LibraryObject, MAX_TEXT_BYTES, Manuscript, Place, ScanProblem, SnapshotObject, TextKind,
};
use ibis_sources::document::Refusal;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The largest reference set read, in bytes.
pub const MAX_SET_BYTES: u64 = 16 * 1024 * 1024;

const CSL_JSON: OutputSpec = OutputSpec::Key {
    syntax: KeySyntax::CslJson,
    command: None,
};

/// Check a manuscript's citations against the open libraries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CheckRequest {
    /// The manuscript: Markdown, Quarto, R Markdown, LaTeX or Typst.
    pub document: PathBuf,
    /// A reference set (a CSL-JSON file `export` wrote) the cited records
    /// must still match: a lock file for co-authors and builds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set: Option<PathBuf>,
}

/// Export the records a manuscript cites.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportRequest {
    /// The manuscript: Markdown, Quarto, R Markdown, LaTeX or Typst.
    pub document: PathBuf,
    /// A record export from `outputs`: `csl-json` writes a reference set.
    pub output: OutputSpec,
}

/// What a check found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum CheckResult {
    Checked(Checked),
    /// The manuscript or the set could not be read; nothing was checked.
    Refused {
        file: Which,
        reason: Refusal,
    },
}

/// What an export wrote.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum ExportResult {
    Exported {
        #[serde(flatten)]
        read: Read,
        /// The records, with a refusal for each the format cannot write.
        /// `None` when the manuscript cites nothing that could be found.
        records: Option<Box<CitationResult>>,
    },
    Refused {
        file: Which,
        reason: Refusal,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Which {
    Document,
    Set,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Checked {
    #[serde(flatten)]
    pub read: Read,
}

/// What reading a manuscript found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Read {
    pub kind: TextKind,
    /// Citations read.
    pub citations: usize,
    /// Different keys cited.
    pub keys: usize,
    /// Every problem, in document order, then those against the set.
    pub findings: Vec<Finding>,
    /// The whole manuscript was read. A text read only in part never passes.
    pub complete: bool,
    /// `\nocite{*}`: every entry of the bibliography counts as cited.
    pub cites_everything: bool,
}

impl Read {
    /// Nothing stops the manuscript building from these libraries.
    pub fn settled(&self) -> bool {
        self.complete && self.findings.is_empty()
    }
}

impl CheckResult {
    pub fn settled(&self) -> bool {
        matches!(self, Self::Checked(checked) if checked.read.settled())
    }
}

/// One problem, where it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Finding {
    /// Where in the manuscript; none for a problem of the set as a whole.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place: Option<Place>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// For a renamed key, the key the library uses now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    pub problem: Problem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Problem {
    /// A citation opened and never closed; the rest of its paragraph is unread.
    Unclosed,
    /// A citation part with no key.
    WithoutKey,
    /// Two keys with no `;` between them.
    TwoKeys,
    /// A braced locator that is not one.
    Locator,
    /// Reading stopped here: the text is too large or too tangled to read whole.
    TooLong,
    /// No open library has this key.
    Missing,
    /// Several references answer to it.
    Ambiguous,
    /// Its library holds the key twice and Ibis reads neither.
    Duplicate,
    /// Its library entry could not be read.
    Damaged,
    /// A library was too large to read whole, so the key may be there.
    CutShort,
    /// Not a key Ibis can look up.
    Invalid,
    /// The reference set has no record for this key.
    NotInSet,
    /// The record changed since the reference set was written.
    Changed,
    /// The record could not be written out to compare it with the set.
    Uncompared,
    /// The key is an old name: the library now calls the reference `current`.
    Renamed,
    /// `\nocite{*}`: the export holds what is cited by key, not the whole bibliography.
    CitesEverything,
}

impl From<ScanProblem> for Problem {
    fn from(kind: ScanProblem) -> Self {
        match kind {
            ScanProblem::Unclosed => Self::Unclosed,
            ScanProblem::WithoutKey => Self::WithoutKey,
            ScanProblem::TwoKeys => Self::TwoKeys,
            ScanProblem::Locator => Self::Locator,
            ScanProblem::TooLong => Self::TooLong,
        }
    }
}

fn key_problem(failure: &EngineError) -> Problem {
    match (failure.code, failure.reason) {
        (_, Some(ErrorReason::AmbiguousKey)) => Problem::Ambiguous,
        (_, Some(ErrorReason::DuplicateKey)) => Problem::Duplicate,
        (_, Some(ErrorReason::DamagedEntry)) => Problem::Damaged,
        (_, Some(ErrorReason::LibraryCutShort)) => Problem::CutShort,
        (ErrorCode::NotFound, _) => Problem::Missing,
        _ => Problem::Invalid,
    }
}

/// A manuscript's citations, and which of its keys the libraries hold.
struct Scanned {
    kind: TextKind,
    manuscript: Manuscript,
    /// Each different key, in first-citation order, with the key its record
    /// goes by now, or why the libraries cannot give it.
    keys: Vec<(String, Result<String, Problem>)>,
    findings: Vec<Finding>,
}

impl Scanned {
    fn read(&self) -> Read {
        Read {
            kind: self.kind,
            citations: self.manuscript.citations.len(),
            keys: self.keys.len(),
            findings: self.findings.clone(),
            complete: self.manuscript.complete(),
            cites_everything: self.manuscript.cites_everything,
        }
    }
    /// The records found, by the key each goes by now, once each.
    fn found(&self) -> Vec<&str> {
        let mut seen = HashSet::new();
        self.keys
            .iter()
            .filter_map(|(_, found)| found.as_deref().ok())
            .filter(|current| seen.insert(*current))
            .collect()
    }
}

fn stop(cancel: &Cancellation) -> Arc<dyn Fn() -> bool + Send + Sync> {
    let cancel = cancel.clone();
    Arc::new(move || cancel.is_cancelled())
}

impl Engine {
    /// Read `document` and look up every key it cites.
    fn scan_manuscript(&self, document: &Path, cancel: &Cancellation) -> Result<Result<Scanned, Refusal>, EngineError> {
        let Some(kind) = TextKind::of(document) else {
            return Ok(Err(Refusal::Unsupported));
        };
        let Some(reader) = &self.documents else {
            return Ok(Err(Refusal::Unavailable));
        };
        let read = ibis_sources::document::read_text(document, MAX_TEXT_BYTES as u64, stop(cancel))
            .and_then(|text| reader.scan_text(kind, &text, &*stop(cancel)));
        cancel.check()?;
        let manuscript = match read {
            Ok(manuscript) => manuscript,
            Err(refusal) => return Ok(Err(refusal)),
        };
        let state = self.state()?;
        let snapshot = state.index.snapshot();
        let mut findings: Vec<Finding> = manuscript
            .problems
            .iter()
            .map(|problem| Finding {
                place: Some(problem.place),
                key: None,
                current: None,
                problem: problem.kind.into(),
            })
            .collect();
        let mut keys: Vec<(String, Result<String, Problem>)> = Vec::new();
        let mut known: BTreeMap<&str, Result<String, Problem>> = BTreeMap::new();
        for cited in &manuscript.citations {
            for cite in &cited.cites {
                let found = match known.get(cite.key.as_str()) {
                    Some(found) => found.clone(),
                    None => {
                        cancel.check()?;
                        let found = match select(snapshot, &state.libraries, &ItemSelector::Key(cite.key.clone())) {
                            Ok(SnapshotObject {
                                object: LibraryObject::Record(record),
                                ..
                            }) => match record.citation_key() {
                                Ok(Some(current)) => Ok(current.to_owned()),
                                _ => Err(Problem::Invalid),
                            },
                            Ok(_) => Err(Problem::Invalid),
                            Err(failure) => Err(key_problem(&failure)),
                        };
                        known.insert(&cite.key, found.clone());
                        keys.push((cite.key.clone(), found.clone()));
                        found
                    }
                };
                let (problem, current) = match found {
                    Ok(current) if current == cite.key => continue,
                    Ok(current) => (Problem::Renamed, Some(current)),
                    Err(problem) => (problem, None),
                };
                findings.push(Finding {
                    place: Some(cited.place),
                    key: Some(cite.key.clone()),
                    current,
                    problem,
                });
            }
        }
        findings.sort_by_key(|finding| finding.place);
        Ok(Ok(Scanned {
            kind,
            manuscript,
            keys,
            findings,
        }))
    }

    /// Check `request.document`'s citations, and against `request.set`.
    pub fn check_document(
        &self,
        request: &CheckRequest,
        cancel: &Cancellation,
    ) -> Result<health::Checked<CheckResult>, EngineError> {
        let checked = |value| {
            Ok(health::Checked {
                source_health: health::SourceHealth::of(self.state()?.libraries.values()),
                value,
            })
        };
        let mut scanned = match self.scan_manuscript(&request.document, cancel)? {
            Ok(scanned) => scanned,
            Err(reason) => {
                return checked(CheckResult::Refused {
                    file: Which::Document,
                    reason,
                });
            }
        };
        if let Some(set) = &request.set {
            match self.against_set(set, &scanned, cancel)? {
                Ok(mut findings) => scanned.findings.append(&mut findings),
                Err(reason) => {
                    return checked(CheckResult::Refused {
                        file: Which::Set,
                        reason,
                    });
                }
            }
        }
        checked(CheckResult::Checked(Checked { read: scanned.read() }))
    }

    /// Export the records `request.document` cites, in first-citation order.
    pub fn export_document(
        &self,
        request: &ExportRequest,
        cancel: &Cancellation,
    ) -> Result<health::Checked<ExportResult>, EngineError> {
        let output = crate::outputs::resolve(&request.output)?;
        if !ibis_core::syntax::capabilities(&output).is_ok_and(|capabilities| capabilities.exports_records) {
            return Err(EngineError::citation(CitationError::UnsupportedOutput));
        }
        let scanned = match self.scan_manuscript(&request.document, cancel)? {
            Ok(scanned) => scanned,
            Err(reason) => {
                return Ok(health::Checked {
                    source_health: health::SourceHealth::of(self.state()?.libraries.values()),
                    value: ExportResult::Refused {
                        file: Which::Document,
                        reason,
                    },
                });
            }
        };
        let keys: Vec<KeyItem> = scanned
            .found()
            .into_iter()
            .map(|key| KeyItem::from(key.to_owned()))
            .collect();
        let mut read = scanned.read();
        if read.cites_everything {
            read.findings.push(Finding {
                place: None,
                key: None,
                current: None,
                problem: Problem::CitesEverything,
            });
        }
        if keys.is_empty() {
            return Ok(health::Checked {
                source_health: health::SourceHealth::of(self.state()?.libraries.values()),
                value: ExportResult::Exported { read, records: None },
            });
        }
        let records = self.cite_keys_checked(
            &KeyCitationRequest {
                strict: false,
                keys,
                output,
                form: CitationForm::default(),
                keep_order: true,
                output_locale: None,
            },
            cancel,
        )?;
        Ok(health::Checked {
            source_health: records.source_health,
            value: ExportResult::Exported {
                read,
                records: Some(Box::new(records.value)),
            },
        })
    }

    /// The cited keys a reference set lacks, and the records that changed
    /// since it was written. Both sides go through the same share-safe
    /// CSL-JSON export, so only a change in what would be cited counts.
    fn against_set(
        &self,
        set: &Path,
        scanned: &Scanned,
        cancel: &Cancellation,
    ) -> Result<Result<Vec<Finding>, Refusal>, EngineError> {
        let bytes = match ibis_sources::document::read(set, MAX_SET_BYTES, stop(cancel)) {
            Ok(bytes) => bytes,
            Err(reason) => {
                cancel.check()?;
                return Ok(Err(reason));
            }
        };
        let Ok(Value::Array(records)) = serde_json::from_slice::<Value>(&bytes) else {
            return Ok(Err(Refusal::Damaged));
        };
        let in_set = if records.is_empty() {
            BTreeMap::new()
        } else {
            match self.cite_records_within(
                &RecordsRequest {
                    strict: false,
                    records,
                    output: CSL_JSON,
                    form: CitationForm::default(),
                    output_locale: None,
                    items: None,
                },
                MAX_SET_BYTES as usize,
                cancel,
            ) {
                Ok(result) => by_key(&result.text),
                Err(failure) if failure.code == ErrorCode::Cancelled => return Err(failure),
                Err(failure)
                    if failure.code == ErrorCode::ResourceLimit
                        || failure.citation_error == Some(CitationError::TooManyRecords) =>
                {
                    return Ok(Err(Refusal::TooLarge));
                }
                Err(_) => return Ok(Err(Refusal::Damaged)),
            }
        };
        let found: Vec<KeyItem> = scanned
            .found()
            .into_iter()
            .map(|key| KeyItem::from(key.to_owned()))
            .collect();
        let current = if found.is_empty() {
            BTreeMap::new()
        } else {
            let result = self.cite_keys(
                &KeyCitationRequest {
                    strict: false,
                    keys: found,
                    output: CSL_JSON,
                    form: CitationForm::default(),
                    keep_order: true,
                    output_locale: None,
                },
                cancel,
            )?;
            by_key(&result.text)
        };
        let mut findings = Vec::new();
        // A key no library holds is already named; the set cannot add to that.
        let now: BTreeMap<&str, &str> = scanned
            .keys
            .iter()
            .filter_map(|(key, found)| Some((key.as_str(), found.as_deref().ok()?)))
            .collect();
        let mut named = HashSet::new();
        for cited in &scanned.manuscript.citations {
            for Cite { key, .. } in &cited.cites {
                // A renamed key is compared under the name the set was written with.
                let Some(&name) = now.get(key.as_str()) else { continue };
                let problem = match (in_set.get(name), current.get(name)) {
                    (None, _) => Problem::NotInSet,
                    (Some(_), None) => Problem::Uncompared,
                    (Some(then), Some(now)) if then != now => Problem::Changed,
                    _ => continue,
                };
                // The set is fixed once per key, so each is named once, at its first citation.
                if named.insert(name) {
                    findings.push(Finding {
                        place: Some(cited.place),
                        key: Some(name.to_owned()),
                        current: None,
                        problem,
                    });
                }
            }
        }
        Ok(Ok(findings))
    }
}

/// CSL-JSON records by citation key, without the file-local `id`.
fn by_key(text: &str) -> BTreeMap<String, Value> {
    let Ok(Value::Array(records)) = serde_json::from_str::<Value>(text) else {
        return BTreeMap::new();
    };
    records
        .into_iter()
        .filter_map(|mut record| {
            let fields = record.as_object_mut()?;
            fields.remove("id");
            let key = fields.get("citation-key")?.as_str()?.to_owned();
            Some((key, record))
        })
        .collect()
}

#[cfg(test)]
#[path = "document_tests.rs"]
mod tests;
