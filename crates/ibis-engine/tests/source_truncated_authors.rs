#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use ibis_engine::{Cancellation, Engine, Method, request::KeyCitationRequest};
use serde_json::json;

#[test]
fn bibtex_truncated_authors_never_become_a_copyable_author_named_others() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("synthetic.bib");
    std::fs::write(
        &path,
        "@book{truncated,title={Synthetic signals},author={Taylor, Avery and others},year={2026},publisher={Imaginary Press}}\n@book{complete,title={Other synthetic work},author={Morgan, Ann},year={2025},publisher={Imaginary Press}}\n",
    )
    .unwrap();
    let engine = Engine::new().unwrap();
    let cancel = Cancellation::default();
    engine
        .dispatch(Method::Libraries, &json!({"action":"open-file","path":path}), &cancel)
        .unwrap();
    for (style, _) in ibis_csl::STYLES {
        for form in [CitationForm::Normal, CitationForm::BibliographyOnly] {
            let result = engine
                .cite_keys(
                    &KeyCitationRequest {
                        keys: vec!["truncated".into()],
                        output: OutputSpec::Csl { style: style.into() },
                        form,
                        output_locale: None,
                    },
                    &cancel,
                )
                .unwrap();
            assert!(
                !result.text.to_lowercase().contains("others"),
                "{style}: {}",
                result.text
            );
            assert_eq!(
                result.outcomes,
                vec![ItemOutcome::Refused {
                    item: 0,
                    error: CitationError::UnsupportedForm
                }]
            );
            assert!(result.copy_content(1).is_none());
            let mixed = engine
                .cite_keys(
                    &KeyCitationRequest {
                        keys: vec!["truncated".into(), "complete".into()],
                        output: OutputSpec::Csl { style: style.into() },
                        form,
                        output_locale: None,
                    },
                    &cancel,
                )
                .unwrap();
            assert_eq!(
                mixed.outcomes,
                vec![
                    ItemOutcome::Refused {
                        item: 0,
                        error: CitationError::UnsupportedForm
                    },
                    ItemOutcome::Rendered { item: 1 }
                ]
            );
            assert!(mixed.text.contains("Morgan"));
            assert!(!mixed.text.contains("others"));
            assert!(mixed.copy_content(2).is_none());
        }
    }
    let keys = engine
        .cite_keys(
            &KeyCitationRequest {
                keys: vec!["truncated".into()],
                output: OutputSpec::Key {
                    syntax: KeySyntax::Pandoc,
                    command: None,
                },
                form: CitationForm::Normal,
                output_locale: None,
            },
            &cancel,
        )
        .unwrap();
    assert_eq!(keys.copy_content(1).unwrap().text, "[@truncated]");
}
