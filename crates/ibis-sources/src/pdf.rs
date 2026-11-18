//! A PDF's text and printed page labels, read by a short-lived child process
//! (decisions 0036 and 0055).
//!
//! The parent opens the file under the reader's rules: a regular file on a
//! local disk, never a link, network or cloud placeholder file, at most
//! 256 MiB, with `%PDF-` in its first 1,024 bytes. The opened file becomes the
//! child's standard input; the child gets no other handle, no environment and
//! no path, parses the bytes and answers with JSON on standard output. It is
//! stopped after ten seconds. Its answer is untrusted: page counts, text size
//! and every label are checked again here. Text stays in memory.
//!
//! Only `ibis` runs the child (`ibis __extract-pdf`), never the desktop app,
//! so PDF parsing stays away from the app's library, clipboard and paste
//! permissions.

use ibis_core::contract::Fingerprint;
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

// SAFETY: only the post-fork descriptor boundary needs unsafe calls.
#[cfg(unix)]
#[allow(unsafe_code)]
#[path = "pdf/inheritance.rs"]
mod inheritance;

/// The argument that makes `ibis` the extractor's child process.
pub const CHILD_ARGUMENT: &str = "__extract-pdf";
/// After [`CHILD_ARGUMENT`]: give page labels and the page count, not text.
pub const LABELS_ONLY: &str = "--labels-only";
/// The largest file read.
pub const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
/// The most pages a file read may have.
pub const MAX_PAGES: u32 = 10_000;
/// The most text, in bytes, one file may give.
pub const MAX_TEXT_BYTES: usize = crate::zotero_local::fulltext::MAX_TEXT_BYTES;
/// How long one extraction may take.
pub const DEADLINE: Duration = Duration::from_secs(10);
/// `%PDF-` must start within this many bytes.
pub const MARK_WITHIN: usize = 1024;
/// The most characters in a citable page label (decision 0036).
pub const MAX_LABEL_CHARS: usize = 16;
// Text JSON-escaped, at most twice its size once control characters are gone,
// and room for labels.
const MAX_ANSWER_BYTES: u64 = 2 * MAX_TEXT_BYTES as u64 + 4 * 1024 * 1024;

/// The child's answer.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum Answer {
    Read(Extracted),
    Refused(Refusal),
}

/// What a PDF holds, page by page from the first.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extracted {
    pub pages: u32,
    /// Each page's label as the file's `/PageLabels` give it, or `None` when
    /// the file has no label tree. A page the tree leaves out is `None`.
    pub labels: Option<Vec<Option<String>>>,
    /// Each page's text, or `None` when only labels were asked for.
    pub text: Option<Vec<String>>,
}
impl std::fmt::Debug for Extracted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Extracted {{ pages: {}, private }}", self.pages)
    }
}

/// Why a file's text was not read: fixed reasons, never a path or text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Refusal {
    /// Nothing is at the attachment's location.
    Missing,
    /// Not a plain file on this computer: a link, a folder or device, a file
    /// on a network or FUSE mount, an online-only placeholder, or one this
    /// account cannot open.
    NotLocal,
    /// The file does not start as a PDF does.
    NotPdf,
    /// Over 256 MiB, over 10,000 pages, or over 16 MiB of text.
    TooLarge,
    /// The PDF could not be parsed.
    Damaged,
    /// Reading took longer than ten seconds.
    TimedOut,
    /// Ibis could not start its PDF reader.
    Unavailable,
}

/// A PDF's pages, checked: text and labels have one entry per page, and each
/// label is citable or `None`.
#[derive(Clone, PartialEq, Eq)]
pub struct PdfDocument {
    /// Check of the verified answer and, for a file read, its bytes.
    pub revision: Fingerprint,
    pub pages: u32,
    /// Printed labels, when the file has a label tree.
    pub labels: Option<Vec<Option<String>>>,
    pub text: Option<Vec<String>>,
}
impl std::fmt::Debug for PdfDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PdfDocument {{ pages: {}, private }}", self.pages)
    }
}

/// Starts the extractor: `program` is an `ibis` executable.
#[derive(Clone)]
pub struct PdfReader {
    program: PathBuf,
    deadline: Duration,
}
impl std::fmt::Debug for PdfReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PdfReader { private }")
    }
}

impl PdfReader {
    pub fn new(program: PathBuf) -> Self {
        Self {
            program,
            deadline: DEADLINE,
        }
    }

    /// The same reader with a shorter deadline, for tests.
    #[cfg(all(test, unix))]
    fn with_deadline(mut self, deadline: Duration) -> Self {
        self.deadline = deadline.min(DEADLINE);
        self
    }

    /// The PDF at `path`, with its text unless `labels_only`.
    pub fn read(
        &self,
        path: &Path,
        labels_only: bool,
        cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<PdfDocument, Refusal> {
        use crate::zotero_local::closed::{ClosedError, with_local_file};
        let started = Instant::now();
        let reader = self.clone();
        let stop = cancelled.clone();
        with_local_file(path, cancelled, self.deadline, move |mut file| {
            let before = stamp(&mut file)?;
            let answer = reader.run(
                file.try_clone().map_err(|_| Refusal::NotLocal)?,
                labels_only,
                started,
                &*stop,
            )?;
            let mut document = match answer {
                Answer::Read(extracted) => checked(extracted, labels_only)?,
                // These refusals belong to the parent, never the child.
                Answer::Refused(Refusal::Missing | Refusal::NotLocal | Refusal::TimedOut | Refusal::Unavailable) => {
                    return Err(Refusal::Damaged);
                }
                Answer::Refused(refusal) => return Err(refusal),
            };
            // A file changed while parsing cannot publish text or labels.
            if stamp(&mut file)? != before {
                return Err(Refusal::NotLocal);
            }
            document.revision = Fingerprint::of_bytes(format!("{before:?}:{}", document.revision.as_str()).as_bytes());
            Ok(document)
        })
        .map_err(|error| match error {
            ClosedError::NotFound => Refusal::Missing,
            ClosedError::TimedOut | ClosedError::Cancelled => Refusal::TimedOut,
            _ => Refusal::NotLocal,
        })?
        .ok_or(Refusal::Missing)?
    }

    fn run(
        &self,
        file: std::fs::File,
        labels_only: bool,
        started: Instant,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Answer, Refusal> {
        if cancelled() || started.elapsed() >= self.deadline {
            return Err(Refusal::TimedOut);
        }
        let mut command = Command::new(&self.program);
        command.arg(CHILD_ARGUMENT);
        if labels_only {
            command.arg(LABELS_ONLY);
        }
        // No environment: the child needs none of the person's settings,
        // tokens or paths. Windows needs its system folder to start a process.
        command.env_clear();
        #[cfg(windows)]
        if let Some(root) = std::env::var_os("SystemRoot") {
            command.env("SystemRoot", root);
        }
        // Also close descriptors a native caller left inheritable. On Windows
        // std::process uses PROC_THREAD_ATTRIBUTE_HANDLE_LIST for stdio, so
        // other inheritable handles are excluded by CreateProcess itself.
        #[cfg(unix)]
        inheritance::restrict(&mut command).map_err(|_| Refusal::Unavailable)?;
        let mut child = command
            .stdin(Stdio::from(file))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| Refusal::Unavailable)?;
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Refusal::Unavailable);
        };
        // Read while waiting, so a full pipe cannot stall the child.
        let reader = std::thread::Builder::new()
            .name("ibis-pdf-answer".into())
            .spawn(move || {
                let mut bytes = Vec::new();
                stdout.take(MAX_ANSWER_BYTES + 1).read_to_end(&mut bytes).map(|_| bytes)
            });
        let Ok(reader) = reader else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Refusal::Unavailable);
        };
        let deadline = started + self.deadline;
        let status = loop {
            if cancelled() || Instant::now() >= deadline {
                break Err(Refusal::TimedOut);
            }
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Err(_) => break Err(Refusal::Damaged),
            }
        };
        if status.is_err() {
            let _ = child.kill();
            let _ = child.wait();
        }
        // The child has exited, so its pipe is closed and the reader ends.
        let bytes = reader.join().ok().and_then(Result::ok);
        if !status?.success() {
            return Err(Refusal::Damaged);
        }
        let bytes = bytes.ok_or(Refusal::Damaged)?;
        if bytes.len() as u64 > MAX_ANSWER_BYTES {
            return Err(Refusal::TooLarge);
        }
        serde_json::from_slice(&bytes).map_err(|_| Refusal::Damaged)
    }
}

/// A PDF opened under the reader's rules, to be copied whole: a regular,
/// resident local file reached without following a link, at most 256 MiB,
/// starting as a PDF does.
pub struct LocalPdf {
    file: std::fs::File,
    stamp: PdfStamp,
}
impl std::fmt::Debug for LocalPdf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LocalPdf {{ bytes: {}, private }}", self.stamp.bytes)
    }
}

impl LocalPdf {
    /// Open the PDF at `path` as [`PdfReader::read`] opens it.
    pub fn open(path: &Path, cancelled: Arc<dyn Fn() -> bool + Send + Sync>) -> Result<Self, Refusal> {
        use crate::zotero_local::closed::{ClosedError, with_local_file};
        with_local_file(path, cancelled, DEADLINE, |mut file| {
            let stamp = stamp(&mut file)?;
            Ok(Self { file, stamp })
        })
        .map_err(|error| match error {
            ClosedError::NotFound => Refusal::Missing,
            ClosedError::TimedOut | ClosedError::Cancelled => Refusal::TimedOut,
            _ => Refusal::NotLocal,
        })?
        .ok_or(Refusal::Missing)?
    }

    /// Identity and version to compare if the path is reopened.
    pub fn stamp(&self) -> PdfStamp {
        self.stamp
    }

    /// Copy the bytes it held when opened to `out`; a file that changed
    /// meanwhile is refused, and `out` may then hold part of it.
    pub fn copy_to(mut self, out: &mut impl std::io::Write) -> Result<u64, Refusal> {
        self.file.rewind().map_err(|_| Refusal::NotLocal)?;
        let copied = std::io::copy(&mut (&mut self.file).take(self.stamp.bytes), out).map_err(|_| Refusal::NotLocal)?;
        if copied != self.stamp.bytes || stamp(&mut self.file)? != self.stamp {
            return Err(Refusal::NotLocal);
        }
        Ok(copied)
    }
}

/// Identity, size and modification time from a validated PDF handle.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PdfStamp {
    identity: [u64; 2],
    bytes: u64,
    modified: std::time::SystemTime,
}

impl PdfStamp {
    /// The PDF's byte length when opened.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}

/// Verify the open file as a PDF within the size bound.
#[allow(
    clippy::disallowed_methods,
    reason = "Opened-handle checks run inside the bounded local-file I/O owner."
)]
fn stamp(file: &mut std::fs::File) -> Result<PdfStamp, Refusal> {
    let metadata = file.metadata().map_err(|_| Refusal::NotLocal)?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err(Refusal::TooLarge);
    }
    let mut start = Vec::with_capacity(MARK_WITHIN);
    file.rewind().map_err(|_| Refusal::NotLocal)?;
    (&mut *file)
        .take(MARK_WITHIN as u64)
        .read_to_end(&mut start)
        .map_err(|_| Refusal::NotLocal)?;
    file.rewind().map_err(|_| Refusal::NotLocal)?;
    if !has_mark(&start) {
        return Err(Refusal::NotPdf);
    }
    let identity = crate::zotero_local::closed::unlinked_file_identity(file, &metadata).ok_or(Refusal::NotLocal)?;
    let modified = metadata.modified().map_err(|_| Refusal::NotLocal)?;
    Ok(PdfStamp {
        identity,
        bytes: metadata.len(),
        modified,
    })
}

/// Whether `%PDF-` starts within the first [`MARK_WITHIN`] bytes.
pub fn has_mark(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(MARK_WITHIN)]
        .windows(5)
        .any(|window| window == b"%PDF-")
}

/// The child's answer once it holds what was asked, within every bound.
pub fn checked(extracted: Extracted, labels_only: bool) -> Result<PdfDocument, Refusal> {
    let pages = extracted.pages;
    let count = usize::try_from(pages).map_err(|_| Refusal::TooLarge)?;
    if pages > MAX_PAGES {
        return Err(Refusal::TooLarge);
    }
    if pages == 0
        || extracted.labels.as_ref().is_some_and(|labels| labels.len() != count)
        || extracted.text.as_ref().is_some_and(|text| text.len() != count)
        || extracted.text.is_some() == labels_only
    {
        return Err(Refusal::Damaged);
    }
    if extracted
        .text
        .iter()
        .flatten()
        .try_fold(0usize, |total, page| total.checked_add(page.len()))
        .is_none_or(|total| total > MAX_TEXT_BYTES)
    {
        return Err(Refusal::TooLarge);
    }
    let revision = Fingerprint::of_bytes(&serde_json::to_vec(&extracted).map_err(|_| Refusal::Damaged)?);
    Ok(PdfDocument {
        revision,
        pages,
        // A tree with no citable label says nothing a citation can use.
        labels: extracted
            .labels
            .map(citable_labels)
            .filter(|labels| labels.iter().any(Option::is_some)),
        text: extracted.text,
    })
}

/// Labels a citation may carry: each must be citable, and a label two pages
/// share names neither (decision 0036 asks rather than choose).
fn citable_labels(labels: Vec<Option<String>>) -> Vec<Option<String>> {
    let labels: Vec<Option<String>> = labels
        .into_iter()
        .map(|label| label.and_then(|label| citable_label(&label)))
        .collect();
    let mut seen = std::collections::BTreeMap::<&str, usize>::new();
    for label in labels.iter().flatten() {
        *seen.entry(label).or_default() += 1;
    }
    let repeated: std::collections::BTreeSet<String> = seen
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(label, _)| label.to_owned())
        .collect();
    labels
        .into_iter()
        .map(|label| label.filter(|label| !repeated.contains(label)))
        .collect()
}

/// `label` in NFC when a citation may carry it: 1 to 16 letters, digits,
/// `-` or `.` (decision 0036). Anything else is unknown.
pub fn citable_label(label: &str) -> Option<String> {
    use unicode_normalization::UnicodeNormalization;
    let label: String = label.nfc().collect();
    let count = label.chars().count();
    ((1..=MAX_LABEL_CHARS).contains(&count)
        && label
            .chars()
            .all(|character| character.is_alphanumeric() || matches!(character, '-' | '.')))
    .then_some(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extracted(pages: u32, labels: Option<Vec<Option<&str>>>, text: Option<Vec<&str>>) -> Extracted {
        Extracted {
            pages,
            labels: labels.map(|labels| labels.into_iter().map(|label| label.map(str::to_owned)).collect()),
            text: text.map(|text| text.into_iter().map(str::to_owned).collect()),
        }
    }

    #[test]
    fn an_answer_that_breaks_a_bound_is_refused() {
        assert_eq!(checked(extracted(0, None, Some(vec![])), false), Err(Refusal::Damaged));
        assert_eq!(
            checked(extracted(MAX_PAGES + 1, None, None), true),
            Err(Refusal::TooLarge)
        );
        assert_eq!(
            checked(extracted(2, None, Some(vec!["one"])), false),
            Err(Refusal::Damaged)
        );
        assert_eq!(
            checked(extracted(2, Some(vec![Some("1")]), None), true),
            Err(Refusal::Damaged)
        );
        // Text where only labels were asked for, or none where text was.
        assert_eq!(
            checked(extracted(1, None, Some(vec!["one"])), true),
            Err(Refusal::Damaged)
        );
        assert_eq!(checked(extracted(1, None, None), false), Err(Refusal::Damaged));
        let large = "x".repeat(MAX_TEXT_BYTES / 2 + 1);
        assert_eq!(
            checked(extracted(2, None, Some(vec![&large, &large])), false),
            Err(Refusal::TooLarge)
        );
        assert!(checked(extracted(1, None, Some(vec!["one"])), false).is_ok());
    }

    #[test]
    fn files_that_are_not_plain_local_pdfs_are_refused_before_any_child_starts() {
        let reader = PdfReader::new(PathBuf::from("/invented/no-such-ibis"));
        let folder = tempfile::tempdir().unwrap();
        // The test's own folder may sit under a system link (macOS /var).
        let root = crate::zotero_local::closed::trusted_folder(folder.path());
        let path = |name: &str| root.join(name);
        assert_eq!(
            reader.read(&path("absent.pdf"), false, Arc::new(|| false)),
            Err(Refusal::Missing)
        );
        std::fs::write(path("notes.pdf"), "Invented plain text, not a PDF.").unwrap();
        assert_eq!(
            reader.read(&path("notes.pdf"), false, Arc::new(|| false)),
            Err(Refusal::NotPdf)
        );
        std::fs::create_dir(path("folder.pdf")).unwrap();
        assert_eq!(
            reader.read(&path("folder.pdf"), false, Arc::new(|| false)),
            Err(Refusal::NotLocal)
        );
        let large = std::fs::File::create(path("large.pdf")).unwrap();
        large.set_len(MAX_FILE_BYTES + 1).unwrap();
        assert_eq!(
            reader.read(&path("large.pdf"), false, Arc::new(|| false)),
            Err(Refusal::TooLarge)
        );
        #[cfg(unix)]
        {
            std::fs::write(path("real.pdf"), "%PDF-1.7\n").unwrap();
            std::os::unix::fs::symlink(path("real.pdf"), path("link.pdf")).unwrap();
            assert_eq!(
                reader.read(&path("link.pdf"), false, Arc::new(|| false)),
                Err(Refusal::NotLocal)
            );
        }
        #[cfg(unix)]
        {
            let directory = path("nested");
            std::fs::create_dir(&directory).unwrap();
            std::fs::write(directory.join("real.pdf"), "%PDF-1.7\n").unwrap();
            let link = path("linked-directory");
            std::os::unix::fs::symlink(directory, &link).unwrap();
            assert_eq!(
                reader.read(&link.join("real.pdf"), false, Arc::new(|| false)),
                Err(Refusal::NotLocal)
            );
            // A real parent-directory component remains usable.
            assert_eq!(
                reader.read(&path("nested/../real.pdf"), false, Arc::new(|| false)),
                Err(Refusal::Unavailable)
            );
            // No link may be hidden by a following parent component.
            assert_eq!(
                reader.read(&link.join("../real.pdf"), false, Arc::new(|| false)),
                Err(Refusal::NotLocal)
            );
        }
        // A real PDF start reaches the child, which is missing here.
        std::fs::write(path("start.pdf"), "%PDF-1.7\n").unwrap();
        assert_eq!(
            reader.read(&path("start.pdf"), false, Arc::new(|| false)),
            Err(Refusal::Unavailable)
        );
    }

    #[cfg(unix)]
    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "Polls only the invented child fixture's ready marker."
    )]
    fn cancelling_a_running_pdf_child_releases_the_caller() {
        use std::os::unix::fs::PermissionsExt;
        let folder = tempfile::tempdir().unwrap();
        let root = crate::zotero_local::closed::trusted_folder(folder.path());
        let program = root.join("cancelled-ibis");
        // The child signals entry, then stays busy until cancellation kills it.
        std::fs::write(&program, "#!/bin/sh\n: > \"$0.ready\"\nexec sleep 30\n").unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let file = root.join("invented.pdf");
        std::fs::write(&file, "%PDF-1.7\n").unwrap();
        let started = Instant::now();
        let ready = program.with_file_name("cancelled-ibis.ready");
        let reader = PdfReader::new(program);
        let stop = Arc::new(move || ready.exists());
        assert_eq!(reader.read(&file, false, stop), Err(Refusal::TimedOut));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[cfg(unix)]
    #[test]
    fn a_child_past_its_deadline_is_stopped() {
        use std::os::unix::fs::PermissionsExt;
        let folder = tempfile::tempdir().unwrap();
        let root = crate::zotero_local::closed::trusted_folder(folder.path());
        let program = root.join("slow-ibis");
        std::fs::write(&program, "#!/bin/sh\nexec sleep 30\n").unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let file = root.join("invented.pdf");
        std::fs::write(&file, "%PDF-1.7\n").unwrap();
        let started = Instant::now();
        let reader = PdfReader::new(program).with_deadline(Duration::from_millis(300));
        assert_eq!(reader.read(&file, false, Arc::new(|| false)), Err(Refusal::TimedOut));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[cfg(unix)]
    #[test]
    fn a_pdf_child_cannot_read_an_inheritable_descriptor_from_the_caller() {
        use nix::fcntl::{FcntlArg, FdFlag, fcntl};
        use std::os::{fd::AsRawFd, unix::fs::PermissionsExt};
        let folder = tempfile::tempdir().unwrap();
        let root = crate::zotero_local::closed::trusted_folder(folder.path());
        let private = tempfile::tempfile().unwrap();
        // Simulate a descriptor a native caller opened without close-on-exec.
        fcntl(&private, FcntlArg::F_SETFD(FdFlag::empty())).unwrap();
        let program = root.join("isolated-ibis");
        std::fs::write(
            &program,
            format!(
                "#!/bin/sh\nif [ -r /dev/fd/{} ]; then text=leaked; else text=isolated; fi\n\
                 printf '{{\"read\":{{\"pages\":1,\"labels\":null,\"text\":[\"%s\"]}}}}' \"$text\"\n",
                private.as_raw_fd()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let file = root.join("invented.pdf");
        std::fs::write(&file, "%PDF-1.7\n").unwrap();
        let document = PdfReader::new(program).read(&file, false, Arc::new(|| false)).unwrap();
        assert_eq!(document.text, Some(vec!["isolated".into()]));
    }
}
