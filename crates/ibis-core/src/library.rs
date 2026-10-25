//! Selection and export rules shared by every front end.
//!
//! Citation keys are what writers type, but they are not identities: the
//! same key can exist in two libraries. Every way of selecting or exporting
//! references goes through these functions, so ambiguity is handled the same
//! way everywhere: it is reported, never resolved by guessing.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::cite::{CiteItem, Locator};
use crate::model::{RecordId, Reference};

/// One transient citation choice, resolved by full source identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "type-export", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct CitationSelection {
    pub id: RecordId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<Locator>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CitationSelectionError {
    #[error("a selected reference is no longer in the library")]
    NotFound,
    #[error("\"{0}\" has no citation key, so it cannot be cited in this format")]
    NoCitationKey(String),
    /// Another record has the same key, so the pasted key would not say which
    /// one is meant; the command line refuses the same key.
    #[error("the citation key `{0}` belongs to more than one reference, so it cannot be cited by key")]
    SharedCitationKey(String),
}

/// Resolve canonical citation keys while keeping order, repetitions and locators.
/// A key that another record also has is refused, as [`find_by_key`] refuses it.
/// Values are validated by the chosen key formatter, not during record lookup.
pub fn citation_items(
    references: &[Reference],
    selections: &[CitationSelection],
) -> Result<Vec<CiteItem>, CitationSelectionError> {
    selections
        .iter()
        .map(|selection| {
            let reference = references
                .iter()
                .find(|reference| reference.id == selection.id)
                .ok_or(CitationSelectionError::NotFound)?;
            let key = reference.citekey.clone().ok_or_else(|| {
                CitationSelectionError::NoCitationKey(
                    reference
                        .title_plain()
                        .unwrap_or_else(|| selection.id.native_key.clone()),
                )
            })?;
            if references
                .iter()
                .any(|other| other.id != reference.id && other.citekey.as_ref() == Some(&key))
            {
                return Err(CitationSelectionError::SharedCitationKey(key));
            }
            Ok(CiteItem {
                key,
                locator: selection.locator.clone(),
                ..CiteItem::default()
            })
        })
        .collect()
}

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
    fn citation_selections_keep_locators_with_record_identity_and_order() {
        use crate::cite::{KeyFormat, Locator, LocatorLabel, render};
        let mut references = refs();
        references[2].citekey = Some("onlyb".into());
        let selected = [
            CitationSelection {
                id: references[2].id.clone(),
                locator: Some(Locator {
                    label: LocatorLabel::Chapter,
                    value: "3".into(),
                }),
            },
            CitationSelection {
                id: references[0].id.clone(),
                locator: Some(Locator {
                    label: LocatorLabel::Page,
                    value: "12".into(),
                }),
            },
        ];
        let items = citation_items(&references, &selected).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            render(KeyFormat::Pandoc, &items).unwrap_or_else(|e| panic!("{e}")).text,
            "[@onlyb, chap. 3; @same, p. 12]"
        );
        assert_eq!(selected[0].id.source, "b.bib");
        assert_eq!(selected[1].id.source, "a.bib");
    }

    #[test]
    fn selection_requests_are_strict_and_keep_missing_key_metadata_usable() {
        let json = r#"{"id":{"source":"synthetic","native_key":"a"}}"#;
        let selected: CitationSelection = serde_json::from_str(json).unwrap_or_else(|error| panic!("{error}"));
        assert!(selected.locator.is_none());
        for json in [
            r#"{"id":{"source":"synthetic","native_key":"a"},"key":"forged"}"#,
            r#"{"id":{"source":"synthetic","native_key":"a"},"locator":{"label":"timestamp","value":"01:23"}}"#,
            r#"{"id":{"source":"synthetic","native_key":"a"},"locator":{"label":"page","value":"1","extra":true}}"#,
            r#"{"id":{"source":"synthetic","native_key":"a"},"locator":null,"locator":null}"#,
        ] {
            assert!(serde_json::from_str::<CitationSelection>(json).is_err());
        }
        let mut references = load_str("synthetic", "@misc{a,title={Synthetic title}}").references;
        assert_eq!(
            citation_items(&references, &[selected.clone(), selected.clone()]).map(|items| items.len()),
            Ok(2)
        );
        references[0].citekey = None;
        assert_eq!(
            citation_items(&references, std::slice::from_ref(&selected)),
            Err(CitationSelectionError::NoCitationKey("Synthetic title".into()))
        );
        assert!(csl_items(&references).is_ok());
        assert_eq!(citation_items(&[], &[selected]), Err(CitationSelectionError::NotFound));
    }

    #[test]
    fn a_selected_record_whose_key_another_record_shares_is_refused() {
        let refs = refs();
        let select = |reference: &Reference| CitationSelection {
            id: reference.id.clone(),
            locator: None,
        };
        // Either record with the shared key is refused, alone or with others.
        for selected in [vec![select(&refs[0])], vec![select(&refs[1]), select(&refs[2])]] {
            assert_eq!(
                citation_items(&refs, &selected),
                Err(CitationSelectionError::SharedCitationKey("same".into()))
            );
        }
        assert!(citation_items(&refs, &[select(&refs[1])]).is_ok());
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
