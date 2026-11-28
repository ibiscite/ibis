//! The attachment tools are offered only for libraries the person named in
//! settings, and every answer labels the text it copies as data.
#![cfg(test)]
#[path = "support/zotero.rs"]
#[allow(dead_code)]
mod fixture;
use ibis_engine::{Cancellation, Engine, Method, mcp};
use serde_json::{Value, json};

fn modern(id: u64, method: &str, mut params: Value) -> Value {
    params["_meta"] = json!({
        "io.modelcontextprotocol/protocolVersion": mcp::PROTOCOL_VERSION,
        "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "1"},
        "io.modelcontextprotocol/clientCapabilities": {}
    });
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn call(engine: &Engine, session: &mut mcp::Session, name: &str, arguments: Value) -> Value {
    mcp::handle(
        engine,
        session,
        &modern(2, "tools/call", json!({"name": name, "arguments": arguments})),
    )
    .unwrap()
}

fn personal_consent(engine: &Engine) -> Value {
    json!([engine
        .libraries()
        .unwrap()
        .libraries
        .into_iter()
        .find(|library| library.zotero == Some(ibis_engine::request::ZoteroLibrary::User))
        .unwrap()
        .id])
}

fn settings(directory: &std::path::Path, chosen: Value) -> std::path::PathBuf {
    let path = directory.join("settings.json");
    std::fs::write(
        &path,
        json!({"schema_version": 2, "attachment_text": chosen}).to_string(),
    )
    .unwrap();
    path
}

#[cfg(unix)]
#[test]
fn outside_library_permission_is_checked_on_every_attachment_tool_call() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    std::fs::create_dir(root.join("library")).unwrap();
    std::fs::write(root.join("outside.pdf"), b"%PDF-1.7\n").unwrap();
    let library = root.join("library/refs.bib");
    std::fs::write(
        &library,
        "@book{atlas2026, title={Invented atlas}, file={../outside.pdf}}",
    )
    .unwrap();
    let program = root.join("invented-ibis");
    std::fs::write(&program, "#!/bin/sh\ncat >/dev/null\nprintf '{\"read\":{\"pages\":1,\"labels\":null,\"text\":[\"Invented lantern.\"]}}'\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    let engine = Engine::new().unwrap().without_file_watches().with_pdf_reader(program);
    engine
        .dispatch(
            Method::Libraries,
            &json!({"action":"open-file", "path":library}),
            &Cancellation::default(),
        )
        .unwrap();
    let chosen = json!([engine.libraries().unwrap().libraries[0].id]);
    let path = settings(root, chosen.clone());
    let mut session = mcp::Session::default().with_formats(path.clone());
    let refused = call(&engine, &mut session, "outline", json!({"ref":"atlas2026"}));
    assert_eq!(refused["result"]["isError"], true, "{refused}");
    let refusal = refused.to_string();
    assert!(refusal.contains("attachment-outside-library"));
    assert!(!refusal.contains("outside.pdf"));
    std::fs::write(
        &path,
        json!({"schema_version":2, "attachment_text":chosen, "attachment_text_outside_library":true}).to_string(),
    )
    .unwrap();
    let outlined = call(&engine, &mut session, "outline", json!({"ref":"atlas2026"}));
    let attachment = outlined["result"]["structuredContent"]["attachments"][0]["id"].clone();
    assert!(attachment.is_string(), "{outlined}");
    for (tool, arguments) in [
        ("read", json!({"attachment":attachment})),
        ("find_in", json!({"attachment":attachment, "query":"lantern"})),
    ] {
        assert_eq!(call(&engine, &mut session, tool, arguments)["result"]["isError"], false);
    }
    settings(root, chosen);
    for (tool, arguments) in [
        ("outline", json!({"ref":"atlas2026"})),
        ("read", json!({"attachment":attachment})),
        ("find_in", json!({"attachment":attachment, "query":"lantern"})),
    ] {
        let refused = call(&engine, &mut session, tool, arguments);
        assert_eq!(refused["result"]["isError"], true, "{refused}");
        assert!(refused.to_string().contains("attachment-outside-library"));
        assert!(!refused.to_string().contains("outside.pdf"));
    }
}

#[test]
fn each_answer_comes_only_from_a_library_the_person_named() {
    let server = fixture::Server::new();
    let engine = Engine::with_zotero_port(server.port).unwrap();
    engine
        .dispatch(
            Method::Libraries,
            &json!({"action":"open-zotero","libraries":["user",{"group":7}]}),
            &Cancellation::default(),
        )
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut session = mcp::Session::default().with_formats(settings(directory.path(), personal_consent(&engine)));
    let hits =
        call(&engine, &mut session, "search", json!({"query":"atlas"}))["result"]["structuredContent"]["hits"].clone();
    let outlines: Vec<Value> = hits
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| call(&engine, &mut session, "outline", json!({"ref": hit["ref"]}))["result"].clone())
        .collect();
    assert_eq!(outlines.len(), 2);
    let (allowed, refused): (Vec<_>, Vec<_>) = outlines.iter().partition(|outline| outline["isError"] == false);
    assert_eq!((allowed.len(), refused.len()), (1, 1));
    let error = &refused[0]["structuredContent"]["error"];
    assert_eq!(error["code"], "invalid-attachment-text");
    assert_eq!(error["recovery"]["action"], "review-settings");
    assert!(!refused[0].to_string().contains("Invented atlas PDF"));
}

#[test]
fn outline_projects_private_attachment_metadata_before_structured_content_and_summary() {
    use ibis_core::contract::{Attachment, Availability, GoldenCase, ItemRef, LibraryObject, Snapshot};
    let case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap();
    let parent = case.library.objects()[0].object.id().clone();
    let mut objects: Vec<_> = case
        .library
        .objects()
        .iter()
        .map(|object| object.object.clone())
        .collect();
    objects.push(LibraryObject::Attachment(Attachment {
        id: ItemRef {
            library: parent.library.clone(),
            key: "invented-attachment".into(),
        },
        parent: Some(parent.clone()),
        title: "file:///synthetic/private/letter.pdf".into(),
        media_type: "/synthetic/private/letter.pdf".into(),
        availability: Availability::OnDisk,
        content_hash: None,
    }));
    let engine = Engine::new().unwrap();
    engine
        .publish_library(&ibis_sources::LoadedLibrary::new(
            parent.library.clone(),
            ibis_sources::LibraryKind::File,
            &[
                ibis_sources::SourceCapability::BibliographicRecords,
                ibis_sources::SourceCapability::AttachmentText,
            ],
            Snapshot::new(objects).unwrap(),
        ))
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut session = mcp::Session::default().with_formats(settings(directory.path(), json!([parent.library])));
    let response = call(&engine, &mut session, "outline", json!({"ref":"Example2026"}));
    assert_eq!(response["result"]["isError"], false, "{response}");
    let attachment = &response["result"]["structuredContent"]["attachments"][0];
    assert!(attachment.get("title").is_none() && attachment.get("media_type").is_none());
    assert!(!response.to_string().contains("synthetic/private"), "{response}");
}
