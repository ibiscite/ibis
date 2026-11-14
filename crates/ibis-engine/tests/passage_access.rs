#![allow(clippy::unwrap_used)]
//! Denial acceptance only. A-only enable/revoke awaits the declaration and safe-text owners.
use ibis_core::contract::*;
use ibis_engine::{Cancellation, Engine, Method, mcp, protocol, request::*, server};
use serde_json::{Value, json};
use std::{io::Cursor, sync::Arc};

const SECRETS: [&str; 5] = [
    "NOTE-TEXT",
    "PASSAGE-TEXT",
    "PASSAGE-COMMENT",
    "file:///invented/private",
    "smb://invented/private",
];

fn id(library: &str, key: &str) -> ItemRef {
    ItemRef {
        library: LibraryId::new(library).unwrap(),
        key: key.into(),
    }
}

fn engine() -> Engine {
    let mut objects = vec![];
    for library in ["A", "B"] {
        let record: Record = serde_json::from_value(json!({
            "id": id(library, "record"),
            "csl": {"type":"book", "title":format!("Invented library {library}")},
            "citation_keys":[{"value":format!("record{library}"),"source":"bibtex"}]
        }))
        .unwrap();
        let mut private = record.clone();
        private.id = id(library, "private-record");
        private.citation_keys.clear();
        private.csl.insert("note".into(), json!(format!("NOTE-TEXT-{library}")));
        private.csl.insert("URL".into(), json!("file:///invented/private"));
        objects.extend([
            LibraryObject::Record(record),
            LibraryObject::Record(private),
            LibraryObject::Attachment(Attachment {
                id: id(library, "attachment"),
                parent: Some(id(library, "record")),
                title: "Invented PDF".into(),
                media_type: "application/pdf".into(),
                availability: Availability::Unknown,
                content_hash: None,
            }),
            LibraryObject::Note(Note {
                id: id(library, "note"),
                parent: Some(id(library, "record")),
                text: format!("NOTE-TEXT-{library} file:///invented/private/{library}"),
            }),
            LibraryObject::Annotation(Annotation {
                id: id(library, "annotation"),
                parent: id(library, "attachment"),
                text: format!("PASSAGE-TEXT-{library}"),
                comment: format!("PASSAGE-COMMENT-{library} smb://invented/private/{library}"),
                page_label: Some("7".into()),
                page_index: Some(6),
            }),
        ]);
    }
    Engine::from_snapshot(Arc::new(Snapshot::new(objects).unwrap()))
}

fn no_secrets(bytes: &[u8]) {
    let text = std::str::from_utf8(bytes).unwrap();
    for secret in SECRETS {
        assert!(!text.contains(secret), "disclosed {secret}: {text}");
    }
}

fn show(library: &str, key: &str) -> Value {
    json!({"item":{"by":"id","value":id(library, key)}})
}

#[test]
fn generic_reads_search_show_dispatch_and_call_deny_both_libraries() {
    let engine = engine();
    let snapshot = engine.snapshot().unwrap();
    no_secrets(&serde_json::to_vec(&snapshot).unwrap());
    assert_eq!(snapshot.objects().len(), 2);
    let cancel = Cancellation::default();
    let search: SearchRequest = serde_json::from_value(json!({"query":"Invented", "limit":20})).unwrap();
    let found = engine.search(&search, &cancel).unwrap();
    no_secrets(&serde_json::to_vec(&found).unwrap());
    assert_eq!(found.hits.len(), 2);
    assert_eq!(engine.desktop_search(&search, &cancel).unwrap().hits.len(), 4);
    for library in ["A", "B"] {
        for key in ["record", "note", "annotation", "private-record"] {
            let params = show(library, key);
            let typed = engine.show(&serde_json::from_value(params.clone()).unwrap());
            assert_eq!(typed.is_ok(), key == "record");
            for (method, params) in [
                (Method::Show, params.clone()),
                (Method::Call, json!({"method":"show", "params":params})),
            ] {
                let result = engine.dispatch(method, &params, &cancel);
                match result {
                    Ok(result) => {
                        assert_eq!(key, "record");
                        no_secrets(&serde_json::to_vec(&result.data).unwrap());
                        no_secrets(result.text.unwrap_or_default().as_bytes());
                    }
                    Err(error) => {
                        assert_ne!(key, "record");
                        assert_eq!(error.code, ErrorCode::NotFound);
                        no_secrets(&serde_json::to_vec(&error).unwrap());
                    }
                }
            }
        }
        let found = engine
            .dispatch(
                Method::Search,
                &json!({"query":"Invented", "libraries":[library]}),
                &cancel,
            )
            .unwrap();
        assert_eq!(found.data["hits"].as_array().unwrap().len(), 1);
        no_secrets(&serde_json::to_vec(&found.data).unwrap());
    }
    for method in [Method::Show, Method::Search] {
        for field in ["trusted", "access", "desktop"] {
            let mut params = if method == Method::Show {
                show("A", "note")
            } else {
                json!({"query":"Invented"})
            };
            params[field] = json!(true);
            let error = engine.dispatch(method, &params, &cancel).unwrap_err();
            assert_eq!(error.code, ErrorCode::BadInput);
            no_secrets(&serde_json::to_vec(&error).unwrap());
        }
    }
}

fn rpc_request(id: usize, method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0", "id":id, "method":method, "params":params})
}

fn tool(id: usize, name: &str, arguments: Value) -> Value {
    rpc_request(
        id,
        "tools/call",
        json!({"name":name,"arguments":arguments,"_meta":{
            "io.modelcontextprotocol/protocolVersion":mcp::PROTOCOL_VERSION,
            "io.modelcontextprotocol/clientCapabilities":{}
        }}),
    )
}

#[test]
fn actual_rpc_frames_and_mcp_lines_disclose_no_passages_even_in_refusals() {
    let engine = engine();
    let mut requests = vec![rpc_request(0, "search", json!({"query":"Invented", "limit":20}))];
    for library in ["A", "B"] {
        for key in ["record", "note", "annotation"] {
            requests.push(rpc_request(
                requests.len(),
                "call",
                json!({"method":"show","params":show(library,key)}),
            ));
        }
    }
    let mut input = vec![];
    for request in &requests {
        protocol::write_frame(&mut input, request).unwrap();
    }
    let mut bytes = vec![];
    server::serve(engine.clone(), Cursor::new(input), &mut bytes).unwrap();
    no_secrets(&bytes);
    let mut reader = Cursor::new(&bytes);
    let mut replies = vec![];
    while let Some(reply) = protocol::read_frame(&mut reader).unwrap() {
        replies.push(reply);
    }
    assert_eq!(replies.len(), requests.len());
    assert_eq!(replies.iter().filter(|reply| reply.get("error").is_some()).count(), 4);
    let rpc_bytes = bytes.len();

    let mut requests = vec![tool(0, "search", json!({"query":"Invented", "limit":20}))];
    for library in ["A", "B"] {
        for key in ["record", "note", "annotation"] {
            requests.push(tool(
                requests.len(),
                "show",
                json!({"ref":mcp::reference(&id(library,key))}),
            ));
        }
    }
    let input = requests
        .iter()
        .map(|request| format!("{request}\n"))
        .collect::<String>();
    let mut bytes = vec![];
    mcp::serve(&engine, mcp::Session::default(), input.as_bytes(), &mut bytes).unwrap();
    no_secrets(&bytes);
    let replies: Vec<Value> = std::str::from_utf8(&bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(replies.len(), requests.len());
    assert_eq!(
        replies
            .iter()
            .filter(|reply| reply["result"]["isError"] == true)
            .count(),
        4
    );
    assert!(
        replies.iter().any(
            |reply| reply["id"] == 0 && reply["result"]["structuredContent"]["hits"].as_array().unwrap().len() == 2
        )
    );
    eprintln!(
        "denial transport evidence: RPC {rpc_bytes} bytes / 7 replies; MCP {} bytes / 7 replies; zero secret markers",
        bytes.len()
    );
}

#[test]
fn trusted_desktop_reads_are_distinct_from_generic_external_object_reads() {
    let engine = engine();
    let desktop = engine.desktop_snapshot().unwrap();
    assert_eq!(desktop.objects().len(), 10);
    let trusted_bytes = serde_json::to_string(&desktop).unwrap();
    assert!(SECRETS.iter().all(|secret| trusted_bytes.contains(secret)));
    let external = engine.external_reads(None);
    for library in ["A", "B"] {
        for key in ["note", "annotation", "private-record"] {
            let request: ShowRequest = serde_json::from_value(show(library, key)).unwrap();
            let trusted = engine.desktop_show_checked(&request).unwrap();
            assert_eq!(&trusted.value, desktop.get(&id(library, key)).unwrap());
            assert_eq!(
                external.object(&id(library, key)).unwrap_err().code,
                ErrorCode::NotFound
            );
        }
        let record = external.object(&id(library, "record")).unwrap();
        assert_eq!(
            record.fingerprints,
            desktop.get(&id(library, "record")).unwrap().fingerprints
        );
    }
    assert_eq!(
        engine.snapshot().unwrap().fingerprints(),
        external.snapshot().unwrap().fingerprints()
    );
    let roundtrip: Snapshot =
        serde_json::from_slice(&serde_json::to_vec(&external.snapshot().unwrap()).unwrap()).unwrap();
    assert_eq!(roundtrip.objects().len(), 2);
}

#[test]
fn owner_authority_never_enables_publication_in_this_slice() {
    use ibis_engine::settings::{Owner, PrimaryRead, Settings};
    let engine = engine();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let owner = Owner::open(path.clone());
    let valid = serde_json::to_vec(&Settings::default()).unwrap();
    for state in [
        "missing",
        "missing-with-backup",
        "invalid",
        "unreadable",
        "valid",
        "unavailable",
    ] {
        match state {
            "missing-with-backup" => {
                std::fs::write(dir.path().join("settings.json.bak"), &valid).unwrap();
                assert_eq!(owner.read().primary_read, PrimaryRead::Missing);
                assert!(owner.read().from_backup);
            }
            "invalid" => {
                std::fs::write(&path, "{invalid NOTE-TEXT").unwrap();
                assert!(owner.read().problem.is_some());
            }
            "unreadable" => {
                std::fs::remove_file(&path).unwrap();
                std::fs::create_dir(&path).unwrap();
                assert_eq!(owner.read().primary_read, PrimaryRead::Unreadable);
            }
            "valid" => {
                std::fs::remove_dir(&path).unwrap();
                std::fs::write(&path, &valid).unwrap();
                assert!(owner.read().cites().is_ok());
            }
            "unavailable" => {
                owner.close();
                assert!(owner.read_with(|_| ()).is_none());
            }
            _ => assert_eq!(owner.read().primary_read, PrimaryRead::Missing),
        }
        let external = engine.external_reads(Some(&owner));
        no_secrets(&serde_json::to_vec(&external.snapshot().unwrap()).unwrap());
        for library in ["A", "B"] {
            for key in ["note", "annotation"] {
                let refusal = external.object(&id(library, key)).unwrap_err();
                assert_eq!(refusal.code, ErrorCode::NotFound, "{state}");
                no_secrets(&serde_json::to_vec(&refusal).unwrap());
            }
        }
    }
}
