#![cfg(not(debug_assertions))]
#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use ibis_engine::{Cancellation, Engine, Method, request::KeyCitationRequest};
use serde_json::json;
use std::time::{Duration, Instant};

#[test]
fn chicago_source_bibliography_and_warm_preview_meet_release_budgets() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("synthetic.bib");
    let bibliography: String = (0..400).map(|index| format!(
        "@book{{synthetic{index},title={{Synthetic signals {index}}},author={{Researcher{index}, Avery}},year={{2026}},publisher={{Imaginary Press}}}}\n"
    )).collect();
    std::fs::write(&path, bibliography).unwrap();
    let engine = Engine::new().unwrap();
    let cancel = Cancellation::default();
    engine
        .dispatch(Method::Libraries, &json!({"action":"open-file","path":path}), &cancel)
        .unwrap();
    let mut request = KeyCitationRequest {
        keys: (0..400).map(|index| format!("synthetic{index}").into()).collect(),
        output: OutputSpec::Csl {
            style: "chicago-author-date".into(),
        },
        form: CitationForm::BibliographyOnly,
        output_locale: None,
    };
    let start = Instant::now();
    let result = engine.cite_keys(&request, &cancel).unwrap();
    let batch = start.elapsed();
    assert!(result.copy_content(400).is_some());
    assert!(
        batch < Duration::from_secs(10),
        "400-entry Chicago bibliography exceeded 10s: {batch:?}"
    );

    request.keys.truncate(2);
    request.form = CitationForm::Normal;
    engine.cite_keys(&request, &cancel).unwrap();
    let mut samples = Vec::new();
    for _ in 0..20 {
        let start = Instant::now();
        assert!(engine.cite_keys(&request, &cancel).unwrap().copy_content(2).is_some());
        samples.push(start.elapsed());
    }
    samples.sort_unstable();
    let p95 = samples[18];
    assert!(
        p95 < Duration::from_millis(100),
        "warm two-item Chicago preview exceeded 100ms: {p95:?}"
    );
    println!(
        "CSL release budget: bibliography-400={}ms warm-two-p95={}ms",
        batch.as_millis(),
        p95.as_millis()
    );
}
