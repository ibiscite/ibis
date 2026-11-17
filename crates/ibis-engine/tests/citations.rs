//! Citation sessions are tested as a client: submit, show, then commit.
#![cfg(test)]
use ibis_core::contract::*;
use ibis_engine::{Cancellation, Engine, citations::SessionId};
use serde_json::json;
use std::sync::Arc;

fn engine() -> Engine {
    let case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap();
    Engine::from_snapshot(Arc::new(case.library))
}
fn draft(engine: &Engine, output: &str) -> Draft {
    let snapshot = engine.snapshot().unwrap();
    let record = &snapshot.objects()[0];
    serde_json::from_value(json!({"items":[{"id":record.object.id(),"cite":record.fingerprints.cite}],"output":{"kind":"preset","name":output}})).unwrap()
}

#[test]
fn typing_fast_then_enter_cannot_commit_an_older_preview() {
    let engine = engine();
    let shown = engine
        .citations()
        .preview(SessionId::PICKER, draft(&engine, "pandoc"), Cancellation::default())
        .unwrap()
        .run()
        .unwrap();
    let delayed = engine
        .citations()
        .preview(SessionId::PICKER, draft(&engine, "apa"), Cancellation::default())
        .unwrap();
    assert!(
        engine
            .citations()
            .commit(SessionId::PICKER, &shown.ticket, &shown.content_hash)
            .is_err()
    );
    let newest = engine
        .citations()
        .preview(SessionId::PICKER, draft(&engine, "keys"), Cancellation::default())
        .unwrap()
        .run()
        .unwrap();
    assert!(delayed.run().is_err());
    let checked = engine
        .citations()
        .commit(SessionId::PICKER, &newest.ticket, &newest.content_hash)
        .unwrap();
    assert_eq!(checked.check(&engine).unwrap().content().text, newest.text);
}
#[test]
fn two_connections_do_not_retire_each_others_previews_and_close_releases_the_session() {
    let engine = engine();
    let first = engine.citations().open().unwrap();
    let second = engine.citations().open().unwrap();
    let shown = engine
        .citations()
        .preview(first, draft(&engine, "pandoc"), Cancellation::default())
        .unwrap()
        .run()
        .unwrap();
    engine
        .citations()
        .preview(second, draft(&engine, "apa"), Cancellation::default())
        .unwrap()
        .run()
        .unwrap();
    assert!(
        engine
            .citations()
            .commit(first, &shown.ticket, &shown.content_hash)
            .is_ok()
    );
    engine.citations().close(first);
    assert!(
        engine
            .citations()
            .commit(first, &shown.ticket, &shown.content_hash)
            .is_err()
    );
}
#[test]
fn changed_record_refuses_commit_even_when_the_text_is_identical() {
    let engine = engine();
    let shown = engine
        .citations()
        .preview(SessionId::PICKER, draft(&engine, "pandoc"), Cancellation::default())
        .unwrap()
        .run()
        .unwrap();
    let snapshot = engine.snapshot().unwrap();
    let mut objects: Vec<_> = snapshot.objects().iter().map(|object| object.object.clone()).collect();
    let LibraryObject::Record(record) = &mut objects[0] else {
        panic!("record")
    };
    record.csl.insert("title".into(), json!("Changed synthetic title"));
    let library = record.id.library.clone();
    engine
        .publish_library(
            library,
            ibis_engine::LibraryKind::Snapshot,
            Arc::new(Snapshot::new(objects).unwrap()),
        )
        .unwrap();
    assert!(
        engine
            .citations()
            .commit(SessionId::PICKER, &shown.ticket, &shown.content_hash)
            .is_err()
    );
    let fresh = engine
        .citations()
        .preview(SessionId::PICKER, draft(&engine, "pandoc"), Cancellation::default())
        .unwrap()
        .run()
        .unwrap();
    assert_eq!(fresh.text, shown.text);
}

#[test]
fn a_record_the_style_cannot_write_is_named_and_keeps_its_neighbours() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invented.bib");
    std::fs::write(&path,"@book{others2026,title={Synthetic signals},author={Taylor, Avery and others},year={2026}}\n@book{morgan2025,title={Synthetic noise},author={Morgan, Ann},year={2025}}\n").unwrap();
    let engine = Engine::new().unwrap().without_file_watches();
    engine.open_file_observed(&path, &Cancellation::default()).unwrap();
    let ids: Vec<_> = engine
        .snapshot()
        .unwrap()
        .objects()
        .iter()
        .map(|object| object.object.id().clone())
        .collect();
    let shown = engine
        .citations()
        .bibliography(
            SessionId::PICKER,
            ids.clone(),
            "apa".into(),
            Cancellation::default(),
            false,
        )
        .unwrap()
        .run()
        .unwrap()
        .unwrap();
    assert_eq!(shown.left_out, ["others2026"]);
    assert!(shown.text.contains("Morgan, A. (2025)"));
    assert!(!shown.text.contains("Taylor"));
    let checked = engine
        .citations()
        .commit_bibliography(
            SessionId::PICKER,
            &ids,
            "apa",
            &shown.content_hash,
            &Cancellation::default(),
        )
        .unwrap();
    assert_eq!(checked.check(&engine).unwrap().content().text, shown.text);
}

#[test]
fn a_full_registry_refuses_admission_without_evicting_a_connection() {
    let engine = engine();
    let ids: Vec<_> = (0..64).map(|_| engine.citations().open().unwrap()).collect();
    assert!(engine.citations().open().is_err());
    engine.citations().close(ids[0]);
    assert!(engine.citations().open().is_ok());
}

#[test]
fn golden_cases_have_the_same_preview_and_commit_through_the_session_api() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cases");
    let mut covered = 0;
    for path in std::fs::read_dir(root).unwrap() {
        let path = path.unwrap().path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let case: GoldenCase = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        if case.request.items.is_empty()
            || case.request.strict
            || case.request.items.iter().any(|pick| {
                pick.annotation.is_some()
                    || pick.suppress_author
                    || pick
                        .prefix
                        .iter()
                        .chain(&pick.suffix)
                        .any(|run| run.italic || run.bold || run.script != Script::Normal)
            })
        {
            continue;
        }
        let engine = Engine::from_snapshot(Arc::new(case.library));
        let expected = engine.cite(&case.request, &Cancellation::default());
        let request = Draft {
            items: case
                .request
                .items
                .iter()
                .map(|pick| DraftItem {
                    id: pick.item.clone(),
                    cite: engine
                        .snapshot()
                        .unwrap()
                        .get(&pick.item)
                        .map(|object| object.fingerprints.cite.clone())
                        .unwrap_or_else(|| Fingerprint::of_bytes(b"missing")),
                    locator: pick.locator.clone(),
                    locator_input: None,
                    form: pick.form,
                    prefix: (!pick.prefix.is_empty())
                        .then(|| pick.prefix.iter().map(|run| run.text.as_str()).collect()),
                    suffix: (!pick.suffix.is_empty())
                        .then(|| pick.suffix.iter().map(|run| run.text.as_str()).collect()),
                })
                .collect(),
            output: case.request.output,
            form: case.request.form,
            keep_order: case.request.keep_order,
            input_language: case.request.input_language,
            output_locale: case.request.output_locale,
            sample: false,
        };
        let preview = engine
            .citations()
            .preview(SessionId::PICKER, request, Cancellation::default())
            .unwrap()
            .run();
        if let Ok(expected) = expected
            && expected.copy_content(case.request.items.len()).is_some()
        {
            let preview = preview.unwrap_or_else(|error| panic!("{}: {error}", case.name));
            assert_eq!(preview.text, expected.text, "{}", case.name);
            assert_eq!(preview.display, expected.display, "{}", case.name);
            let checked = engine
                .citations()
                .commit(SessionId::PICKER, &preview.ticket, &preview.content_hash)
                .unwrap();
            let guard = checked.check(&engine).unwrap();
            assert_eq!(guard.content().text, expected.text);
            assert_eq!(guard.content().html, expected.html.as_deref());
            covered += 1;
        }
    }
    assert!(covered >= 5);
}

#[test]
fn delivery_refuses_a_key_that_became_ambiguous_after_commit() {
    let engine = engine();
    let shown = engine
        .citations()
        .preview(SessionId::PICKER, draft(&engine, "pandoc"), Cancellation::default())
        .unwrap()
        .run()
        .unwrap();
    let checked = engine
        .citations()
        .commit(SessionId::PICKER, &shown.ticket, &shown.content_hash)
        .unwrap();
    let snapshot = engine.snapshot().unwrap();
    let first = snapshot.objects()[0].object.clone();
    let mut other = first.clone();
    let LibraryObject::Record(record) = &mut other else {
        panic!("record")
    };
    record.id.key = "another-invented-record".into();
    let library = record.id.library.clone();
    engine
        .publish_library(
            library,
            ibis_engine::LibraryKind::Snapshot,
            Arc::new(Snapshot::new(vec![first, other]).unwrap()),
        )
        .unwrap();
    assert!(checked.check(&engine).is_err());
}

#[test]
fn citation_and_bibliography_delivery_allow_metadata_updates_and_refuse_changed_citable_data() {
    for bibliography in [false, true] {
        let engine = engine();
        let ids: Vec<_> = engine
            .snapshot()
            .unwrap()
            .objects()
            .iter()
            .map(|row| row.object.id().clone())
            .collect();
        let checked = if bibliography {
            let shown = engine
                .citations()
                .bibliography(
                    SessionId::PICKER,
                    ids.clone(),
                    "apa".into(),
                    Cancellation::default(),
                    false,
                )
                .unwrap()
                .run()
                .unwrap()
                .unwrap();
            engine
                .citations()
                .commit_bibliography(
                    SessionId::PICKER,
                    &ids,
                    "apa",
                    &shown.content_hash,
                    &Cancellation::default(),
                )
                .unwrap()
        } else {
            let shown = engine
                .citations()
                .preview(SessionId::PICKER, draft(&engine, "apa"), Cancellation::default())
                .unwrap()
                .run()
                .unwrap();
            engine
                .citations()
                .commit(SessionId::PICKER, &shown.ticket, &shown.content_hash)
                .unwrap()
        };
        let LibraryObject::Record(mut record) = engine.snapshot().unwrap().objects()[0].object.clone() else {
            panic!("record")
        };
        record
            .identifiers
            .push(ibis_core::identifier::Identifier::Arxiv("2601.00001".into()));
        let publish = |record: Record| {
            engine
                .publish_library(
                    record.id.library.clone(),
                    ibis_engine::LibraryKind::Snapshot,
                    Arc::new(Snapshot::new(vec![LibraryObject::Record(record)]).unwrap()),
                )
                .unwrap()
        };
        publish(record.clone());
        assert_eq!(checked.check(&engine).unwrap().content().text, checked.result().text);
        record.csl.insert("title".into(), json!("Changed invented title"));
        publish(record);
        assert!(checked.check(&engine).is_err(), "bibliography={bibliography}");
    }
}

#[test]
fn sharing_uses_the_session_preview_and_checked_commit() {
    let original = engine();
    let LibraryObject::Record(mut record) = original.snapshot().unwrap().objects()[0].object.clone() else {
        panic!("record")
    };
    record.csl.insert("DOI".into(), json!("10.1000/Invented"));
    record.csl.insert("note".into(), json!("Invented private note"));
    let engine = Engine::from_snapshot(Arc::new(Snapshot::new(vec![LibraryObject::Record(record)]).unwrap()));
    for output in ["bibtex", "doi-links"] {
        let shown = engine
            .citations()
            .preview(SessionId::PICKER, draft(&engine, output), Cancellation::default())
            .unwrap()
            .run()
            .unwrap();
        assert_eq!(shown.effective_output.as_deref(), Some(output));
        assert!(shown.text.contains("10.1000/Invented"));
        assert!(!shown.text.contains("Invented private note"));
        let checked = engine
            .citations()
            .commit(SessionId::PICKER, &shown.ticket, &shown.content_hash)
            .unwrap();
        assert_eq!(checked.check(&engine).unwrap().content().text, shown.text);
    }
}

#[test]
fn oversized_reference_fields_are_refused_before_formatting_without_echoing_them() {
    let engine = engine();
    for output in ["pandoc", "apa"] {
        let mut request = draft(&engine, output);
        let text = "x".repeat(ibis_core::syntax::MAX_CITATION_BYTES / 2 + 1);
        request.items[0].prefix = Some(text.clone());
        request.items[0].suffix = Some(text);
        let error = engine
            .citations()
            .preview(SessionId::PICKER, request, Cancellation::default())
            .unwrap()
            .run()
            .unwrap_err();
        assert_eq!(error.problem().info.recovery(), Recovery::ReduceSelection);
        assert_eq!(
            error.to_string(),
            "This citation has too much text. Shorten its fields or remove references."
        );
    }
}

/// Two invented works; only the second has a field that could not be converted.
fn two_records_the_second_warned() -> Engine {
    let LibraryObject::Record(first) = engine().snapshot().unwrap().objects()[0].object.clone() else {
        panic!("record")
    };
    let mut second = first.clone();
    second.id.key = "work-2".into();
    second.citation_keys[0].value = "Second2026".into();
    second.warnings = vec![CitationWarningCode::SourceField];
    let objects = vec![LibraryObject::Record(first), LibraryObject::Record(second)];
    Engine::from_snapshot(Arc::new(Snapshot::new(objects).unwrap()))
}
fn every_record(engine: &Engine, output: &str) -> Draft {
    let snapshot = engine.snapshot().unwrap();
    let items: Vec<_> = snapshot
        .objects()
        .iter()
        .map(|record| json!({"id": record.object.id(), "cite": record.fingerprints.cite}))
        .collect();
    serde_json::from_value(json!({"items": items, "output": {"kind": "preset", "name": output}})).unwrap()
}

#[test]
fn a_share_example_is_what_choosing_it_copies_and_keeps_the_shown_citation() {
    let engine = two_records_the_second_warned();
    let shown = engine
        .citations()
        .preview(
            SessionId::PICKER,
            every_record(&engine, "pandoc"),
            Cancellation::default(),
        )
        .unwrap()
        .run()
        .unwrap();
    let bibtex = OutputSpec::Preset { name: "bibtex".into() };
    let example = engine
        .citations()
        .example(
            SessionId::PICKER,
            &shown.ticket,
            &shown.content_hash,
            bibtex,
            Cancellation::default(),
        )
        .unwrap()
        .run()
        .unwrap()
        .unwrap();
    // The shown citation is still the one Enter copies.
    let checked = engine
        .citations()
        .commit(SessionId::PICKER, &shown.ticket, &shown.content_hash)
        .unwrap();
    assert_eq!(checked.check(&engine).unwrap().content().text, shown.text);
    drop(checked);
    let chosen = engine
        .citations()
        .preview(
            SessionId::PICKER,
            every_record(&engine, "bibtex"),
            Cancellation::default(),
        )
        .unwrap()
        .run()
        .unwrap();
    let copied = engine
        .citations()
        .commit(SessionId::PICKER, &chosen.ticket, &chosen.content_hash)
        .unwrap();
    assert_eq!(copied.check(&engine).unwrap().content().text, example);
    assert!(example.contains("Second2026"));
}
