//! One list of outputs (#1260): each has one id and one name, says what it
//! writes, and lists exactly the forms the engine writes for it.
#![cfg(test)]
use ibis_core::contract::*;
use ibis_engine::{Cancellation, Engine, outputs};
use serde_json::json;
use std::collections::BTreeSet;
use std::sync::Arc;

/// The invented article each example is written for, under the key `key`.
fn engine() -> (Engine, ItemRef) {
    engine_with_doi(false)
}

fn engine_with_doi(doi: bool) -> (Engine, ItemRef) {
    let id = ItemRef {
        library: LibraryId::digest(b"synthetic-outputs"),
        key: "key".into(),
    };
    let mut csl: Csl = serde_json::from_value(json!({
        "type": "article-journal", "title": "Synthetic sleep study", "container-title": "Journal of Tests",
        "volume": "4", "author": [{"family": "García", "given": "Ana"}], "issued": {"date-parts": [[2021]]}
    }))
    .unwrap();
    if doi {
        csl.insert("DOI".into(), json!("10.1000/invented"));
    }
    let snapshot = Snapshot::new(vec![LibraryObject::Record(Record {
        added_at: None,
        id: id.clone(),
        csl,
        citation_keys: vec![CitationKey {
            value: "key".into(),
            source: CitationKeySource::Bibtex,
        }],
        aliases: vec![],
        warnings: vec![],
        identifiers: vec![],
    })])
    .unwrap();
    (Engine::from_snapshot(Arc::new(snapshot)), id)
}

fn request(items: &[(ItemRef, Option<CitationForm>)], form: CitationForm, output: OutputSpec) -> CitationRequest {
    CitationRequest {
        items: items
            .iter()
            .map(|(item, form)| CitationItem {
                item: item.clone(),
                expected_cite: None,
                form: *form,
                locator: None,
                prefix: vec![],
                suffix: vec![],
                suppress_author: false,
                annotation: None,
                expected_annotation_cite: None,
            })
            .collect(),
        form,
        keep_order: true,
        output,
        input_language: None,
        strict: false,
        output_locale: None,
    }
}

fn preset(id: &str) -> OutputSpec {
    OutputSpec::Preset { name: id.into() }
}

#[test]
fn every_output_has_one_id_and_one_name() {
    let list = outputs::outputs();
    let ids: BTreeSet<_> = list.iter().map(|output| output.id.as_str()).collect();
    let names: BTreeSet<_> = list.iter().map(|output| output.name.as_str()).collect();
    assert_eq!(ids.len(), list.len(), "ids are unique");
    assert_eq!(names.len(), list.len(), "names are unique");
    // Every key output the engine writes is listed, and every common style.
    assert_eq!(
        list.iter()
            .filter(|output| matches!(output.output, OutputSpec::Key { .. }))
            .count(),
        ibis_core::syntax::CAPABILITIES.len()
    );
    // A common style's one name is its short name; its full title is the quiet detail.
    for style in ibis_csl::curated().unwrap() {
        let listed = outputs::find(style.id()).unwrap();
        assert_eq!(listed.name, style.name());
        assert_eq!(
            listed.output,
            OutputSpec::Csl {
                style: style.id().into()
            }
        );
        let detail = (style.title() != style.name()).then_some(style.title());
        assert_eq!(listed.description.as_deref(), detail);
    }
    // The defaults are listed, and the default style is a style.
    let listed = outputs::list();
    assert!(outputs::find(&listed.default).is_some());
    assert!(matches!(
        outputs::find(&listed.default_style).unwrap().output,
        OutputSpec::Csl { .. }
    ));
}

#[test]
fn each_example_is_what_the_engine_writes() {
    let (engine, id) = engine();
    let mut wrong = vec![];
    for output in outputs::outputs() {
        let result = engine
            .cite(
                &request(&[(id.clone(), None)], CitationForm::Normal, preset(&output.id)),
                &Cancellation::default(),
            )
            .unwrap();
        // Zotero's markers name Zotero items: this library is not Zotero's,
        // so they refuse, saying why (written from Zotero in zotero_sources.rs).
        if output.zotero_items {
            assert_eq!(
                result.outcomes,
                [ItemOutcome::Refused {
                    item: 0,
                    error: CitationError::NotFromZotero
                }],
                "{}",
                output.id
            );
            assert_eq!(result.text, "");
            continue;
        }
        // Only a numbered style writes reference entries, and says so.
        assert_eq!(
            result.notices.contains(&Notice::ReferenceEntry),
            output.entries,
            "{}",
            output.id
        );
        match &output.example {
            Some(example) if *example != result.text => wrong.push((output.id.as_str(), result.text.clone())),
            Some(_) => {}
            None => assert!(output.exports_records, "{} has an example", output.id),
        }
        // The preset writes exactly what its own output writes.
        let direct = engine
            .cite(
                &request(&[(id.clone(), None)], CitationForm::Normal, output.output.clone()),
                &Cancellation::default(),
            )
            .unwrap();
        assert_eq!(result, direct, "{}", output.id);
    }
    assert!(wrong.is_empty(), "these outputs write something else: {wrong:#?}");
}

#[test]
fn an_unknown_id_is_refused_and_a_stored_one_fails_closed() {
    let (engine, id) = engine();
    for name in ["", "no-such-output", "Pandoc", "style:apa"] {
        assert_eq!(
            engine.cite(
                &request(&[(id.clone(), None)], CitationForm::Normal, preset(name)),
                &Cancellation::default()
            ),
            Err(EngineError::because(ErrorCode::BadInput, ErrorReason::UnknownOutput)),
            "{name:?}"
        );
        // A settings file naming an id this Ibis does not have uses the default.
        assert_eq!(outputs::or_default(Some(name)).id, outputs::DEFAULT);
    }
    assert_eq!(outputs::or_default(None).id, outputs::DEFAULT);
    assert_eq!(outputs::or_default(Some("typst")).id, "typst");
    assert_eq!(outputs::or_default(Some("ieee")).id, "ieee");
}

#[test]
fn partial_record_shares_keep_checked_text_but_never_bypass_representation_checks() {
    use ibis_engine::DispatchResult;
    let (engine, id) = engine_with_doi(true);
    let mut missing = id.clone();
    missing.key = "absent".into();
    let output = preset("doi-links");
    let result = engine
        .cite(
            &request(&[(id, None), (missing, None)], CitationForm::Normal, output.clone()),
            &Cancellation::default(),
        )
        .unwrap();
    let checked = |value| ibis_engine::health::Checked {
        value,
        source_health: ibis_engine::health::SourceHealth::of(std::iter::empty()),
    };
    let dispatched = DispatchResult::citation(checked(result.clone()), 2, &output).unwrap();
    assert_eq!(dispatched.text.as_deref(), Some("https://doi.org/10.1000/invented"));
    assert_eq!(dispatched.exit_code, ExitCode::Findings);
    assert!(
        DispatchResult::citation(checked(result.clone()), 3, &output)
            .unwrap()
            .text
            .is_none()
    );
    let mut changed = result.clone();
    changed.text.push_str("tampered");
    assert!(
        DispatchResult::citation(checked(changed), 2, &output)
            .unwrap()
            .text
            .is_none()
    );
    for (html, rtf) in [(Some("\0".into()), None), (None, Some("\0".into()))] {
        let poisoned = CitationResult::new(
            result.text.clone(),
            html,
            rtf,
            result.display.clone(),
            result.outcomes.clone(),
            vec![],
            vec![],
        );
        assert!(poisoned.content_matches_hash());
        assert!(
            DispatchResult::citation(checked(poisoned), 2, &output)
                .unwrap()
                .text
                .is_none()
        );
    }
}
