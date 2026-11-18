//! `ibis format`: a finished copy of a Word or OpenDocument file whose
//! citation markers (`[@key, p. 12]`) become citations in one style, with
//! its reference list. Every key is resolved in the open libraries and every
//! problem named before anything is written; one formatting pass gives the
//! whole document its numbers, year letters and first citations. The file is
//! parsed only by the office child; the original is never changed and an
//! existing file is never replaced.
use super::formatting::formatter_error;
use super::{Engine, LibraryInfo, error, select};
use crate::request::{ItemSelector, KeyItem};
use ibis_core::contract::*;
use ibis_core::syntax::marker::{self, Found, Marker, MarkerProblemKind};
use ibis_sources::office::{self, Bibliography, Edit, Edits, Kind, Part};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Make `output` from `input` with its markers cited in `style`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatRequest {
    pub input: PathBuf,
    pub output: PathBuf,
    pub style: String,
    pub output_locale: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum FormatOutcome {
    Written(FormatReport),
    Refused(FormatRefusal),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FormatReport {
    pub kind: Kind,
    /// Citations written.
    pub citations: usize,
    /// Entries in the reference list.
    pub references: usize,
    /// Whether the list replaced a `[bibliography]` paragraph.
    pub placed: bool,
    /// Markers left as they are, outside the body.
    pub left: Vec<FormatPlace>,
}

/// Where a marker is, as a person finds it: the body's paragraphs, or the
/// part's, counted from 1, and the character in it, from 1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FormatPlace {
    pub part: Part,
    pub paragraph: usize,
    pub column: usize,
    /// The marker as written, shortened.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "reason", content = "detail", rename_all = "kebab-case")]
pub enum FormatRefusal {
    /// The document could not be read or written.
    Document(office::Refusal),
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
    /// The output path is the input itself.
    SameFile,
    /// The output's extension is not the input's kind.
    OutputKind,
    /// The output could not be written there.
    OutputUnwritable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FormatIssue {
    pub place: FormatPlace,
    pub issue: Issue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Issue {
    /// Text that starts a citation but is not one.
    Marker { problem: MarkerProblemKind },
    /// A reference the libraries cannot give.
    Key { key: String, problem: KeyProblem },
    /// A second `[bibliography]` paragraph.
    SecondBibliography,
}

/// One citation in a document: its references, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DocumentCitation {
    pub keys: Vec<KeyItem>,
    /// The author as the sentence's subject: one reference.
    pub narrative: bool,
}

/// A document's citations, in document order, in one style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DocumentRequest {
    pub citations: Vec<DocumentCitation>,
    pub style: String,
    pub output_locale: Option<String>,
}

/// Why one reference in a document cannot be cited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
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
    #[serde(untagged)]
    Citation(CitationError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DocumentProblem {
    /// The citation's place in the request, from zero.
    pub citation: usize,
    /// The reference's place in its citation, from zero.
    pub item: usize,
    pub problem: KeyProblem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DocumentReply {
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
    pub(crate) fn cite_document(
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
        let Resolved {
            items,
            places,
            clusters,
            problems,
        } = resolve(snapshot, &state.libraries, &request.citations, cancel)?;
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

impl Engine {
    /// `ibis format`: write `request.output`, a copy of `request.input` with
    /// its markers cited and its reference list, or refuse with every reason.
    pub fn format_file(&self, request: &FormatRequest, cancel: &super::Cancellation) -> Result<FormatOutcome, EngineError> {
        let refused = |refusal| Ok(FormatOutcome::Refused(refusal));
        let Some(reader) = &self.office else {
            return refused(FormatRefusal::Document(office::Refusal::Unavailable));
        };
        let output = std::path::absolute(&request.output).map_err(|_| error(ErrorCode::BadInput))?;
        let input = std::path::absolute(&request.input).map_err(|_| error(ErrorCode::BadInput))?;
        if same_file(&input, &output) {
            return refused(FormatRefusal::SameFile);
        }
        if output.symlink_metadata().is_ok() {
            return refused(FormatRefusal::OutputExists);
        }
        let stop = cancel.clone();
        let cancelled: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || stop.is_cancelled());
        let document = match office::read(&input, cancelled.clone()) {
            Ok(document) => document,
            Err(refusal) => return refused(FormatRefusal::Document(refusal)),
        };
        let scanned = match reader.scan(&document, &*cancelled) {
            Ok(scanned) => scanned,
            Err(refusal) => return refused(FormatRefusal::Document(refusal)),
        };
        cancel.check()?;
        let extension = output
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase);
        let expected = match scanned.kind {
            Kind::Docx => "docx",
            Kind::Odt => "odt",
        };
        if extension.as_deref() != Some(expected) {
            return refused(FormatRefusal::OutputKind);
        }
        let mut read = Markers::read(&scanned.paragraphs);
        if read.markers.is_empty() && read.issues.is_empty() {
            return refused(FormatRefusal::NoMarkers);
        }
        let citations: Vec<DocumentCitation> = read
            .markers
            .iter()
            .map(|(_, marker)| DocumentCitation {
                narrative: marker.narrative,
                keys: marker
                    .cites
                    .iter()
                    .map(|cite| KeyItem {
                        key: cite.key.clone(),
                        locator: cite.locator.clone(),
                        prefix: plain(&cite.prefix),
                        suffix: plain(&cite.suffix),
                        suppress_author: cite.suppress_author,
                    })
                    .collect(),
            })
            .collect();
        if !read.issues.is_empty() {
            // Name every problem at once: keys the libraries lack, too.
            let state = self.state()?;
            let problems = resolve(state.index.snapshot(), &state.libraries, &citations, cancel)?.problems;
            let mut issues = read.key_issues(&scanned.paragraphs, problems);
            issues.append(&mut read.issues);
            issues.sort_by_key(|issue| (issue.place.part != Part::Body, issue.place.paragraph, issue.place.column));
            return refused(FormatRefusal::Markers(issues));
        }
        let reply = self.cite_document(
            &DocumentRequest {
                citations,
                style: request.style.clone(),
                output_locale: request.output_locale.clone(),
            },
            cancel,
        )?;
        let formatted = match reply {
            DocumentReply::Formatted(formatted) => formatted,
            DocumentReply::NoteStyle => return refused(FormatRefusal::NoteStyle),
            DocumentReply::StyleCannotWrite => return refused(FormatRefusal::StyleCannotWrite),
            DocumentReply::Refused(problems) => {
                return refused(FormatRefusal::Markers(read.key_issues(&scanned.paragraphs, problems)));
            }
        };
        let edits = Edits {
            citations: read
                .markers
                .iter()
                .zip(formatted.citations)
                .map(|((paragraph, marker), runs)| Edit {
                    paragraph: *paragraph,
                    start: marker.start,
                    end: marker.end,
                    runs,
                })
                .collect(),
            bibliography: Bibliography {
                at: read.bibliography,
                entries: formatted.entries,
                hanging_indent: formatted.hanging_indent,
            },
        };
        let written = match reader.write(&document, &edits, &*cancelled) {
            Ok(written) => written,
            Err(refusal) => return refused(FormatRefusal::Document(refusal)),
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
        Ok(FormatOutcome::Written(FormatReport {
            kind: scanned.kind,
            citations: edits.citations.len(),
            references: edits.bibliography.entries.len(),
            placed: edits.bibliography.at.is_some(),
            left: read.left,
        }))
    }
}

/// The markers of a scanned document, by body paragraph, and what stops it.
struct Markers {
    markers: Vec<(usize, Marker)>,
    issues: Vec<FormatIssue>,
    left: Vec<FormatPlace>,
    bibliography: Option<usize>,
}

impl Markers {
    fn read(paragraphs: &[office::Paragraph]) -> Self {
        let mut read = Self {
            markers: Vec::new(),
            issues: Vec::new(),
            left: Vec::new(),
            bibliography: None,
        };
        for (index, paragraph) in paragraphs.iter().enumerate() {
            if paragraph.part == Part::Hidden {
                continue;
            }
            if paragraph.part == Part::Body && paragraph.text.trim() == "[bibliography]" {
                if read.bibliography.is_some() {
                    let place = read.place(paragraphs, index, 0, paragraph.text.len());
                    read.issues.push(FormatIssue {
                        place,
                        issue: Issue::SecondBibliography,
                    });
                }
                read.bibliography.get_or_insert(index);
                continue;
            }
            for found in marker::markers(&paragraph.text) {
                match found {
                    Found::Marker(marker) if paragraph.part == Part::Body => read.markers.push((index, marker)),
                    Found::Marker(marker) => {
                        let place = read.place(paragraphs, index, marker.start, marker.end);
                        read.left.push(place);
                    }
                    Found::Problem(problem) if paragraph.part == Part::Body => {
                        let place = read.place(paragraphs, index, problem.start, problem.end);
                        read.issues.push(FormatIssue {
                            place,
                            issue: Issue::Marker { problem: problem.kind },
                        });
                    }
                    Found::Problem(_) => {}
                }
            }
        }
        read
    }

    fn key_issues(&self, paragraphs: &[office::Paragraph], problems: Vec<DocumentProblem>) -> Vec<FormatIssue> {
        problems
            .into_iter()
            .map(|problem| {
                let (paragraph, marker) = &self.markers[problem.citation];
                FormatIssue {
                    place: self.place(paragraphs, *paragraph, marker.start, marker.end),
                    issue: Issue::Key {
                        key: marker.cites[problem.item].key.clone(),
                        problem: problem.problem,
                    },
                }
            })
            .collect()
    }

    fn place(&self, paragraphs: &[office::Paragraph], index: usize, start: usize, end: usize) -> FormatPlace {
        let paragraph = &paragraphs[index];
        let text = &paragraph.text;
        let shown: String = text[start..end].chars().take(80).collect();
        FormatPlace {
            part: paragraph.part,
            paragraph: paragraphs[..index].iter().filter(|other| other.part == paragraph.part).count() + 1,
            column: text[..start].chars().count() + 1,
            text: if shown.len() < end - start { format!("{shown}…") } else { shown },
        }
    }
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

/// Each reference of `citations` in the libraries, or why it is not there.
struct Resolved {
    items: Vec<CitationItem>,
    /// Each item's citation and place in it.
    places: Vec<(usize, usize)>,
    /// Each citation's items.
    clusters: Vec<Vec<usize>>,
    problems: Vec<DocumentProblem>,
}

fn resolve(
    snapshot: &Snapshot,
    libraries: &std::collections::BTreeMap<LibraryId, LibraryInfo>,
    citations: &[DocumentCitation],
    cancel: &super::Cancellation,
) -> Result<Resolved, EngineError> {
    let mut resolved = Resolved {
        items: Vec::new(),
        places: Vec::new(),
        clusters: Vec::with_capacity(citations.len()),
        problems: Vec::new(),
    };
    for (citation_index, citation) in citations.iter().enumerate() {
        let mut cluster = Vec::with_capacity(citation.keys.len());
        for (item_index, key) in citation.keys.iter().enumerate() {
            cancel.check()?;
            match select(snapshot, libraries, &ItemSelector::Key(key.key.clone())) {
                Ok(selected) => {
                    cluster.push(resolved.items.len());
                    resolved.places.push((citation_index, item_index));
                    resolved.items.push(CitationItem {
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
                Err(failure) => resolved.problems.push(DocumentProblem {
                    citation: citation_index,
                    item: item_index,
                    problem: key_problem(failure),
                }),
            }
        }
        resolved.clusters.push(cluster);
    }
    Ok(resolved)
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
    use super::*;

    const LIBRARY: &str = "\
@book{vale2024, author = {Vale, Avery}, title = {Lanterns of the North}, year = {2024}, publisher = {Imaginary Press}}
@book{vale2024tides, author = {Vale, Avery}, title = {Tides of the South}, year = {2024}, publisher = {Imaginary Press}}
@article{fen2023, author = {Fen, Rowan}, title = {Clouds over invented hills}, journal = {Journal of Made-up Weather}, year = {2023}, volume = {4}, pages = {1--9}}
";

    fn engine() -> (Engine, tempfile::TempDir) {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("refs.bib");
        std::fs::write(&path, LIBRARY).unwrap();
        let engine = Engine::new().unwrap().without_file_watches();
        engine.open_file(&path, &crate::Cancellation::default()).unwrap();
        (engine, folder)
    }

    fn key(key: &str) -> KeyItem {
        KeyItem::from(key)
    }

    fn citation(keys: &[KeyItem]) -> DocumentCitation {
        DocumentCitation {
            keys: keys.to_vec(),
            narrative: false,
        }
    }

    fn text(runs: &[DisplayRun]) -> String {
        runs.iter().map(|run| run.text.as_str()).collect()
    }

    fn cite(engine: &Engine, style: &str, citations: Vec<DocumentCitation>) -> DocumentCitations {
        let reply = engine
            .cite_document(
                &DocumentRequest {
                    citations,
                    style: style.into(),
                    output_locale: None,
                },
                &crate::Cancellation::default(),
            )
            .unwrap();
        let DocumentReply::Formatted(formatted) = reply else {
            panic!("{reply:?}")
        };
        formatted
    }

    fn page(value: &str) -> Option<Locator> {
        Some(Locator {
            term: LocatorTerm::Page,
            value: value.into(),
        })
    }

    #[test]
    fn a_document_is_cited_as_one_sequence() {
        let (engine, _folder) = engine();
        let sequence = || {
            vec![
                citation(&[KeyItem {
                    locator: page("12"),
                    ..key("vale2024")
                }]),
                citation(&[key("vale2024tides"), key("fen2023")]),
                DocumentCitation {
                    keys: vec![key("fen2023")],
                    narrative: true,
                },
            ]
        };
        let apa = cite(&engine, "apa", sequence());
        let texts: Vec<String> = apa.citations.iter().map(|runs| text(runs)).collect();
        assert_eq!(texts, ["(Vale, 2024a, p. 12)", "(Fen, 2023; Vale, 2024b)", "Fen (2023)"]);
        assert_eq!(apa.entries.len(), 3);
        assert!(apa.hanging_indent);
        assert!(text(&apa.entries[1].content).starts_with("Vale, A. (2024a)."));
        assert!(apa.entries.iter().any(|entry| entry.content.iter().any(|run| run.italic)));

        // Without document context, `ibis cite` writes the same citation.
        let alone = engine
            .cite_keys(
                &crate::request::KeyCitationRequest {
                    strict: false,
                    keys: vec![key("fen2023")],
                    output: OutputSpec::Csl { style: "apa".into() },
                    form: CitationForm::Narrative,
                    keep_order: false,
                    output_locale: None,
                },
                &crate::Cancellation::default(),
            )
            .unwrap();
        assert_eq!(alone.text, texts[2]);

        let ieee = cite(&engine, "ieee", sequence()[..2].to_vec());
        let texts: Vec<String> = ieee.citations.iter().map(|runs| text(runs)).collect();
        assert_eq!(texts, ["[1, p. 12]", "[2], [3]"]);
        let margins: Vec<String> = ieee.entries.iter().map(|entry| text(&entry.margin)).collect();
        assert_eq!(margins, ["[1]", "[2]", "[3]"]);
        assert!(text(&ieee.entries[2].content).starts_with("R. Fen,"));
    }

    #[test]
    fn every_unknown_key_is_named_and_note_styles_wait() {
        let (engine, _folder) = engine();
        let reply = engine
            .cite_document(
                &DocumentRequest {
                    citations: vec![citation(&[key("fen2023"), key("nobody")]), citation(&[key("ghost")])],
                    style: "apa".into(),
                    output_locale: None,
                },
                &crate::Cancellation::default(),
            )
            .unwrap();
        assert_eq!(
            reply,
            DocumentReply::Refused(vec![
                DocumentProblem {
                    citation: 0,
                    item: 1,
                    problem: KeyProblem::Missing
                },
                DocumentProblem {
                    citation: 1,
                    item: 0,
                    problem: KeyProblem::Missing
                },
            ])
        );
        let reply = engine
            .cite_document(
                &DocumentRequest {
                    citations: vec![citation(&[key("fen2023")])],
                    style: "chicago-note-bibliography".into(),
                    output_locale: None,
                },
                &crate::Cancellation::default(),
            )
            .unwrap();
        assert_eq!(reply, DocumentReply::NoteStyle);
    }

    #[test]
    fn a_locator_counts_as_printed_by_its_first_part() {
        assert!(super::printed("321-328", "(Vale, 2024, pp. 321–28)"));
        assert!(super::printed("xii", "[1, p. XII]"));
        assert!(!super::printed("12", "(Vale, 2024)"));
    }
}

