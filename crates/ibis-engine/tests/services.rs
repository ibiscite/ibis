#![cfg(test)]
use ibis_core::contract::*;
use ibis_engine::{Cancellation, Engine, Method, request::*};
use serde_json::json;
use std::sync::Arc;

fn case() -> GoldenCase {
    serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap()
}
#[test]
fn typed_methods_and_dispatch_share_one_snapshot() {
    let case = case();
    let engine = Engine::from_snapshot(Arc::new(case.library.clone()));
    let cancel = Cancellation::default();
    let search = SearchRequest {
        interpret_locator: false,
        query: "signals".into(),
        limit: 10,
        libraries: None,
    };
    let typed = engine.search(&search, &cancel).unwrap();
    let dispatched = engine
        .dispatch(Method::Search, &serde_json::to_value(search).unwrap(), &cancel)
        .unwrap();
    assert_eq!(serde_json::to_value(typed).unwrap(), dispatched.data);
    let shown = engine
        .show(&ShowRequest {
            item: ItemSelector::Key("Example2026".into()),
        })
        .unwrap();
    assert_eq!(shown, *case.library.objects()[0]);
    let mut cited = engine.cite(&case.request, &cancel).unwrap();
    cited.receipt = None; // Formatting seeds predate request-specific evidence.
    assert_eq!(cited, *case.expected.data().unwrap());
    assert_eq!(engine.libraries().unwrap().libraries.len(), 1);
    assert_eq!(
        engine
            .dispatch(Method::Call, &json!({"method":"call","params":null}), &cancel)
            .unwrap_err()
            .code,
        ErrorCode::BadInput
    );
    assert_eq!(
        engine
            .dispatch(Method::Search, &json!({"query":"x","private":"payload"}), &cancel)
            .unwrap_err()
            .code,
        ErrorCode::BadInput
    );
    cancel.cancel();
    assert_eq!(
        engine
            .search(
                &SearchRequest {
                    interpret_locator: false,
                    query: "x".into(),
                    limit: 1,
                    libraries: None
                },
                &cancel
            )
            .unwrap_err()
            .code,
        ErrorCode::Cancelled
    );
}

#[test]
fn pasted_search_is_ordered_unlimited_and_reports_missing_keys() {
    let case = case();
    let LibraryObject::Record(base) = case.library.objects()[0].object.clone() else {
        panic!("record")
    };
    let objects = (0..205)
        .map(|n| {
            let mut record = base.clone();
            record.id.key = format!("key{n}");
            record.citation_keys[0].value = record.id.key.clone();
            LibraryObject::Record(record)
        })
        .collect();
    let engine = Engine::from_snapshot(Arc::new(Snapshot::new(objects).unwrap()));
    let query = (0..205).rev().map(|n| format!("@key{n}")).collect::<Vec<_>>().join(" ");
    let request = SearchRequest {
        query: format!("{query} @absent"),
        limit: 1,
        libraries: None,
        interpret_locator: true,
    };
    let cancel = Cancellation::default();
    let reply = engine.search(&request, &cancel).unwrap();
    assert!(reply.complete);
    assert_eq!(reply.hits.len(), 205);
    assert_eq!(reply.hits[0].item.object.id().key, "key204");
    assert_eq!(reply.hits[204].item.object.id().key, "key0");
    assert_eq!(reply.list.as_ref().unwrap().missing, ["absent"]);
    assert_eq!(
        serde_json::to_value(reply).unwrap(),
        engine
            .dispatch(Method::Search, &serde_json::to_value(&request).unwrap(), &cancel)
            .unwrap()
            .data
    );
    let empty = engine
        .search(
            &SearchRequest {
                libraries: Some(vec![]),
                ..request
            },
            &cancel,
        )
        .unwrap();
    assert!(empty.hits.is_empty());
    assert_eq!(empty.list.unwrap().missing.len(), 206);
}

#[test]
fn refresh_keeps_old_snapshots_and_refuses_ambiguous_keys() {
    let case = case();
    let engine = Engine::from_snapshot(Arc::new(case.library.clone()));
    let old = engine.snapshot().unwrap();
    let LibraryObject::Record(mut record) = old.objects()[0].object.clone() else {
        panic!("record")
    };
    let library = LibraryId::digest(b"another synthetic library");
    record.id.library = library.clone();
    let second = Arc::new(Snapshot::new(vec![LibraryObject::Record(record)]).unwrap());
    engine
        .publish_library(&ibis_sources::LoadedLibrary::new(
            library.clone(),
            ibis_engine::LibraryKind::Snapshot,
            ibis_engine::SourceCapability::SNAPSHOT,
            (*second).clone(),
        ))
        .unwrap();
    assert_eq!(old.objects().len(), 1);
    assert_eq!(engine.snapshot().unwrap().objects().len(), 2);
    assert_eq!(
        engine
            .show(&ShowRequest {
                item: ItemSelector::Key("Example2026".into())
            })
            .unwrap_err()
            .code,
        ErrorCode::BadInput
    );
    engine
        .publish_library(&ibis_sources::LoadedLibrary::new(
            library,
            ibis_engine::LibraryKind::Snapshot,
            ibis_engine::SourceCapability::SNAPSHOT,
            Snapshot::new(vec![]).unwrap(),
        ))
        .unwrap();
    assert!(
        engine
            .show(&ShowRequest {
                item: ItemSelector::Key("Example2026".into())
            })
            .is_ok()
    );
}

#[test]
fn a_named_missing_library_is_not_silently_an_empty_search() {
    let engine = Engine::new().unwrap();
    let mut request = SearchRequest {
        interpret_locator: false,
        query: "synthetic".into(),
        limit: 10,
        libraries: Some(vec![LibraryId::digest(b"missing synthetic scope")]),
    };
    assert_eq!(
        engine.search(&request, &Cancellation::default()).unwrap_err().code,
        ErrorCode::NotFound
    );
    request.libraries = Some(vec![]);
    assert!(
        engine
            .search(&request, &Cancellation::default())
            .unwrap()
            .hits
            .is_empty()
    );
}

#[test]
fn a_slow_subscriber_wakes_to_current_state_without_blocking_publications() {
    let engine = Engine::new().unwrap();
    let events = engine.subscribe().unwrap();
    let newest = Arc::new(case().library);
    let library = newest.objects()[0].object.id().library.clone();
    let publishing = engine.clone();
    let latest = newest.clone();
    let (completed, done) = std::sync::mpsc::channel();
    let publisher = std::thread::spawn(move || {
        for _ in 0..40 {
            publishing
                .publish_library(&ibis_sources::LoadedLibrary::new(
                    library.clone(),
                    ibis_engine::LibraryKind::Snapshot,
                    ibis_engine::SourceCapability::SNAPSHOT,
                    Snapshot::new(vec![]).unwrap(),
                ))
                .unwrap();
        }
        publishing
            .publish_library(&ibis_sources::LoadedLibrary::new(
                library,
                ibis_engine::LibraryKind::Snapshot,
                ibis_engine::SourceCapability::SNAPSHOT,
                (*latest).clone(),
            ))
            .unwrap();
        completed.send(()).unwrap();
    });
    // The publisher must finish while the subscriber leaves its single wake queued.
    done.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
    publisher.join().unwrap();
    events.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
    assert_eq!(engine.snapshot().unwrap().fingerprints(), newest.fingerprints());
    assert!(matches!(events.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)));
    engine
        .publish_library(&ibis_sources::LoadedLibrary::new(
            newest.objects()[0].object.id().library.clone(),
            ibis_engine::LibraryKind::Snapshot,
            ibis_engine::SourceCapability::SNAPSHOT,
            Snapshot::new(vec![]).unwrap(),
        ))
        .unwrap();
    events.try_recv().unwrap();
    assert!(engine.snapshot().unwrap().objects().is_empty());
}

#[test]
fn library_subscriptions_are_limited_and_drop_releases_a_slot() {
    let engine = Engine::new().unwrap();
    let mut events: Vec<_> = (0..64).map(|_| engine.subscribe().unwrap()).collect();
    assert!(matches!(engine.subscribe(), Err(error) if error.code == ErrorCode::ResourceLimit));
    drop(events.pop());
    events.push(engine.subscribe().unwrap());
    assert!(matches!(engine.subscribe(), Err(error) if error.code == ErrorCode::ResourceLimit));
}

#[test]
fn bundled_styles_are_available_and_unknown_styles_or_locales_refuse_the_request() {
    let mut case = case();
    let engine = Engine::from_snapshot(Arc::new(case.library));
    let cancel = Cancellation::default();
    let catalog = engine.dispatch(Method::Styles, &json!({}), &cancel).unwrap().data;
    assert_eq!(catalog["styles"][0]["id"], "apa");
    assert!(catalog["total"].as_u64().unwrap() > 10_000);
    case.request.output = OutputSpec::Csl {
        style: "unknown-style".into(),
    };
    // The request is wrong, not its item: refused whole, before formatting.
    let refused = engine
        .dispatch(Method::Cite, &serde_json::to_value(&case.request).unwrap(), &cancel)
        .unwrap_err();
    assert_eq!(
        refused,
        EngineError::because(ErrorCode::BadInput, ErrorReason::UnknownStyle)
    );
    case.request.output = OutputSpec::Csl { style: "apa".into() };
    case.request.output_locale = Some("xx-synthetic".into());
    let refused = engine
        .dispatch(Method::Cite, &serde_json::to_value(&case.request).unwrap(), &cancel)
        .unwrap_err();
    assert_eq!(
        refused,
        EngineError::because(ErrorCode::BadInput, ErrorReason::UnknownLocale)
    );
}

#[test]
fn search_reads_shorthand_only_when_asked() {
    let case = case();
    let engine = Engine::from_snapshot(Arc::new(case.library.clone()));
    let cancel = Cancellation::default();
    let read = engine
        .dispatch(
            Method::Search,
            &json!({"query":"see signals chap. 3","interpret_locator":true}),
            &cancel,
        )
        .unwrap();
    assert_eq!(
        read.data["shorthand"],
        json!({"search":"signals","prefix":"see","locator":{"search":"signals","locator":{"term":"chapter","value":"3"},"typed":"chap. 3"}})
    );
    let plain = engine
        .dispatch(Method::Search, &json!({"query":"signals"}), &cancel)
        .unwrap();
    assert_eq!(read.data["hits"], plain.data["hits"]);
    assert!(!plain.data["hits"].as_array().unwrap().is_empty());
    // Without the flag the whole query is searched and nothing is read.
    let literal = engine
        .dispatch(Method::Search, &json!({"query":"see signals chap. 3"}), &cancel)
        .unwrap();
    assert!(literal.data.get("shorthand").is_none());
}

#[test]
fn words_that_name_a_title_stay_search_text_and_a_comma_asks_for_the_page() {
    // Invented titles: a title's number never becomes a page, nor its "See" a signal.
    let records = [
        json!({"type":"book","title":"Room 22","author":[{"family":"Aster","given":"Ada"}],"issued":{"date-parts":[[2020]]}}),
        json!({"type":"book","title":"See You Later","author":[{"family":"Okafor","given":"Tobi"}],"issued":{"date-parts":[[2021]]}}),
        json!({"type":"book","title":"Nineteen Eighty-Four","author":[{"family":"Vale","given":"Bo"}],"issued":{"date-parts":[[1984]]}}),
    ];
    let snapshot = Snapshot::new(
        records
            .iter()
            .enumerate()
            .map(|(index, csl)| {
                LibraryObject::Record(Record {
                    added_at: None,
                    id: ItemRef {
                        library: LibraryId::digest(b"invented-shorthand"),
                        key: format!("work-{index}"),
                    },
                    csl: serde_json::from_value(csl.clone()).unwrap(),
                    citation_keys: vec![],
                    aliases: vec![],
                    warnings: vec![],
                    identifiers: vec![],
                })
            })
            .collect(),
    )
    .unwrap();
    let engine = Engine::from_snapshot(Arc::new(snapshot));
    let cancel = Cancellation::default();
    let search = |query: &str| {
        engine
            .dispatch(
                Method::Search,
                &json!({"query": query, "interpret_locator": true}),
                &cancel,
            )
            .unwrap()
            .data
    };
    let title = |data: &serde_json::Value| data["hits"][0]["item"]["object"]["data"]["csl"]["title"].clone();
    for (query, expected) in [
        ("room 22", "Room 22"),
        ("see you later", "See You Later"),
        ("Nineteen Eighty-Four 1984", "Nineteen Eighty-Four"),
    ] {
        let data = search(query);
        assert!(data.get("shorthand").is_none(), "{query}: {data}");
        assert_eq!(title(&data), expected, "{query}: {data}");
    }
    assert_eq!(search("room 22")["kept_words"], true);
    assert_eq!(search("see you later")["kept_words"], true);
    // A comma asks for the page; words that name nothing are read as before.
    let asked = search("room, 22");
    assert_eq!(asked["shorthand"]["locator"]["locator"]["value"], "22");
    assert!(asked.get("kept_words").is_none());
    let read = search("see aster 417");
    assert_eq!(read["shorthand"]["prefix"], "see");
    assert_eq!(read["shorthand"]["locator"]["locator"]["value"], "417");
    assert_eq!(title(&read), "Room 22");
}
