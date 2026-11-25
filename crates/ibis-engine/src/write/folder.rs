//! Keeping a folder's PDFs (decision 0061): the one automatic write Ibis
//! makes outside a proposal, and only when the person keeps the folder. Each
//! PDF no entry names yet gets one appended entry in the folder's
//! `ibis-references.bib`, holding only what the PDF said about itself. No
//! entry is edited or removed and no PDF is touched. The append goes through
//! the proposal writer: its cross-process lock, a refusal if the file changed
//! since it was read, an atomic replace and backups in Ibis's data folder.

use super::bib;
use super::{Proposals, Target, WriteError, WriteProblem};
use crate::{Cancellation, Engine};
use ibis_core::citable::CitableFields;
use ibis_sources::bibtex_edit::BibFile;
use ibis_sources::folder::{LIBRARY_FILE, file_value, list, unkept};
use ibis_sources::pdf::{PdfDetails, PdfReader, Refusal};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

/// PDFs read and appended in one write.
pub const KEEP_BATCH: usize = 100;

/// What one keep did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Kept {
    /// Entries appended, one per PDF.
    pub kept: usize,
    /// Of those, PDFs Ibis could not read details from: their entries hold
    /// only the file.
    pub unread: usize,
    /// PDFs to try again at the next keep, folder-relative: they timed out,
    /// were online-only, locked or gone, or the reader could not start.
    pub retried: Vec<String>,
    /// PDFs past this batch, kept by the next batch.
    pub waiting: usize,
    /// The folder held more than one listing reads.
    pub cut_short: bool,
}

impl Engine {
    /// Keep up to [`KEEP_BATCH`] of `folder`'s unkept PDFs in its
    /// `ibis-references.bib`, creating the file if it is missing. PDFs are
    /// read by the engine's PDF reader; without one nothing is kept. PDFs in
    /// `skip` (those an earlier batch of the same keep must try again later)
    /// are passed over, so they never hold up the PDFs after them.
    pub fn keep_folder(
        &self,
        store: &Proposals,
        folder: &Path,
        skip: &BTreeSet<String>,
        cancel: &Cancellation,
    ) -> Result<Kept, WriteError> {
        let reader: &PdfReader = self.pdf_reader().ok_or(WriteProblem::Unreadable)?;
        let _lock = store.lock()?;
        // PDFs are opened by the path as given, which the reader's local-file
        // checks take on every system; the write goes to the resolved folder,
        // which must still be the same folder when it is replaced.
        let given = std::path::absolute(folder).map_err(|_| WriteProblem::Unreadable)?;
        let resolved = std::fs::canonicalize(&given).map_err(|_| WriteProblem::Unreadable)?;
        let identity = FolderIdentity::of(&resolved).ok_or(WriteProblem::Unreadable)?;
        let path = resolved.join(LIBRARY_FILE);
        let before = state(&path)?;
        let text = before.clone().unwrap_or_default();
        let file = BibFile::read(&text).map_err(bib::problem)?;
        let listing = list(&resolved, &|| cancel.is_cancelled()).map_err(|_| WriteProblem::Unreadable)?;
        let named = ibis_sources::folder::kept_files(&text);
        let waiting: Vec<String> = unkept(&listing, &named)
            .into_iter()
            .filter(|relative| !skip.contains(relative))
            .collect();
        let mut report = Kept {
            cut_short: listing.cut_short,
            ..Kept::default()
        };
        let mut taken = BTreeSet::new();
        let mut entries = Vec::new();
        let mut attempted = 0;
        for relative in waiting.iter().take(KEEP_BATCH) {
            attempted += 1;
            let stop = cancel.clone();
            let details = match reader.details(&given.join(relative), Arc::new(move || stop.is_cancelled())) {
                Ok(details) => Some(details),
                // Settled: this file will never say more.
                Err(Refusal::Damaged | Refusal::NotPdf | Refusal::TooLarge) => None,
                Err(_) => {
                    report.retried.push(relative.clone());
                    continue;
                }
            };
            if cancel.is_cancelled() {
                return Err(WriteProblem::NotWritten.into());
            }
            let fields = details.as_ref().map(csl).unwrap_or_default();
            let key = super::citation_key(&CitableFields::new(key_fields(&fields, relative)), |key| {
                file.key_taken(key) || taken.contains(&key.to_lowercase())
            })
            .ok_or(WriteProblem::KeyTaken)?;
            taken.insert(key.to_lowercase());
            entries.push(entry(&key, fields, relative)?);
            report.unread += usize::from(details.is_none());
        }
        report.waiting = waiting.len() - attempted;
        report.kept = entries.len();
        if entries.is_empty() || cancel.is_cancelled() {
            return if cancel.is_cancelled() {
                Err(WriteProblem::NotWritten.into())
            } else {
                Ok(report)
            };
        }
        let mut after = text.clone();
        for entry in &entries {
            after = BibFile::read(&after).map_err(bib::problem)?.append(entry);
        }
        let target = Target {
            library: ibis_sources::folder::library_id(&path),
            path: path.clone(),
            resolved: path.clone(),
        };
        // Nothing may have changed: not the file (absent stays absent, a read
        // failure is a change), not the folder (never swapped for a link or
        // another folder), and the keep not cancelled.
        let check = || {
            if cancel.is_cancelled() {
                return Err(WriteProblem::Changed);
            }
            if FolderIdentity::of(&resolved).as_ref() != Some(&identity)
                || std::fs::canonicalize(&given).ok().as_ref() != Some(&resolved)
            {
                return Err(WriteProblem::Changed);
            }
            match state(&path) {
                Ok(now) if now == before => Ok(()),
                _ => Err(WriteProblem::Changed),
            }
        };
        store.replace_library(&target, text.as_bytes(), after.as_bytes(), check)?;
        // The file watch would see it too; reading now shows the entries at once.
        let _ = self.reload_files(None, &Cancellation::default());
        Ok(report)
    }
}

/// The library file's text, or `None` when there is none; any other failure
/// to read it is an error, never taken for absence.
fn state(path: &Path) -> Result<Option<String>, WriteError> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(WriteProblem::Unreadable.into()),
        Ok(_) => bib::read(path).map(Some),
    }
}

/// A folder as the file system knows it, not by its name: a real folder,
/// never a link, with its device and inode where the system has them.
#[derive(Debug, PartialEq, Eq)]
struct FolderIdentity(u64, u64);
impl FolderIdentity {
    fn of(path: &Path) -> Option<Self> {
        let metadata = std::fs::symlink_metadata(path).ok()?;
        if !metadata.is_dir() {
            return None;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Some(Self(metadata.dev(), metadata.ino()))
        }
        #[cfg(not(unix))]
        {
            Some(Self(0, 0))
        }
    }
}

/// What a PDF said, as CSL fields: only what it stated.
fn csl(details: &PdfDetails) -> Map<String, Value> {
    let mut fields = Map::new();
    let kind = if details.container.is_some() {
        "article-journal"
    } else {
        "document"
    };
    fields.insert("type".into(), kind.into());
    let mut text = |name: &str, value: &Option<String>| {
        if let Some(value) = value {
            fields.insert(name.into(), value.clone().into());
        }
    };
    text("title", &details.title);
    text("container-title", &details.container);
    text("publisher", &details.publisher);
    text("volume", &details.volume);
    text("issue", &details.issue);
    text("page", &details.page);
    text("DOI", &details.doi);
    if !details.authors.is_empty() {
        fields.insert(
            "author".into(),
            details.authors.iter().map(|name| person(name)).collect(),
        );
    }
    if let Some(issued) = &details.issued {
        let parts: Vec<Value> = issued
            .split('-')
            .filter_map(|part| part.parse::<u32>().ok())
            .map(Value::from)
            .collect();
        fields.insert("issued".into(), json!({ "date-parts": [parts] }));
    }
    fields
}

/// A name as the PDF wrote it, in CSL's parts: `Family, Given`, or the last
/// word as the family name; one word stays whole.
fn person(name: &str) -> Value {
    if let Some((family, given)) = name.split_once(',') {
        return json!({ "family": family.trim(), "given": given.trim() });
    }
    match name.rsplit_once(' ') {
        Some((given, family)) => json!({ "family": family, "given": given }),
        None => json!({ "literal": name }),
    }
}

/// The fields a key is made from: the PDF's own, or, when it says nothing a
/// key uses, the words of its file name, for the key alone.
fn key_fields(fields: &Map<String, Value>, relative: &str) -> Map<String, Value> {
    if ["title", "author", "issued"]
        .iter()
        .any(|name| fields.contains_key(*name))
    {
        return fields.clone();
    }
    let stem = Path::new(relative)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("document")
        .replace(['_', '-', '.'], " ");
    let mut named = Map::new();
    named.insert("type".into(), "document".into());
    named.insert("title".into(), stem.into());
    named
}

/// The entry for the PDF at `relative`: Ibis's BibTeX writer over its
/// fields, with the `file` field first.
fn entry(key: &str, mut fields: Map<String, Value>, relative: &str) -> Result<String, WriteError> {
    fields.entry("type").or_insert_with(|| "document".into());
    let text = bib::entry_text(key, &fields)?;
    let (head, rest) = text.split_once('\n').ok_or(WriteProblem::Invalid)?;
    Ok(format!("{head}\n  file = {{{}}},\n{rest}", file_value(relative)))
}

#[cfg(test)]
#[path = "folder_tests.rs"]
mod tests;
