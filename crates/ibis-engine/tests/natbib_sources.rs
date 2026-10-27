#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use ibis_engine::{Cancellation, Engine, Method};
use serde_json::json;

#[test]
fn bibtex_source_to_natbib_preserves_or_explicitly_refuses_literal_suffixes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("synthetic.bib");
    std::fs::write(
        &path,
        "@book{Example2026,title={Synthetic signals},author={Taylor, Avery},year={2026}}",
    )
    .unwrap();
    let engine = Engine::new().unwrap();
    let cancel = Cancellation::default();
    let loaded = engine
        .dispatch(Method::Libraries, &json!({"action":"open-file","path":path}), &cancel)
        .unwrap();
    assert_eq!(loaded.exit_code, ExitCode::Ok);
    let item = engine.snapshot().unwrap().objects()[0].object.id().clone();
    for (form, command) in [
        (CitationForm::Normal, "citep"),
        (CitationForm::Narrative, "citet"),
        (CitationForm::AuthorOnly, "citeauthor"),
        (CitationForm::YearOnly, "citeyearpar"),
    ] {
        let request: CitationRequest = serde_json::from_value(json!({
            "items":[{"item":item,"suffix":[{"text":"12"}]}],
            "form":form,"output":{"kind":"key","syntax":"natbib"}
        }))
        .unwrap();
        let result = engine.cite(&request, &cancel).unwrap();
        if form == CitationForm::AuthorOnly {
            assert_eq!(
                result.outcomes,
                [ItemOutcome::Refused {
                    item: 0,
                    error: CitationError::UnsupportedForm
                }]
            );
            assert!(result.copy_content(1).is_none());
        } else {
            assert_eq!(
                result.copy_content(1).unwrap().text,
                format!("\\{command}[12]{{Example2026}}")
            );
            assert!(result.losses.is_empty());
        }
    }
}
