//! A proposal as `.bib` text: new entries come from Ibis's own BibTeX writer,
//! and changes replace only the values of the fields they name.
use super::{Change, Proposal, WriteError, WriteProblem};
use ibis_core::citable::CitableFields;
use ibis_core::contract::Fingerprint;
use ibis_sources::bibtex_edit::{BibFile, EditProblem, Entry, written_fields};
use serde_json::{Map, Value};
use std::io::Read;
use std::path::Path;

/// What applying writes: the entry before and after, and the whole file.
pub(super) struct Plan {
    pub before: Option<String>,
    pub after: String,
    pub text: String,
}

#[cfg(test)]
thread_local! { pub(super) static READS: std::cell::RefCell<Vec<std::path::PathBuf>> = const { std::cell::RefCell::new(Vec::new()) }; }

/// Read regular UTF-8 text within the library size bound, I/O pool and deadline.
/// Refuse links: replacement would put a file where the link was.
pub(super) fn read(path: &Path) -> Result<String, WriteError> {
    #[cfg(test)]
    READS.with_borrow_mut(|reads| reads.push(path.to_path_buf()));
    let path = path.to_path_buf();
    ibis_sources::inspect_io(move || read_sync(&path)).ok_or(WriteProblem::Unreadable)?
}

fn read_sync(path: &Path) -> Result<String, WriteError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| WriteProblem::Unreadable)?;
    if !metadata.is_file() || metadata.len() > ibis_sources::MAX_LIBRARY_BYTES {
        return Err(WriteProblem::Unreadable.into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(ibis_sources::MAX_LIBRARY_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|_| WriteProblem::Unreadable)?;
    if bytes.len() as u64 > ibis_sources::MAX_LIBRARY_BYTES {
        return Err(WriteProblem::Unreadable.into());
    }
    String::from_utf8(bytes).map_err(|_| WriteProblem::Unreadable.into())
}

pub(super) fn problem(problem: EditProblem) -> WriteError {
    match problem {
        EditProblem::Damaged => WriteProblem::Damaged,
        EditProblem::NotFound => WriteProblem::Changed,
        EditProblem::Repeated => WriteProblem::Repeated,
        EditProblem::NotLiteral => WriteProblem::NotLiteral,
    }
    .into()
}

/// The new entry's text, from the share-safe fields.
pub(super) fn entry_text(key: &str, fields: &Map<String, Value>) -> Result<String, WriteError> {
    ibis_core::syntax::bibtex_entry(key, &CitableFields::new(fields.clone()))
        .ok_or_else(|| WriteError::about(WriteProblem::Invalid, "key"))
}

/// What applying `proposal` to `text` would write, or why it would refuse.
pub(super) fn plan(text: &str, proposal: &Proposal) -> Result<Plan, WriteError> {
    let file = BibFile::read(text).map_err(problem)?;
    let (record, changes) = match &proposal.change {
        Change::Add { key, fields } => {
            if file.key_taken(key) {
                return Err(WriteProblem::KeyTaken.into());
            }
            let entry = entry_text(key, fields)?;
            return Ok(Plan {
                before: None,
                text: file.append(&entry),
                after: entry,
            });
        }
        Change::Fields { record, fields, .. } => {
            let entry = unchanged(&file, record, proposal.fingerprint.as_ref())?;
            (record, field_changes(entry, fields)?)
        }
        Change::Tags {
            record, add, remove, ..
        } => {
            let entry = unchanged(&file, record, proposal.fingerprint.as_ref())?;
            (record, vec![("keywords".to_owned(), keywords(entry, add, remove)?)])
        }
    };
    let entry = file.entry(record).map_err(problem)?;
    let changes: Vec<(&str, Option<String>)> = changes
        .iter()
        .map(|(name, value)| (name.as_str(), value.clone()))
        .collect();
    let edited = entry.edited(&changes);
    let after = BibFile::read(&edited)
        .and_then(|file| file.entry(record).map(|entry| entry.text().to_owned()))
        .map_err(problem)?;
    Ok(Plan {
        before: Some(entry.text().to_owned()),
        after,
        text: edited,
    })
}

/// The entry, as long as it is exactly what the proposal saw.
fn unchanged<'f, 'a>(
    file: &'f BibFile<'a>,
    key: &str,
    fingerprint: Option<&Fingerprint>,
) -> Result<&'f Entry<'a>, WriteError> {
    let entry = file.entry(key).map_err(problem)?;
    if fingerprint != Some(&Fingerprint::of_bytes(entry.text().as_bytes())) {
        return Err(WriteProblem::Changed.into());
    }
    Ok(entry)
}

/// The CSL type that makes the writer choose this entry's field names.
fn csl_type(kind: &str) -> &'static str {
    match kind {
        "article" => "article-journal",
        "incollection" | "inbook" => "chapter",
        "inproceedings" => "paper-conference",
        _ => "document",
    }
}

/// Each CSL field as the BibTeX fields that hold it, in the names this entry
/// already uses: `journal` for `journaltitle`, `year` for a year-only date.
pub(super) fn field_changes(
    entry: &Entry<'_>,
    fields: &Map<String, Value>,
) -> Result<Vec<(String, Option<String>)>, WriteError> {
    let mut changes: Vec<(String, Option<String>)> = Vec::new();
    for (name, value) in fields {
        let mut one = Map::new();
        one.insert("type".into(), Value::String(csl_type(&entry.kind).into()));
        one.insert(name.clone(), value.clone());
        let written = written(&one).filter(|written| !written.is_empty());
        let Some(written) = written else {
            return Err(WriteError::about(WriteProblem::Invalid, name));
        };
        for (bib, value) in written {
            let mut set = |name: &str, value: Option<String>| {
                changes.retain(|(other, _)| other != name);
                changes.push((name.to_owned(), value));
            };
            match bib.as_str() {
                "journaltitle" if !entry.has("journaltitle") && entry.has("journal") => set("journal", Some(value)),
                "location" if !entry.has("location") && entry.has("address") => set("address", Some(value)),
                "date" if !entry.has("date") && entry.has("year") => {
                    let year_only = value.len() == 6 && value[1..5].bytes().all(|byte| byte.is_ascii_digit());
                    if year_only {
                        set("year", Some(value));
                    } else {
                        set("date", Some(value));
                        set("year", None);
                    }
                    set("month", None);
                }
                "year" if entry.has("date") => {
                    set("year", Some(value));
                    set("date", None);
                }
                other => set(other, Some(value)),
            }
        }
    }
    Ok(changes)
}

/// The `keywords` value after adding and removing tags, or `None` when no
/// tag is left. Tags already written keep their spelling.
pub(super) fn keywords(entry: &Entry<'_>, add: &[String], remove: &[String]) -> Result<Option<String>, WriteError> {
    let existing = match entry.literal("keywords") {
        None => "",
        Some(Ok(text)) => text,
        Some(Err(_)) => return Err(WriteError::about(WriteProblem::NotLiteral, "keywords")),
    };
    let same = |written: &str, tag: &str| {
        let written = written.trim();
        let written = written
            .strip_prefix('{')
            .and_then(|inner| inner.strip_suffix('}'))
            .unwrap_or(written);
        written.to_lowercase() == tag.to_lowercase() || written.to_lowercase() == escaped(tag).to_lowercase()
    };
    let mut kept: Vec<String> = split(existing)
        .into_iter()
        .map(str::trim)
        .filter(|written| !written.is_empty())
        .filter(|written| !remove.iter().any(|tag| same(written, tag)))
        .map(str::to_owned)
        .collect();
    for tag in add {
        if !kept.iter().any(|written| same(written, tag)) {
            kept.push(escaped(tag));
        }
    }
    Ok((!kept.is_empty()).then(|| format!("{{{}}}", kept.join(", "))))
}

/// A tag as the writer escapes BibTeX text.
fn escaped(tag: &str) -> String {
    let mut one = Map::new();
    one.insert("keyword".into(), Value::String(tag.to_owned()));
    written(&one)
        .and_then(|written| written.into_iter().find(|(name, _)| name == "keywords"))
        .and_then(|(_, value)| Some(value.strip_prefix('{')?.strip_suffix('}')?.to_owned()))
        .unwrap_or_default()
}

/// Commas outside braces separate keywords.
fn split(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0usize, 0);
    for (at, c) in text.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&text[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

/// The BibTeX fields the writer makes of `fields`, with their written values.
fn written(fields: &Map<String, Value>) -> Option<Vec<(String, String)>> {
    let entry = ibis_core::syntax::bibtex_entry("ibis", &CitableFields::new(fields.clone()))?;
    written_fields(&entry)
}
