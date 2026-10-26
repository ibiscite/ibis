use ibis_core::contract::*;
use ibis_engine::{Cancellation, CitationCommitError, Engine};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn case() -> Result<GoldenCase, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(include_str!(
        "../../../fixtures/cases/02-pandoc.json"
    ))?)
}
#[test]
fn changed_pick_never_reaches_the_native_commit() -> Result<(), Box<dyn std::error::Error>> {
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
            Arc::new(Snapshot::new(vec![LibraryObject::Record(record)])?),
        )
        .map_err(|error| format!("{:?}", error.code))?;
    let wrote = AtomicBool::new(false);
    let result = engine.commit_citation(&prepared, &prepared.result().content_hash, |_| {
        wrote.store(true, Ordering::SeqCst)
    });
    assert!(matches!(
        result,
        Err(CitationCommitError::Refused {
            item: 0,
            error: CitationError::ChangedItem
        })
    ));
    assert!(!wrote.load(Ordering::SeqCst));
    Ok(())
}

#[test]
fn commit_binds_preview_and_engine_but_allows_a_full_only_refresh() -> Result<(), Box<dyn std::error::Error>> {
    let mut case = case()?;
    case.request.output = OutputSpec::Csl { style: "apa".into() };
    let engine = Engine::from_snapshot(Arc::new(case.library.clone()));
    let prepared = engine
        .prepare_citation(&case.request, &Cancellation::default())
        .map_err(|error| format!("{:?}", error.code))?;
    let other = Engine::from_snapshot(Arc::new(case.library.clone()));
    for (owner, hash) in [
        (&engine, Fingerprint::of_bytes(b"different synthetic preview")),
        (&other, prepared.result().content_hash.clone()),
    ] {
        assert!(matches!(
            owner.commit_citation(&prepared, &hash, |_| panic!("invalid commit")),
            Err(CitationCommitError::PreviewChanged)
        ));
    }
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
        .publish_library(record.id.library, next)
        .map_err(|error| format!("{:?}", error.code))?;
    let text = engine.commit_citation(&prepared, &prepared.result().content_hash, |content| {
        content.text.to_owned()
    })?;
    assert_eq!(text, "(Taylor, 2026)");
    Ok(())
}
