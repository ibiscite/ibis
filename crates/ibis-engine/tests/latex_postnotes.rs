#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use ibis_engine::{Cancellation, Engine, Method};
use serde_json::json;

#[test]
fn bibtex_file_to_final_latex_keeps_a_numeric_suffix_literal() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("synthetic.bib");
    std::fs::write(
        &path,
        "@article{Example2026,title={Synthetic research},author={Taylor, Avery},year={2026}}",
    )
    .unwrap();
    let engine = Engine::new().unwrap();
    let cancel = Cancellation::default();
    let opened = engine
        .dispatch(Method::Libraries, &json!({"action":"open-file","path":path}), &cancel)
        .unwrap();
    assert_eq!(opened.exit_code, ExitCode::Ok);
    let item = engine.snapshot().unwrap().objects()[0].object.id().clone();
    for (syntax, expected) in [
        (KeySyntax::Biblatex, r"\autocite[12\nopp]{Example2026}"),
        (KeySyntax::Latex, r"\cite[12\relax]{Example2026}"),
    ] {
        let request: CitationRequest = serde_json::from_value(json!({
            "items":[{"item":item,"suffix":[{"text":"12"}]}],
            "output":{"kind":"key","syntax":syntax}
        }))
        .unwrap();
        let result = engine.cite(&request, &cancel).unwrap();
        assert_eq!(result.copy_content(1).unwrap().text, expected);
        assert!(result.losses.is_empty());
    }
}
