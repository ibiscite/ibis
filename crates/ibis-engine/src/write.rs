//! Library changes an agent proposes and the person confirms in Ibis
//! (decisions 0051 and 0055).
//!
//! An agent never writes. `ibis mcp --allow-writes` records what it proposes,
//! one private file per proposal beside the app's settings, and no library
//! changes. The person reads the preview in Ibis and applies or rejects it
//! there. [`Engine::apply_proposal`] is for the app's confirmation screen
//! only: it is never an MCP tool, an `ibis` command or a generated method.
//!
//! A proposal adds a reference from CSL-JSON fields, sets fields of one
//! reference, or adds and removes its tags (BibTeX `keywords`), in one
//! library. Every field goes through the share-safe filter first, so a
//! proposal never carries notes, files or private links.
//!
//! Only `.bib` libraries take proposals for now; Zotero waits for its official
//! local write API (0051). Applying refuses when the entry changed since the
//! proposal or the file is reached through a link, edits only that entry's
//! text or adds the new entry at the end, keeps every other byte, writes
//! atomically and keeps the file it replaced as `refs.bib.ibis-backup-N`.
//! Applies run one at a time, across Ibis processes.

mod bib;
mod key;
mod store;
#[cfg(test)]
mod tests;

pub use key::citation_key;
pub use store::{MAX_KEPT, MAX_PENDING, MAX_PROPOSAL_BYTES, Proposals};

use crate::request::{ItemSelector, ShowRequest};
use crate::{Cancellation, Engine, LibraryKind};
use ibis_core::citable::CitableFields;
use ibis_core::contract::{Fingerprint, ItemRef, LibraryId, LibraryObject};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::PathBuf;

/// Version of the proposal file format this build reads and writes.
pub const FORMAT: u32 = 1;
/// Tags one proposal adds and removes, together.
pub const MAX_TAGS: usize = 20;
/// Characters in one tag.
pub const MAX_TAG_CHARS: usize = 64;
/// Characters kept of a reference's title to name it in a preview.
const LABEL_CHARS: usize = 120;

/// Who proposed a change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub via: Via,
    /// The name the client gave itself; unchecked, so shown as a claim.
    pub client: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Via {
    Mcp,
}

/// The one `.bib` library a proposal changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub library: LibraryId,
    pub path: PathBuf,
}

impl Target {
    /// How a person knows the library: its file name.
    pub fn name(&self) -> String {
        self.path.file_name().map_or_else(
            || "the .bib file".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        )
    }
}

/// What a proposal changes. `record` names a reference in the target: its
/// citation key in a `.bib` file, its item key in Zotero.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Change {
    /// A new reference from CSL-JSON fields, under the citation key Ibis made.
    Add { key: String, fields: Map<String, Value> },
    /// New values for CSL-JSON fields of one reference.
    Fields {
        record: String,
        label: String,
        fields: Map<String, Value>,
    },
    /// Tags to add to and remove from one reference.
    Tags {
        record: String,
        label: String,
        add: Vec<String>,
        remove: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Pending,
    Rejected,
    Applied,
}

/// One proposal as its file holds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    /// [`FORMAT`].
    pub ibis_proposal: u32,
    /// Sixteen lowercase hexadecimal digits.
    pub id: String,
    pub created_ms: u64,
    pub source: Source,
    pub target: Target,
    pub change: Change,
    /// The reference as it was when proposed: its `.bib` entry text, or its
    /// Zotero fields. Absent for a new reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<Fingerprint>,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_ms: Option<u64>,
}

/// What a person reads before applying: the change in words and, for a
/// `.bib` file, the entry exactly as it is and as it would be written.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Preview {
    pub proposal: Proposal,
    /// The library's file name, or Zotero's library.
    pub library: String,
    /// One sentence on what applying does.
    pub summary: String,
    /// The change field by field.
    pub lines: Vec<String>,
    /// The entry as the file holds it now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// The entry as applying would write it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    /// Why applying would refuse now; absent when it can apply.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<WriteProblem>,
}

/// A change made: the proposal as now saved and the backup of the file.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Applied {
    pub proposal: Proposal,
    pub backup: PathBuf,
}

/// Why a proposal was not recorded, shown or applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum WriteProblem {
    /// No proposal has this id.
    UnknownProposal,
    /// A proposal file is damaged, from a newer Ibis or not a file.
    ProposalUnreadable,
    /// The proposal could not be saved.
    NotSaved,
    /// [`MAX_PENDING`] proposals wait already.
    TooMany,
    /// The proposal would be larger than [`MAX_PROPOSAL_BYTES`].
    TooLarge,
    /// The proposal was applied or rejected already.
    Decided,
    /// A field, tag or reference in the proposal cannot be used.
    Invalid,
    /// Several libraries are open; the proposal must name one.
    NoTarget,
    /// The reference is not in an open library.
    UnknownReference,
    /// Ibis has no safe way to write this kind of library.
    LibraryKind,
    /// A reference with this DOI is in the library already.
    AlreadyInLibrary,
    /// The library is not open in Ibis.
    NotOpen,
    /// The reference changed since the proposal.
    Changed,
    /// Another entry took the new entry's citation key.
    KeyTaken,
    /// Some entry in the file cannot be read, so nothing is edited.
    Damaged,
    /// Two entries in the file have the reference's key.
    Repeated,
    /// A field to change uses a macro or joins parts.
    NotLiteral,
    /// The library file cannot be read as text.
    Unreadable,
    /// The backup could not be written; nothing changed.
    BackupFailed,
    /// The file could not be written; nothing changed.
    NotWritten,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}", self.sentence())]
pub struct WriteError {
    pub problem: WriteProblem,
    /// The field or tag the problem is about, as the proposal named it.
    pub field: Option<String>,
}

impl From<WriteProblem> for WriteError {
    fn from(problem: WriteProblem) -> Self {
        Self { problem, field: None }
    }
}

impl WriteError {
    fn about(problem: WriteProblem, field: &str) -> Self {
        Self {
            problem,
            field: Some(field.chars().filter(|c| !c.is_control()).take(64).collect()),
        }
    }

    /// What happened and what to do, in words for a person or an agent.
    pub fn sentence(&self) -> String {
        let field = self.field.as_deref().unwrap_or("a field");
        match self.problem {
            WriteProblem::UnknownProposal => "No proposal has this id; `ibis proposals list` shows them.".into(),
            WriteProblem::ProposalUnreadable => {
                "This proposal's file cannot be read; reject it and propose again.".into()
            }
            WriteProblem::NotSaved => "The proposal could not be saved beside Ibis's settings.".into(),
            WriteProblem::TooMany => {
                format!("{MAX_PENDING} proposals are waiting already; the person can review or reject them in Ibis.")
            }
            WriteProblem::TooLarge => format!("A proposal holds at most {} KiB.", MAX_PROPOSAL_BYTES / 1024),
            WriteProblem::Decided => "This proposal was applied or rejected already.".into(),
            WriteProblem::Invalid => format!("`{field}` cannot be used in a proposal."),
            WriteProblem::NoTarget => "Several libraries are open: name the one to add to.".into(),
            WriteProblem::UnknownReference => "The reference is not in an open library; search again.".into(),
            WriteProblem::LibraryKind => "Ibis can propose changes to .bib files only, for now.".into(),
            WriteProblem::AlreadyInLibrary => "A reference with this DOI is in the library already.".into(),
            WriteProblem::NotOpen => "The library is not open in Ibis.".into(),
            WriteProblem::Changed => "The reference changed since this was proposed; nothing was written.".into(),
            WriteProblem::KeyTaken => "Another entry now uses this citation key; nothing was written.".into(),
            WriteProblem::Damaged => "Some entry in the file cannot be read, so Ibis leaves the file alone.".into(),
            WriteProblem::Repeated => "Two entries in the file have this citation key.".into(),
            WriteProblem::NotLiteral => {
                format!("`{field}` uses a @string macro or joins values, so Ibis does not edit it.")
            }
            WriteProblem::Unreadable => "The library file cannot be read as text.".into(),
            WriteProblem::BackupFailed => "A backup of the file could not be written; nothing was changed.".into(),
            WriteProblem::NotWritten => "The file could not be written; nothing was changed.".into(),
        }
    }
}

/// What an agent asks to change.
#[derive(Debug, Clone, PartialEq)]
pub enum ProposeRequest {
    /// Add a reference to `library`, or to the only open library.
    Add {
        library: Option<LibraryId>,
        fields: Map<String, Value>,
    },
    Fields {
        item: ItemRef,
        fields: Map<String, Value>,
    },
    Tags {
        item: ItemRef,
        add: Vec<String>,
        remove: Vec<String>,
    },
}

impl Engine {
    /// Record what an agent proposes. No library changes.
    pub fn propose(&self, store: &Proposals, source: Source, request: ProposeRequest) -> Result<Proposal, WriteError> {
        let snapshot = self.snapshot().map_err(|_| WriteProblem::UnknownReference)?;
        let library = match &request {
            ProposeRequest::Add {
                library: Some(library), ..
            } => library.clone(),
            ProposeRequest::Add { library: None, .. } => {
                let open = self.libraries().map_err(|_| WriteProblem::NoTarget)?.libraries;
                match open.as_slice() {
                    [only] => only.id.clone(),
                    _ => return Err(WriteProblem::NoTarget.into()),
                }
            }
            ProposeRequest::Fields { item, .. } | ProposeRequest::Tags { item, .. } => item.library.clone(),
        };
        let target = self.proposal_target(&library)?;
        let record = match &request {
            ProposeRequest::Add { .. } => None,
            ProposeRequest::Fields { item, .. } | ProposeRequest::Tags { item, .. } => {
                let found = self
                    .show(&ShowRequest {
                        item: ItemSelector::Id(item.clone()),
                    })
                    .map_err(|_| WriteProblem::UnknownReference)?;
                let LibraryObject::Record(record) = &found.object else {
                    return Err(WriteProblem::UnknownReference.into());
                };
                Some(record.clone())
            }
        };
        let text = bib::read(&target.path)?;
        let file = ibis_sources::bibtex_edit::BibFile::read(&text).map_err(bib::problem)?;
        let (change, fingerprint) = match (request, record) {
            (ProposeRequest::Add { fields, .. }, _) => {
                let citable = citable(&fields, &["id", "citation-key"])?;
                if !citable
                    .plain()
                    .get("title")
                    .and_then(Value::as_str)
                    .is_some_and(|title| !title.trim().is_empty())
                {
                    return Err(WriteError::about(WriteProblem::Invalid, "title"));
                }
                if let Some(doi) = citable.doi() {
                    let doi = doi.identifier().to_lowercase();
                    let held = snapshot.objects().iter().any(|object| {
                        matches!(&object.object, LibraryObject::Record(record)
                            if record.id.library == library
                                && record.csl.to_map().get("DOI").and_then(Value::as_str)
                                    .is_some_and(|held| held.to_lowercase() == doi))
                    });
                    if held {
                        return Err(WriteProblem::AlreadyInLibrary.into());
                    }
                }
                let pending = store.list()?;
                let proposed = |key: &str| {
                    pending.iter().any(|other| {
                        other.status == Status::Pending
                            && other.target.library == library
                            && matches!(&other.change, Change::Add { key: other, .. } if other.eq_ignore_ascii_case(key))
                    })
                };
                let key =
                    citation_key(&citable, |key| file.key_taken(key) || proposed(key)).ok_or(WriteProblem::KeyTaken)?;
                (Change::Add { key, fields }, None)
            }
            (ProposeRequest::Fields { fields, .. }, Some(record)) => {
                if fields.is_empty() {
                    return Err(WriteError::about(WriteProblem::Invalid, "fields"));
                }
                citable(&fields, &["id", "citation-key", "type", "keyword"])?;
                let key = bib_key(&record)?;
                let entry = file.entry(&key).map_err(bib::problem)?;
                bib::field_changes(entry, &fields)?;
                let fingerprint = Fingerprint::of_bytes(entry.text().as_bytes());
                let label = label(&record);
                (
                    Change::Fields {
                        record: key,
                        label,
                        fields,
                    },
                    Some(fingerprint),
                )
            }
            (ProposeRequest::Tags { add, remove, .. }, Some(record)) => {
                let (add, remove) = tags(add, remove)?;
                let key = bib_key(&record)?;
                let entry = file.entry(&key).map_err(bib::problem)?;
                bib::keywords(entry, &add, &remove)?;
                let fingerprint = Fingerprint::of_bytes(entry.text().as_bytes());
                let label = label(&record);
                (
                    Change::Tags {
                        record: key,
                        label,
                        add,
                        remove,
                    },
                    Some(fingerprint),
                )
            }
            _ => return Err(WriteProblem::UnknownReference.into()),
        };
        let source = Source {
            via: source.via,
            client: client_name(&source.client),
        };
        store.create(source, target, change, fingerprint)
    }

    /// Apply a pending proposal to its `.bib` file, which must be open in
    /// this engine. For the app's confirmation screen, after the person has
    /// read [`Proposals::show`]'s preview and confirmed it there.
    pub fn apply_proposal(&self, store: &Proposals, id: &str) -> Result<Applied, WriteError> {
        // One apply at a time, also across Ibis processes.
        let _lock = store.lock()?;
        let proposal = store.get(id)?;
        if proposal.status != Status::Pending {
            return Err(WriteProblem::Decided.into());
        }
        let path = &proposal.target.path;
        // Only a library the person has open, at the path proposed.
        let open = self
            .libraries()
            .map_err(|_| WriteProblem::NotOpen)?
            .libraries
            .into_iter()
            .filter(|info| info.kind == LibraryKind::File)
            .find(|info| self.file_path(&info.id).ok().flatten().as_deref() == Some(path.as_path()))
            .ok_or(WriteProblem::NotOpen)?;
        // A linked folder could point the write somewhere else.
        if std::fs::canonicalize(path).ok().as_deref() != Some(path.as_path()) {
            return Err(WriteProblem::NotOpen.into());
        }
        let text = bib::read(path)?;
        let plan = bib::plan(&text, &proposal)?;
        let check = || match bib::read(path) {
            Ok(latest) if latest == text => Ok(()),
            _ => Err(WriteProblem::Changed),
        };
        use ibis_core::atomic_file::{BackupReplaceError, replace_keeping_backup};
        let backup = match replace_keeping_backup(path, Some(text.as_bytes()), plan.text.as_bytes(), check) {
            Ok(Some(backup)) => backup,
            Ok(None) => return Err(WriteProblem::NotWritten.into()),
            Err(BackupReplaceError::Backup) => return Err(WriteProblem::BackupFailed.into()),
            Err(BackupReplaceError::Check(problem) | BackupReplaceError::BackupLeft(problem)) => {
                return Err(problem.into());
            }
            Err(BackupReplaceError::Stage(_) | BackupReplaceError::Write(_)) => {
                return Err(WriteProblem::NotWritten.into());
            }
        };
        let proposal = store.decide(id, Status::Applied)?;
        // The file watch would see it too; reading now shows the change at once.
        let _ = self.reload_files(Some(&open.id), &Cancellation::default());
        Ok(Applied { proposal, backup })
    }

    pub(crate) fn proposal_target(&self, library: &LibraryId) -> Result<Target, WriteError> {
        let info = self
            .libraries()
            .map_err(|_| WriteProblem::UnknownReference)?
            .libraries
            .into_iter()
            .find(|info| &info.id == library)
            .ok_or(WriteProblem::UnknownReference)?;
        let path = (info.kind == LibraryKind::File)
            .then(|| self.file_path(library).ok().flatten())
            .flatten()
            .filter(|path| bib::is_bib(path))
            .ok_or(WriteProblem::LibraryKind)?;
        Ok(Target { library: info.id, path })
    }
}

impl Proposals {
    /// The proposal with its preview, read against the library as it is now.
    pub fn show(&self, id: &str) -> Result<Preview, WriteError> {
        Ok(preview(self.get(id)?))
    }

    /// Mark a pending proposal rejected. Nothing else changes.
    pub fn reject(&self, id: &str) -> Result<Proposal, WriteError> {
        let _lock = self.lock()?;
        if self.get(id)?.status != Status::Pending {
            return Err(WriteProblem::Decided.into());
        }
        self.decide(id, Status::Rejected)
    }
}

fn preview(proposal: Proposal) -> Preview {
    let mut preview = Preview {
        library: proposal.target.name(),
        summary: summary(&proposal),
        lines: lines(&proposal.change),
        before: None,
        after: None,
        refusal: None,
        proposal,
    };
    match bib::read(&preview.proposal.target.path).and_then(|text| bib::plan(&text, &preview.proposal)) {
        Ok(plan) => {
            preview.before = plan.before;
            preview.after = Some(plan.after);
        }
        Err(error) => preview.refusal = Some(error.problem),
    }
    if preview.proposal.status != Status::Pending {
        preview.refusal = Some(WriteProblem::Decided);
    }
    preview
}

/// One sentence on what applying a proposal does, naming no file.
pub fn summary(proposal: &Proposal) -> String {
    match &proposal.change {
        Change::Add { key, fields } => format!(
            "Add “{}” as {key}.",
            fields.get("title").map(display).unwrap_or_default()
        ),
        Change::Fields { label, fields, .. } => {
            let names: Vec<&str> = fields.keys().map(String::as_str).collect();
            format!("Change {} of “{label}”.", names.join(", "))
        }
        Change::Tags { label, add, remove, .. } => {
            let mut parts = Vec::new();
            if !add.is_empty() {
                parts.push(format!("add the tags {}", add.join(", ")));
            }
            if !remove.is_empty() {
                parts.push(format!("remove the tags {}", remove.join(", ")));
            }
            let mut sentence = parts.join(" and ");
            if let Some(first) = sentence.get(..1) {
                sentence = first.to_uppercase() + &sentence[1..];
            }
            format!("{sentence} on “{label}”.")
        }
    }
}

/// The change field by field, as a person reads it.
fn lines(change: &Change) -> Vec<String> {
    match change {
        Change::Add { fields, .. } | Change::Fields { fields, .. } => fields
            .iter()
            .map(|(name, value)| format!("{name}: {}", display(value)))
            .collect(),
        Change::Tags { add, remove, .. } => add
            .iter()
            .map(|tag| format!("+ {tag}"))
            .chain(remove.iter().map(|tag| format!("- {tag}")))
            .collect(),
    }
}

/// A value on one line, with nothing hidden.
fn display(value: &Value) -> String {
    let text = match value {
        Value::String(text) => text.clone(),
        Value::Array(names) => names
            .iter()
            .map(|name| {
                let part = |part: &str| name.get(part).and_then(Value::as_str).unwrap_or_default();
                match (part("literal"), part("family"), part("given")) {
                    (literal, _, _) if !literal.is_empty() => literal.to_owned(),
                    (_, family, "") => family.to_owned(),
                    (_, family, given) => format!("{family}, {given}"),
                }
            })
            .collect::<Vec<_>>()
            .join("; "),
        Value::Object(date) if date.contains_key("date-parts") => date["date-parts"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|parts| {
                parts
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|part| part.to_string().trim_matches('"').to_owned())
                    .collect::<Vec<_>>()
                    .join("-")
            })
            .collect::<Vec<_>>()
            .join("/"),
        Value::Object(date) => date
            .get("literal")
            .or_else(|| date.get("raw"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        other => other.to_string(),
    };
    let visible: String = text.chars().filter(|c| !ibis_core::model::is_hidden(*c)).collect();
    visible.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The fields through the share-safe filter, refusing any it would drop or
/// that a proposal may not set, by name.
fn citable(fields: &Map<String, Value>, refused: &[&str]) -> Result<CitableFields, WriteError> {
    if let Some(name) = fields.keys().find(|name| refused.contains(&name.as_str())) {
        return Err(WriteError::about(WriteProblem::Invalid, name));
    }
    let citable = CitableFields::new(fields.clone());
    if let Some(name) = fields.keys().find(|name| !citable.plain().contains_key(*name)) {
        return Err(WriteError::about(WriteProblem::Invalid, name));
    }
    Ok(citable)
}

/// Tags to add and remove, trimmed, each once, none in both lists.
fn tags(add: Vec<String>, remove: Vec<String>) -> Result<(Vec<String>, Vec<String>), WriteError> {
    let clean = |tags: Vec<String>| -> Result<Vec<String>, WriteError> {
        let mut kept: Vec<String> = Vec::new();
        for tag in tags {
            let tag = tag.trim().to_owned();
            if tag.is_empty()
                || tag.chars().count() > MAX_TAG_CHARS
                || tag
                    .chars()
                    .any(|c| matches!(c, ',' | ';' | '{' | '}') || c.is_control() || ibis_core::model::is_hidden(c))
            {
                return Err(WriteError::about(WriteProblem::Invalid, &tag));
            }
            if !kept.iter().any(|kept| kept.to_lowercase() == tag.to_lowercase()) {
                kept.push(tag);
            }
        }
        Ok(kept)
    };
    let (add, remove) = (clean(add)?, clean(remove)?);
    if add.is_empty() && remove.is_empty() || add.len() + remove.len() > MAX_TAGS {
        return Err(WriteError::about(WriteProblem::Invalid, "tags"));
    }
    if let Some(both) = add
        .iter()
        .find(|tag| remove.iter().any(|other| other.to_lowercase() == tag.to_lowercase()))
    {
        return Err(WriteError::about(WriteProblem::Invalid, both));
    }
    Ok((add, remove))
}

fn bib_key(record: &ibis_core::contract::Record) -> Result<String, WriteError> {
    match record.citation_key() {
        Ok(Some(key)) => Ok(key.to_owned()),
        _ => Err(WriteProblem::UnknownReference.into()),
    }
}

/// The reference's title, short and on one line, to name it in a preview.
fn label(record: &ibis_core::contract::Record) -> String {
    let citable = CitableFields::new(record.csl.to_map());
    let title = citable.plain().get("title").map(display).unwrap_or_default();
    let title = ibis_core::model::strip_markup(&title);
    match title.char_indices().nth(LABEL_CHARS) {
        Some((end, _)) => format!("{}…", &title[..end]),
        None => title,
    }
}

/// The client's own name, short and on one line.
pub(crate) fn client_name(name: &str) -> String {
    let name = display(&Value::String(name.to_owned()));
    let name: String = name.chars().take(64).collect();
    if name.is_empty() {
        "an MCP client".to_owned()
    } else {
        name
    }
}
