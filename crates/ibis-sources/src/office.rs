//! Word (`.docx`) and OpenDocument (`.odt`) text for `ibis format`, read
//! and written by a short-lived child process, `ibis __office` (decisions
//! 0055 and 0056: every new parser of untrusted files runs apart).
//!
//! The parent opens the person's file under the reader's rules (a regular
//! local file, at most 64 MiB) and sends the child one framed request on
//! standard input: the command as JSON, then the document's bytes. The child
//! gets no path, environment or other handle. `scan` answers with each
//! paragraph's text; `write` answers with a new document carrying the
//! replacements, which the child checks against its own reading of the same
//! bytes. Answers are untrusted and checked again here.

use ibis_core::contract::{DisplayRun, DocumentEntry};
use serde::{Deserialize, Serialize};

/// The largest document read.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// The argument that makes `ibis` the office child.
pub const CHILD_ARGUMENT: &str = "__office";
/// The most text one document may hold, in bytes.
pub const MAX_TEXT_BYTES: usize = 64 * 1024 * 1024;
/// The most paragraphs one document may hold.
pub const MAX_PARAGRAPHS: usize = 1_000_000;
/// The largest command the parent sends.
pub const MAX_COMMAND_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Docx,
    Odt,
}

/// Where a paragraph is. Only the body, tables included, is formatted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Part {
    Body,
    /// A footnote or endnote.
    Note,
    /// A header or footer.
    Header,
    Comment,
    /// A text box, frame or shape.
    TextBox,
    /// Text the document does not show, such as tracked deletions.
    Hidden,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paragraph {
    pub part: Part,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scanned {
    pub kind: Kind,
    pub paragraphs: Vec<Paragraph>,
}

/// Replace `start..end` (bytes of the paragraph's text) with `runs`, in the
/// formatting of the text it replaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    pub paragraph: usize,
    pub start: usize,
    pub end: usize,
    pub runs: Vec<DisplayRun>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bibliography {
    /// The body paragraph that reads `[bibliography]`, replaced by the
    /// entries; without one, they follow the body's last paragraph.
    pub at: Option<usize>,
    pub entries: Vec<DocumentEntry>,
    pub hanging_indent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edits {
    /// In document order, none overlapping.
    pub citations: Vec<Edit>,
    pub bibliography: Bibliography,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum Command {
    Scan,
    Write(Edits),
}

/// Why a document was not read or written: fixed reasons, never its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Refusal {
    /// Nothing is at the path.
    Missing,
    /// Not a plain file on this computer.
    NotLocal,
    /// The child did not answer in time.
    TimedOut,
    /// The child could not be started.
    Unavailable,
    /// Neither a Word nor an OpenDocument text file.
    NotOffice,
    /// A kind Ibis does not write: with macros, encrypted, or not a text document.
    Unsupported,
    /// Its zip archive or XML is damaged.
    Damaged,
    /// Over a size, entry, depth or text bound.
    TooLarge,
    /// Entry names that leave the archive's folder, repeated names, or a
    /// document type declaration.
    Unsafe,
    /// The replacements do not fit this document's text.
    Mismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum Answer {
    Scanned(Scanned),
    /// The new document's length; its bytes follow the answer's line.
    Written(u64),
    Refused(Refusal),
}

/// The request the parent writes on the child's standard input.
pub fn frame(command: &Command, document: &[u8]) -> Result<Vec<u8>, serde_json::Error> {
    let json = serde_json::to_vec(command)?;
    let mut framed = Vec::with_capacity(8 + json.len() + document.len());
    framed.extend_from_slice(&(json.len() as u64).to_le_bytes());
    framed.extend_from_slice(&json);
    framed.extend_from_slice(document);
    Ok(framed)
}

/// The child's answer to `scan`, within every bound.
pub fn checked(scanned: Scanned) -> Result<Scanned, Refusal> {
    let text: usize = scanned.paragraphs.iter().map(|paragraph| paragraph.text.len()).sum();
    if scanned.paragraphs.len() > MAX_PARAGRAPHS || text > MAX_TEXT_BYTES {
        return Err(Refusal::TooLarge);
    }
    Ok(scanned)
}
