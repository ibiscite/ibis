//! Word (`.docx`) and OpenDocument (`.odt`) text, read and written only in
//! the document child (decision 0060). The zip archive and its XML are read
//! with pure-Rust parsers (`zip`, `quick-xml`) within fixed bounds; the
//! citation markers in the body's paragraphs are read here too, so nothing
//! but places, keys and short excerpts leaves the child.

use super::{DocumentEntry, Kind, OfficeCited, OfficeIssue, OfficePlace, OfficeProblem, OfficeScan, Part, Refusal};
use ibis_core::contract::DisplayRun;
use ibis_core::syntax::marker::{self, Found};

mod archive;
mod docx;
mod odt;
mod xml;

/// The largest office file read.
pub(super) const MAX_FILE_BYTES: u64 = super::MAX_OFFICE_BYTES;
/// The most text one document may hold, in bytes.
const MAX_TEXT_BYTES: usize = 64 * 1024 * 1024;
/// The most paragraphs one document may hold.
const MAX_PARAGRAPHS: usize = 1_000_000;
/// Characters of a marker shown where it is named.
const SHOWN: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Paragraph {
    part: Part,
    text: String,
}

/// Replace `start..end` (bytes of the paragraph's text) with `runs`, in the
/// formatting of the text it replaces.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Edit {
    paragraph: usize,
    start: usize,
    end: usize,
    runs: Vec<DisplayRun>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Bibliography {
    /// The body paragraph that reads `[bibliography]`, replaced by the
    /// entries; without one, they follow the body's last paragraph.
    at: Option<usize>,
    entries: Vec<DocumentEntry>,
    hanging_indent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Edits {
    /// In document order, none overlapping.
    citations: Vec<Edit>,
    bibliography: Bibliography,
}

/// Every paragraph of the document, the body's first and in order.
fn paragraphs(bytes: &[u8]) -> Result<(Kind, Vec<Paragraph>), Refusal> {
    let mut archive = archive::Archive::open(bytes)?;
    let kind = kind(&mut archive)?;
    let mut frames = xml::Frames::default();
    match kind {
        Kind::Docx => docx::scan(&mut archive, &mut frames)?,
        Kind::Odt => odt::scan(&mut archive, &mut frames)?,
    }
    Ok((kind, frames.paragraphs))
}

/// The body's markers, by paragraph, with what stops formatting.
struct Read {
    paragraphs: Vec<Paragraph>,
    scan: OfficeScan,
    /// Each body citation's paragraph and bytes.
    spans: Vec<(usize, usize, usize)>,
    bibliography: Option<usize>,
}

fn read(bytes: &[u8]) -> Result<Read, Refusal> {
    let (kind, paragraphs) = paragraphs(bytes)?;
    let mut read = Read {
        paragraphs: Vec::new(),
        scan: OfficeScan {
            kind,
            citations: Vec::new(),
            problems: Vec::new(),
            left: Vec::new(),
            placed: false,
        },
        spans: Vec::new(),
        bibliography: None,
    };
    let mut counts = std::collections::BTreeMap::<Part, usize>::new();
    for (index, paragraph) in paragraphs.iter().enumerate() {
        let number = {
            let count = counts.entry(paragraph.part).or_default();
            *count += 1;
            *count
        };
        let place = |start: usize, end: usize| {
            let text = &paragraph.text;
            let shown: String = text[start..end].chars().take(SHOWN).collect();
            OfficePlace {
                part: paragraph.part,
                paragraph: number,
                column: text[..start].chars().count() + 1,
                text: if shown.len() < end - start {
                    format!("{shown}…")
                } else {
                    shown
                },
            }
        };
        if paragraph.part == Part::Hidden {
            continue;
        }
        if paragraph.part == Part::Body && paragraph.text.trim() == "[bibliography]" {
            if read.bibliography.is_some() {
                read.scan.problems.push(OfficeProblem {
                    place: place(0, paragraph.text.len()),
                    issue: OfficeIssue::SecondBibliography,
                });
            }
            read.bibliography.get_or_insert(index);
            continue;
        }
        for found in marker::markers(&paragraph.text) {
            match found {
                Found::Marker(marker) if paragraph.part == Part::Body => {
                    read.spans.push((index, marker.start, marker.end));
                    read.scan.citations.push(OfficeCited {
                        place: place(marker.start, marker.end),
                        narrative: marker.narrative,
                        cites: marker.cites,
                    });
                }
                Found::Marker(marker) => read.scan.left.push(place(marker.start, marker.end)),
                Found::Problem(problem) if paragraph.part == Part::Body => read.scan.problems.push(OfficeProblem {
                    place: place(problem.start, problem.end),
                    issue: OfficeIssue::Marker { problem: problem.kind },
                }),
                Found::Problem(_) => {}
            }
        }
    }
    read.scan.placed = read.bibliography.is_some();
    read.paragraphs = paragraphs;
    Ok(read)
}

/// What the parent sees of the document: its kind, body citations, problems
/// and markers left outside the body.
pub(super) fn scan(bytes: &[u8]) -> Result<OfficeScan, Refusal> {
    read(bytes).map(|read| read.scan)
}

/// The document with each body citation, in order, replaced by `citations`
/// and the reference list placed, after reading it again here.
pub(super) fn write(
    bytes: &[u8],
    citations: &[Vec<DisplayRun>],
    entries: &[DocumentEntry],
    hanging_indent: bool,
) -> Result<Vec<u8>, Refusal> {
    let read = read(bytes)?;
    if !read.scan.problems.is_empty() || read.spans.len() != citations.len() || citations.is_empty() {
        return Err(Refusal::Mismatch);
    }
    let edits = Edits {
        citations: read
            .spans
            .iter()
            .zip(citations)
            .map(|(&(paragraph, start, end), runs)| Edit {
                paragraph,
                start,
                end,
                runs: runs.clone(),
            })
            .collect(),
        bibliography: Bibliography {
            at: read.bibliography,
            entries: entries.to_vec(),
            hanging_indent,
        },
    };
    apply(bytes, read.scan.kind, &read.paragraphs, &edits)
}

/// `bytes` with `edits` applied, each checked against `paragraphs`, which
/// were read from those same bytes.
fn apply(bytes: &[u8], kind: Kind, paragraphs: &[Paragraph], edits: &Edits) -> Result<Vec<u8>, Refusal> {
    let plan = xml::Plan::new(paragraphs, edits)?;
    let mut archive = archive::Archive::open(bytes)?;
    match kind {
        Kind::Docx => docx::write(&mut archive, &plan),
        Kind::Odt => odt::write(&mut archive, &plan),
    }
}

/// [`apply`] after reading `bytes`: for tests of the writer alone.
#[cfg(test)]
fn write_edits(bytes: &[u8], edits: &Edits) -> Result<Vec<u8>, Refusal> {
    let (kind, paragraphs) = paragraphs(bytes)?;
    apply(bytes, kind, &paragraphs, edits)
}

#[cfg(test)]
#[path = "office_tests.rs"]
mod tests;

fn kind(archive: &mut archive::Archive<'_>) -> Result<Kind, Refusal> {
    if let Some(mimetype) = archive.text("mimetype")? {
        return if mimetype.trim() == "application/vnd.oasis.opendocument.text" {
            Ok(Kind::Odt)
        } else {
            Err(Refusal::Unsupported)
        };
    }
    if !archive.has("word/document.xml") {
        return Err(Refusal::Unsupported);
    }
    let types = archive.text("[Content_Types].xml")?.ok_or(Refusal::Unsupported)?;
    // A macro-enabled document or a template would not open under .docx.
    if archive.has("word/vbaProject.bin")
        || !types.contains("application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml")
    {
        return Err(Refusal::Unsupported);
    }
    Ok(Kind::Docx)
}
