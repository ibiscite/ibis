//! `ibis format` (decision 0060): a finished copy of a Word or OpenDocument
//! file whose citation markers (`[@key, p. 12]`) become citations in one
//! style, with its reference list. Every key is resolved in the open
//! libraries and every problem named before anything is written; one
//! formatting pass gives the whole document its numbers, year letters and
//! first citations. Only the document child reads or writes the file's
//! contents; the original is never changed and an existing file is never
//! replaced. Re-running on the original with another style restyles it.

use super::super::formatting::formatter_error;
use super::super::{Cancellation, Engine, error, select};
use super::{Problem as KeyProblem, key_problem, stop};
use crate::request::ItemSelector;
use ibis_core::contract::{
    CitationError, CitationForm, CitationItem, CitationRequest, DocumentCitations, EngineError, ErrorCode, OutputSpec,
    RichText, Script, TextRun,
};
use ibis_sources::document::{Kind, OfficeCited, OfficeIssue, OfficePlace, Refusal};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Make `output` from `document` with its markers cited in `style`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FormatRequest {
    /// A Word (`.docx`) or OpenDocument (`.odt`) file.
    pub document: PathBuf,
    /// A new file of the same kind; nothing already there is replaced.
    pub output: PathBuf,
    /// A CSL style id from `styles`.
    pub style: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_locale: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum FormatResult {
    Written(FormatReport),
    Refused(FormatRefusal),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FormatReport {
    pub kind: Kind,
    /// Citations written.
    pub citations: usize,
    /// Entries in the reference list.
    pub references: usize,
    /// Whether the list replaced a `[bibliography]` paragraph.
    pub placed: bool,
    /// Markers left as they are, outside the body.
    pub left: Vec<OfficePlace>,
    /// The style and locale the copy was written in, to make it again.
    pub style: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_locale: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "reason", content = "detail", rename_all = "kebab-case")]
pub enum FormatRefusal {
    /// The document could not be read or written.
    Document(Refusal),
    /// Every marker and key problem, in document order.
    Markers(Vec<FormatIssue>),
    /// The body has no citation marker.
    NoMarkers,
    /// The style writes notes; this version writes in-text citations.
    NoteStyle,
    /// The style could not write these citations as a whole.
    StyleCannotWrite,
    /// Something is already at the output path.
    OutputExists,
    /// The output path is the document itself.
    SameFile,
    /// The output's extension is not the document's kind.
    OutputKind,
    /// The output could not be written there.
    OutputUnwritable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FormatIssue {
    pub place: OfficePlace,
    pub issue: Issue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Issue {
    /// Text that starts a citation but is not one, or a second `[bibliography]`.
    Document(OfficeIssue),
    /// A reference the libraries cannot give.
    Key { key: String, problem: KeyProblem },
    /// The style cannot cite this reference this way.
    Citation { key: String, problem: CitationError },
    /// The style printed the citation without its locator.
    LocatorNotPrinted { key: String },
}

impl Engine {
    /// `ibis format`: write `request.output`, a copy of `request.document`
    /// with its markers cited and its reference list, or refuse with every reason.
    pub fn format_document(&self, request: &FormatRequest, cancel: &Cancellation) -> Result<FormatResult, EngineError> {
        let refused = |refusal| Ok(FormatResult::Refused(refusal));
        let Some(reader) = &self.documents else {
            return refused(FormatRefusal::Document(Refusal::Unavailable));
        };
        let output = std::path::absolute(&request.output).map_err(|_| error(ErrorCode::BadInput))?;
        let input = std::path::absolute(&request.document).map_err(|_| error(ErrorCode::BadInput))?;
        let Some(kind) = Kind::of(&input) else {
            return refused(FormatRefusal::Document(Refusal::Unsupported));
        };
        if Kind::of(&output) != Some(kind) {
            return refused(FormatRefusal::OutputKind);
        }
        if same_file(&input, &output) {
            return refused(FormatRefusal::SameFile);
        }
        if output.symlink_metadata().is_ok() {
            return refused(FormatRefusal::OutputExists);
        }
        // The style is checked before the document is read.
        let resolved =
            ibis_csl::ResolvedOutput::new(&request.style, request.output_locale.as_deref()).map_err(formatter_error)?;
        if ibis_csl::Formatter::style_writes_notes(&request.style).map_err(formatter_error)? {
            return refused(FormatRefusal::NoteStyle);
        }
        let cancelled = stop(cancel);
        let document =
            match ibis_sources::document::read(&input, ibis_sources::document::MAX_OFFICE_BYTES, cancelled.clone()) {
                Ok(document) => document,
                Err(refusal) => {
                    cancel.check()?;
                    return refused(FormatRefusal::Document(refusal));
                }
            };
        let scanned = match reader.scan_office(&document, &*cancelled) {
            Ok(scanned) if scanned.kind == kind => scanned,
            Ok(_) => return refused(FormatRefusal::Document(Refusal::Unsupported)),
            Err(refusal) => {
                cancel.check()?;
                return refused(FormatRefusal::Document(refusal));
            }
        };
        cancel.check()?;
        if scanned.citations.is_empty() && scanned.problems.is_empty() {
            return refused(FormatRefusal::NoMarkers);
        }
        let count: usize = scanned.citations.iter().map(|cited| cited.cites.len()).sum();
        ibis_core::citation::check_item_limit(count).map_err(|_| EngineError::citation(CitationError::TooManyItems))?;
        // Every problem at once: the document's, then the keys the libraries lack.
        let mut issues: Vec<FormatIssue> = scanned
            .problems
            .iter()
            .map(|problem| FormatIssue {
                place: problem.place.clone(),
                issue: Issue::Document(problem.issue),
            })
            .collect();
        let state = self.state()?;
        let snapshot = state.index.snapshot();
        let mut items = Vec::with_capacity(count);
        let mut clusters = Vec::with_capacity(scanned.citations.len());
        let mut places = Vec::with_capacity(count);
        for (index, cited) in scanned.citations.iter().enumerate() {
            let mut cluster = Vec::with_capacity(cited.cites.len());
            for (item, cite) in cited.cites.iter().enumerate() {
                cancel.check()?;
                match select(snapshot, &state.libraries, &ItemSelector::Key(cite.key.clone())) {
                    Ok(selected) => {
                        cluster.push(items.len());
                        places.push((index, item));
                        items.push(CitationItem {
                            item: selected.object.id().clone(),
                            expected_cite: Some(selected.fingerprints.cite.clone()),
                            form: cited.narrative.then_some(CitationForm::Narrative),
                            locator: cite.locator.clone(),
                            prefix: plain(&cite.prefix),
                            suffix: plain(&cite.suffix),
                            suppress_author: cite.suppress_author,
                            annotation: None,
                            expected_annotation_cite: None,
                        });
                    }
                    Err(failure) => issues.push(FormatIssue {
                        place: cited.place.clone(),
                        issue: Issue::Key {
                            key: cite.key.clone(),
                            problem: key_problem(&failure),
                        },
                    }),
                }
            }
            clusters.push(cluster);
        }
        if !issues.is_empty() {
            issues.sort_by_key(|issue| (issue.place.part, issue.place.paragraph, issue.place.column));
            return refused(FormatRefusal::Markers(issues));
        }
        let citation_request = CitationRequest {
            strict: true,
            items,
            form: CitationForm::Normal,
            keep_order: false,
            output: OutputSpec::Csl {
                style: request.style.clone(),
            },
            input_language: None,
            output_locale: request.output_locale.clone(),
        };
        let formatter = self.formatting.formatter()?;
        let answer = formatter
            .format_document(snapshot, &citation_request, &resolved, &clusters)
            .map_err(|failure| match failure {
                // A problem the validator finds before any item is a whole-document one.
                ibis_csl::FormatError::Citation(error) => EngineError::citation(error),
                other => formatter_error(other),
            })?;
        cancel.check()?;
        let formatted = match answer {
            ibis_csl::DocumentAnswer::Formatted(formatted) => formatted,
            ibis_csl::DocumentAnswer::Refused(refused_items) if refused_items.is_empty() => {
                return refused(FormatRefusal::StyleCannotWrite);
            }
            ibis_csl::DocumentAnswer::Refused(refused_items) => {
                let issues = refused_items
                    .into_iter()
                    .filter_map(|(index, problem)| {
                        let &(citation, item) = places.get(index)?;
                        let cited = &scanned.citations[citation];
                        Some(FormatIssue {
                            place: cited.place.clone(),
                            issue: Issue::Citation {
                                key: cited.cites[item].key.clone(),
                                problem,
                            },
                        })
                    })
                    .collect();
                return refused(FormatRefusal::Markers(issues));
            }
        };
        if formatted.citations.len() != scanned.citations.len() {
            return Err(error(ErrorCode::Internal));
        }
        let issues = unprinted(&scanned.citations, &formatted);
        if !issues.is_empty() {
            return refused(FormatRefusal::Markers(issues));
        }
        let references = formatted.entries.len();
        let written = match reader.write_office(
            &document,
            formatted.citations,
            formatted.entries,
            formatted.hanging_indent,
            &*cancelled,
        ) {
            Ok(written) => written,
            Err(refusal) => {
                cancel.check()?;
                return refused(FormatRefusal::Document(refusal));
            }
        };
        cancel.check()?;
        // A new file only: never one that appeared while formatting.
        let created = ibis_core::atomic_file::AtomicFile::new(&output, &written).and_then(|file| file.create());
        match created {
            Ok(()) => {}
            Err(failure) if failure.kind() == std::io::ErrorKind::AlreadyExists => {
                return refused(FormatRefusal::OutputExists);
            }
            Err(_) => return refused(FormatRefusal::OutputUnwritable),
        }
        Ok(FormatResult::Written(FormatReport {
            kind,
            citations: scanned.citations.len(),
            references,
            placed: scanned.placed,
            left: scanned.left,
            style: request.style.clone(),
            output_locale: request.output_locale.clone(),
        }))
    }
}

/// A locator the style leaves out would change what the text claims.
fn unprinted(citations: &[OfficeCited], formatted: &DocumentCitations) -> Vec<FormatIssue> {
    citations
        .iter()
        .zip(&formatted.citations)
        .flat_map(|(cited, runs)| {
            let text: String = runs.iter().map(|run| run.text.as_str()).collect();
            cited.cites.iter().filter_map(move |cite| {
                cite.locator
                    .as_ref()
                    .filter(|locator| !printed(&locator.value, &text))
                    .map(|_| FormatIssue {
                        place: cited.place.clone(),
                        issue: Issue::LocatorNotPrinted { key: cite.key.clone() },
                    })
            })
        })
        .collect()
}

/// Whether a citation's text shows the locator's first number or word: a
/// check that catches a style dropping locators, not proof one printed.
/// Only the first: styles shorten ranges (`321–28`) and change dashes.
fn printed(locator: &str, text: &str) -> bool {
    let first: String = locator
        .chars()
        .skip_while(|ch| !ch.is_alphanumeric())
        .take_while(|ch| ch.is_alphanumeric())
        .collect();
    first.is_empty() || text.to_lowercase().contains(&first.to_lowercase())
}

fn plain(text: &str) -> RichText {
    if text.is_empty() {
        return Vec::new();
    }
    vec![TextRun {
        text: text.to_owned(),
        italic: false,
        bold: false,
        script: Script::Normal,
    }]
}

/// Whether two paths name one file, as far as can be known.
fn same_file(input: &Path, output: &Path) -> bool {
    input == output
        || matches!(
            (std::fs::canonicalize(input), output.parent().map(std::fs::canonicalize), output.file_name()),
            (Ok(input), Some(Ok(folder)), Some(name)) if input == folder.join(name)
        )
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_locator_counts_as_printed_by_its_first_part() {
        assert!(super::printed("321-328", "(Vale, 2024, pp. 321–28)"));
        assert!(super::printed("xii", "[1, p. XII]"));
        assert!(!super::printed("12", "(Vale, 2024)"));
    }
}
