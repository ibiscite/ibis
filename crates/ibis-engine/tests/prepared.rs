use ibis_core::contract::*;
use ibis_engine::{Cancellation, CitationCommitError, Engine};
use serde_json::json;
use std::sync::Arc;

fn case() -> Result<GoldenCase, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(include_str!(
        "../../../fixtures/cases/02-pandoc.json"
    ))?)
}

#[test]
fn sample_provenance_labels_every_representation_and_binds_the_preview_hash() -> Result<(), Box<dyn std::error::Error>>
{
    let mut case = case()?;
    let engine = Engine::from_snapshot(Arc::new(case.library.clone()));
    for output in [case.request.output.clone(), OutputSpec::Csl { style: "apa".into() }] {
        case.request.output = output;
        let original = engine
            .prepare_citation(&case.request, &Cancellation::default())
            .map_err(|_| "prepare")?;
        let unmarked = original.result().clone();
        let sample = original.mark_sample().map_err(|_| "mark sample")?;
        let result = sample.result().clone();
        assert_eq!(result.text, format!("{} [Sample]", unmarked.text));
        assert_eq!(result.html.is_some(), unmarked.html.is_some());
        assert_eq!(result.rtf.is_some(), unmarked.rtf.is_some());
        assert!(result.content_matches_hash());
        assert_ne!(result.content_hash, unmarked.content_hash);
        let copied = engine.recite(&sample, &Cancellation::default())?;
        assert_eq!(copied, result, "a copy re-cites to exactly the marked preview");
        let content = copied.copy_content(1).ok_or("copy content")?;
        assert_eq!(content.text, result.text);
        if let Some(html) = content.html {
            assert!(html.ends_with("<span> [Sample]</span>"), "visible HTML marker");
        }
        if let Some(rtf) = content.rtf {
            assert!(rtf.starts_with("{\\rtf1"));
            assert!(
                rtf.ends_with(" {\\plain [Sample]}}"),
                "visible RTF marker inside the root group"
            );
        }
        let display_text: String = result
            .display
            .nodes
            .iter()
            .flat_map(|node| &node.content)
            .map(|run| run.text.as_str())
            .collect();
        assert!(display_text.ends_with(" [Sample]"));
        let sample = sample.mark_sample().map_err(|_| "repeat marker")?;
        assert_eq!(sample.result(), &result, "provenance is applied once");
        let normal = engine
            .prepare_citation(&case.request, &Cancellation::default())
            .map_err(|_| "normal citation")?;
        assert_eq!(
            normal.result(),
            &unmarked,
            "marking a prepared citation never changes the library"
        );
    }
    Ok(())
}

#[test]
fn labelled_samples_keep_the_source_change_guard_and_cannot_label_a_refusal() -> Result<(), Box<dyn std::error::Error>>
{
    let mut case = case()?;
    let engine = Engine::from_snapshot(Arc::new(case.library.clone()));
    let sample = engine
        .prepare_citation(&case.request, &Cancellation::default())
        .map_err(|_| "prepare")?
        .mark_sample()
        .map_err(|_| "mark sample")?;
    let LibraryObject::Record(mut record) = case.library.objects()[0].object.clone() else {
        return Err("record".into());
    };
    record.csl.insert("title".into(), json!("Changed invented title"));
    engine
        .publish_library(
            record.id.library.clone(),
            ibis_engine::LibraryKind::Snapshot,
            Arc::new(Snapshot::new(vec![LibraryObject::Record(record)])?),
        )
        .map_err(|_| "publish")?;
    assert!(matches!(
        engine.recite(&sample, &Cancellation::default()),
        Err(CitationCommitError::Refused {
            item: 0,
            error: CitationError::ChangedItem
        })
    ));
    // An unknown style is a wrong request: nothing is prepared to label.
    let mut unknown = case.request.clone();
    unknown.output = OutputSpec::Csl {
        style: "invented-unavailable-style".into(),
    };
    assert_eq!(
        engine.prepare_citation(&unknown, &Cancellation::default()).err(),
        Some(EngineError::because(ErrorCode::BadInput, ErrorReason::UnknownStyle))
    );
    case.request.items[0].item.key = "missing-synthetic-record".into();
    let refused = engine
        .prepare_citation(&case.request, &Cancellation::default())
        .map_err(|_| "prepare refusal")?;
    assert!(refused.result().copy_content(1).is_none());
    assert_eq!(
        refused.mark_sample().err().ok_or("refusal labelled")?.code,
        ErrorCode::BadInput
    );
    Ok(())
}
#[test]
fn changed_pick_is_refused_when_copied() -> Result<(), Box<dyn std::error::Error>> {
    let mut case = case()?;
    case.request.output = OutputSpec::Csl { style: "apa".into() };
    let engine = Engine::from_snapshot(Arc::new(case.library.clone()));
    let prepared = engine
        .prepare_citation(&case.request, &Cancellation::default())
        .map_err(|error| format!("{:?}", error.code))?;
    let LibraryObject::Record(mut record) = case.library.objects()[0].object.clone() else {
        return Err("record".into());
    };
    record.csl.insert("title".into(), json!("Changed synthetic title"));
    engine
        .publish_library(
            record.id.library.clone(),
            ibis_engine::LibraryKind::Snapshot,
            Arc::new(Snapshot::new(vec![LibraryObject::Record(record)])?),
        )
        .map_err(|error| format!("{:?}", error.code))?;
    assert!(matches!(
        engine.recite(&prepared, &Cancellation::default()),
        Err(CitationCommitError::Refused {
            item: 0,
            error: CitationError::ChangedItem
        })
    ));
    Ok(())
}

#[test]
fn a_copy_recites_against_the_engine_it_asks_and_allows_a_full_only_refresh() -> Result<(), Box<dyn std::error::Error>>
{
    let mut case = case()?;
    case.request.output = OutputSpec::Csl { style: "apa".into() };
    let engine = Engine::from_snapshot(Arc::new(case.library.clone()));
    let prepared = engine
        .prepare_citation(&case.request, &Cancellation::default())
        .map_err(|error| format!("{:?}", error.code))?;
    // Another engine without this reference cannot copy what this one showed.
    let other = Engine::from_snapshot(Arc::new(Snapshot::new(vec![])?));
    assert!(matches!(
        other.recite(&prepared, &Cancellation::default()),
        Err(CitationCommitError::Refused {
            item: 0,
            error: CitationError::MissingItem
        })
    ));
    let LibraryObject::Record(mut record) = case.library.objects()[0].object.clone() else {
        return Err("record".into());
    };
    record
        .identifiers
        .push(ibis_core::identifier::Identifier::Arxiv("2601.00001".into()));
    let next = Arc::new(Snapshot::new(vec![LibraryObject::Record(record.clone())])?);
    assert_ne!(next.fingerprints().full, case.library.fingerprints().full);
    assert_eq!(next.fingerprints().cite, case.library.fingerprints().cite);
    engine
        .publish_library(record.id.library, ibis_engine::LibraryKind::Snapshot, next)
        .map_err(|error| format!("{:?}", error.code))?;
    assert_eq!(
        engine.recite(&prepared, &Cancellation::default())?.text,
        "(Taylor, 2026)"
    );
    Ok(())
}

#[test]
fn key_collision_refuses_keys_but_not_formatted_citations() -> Result<(), Box<dyn std::error::Error>> {
    let mut case = case()?;
    let engine = Engine::from_snapshot(Arc::new(case.library.clone()));
    let key = engine
        .prepare_citation(&case.request, &Cancellation::default())
        .map_err(|_| "prepare")?;
    case.request.output = OutputSpec::Csl { style: "apa".into() };
    let formatted = engine
        .prepare_citation(&case.request, &Cancellation::default())
        .map_err(|_| "prepare")?;
    let LibraryObject::Record(mut other) = case.library.objects()[0].object.clone() else {
        return Err("record".into());
    };
    other.id.library = LibraryId::digest(b"another synthetic library");
    engine
        .publish_library(
            other.id.library.clone(),
            ibis_engine::LibraryKind::Snapshot,
            Arc::new(Snapshot::new(vec![LibraryObject::Record(other)])?),
        )
        .map_err(|_| "publish")?;
    assert!(matches!(
        engine.recite(&key, &Cancellation::default()),
        Err(CitationCommitError::Refused {
            item: 0,
            error: CitationError::AmbiguousKey
        })
    ));
    assert_eq!(
        engine.recite(&formatted, &Cancellation::default())?.text,
        "(Taylor, 2026)"
    );
    Ok(())
}

#[test]
fn formatted_zotero_picks_need_no_agreeing_key() -> Result<(), Box<dyn std::error::Error>> {
    let mut case = case()?;
    case.request.output = OutputSpec::Csl { style: "apa".into() };
    case.request.items[0].expected_cite = None;
    for conflicting in [false, true] {
        let LibraryObject::Record(mut record) = case.library.objects()[0].object.clone() else {
            return Err("record".into());
        };
        record.id.library = LibraryId::digest(b"synthetic keyless library");
        record.citation_keys.clear();
        if conflicting {
            record.citation_keys = vec![
                CitationKey {
                    value: "First2026".into(),
                    source: CitationKeySource::Extra,
                },
                CitationKey {
                    value: "Other2026".into(),
                    source: CitationKeySource::BetterBibtex,
                },
            ];
        }
        case.request.items[0].item = record.id.clone();
        let engine = Engine::from_snapshot(Arc::new(Snapshot::new(vec![LibraryObject::Record(record)])?));
        let prepared = engine
            .prepare_citation(&case.request, &Cancellation::default())
            .map_err(|_| "prepare")?;
        assert_eq!(
            engine.recite(&prepared, &Cancellation::default())?.text,
            "(Taylor, 2026)"
        );
    }
    Ok(())
}

#[test]
fn annotation_uses_printed_label_and_rechecks_its_file() -> Result<(), Box<dyn std::error::Error>> {
    let mut case = case()?;
    let record = case.library.objects()[0].object.clone();
    let id = case.request.items[0].item.clone();
    let mut attachment = Attachment {
        id: ItemRef {
            library: id.library.clone(),
            key: "synthetic-attachment".into(),
        },
        parent: Some(id.clone()),
        title: "Synthetic PDF".into(),
        media_type: "application/pdf".into(),
        availability: Availability::Unknown,
        content_hash: Some(Fingerprint::of_bytes(b"synthetic file one")),
    };
    let annotation = Annotation {
        id: ItemRef {
            library: id.library.clone(),
            key: "synthetic-annotation".into(),
        },
        parent: attachment.id.clone(),
        text: "Synthetic note".into(),
        comment: String::new(),
        page_label: Some("iv".into()),
        page_index: Some(12),
    };
    case.request.output = OutputSpec::Csl { style: "apa".into() };
    case.request.items[0].annotation = Some(annotation.id.clone());
    case.request.items[0].locator = Some(Locator {
        term: LocatorTerm::Page,
        value: "iv".into(),
    });
    let engine = Engine::from_snapshot(Arc::new(Snapshot::new(vec![
        record.clone(),
        LibraryObject::Attachment(attachment.clone()),
        LibraryObject::Annotation(annotation.clone()),
    ])?));
    let prepared = engine
        .prepare_citation(&case.request, &Cancellation::default())
        .map_err(|_| "prepare")?;
    assert!(prepared.result().copy_content(1).is_some());
    case.request.items[0].locator.as_mut().ok_or("locator")?.value = "13".into();
    let invalid = engine
        .prepare_citation(&case.request, &Cancellation::default())
        .map_err(|_| "prepare")?;
    assert_eq!(
        invalid.result().outcomes,
        vec![ItemOutcome::Refused {
            item: 0,
            error: CitationError::AnnotationMismatch
        }]
    );
    assert!(invalid.result().copy_content(1).is_none());
    attachment.content_hash = Some(Fingerprint::of_bytes(b"synthetic file two"));
    engine
        .publish_library(
            id.library,
            ibis_engine::LibraryKind::Snapshot,
            Arc::new(Snapshot::new(vec![
                record,
                LibraryObject::Attachment(attachment),
                LibraryObject::Annotation(annotation),
            ])?),
        )
        .map_err(|_| "publish")?;
    assert!(matches!(
        engine.recite(&prepared, &Cancellation::default()),
        Err(CitationCommitError::Refused {
            item: 0,
            error: CitationError::AnnotationMismatch
        })
    ));
    Ok(())
}
