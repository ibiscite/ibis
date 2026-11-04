//! A project `.bib` beside the libraries, never among them: its keys are
//! compared exactly as the citation would emit them, and it owns no records.
use super::*;
use std::path::{Path, PathBuf};

const MAX_KEYS: usize = 1000;

/// A request-local writing context, with the ordinary citation contract kept
/// intact. Paths are native input only; the check names just its bibliography.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WritingProjectCitation {
    pub id: String,
    pub name: String,
    pub revision: String,
    pub citation: CitationResult,
    pub project: Option<ProjectResult>,
}
impl WritingProjectCitation {
    pub fn schema() -> serde_json::Value {
        schemars::schema_for!(Self).into()
    }
}

/// One key against the chosen file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectKey {
    pub key: String,
    pub check: KeyCheck,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectResult {
    /// The file's name, never its folder.
    pub name: String,
    pub keys: Vec<ProjectKey>,
    /// Why this read could not be used; the answers then say "could not check".
    pub problem: Option<ProjectProblem>,
    /// Lines that could not be read, which is why nothing can be called missing.
    pub damaged_lines: Vec<usize>,
}
impl ProjectResult {
    /// Every key is in the file, as far as a check can say.
    pub fn settled(&self) -> bool {
        self.problem.is_none() && self.keys.iter().all(|key| matches!(key.check, KeyCheck::In { .. }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum EntryResult {
    /// The entry as written, with the `@string`, `crossref` and `xdata` it needs.
    Entry {
        key: String,
        text: String,
    },
    Refused {
        reason: EntryRefusal,
    },
}

/// The name a person knows the file by. No folder ever leaves the engine.
pub fn project_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

impl Engine {
    /// Resolve keys through the same path as an ordinary citation, then apply
    /// one explicit project's output and bibliography. GUI selection is absent.
    pub fn cite_writing_project_keys(
        &self,
        project: &ibis_core::settings::writing_projects::WritingProject,
        request: &KeyCitationRequest,
        cancel: &Cancellation,
    ) -> Result<WritingProjectCitation, EngineError> {
        let mut request = request.clone();
        request.output = project.output.clone();
        let request = self.key_citation_request(&request, cancel)?;
        self.cite_writing_project(project, &request, cancel)
    }

    /// The project definition is immutable for this request. Its output is the
    /// authority; callers choose another request rather than mixing outputs.
    pub fn cite_writing_project(
        &self,
        project: &ibis_core::settings::writing_projects::WritingProject,
        request: &CitationRequest,
        cancel: &Cancellation,
    ) -> Result<WritingProjectCitation, EngineError> {
        ibis_core::settings::writing_projects::validate(std::slice::from_ref(project))
            .map_err(|_| error(ErrorCode::BadInput))?;
        let mut request = request.clone();
        request.output = project.output.clone();
        let prepared = self.prepare_citation(&request, cancel)?;
        let checked = if prepared.uses_citation_keys() {
            project
                .bibliography
                .as_ref()
                .map(|path| {
                    cancel.check()?;
                    let read = ProjectRead::read(path, None);
                    cancel.check()?;
                    let keys = prepared.project_checks(&read);
                    Ok::<_, EngineError>(ProjectResult {
                        name: project_name(path),
                        keys,
                        problem: read.problem(),
                        damaged_lines: read.damaged_lines().to_vec(),
                    })
                })
                .transpose()?
        } else {
            None
        };
        Ok(WritingProjectCitation {
            id: project.id.clone(),
            name: project.name.clone(),
            revision: project.revision(),
            citation: prepared.into_result(),
            project: checked,
        })
    }
    pub(super) fn project(
        &self,
        request: &ProjectRequest,
        cancel: &Cancellation,
    ) -> Result<ProjectResult, EngineError> {
        if request.keys.len() > MAX_KEYS {
            return Err(error(ErrorCode::BadInput));
        }
        cancel.check()?;
        let read = ProjectRead::read(&request.path, None);
        cancel.check()?;
        self.project_checks(&read, &project_name(&request.path), &request.keys)
    }

    /// `keys` are compared exactly; a loaded record with that key adds its fields.
    pub fn project_checks(
        &self,
        read: &ProjectRead,
        name: &str,
        keys: &[String],
    ) -> Result<ProjectResult, EngineError> {
        let state = self.state()?;
        let keys = keys
            .iter()
            .map(|key| {
                let selector = ItemSelector::Key(key.clone());
                let record = match select(state.index.snapshot(), &state.libraries, &selector) {
                    Ok(SnapshotObject {
                        object: LibraryObject::Record(record),
                        ..
                    }) => Some(record),
                    _ => None,
                };
                ProjectKey {
                    key: key.clone(),
                    check: read.check(key, record),
                }
            })
            .collect();
        Ok(ProjectResult {
            name: name.to_owned(),
            keys,
            problem: read.problem(),
            damaged_lines: read.damaged_lines().to_vec(),
        })
    }

    /// One record against the file, by the key a citation of it would emit.
    /// `None` when the record has no usable key, which key outputs refuse anyway.
    pub fn project_check_item(
        &self,
        read: &ProjectRead,
        item: &ItemRef,
    ) -> Result<Option<(String, KeyCheck)>, EngineError> {
        let state = self.state()?;
        let Some(SnapshotObject {
            object: LibraryObject::Record(record),
            ..
        }) = state.index.snapshot().get(item)
        else {
            return Ok(None);
        };
        Ok(record
            .citation_key()
            .ok()
            .flatten()
            .map(|key| (key.to_owned(), read.check(key, Some(record)))))
    }

    /// The original entry of one loaded record.
    pub fn original_entry(&self, item: &ItemRef, cancel: &Cancellation) -> Result<EntryResult, EngineError> {
        self.entry(
            &EntryRequest {
                item: ItemSelector::Id(item.clone()),
            },
            cancel,
        )
    }

    pub(super) fn entry(&self, request: &EntryRequest, cancel: &Cancellation) -> Result<EntryResult, EngineError> {
        cancel.check()?;
        let state = self.state()?;
        let selected = select(state.index.snapshot(), &state.libraries, &request.item)?;
        let LibraryObject::Record(record) = &selected.object else {
            return Err(error(ErrorCode::NotFound));
        };
        let key = match record.citation_key() {
            Ok(Some(key)) => key.to_owned(),
            _ => {
                return Ok(EntryResult::Refused {
                    reason: EntryRefusal::NoOriginal,
                });
            }
        };
        let Some(path) = self.file_path(&record.id.library)? else {
            return Ok(EntryResult::Refused {
                reason: EntryRefusal::NoOriginal,
            });
        };
        let answer = ibis_sources::original_entry(&path, &record.id, &key, &selected.fingerprints.cite);
        cancel.check()?;
        Ok(match answer {
            Ok(text) => EntryResult::Entry { key, text },
            Err(reason) => EntryResult::Refused { reason },
        })
    }

    fn file_path(&self, library: &LibraryId) -> Result<Option<PathBuf>, EngineError> {
        let registry = self.shared.sources.lock().map_err(|_| error(ErrorCode::Internal))?;
        Ok(registry.file_path(library))
    }
}
