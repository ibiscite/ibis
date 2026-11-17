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
        let receipt = result.receipt.as_ref().ok_or("sample receipt")?;
        assert_eq!(receipt.content_hash, result.content_hash);
        assert!(receipt.sample);
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
            ibis_engine::SourceCapability::SNAPSHOT,
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
            ibis_engine::SourceCapability::SNAPSHOT,
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
