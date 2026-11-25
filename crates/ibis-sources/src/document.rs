//! A person's document, read once for the document pipeline (decision 0060):
//! a regular local file, never a link, network or placeholder file, within a
//! size bound. Plain-text manuscripts are read in place by `ibis-core`'s
//! bounded readers; office containers go to the office child.

use crate::zotero_local::closed::{ClosedError, with_local_file};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// How long opening and reading a document may take.
pub const READ_BUDGET: Duration = Duration::from_secs(10);

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
    /// Over the size bound.
    TooLarge,
    /// Text that is not UTF-8.
    NotText,
    /// Not a valid file of its kind.
    Damaged,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn never() -> Arc<dyn Fn() -> bool + Send + Sync> {
        Arc::new(|| false)
    }

    #[test]
    fn a_document_is_read_within_its_bound_and_as_text() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("draft.md");
        std::fs::write(&path, "\u{feff}Lanterns [@vale2024].").unwrap();
        assert_eq!(read_text(&path, 64, never()).unwrap(), "Lanterns [@vale2024].");
        assert_eq!(read(&path, 4, never()), Err(Refusal::TooLarge));
        std::fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(read_text(&path, 64, never()), Err(Refusal::NotText));
        assert_eq!(read(&folder.path().join("gone.md"), 64, never()), Err(Refusal::Missing));
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_not_read() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("draft.md");
        std::fs::write(&path, "text").unwrap();
        let link = folder.path().join("link.md");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert_eq!(read(&link, 64, never()), Err(Refusal::NotLocal));
    }
}
