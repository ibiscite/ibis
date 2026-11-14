//! Conversion only: no acquisition, attachment probing or publication.
//!
//! Acquisition/consent handoff: before wiring either adapter to this function,
//! the engine must enforce per-library passage consent, off by default and
//! revocable. Adapters must supply these neutral fields and establish that the
//! parent is an attachment in that library. Existing publication uses only
//! `map_item`, which continues to refuse children. Authoritative page resolution
//! belongs to the reader; this mapper preserves the raw printed label.

use ibis_core::contract::{Annotation, ItemRef, LibraryId};
use serde::Deserialize;

use super::{MappingError, ZoteroItem, validate, validate_identity};

/// Maps stored annotation strings without HTML or citation normalization.
/// Only highlight/underline text supplies quotation text; comments stay separate.
pub fn map_annotation(library: LibraryId, item: &ZoteroItem) -> Result<Annotation, MappingError> {
    validate(item)?;
    if item.item_type != "annotation" {
        return Err(MappingError::UnsupportedType);
    }
    let field = |name: &str| item.fields.get(name).map(String::as_str);
    let quotation = match field("annotationType") {
        Some("highlight" | "underline") => true,
        Some("image" | "ink" | "note" | "text") => false,
        _ => return Err(MappingError::UnsupportedType),
    };
    let parent = field("parentItem").ok_or(MappingError::InvalidIdentity)?;
    validate_identity(parent)?;
    if parent == item.key {
        return Err(MappingError::InvalidIdentity);
    }
    let page_index = field("annotationPosition").map(page_index).transpose()?.flatten();
    Ok(Annotation {
        id: ItemRef {
            library: library.clone(),
            key: item.key.clone(),
        },
        parent: ItemRef {
            library,
            key: parent.to_owned(),
        },
        text: if quotation {
            field("annotationText").unwrap_or_default().to_owned()
        } else {
            String::new()
        },
        comment: field("annotationComment").unwrap_or_default().to_owned(),
        page_label: field("annotationPageLabel").map(str::to_owned),
        page_index,
    })
}

fn page_index(position: &str) -> Result<Option<u32>, MappingError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Position {
        page_index: Option<u32>,
    }
    // Serde structs also accept positional arrays; stored positions are objects.
    // Bounds were checked before parsing. Ignore geometry, reject malformed JSON,
    // duplicate indices and non-u32 values without retaining parser diagnostics.
    if !position.trim_start().starts_with('{') {
        return Err(MappingError::InvalidPosition);
    }
    serde_json::from_str::<Position>(position)
        .map(|position| position.page_index)
        .map_err(|_| MappingError::InvalidPosition)
}
