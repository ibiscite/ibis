#![allow(clippy::unwrap_used)]
use ibis_core::{
    contract::*,
    settings::{Settings, writing_projects},
};
use ibis_engine::{
    Cancellation, Engine, KeyCheck, LibraryKind, ProjectAssessmentState as Assessment, ProjectPolicy, ProjectRead,
    request::KeyCitationRequest,
};
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
fn a_strict_key_project_without_a_bibliography_cannot_claim_a_successful_check() {
    let engine = engine();
    let mut settings = Settings::default();
    let project = writing_projects::create(
        &mut settings,
        "Atlas".into(),
        OutputSpec::Key {
            syntax: KeySyntax::Pandoc,
            command: None,
        },
        None,
    )
    .unwrap();
    let request: KeyCitationRequest =
        serde_json::from_value(json!({"keys":[{"key":"vale2024"}],"output":{"kind":"key","syntax":"pandoc"}})).unwrap();
    let normal = engine
        .cite_writing_project_keys_with_policy(&project, &request, ProjectPolicy::Normal, &Cancellation::default())
        .unwrap();
    assert_eq!(normal.citation.text, "[@vale2024]");
    let strict = engine
        .cite_writing_project_keys_with_policy(&project, &request, ProjectPolicy::Strict, &Cancellation::default())
        .unwrap();
    assert!(strict.citation.text.is_empty());
    assert_eq!(strict.assessment.unwrap().state, Assessment::Paused);
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

#[test]
fn a_project_doi_conflict_never_returns_copyable_key_output() {
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
    let project = writing_projects::create(
        &mut Settings::default(),
        "Atlas".into(),
        OutputSpec::Preset { name: "pandoc".into() },
        Some(path),
    )
    .unwrap();
    let request: KeyCitationRequest =
        serde_json::from_value(json!({"keys":[{"key":"vale2024"}],"output":{"kind":"key","syntax":"pandoc"}})).unwrap();
    let result = engine
        .cite_writing_project_keys(&project, &request, &Cancellation::default())
        .unwrap();
    assert!(
        result.citation.copy_content(1).is_none(),
        "a known wrong-work key must never be copied"
    );
}

#[test]
fn without_bibliography_evidence_normal_project_citation_has_no_copyable_output() {
    let engine = engine();
    let directory = tempfile::tempdir().unwrap();
    let project = writing_projects::create(
        &mut Settings::default(),
        "Atlas".into(),
        OutputSpec::Preset { name: "pandoc".into() },
        Some(directory.path().join("unavailable.bib")),
    )
    .unwrap();
    let request: KeyCitationRequest =
        serde_json::from_value(json!({"keys":[{"key":"vale2024"}],"output":{"kind":"key","syntax":"pandoc"}})).unwrap();
    let result = engine
        .cite_writing_project_keys(&project, &request, &Cancellation::default())
        .unwrap();
    assert!(
        result.citation.copy_content(1).is_none(),
        "normal requests must pause without project evidence"
    );
}

fn prepared(engine: &Engine) -> ibis_engine::PreparedCitation {
    let item = engine.snapshot().unwrap().objects()[0].object.id().clone();
    let request: CitationRequest =
        serde_json::from_value(json!({"items":[{"item":item}],"output":{"kind":"preset","name":"pandoc"}})).unwrap();
    engine.prepare_citation(&request, &Cancellation::default()).unwrap()
}

#[test]
fn retained_matching_evidence_allows_normal_action_but_strict_freshness_pauses() {
    let engine = engine();
    let prepared = prepared(&engine);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    std::fs::write(
        &path,
        "@book{vale2024,title={Invented Atlas},author={Vale, Ada},year={2024}}\n",
    )
    .unwrap();
    let current = ProjectRead::read(&path, None);
    assert_eq!(
        prepared.project_evidence(&current, ProjectPolicy::Strict).state,
        Assessment::Ready
    );
    std::fs::remove_file(&path).unwrap();
    let retained = ProjectRead::read(&path, Some(&current));
    let normal = prepared.project_evidence(&retained, ProjectPolicy::Normal);
    assert_eq!(normal.state, Assessment::Retained);
    assert!(normal.can_copy());
    assert!(normal.warning().unwrap().contains("last checked copy"));
    assert_eq!(normal.last_checked_ms, current.last_checked_ms());
    assert_eq!(
        prepared.project_evidence(&retained, ProjectPolicy::Strict).state,
        Assessment::Paused
    );
    assert_eq!(
        prepared.result().text,
        "[@vale2024]",
        "assessing a pause never destroys a native preview"
    );
}

#[test]
fn key_presence_without_work_evidence_needs_an_explicit_unchecked_request() {
    let engine = engine();
    let prepared = prepared(&engine);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    std::fs::write(&path, "@book{vale2024,title={Invented Atlas}}\n").unwrap();
    let current = ProjectRead::read(&path, None);
    std::fs::remove_file(&path).unwrap();
    let retained = ProjectRead::read(&path, Some(&current));
    assert_eq!(
        prepared.project_evidence(&retained, ProjectPolicy::Normal).state,
        Assessment::Paused
    );
    let unchecked = prepared.project_evidence(&retained, ProjectPolicy::Unchecked);
    assert_eq!(unchecked.state, Assessment::Unverified);
    assert!(unchecked.can_copy());
    assert!(unchecked.warning().unwrap().contains("unverified"));
}

#[test]
fn every_policy_refuses_a_newer_incomplete_doi_conflict() {
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
    let prepared = prepared(&engine);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    std::fs::write(&path, "@book{vale2024,title={Invented Atlas},doi={10.1000/intended}}\n").unwrap();
    let good = ProjectRead::read(&path, None);
    std::fs::write(
        &path,
        "@book{vale2024,title={Invented Atlas},doi={10.1000/other}}\n@book{broken,title={Never closed\n",
    )
    .unwrap();
    let conflicting = ProjectRead::read(&path, Some(&good));
    for policy in [ProjectPolicy::Normal, ProjectPolicy::Unchecked, ProjectPolicy::Strict] {
        let assessment = prepared.project_evidence(&conflicting, policy);
        assert_eq!(assessment.state, Assessment::Conflict);
        assert!(!assessment.can_copy());
    }
}

#[test]
fn limited_conflict_history_cannot_be_bypassed_by_an_unchecked_action() {
    let engine = engine();
    let prepared = prepared(&engine);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    let matched = "@book{vale2024,title={Invented Atlas},author={Vale, Ada},year={2024}}\n";
    std::fs::write(&path, matched).unwrap();
    let good = ProjectRead::read(&path, None);
    let mut changed = (0..4100)
        .map(|index| format!("@book{{other{index},doi={{10.1000/other{index}}}}}\n"))
        .collect::<String>();
    changed.push_str("@book{broken,title={Never closed\n");
    std::fs::write(&path, changed).unwrap();
    let limited = ProjectRead::read(&path, Some(&good));
    assert!(limited.evidence_limited());
    assert!(!prepared.project_evidence(&limited, ProjectPolicy::Unchecked).can_copy());
    std::fs::write(&path, matched).unwrap();
    let recovered = ProjectRead::read(&path, Some(&limited));
    assert!(!recovered.evidence_limited());
    assert_eq!(
        prepared.project_evidence(&recovered, ProjectPolicy::Normal).state,
        Assessment::Ready
    );
}

#[test]
fn duplicate_partial_work_identities_pause_every_policy_in_both_orders_and_after_failure() {
    let engine = engine();
    let prepared = prepared(&engine);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    let matched = "@book{vale2024,title={Invented Atlas},author={Vale, Ada},year={2024},doi={10.1000/intended}}\n";
    let other = matched.replace("10.1000/intended", "10.1000/other");
    for (first, second) in [(matched, other.as_str()), (other.as_str(), matched)] {
        std::fs::write(&path, matched).unwrap();
        let good = ProjectRead::read(&path, None);
        std::fs::write(&path, format!("{first}{second}@book{{broken,title={{Never closed\n")).unwrap();
        let partial = ProjectRead::read(&path, Some(&good));
        for policy in [ProjectPolicy::Normal, ProjectPolicy::Unchecked, ProjectPolicy::Strict] {
            let assessment = prepared.project_evidence(&partial, policy);
            assert_eq!(
                assessment.state,
                Assessment::Paused,
                "deduplicated identities are uncertainty, not a proven selected-work mismatch"
            );
            assert!(!assessment.can_copy());
        }
        std::fs::remove_file(&path).unwrap();
        let missing = ProjectRead::read(&path, Some(&partial));
        assert!(!prepared.project_evidence(&missing, ProjectPolicy::Unchecked).can_copy());
    }
}

#[test]
fn a_large_observation_key_cannot_turn_budget_exhaustion_into_an_unchecked_escape() {
    let engine = engine();
    let prepared = prepared(&engine);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    let matched = "@book{vale2024,title={Invented Atlas},author={Vale, Ada},year={2024}}\n";
    std::fs::write(&path, matched).unwrap();
    let good = ProjectRead::read(&path, None);
    let key = "k".repeat(256 * 1024 + 1);
    std::fs::write(
        &path,
        format!("@book{{{key},doi={{10.1000/other}}}}\n@book{{broken,title={{Never closed\n"),
    )
    .unwrap();
    let limited = ProjectRead::read(&path, Some(&good));
    assert!(!prepared.project_evidence(&limited, ProjectPolicy::Unchecked).can_copy());
}

#[test]
fn an_unchecked_project_policy_never_changes_an_original_source_refusal() {
    let engine = engine();
    let item = engine.snapshot().unwrap().objects()[0].object.id().clone();
    let directory = tempfile::tempdir().unwrap();
    let project = writing_projects::create(
        &mut Settings::default(),
        "Atlas".into(),
        OutputSpec::Preset { name: "pandoc".into() },
        Some(directory.path().join("unavailable.bib")),
    )
    .unwrap();
    let request: CitationRequest = serde_json::from_value(json!({"items":[{"item":item,"expected_cite":Fingerprint::of_bytes(b"obsolete record")}],"output":{"kind":"preset","name":"pandoc"}})).unwrap();
    let result = engine
        .cite_writing_project_with_policy(&project, &request, ProjectPolicy::Unchecked, &Cancellation::default())
        .unwrap();
    assert!(result.citation.copy_content(1).is_none());
    assert!(matches!(
        result.citation.outcomes[0],
        ItemOutcome::Refused {
            error: CitationError::ChangedItem,
            ..
        }
    ));
}

#[test]
fn a_complete_duplicate_file_never_releases_a_known_conflict_in_either_order() {
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
    let prepared = prepared(&engine);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    let matched = "@book{vale2024,title={Invented Atlas},author={Vale, Ada},year={2024},doi={10.1000/intended}}\n";
    let other = matched.replace("10.1000/intended", "10.1000/other");
    for (first, second) in [(matched, other.as_str()), (other.as_str(), matched)] {
        std::fs::write(&path, matched).unwrap();
        let good = ProjectRead::read(&path, None);
        std::fs::write(&path, format!("{other}@book{{broken,title={{Never closed\n")).unwrap();
        let known = ProjectRead::read(&path, Some(&good));
        std::fs::write(&path, format!("{first}{second}")).unwrap();
        let duplicates = ProjectRead::read(&path, Some(&known));
        for policy in [ProjectPolicy::Normal, ProjectPolicy::Unchecked, ProjectPolicy::Strict] {
            assert_eq!(
                prepared.project_evidence(&duplicates, policy).state,
                Assessment::Conflict
            );
        }
        std::fs::remove_file(&path).unwrap();
        let missing = ProjectRead::read(&path, Some(&duplicates));
        assert!(!prepared.project_evidence(&missing, ProjectPolicy::Unchecked).can_copy());
        std::fs::write(&path, matched).unwrap();
        let repaired = ProjectRead::read(&path, Some(&missing));
        assert_eq!(
            prepared.project_evidence(&repaired, ProjectPolicy::Normal).state,
            Assessment::Ready
        );
    }
}

#[test]
fn complete_conflict_snapshots_and_new_duplicate_mappings_refuse_every_policy() {
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
    let prepared = prepared(&engine);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("refs.bib");
    let matched = "@book{vale2024,title={Invented Atlas},author={Vale, Ada},year={2024},doi={10.1000/intended}}\n";
    let other = matched.replace("10.1000/intended", "10.1000/other");
    for previous_conflict in [true, false] {
        for (first, second) in [(matched, other.as_str()), (other.as_str(), matched)] {
            let previous = if previous_conflict {
                std::fs::write(&path, &other).unwrap();
                let conflict = ProjectRead::read(&path, None);
                assert_eq!(
                    prepared.project_evidence(&conflict, ProjectPolicy::Normal).state,
                    Assessment::Conflict
                );
                Some(conflict)
            } else {
                None
            };
            std::fs::write(&path, format!("{first}{second}")).unwrap();
            let duplicates = ProjectRead::read(&path, previous.as_ref());
            for policy in [ProjectPolicy::Normal, ProjectPolicy::Unchecked, ProjectPolicy::Strict] {
                let assessment = prepared.project_evidence(&duplicates, policy);
                assert!(!assessment.can_copy(), "duplicates cannot become a policy bypass");
                let message = assessment.message.unwrap();
                assert!(
                    !message.contains("incomplete"),
                    "a complete duplicated file needs a truthful identity-evidence message"
                );
            }
            std::fs::remove_file(&path).unwrap();
            let failed = ProjectRead::read(&path, Some(&duplicates));
            assert!(!prepared.project_evidence(&failed, ProjectPolicy::Unchecked).can_copy());
            std::fs::write(&path, format!("{first}{second}")).unwrap();
            let same_duplicates = ProjectRead::read(&path, Some(&failed));
            assert!(
                !prepared
                    .project_evidence(&same_duplicates, ProjectPolicy::Unchecked)
                    .can_copy()
            );
            std::fs::write(&path, matched).unwrap();
            let repaired = ProjectRead::read(&path, Some(&same_duplicates));
            assert_eq!(
                prepared.project_evidence(&repaired, ProjectPolicy::Normal).state,
                Assessment::Ready
            );
        }
    }
}
