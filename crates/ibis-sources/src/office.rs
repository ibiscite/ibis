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

/// The document at `path`: a regular local file, never a link, network or
/// placeholder file, at most [`MAX_FILE_BYTES`]. Read once; the child gets
/// these same bytes for both requests.
pub fn read(path: &std::path::Path, cancelled: std::sync::Arc<dyn Fn() -> bool + Send + Sync>) -> Result<Vec<u8>, Refusal> {
    use crate::zotero_local::closed::{ClosedError, with_local_file};
    use std::io::Read;
    let read = with_local_file(path, cancelled, std::time::Duration::from_secs(10), |file| {
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes).map(|_| bytes)
    })
    .map_err(|error| match error {
        ClosedError::NotFound => Refusal::Missing,
        ClosedError::TimedOut | ClosedError::Cancelled => Refusal::TimedOut,
        _ => Refusal::NotLocal,
    })?
    .ok_or(Refusal::Missing)?
    .map_err(|_| Refusal::NotLocal)?;
    if read.len() as u64 > MAX_FILE_BYTES {
        return Err(Refusal::TooLarge);
    }
    Ok(read)
}

/// Runs `ibis __office` for each request (decision 0055).
#[derive(Clone)]
pub struct OfficeReader {
    program: std::path::PathBuf,
}
impl std::fmt::Debug for OfficeReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OfficeReader { private }")
    }
}

impl OfficeReader {
    pub fn new(program: std::path::PathBuf) -> Self {
        Self { program }
    }

    /// Every paragraph of `document`, the body's first.
    pub fn scan(&self, document: &[u8], cancelled: &dyn Fn() -> bool) -> Result<Scanned, Refusal> {
        match self.exchange(&Command::Scan, document, cancelled)? {
            (Answer::Scanned(scanned), rest) if rest.is_empty() => checked(scanned),
            _ => Err(Refusal::Damaged),
        }
    }

    /// `document` with `edits` applied.
    pub fn write(&self, document: &[u8], edits: &Edits, cancelled: &dyn Fn() -> bool) -> Result<Vec<u8>, Refusal> {
        match self.exchange(&Command::Write(edits.clone()), document, cancelled)? {
            (Answer::Written(length), rest) if rest.len() as u64 == length => Ok(rest),
            _ => Err(Refusal::Damaged),
        }
    }

    /// One request and the child's answer line, then the bytes after it.
    fn exchange(&self, command: &Command, document: &[u8], cancelled: &dyn Fn() -> bool) -> Result<(Answer, Vec<u8>), Refusal> {
        let framed = frame(command, document).map_err(|_| Refusal::TooLarge)?;
        let answer = crate::child::run(
            &self.program,
            &[CHILD_ARGUMENT],
            crate::child::Input::Bytes(framed),
            DEADLINE,
            cancelled,
            MAX_ANSWER_BYTES,
        )
        .map_err(|failure| match failure {
            crate::child::Failure::TimedOut => Refusal::TimedOut,
            crate::child::Failure::TooLarge => Refusal::TooLarge,
            crate::child::Failure::Unavailable => Refusal::Unavailable,
            crate::child::Failure::Failed => Refusal::Damaged,
        })?;
        let line = answer.iter().position(|byte| *byte == b'\n').ok_or(Refusal::Damaged)?;
        let parsed: Answer = serde_json::from_slice(&answer[..line]).map_err(|_| Refusal::Damaged)?;
        let rest = answer[line + 1..].to_vec();
        match parsed {
            // These refusals belong to the parent, never the child.
            Answer::Refused(Refusal::Missing | Refusal::NotLocal | Refusal::TimedOut | Refusal::Unavailable) => {
                Err(Refusal::Damaged)
            }
            Answer::Refused(refusal) => Err(refusal),
            answer => Ok((answer, rest)),
        }
    }
}

/// How long one request may take.
pub const DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);
// The new document, with room for its answer line.
const MAX_ANSWER_BYTES: u64 = 2 * MAX_FILE_BYTES + MAX_TEXT_BYTES as u64 + 1024 * 1024;
