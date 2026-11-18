//! A document's citations formatted as one sequence (`ibis format`): every
//! key resolved in the libraries, every problem named, then one formatting
//! pass so numbers, year letters and first citations follow the document.
use super::formatting::formatter_error;
use super::{Engine, error, select};
use crate::request::{ItemSelector, KeyItem};
use ibis_core::contract::*;

/// One citation in a document: its references, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentCitation {
    pub keys: Vec<KeyItem>,
    /// The author as the sentence's subject: one reference.
    pub narrative: bool,
}

/// A document's citations, in document order, in one style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentRequest {
    pub citations: Vec<DocumentCitation>,
    pub style: String,
    pub output_locale: Option<String>,
}

/// Why one reference in a document cannot be cited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyProblem {
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
    /// The style printed the citation without its locator.
    LocatorNotPrinted,
    /// The style cannot cite this reference this way.
    Citation(CitationError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentProblem {
    /// The citation's place in the request, from zero.
    pub citation: usize,
    /// The reference's place in its citation, from zero.
    pub item: usize,
    pub problem: KeyProblem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentReply {
    Formatted(DocumentCitations),
    /// Every problem found, in document order; nothing is formatted.
    Refused(Vec<DocumentProblem>),
    /// The style writes notes; this first version writes in-text citations.
    NoteStyle,
    /// The style could not write these citations as a whole.
    StyleCannotWrite,
}

impl Engine {
    /// Format `request`'s citations as one document. A problem with any
    /// reference refuses the whole document and names every problem.
    pub fn cite_document(
        &self,
        request: &DocumentRequest,
        cancel: &super::Cancellation,
    ) -> Result<DocumentReply, EngineError> {
        cancel.check()?;
        let output = ibis_csl::ResolvedOutput::new(&request.style, request.output_locale.as_deref())
            .map_err(formatter_error)?;
        if ibis_csl::Formatter::style_writes_notes(&request.style).map_err(formatter_error)? {
            return Ok(DocumentReply::NoteStyle);
        }
        let count: usize = request.citations.iter().map(|citation| citation.keys.len()).sum();
        if request.citations.is_empty() || request.citations.iter().any(|citation| citation.keys.is_empty()) {
            return Err(error(ErrorCode::BadInput));
        }
        ibis_core::citation::check_item_limit(count).map_err(|_| EngineError::citation(CitationError::TooManyItems))?;
        let state = self.state()?;
        let snapshot = state.index.snapshot();
        let mut problems = Vec::new();
        let mut items = Vec::with_capacity(count);
        let mut places = Vec::with_capacity(count);
        let mut clusters = Vec::with_capacity(request.citations.len());
        for (citation_index, citation) in request.citations.iter().enumerate() {
            let mut cluster = Vec::with_capacity(citation.keys.len());
            for (item_index, key) in citation.keys.iter().enumerate() {
                cancel.check()?;
                match select(snapshot, &state.libraries, &ItemSelector::Key(key.key.clone())) {
                    Ok(selected) => {
                        cluster.push(items.len());
                        places.push((citation_index, item_index));
                        items.push(CitationItem {
                            item: selected.object.id().clone(),
                            expected_cite: Some(selected.fingerprints.cite.clone()),
                            form: citation.narrative.then_some(CitationForm::Narrative),
                            locator: key.locator.clone(),
                            prefix: key.prefix.clone(),
                            suffix: key.suffix.clone(),
                            suppress_author: key.suppress_author,
                            annotation: None,
                            expected_annotation_cite: None,
                        });
                    }
                    Err(failure) => problems.push(DocumentProblem {
                        citation: citation_index,
                        item: item_index,
                        problem: key_problem(failure),
                    }),
                }
            }
            clusters.push(cluster);
        }
        if !problems.is_empty() {
            return Ok(DocumentReply::Refused(problems));
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
            .format_document(snapshot, &citation_request, &output, &clusters)
            .map_err(|failure| match failure {
                // A problem the validator finds before any item is a whole-document one.
                ibis_csl::FormatError::Citation(error) => EngineError::citation(error),
                other => formatter_error(other),
            })?;
        cancel.check()?;
        let formatted = match answer {
            ibis_csl::DocumentAnswer::Formatted(formatted) => formatted,
            ibis_csl::DocumentAnswer::Refused(refused) if refused.is_empty() => {
                return Ok(DocumentReply::StyleCannotWrite);
            }
            ibis_csl::DocumentAnswer::Refused(refused) => {
                return Ok(DocumentReply::Refused(
                    refused
                        .into_iter()
                        .filter_map(|(index, error)| {
                            places.get(index).map(|&(citation, item)| DocumentProblem {
                                citation,
                                item,
                                problem: KeyProblem::Citation(error),
                            })
                        })
                        .collect(),
                ));
            }
        };
        if formatted.citations.len() != request.citations.len() {
            return Err(error(ErrorCode::Internal));
        }
        // A locator the style leaves out would change what the text claims.
        let problems: Vec<_> = request
            .citations
            .iter()
            .zip(&formatted.citations)
            .enumerate()
            .flat_map(|(citation, (asked, runs))| {
                let text: String = runs.iter().map(|run| run.text.as_str()).collect();
                asked.keys.iter().enumerate().filter_map(move |(item, key)| {
                    key.locator
                        .as_ref()
                        .filter(|locator| !printed(&locator.value, &text))
                        .map(|_| DocumentProblem {
                            citation,
                            item,
                            problem: KeyProblem::LocatorNotPrinted,
                        })
                })
            })
            .collect();
        if !problems.is_empty() {
            return Ok(DocumentReply::Refused(problems));
        }
        Ok(DocumentReply::Formatted(formatted))
    }
}

fn key_problem(failure: EngineError) -> KeyProblem {
    match (failure.code, failure.reason) {
        (_, Some(ErrorReason::AmbiguousKey)) => KeyProblem::Ambiguous,
        (_, Some(ErrorReason::DuplicateKey)) => KeyProblem::Duplicate,
        (_, Some(ErrorReason::DamagedEntry)) => KeyProblem::Damaged,
        (_, Some(ErrorReason::LibraryCutShort)) => KeyProblem::CutShort,
        (ErrorCode::NotFound, _) => KeyProblem::Missing,
        _ => KeyProblem::Invalid,
    }
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

#[cfg(test)]
mod tests {
    #[test]
    fn a_locator_counts_as_printed_by_its_first_part() {
        assert!(super::printed("321-328", "(Vale, 2024, pp. 321–28)"));
        assert!(super::printed("xii", "[1, p. XII]"));
        assert!(!super::printed("12", "(Vale, 2024)"));
    }
}

