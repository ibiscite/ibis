//! Native-only preparation/commit; the serialized method surface has no clipboard operation.
use super::{Cancellation, Engine, Shared, error};
use ibis_core::contract::*;
use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

pub struct PreparedCitation {
    origin: Weak<Shared>,
    snapshot: Arc<Snapshot>,
    request: CitationRequest,
    result: CitationResult,
}
impl PreparedCitation {
    pub fn result(&self) -> &CitationResult {
        &self.result
    }
    pub fn into_result(self) -> CitationResult {
        self.result
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
        let snapshot = self.snapshot()?;
        let result = self.cite_snapshot(&snapshot, request, cancel)?;
        Ok(PreparedCitation {
            origin: Arc::downgrade(&self.shared),
            snapshot,
            request: request.clone(),
            result,
        })
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
            let latest = self.snapshot()?;
            prepared.validate(&latest)?;
            let state = self.shared.state.read().map_err(|_| error(ErrorCode::Internal))?;
            if state.index.snapshot().fingerprints().cite != latest.fingerprints().cite {
                continue;
            }
            // Publication cannot replace cited data until the native write returns.
            return Ok(write(content));
        }
        Err(error(ErrorCode::ResourceLimit).into())
    }
}
impl PreparedCitation {
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
