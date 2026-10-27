//! The real CLI parser/engine/transport, against an ephemeral synthetic server.
#![allow(clippy::unwrap_used)]
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
                let body = if request.contains("format=versions") {
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
    assert_eq!(chosen, [r#""personal""#, r#"{"group":7}"#]);
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
