//! The real CLI parser/engine/transport, against an ephemeral synthetic server.
#![cfg(test)]
use ibis_engine::{Cancellation, Engine, Method};
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
    state: Arc<Mutex<(u64, String, bool)>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(Mutex::new((1, "Invented atlas".into(), false)));
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
            state,
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
fn a_zotero_profile_keeps_its_ids_across_close_and_reopen_and_two_profiles_differ() {
    let server = Server::new();
    let ids = |engine: &Engine| {
        let (_, output) = run(engine, &["ibis", "--zotero", "libraries", "--json"]);
        let libraries: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(libraries["data"]["libraries"][0]["kind"], "zotero");
        libraries["data"]["libraries"][0]["id"].clone()
    };
    let first = Engine::with_zotero_profile(server.port, b"directory:/invented/one").unwrap();
    let again = Engine::with_zotero_profile(server.port, b"directory:/invented/one").unwrap();
    let other = Engine::with_zotero_profile(server.port, b"directory:/invented/two").unwrap();
    let (first, again, other) = (ids(&first), ids(&again), ids(&other));
    assert_eq!(first, again);
    assert_ne!(first, other);
    // Both still read as the same kind of library, whatever the id says.
    assert!(first.as_str().unwrap().starts_with("lib_"));
    assert!(other.as_str().unwrap().starts_with("lib_"));
}

#[test]
fn failed_reload_preserves_last_good_and_successful_reload_replaces_it() {
    let server = Server::new();
    let engine = Engine::with_zotero_port(server.port).unwrap();
    let cancel = Cancellation::default();
    engine
        .dispatch(Method::Libraries, &json!({"action":"open-zotero"}), &cancel)
        .unwrap();
    let previous = engine.snapshot().unwrap();
    server.state.lock().unwrap().2 = true;
    engine
        .dispatch(Method::Libraries, &json!({"action":"reload"}), &cancel)
        .unwrap();
    assert_eq!(engine.snapshot().unwrap().fingerprints(), previous.fingerprints());
    assert!(engine.libraries().unwrap().libraries[0].refresh_error.is_some());
    *server.state.lock().unwrap() = (2, "Edited atlas".into(), false);
    engine
        .dispatch(Method::Libraries, &json!({"action":"reload"}), &cancel)
        .unwrap();
    assert_ne!(engine.snapshot().unwrap().fingerprints(), previous.fingerprints());
    assert!(engine.libraries().unwrap().libraries[0].refresh_error.is_none());
}
#[test]
fn zotero_group_flags_accumulate_across_positions_and_ignore_duplicates() {
    let server = Server::new();
    for duplicates in [false, true] {
        let engine = Engine::with_zotero_port(server.port).unwrap();
        let mut args = vec!["ibis", "--zotero-group", "7", "search", "atlas", "--zotero-group", "9"];
        if duplicates {
            args.extend(["--zotero-group", "7", "--zotero-group", "9"]);
        }
        args.push("--json");
        let (code, output) = run(&engine, &args);
        assert_eq!(code, 0);
        let response: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(response["data"]["hits"].as_array().unwrap().len(), 2);
        let mut chosen: Vec<_> = engine
            .libraries()
            .unwrap()
            .libraries
            .into_iter()
            .map(|library| serde_json::to_value(library.zotero).unwrap())
            .collect();
        chosen.sort_by_key(Value::to_string);
        assert_eq!(chosen, [json!({"group":7}), json!({"group":9})]);
    }
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

fn run_failing(engine: &Engine, args: &[&str]) -> (u8, String) {
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
    assert!(output.is_empty());
    (code, String::from_utf8(errors).unwrap())
}

#[test]
fn outline_read_and_find_in_give_a_zotero_attachments_text_by_page() {
    let server = Server::new();
    let engine = Engine::with_zotero_port(server.port).unwrap();
    let (code, output) = run(&engine, &["ibis", "--zotero", "outline", "Synthetic2026", "--json"]);
    assert_eq!(code, 0, "{output}");
    let outline: Value = serde_json::from_str(&output).unwrap();
    let attachment = &outline["data"]["attachments"][0];
    assert_eq!(attachment["pages"], 3);
    let id = attachment["id"].as_str().unwrap().to_owned();
    assert!(!output.contains("PDFA0001"));
    let (code, output) = run(&engine, &["ibis", "--zotero", "outline", "Synthetic2026"]);
    assert_eq!(code, 0);
    assert_eq!(
        output,
        format!("{id}\tInvented atlas PDF\tapplication/pdf\ttext, 3 pages\n")
    );

    // Another process opening the same generation and revision can use the id.
    let other = Engine::with_zotero_port(server.port).unwrap();
    let (code, output) = run(&other, &["ibis", "--zotero", "read", &id, "--pages", "2"]);
    assert_eq!(code, 0);
    assert_eq!(
        output,
        format!("[file page 2]\nInvented page two holds a brass lantern.\nMore text: ibis read {id} --pages 3-7\n")
    );
    let (_, output) = run(&engine, &["ibis", "--zotero", "read", &id, "--pages", "2-3", "--json"]);
    let read: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(read["data"]["pages"][1]["pdf_page"], 3);
    assert_eq!(read["data"]["pages"][1]["cite_page"], Value::Null);
    let (code, output) = run(&engine, &["ibis", "--zotero", "find-in", &id, "BRASS lantern"]);
    assert_eq!(code, 0);
    assert_eq!(output, "[file page 2]\tInvented page two holds a brass lantern.\n");
    let (_, output) = run(&engine, &["ibis", "--zotero", "find-in", &id, "compass"]);
    assert_eq!(output, "No passage holds those words.\n");

    let (code, errors) = run_failing(&engine, &["ibis", "--zotero", "read", &id, "--pages", "4"]);
    assert_ne!(code, 0);
    assert!(errors.contains("ibis outline"), "{errors}");
    let (code, errors) = run_failing(&engine, &["ibis", "--zotero", "read", "att:invented"]);
    assert_ne!(code, 0);
    assert!(errors.contains("run ibis outline again"), "{errors}");
    let (code, _) = run_failing(&engine, &["ibis", "--zotero", "read", &id, "--pages", "3-2"]);
    assert_ne!(code, 0);
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
