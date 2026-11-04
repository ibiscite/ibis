#![allow(clippy::unwrap_used)]
use ibis_core::{
    contract::*,
    settings::{Settings, writing_projects},
};
use ibis_engine::{Cancellation, Engine, KeyCheck, LibraryKind, ProjectRead, request::KeyCitationRequest};
use serde_json::json;
use std::sync::Arc;

fn engine() -> Engine {
    let id = ItemRef {
        library: LibraryId::digest(b"invented-writing-project"),
        key: "book".into(),
    };
    let object = LibraryObject::Record(Record {
        id, added_at: None,
        csl: serde_json::from_value(json!({"type":"book","title":"Invented Atlas","author":[{"family":"Vale","given":"Ada"}],"issued":{"date-parts":[[2024]]}})).unwrap(),
        citation_keys: vec![CitationKey { value: "vale2024".into(), source: CitationKeySource::Bibtex }],
        aliases: vec![], warnings: vec![], identifiers: vec![],
    });
    Engine::from_snapshot(Arc::new(Snapshot::new(vec![object]).unwrap()))
}

#[test]
fn one_explicit_project_controls_output_and_checks_its_bibliography() {
    let engine = engine();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    std::fs::write(
        &path,
        "@book{vale2024,title={Invented Atlas},author={Vale, Ada},year={2024}}\n",
    )
    .unwrap();
    let mut settings = Settings::default();
    let project = writing_projects::create(
        &mut settings,
        "Atlas".into(),
        OutputSpec::Key {
            syntax: KeySyntax::Latex,
            command: Some("cite".into()),
        },
        Some(path),
    )
    .unwrap();
    let request: KeyCitationRequest =
        serde_json::from_value(json!({"keys":[{"key":"vale2024"}],"output":{"kind":"csl","style":"apa"}})).unwrap();
    let result = engine
        .cite_writing_project_keys(&project, &request, &Cancellation::default())
        .unwrap();
    assert_eq!(result.citation.text, "\\cite{vale2024}");
    assert_eq!(result.id, project.id);
    assert_eq!(result.revision, project.revision());
    assert!(matches!(
        result.project.as_ref().unwrap().keys[0].check,
        KeyCheck::In { .. }
    ));
    let json = serde_json::to_string(&result).unwrap();
    assert!(!json.contains(directory.path().to_str().unwrap()));
    let ordinary = engine.cite_keys(&request, &Cancellation::default()).unwrap();
    assert!(ordinary.text.contains("Vale"));
}

#[test]
fn prepared_bibliography_checks_keep_the_original_key_after_a_source_rename() {
    let engine = engine();
    let snapshot = engine.snapshot().unwrap();
    let LibraryObject::Record(mut renamed) = snapshot.objects()[0].object.clone() else {
        panic!("record");
    };
    let request: CitationRequest =
        serde_json::from_value(json!({"items":[{"item":renamed.id}],"output":{"kind":"key","syntax":"pandoc"}}))
            .unwrap();
    let prepared = engine.prepare_citation(&request, &Cancellation::default()).unwrap();
    renamed.citation_keys[0].value = "vale2025".into();
    engine
        .publish_library(
            renamed.id.library.clone(),
            LibraryKind::Snapshot,
            Arc::new(Snapshot::new(vec![LibraryObject::Record(renamed)]).unwrap()),
        )
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    std::fs::write(
        &path,
        "@book{vale2024,title={Invented Atlas},author={Vale, Ada},year={2024}}\n",
    )
    .unwrap();
    let checked = prepared.project_checks(&ProjectRead::read(&path, None));
    assert_eq!(checked[0].key, "vale2024");
    assert!(matches!(checked[0].check, KeyCheck::In { .. }));
    assert_eq!(prepared.result().text, "[@vale2024]");
}

#[test]
fn a_project_key_preset_checks_the_bibliography_for_its_canonical_output() {
    let engine = engine();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    std::fs::write(
        &path,
        "@book{vale2024,title={Invented Atlas},author={Vale, Ada},year={2024}}\n",
    )
    .unwrap();
    let mut settings = Settings::default();
    let project = writing_projects::create(
        &mut settings,
        "Atlas".into(),
        OutputSpec::Preset {
            name: "pandoc-bare".into(),
        },
        Some(path),
    )
    .unwrap();
    let request: KeyCitationRequest =
        serde_json::from_value(json!({"keys":[{"key":"vale2024"}],"output":{"kind":"csl","style":"apa"}})).unwrap();
    let result = engine
        .cite_writing_project_keys(&project, &request, &Cancellation::default())
        .unwrap();
    assert_eq!(result.citation.text, "@vale2024");
    assert!(matches!(result.project.unwrap().keys[0].check, KeyCheck::In { .. }));
}

#[test]
fn a_project_record_export_does_not_check_a_bibliography() {
    let engine = engine();
    let directory = tempfile::tempdir().unwrap();
    let mut settings = Settings::default();
    let project = writing_projects::create(
        &mut settings,
        "Atlas".into(),
        OutputSpec::Preset {
            name: "csl-json".into(),
        },
        Some(directory.path().join("unavailable.bib")),
    )
    .unwrap();
    let request: KeyCitationRequest =
        serde_json::from_value(json!({"keys":[{"key":"vale2024"}],"output":{"kind":"key","syntax":"pandoc"}})).unwrap();
    let result = engine
        .cite_writing_project_keys(&project, &request, &Cancellation::default())
        .unwrap();
    assert!(serde_json::from_str::<serde_json::Value>(&result.citation.text).is_ok());
    assert!(result.project.is_none());
}
