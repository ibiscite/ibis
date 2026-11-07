//! Native-only preparation/commit; the serialized method surface has no clipboard operation.
use super::{
    Cancellation, Engine, ProjectAssessment, ProjectAssessmentState, ProjectKey, ProjectPolicy, ProjectRead, error,
};
use ibis_core::contract::*;
use std::sync::Arc;

pub struct PreparedCitation {
    zotero_generation: Option<u64>,
    snapshot: Arc<Snapshot>,
    request: CitationRequest,
    result: CitationResult,
    sample: bool,
}
impl PreparedCitation {
    /// Caller owns the selected bibliography and binds it to this task's
    /// immutable context. Rechecking never consumes or changes the preview.
    /// A check is advice: only a key known to name a different work, or a
    /// strict request without a complete current read, withholds the citation.
    pub fn project_evidence(&self, read: &ProjectRead, policy: ProjectPolicy) -> ProjectAssessment {
        let checks = self.project_checks(read);
        let conflicts: Vec<_> = if self.uses_citation_keys() {
            self.request
                .items
                .iter()
                .filter_map(|pick| {
                    let LibraryObject::Record(record) = &self.snapshot.get(&pick.item)?.object else {
                        return None;
                    };
                    let key = record.citation_key().ok().flatten()?;
                    read.conflicts(key, record).then(|| key.to_owned())
                })
                .collect()
        } else {
            Vec::new()
        };
        let name = read.name();
        let (state, message) = if !conflicts.is_empty() {
            (
                ProjectAssessmentState::Conflict,
                Some(format!(
                    "A key in {name} identifies a different work. Resolve the conflict before citing it."
                )),
            )
        } else if policy == ProjectPolicy::Strict && self.uses_citation_keys() && !read.current_complete() {
            (
                ProjectAssessmentState::Paused,
                Some(format!(
                    "A current successful check of {name} is required. Retry before citing."
                )),
            )
        } else {
            (ProjectAssessmentState::Ready, None)
        };
        ProjectAssessment {
            state,
            name,
            checks,
            message,
            conflicts,
        }
    }
    /// Record exports do not emit citation keys even though they share the
    /// key-output request family; their target needs no project bibliography.
    pub fn uses_citation_keys(&self) -> bool {
        ibis_core::syntax::capabilities(&self.request.output).is_ok_and(|capabilities| !capabilities.exports_records)
    }
    /// Check exactly the emitted keys and record fields that made this receipt,
    /// even if a later source snapshot has renamed or replaced a reference.
    pub fn project_checks(&self, read: &ProjectRead) -> Vec<ProjectKey> {
        if !self.uses_citation_keys() {
            return Vec::new();
        }
        self.request
            .items
            .iter()
            .filter_map(|pick| {
                let LibraryObject::Record(record) = &self.snapshot.get(&pick.item)?.object else {
                    return None;
                };
                let key = record.citation_key().ok().flatten()?;
                Some(ProjectKey {
                    key: key.to_owned(),
                    check: read.check(key, Some(record)),
                })
            })
            .collect()
    }
    pub fn result(&self) -> &CitationResult {
        &self.result
    }
    pub fn into_result(self) -> CitationResult {
        self.result
    }
    pub fn request_output(&self) -> &OutputSpec {
        &self.request.output
    }
    pub fn request_form(&self) -> CitationForm {
        self.request.form
    }
    /// The records this citation cites, in its order.
    pub fn items(&self) -> Vec<ItemRef> {
        self.request.items.iter().map(|pick| pick.item.clone()).collect()
    }
    pub fn item_form(&self, item: usize) -> Option<CitationForm> {
        self.request
            .items
            .get(item)
            .map(|pick| pick.form.unwrap_or(self.request.form))
    }
    /// A short diagnostic name from this receipt's immutable source, never a
    /// later library snapshot. It is presentation text, not citation markup.
    pub fn item_label(&self, item: usize) -> Option<String> {
        let LibraryObject::Record(record) = &self.snapshot.get(&self.request.items.get(item)?.item)?.object else {
            return None;
        };
        let value = record
            .citation_key()
            .ok()
            .flatten()
            .map(str::to_owned)
            .or_else(|| record.csl.get("title")?.as_str().map(ibis_core::model::strip_markup))?;
        Some(if value.chars().count() > 120 {
            format!("{}…", value.chars().take(119).collect::<String>())
        } else {
            value
        })
    }

    /// Read-only alternatives against the exact snapshot and group that made
    /// this receipt. No preview is published and no clipboard ticket changes.
    pub fn form_example(
        &self,
        engine: &Engine,
        original_output: &OutputSpec,
        item: usize,
        form: CitationForm,
        cancel: &Cancellation,
    ) -> Result<CitationResult, EngineError> {
        let mut request = self.request.clone();
        request.output = original_output.clone();
        let pick = request.items.get_mut(item).ok_or_else(|| error(ErrorCode::BadInput))?;
        pick.form = (form != CitationForm::Normal).then_some(form);
        pick.suppress_author = false;
        engine.cite_snapshot(&self.snapshot, &request, cancel)
    }

    /// Bind sample provenance to every representation before preview or commit.
    /// The marker is fixed text, never source-provided markup. The original
    /// request and source validation stay attached to the marked result.
    pub fn mark_sample(mut self) -> Result<Self, EngineError> {
        if self.sample {
            return Ok(self);
        }
        if self.result.copy_content(self.request.items.len()).is_none() {
            return Err(error(ErrorCode::BadInput));
        }
        let CitationResult {
            text,
            html,
            rtf,
            mut display,
            outcomes,
            losses,
            notices,
            warnings,
            ..
        } = self.result;
        let rtf = rtf
            .map(|rtf| {
                // The formatter owns the RTF document. Keep its header and content,
                // and put a plain, visible marker inside its final root brace.
                let rtf = rtf.trim_end();
                let body = rtf
                    .strip_suffix('}')
                    .filter(|_| rtf.starts_with("{\\rtf1"))
                    .ok_or_else(|| error(ErrorCode::Internal))?;
                Ok::<_, EngineError>(format!("{body} {{\\plain [Sample]}}}}"))
            })
            .transpose()?;
        display.nodes.push(DisplayNode {
            item: None,
            content: vec![DisplayRun {
                text: " [Sample]".into(),
                ..DisplayRun::default()
            }],
        });
        self.result = CitationResult::new(
            format!("{text} [Sample]"),
            html.map(|html| format!("{html}<span> [Sample]</span>")),
            rtf,
            display,
            outcomes,
            losses,
            notices,
        )
        .with_warnings(warnings);
        self.sample = true;
        Ok(self)
    }
}
#[derive(Debug, thiserror::Error)]
pub enum CitationCommitError {
    #[error("citation preview changed")]
    PreviewChanged,
    #[error("citation item refused")]
    Refused { item: usize, error: CitationError },
    #[error("citation commit unavailable")]
    Engine(EngineError),
}
impl From<EngineError> for CitationCommitError {
    fn from(error: EngineError) -> Self {
        Self::Engine(error)
    }
}
impl Engine {
    pub fn prepare_citation(
        &self,
        request: &CitationRequest,
        cancel: &Cancellation,
    ) -> Result<PreparedCitation, EngineError> {
        cancel.check()?;
        let state = self.state()?;
        let uses_zotero = request.items.iter().any(|pick| {
            state
                .libraries
                .get(&pick.item.library)
                .is_some_and(|library| library.kind == ibis_sources::LibraryKind::Zotero)
        });
        if uses_zotero && !state.zotero_ready {
            return Err(error(ErrorCode::SourceUnavailable));
        }
        let snapshot = state.index.snapshot().clone();
        // The receipt keeps the output the preset means, and the exact objects
        // it cites: citing it again refuses a changed reference even when its
        // text would come out alike.
        let mut request = CitationRequest {
            output: crate::outputs::resolve(&request.output)?,
            ..request.clone()
        };
        for pick in &mut request.items {
            let held = |id: &ItemRef| snapshot.get(id).map(|object| object.fingerprints.cite.clone());
            pick.expected_cite = pick.expected_cite.take().or_else(|| held(&pick.item));
            pick.expected_annotation_cite = pick
                .expected_annotation_cite
                .take()
                .or_else(|| pick.annotation.as_ref().and_then(held));
        }
        let result = self.cite_snapshot(&snapshot, &request, cancel)?;
        let prepared = PreparedCitation {
            zotero_generation: uses_zotero.then_some(state.zotero_generation),
            snapshot,
            request,
            result,
            sample: false,
        };
        // Formatting takes time, and a Zotero read can begin or finish during it.
        prepared
            .check_zotero(self.state()?.as_ref())
            .map_err(|_| error(ErrorCode::SourceUnavailable))?;
        Ok(prepared)
    }

    /// What a copy may write now, or why not: the stored request is cited again
    /// against the current library and must give exactly the content that was
    /// shown. A changed or removed reference, a key another record now shares
    /// or a different Zotero shows up as a refusal or a different hash, so no
    /// separate check can drift from what citing does. A publication after
    /// this returns cannot change what was true when it was checked.
    pub fn recite(
        &self,
        prepared: &PreparedCitation,
        cancel: &Cancellation,
    ) -> Result<CitationResult, CitationCommitError> {
        let items = prepared.request.items.len();
        if prepared.result.copy_content(items).is_none() {
            return Err(refusal(&prepared.result));
        }
        let fresh = self.prepare_citation(&prepared.request, cancel)?;
        // Zotero may have become another library with the same contents.
        if fresh.zotero_generation != prepared.zotero_generation {
            return Err(CitationCommitError::PreviewChanged);
        }
        if fresh.result.copy_content(items).is_none() {
            return Err(refusal(&fresh.result));
        }
        // An annotation's page means its file as it was shown.
        for (item, pick) in prepared.request.items.iter().enumerate() {
            if let Some(error) = annotation_error(&fresh.snapshot, pick, Some(&prepared.snapshot)) {
                return Err(CitationCommitError::Refused { item, error });
            }
        }
        let fresh = if prepared.sample { fresh.mark_sample()? } else { fresh };
        if fresh.result.content_hash != prepared.result.content_hash {
            return Err(CitationCommitError::PreviewChanged);
        }
        Ok(fresh.result)
    }
}
impl PreparedCitation {
    fn check_zotero(&self, state: &super::State) -> Result<(), CitationCommitError> {
        if let Some(expected) = self.zotero_generation {
            if !state.zotero_ready {
                return Err(error(ErrorCode::SourceUnavailable).into());
            }
            if state.zotero_generation != expected {
                return Err(CitationCommitError::PreviewChanged);
            }
        }
        Ok(())
    }
}

fn refusal(result: &CitationResult) -> CitationCommitError {
    result
        .outcomes
        .iter()
        .find_map(|outcome| match outcome {
            ItemOutcome::Refused { item, error } => Some(CitationCommitError::Refused {
                item: *item,
                error: *error,
            }),
            _ => None,
        })
        .unwrap_or(CitationCommitError::PreviewChanged)
}

impl std::fmt::Debug for PreparedCitation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedCitation").finish_non_exhaustive()
    }
}

pub(super) fn annotation_error(
    snapshot: &Snapshot,
    pick: &CitationItem,
    original: Option<&Snapshot>,
) -> Option<CitationError> {
    let Some(id) = &pick.annotation else {
        return pick
            .expected_annotation_cite
            .as_ref()
            .map(|_| CitationError::AnnotationMismatch);
    };
    let invalid = || Some(CitationError::AnnotationMismatch);
    let Some(object) = snapshot.get(id) else {
        return invalid();
    };
    if pick
        .expected_annotation_cite
        .as_ref()
        .is_some_and(|expected| expected != &object.fingerprints.cite)
        || original.is_some_and(|old| {
            old.get(id)
                .is_none_or(|old| old.fingerprints.cite != object.fingerprints.cite)
        })
    {
        return invalid();
    }
    let LibraryObject::Annotation(annotation) = &object.object else {
        return invalid();
    };
    let Some(parent) = snapshot.get(&annotation.parent) else {
        return invalid();
    };
    let LibraryObject::Attachment(attachment) = &parent.object else {
        return invalid();
    };
    if attachment.parent.as_ref() != Some(&pick.item)
        || original.is_some_and(|old| {
            old.get(&annotation.parent)
                .is_none_or(|old| old.fingerprints.cite != parent.fingerprints.cite)
        })
    {
        return invalid();
    }
    match (&annotation.page_label, &pick.locator) {
        (Some(page), Some(locator)) if locator.term == LocatorTerm::Page && page.trim() == locator.value.trim() => None,
        _ => invalid(),
    }
}
