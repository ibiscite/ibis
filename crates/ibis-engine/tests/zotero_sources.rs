#[path = "support/zotero.rs"]
mod fixture;
use ibis_core::contract::{ErrorCode, ErrorReason, GoldenCase};
use ibis_engine::{Cancellation, Engine, Method, request::ZoteroLibrary};
use serde_json::json;
use std::sync::atomic::Ordering;

#[test]
fn unaccepted_zotero_reload_keeps_search_and_health_private() {
    let server = fixture::Server::new();
    let engine = Engine::with_zotero_port(server.port).unwrap();
    let cancel = Cancellation::default();
    engine
        .dispatch(Method::Libraries, &json!({"action":"open-zotero"}), &cancel)
        .unwrap();
    let before = engine.snapshot().unwrap();
    let health = serde_json::to_value(engine.libraries().unwrap()).unwrap();
    for fails in [false, true] {
        let cancel = Cancellation::default();
        *server.title.lock().unwrap() = "Obsolete atlas".into();
        server.sharing_off.store(fails, Ordering::SeqCst);
        let prepared = engine.prepare_zotero_reload(true, None, None, &cancel).unwrap();
        assert_eq!(engine.snapshot().unwrap().fingerprints(), before.fingerprints());
        assert_eq!(serde_json::to_value(engine.libraries().unwrap()).unwrap(), health);
        // The newer choice cancels the operation whose candidate arrives late.
        let directory = tempfile::tempdir().unwrap();
        assert!(
            engine
                .open_file_observed(&directory.path().join("unreadable.bib"), &cancel)
                .is_err()
        );
        cancel.cancel();
        assert_eq!(
            engine.accept_zotero_reload(prepared).unwrap_err().code,
            ErrorCode::Cancelled
        );
        assert_eq!(engine.snapshot().unwrap().fingerprints(), before.fingerprints());
        assert_eq!(serde_json::to_value(engine.libraries().unwrap()).unwrap(), health);
    }
    server.sharing_off.store(false, Ordering::SeqCst);
    let prepared = engine.prepare_zotero_reload(true, None, None, &cancel).unwrap();
    engine.accept_zotero_reload(prepared).unwrap();
    assert_ne!(engine.snapshot().unwrap().fingerprints(), before.fingerprints());
    assert!(
        engine
            .libraries()
            .unwrap()
            .libraries
            .iter()
            .all(|info| info.refresh_error.is_none())
    );
}

#[test]
fn private_zotero_failure_retires_copy_without_publishing_health_and_a_retry_recovers() {
    let server = fixture::Server::new();
    let engine = Engine::with_zotero_port(server.port).unwrap();
    let cancel = Cancellation::default();
    engine
        .dispatch(Method::Libraries, &json!({"action":"open-zotero"}), &cancel)
        .unwrap();
    let snapshot = engine.snapshot().unwrap();
    let health = serde_json::to_value(engine.libraries().unwrap()).unwrap();
    let mut case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap();
    case.request.items[0].item = snapshot.objects()[0].object.id().clone();
    let citation = engine.prepare_citation(&case.request, &cancel).unwrap();
    server.sharing_off.store(true, Ordering::SeqCst);
    let reload = engine.prepare_zotero_reload(false, None, None, &cancel).unwrap();
    assert!(
        engine.recite(&citation, &cancel).is_err(),
        "a failed read cannot keep a copy ready"
    );
    assert_eq!(serde_json::to_value(engine.libraries().unwrap()).unwrap(), health);
    assert_eq!(
        engine.accept_zotero_reload(reload).unwrap_err().reason,
        Some(ErrorReason::ZoteroSharingOff)
    );
    server.sharing_off.store(false, Ordering::SeqCst);
    let reload = engine.prepare_zotero_reload(false, None, None, &cancel).unwrap();
    engine.accept_zotero_reload(reload).unwrap();
    let current = engine.prepare_citation(&case.request, &cancel).unwrap();
    assert_eq!(engine.recite(&current, &cancel).unwrap().text, "[@Synthetic2026]");
    assert!(
        engine.recite(&citation, &cancel).is_err(),
        "the earlier receipt stays retired"
    );
}

#[test]
fn chosen_group_libraries_publish_separately_and_a_new_choice_replaces_the_old() {
    let server = fixture::Server::new();
    let engine = Engine::with_zotero_port(server.port).unwrap();
    let cancel = Cancellation::default();
    let open = |libraries: serde_json::Value| {
        engine.dispatch(
            Method::Libraries,
            &json!({"action":"open-zotero","libraries":libraries}),
            &cancel,
        )
    };
    let chosen = || {
        let mut chosen: Vec<_> = engine
            .libraries()
            .unwrap()
            .libraries
            .into_iter()
            .map(|library| library.zotero)
            .collect();
        chosen.sort_by_key(|library| format!("{library:?}"));
        chosen
    };
    open(json!(["user", {"group": 7}])).unwrap();
    assert_eq!(chosen(), vec![Some(ZoteroLibrary::Group(7)), Some(ZoteroLibrary::User)]);
    // The fixture serves the same item and key in both libraries: two
    // references, and the shared key names neither rather than guessing.
    let snapshot = engine.snapshot().unwrap();
    assert_eq!(snapshot.objects().len(), 2);
    assert_ne!(snapshot.objects()[0].object.id(), snapshot.objects()[1].object.id());
    assert_eq!(
        engine
            .dispatch(
                Method::Show,
                &json!({"item":{"by":"key","value":"Synthetic2026"}}),
                &cancel
            )
            .unwrap_err()
            .code,
        ErrorCode::BadInput
    );
    let mut case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap();
    let mut cite = |object: &ibis_core::contract::SnapshotObject| {
        case.request.items[0].item = object.object.id().clone();
        case.request.items[0].expected_cite = Some(object.fingerprints.cite.clone());
        engine.cite(&case.request, &cancel).unwrap().text
    };
    assert_eq!(cite(&snapshot.objects()[1]), "", "a conflicting key is refused");

    open(json!([{"group": 7}])).unwrap();
    assert_eq!(chosen(), vec![Some(ZoteroLibrary::Group(7))]);
    let only = engine.snapshot().unwrap();
    assert_eq!(only.objects().len(), 1);
    assert_eq!(cite(&only.objects()[0]), "[@Synthetic2026]");
    let group = engine.snapshot().unwrap().fingerprints().clone();

    for invalid in [json!([]), json!([{"group": 0}]), json!(["user", "user"])] {
        assert_eq!(open(invalid).unwrap_err().code, ErrorCode::BadInput);
    }
    // A group that does not exist is reported, never read as another library,
    // and the previous choice stays published until it is read again.
    assert_eq!(open(json!([{"group": 99}])).unwrap_err().code, ErrorCode::NotFound);
    assert_eq!(chosen(), vec![Some(ZoteroLibrary::Group(7))]);
    assert_eq!(engine.snapshot().unwrap().fingerprints(), &group);
    engine
        .dispatch(Method::Libraries, &json!({"action":"open-zotero"}), &cancel)
        .unwrap();
    assert_eq!(chosen(), vec![Some(ZoteroLibrary::Group(7))]);
    assert_eq!(engine.snapshot().unwrap().fingerprints(), &group);
}

#[test]
fn turning_sharing_off_later_keeps_the_last_read_and_says_why() {
    let server = fixture::Server::new();
    let engine = Engine::with_zotero_port(server.port).unwrap();
    let cancel = Cancellation::default();
    engine
        .dispatch(Method::Libraries, &json!({"action":"open-zotero"}), &cancel)
        .unwrap();
    server.sharing_off.store(true, Ordering::Relaxed);
    // Reloading everything reports each library's own health, not one failure.
    engine
        .dispatch(Method::Libraries, &json!({"action":"reload"}), &cancel)
        .unwrap();
    let info = engine.libraries().unwrap();
    assert_eq!(info.libraries[0].objects, 1, "the last good references stay");
    assert_eq!(info.libraries[0].refresh_error, Some(ErrorCode::SourceUnavailable));
    assert_eq!(info.libraries[0].refresh_reason, Some(ErrorReason::ZoteroSharingOff));

    server.sharing_off.store(false, Ordering::Relaxed);
    engine
        .dispatch(Method::Libraries, &json!({"action":"reload"}), &cancel)
        .unwrap();
    let info = engine.libraries().unwrap();
    assert_eq!(info.libraries[0].refresh_error, None);
    assert_eq!(info.libraries[0].refresh_reason, None, "recovery leaves no old reason");
}

#[test]
fn group_names_come_from_a_running_zotero_and_a_refusal_says_why() {
    let server = fixture::Server::new();
    let engine = Engine::with_zotero_port(server.port).unwrap();
    let cancel = Cancellation::default();
    let ask = json!({"action":"zotero-groups"});
    server.sharing_off.store(true, Ordering::Relaxed);
    assert_eq!(
        engine.dispatch(Method::Libraries, &ask, &cancel).unwrap_err().reason,
        Some(ErrorReason::ZoteroSharingOff)
    );
}

#[test]
fn a_running_zoteros_indexed_text_is_outlined_read_and_searched_by_page() {
    let server = fixture::Server::new();
    let engine = Engine::with_zotero_port(server.port).unwrap();
    let cancel = Cancellation::default();
    engine
        .dispatch(Method::Libraries, &json!({"action":"open-zotero"}), &cancel)
        .unwrap();
    let outline = engine
        .dispatch(
            Method::Outline,
            &json!({"item":{"by":"key","value":"Synthetic2026"}}),
            &cancel,
        )
        .unwrap()
        .data;
    let attachments = outline["attachments"].as_array().unwrap();
    assert_eq!(attachments.len(), 2);
    assert_eq!(attachments[0]["title"], "Invented atlas PDF");
    assert_eq!(attachments[0]["media_type"], "application/pdf");
    assert_eq!(attachments[0]["text"], "available");
    assert_eq!(attachments[0]["pages"], 3);
    assert_eq!(attachments[0]["pages_known"], true);
    assert_eq!(attachments[0]["cite_pages_known"], false);
    assert_eq!(attachments[1]["text"], "no-file");
    assert!(!outline.to_string().contains("PDFA0001"));
    let file = attachments[0]["id"].clone();
    let read = engine
        .dispatch(Method::Read, &json!({"attachment":file,"first_page":2}), &cancel)
        .unwrap()
        .data;
    assert_eq!(read["pages"].as_array().unwrap().len(), 2);
    assert_eq!(read["pages"][0]["pdf_page"], 2);
    assert_eq!(read["pages"][0]["cite_page"], serde_json::Value::Null);
    assert_eq!(read["pages"][0]["text"], "Invented page two holds a brass lantern.");
    assert!(read.get("next").is_none());
    let found = engine
        .dispatch(
            Method::FindIn,
            &json!({"attachment":file,"query":"Brass Lantern"}),
            &cancel,
        )
        .unwrap()
        .data;
    assert_eq!(found["complete"], true);
    assert_eq!(found["passages"][0]["pdf_page"], 2);
    assert_eq!(found["passages"][0]["text"], "Invented page two holds a brass lantern.");
    let refused = engine
        .dispatch(Method::Read, &json!({"attachment":attachments[1]["id"]}), &cancel)
        .unwrap_err();
    assert_eq!(refused.code, ErrorCode::SourceUnavailable);
    assert_eq!(refused.reason, Some(ErrorReason::NoAttachmentText));
}

#[test]
fn attachment_reading_ids_survive_equal_reload_and_expire_after_content_changes() {
    let server = fixture::Server::new();
    let engine = Engine::with_zotero_port(server.port).unwrap();
    let cancel = Cancellation::default();
    engine
        .dispatch(Method::Libraries, &json!({"action":"open-zotero"}), &cancel)
        .unwrap();
    let current_id = || {
        engine
            .dispatch(
                Method::Outline,
                &json!({"item":{"by":"key","value":"Synthetic2026"}}),
                &cancel,
            )
            .unwrap()
            .data["attachments"][0]["id"]
            .clone()
    };
    let before_reload = current_id();
    engine
        .dispatch(Method::Libraries, &json!({"action":"reload"}), &cancel)
        .unwrap();
    assert_eq!(current_id(), before_reload);
    engine
        .dispatch(Method::Read, &json!({"attachment":before_reload}), &cancel)
        .unwrap();

    for change in ["version", "modified", "index", "text"] {
        let old = current_id();
        match change {
            "version" => {
                let mut attachment = server.attachment.lock().unwrap();
                attachment["version"] = json!(2);
                attachment["data"]["version"] = json!(2);
            }
            "modified" => server.attachment.lock().unwrap()["data"]["dateModified"] = json!("2026-11-15T13:00:00Z"),
            "index" => server.fulltext.lock().unwrap()["indexedPages"] = json!(2),
            "text" => server.fulltext.lock().unwrap()["content"] = json!("Invented replacement text."),
            _ => unreachable!(),
        }
        let refused = engine
            .dispatch(Method::Read, &json!({"attachment":old}), &cancel)
            .unwrap_err();
        assert_eq!(refused.code, ErrorCode::NotFound, "{change}");
        assert_ne!(current_id(), old, "{change}");
        // A fresh id always resolves the current text.
        engine
            .dispatch(Method::Read, &json!({"attachment":current_id()}), &cancel)
            .unwrap();
    }
}
