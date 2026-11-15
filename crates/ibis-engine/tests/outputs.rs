//! One list of outputs (#1260): each has one id and one name, says what it
//! writes, and lists exactly the forms the engine writes for it.
#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use ibis_engine::{Cancellation, Engine, Method, outputs};
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
fn listed_forms_are_exactly_the_forms_the_engine_writes() {
    let (engine, id) = engine_with_doi(true);
    let mut wrong = vec![];
    // Zotero's markers write nothing for this library; their forms are
    // tested in `ibis_core::syntax::scannable`.
    for output in outputs::outputs().iter().filter(|output| !output.zotero_items) {
        for form in CitationForm::ALL {
            // A form is listed when it can be written for one reference: with a
            // page, MLA's narrative form has something to write after the name;
            // without one, natbib's author-only form loses nothing.
            let written = [None, Some("12")].into_iter().any(|page| {
                let mut request = request(&[(id.clone(), None)], form, preset(&output.id));
                request.items[0].locator = page.map(|page| Locator {
                    term: LocatorTerm::Page,
                    value: page.into(),
                });
                let result = engine.cite(&request, &Cancellation::default()).unwrap();
                result.outcomes == [ItemOutcome::Rendered { item: 0 }]
            });
            if written != output.forms.contains(&form) {
                wrong.push((output.id.as_str(), form));
            }
        }
    }
    assert!(wrong.is_empty(), "listed forms differ from what is written: {wrong:#?}");
}

#[test]
fn a_narrative_biblatex_citation_is_written_with_textcite() {
    let (engine, id) = engine();
    let cite = |items: &[(ItemRef, Option<CitationForm>)], form, output: &str| {
        engine
            .cite(&request(items, form, preset(output)), &Cancellation::default())
            .unwrap()
    };
    let narrative = Some(CitationForm::Narrative);
    for output in ["autocite", "parencite"] {
        assert_eq!(
            cite(&[(id.clone(), None)], CitationForm::Narrative, output).text,
            "\\textcite{key}"
        );
        assert_eq!(
            cite(&[(id.clone(), narrative)], CitationForm::Normal, output).text,
            "\\textcite{key}"
        );
        // Narrative throughout, several references are one \textcites.
        let both = cite(
            &[(id.clone(), narrative), (id.clone(), narrative)],
            CitationForm::Normal,
            output,
        );
        assert_eq!(both.text, "\\textcites{key}{key}");
        // One command cannot mix forms: the narrative reference is refused, not rewritten.
        let mixed = cite(
            &[(id.clone(), None), (id.clone(), narrative)],
            CitationForm::Normal,
            output,
        );
        assert_eq!(
            mixed.outcomes[1],
            ItemOutcome::Refused {
                item: 1,
                error: CitationError::UnsupportedForm
            }
        );
        assert!(mixed.copy_content(2).is_none());
    }
    assert_eq!(
        cite(&[(id.clone(), None)], CitationForm::Normal, "autocite").text,
        "\\autocite{key}"
    );
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
fn the_list_is_an_engine_method_with_a_schema() {
    let listed = Engine::new()
        .unwrap()
        .dispatch(Method::Outputs, &json!({}), &Cancellation::default())
        .unwrap()
        .data;
    assert_eq!(listed, serde_json::to_value(outputs::list()).unwrap());
    assert_eq!(listed["default"], "pandoc");
    assert_eq!(listed["outputs"][0]["id"], "pandoc");
    assert_eq!(listed["outputs"][0]["name"], "Pandoc");
    assert_eq!(
        listed["outputs"][0]["output"],
        json!({"kind": "key", "syntax": "pandoc", "command": null})
    );
    assert!(
        Engine::new()
            .unwrap()
            .dispatch(Method::Outputs, &json!({"unknown": true}), &Cancellation::default())
            .is_err()
    );
}

#[test]
fn each_output_is_named_by_its_one_name() {
    let mut names = BTreeSet::new();
    for output in outputs::outputs() {
        // Short enough for the closed format control ("APA · Word"), and never twice.
        assert!(
            !output.name.is_empty() && output.name.chars().count() <= 24,
            "{}: {:?}",
            output.id,
            output.name
        );
        assert!(
            names.insert(output.name.to_lowercase()),
            "{:?} names two outputs",
            output.name
        );
        // Only a style has a description, and it is never the name again.
        assert!(
            output.description.is_none() || matches!(output.output, OutputSpec::Csl { .. }),
            "{}",
            output.id
        );
        assert_ne!(
            output.description.as_deref(),
            Some(output.name.as_str()),
            "{}",
            output.id
        );
        assert_eq!(outputs::named(&output.output).unwrap().id, output.id);
        assert_eq!(outputs::named(&preset(&output.id)).unwrap().id, output.id);
    }
    // A command left out means the syntax's own.
    let biblatex = OutputSpec::Key {
        syntax: KeySyntax::Biblatex,
        command: None,
    };
    assert_eq!(outputs::named(&biblatex).unwrap().id, "autocite");
    assert!(
        outputs::named(&OutputSpec::Csl {
            style: "elsevier-harvard".into()
        })
        .is_none()
    );
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
    let dispatched = DispatchResult::citation(result.clone(), 2, &output).unwrap();
    assert_eq!(dispatched.text.as_deref(), Some("https://doi.org/10.1000/invented"));
    assert_eq!(dispatched.exit_code, ExitCode::Findings);
    assert!(
        DispatchResult::citation(result.clone(), 3, &output)
            .unwrap()
            .text
            .is_none()
    );
    let mut changed = result.clone();
    changed.text.push_str("tampered");
    assert!(DispatchResult::citation(changed, 2, &output).unwrap().text.is_none());
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
        assert!(DispatchResult::citation(poisoned, 2, &output).unwrap().text.is_none());
    }
}
