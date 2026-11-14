//! Receipt construction and strict policy use only the citation's held state.
use super::*;
use std::collections::BTreeSet;

pub(super) fn source_findings(receipt: &CitationReceipt) -> bool {
    receipt
        .sources
        .iter()
        .any(|source| source.refresh_error.is_some() || source.warnings > 0)
}

/// Only citation dependencies survive preparation; the old search index does not.
pub(super) struct CitationContext {
    pub snapshot: Arc<Snapshot>,
    pub sources: Vec<CitationSourceCheck>,
    pub source_failure: Option<EngineError>,
    pub zotero: BTreeMap<LibraryId, ZoteroLibrary>,
}

impl CitationContext {
    pub fn capture(state: &State, request: &CitationRequest) -> Self {
        let mut libraries = BTreeSet::new();
        for pick in &request.items {
            libraries.insert(pick.item.library.clone());
            if let Some(annotation) = &pick.annotation {
                libraries.insert(annotation.library.clone());
                if let Some(SnapshotObject {
                    object: LibraryObject::Annotation(annotation),
                    ..
                }) = state.index.snapshot().get(annotation)
                {
                    libraries.insert(annotation.parent.library.clone());
                }
            }
        }
        let zotero = libraries
            .iter()
            .filter_map(|id| {
                let source = state.libraries.get(id)?;
                (source.kind == LibraryKind::Zotero)
                    .then_some(source.zotero)
                    .flatten()
                    .map(|origin| (id.clone(), origin))
            })
            .collect();
        let sources: Vec<_> = libraries
            .into_iter()
            .filter_map(|id| {
                let source = state.libraries.get(&id)?;
                Some(CitationSourceCheck {
                    library: id,
                    last_successful_read_ms: (source.kind != LibraryKind::Snapshot).then_some(source.updated_at_ms),
                    refresh_error: source.refresh_error,
                    warnings: source.warnings_total,
                })
            })
            .collect();
        Self {
            snapshot: state.index.snapshot().clone(),
            source_failure: sources.iter().find_map(|source| {
                let library = state.libraries.get(&source.library)?;
                Some(EngineError {
                    reason: library.refresh_reason,
                    ..EngineError::new(library.refresh_error?)
                })
            }),
            sources,
            zotero,
        }
    }
}

pub(super) fn attach(
    result: &mut CitationResult,
    request: &CitationRequest,
    context: &CitationContext,
) -> Result<(), EngineError> {
    let sources = &context.sources;
    if request.strict {
        let cause = context
            .source_failure
            .map(|error| StrictCause::Source {
                code: error.code,
                reason: error.reason,
            })
            .or_else(|| {
                (sources.iter().any(|source| source.warnings > 0)
                    || result
                        .warnings
                        .iter()
                        .any(|warning| warning.code == CitationWarningCode::SourceField))
                .then_some(StrictCause::SourceWarning)
            })
            .or_else(|| {
                result.outcomes.iter().find_map(|outcome| match outcome {
                    ItemOutcome::Refused { error, .. } => Some(StrictCause::Refused { error: *error }),
                    ItemOutcome::Unknown => Some(StrictCause::Refused {
                        error: CitationError::Unknown,
                    }),
                    ItemOutcome::Rendered { .. } => None,
                })
            })
            .or_else(|| result.losses.first().map(|loss| StrictCause::Loss { loss: *loss }))
            .or_else(|| {
                result
                    .copy_content(request.items.len())
                    .is_none()
                    .then_some(StrictCause::Refused {
                        error: CitationError::Unknown,
                    })
            });
        if let Some(cause) = cause {
            return Err(EngineError {
                strict_cause: Some(cause),
                ..EngineError::because(
                    match cause {
                        StrictCause::Source { code, .. } => code,
                        _ => ErrorCode::BadInput,
                    },
                    ErrorReason::StrictCitation,
                )
            });
        }
    }
    // A partial result cannot attest to the requested citation.
    if result.copy_content(request.items.len()).is_some() {
        // Native source keys may be paths. Evidence keeps opaque identities;
        // PreparedCitation owns the original request used for revalidation.
        let mut public_request = request.clone();
        for pick in &mut public_request.items {
            for id in std::iter::once(&mut pick.item).chain(pick.annotation.iter_mut()) {
                let bytes = serde_json::to_vec(id).map_err(|_| error(ErrorCode::Internal))?;
                id.key = format!("item_{}", Fingerprint::of_bytes(&bytes).as_str());
            }
        }
        result.receipt = Some(CitationReceipt {
            sample: false,
            request: public_request,
            sources: sources.clone(),
            content_hash: result.content_hash.clone(),
            ibis_version: ibis_core::VERSION.into(),
            meaning_version: 1,
            language: result.language.clone(),
            formatting: matches!(request.output, OutputSpec::Csl { .. }).then(ibis_csl::provenance),
            project: None,
        });
    }
    Ok(())
}
