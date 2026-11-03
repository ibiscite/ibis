//! Every official style and language, through the engine and its front ends:
//! a page of the list, the right language, refusals, order and restart.
#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use ibis_engine::{Cancellation, Engine, Method, mcp, request::*};
use serde_json::{Value, json};
use std::sync::Arc;

fn engine() -> (Engine, ItemRef) {
    let id = ItemRef {
        library: LibraryId::file(b"synthetic-styles"),
        key: "work".into(),
    };
    let csl = serde_json::from_value(json!({
        "type":"book","title":"Synthetic signals","publisher":"Imaginary Press",
        "author":[{"family":"Taylor","given":"Avery"},{"family":"Dale","given":"Morgan"}],
        "issued":{"date-parts":[[2026]]}
    }))
    .unwrap();
    let snapshot = Snapshot::new(vec![LibraryObject::Record(Record {
        added_at: None,
        id: id.clone(),
        csl,
        citation_keys: vec![],
        aliases: vec![],
        warnings: vec![],
        identifiers: vec![],
    })])
    .unwrap();
    (Engine::from_snapshot(Arc::new(snapshot)), id)
}

fn request(id: &ItemRef, style: &str, locale: Option<&str>) -> CitationRequest {
    serde_json::from_value(json!({
        "items":[{"item":id}],
        "output":{"kind":"csl","style":style},
        "output_locale":locale,
    }))
    .unwrap()
}

fn cite(engine: &Engine, id: &ItemRef, style: &str, locale: Option<&str>) -> Result<CitationResult, EngineError> {
    engine.cite(&request(id, style, locale), &Cancellation::default())
}

/// A journal style in another language, found in the data: the list is upstream's.
fn german_journal(engine: &Engine) -> ibis_engine::StyleInfo {
    let all = engine
        .styles(&StylesRequest {
            limit: 20_000,
            ..StylesRequest::default()
        })
        .unwrap();
    all.styles
        .into_iter()
        .find(|style| style.parent.is_some() && style.default_locale.as_deref() == Some("de-DE"))
        .unwrap()
}

#[test]
fn a_dependent_style_uses_its_parents_rules_and_its_own_language() {
    let (engine, id) = engine();
    let style = german_journal(&engine);
    let german = cite(&engine, &id, &style.id, None).unwrap();
    assert_eq!(german.language.as_deref(), Some("de-DE"));
    // Written directly, the parent is in its own language, not the journal's.
    let parent = cite(&engine, &id, style.parent.as_deref().unwrap(), None).unwrap();
    assert_ne!(parent.language, german.language);
    // A named language replaces the style's own.
    let french = cite(&engine, &id, &style.id, Some("fr-FR")).unwrap();
    assert_eq!(french.language.as_deref(), Some("fr-FR"));
    assert_ne!(french.text, german.text);
}

#[test]
fn the_output_says_its_language_and_a_key_syntax_has_none() {
    let (engine, id) = engine();
    let chinese = cite(&engine, &id, "china-national-standard-gb-t-7714-2025-author-date", None).unwrap();
    assert_eq!(chinese.language.as_deref(), Some("zh-CN"));
    assert!(chinese.text.contains('（'), "{}", chinese.text);
    assert_eq!(
        cite(&engine, &id, "apa", None).unwrap().language.as_deref(),
        Some("en-US")
    );
    assert_eq!(
        cite(&engine, &id, "apa", Some("de")).unwrap().language.as_deref(),
        Some("de-DE")
    );
}

#[test]
fn a_language_written_in_another_script_keeps_its_script() {
    let (engine, id) = engine();
    let mut request = request(&id, "chicago-notes-bibliography", Some("sr-Cyrl-RS"));
    request.form = CitationForm::BibliographyOnly;
    let cyrillic = engine.cite(&request, &Cancellation::default()).unwrap();
    assert_eq!(cyrillic.language.as_deref(), Some("sr-Cyrl-RS"));
    request.output_locale = Some("sr-Latn-RS".into());
    let latin = engine.cite(&request, &Cancellation::default()).unwrap();
    assert_ne!(cyrillic.text, latin.text);
}

#[test]
fn a_style_no_one_has_checked_refuses_to_omit_the_author() {
    let (engine, id) = engine();
    let mut request = request(&id, "nature", None);
    request.items[0].suppress_author = true;
    for style in ["the-open-university-a251", "chicago-notes-bibliography"] {
        request.output = OutputSpec::Csl { style: style.into() };
        let refused = engine.cite(&request, &Cancellation::default()).unwrap();
        assert!(refused.copy_content(1).is_none(), "{style}");
    }
    request.output = OutputSpec::Csl { style: "apa".into() };
    assert_eq!(engine.cite(&request, &Cancellation::default()).unwrap().text, "(2026)");
}

#[test]
fn a_style_without_a_document_says_what_it_printed() {
    let (engine, id) = engine();
    let numeric = cite(&engine, &id, "nature", None).unwrap();
    assert_eq!(numeric.notices, [Notice::NeedsDocumentContext, Notice::ReferenceEntry]);
    assert!(numeric.copy_content(1).is_some());
    let note = cite(&engine, &id, "chicago-notes-bibliography", None).unwrap();
    assert_eq!(note.notices, [Notice::NeedsDocumentContext, Notice::FullFirstNote]);
    assert!(note.text.contains("Synthetic Signals"), "{}", note.text);
}

#[test]
fn an_old_style_id_still_works_and_unknown_ids_are_refused_with_their_reason() {
    let (engine, id) = engine();
    assert!(cite(&engine, &id, "aaa", None).unwrap().copy_content(1).is_some());
    assert_eq!(
        cite(&engine, &id, "invented-style", None).unwrap_err(),
        EngineError::because(ErrorCode::BadInput, ErrorReason::UnknownStyle)
    );
    assert_eq!(
        cite(&engine, &id, "apa", Some("xx-INVENTED")).unwrap_err(),
        EngineError::because(ErrorCode::BadInput, ErrorReason::UnknownLocale)
    );
    // The refusal leaves the next request unaffected.
    assert!(cite(&engine, &id, "apa", None).unwrap().copy_content(1).is_some());
}

#[test]
fn the_list_is_paged_searched_and_bounded() {
    let (engine, _) = engine();
    let cancel = Cancellation::default();
    let first = engine.dispatch(Method::Styles, &json!({}), &cancel).unwrap().data;
    assert_eq!(first["styles"].as_array().unwrap().len(), 100);
    assert!(first["total"].as_u64().unwrap() > 10_000);
    assert_eq!(first["styles"][0]["id"], "apa");
    assert_eq!(first["styles"][0]["curated"], true);
    assert!(first["styles_release"]["tag"].as_str().unwrap().starts_with('v'));
    assert!(first["locales"].as_array().unwrap().len() > 50);
    let found = engine
        .dispatch(
            Method::Styles,
            &json!({"query":"gb/t 7714-2025","limit":2,"offset":1}),
            &cancel,
        )
        .unwrap()
        .data;
    assert_eq!(found["total"], 3);
    assert_eq!(found["styles"].as_array().unwrap().len(), 2);
    for refused in [
        json!({"limit":0}),
        json!({"limit":20_001}),
        json!({"query":"x".repeat(1025)}),
        json!({"unexpected":true}),
    ] {
        assert_eq!(
            engine.dispatch(Method::Styles, &refused, &cancel).unwrap_err().code,
            ErrorCode::BadInput
        );
    }
}

#[test]
fn a_slow_style_never_receives_a_later_styles_rules() {
    let (engine, id) = engine();
    let snapshot = engine.snapshot().unwrap();
    let formatter = ibis_csl::Formatter::new().unwrap();
    let first = formatter
        .submit(snapshot.clone(), request(&id, "nature", None))
        .unwrap();
    let second = formatter.submit(snapshot, request(&id, "apa", Some("de-DE"))).unwrap();
    // The later request is read first; each keeps the rules it was prepared with.
    assert_eq!(second.wait().unwrap().text, "(Taylor & Dale, 2026)");
    let first = first.wait().unwrap();
    assert_eq!(first.language.as_deref(), Some("en-GB"));
    assert!(first.text.contains("Imaginary Press"), "{}", first.text);
}

#[test]
fn a_bibliography_never_depends_on_what_the_engine_cited_before() {
    let (engine, id) = engine();
    let style = "cahiers-du-centre-gustave-glotz";
    let mut webpage = request(&id, style, None);
    webpage.form = CitationForm::BibliographyOnly;
    let snapshot = Arc::new(
        Snapshot::new(vec![LibraryObject::Record(Record {
            added_at: None,
            id: id.clone(),
            csl: serde_json::from_value(json!({
                "type":"webpage","title":"A page that does not exist",
                "author":[{"literal":"Institute of Examples"}],
                "URL":"https://example.org/invented",
                "accessed":{"date-parts":[[2026,1,2]]},"issued":{"date-parts":[[2025]]}
            }))
            .unwrap(),
            citation_keys: vec![],
            aliases: vec![],
            warnings: vec![],
            identifiers: vec![],
        })])
        .unwrap(),
    );
    let cold = ibis_csl::Formatter::new().unwrap().format(&snapshot, &webpage).unwrap();
    assert!(cold.text.contains("2026)"), "{}", cold.text);
    // A citation first leaves the processor's bracket state behind.
    let warm = ibis_csl::Formatter::new().unwrap();
    warm.format(&snapshot, &request(&id, style, None)).unwrap();
    assert_eq!(warm.format(&snapshot, &webpage).unwrap(), cold);
    drop(engine);
}

#[test]
fn a_cancelled_style_request_leaves_the_next_one_correct() {
    let (engine, id) = engine();
    let cancel = Cancellation::default();
    cancel.cancel();
    assert_eq!(
        engine.cite(&request(&id, "nature", None), &cancel).unwrap_err().code,
        ErrorCode::Cancelled
    );
    assert!(cite(&engine, &id, "nature", None).unwrap().copy_content(1).is_some());
}

#[test]
fn a_new_engine_gives_the_same_citation() {
    let style = {
        let (engine, _) = engine();
        german_journal(&engine).id
    };
    let (first, id) = engine();
    let before = cite(&first, &id, &style, None).unwrap();
    drop(first);
    let (reopened, _) = engine();
    assert_eq!(cite(&reopened, &id, &style, None).unwrap(), before);
}

#[test]
fn agents_see_a_page_and_the_language() {
    let (engine, id) = engine();
    let mut session = mcp::Session::default();
    let call = |session: &mut mcp::Session, name: &str, arguments: Value| {
        mcp::handle(
            &engine,
            session,
            &json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
                "name":name,"arguments":arguments,
                "_meta":{
                    "io.modelcontextprotocol/protocolVersion":mcp::PROTOCOL_VERSION,
                    "io.modelcontextprotocol/clientCapabilities":{}
                }
            }}),
        )
        .unwrap()["result"]
            .clone()
    };
    let page = call(&mut session, "styles", json!({"query":"nature","limit":3}));
    assert_eq!(page["isError"], false);
    assert_eq!(page["structuredContent"]["styles"].as_array().unwrap().len(), 3);
    assert!(page["structuredContent"]["total"].as_u64().unwrap() > 3);
    let all = call(&mut session, "styles", json!({"limit":mcp::MAX_STYLES}));
    assert_eq!(all["isError"], false, "the largest page fits one answer");
    assert_eq!(
        call(&mut session, "styles", json!({"limit":mcp::MAX_STYLES + 1}))["isError"],
        true
    );
    let reference = mcp::reference(&id);
    let cited = call(
        &mut session,
        "cite",
        json!({"items":[{"ref":reference}],"style":"apa","locale":"de-DE"}),
    );
    assert_eq!(cited["structuredContent"]["language"], "de-DE");
    let refused = call(
        &mut session,
        "cite",
        json!({"items":[{"ref":reference}],"style":"apa","locale":"xx-INVENTED"}),
    );
    assert_eq!(refused["isError"], true);
    assert!(
        refused["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("styles tool lists the locales")
    );
}
