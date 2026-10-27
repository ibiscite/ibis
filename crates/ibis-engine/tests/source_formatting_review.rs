#![allow(clippy::unwrap_used)]
//! Post-merge review regressions for formatted citations from source files.
use ibis_core::contract::*;
use ibis_engine::{Cancellation, Engine, Method, request::KeyCitationRequest};
use serde_json::json;

fn engine(name: &str, text: &str) -> (tempfile::TempDir, Engine) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(name);
    std::fs::write(&path, text).unwrap();
    let engine = Engine::new().unwrap();
    engine
        .dispatch(
            Method::Libraries,
            &json!({"action":"open-file","path":path}),
            &Cancellation::default(),
        )
        .unwrap();
    (directory, engine)
}

fn cite(engine: &Engine, style: &str, form: CitationForm) -> CitationResult {
    engine
        .cite_keys(
            &KeyCitationRequest {
                keys: vec!["k1".into()],
                output: OutputSpec::Csl { style: style.into() },
                form,
                output_locale: None,
            },
            &Cancellation::default(),
        )
        .unwrap()
}

#[test]
fn line_breaks_at_the_ends_of_a_field_print_nothing() {
    let (_directory, engine) = engine(
        "synthetic.json",
        r#"[{"id":"k1","citation-key":"k1","type":"book","title":"\nSynthetic signals\n","author":[{"family":"Taylor","given":"Avery"}],"issued":{"date-parts":[[2026]]},"publisher":"Imaginary\nPress"}]"#,
    );
    let entry = cite(&engine, "chicago-author-date", CitationForm::BibliographyOnly);
    assert_eq!(entry.text, "Taylor, Avery. 2026. Synthetic Signals. Imaginary Press.");
}

#[test]
fn a_truncated_editor_list_refuses_entries_but_not_author_citations() {
    let (_directory, engine) = engine(
        "synthetic.bib",
        "@book{k1, title={Synthetic signals}, author={Taylor, Avery}, editor={Example, Blake and others}, year={2026}, publisher={Imaginary Press}}\n",
    );
    let citation = cite(&engine, "apa", CitationForm::Normal);
    assert_eq!(citation.text, "(Taylor, 2026)");
    assert!(citation.copy_content(1).is_some());
    for (style, form) in [("apa", CitationForm::BibliographyOnly), ("ieee", CitationForm::Normal)] {
        let entry = cite(&engine, style, form);
        assert_eq!(
            entry.outcomes,
            vec![ItemOutcome::Refused {
                item: 0,
                error: CitationError::UnsupportedForm
            }],
            "{style}"
        );
    }
}
