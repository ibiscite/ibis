//! Native-only preparation/commit; the serialized method surface has no clipboard operation.
use super::{Cancellation, Engine, Shared, error};
use ibis_core::contract::*;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Weak},
};

pub struct PreparedCitation {
    origin: Weak<Shared>,
    generations: BTreeMap<Option<LibraryId>, u64>,
    snapshot: Arc<Snapshot>,
    request: CitationRequest,
    result: CitationResult,
    sample: bool,
}
impl PreparedCitation {
    pub fn result(&self) -> &CitationResult {
        &self.result
    }
    pub fn into_result(self) -> CitationResult {
        self.result
    }

    /// Read-only alternatives against the exact snapshot and group that made
    /// this receipt. No preview is published and no clipboard ticket changes.
    pub fn form_example(&self, engine: &Engine, original_output: &OutputSpec, item: usize, form: CitationForm, cancel: &Cancellation) -> Result<CitationResult, EngineError> {
        if !Weak::ptr_eq(&self.origin, &Arc::downgrade(&engine.shared)) { return Err(error(ErrorCode::BadInput)); }
        let mut request = self.request.clone();
        request.output = original_output.clone();
        let pick = request.items.get_mut(item).ok_or_else(|| error(ErrorCode::BadInput))?;
        pick.form = (form != CitationForm::Normal).then_some(form);
        pick.suppress_author = false;
        // Narrative is the textcite member of the native BibLaTeX family.
        if request.items.len() == 1 && form == CitationForm::Narrative {
            if let OutputSpec::Key { syntax: KeySyntax::Biblatex, command } = &mut request.output {
                if matches!(command.as_deref(), Some("autocite" | "parencite")) { *command = Some("textcite".into()); }
            }
        }
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
                Ok(format!("{body} {{\\plain [Sample]}}}}"))
            })
            .transpose()?;
        display.nodes.push(DisplayNode {
            item: None,
            content: vec![TextRun {
                text: " [Sample]".into(),
                ..TextRun::default()
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
        );
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
        let generations = request
            .items
            .iter()
            .map(|pick| {
                let source = super::read_source(&pick.item.library);
                let (generation, ready) = state.readiness.get(&source).copied().unwrap_or((0, true));
                if !ready {
                    return Err(error(ErrorCode::SourceUnavailable));
                }
                Ok((source, generation))
            })
            .collect::<Result<_, _>>()?;
        let snapshot = state.index.snapshot().clone();
        let result = self.cite_snapshot(&snapshot, request, cancel)?;
        let prepared = PreparedCitation {
            generations,
            origin: Arc::downgrade(&self.shared),
            snapshot,
            request: request.clone(),
            result,
            sample: false,
        };
        prepared
            .validate_source(self.state()?.as_ref())
            .map_err(|_| error(ErrorCode::SourceUnavailable))?;
        Ok(prepared)
    }
    /// Caller holds its native source/intent guard; callback must not reenter the
    /// engine. Validation/rendering never hold the publication barrier.
    pub fn commit_citation<T>(
        &self,
        prepared: &PreparedCitation,
        preview: &Fingerprint,
        write: impl FnOnce(ClipboardContent<'_>) -> T,
    ) -> Result<T, CitationCommitError> {
        if !Weak::ptr_eq(&prepared.origin, &Arc::downgrade(&self.shared)) || preview != &prepared.result.content_hash {
            return Err(CitationCommitError::PreviewChanged);
        }
        let Some(content) = prepared.result.copy_content(prepared.request.items.len()) else {
            return Err(prepared
                .result
                .outcomes
                .iter()
                .find_map(|outcome| match outcome {
                    ItemOutcome::Refused { item, error } => Some(CitationCommitError::Refused {
                        item: *item,
                        error: *error,
                    }),
                    _ => None,
                })
                .unwrap_or(CitationCommitError::PreviewChanged));
        };
        for _ in 0..3 {
            let latest = self.state()?;
            prepared.validate_source(&latest)?;
            let latest = latest.index.snapshot();
            prepared.validate(latest)?;
            let state = self.shared.state.read().map_err(|_| error(ErrorCode::Internal))?;
            if state.index.snapshot().fingerprints().cite != latest.fingerprints().cite {
                continue;
            }
            prepared.validate_source(&state)?;
            // Publication cannot replace cited data until the native write returns.
            return Ok(write(content));
        }
        Err(error(ErrorCode::ResourceLimit).into())
    }
}
impl PreparedCitation {
    fn validate_source(&self, state: &super::State) -> Result<(), CitationCommitError> {
        for (source, expected) in &self.generations {
            let (generation, ready) = state.readiness.get(source).copied().unwrap_or((0, true));
            if !ready {
                return Err(error(ErrorCode::SourceUnavailable).into());
            }
            if generation != *expected {
                return Err(CitationCommitError::PreviewChanged);
            }
        }
        Ok(())
    }
    fn validate(&self, latest: &Snapshot) -> Result<(), CitationCommitError> {
        if latest.fingerprints().cite == self.snapshot.fingerprints().cite {
            return Ok(());
        }
        let mut bindings = Vec::new();
        for (item, pick) in self.request.items.iter().enumerate() {
            let refuse = |error| CitationCommitError::Refused { item, error };
            let old = self
                .snapshot
                .get(&pick.item)
                .ok_or_else(|| refuse(CitationError::MissingItem))?;
            let now = latest
                .get(&pick.item)
                .ok_or_else(|| refuse(CitationError::MissingItem))?;
            if old.fingerprints.cite != now.fingerprints.cite {
                return Err(refuse(CitationError::ChangedItem));
            }
            if let Some(error) = annotation_error(latest, pick, Some(&self.snapshot)) {
                return Err(refuse(error));
            }
            if matches!(self.request.output, OutputSpec::Key { .. }) {
                let LibraryObject::Record(record) = &old.object else {
                    return Err(refuse(CitationError::NotCitable));
                };
                let key = record
                    .citation_key()
                    .map_err(|_| refuse(CitationError::ConflictingKey))?
                    .ok_or_else(|| refuse(CitationError::MissingKey))?;
                bindings.push((item, key, &pick.item));
            }
        }
        if bindings.is_empty() {
            return Ok(());
        }
        // Only requested key bindings are indexed, across every loaded library.
        enum Owner<'a> {
            Missing,
            Unique(&'a ItemRef),
            Ambiguous,
        }
        let mut owners: HashMap<&str, Owner<'_>> = bindings.iter().map(|(_, key, _)| (*key, Owner::Missing)).collect();
        for object in latest.objects() {
            let LibraryObject::Record(record) = &object.object else {
                continue;
            };
            for key in record
                .citation_keys
                .iter()
                .map(|key| key.value.as_str())
                .chain(record.aliases.iter().map(String::as_str))
            {
                let Some(owner) = owners.get_mut(key) else {
                    continue;
                };
                match owner {
                    Owner::Missing => *owner = Owner::Unique(&record.id),
                    Owner::Unique(previous) if *previous != &record.id => *owner = Owner::Ambiguous,
                    _ => {}
                }
            }
        }
        for (item, key, expected) in bindings {
            if !matches!(owners.get(key),Some(Owner::Unique(owner)) if *owner==expected) {
                return Err(CitationCommitError::Refused {
                    item,
                    error: CitationError::AmbiguousKey,
                });
            }
        }
        Ok(())
    }
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
