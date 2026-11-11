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
        let sources = libraries
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
        let source_failure = sources.iter().any(|source| source.refresh_error.is_some());
        if source_failure
            || sources.iter().any(|source| source.warnings > 0)
            || !result.losses.is_empty()
            || result.copy_content(request.items.len()).is_none()
        {
            return Err(EngineError::because(
                if source_failure {
                    ErrorCode::SourceUnavailable
                } else {
                    ErrorCode::BadInput
                },
                ErrorReason::StrictCitation,
            ));
        }
    }
    // A partial result cannot attest to the requested citation.
    if result.copy_content(request.items.len()).is_some() {
        result.receipt = Some(CitationReceipt {
            sample: false,
            request: request.clone(),
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
