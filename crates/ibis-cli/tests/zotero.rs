//! The real CLI parser/engine/transport, against an ephemeral synthetic server.
#![cfg(test)]
use ibis_engine::Engine;
use serde_json::{Value, json};
use std::{
    io::{Cursor, Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
struct Server {
    port: u16,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(Mutex::new((1_u64, String::from("Invented atlas"), false)));
        let stop = Arc::new(AtomicBool::new(false));
        let (shared, stopping) = (state.clone(), stop.clone());
        let worker = thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                let Ok((mut stream, _)) = listener.accept() else {
                    thread::sleep(Duration::from_millis(1));
                    continue;
                };
                stream.set_nonblocking(false).unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    if stream.read(&mut byte).unwrap_or(0) == 0 {
                        break;
                    }
                    request.push(byte[0]);
                }
                let (version, title, fail) = shared.lock().unwrap().clone();
                if fail {
                    continue;
                }
                let request = String::from_utf8(request).unwrap();
                // SYNTH001 has one file, which Zotero indexed in three pages.
                let body = if request.contains("/items/SYNTH001/children?") {
                    json!([{"key":"PDFA0001","data":{"key":"PDFA0001","itemType":"attachment","parentItem":"SYNTH001",
                        "title":"Invented atlas PDF","contentType":"application/pdf","linkMode":"imported_file"}}])
                } else if request.contains("/items/PDFA0001/fulltext") {
                    json!({"content":"Invented page one about maps.\u{c}Invented page two holds a brass lantern.\u{c}Invented page three.",
                        "indexedPages":3,"totalPages":3})
                } else if request.contains("format=versions") {
                    json!({"SYNTH001":version})
                } else {
                    json!([{"key":"SYNTH001","version":version,"data":{"key":"SYNTH001","version":version,"itemType":"book",
                "title":title,"date":"2026","citationKey":"Synthetic2026","extra":"ordinary raw Extra marker",
                "creators":[{"creatorType":"author","firstName":"Ada","lastName":"Example"}],"dateModified":"2026-10-26T12:00:00Z"}}])
                };
                let body = serde_json::to_vec(&body).unwrap();
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\nZotero-API-Version: 3\r\nX-Zotero-Version: 10.0.6\r\nZotero-Server-ID: invented-http\r\nLast-Modified-Version: {version}\r\nTotal-Results: 1\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(&body);
            }
        });
        Self {
            port,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap();
    }
}
fn run(engine: &Engine, args: &[&str]) -> (u8, String) {
    let mut output = vec![];
    let mut errors = vec![];
    let code = ibis::run_with(
        engine.clone(),
        args.iter().map(Into::into).collect(),
        Cursor::new(vec![]),
        &mut output,
        &mut errors,
        ibis::OutputMode::Plain,
    );
    assert!(
        errors.is_empty() || errors == b"ibis: year suffixes require the whole document\n",
        "{}",
        String::from_utf8_lossy(&errors)
    );
    (code, String::from_utf8(output).unwrap())
}
#[test]
fn explicit_zotero_search_show_pandoc_apa_and_call_use_the_shared_engine() {
    let server = Server::new();
    let engine = Engine::with_zotero_port(server.port).unwrap();
    let (code, output) = run(&engine, &["ibis", "--zotero", "search", "atlas", "--json"]);
    assert_eq!(code, 0);
    let response: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(response["data"]["hits"].as_array().unwrap().len(), 1);
    assert_eq!(
        run(&engine, &["ibis", "--zotero", "cite", "Synthetic2026"]).1,
        "[@Synthetic2026]\n"
    );
    assert_eq!(
        run(
            &engine,
            &["ibis", "--zotero", "cite", "Synthetic2026", "--style", "apa"]
        )
        .1,
        "(Example, 2026)\n"
    );
    assert_eq!(run(&engine, &["ibis", "show", "Synthetic2026", "--json"]).0, 0);
    let (code, output) = run(
        &engine,
        &["ibis", "call", "search", "--params", r#"{"query":"atlas"}"#, "--json"],
    );
    assert_eq!(code, 0);
    assert!(output.contains("Invented atlas"));
    assert!(!output.contains("invented-http"));
    assert!(!output.contains("dateModified"));
}

#[test]
fn zotero_group_flags_choose_the_libraries_read() {
    let server = Server::new();
    let engine = Engine::with_zotero_port(server.port).unwrap();
    assert_eq!(
        run(&engine, &["ibis", "--zotero-group", "7", "cite", "Synthetic2026"]).1,
        "[@Synthetic2026]\n"
    );
    let (code, output) = run(
        &engine,
        &["ibis", "--zotero", "--zotero-group", "7", "libraries", "--json"],
    );
    assert_eq!(code, 0);
    let response: Value = serde_json::from_str(&output).unwrap();
    let mut chosen: Vec<_> = response["data"]["libraries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|library| library["zotero"].to_string())
        .collect();
    chosen.sort();
    assert_eq!(chosen, [r#""user""#, r#"{"group":7}"#]);
    // Both libraries hold the fixture's key, so it names neither.
    let mut errors = vec![];
    let code = ibis::run_with(
        engine.clone(),
        ["ibis", "cite", "Synthetic2026"].iter().map(Into::into).collect(),
        Cursor::new(vec![]),
        vec![],
        &mut errors,
        ibis::OutputMode::Plain,
    );
    assert_ne!(code, 0);
    assert!(!errors.is_empty());
}

#[test]
fn outline_omits_private_attachment_metadata_in_plain_and_json_output() {
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
        parent: Some(parent),
        title: "file:///synthetic/private/letter.pdf".into(),
        media_type: "/synthetic/private/letter.pdf".into(),
        availability: Availability::OnDisk,
        content_hash: None,
    }));
    let engine = Engine::from_snapshot(Arc::new(Snapshot::new(objects).unwrap()));
    for json in [false, true] {
        let mut args = vec!["ibis", "outline", "Example2026"];
        if json {
            args.push("--json");
        }
        let (code, output) = run(&engine, &args);
        assert_eq!(code, 0, "{output}");
        assert!(!output.contains("synthetic/private"), "{output}");
        if json {
            let value: Value = serde_json::from_str(&output).unwrap();
            assert!(value["data"]["attachments"][0].get("title").is_none());
            assert!(value["data"]["attachments"][0].get("media_type").is_none());
        } else {
            assert_eq!(
                output.split('\t').count(),
                4,
                "the omitted fields keep their columns: {output}"
            );
        }
    }
}
