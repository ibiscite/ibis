#![allow(clippy::unwrap_used)]
//! Citation sessions are tested as a client: submit, show, then commit.
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
fn numeric_style_names_why_forms_are_unavailable() {
    let engine = engine();
    let shown = engine
        .citations()
        .preview(SessionId::PICKER, draft(&engine, "ieee"), Cancellation::default())
        .unwrap()
        .run()
        .unwrap();
    let details = engine
        .citations()
        .forms(
            SessionId::PICKER,
            &shown.ticket,
            &shown.content_hash,
            0,
            Cancellation::default(),
        )
        .unwrap()
        .run()
        .unwrap();
    assert_eq!(details.current_form, None);
    assert!(
        details
            .forms
            .iter()
            .all(|form| form.example.is_none() && form.reason.as_ref().is_some_and(|reason| !reason.is_empty()))
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
fn queued_bibliography_and_forms_are_retired_when_a_new_preview_is_submitted() {
    let engine = engine();
    let shown = engine
        .citations()
        .preview(SessionId::PICKER, draft(&engine, "apa"), Cancellation::default())
        .unwrap()
        .run()
        .unwrap();
    let forms = engine
        .citations()
        .forms(
            SessionId::PICKER,
            &shown.ticket,
            &shown.content_hash,
            0,
            Cancellation::default(),
        )
        .unwrap();
    let bibliography = engine
        .citations()
        .bibliography(
            SessionId::PICKER,
            vec![],
            "unknown-style".into(),
            Cancellation::default(),
            false,
        )
        .unwrap();
    let _next = engine
        .citations()
        .preview(SessionId::PICKER, draft(&engine, "keys"), Cancellation::default())
        .unwrap();
    assert!(forms.run().is_err());
    assert!(
        matches!(bibliography.run(),Err(ibis_engine::citations::SessionError::Engine(error)) if error.code==ErrorCode::Cancelled)
    );
}

#[test]
fn checked_content_waiting_for_the_main_thread_cannot_write_after_a_new_submission() {
    let engine = engine();
    let shown = engine
        .citations()
        .preview(SessionId::PICKER, draft(&engine, "apa"), Cancellation::default())
        .unwrap()
        .run()
        .unwrap();
    let checked = engine
        .citations()
        .commit(SessionId::PICKER, &shown.ticket, &shown.content_hash)
        .unwrap();
    let _newer = engine
        .citations()
        .preview(SessionId::PICKER, draft(&engine, "keys"), Cancellation::default())
        .unwrap();
    assert!(checked.check(&engine).is_err());
}

#[test]
fn a_closed_session_cannot_publish_late_work_or_retire_a_reopened_connection() {
    let engine = engine();
    let old = engine.citations().open().unwrap();
    let delayed = engine
        .citations()
        .preview(old, draft(&engine, "apa"), Cancellation::default())
        .unwrap();
    engine.citations().close(old);
    let new = engine.citations().open().unwrap();
    let shown = engine
        .citations()
        .preview(new, draft(&engine, "keys"), Cancellation::default())
        .unwrap()
        .run()
        .unwrap();
    assert!(delayed.run().is_err());
    assert!(
        engine
            .citations()
            .commit(new, &shown.ticket, &shown.content_hash)
            .is_ok()
    );
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
fn locator_fields_and_canonical_locators_give_the_same_shown_and_checked_content() {
    let engine = engine();
    for output in ["pandoc", "apa", "autocite", "textcite", "ieee"] {
        for (label, text) in [
            (LocatorTerm::Page, "12–15"),
            (LocatorTerm::Chapter, "3"),
            (LocatorTerm::Section, "2.1"),
        ] {
            let mut canonical = draft(&engine, output);
            canonical.items[0].locator = Some(Locator {
                term: label,
                value: text.into(),
            });
            let shown = engine
                .citations()
                .preview(SessionId::PICKER, canonical.clone(), Cancellation::default())
                .unwrap()
                .run()
                .unwrap();
            let mut typed = canonical;
            typed.items[0].locator = None;
            typed.items[0].locator_input = Some(LocatorInput {
                label,
                text: text.into(),
            });
            let typed = engine
                .citations()
                .preview(SessionId::PICKER, typed, Cancellation::default())
                .unwrap()
                .run()
                .unwrap();
            assert_eq!(shown.text, typed.text, "{output} {label:?}");
            let checked = engine
                .citations()
                .commit(SessionId::PICKER, &typed.ticket, &typed.content_hash)
                .unwrap();
            assert_eq!(checked.check(&engine).unwrap().content().text, typed.text);
        }
    }
    let mut wrong = draft(&engine, "apa");
    wrong.items[0].locator_input = Some(LocatorInput {
        label: LocatorTerm::Page,
        text: "p.".into(),
    });
    let error = engine
        .citations()
        .preview(SessionId::PICKER, wrong, Cancellation::default())
        .unwrap()
        .run()
        .unwrap_err();
<<<<<<< HEAD
    assert_eq!(error.problem().info.focus, FocusTarget::Locator);
=======
    assert_eq!(error.problem().info.recovery().focus(), FocusTarget::Locator);
>>>>>>> origin/main
}

#[test]
fn forms_keep_the_original_biblatex_family_and_true_year_only_examples() {
    let engine = engine();
    for output in ["apa", "autocite", "parencite"] {
        let mut request = draft(&engine, output);
        request.items[0].form = Some(CitationForm::Narrative);
        let shown = engine
            .citations()
            .preview(SessionId::PICKER, request, Cancellation::default())
            .unwrap()
            .run()
            .unwrap();
        let details = engine
            .citations()
            .forms(
                SessionId::PICKER,
                &shown.ticket,
                &shown.content_hash,
                0,
                Cancellation::default(),
            )
            .unwrap()
            .run()
            .unwrap();
        let normal = details
            .forms
            .iter()
            .find(|form| form.form == CitationForm::Normal)
            .unwrap();
        let year = details
            .forms
            .iter()
            .find(|form| form.form == CitationForm::YearOnly)
            .unwrap();
        if output == "apa" {
            assert_eq!(year.example.as_deref(), Some("(2026)"));
        } else {
            assert_eq!(
                normal.example.as_deref(),
                Some(format!("\\{output}{{Example2026}}").as_str())
            );
            assert_eq!(
                year.example.as_deref(),
                Some(format!("\\{output}*{{Example2026}}").as_str())
            );
        }
        assert!(
            engine
                .citations()
                .commit(SessionId::PICKER, &shown.ticket, &shown.content_hash)
                .is_ok()
        );
    }
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
fn bibliography_copy_refuses_when_a_selected_absent_record_is_restored() {
    let engine = engine();
    let snapshot = engine.snapshot().unwrap();
    let present = snapshot.objects()[0].object.clone();
    let mut restored = present.clone();
    let LibraryObject::Record(record) = &mut restored else {
        panic!("record")
    };
    record.id.key = "restored-invented-record".into();
    record.csl.insert("title".into(), json!("Restored invented reference"));
    let library = record.id.library.clone();
    let ids = vec![present.id().clone(), record.id.clone()];
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
    assert!(!shown.text.contains("Restored invented reference"));
    // The absent selection is harmless while it stays absent.
    assert!(
        engine
            .citations()
            .commit_bibliography(
                SessionId::PICKER,
                &ids,
                "apa",
                &shown.content_hash,
                &Cancellation::default(),
            )
            .is_ok()
    );
    engine
        .publish_library(
            library,
            ibis_engine::LibraryKind::Snapshot,
            Arc::new(Snapshot::new(vec![present, restored]).unwrap()),
        )
        .unwrap();
    let error = engine
        .citations()
        .commit_bibliography(
            SessionId::PICKER,
            &ids,
            "apa",
            &shown.content_hash,
            &Cancellation::default(),
        )
        .err()
        .unwrap();
    assert_eq!(error.problem(), CitationError::ChangedItem.problem());
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
fn a_missing_doi_keeps_its_typed_recovery_through_the_session() {
    let engine = engine();
    let problem = engine
        .citations()
        .preview(SessionId::PICKER, draft(&engine, "doi-links"), Cancellation::default())
        .unwrap()
        .run()
        .unwrap_err()
        .problem();
    assert_eq!(problem, CitationError::MissingDoi.problem());
<<<<<<< HEAD
    assert_eq!(problem.info.recovery, Recovery::ChooseForm);
    assert_eq!(problem.info.focus, FocusTarget::CitationForm);
=======
    assert_eq!(problem.info.recovery(), Recovery::ChooseOutput);
    assert_eq!(problem.info.recovery().focus(), FocusTarget::FormatPicker);
}

#[test]
fn all_refused_bibliography_offers_removing_references() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invented.bib");
    std::fs::write(
        &path,
        "@book{others2026,title={Invented signals},author={Taylor, Avery and others},year={2026}}",
    )
    .unwrap();
    let engine = Engine::new().unwrap().without_file_watches();
    engine.open_file_observed(&path, &Cancellation::default()).unwrap();
    let ids = engine
        .snapshot()
        .unwrap()
        .objects()
        .iter()
        .map(|object| object.object.id().clone())
        .collect();
    let error = engine
        .citations()
        .bibliography(SessionId::PICKER, ids, "apa".into(), Cancellation::default(), false)
        .unwrap()
        .run()
        .unwrap_err();
    let problem = error.problem();
    assert_eq!(problem.info.recovery(), Recovery::RemoveReferences);
    assert_eq!(problem.info.recovery().focus(), FocusTarget::SessionReferences);
    assert!(error.to_string().contains("others2026"));
    assert!(error.to_string().contains("Remove"));
}

#[test]
fn unsupported_group_forms_name_the_reference_and_offer_a_form_the_picker_can_choose() {
    let engine = engine();
    for output in ["pandoc", "autocite", "apa"] {
        let mut request = draft(&engine, output);
        let mut second = request.items[0].clone();
        second.form = Some(CitationForm::Narrative);
        request.items.push(second);
        let shown = engine
            .citations()
            .preview(SessionId::PICKER, request, Cancellation::default())
            .unwrap()
            .run()
            .unwrap();
        let failure = shown.failure.unwrap();
        assert_eq!(failure.info.recovery(), Recovery::ChooseForm);
        assert_eq!(failure.info.recovery().focus(), FocusTarget::CitationForm);
        let text = shown.problem.unwrap();
        assert!(text.contains("Reference 2"), "{text}");
        assert!(text.contains("Name in brackets"), "{text}");
        assert!(!text.contains("app-command-"), "{text}");
        assert!(
            engine
                .citations()
                .commit(SessionId::PICKER, &shown.ticket, &shown.content_hash)
                .is_err()
        );
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
>>>>>>> origin/main
}
