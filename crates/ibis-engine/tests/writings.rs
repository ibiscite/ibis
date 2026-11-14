#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use ibis_engine::settings::{Settings, writings};
use ibis_engine::{
    Cancellation, Engine, KeyCheck, LibraryKind, ProjectAssessmentState as Assessment, ProjectRead,
    request::KeyCitationRequest,
};
use serde_json::json;
use std::sync::Arc;

fn engine() -> Engine {
    let id = ItemRef {
        library: LibraryId::digest(b"invented-writing"),
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
fn a_writing_without_a_bibliography_cites_without_requesting_setup() {
    let engine = engine();
    let writing = writings::create(
        &mut Settings::default(),
        "Atlas".into(),
        OutputSpec::Key {
            syntax: KeySyntax::Pandoc,
            command: None,
        },
        None,
    )
    .unwrap();
    let request: KeyCitationRequest =
        serde_json::from_value(json!({"keys":[{"key":"vale2024"}],"output":{"kind":"csl","style":"apa"}})).unwrap();
    let normal = engine
        .cite_writing_keys(&writing, &request, &Cancellation::default())
        .unwrap();
    assert_eq!(normal.citation.text, "[@vale2024]");
    assert!(normal.bibliography.is_none());
    let strict = engine
        .cite_writing_keys(
            &writing,
            &KeyCitationRequest {
                strict: true,
                ..request
            },
            &Cancellation::default(),
        )
        .unwrap_err();
    assert_eq!(
        strict,
        EngineError::because(ErrorCode::BadInput, ErrorReason::StrictCitation)
    );
}

#[test]
fn one_explicit_writing_controls_output_and_checks_its_bibliography() {
    let engine = engine();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    std::fs::write(
        &path,
        "@book{vale2024,title={Invented Atlas},author={Vale, Ada},year={2024}}\n",
    )
    .unwrap();
    let mut settings = Settings::default();
    let writing = writings::create(
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
        .cite_writing_keys(&writing, &request, &Cancellation::default())
        .unwrap();
    assert_eq!(result.citation.text, "\\cite{vale2024}");
    assert_eq!(result.name, writing.name);
    assert!(matches!(
        result.bibliography.as_ref().unwrap().keys[0].check,
        KeyCheck::In { .. }
    ));
    let json = serde_json::to_string(&result).unwrap();
    assert!(!json.contains(directory.path().to_str().unwrap()));
    let mut newer = serde_json::to_value(&result).unwrap();
    newer["added"] = json!(true);
    assert_eq!(
        serde_json::from_value::<ibis_engine::WritingCitation>(newer).unwrap(),
        result
    );
    let ordinary = engine.cite_keys(&request, &Cancellation::default()).unwrap();
    assert!(ordinary.text.contains("Vale"));
    let evidence = result.citation.receipt.as_ref().unwrap().project.as_ref().unwrap();
    assert!(!evidence.revision.is_empty());
    assert!(evidence.bibliography.is_some());
    assert!(evidence.checked_at_ms.unwrap() > 0);
    let mut renamed = writing.clone();
    renamed.name = "Revised Atlas".into();
    let renamed = engine
        .cite_writing_keys(&renamed, &request, &Cancellation::default())
        .unwrap();
    assert_ne!(
        renamed.citation.receipt.unwrap().project.unwrap().revision,
        evidence.revision
    );
    let strict = KeyCitationRequest {
        strict: true,
        ..request
    };
    assert!(
        engine
            .cite_writing_keys(&writing, &strict, &Cancellation::default())
            .is_ok()
    );
    std::fs::remove_file(writing.bibliography.as_ref().unwrap()).unwrap();
    assert_eq!(
        engine
            .cite_writing_keys(&writing, &strict, &Cancellation::default())
            .unwrap_err(),
        EngineError::because(ErrorCode::BadInput, ErrorReason::StrictCitation)
    );
    let unverified = engine
        .cite_writing_keys(
            &writing,
            &KeyCitationRequest {
                strict: false,
                ..strict
            },
            &Cancellation::default(),
        )
        .unwrap();
    assert_eq!(unverified.citation.text, "\\cite{vale2024}");
    let evidence = unverified.citation.receipt.unwrap().project.unwrap();
    assert_eq!(evidence.bibliography, None);
    assert_eq!(evidence.checked_at_ms, None);
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
fn a_writing_key_preset_checks_the_bibliography_for_its_canonical_output() {
    let engine = engine();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    std::fs::write(
        &path,
        "@book{vale2024,title={Invented Atlas},author={Vale, Ada},year={2024}}\n",
    )
    .unwrap();
    let mut settings = Settings::default();
    let writing = writings::create(
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
        .cite_writing_keys(&writing, &request, &Cancellation::default())
        .unwrap();
    assert_eq!(result.citation.text, "@vale2024");
    assert!(matches!(
        result.bibliography.unwrap().keys[0].check,
        KeyCheck::In { .. }
    ));
}

#[test]
fn a_writing_record_export_does_not_check_a_bibliography() {
    let engine = engine();
    let directory = tempfile::tempdir().unwrap();
    let mut settings = Settings::default();
    let writing = writings::create(
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
        .cite_writing_keys(&writing, &request, &Cancellation::default())
        .unwrap();
    assert!(serde_json::from_str::<serde_json::Value>(&result.citation.text).is_ok());
    assert!(result.bibliography.is_none());
}

#[test]
fn a_writing_doi_conflict_never_returns_copyable_key_output() {
    let engine = engine();
    let snapshot = engine.snapshot().unwrap();
    let LibraryObject::Record(mut record) = snapshot.objects()[0].object.clone() else {
        panic!("record");
    };
    record.csl.insert("DOI".into(), json!("10.1000/intended"));
    engine
        .publish_library(
            record.id.library.clone(),
            LibraryKind::Snapshot,
            Arc::new(Snapshot::new(vec![LibraryObject::Record(record)]).unwrap()),
        )
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    std::fs::write(&path, "@book{vale2024,title={Other work},doi={10.1000/other}}\n").unwrap();
    let writing = writings::create(
        &mut Settings::default(),
        "Atlas".into(),
        OutputSpec::Preset { name: "pandoc".into() },
        Some(path),
    )
    .unwrap();
    let request: KeyCitationRequest =
        serde_json::from_value(json!({"keys":[{"key":"vale2024"}],"output":{"kind":"key","syntax":"pandoc"}})).unwrap();
    let result = engine
        .cite_writing_keys(&writing, &request, &Cancellation::default())
        .unwrap();
    assert!(
        result.citation.copy_content(1).is_none(),
        "a known wrong-work key must never be copied"
    );
}

#[test]
fn an_unreadable_writing_bibliography_is_a_warning() {
    let engine = engine();
    let directory = tempfile::tempdir().unwrap();
    let writing = writings::create(
        &mut Settings::default(),
        "Atlas".into(),
        OutputSpec::Preset { name: "pandoc".into() },
        Some(directory.path().join("unavailable.bib")),
    )
    .unwrap();
    let request: KeyCitationRequest =
        serde_json::from_value(json!({"keys":[{"key":"vale2024"}],"output":{"kind":"key","syntax":"pandoc"}})).unwrap();
    let result = engine
        .cite_writing_keys(&writing, &request, &Cancellation::default())
        .unwrap();
    assert_eq!(result.citation.text, "[@vale2024]");
    assert_eq!(result.assessment.unwrap().state, Assessment::Ready);
    assert_eq!(result.bibliography.unwrap().keys[0].check, KeyCheck::CouldNotCheck);
}

fn prepared(engine: &Engine, strict: bool) -> ibis_engine::PreparedCitation {
    let item = engine.snapshot().unwrap().objects()[0].object.id().clone();
    let request: CitationRequest = serde_json::from_value(
        json!({"strict":strict,"items":[{"item":item}],"output":{"kind":"preset","name":"pandoc"}}),
    )
    .unwrap();
    engine.prepare_citation(&request, &Cancellation::default()).unwrap()
}

#[test]
fn ordinary_and_strict_requests_refuse_a_doi_conflict_in_a_damaged_file() {
    let engine = engine();
    let snapshot = engine.snapshot().unwrap();
    let LibraryObject::Record(mut record) = snapshot.objects()[0].object.clone() else {
        panic!("record");
    };
    record.csl.insert("DOI".into(), json!("10.1000/intended"));
    engine
        .publish_library(
            record.id.library.clone(),
            LibraryKind::Snapshot,
            Arc::new(Snapshot::new(vec![LibraryObject::Record(record)]).unwrap()),
        )
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    std::fs::write(
        &path,
        "@book{vale2024,title={Invented Atlas},doi={10.1000/other}}\n@book{broken,title={Never closed\n",
    )
    .unwrap();
    let conflicting = ProjectRead::read(&path, None);
    for strict in [false, true] {
        let prepared = prepared(&engine, strict);
        let assessment = prepared.project_evidence(&conflicting);
        assert_eq!(assessment.state, Assessment::Conflict);
        assert!(!assessment.can_copy());
    }
}

#[test]
fn a_duplicate_writing_name_requests_a_new_name_instead_of_retrying_the_same_save() {
    let problem = ibis_core::contract::Problem::from(&ibis_engine::settings::writings::WritingError::DuplicateName);
    let problem = serde_json::to_value(problem).unwrap_or_default();
    assert_eq!(problem["code"], "project-duplicate-name");
    assert_eq!(problem["message"]["id"], "app-project-error-duplicate-name");
    assert_eq!(problem["recovery"]["action"], "rename-project");
    assert_eq!(problem["focus"], "project-name");
}

#[test]
fn settings_contention_and_outside_edits_have_distinct_retryable_diagnostics() {
    use ibis_core::contract::{FocusTarget, Problem, Recovery};
    use ibis_engine::settings::SettingsError;
    for (error, code) in [
        (SettingsError::Busy, "settings-busy"),
        (SettingsError::Changed, "settings-changed"),
    ] {
        let problem = Problem::from(&error);
        assert_eq!(problem.code, code);
        assert_eq!(problem.info.recovery(), Recovery::Retry);
        assert_eq!(problem.info.recovery().focus(), FocusTarget::RecoveryAction);
        let text = ibis_engine::catalogue::problem(&problem, ibis_engine::catalogue::Audience::App);
        assert!(!text.is_empty() && !text.contains("app-settings-error"), "{text}");
    }
}
