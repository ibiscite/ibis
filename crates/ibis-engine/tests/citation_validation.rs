//! Shared eligibility and precedence, observed through both output families.
#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use ibis_engine::{Cancellation, CitationCommitError, Engine, LibraryKind};
use serde_json::json;
use std::sync::Arc;

fn case() -> (CitationRequest, Vec<LibraryObject>) {
    let case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap();
    let mut request = case.request;
    request.items[0].expected_cite = None;
    let id = &request.items[0].item;
    let attachment = Attachment {
        id: ItemRef {
            library: id.library.clone(),
            key: "synthetic-file".into(),
        },
        parent: Some(id.clone()),
        title: "Synthetic file".into(),
        media_type: "application/pdf".into(),
        availability: Availability::Unknown,
        content_hash: Some(Fingerprint::of_bytes(b"file one")),
    };
    let annotation = Annotation {
        id: ItemRef {
            library: id.library.clone(),
            key: "synthetic-note".into(),
        },
        parent: attachment.id.clone(),
        text: "Synthetic note".into(),
        comment: String::new(),
        page_label: Some("iv".into()),
        page_index: Some(12),
    };
    (
        request,
        vec![
            case.library.objects()[0].object.clone(),
            LibraryObject::Attachment(attachment),
            LibraryObject::Annotation(annotation),
        ],
    )
}

fn annotate(pick: &mut CitationItem, objects: &[LibraryObject]) {
    pick.annotation = Some(objects[2].id().clone());
    pick.locator = Some(Locator {
        term: LocatorTerm::Page,
        value: "iv".into(),
    });
}

#[test]
fn whole_request_refusals_keep_their_diagnostic_for_each_output() {
    for output in ["pandoc", "apa"] {
        let (mut request, objects) = case();
        request.output = OutputSpec::Preset { name: output.into() };
        request.items.clear();
        let engine = Engine::from_snapshot(Arc::new(Snapshot::new(objects).unwrap()));
        let error = engine.cite(&request, &Cancellation::default()).unwrap_err();
        let presented = serde_json::to_value(error).unwrap();
        assert_eq!(presented["citation_error"], "empty-selection", "{output}");
        assert_eq!(presented["message"]["id"], "problem-empty-selection", "{output}");
        assert_eq!(presented["recovery"]["action"], "select-references", "{output}");
    }
}

#[test]
fn each_fault_has_the_same_refusal_and_precedence_in_pandoc_and_apa() {
    for output in ["pandoc", "apa"] {
        for (fault, expected) in [
            ("none", None),
            ("missing", Some(CitationError::MissingItem)),
            ("changed", Some(CitationError::ChangedItem)),
            ("changed-nonrecord", Some(CitationError::ChangedItem)),
            ("nonrecord", Some(CitationError::NotCitable)),
            ("annotation-missing", Some(CitationError::AnnotationMismatch)),
            ("annotation-changed", Some(CitationError::AnnotationMismatch)),
            ("annotation-parent", Some(CitationError::AnnotationMismatch)),
            ("annotation-page", Some(CitationError::AnnotationMismatch)),
            ("annotation-unbound", Some(CitationError::AnnotationMismatch)),
            ("locator", Some(CitationError::InvalidLocator)),
            ("form", Some(CitationError::UnsupportedForm)),
        ] {
            let (mut request, objects) = case();
            request.output = OutputSpec::Preset { name: output.into() };
            let good = request.items[0].clone();
            let mut bad = good.clone();
            // Combine earlier faults with later ones to prove refusal precedence.
            if fault != "none" {
                bad.form = Some(CitationForm::Narrative);
                bad.suppress_author = true;
            }
            if fault != "none" && fault != "form" {
                bad.locator = Some(Locator {
                    term: LocatorTerm::Page,
                    value: " ".into(),
                });
            }
            match fault {
                "missing" => {
                    bad.item.key = "absent".into();
                    bad.expected_cite = Some(Fingerprint::of_bytes(b"old"));
                    bad.annotation = Some(objects[2].id().clone());
                }
                "changed" | "changed-nonrecord" => {
                    if fault == "changed-nonrecord" {
                        bad.item = objects[1].id().clone();
                    }
                    bad.expected_cite = Some(Fingerprint::of_bytes(b"old"));
                    bad.annotation = Some(objects[2].id().clone());
                }
                "nonrecord" => {
                    bad.item = objects[1].id().clone();
                    bad.annotation = Some(objects[2].id().clone());
                }
                "annotation-missing" => {
                    bad.annotation = Some(ItemRef {
                        key: "absent".into(),
                        ..bad.item.clone()
                    });
                }
                "annotation-changed" => {
                    annotate(&mut bad, &objects);
                    bad.expected_annotation_cite = Some(Fingerprint::of_bytes(b"old note"));
                }
                "annotation-parent" => {
                    bad.annotation = Some(objects[1].id().clone());
                }
                "annotation-page" => {
                    annotate(&mut bad, &objects);
                    bad.locator.as_mut().unwrap().value = "13".into();
                }
                "annotation-unbound" => {
                    bad.expected_annotation_cite = Some(Fingerprint::of_bytes(b"unbound"));
                }
                _ => {}
            }
            request.items = vec![good.clone(), bad, good];
            let engine = Engine::from_snapshot(Arc::new(Snapshot::new(objects).unwrap()));
            let result = engine.cite(&request, &Cancellation::default()).unwrap();
            assert_eq!(
                result.outcomes,
                [
                    ItemOutcome::Rendered { item: 0 },
                    expected.map_or(ItemOutcome::Rendered { item: 1 }, |error| ItemOutcome::Refused {
                        item: 1,
                        error
                    }),
                    ItemOutcome::Rendered { item: 2 },
                ],
                "{output}: {fault}"
            );
            assert_eq!(
                result.copy_content(3).is_some(),
                expected.is_none(),
                "{output}: {fault}"
            );
            // A whole-request refusal wins over every item fault.
            request.input_language = Some("invalid_locale".into());
            assert_eq!(
                engine.cite(&request, &Cancellation::default()).unwrap_err().code,
                ErrorCode::BadInput
            );
        }
    }
}

#[test]
fn recite_refuses_changed_identity_or_annotation_meaning_even_with_identical_text() {
    for output in ["pandoc", "apa"] {
        for change in ["record", "annotation", "file"] {
            let (mut request, mut objects) = case();
            request.output = OutputSpec::Preset { name: output.into() };
            annotate(&mut request.items[0], &objects);
            let engine = Engine::from_snapshot(Arc::new(Snapshot::new(objects.clone()).unwrap()));
            let prepared = engine.prepare_citation(&request, &Cancellation::default()).unwrap();
            assert!(prepared.result().copy_content(1).is_some());
            match (&mut objects[..], change) {
                ([LibraryObject::Record(record), ..], "record") => {
                    record.csl.insert("publisher".into(), json!("Another invented press"));
                }
                ([_, _, LibraryObject::Annotation(annotation)], "annotation") => {
                    annotation.text = "Changed meaning".into();
                }
                ([_, LibraryObject::Attachment(attachment), _], "file") => {
                    attachment.content_hash = Some(Fingerprint::of_bytes(b"file two"));
                }
                _ => unreachable!(),
            }
            engine
                .publish_library(
                    request.items[0].item.library.clone(),
                    LibraryKind::Snapshot,
                    Arc::new(Snapshot::new(objects).unwrap()),
                )
                .unwrap();
            let fresh = engine.cite(&request, &Cancellation::default()).unwrap();
            assert_eq!(fresh.text, prepared.result().text, "{output}: {change}");
            assert!(fresh.copy_content(1).is_some());
            let expected = if change == "record" {
                CitationError::ChangedItem
            } else {
                CitationError::AnnotationMismatch
            };
            assert!(
                matches!(engine.recite(&prepared, &Cancellation::default()), Err(CitationCommitError::Refused { item: 0, error }) if error == expected),
                "{output}: {change}"
            );
        }
    }
}
