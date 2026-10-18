//! Selection and export rules shared by every front end.
//!
//! Citation keys are what writers type, but they are not identities: the
//! same key can exist in two libraries. Every way of selecting or exporting
//! references goes through these functions, so ambiguity is handled the same
//! way everywhere: it is reported, never resolved by guessing.

use std::collections::{BTreeMap, HashSet};

use serde_json::Value;
use thiserror::Error;

use crate::model::{RecordId, Reference};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SelectError {
    #[error("key `{0}` was not found")]
    NotFound(String),
    #[error("key `{key}` exists in more than one source ({})", sources.join(", "))]
    Ambiguous { key: String, sources: Vec<String> },
    #[error(
        "these keys exist in more than one source, so their citation IDs would collide: {}",
        keys.iter().map(|(k, s)| format!("`{k}` ({})", s.join(", "))).collect::<Vec<_>>().join("; ")
    )]
    DuplicateKeys { keys: Vec<(String, Vec<String>)> },
}

/// The single reference with this citation key.
pub fn find_by_key<'a>(references: &'a [Reference], key: &str) -> Result<&'a Reference, SelectError> {
    let matches: Vec<&Reference> = references
        .iter()
        .filter(|r| r.citekey.as_deref() == Some(key))
        .collect();
    match matches.as_slice() {
        [one] => Ok(one),
        [] => Err(SelectError::NotFound(key.to_string())),
        many => Err(SelectError::Ambiguous {
            key: key.to_string(),
            sources: many.iter().map(|r| r.id.source.clone()).collect(),
        }),
    }
}

/// CSL-JSON items for the given references. CSL IDs come from citation keys
/// and must be unique, so two different records sharing a key are refused.
/// Selecting the same record twice exports it once.
pub fn csl_items<'a>(references: impl IntoIterator<Item = &'a Reference>) -> Result<Vec<Value>, SelectError> {
    let mut seen_records: HashSet<&RecordId> = HashSet::new();
    let mut by_id: BTreeMap<String, Vec<&Reference>> = BTreeMap::new();
    let mut ordered = Vec::new();
    for reference in references {
        if !seen_records.insert(&reference.id) {
            continue;
        }
        let csl_id = reference
            .citekey
            .clone()
            .unwrap_or_else(|| reference.id.native_key.clone());
        by_id.entry(csl_id).or_default().push(reference);
        ordered.push(reference);
    }
    let collisions: Vec<(String, Vec<String>)> = by_id
        .into_iter()
        .filter(|(_, refs)| refs.len() > 1)
        .map(|(key, refs)| (key, refs.iter().map(|r| r.id.source.clone()).collect()))
        .collect();
    if !collisions.is_empty() {
        return Err(SelectError::DuplicateKeys { keys: collisions });
    }
    Ok(ordered.into_iter().map(Reference::to_csl_json).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bibtex::load_str;

    fn refs() -> Vec<Reference> {
        let mut all = load_str("a.bib", "@article{same, title={A}}\n@article{onlya, title={Only A}}").references;
        all.extend(load_str("b.bib", "@article{same, title={B}}").references);
        all
    }

    #[test]
    fn ambiguous_keys_are_reported_with_their_sources() {
        let refs = refs();
        assert_eq!(
            find_by_key(&refs, "same"),
            Err(SelectError::Ambiguous {
                key: "same".into(),
                sources: vec!["a.bib".into(), "b.bib".into()]
            })
        );
        assert!(find_by_key(&refs, "onlya").is_ok());
        assert_eq!(find_by_key(&refs, "none"), Err(SelectError::NotFound("none".into())));
    }

    #[test]
    fn export_refuses_colliding_ids_but_allows_repeated_selection() {
        let refs = refs();
        assert!(matches!(csl_items(&refs), Err(SelectError::DuplicateKeys { .. })));
        let only_a = &refs[1];
        assert_eq!(csl_items([only_a, only_a]).map(|v| v.len()), Ok(1));
    }
}
