//! A project `.bib` beside the libraries, never among them: its keys are
//! compared exactly as the citation would emit them, and it owns no records.
use super::*;
use std::path::{Path, PathBuf};

const MAX_KEYS: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectAssessmentState {
    /// The citation may be used; its key checks say what to look at.
    Ready,
    /// A key known to name a different work.
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectAssessment {
    pub state: ProjectAssessmentState,
    pub name: String,
    pub checks: Vec<ProjectKey>,
    pub message: Option<String>,
    /// Keys with contradictory work identifiers, not mere metadata differences.
    pub conflicts: Vec<String>,
}
impl ProjectAssessment {
    pub fn can_copy(&self) -> bool {
        self.state == ProjectAssessmentState::Ready
    }
}

/// A request-local writing context, with the ordinary citation contract kept
/// intact. Paths are native input only; the check names just its bibliography.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WritingCitation {
    pub name: String,
    pub citation: CitationResult,
    pub bibliography: Option<ProjectResult>,
    pub assessment: Option<ProjectAssessment>,
    /// The libraries the citation was made from, as they were then.
    pub source_health: super::health::SourceHealth,
}
impl WritingCitation {
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
    pub fn cite_writing_keys(
        &self,
        writing: &crate::settings::writings::Writing,
        request: &KeyCitationRequest,
        cancel: &Cancellation,
    ) -> Result<WritingCitation, EngineError> {
        let mut request = request.clone();
        request.output = writing.format.clone();
        let request = self.key_citation_request(&request, cancel)?;
        self.writing_citation(writing, &request, true, cancel)
    }

    /// This immutable writing owns the request's format. Its optional .bib is
    /// advice unless an emitted key is known to identify a different work.
    pub fn cite_writing(
        &self,
        writing: &crate::settings::writings::Writing,
        request: &CitationRequest,
        cancel: &Cancellation,
    ) -> Result<WritingCitation, EngineError> {
        self.writing_citation(writing, request, false, cancel)
    }

    pub(crate) fn writing_citation(
        &self,
        writing: &crate::settings::writings::Writing,
        request: &CitationRequest,
        by_key: bool,
        cancel: &Cancellation,
    ) -> Result<WritingCitation, EngineError> {
        crate::settings::writings::validate(std::slice::from_ref(writing)).map_err(|_| error(ErrorCode::BadInput))?;
        let mut request = request.clone();
        request.output = writing.format.clone();
        let prepared = self.prepare_consulting(&request, by_key, cancel)?;
        self.writing_prepared(writing, &request, &prepared, cancel)
    }

    pub(crate) fn writing_prepared(
        &self,
        writing: &crate::settings::writings::Writing,
        request: &CitationRequest,
        prepared: &super::PreparedCitation,
        cancel: &Cancellation,
    ) -> Result<WritingCitation, EngineError> {
        let source_health = prepared.source_health().clone();
        let mut assessment = None;
        let mut evidence = CitationProjectCheck {
            revision: Fingerprint::of_bytes(&serde_json::to_vec(writing).map_err(|_| error(ErrorCode::Internal))?)
                .as_str()
                .to_owned(),
            bibliography: None,
            checked_at_ms: None,
        };
        let checked = if prepared.uses_citation_keys() {
            writing
                .bibliography
                .as_ref()
                .map(|path| {
                    cancel.check()?;
                    let read = ProjectRead::read(path, None);
                    cancel.check()?;
                    evidence.bibliography = read.fingerprint().cloned();
                    evidence.checked_at_ms = read.current_complete().then(now_ms);
                    let keys = prepared.project_checks(&read);
                    assessment = Some(prepared.project_evidence(&read));
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
        if request.strict
            && prepared.uses_citation_keys()
            && (checked.as_ref().is_none_or(|check| !check.settled())
                || assessment.as_ref().is_some_and(|check| !check.can_copy()))
        {
            return Err(EngineError::because(ErrorCode::BadInput, ErrorReason::StrictCitation));
        }
        let mut citation = prepared.result().clone();
        if let Some(receipt) = &mut citation.receipt {
            receipt.project = Some(evidence);
        }
        if assessment.as_ref().is_some_and(|check| !check.can_copy()) {
            citation = CitationResult::new(
                String::new(),
                None,
                None,
                DisplayTree::default(),
                citation
                    .outcomes
                    .into_iter()
                    .map(|outcome| match outcome {
                        ItemOutcome::Rendered { item } => ItemOutcome::Refused {
                            item,
                            error: CitationError::ConflictingKey,
                        },
                        refused => refused,
                    })
                    .collect(),
                citation.losses,
                citation.notices,
            )
            .with_warnings(citation.warnings);
        }
        Ok(WritingCitation {
            name: writing.name.clone(),
            citation,
            bibliography: checked,
            assessment,
            source_health,
        })
    }
    pub(super) fn project(
        &self,
        request: &ProjectRequest,
        cancel: &Cancellation,
    ) -> Result<super::health::Checked<ProjectResult>, EngineError> {
        if request.keys.len() > MAX_KEYS {
            return Err(error(ErrorCode::BadInput));
        }
        cancel.check()?;
        let read = ProjectRead::read(&request.path, None);
        cancel.check()?;
        self.project_checks_checked(&read, &project_name(&request.path), &request.keys)
    }

    /// `keys` are compared exactly; a loaded record with that key adds its fields.
    pub fn project_checks(
        &self,
        read: &ProjectRead,
        name: &str,
        keys: &[String],
    ) -> Result<ProjectResult, EngineError> {
        Ok(self.project_checks_checked(read, name, keys)?.value)
    }

    /// The same, with the health of every library: the keys are looked up in all of them.
    fn project_checks_checked(
        &self,
        read: &ProjectRead,
        name: &str,
        keys: &[String],
    ) -> Result<super::health::Checked<ProjectResult>, EngineError> {
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
        Ok(super::health::Checked {
            source_health: super::health::SourceHealth::of(state.libraries.values()),
            value: ProjectResult {
                name: name.to_owned(),
                keys,
                problem: read.problem(),
                damaged_lines: read.damaged_lines().to_vec(),
            },
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
        Ok(self
            .entry_checked(
                &EntryRequest {
                    item: ItemSelector::Id(item.clone()),
                },
                cancel,
            )?
            .value)
    }

    pub(super) fn entry_checked(
        &self,
        request: &EntryRequest,
        cancel: &Cancellation,
    ) -> Result<super::health::Checked<EntryResult>, EngineError> {
        cancel.check()?;
        let state = self.state()?;
        let selected = select(state.index.snapshot(), &state.libraries, &request.item)?;
        let source_health = super::consulted(&state, &request.item, &selected.object.id().library);
        let checked = |value| super::health::Checked {
            value,
            source_health: source_health.clone(),
        };
        let LibraryObject::Record(record) = &selected.object else {
            return Err(error(ErrorCode::NotFound));
        };
        let key = match record.citation_key() {
            Ok(Some(key)) => key.to_owned(),
            _ => {
                return Ok(checked(EntryResult::Refused {
                    reason: EntryRefusal::NoOriginal,
                }));
            }
        };
        let Some(path) = self.file_path(&record.id.library)? else {
            return Ok(checked(EntryResult::Refused {
                reason: EntryRefusal::NoOriginal,
            }));
        };
        let answer = ibis_sources::original_entry(&path, &record.id, &key, &selected.fingerprints.cite);
        cancel.check()?;
        Ok(checked(match answer {
            Ok(text) => EntryResult::Entry { key, text },
            Err(reason) => EntryResult::Refused { reason },
        }))
    }

    fn file_path(&self, library: &LibraryId) -> Result<Option<PathBuf>, EngineError> {
        let registry = self.shared.sources.lock().map_err(|_| error(ErrorCode::Internal))?;
        Ok(registry.file_path(library))
    }
}
