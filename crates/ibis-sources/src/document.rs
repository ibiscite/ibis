//! A person's document for the document pipeline (decision 0060). The parent
//! reads the file once under the local-file rules (a regular local file,
//! never a link, network or placeholder file, within a size bound) and hands
//! its bytes to `ibis __document`, a short-lived child started through the
//! shared runner, with no path or environment. Only the child parses: its
//! readers exist only where the `documents` feature is on, which only the
//! command line enables. Answers are untrusted and checked here again.

use crate::zotero_local::closed::{ClosedError, with_local_file};
use ibis_core::contract::{MAX_TEXT_BYTES, Manuscript, TextKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// The argument that makes `ibis` the document child.
pub const CHILD_ARGUMENT: &str = "__document";
/// How long opening and reading a document may take.
pub const READ_BUDGET: Duration = Duration::from_secs(10);
/// How long one request to the child may take.
pub const DEADLINE: Duration = Duration::from_secs(30);
/// The most citations, and of references in one citation, an answer may hold.
pub const MAX_CITATIONS: usize = 100_000;
const MAX_KEY_BYTES: usize = 64 * 1024;
#[cfg(feature = "documents")]
const MAX_COMMAND_BYTES: u64 = 4096;
const MAX_ANSWER_BYTES: u64 = 4 * MAX_TEXT_BYTES as u64;

/// Why a document was not read: fixed reasons, never its text or path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Refusal {
    /// Nothing is at the path.
    Missing,
    /// Not a plain file on this computer.
    NotLocal,
    /// Reading did not finish in time, or was cancelled.
    TimedOut,
    /// A kind of document Ibis does not read.
    Unsupported,
    /// Over a size or count bound.
    TooLarge,
    /// Text that is not UTF-8.
    NotText,
    /// Not a valid file of its kind.
    Damaged,
    /// The document reader could not be started.
    Unavailable,
}

/// The bytes of the document at `path`, at most `max` of them.
pub fn read(path: &Path, max: u64, cancelled: Arc<dyn Fn() -> bool + Send + Sync>) -> Result<Vec<u8>, Refusal> {
    let read = with_local_file(path, cancelled, READ_BUDGET, move |file| {
        let mut bytes = Vec::new();
        file.take(max + 1).read_to_end(&mut bytes).map(|_| bytes)
    })
    .map_err(|error| match error {
        ClosedError::NotFound => Refusal::Missing,
        ClosedError::TimedOut | ClosedError::Cancelled => Refusal::TimedOut,
        _ => Refusal::NotLocal,
    })?
    .ok_or(Refusal::Missing)?
    .map_err(|_| Refusal::NotLocal)?;
    if read.len() as u64 > max {
        return Err(Refusal::TooLarge);
    }
    Ok(read)
}

/// The document at `path` as text: UTF-8, a leading byte-order mark dropped.
pub fn read_text(path: &Path, max: u64, cancelled: Arc<dyn Fn() -> bool + Send + Sync>) -> Result<String, Refusal> {
    let bytes = read(path, max, cancelled)?;
    let mut text = String::from_utf8(bytes).map_err(|_| Refusal::NotText)?;
    if text.starts_with('\u{feff}') {
        text.drain(..'\u{feff}'.len_utf8());
    }
    Ok(text)
}

/// What the parent asks of the child.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum Command {
    /// Read the citations of a manuscript of this kind.
    Scan { kind: TextKind },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum Answer {
    Scanned(Manuscript),
    Refused(Refusal),
}

/// The request on the child's standard input: the command's length as eight
/// little-endian bytes, the command as JSON, then the document's bytes.
fn frame(command: &Command, document: &[u8]) -> Result<Vec<u8>, Refusal> {
    let json = serde_json::to_vec(command).map_err(|_| Refusal::Damaged)?;
    let mut framed = Vec::with_capacity(8 + json.len() + document.len());
    framed.extend_from_slice(&(json.len() as u64).to_le_bytes());
    framed.extend_from_slice(&json);
    framed.extend_from_slice(document);
    Ok(framed)
}

/// An answer within every bound, with places that can be in `text`.
fn checked(manuscript: Manuscript, text: &str) -> Result<Manuscript, Refusal> {
    let lines = text.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let fits = |place: &ibis_core::contract::Place| {
        (1..=lines).contains(&place.line) && place.column >= 1 && place.column <= text.len() + 1
    };
    let references: usize = manuscript.citations.iter().map(|cited| cited.cites.len()).sum();
    if manuscript.citations.len() > MAX_CITATIONS
        || references > MAX_CITATIONS
        || manuscript.problems.len() > MAX_CITATIONS
    {
        return Err(Refusal::TooLarge);
    }
    let sound = manuscript.citations.iter().all(|cited| {
        fits(&cited.place)
            && !cited.cites.is_empty()
            && cited
                .cites
                .iter()
                .all(|cite| !cite.key.is_empty() && cite.key.len() <= MAX_KEY_BYTES.min(text.len()))
    }) && manuscript.problems.iter().all(|problem| fits(&problem.place));
    if !sound {
        return Err(Refusal::Damaged);
    }
    Ok(manuscript)
}

/// Reads documents through `ibis __document` (decisions 0055, 0056, 0060).
#[derive(Clone)]
pub struct DocumentReader {
    how: How,
}

#[derive(Clone)]
enum How {
    Child(PathBuf),
    /// Tests and the child itself: the readers in this process.
    #[cfg(feature = "documents")]
    InProcess,
}

impl std::fmt::Debug for DocumentReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DocumentReader { private }")
    }
}

impl DocumentReader {
    /// Start `program` (an `ibis` executable) as the child for each document.
    pub fn new(program: PathBuf) -> Self {
        Self {
            how: How::Child(program),
        }
    }

    /// Read in this process, for tests: never for a person's file in the app.
    #[cfg(feature = "documents")]
    pub fn in_process() -> Self {
        Self { how: How::InProcess }
    }

    /// The citations of `text`, a manuscript of `kind`.
    pub fn scan_text(&self, kind: TextKind, text: &str, cancelled: &dyn Fn() -> bool) -> Result<Manuscript, Refusal> {
        if text.len() > MAX_TEXT_BYTES {
            return Err(Refusal::TooLarge);
        }
        let command = Command::Scan { kind };
        let answer = match &self.how {
            How::Child(program) => self.exchange(program, &command, text.as_bytes(), cancelled)?,
            #[cfg(feature = "documents")]
            How::InProcess => child::answer(&command, text.as_bytes(), cancelled),
        };
        match answer {
            Answer::Scanned(manuscript) => checked(manuscript, text),
            // These refusals belong to the parent, never the child.
            Answer::Refused(Refusal::Missing | Refusal::NotLocal | Refusal::Unavailable | Refusal::NotText) => {
                Err(Refusal::Damaged)
            }
            Answer::Refused(refusal) => Err(refusal),
        }
    }

    fn exchange(
        &self,
        program: &Path,
        command: &Command,
        document: &[u8],
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Answer, Refusal> {
        let framed = frame(command, document)?;
        let mut process = std::process::Command::new(program);
        process.arg(CHILD_ARGUMENT);
        let output = crate::child::run(
            process,
            &framed,
            crate::child::Limits {
                time: DEADLINE,
                stdout: MAX_ANSWER_BYTES,
                stderr: None,
            },
            cancelled,
        )
        .map_err(|failure| match failure {
            crate::child::ChildError::TimedOut => Refusal::TimedOut,
            crate::child::ChildError::TooLarge => Refusal::TooLarge,
            crate::child::ChildError::Unavailable => Refusal::Unavailable,
            crate::child::ChildError::Failed => Refusal::Damaged,
        })?;
        if !output.status.success() {
            return Err(Refusal::Damaged);
        }
        serde_json::from_slice(&output.stdout).map_err(|_| Refusal::Damaged)
    }
}

/// The child's side: only where the `documents` feature is on.
#[cfg(feature = "documents")]
pub mod child {
    use super::*;
    use std::io::Write;

    /// Serve one framed request from `input`, answering on `output`.
    /// Returns the process's exit status.
    pub fn serve(mut input: impl Read, mut output: impl Write) -> u8 {
        let mut length = [0_u8; 8];
        if input.read_exact(&mut length).is_err() {
            return 2;
        }
        let length = u64::from_le_bytes(length);
        if length > MAX_COMMAND_BYTES {
            return 2;
        }
        let mut command = vec![0; length as usize];
        if input.read_exact(&mut command).is_err() {
            return 2;
        }
        let Ok(command) = serde_json::from_slice::<Command>(&command) else {
            return 2;
        };
        let mut document = Vec::new();
        if input
            .take(MAX_TEXT_BYTES as u64 + 1)
            .read_to_end(&mut document)
            .is_err()
        {
            return 2;
        }
        let answer = answer(&command, &document, &|| false);
        match serde_json::to_writer(&mut output, &answer) {
            Ok(()) if output.flush().is_ok() => 0,
            _ => 70,
        }
    }

    /// The answer to one request, from the document's bytes.
    pub(super) fn answer(command: &Command, document: &[u8], stop: &dyn Fn() -> bool) -> Answer {
        if document.len() > MAX_TEXT_BYTES {
            return Answer::Refused(Refusal::TooLarge);
        }
        match command {
            Command::Scan { kind } => match std::str::from_utf8(document) {
                Ok(text) => Answer::Scanned(ibis_core::syntax::manuscript::scan(*kind, text, stop)),
                Err(_) => Answer::Refused(Refusal::Damaged),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn never() -> Arc<dyn Fn() -> bool + Send + Sync> {
        Arc::new(|| false)
    }

    #[test]
    fn a_document_is_read_within_its_bound_and_as_text() {
        let folder = tempfile::tempdir().unwrap();
        // Resolve the test root, including macOS's /var -> /private/var alias.
        let root = std::fs::canonicalize(folder.path()).unwrap();
        let path = root.join("draft.md");
        std::fs::write(&path, "\u{feff}Lanterns [@vale2024].").unwrap();
        assert_eq!(read_text(&path, 64, never()).unwrap(), "Lanterns [@vale2024].");
        // Windows canonicalization produces a verbatim drive path.
        assert_eq!(
            read_text(&path.canonicalize().unwrap(), 64, never()).unwrap(),
            "Lanterns [@vale2024]."
        );
        assert_eq!(read(&path, 4, never()), Err(Refusal::TooLarge));
        std::fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(read_text(&path, 64, never()), Err(Refusal::NotText));
        assert_eq!(read(&root.join("gone.md"), 64, never()), Err(Refusal::Missing));
    }

    #[cfg(unix)]
    #[test]
    fn a_document_with_a_symlinked_parent_is_refused() {
        let folder = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(folder.path()).unwrap();
        let real = root.join("real");
        std::fs::create_dir(&real).unwrap();
        std::fs::write(real.join("draft.md"), "Lanterns [@vale2024].").unwrap();
        assert_eq!(
            read_text(&real.join("draft.md"), 64, never()).unwrap(),
            "Lanterns [@vale2024]."
        );
        let link = root.join("linked");
        std::os::unix::fs::symlink("real", &link).unwrap();
        assert_eq!(read_text(&link.join("draft.md"), 64, never()), Err(Refusal::NotLocal));
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_not_read() {
        let folder = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(folder.path()).unwrap();
        let path = root.join("draft.md");
        std::fs::write(&path, "text").unwrap();
        let link = root.join("link.md");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert_eq!(read(&link, 64, never()), Err(Refusal::NotLocal));
    }

    #[test]
    fn an_answer_that_cannot_come_from_the_text_is_refused() {
        use ibis_core::contract::{Cite, Cited, Place};
        let text = "As [@vale2024] says.";
        let cited = |key: &str, line| Cited {
            place: Place { line, column: 4 },
            narrative: false,
            cites: vec![Cite {
                key: key.into(),
                ..Cite::default()
            }],
        };
        let answer = |citations| Manuscript {
            citations,
            ..Manuscript::default()
        };
        assert!(checked(answer(vec![cited("vale2024", 1)]), text).is_ok());
        assert_eq!(
            checked(answer(vec![cited(&"k".repeat(64), 1)]), text),
            Err(Refusal::Damaged)
        );
        assert_eq!(checked(answer(vec![cited("vale2024", 9)]), text), Err(Refusal::Damaged));
        assert_eq!(checked(answer(vec![cited("", 1)]), text), Err(Refusal::Damaged));
    }

    #[test]
    fn a_missing_child_is_unavailable() {
        let folder = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(folder.path()).unwrap();
        let reader = DocumentReader::new(root.join("no-such-ibis"));
        assert_eq!(
            reader.scan_text(TextKind::Markdown, "[@vale2024]", &|| false),
            Err(Refusal::Unavailable)
        );
    }

    #[cfg(feature = "documents")]
    #[test]
    fn the_child_answers_one_framed_request() {
        let request = frame(&Command::Scan { kind: TextKind::Latex }, b"\\cite{vale2024}").unwrap();
        let mut answer = Vec::new();
        assert_eq!(child::serve(&request[..], &mut answer), 0);
        let Answer::Scanned(manuscript) = serde_json::from_slice(&answer).unwrap() else {
            panic!()
        };
        assert_eq!(manuscript.citations[0].cites[0].key, "vale2024");
        assert_eq!(
            child::serve(&b"\x00\x00\x00\x00\x00\x10\x00\x00"[..], &mut Vec::new()),
            2
        );
    }
}
