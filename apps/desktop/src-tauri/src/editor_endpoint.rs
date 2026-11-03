//! The editor endpoint: Better BibTeX's Cite as You Write on `127.0.0.1`, so an
//! editor can open the picker and receive the citation the person chooses.
//!
//! Off until switched on in Settings (decision 0008). `ibis_engine::cayw` decides
//! what a request means; this file owns the sockets and the one request that may
//! be waiting. That request's output replaces the picker's while it waits, so the
//! preview is exactly what the editor gets. Its identity follows it from request
//! to answer, so nothing late can answer for a newer one. Nothing here polls:
//! threads block on `accept`, on a read or on the answer.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ibis_core::contract::{CitationForm, OutputSpec};
use ibis_core::settings::{self, EditorEndpoint};
use ibis_engine::cayw::{self, CaywPick, CaywRequest};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::diagnostics::{NativeIssue, report};
use crate::library::Library;
use crate::preferences::SettingsFile;

/// Zotero's own port is 23119; this one is Ibis's alone.
const DEFAULT_PORT: u16 = 23_124;
const ZOTERO_PORT: u16 = 23_119;
/// A client gets this long to send its request.
const HEAD_DEADLINE: Duration = Duration::from_secs(5);
const WRITE_DEADLINE: Duration = Duration::from_secs(5);
/// A person may take a while, but a forgotten request must end. The longest
/// wait the VS Code picker offers is 30 minutes.
const ANSWER_DEADLINE: Duration = Duration::from_secs(30 * 60);
/// Connections at once; the rest are closed unread.
const MAX_CONNECTIONS: usize = 8;
const BUSY: &str = "Ibis is already waiting for a citation.";

/// The request the picker is answering, as the rest of the app sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EditorRequest {
    pub(crate) id: u64,
    pub(crate) output: OutputSpec,
    pub(crate) form: CitationForm,
    pub(crate) clipboard: bool,
    pub(crate) label: &'static str,
}

/// One request at a time. An answer counts only for the request it names.
#[derive(Default)]
pub(crate) struct Pending {
    next: AtomicU64,
    active: Mutex<Option<(EditorRequest, SyncSender<Option<String>>)>>,
}

impl Pending {
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<(EditorRequest, SyncSender<Option<String>>)>> {
        self.active.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn begin(&self, pick: &CaywPick, style: &str) -> Option<(u64, Receiver<Option<String>>)> {
        let mut active = self.lock();
        if active.is_some() {
            return None;
        }
        let id = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        let (send, receive) = mpsc::sync_channel(1);
        // Numeric styles write whole entries, as the picker does.
        let form = if style == "ieee" && pick.form == CitationForm::Normal {
            CitationForm::BibliographyOnly
        } else {
            pick.form
        };
        let request = EditorRequest {
            id,
            output: pick.spec(style),
            form,
            clipboard: pick.clipboard,
            label: pick.label,
        };
        *active = Some((request, send));
        Some((id, receive))
    }

    pub(crate) fn current(&self) -> Option<EditorRequest> {
        self.lock().as_ref().map(|(request, _)| request.clone())
    }

    /// Give `id` its citation. False when that request has ended.
    pub(crate) fn answer(&self, id: u64, text: &str) -> bool {
        let mut active = self.lock();
        match active.take() {
            Some((request, send)) if request.id == id => send.try_send(Some(text.to_owned())).is_ok(),
            other => {
                *active = other;
                false
            }
        }
    }

    /// End `id` with no citation, which is how a client hears it was cancelled.
    fn cancel(&self, id: u64) -> bool {
        let mut active = self.lock();
        if active.as_ref().is_some_and(|(request, _)| request.id == id) {
            if let Some((_, send)) = active.take() {
                let _ = send.try_send(None);
            }
            true
        } else {
            false
        }
    }

    fn cancel_any(&self) -> bool {
        let id = self.lock().as_ref().map(|(request, _)| request.id);
        id.is_some_and(|id| self.cancel(id))
    }
}

/// What a connection needs from the app, apart so it can be tested on real
/// sockets without one.
trait Host: Send + Sync + 'static {
    fn pending(&self) -> &Pending;
    fn ready(&self) -> bool;
    fn style(&self) -> String;
    /// A request began or ended.
    fn changed(&self);
    fn present(&self);
}

fn read_head(stream: &mut TcpStream) -> Result<Vec<u8>, cayw::Refusal> {
    let started = Instant::now();
    let mut head = vec![];
    let mut chunk = [0; 1024];
    loop {
        let left = HEAD_DEADLINE
            .checked_sub(started.elapsed())
            .filter(|left| !left.is_zero())
            .ok_or(cayw::Refusal::Malformed)?;
        stream
            .set_read_timeout(Some(left))
            .map_err(|_| cayw::Refusal::Malformed)?;
        let count = stream.read(&mut chunk).map_err(|_| cayw::Refusal::Malformed)?;
        if count == 0 {
            return Err(cayw::Refusal::Malformed);
        }
        head.extend_from_slice(&chunk[..count]);
        if let Some(end) = cayw::head_end(&head) {
            head.truncate(end);
            return Ok(head);
        }
        if head.len() > cayw::MAX_HEAD_BYTES {
            return Err(cayw::Refusal::TooLarge);
        }
    }
}

fn reply(stream: &mut TcpStream, status: (u16, &str), body: &str) {
    let _ = stream
        .set_write_timeout(Some(WRITE_DEADLINE))
        .and_then(|()| stream.write_all(&cayw::response(status, body)))
        .and_then(|()| stream.flush());
}

/// A client that gives up closes its connection. Nothing more is expected from
/// it, so this read blocks until that happens or the answer closes the socket.
fn cancel_when_closed<H: Host>(mut stream: TcpStream, id: u64, host: Arc<H>) {
    let _ = std::thread::Builder::new()
        .name("ibis-editor-client".into())
        .spawn(move || {
            let _ = stream.set_read_timeout(None);
            let mut byte = [0; 1];
            while matches!(stream.read(&mut byte), Ok(1)) {}
            if host.pending().cancel(id) {
                host.changed();
            }
        });
}

fn serve<H: Host>(mut stream: TcpStream, port: u16, token: &str, host: Arc<H>) {
    let request = read_head(&mut stream).and_then(|head| cayw::parse(&head, port, token));
    match request {
        Err(refusal) => reply(&mut stream, refusal.status(), refusal.message()),
        Ok(CaywRequest::Probe) => reply(&mut stream, (200, "OK"), if host.ready() { "ready" } else { "No" }),
        Ok(CaywRequest::Pick(pick)) => {
            let Some((id, answer)) = host.pending().begin(&pick, &host.style()) else {
                reply(&mut stream, (409, "Conflict"), BUSY);
                return;
            };
            match stream.try_clone() {
                Ok(watched) => cancel_when_closed(watched, id, Arc::clone(&host)),
                Err(_) => {
                    host.pending().cancel(id);
                    reply(&mut stream, (500, "Internal Server Error"), "");
                    return;
                }
            }
            host.changed();
            host.present();
            let text = answer.recv_timeout(ANSWER_DEADLINE).ok().flatten();
            host.pending().cancel(id);
            host.changed();
            reply(&mut stream, (200, "OK"), text.as_deref().unwrap_or_default());
        }
    }
    let _ = stream.shutdown(Shutdown::Both);
}

/// The listening socket, on loopback only.
struct Listening {
    port: u16,
    stop: Arc<AtomicBool>,
}

impl Drop for Listening {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // `accept` has no timeout, so one connection wakes it to see the flag.
        let _ = TcpStream::connect((Ipv4Addr::LOCALHOST, self.port));
    }
}

fn listen<H: Host>(port: u16, token: String, host: Arc<H>) -> std::io::Result<Listening> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = Arc::clone(&stop);
    let token: Arc<str> = token.into();
    std::thread::Builder::new()
        .name("ibis-editor-endpoint".into())
        .spawn(move || {
            let open = Arc::new(AtomicUsize::new(0));
            for stream in listener.incoming() {
                if stopping.load(Ordering::SeqCst) {
                    return;
                }
                let Ok(stream) = stream else { continue };
                if !stream.peer_addr().is_ok_and(|peer| peer.ip().is_loopback())
                    || open.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS
                {
                    open.fetch_sub(1, Ordering::SeqCst);
                    continue;
                }
                let (open, token, host) = (Arc::clone(&open), Arc::clone(&token), Arc::clone(&host));
                let spawned = std::thread::Builder::new().name("ibis-editor-request".into()).spawn({
                    let open = Arc::clone(&open);
                    move || {
                        serve(stream, port, &token, host);
                        open.fetch_sub(1, Ordering::SeqCst);
                    }
                });
                if spawned.is_err() {
                    open.fetch_sub(1, Ordering::SeqCst);
                }
            }
        })?;
    Ok(Listening { port, stop })
}

struct AppHost(AppHandle);

impl Host for AppHost {
    fn pending(&self) -> &Pending {
        self.0.state::<Pending>().inner()
    }
    fn ready(&self) -> bool {
        self.0
            .state::<Mutex<Library>>()
            .lock()
            .is_ok_and(|library| library.file.is_some() && !library.opening)
    }
    fn style(&self) -> String {
        let chosen = settings::load(&self.0.state::<SettingsFile>().path)
            .settings
            .citation_style;
        crate::library::engine_file::style_id(chosen.unwrap_or(settings::CitationStyle::Apa)).to_owned()
    }
    fn changed(&self) {
        // A ticket made under one output must not be copied under another.
        if let Some(file) = self
            .0
            .state::<Mutex<Library>>()
            .lock()
            .ok()
            .and_then(|library| library.file.clone())
        {
            file.invalidate_preview();
        }
        report(self.0.emit("editor:request", ()), NativeIssue::LibraryEvent);
    }
    fn present(&self) {
        crate::picker::reveal_picker(&self.0);
    }
}

/// Why the endpoint could not listen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum EndpointProblem {
    /// Another program has the port.
    PortInUse,
    /// The saved port or token cannot be used.
    BadSetting,
    NotSaved,
}

/// What Settings shows. The address holds the token: anyone who has it can open
/// the picker, so it stays on this computer and in this window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
pub(crate) struct EndpointStatus {
    pub(crate) on: bool,
    pub(crate) url: Option<String>,
    pub(crate) problem: Option<EndpointProblem>,
}

fn address(endpoint: &EditorEndpoint) -> String {
    format!(
        "http://127.0.0.1:{}/{}/better-bibtex/cayw?format=pandoc",
        endpoint.port, endpoint.token
    )
}

fn usable(endpoint: &EditorEndpoint) -> bool {
    endpoint.port >= 1024
        && endpoint.port != ZOTERO_PORT
        && endpoint.token.len() == 32
        && endpoint.token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn new_token() -> Option<String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).ok()?;
    Some(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// The listening socket and what became of the last attempt to open it.
#[derive(Default)]
pub(crate) struct Endpoint(Mutex<(Option<Listening>, Option<EndpointProblem>)>);

fn open(app: &AppHandle, endpoint: &EditorEndpoint) -> Option<EndpointProblem> {
    if !usable(endpoint) {
        return Some(EndpointProblem::BadSetting);
    }
    let host = Arc::new(AppHost(app.clone()));
    match listen(endpoint.port, endpoint.token.clone(), host) {
        Ok(listening) => {
            let endpoint = app.state::<Endpoint>();
            let mut state = endpoint.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.0 = Some(listening);
            None
        }
        Err(_) => Some(EndpointProblem::PortInUse),
    }
}

/// Listen at launch if the person left it on.
pub(crate) fn start(app: &AppHandle, saved: Option<EditorEndpoint>) {
    if let Some(endpoint) = saved {
        let problem = open(app, &endpoint);
        app.state::<Endpoint>()
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .1 = problem;
    }
    use tauri::Listener;
    let handle = app.clone();
    // Hiding the picker is how a person says no.
    app.listen("picker:hidden", move |_| {
        if handle.state::<Pending>().cancel_any() {
            AppHost(handle.clone()).changed();
        }
    });
}

fn status(app: &AppHandle) -> EndpointStatus {
    let saved = settings::load(&app.state::<SettingsFile>().path)
        .settings
        .editor_endpoint;
    let state = app.state::<Endpoint>();
    let state = state.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    EndpointStatus {
        on: state.0.is_some(),
        url: saved.filter(|_| state.0.is_some()).as_ref().map(address),
        problem: state.1,
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) fn editor_endpoint(app: AppHandle) -> EndpointStatus {
    status(&app)
}

/// Switch the endpoint. Off closes the port and forgets the address, so the next
/// time it is on, an old copy of that address no longer works.
#[tauri::command]
#[specta::specta]
pub(crate) fn set_editor_endpoint(on: bool, app: AppHandle) -> EndpointStatus {
    let state = app.state::<Endpoint>();
    let closing = {
        let mut state = state.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.1 = None;
        state.0.take()
    };
    drop(closing);
    if app.state::<Pending>().cancel_any() {
        AppHost(app.clone()).changed();
    }
    let problem = if on {
        let Some(token) = new_token() else {
            return EndpointStatus {
                on: false,
                url: None,
                problem: Some(EndpointProblem::NotSaved),
            };
        };
        let endpoint = EditorEndpoint {
            port: DEFAULT_PORT,
            token,
        };
        open(&app, &endpoint).or_else(|| {
            let saved = endpoint.clone();
            app.state::<SettingsFile>()
                .update(move |settings| settings.editor_endpoint = Some(saved))
                .err()
                .map(|_| {
                    let mut state = state.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.0 = None;
                    EndpointProblem::NotSaved
                })
        })
    } else {
        app.state::<SettingsFile>()
            .update(|settings| settings.editor_endpoint = None)
            .err()
            .map(|_| EndpointProblem::NotSaved)
    };
    state.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).1 = problem;
    status(&app)
}

/// Put the address on the clipboard for pasting into an editor's settings.
#[tauri::command]
#[specta::specta]
pub(crate) fn copy_editor_endpoint(app: AppHandle) -> Result<(), String> {
    let url = status(&app).url.ok_or("The editor endpoint is off.")?;
    crate::copy::write_plain(&app, &url)
}

/// The format the waiting editor asked for; `None` when no editor is waiting.
#[tauri::command]
#[specta::specta]
pub(crate) fn editor_request(pending: State<'_, Pending>) -> Option<String> {
    pending.current().map(|request| request.label.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_engine::cayw::CaywOutput;
    use std::sync::Mutex as StdMutex;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    type Then = Box<dyn FnOnce(&Pending) + Send>;

    #[derive(Default)]
    struct Fake {
        pending: Pending,
        presented: AtomicUsize,
        changes: AtomicUsize,
        ready: AtomicBool,
        then: StdMutex<Option<Then>>,
    }

    impl Host for Fake {
        fn pending(&self) -> &Pending {
            &self.pending
        }
        fn ready(&self) -> bool {
            self.ready.load(Ordering::SeqCst)
        }
        fn style(&self) -> String {
            "apa".into()
        }
        fn changed(&self) {
            self.changes.fetch_add(1, Ordering::SeqCst);
        }
        fn present(&self) {
            self.presented.fetch_add(1, Ordering::SeqCst);
            if let Some(then) = self.then.lock().ok().and_then(|mut then| then.take()) {
                then(&self.pending);
            }
        }
    }

    fn server(fake: Fake) -> (Listening, u16, Arc<Fake>) {
        let host = Arc::new(fake);
        // Port 0 picks a free one for this test; the listener is what is tested.
        let probe = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap_or_else(|error| panic!("{error}"));
        let port = probe.local_addr().map(|address| address.port()).unwrap_or_default();
        drop(probe);
        let listening = listen(port, TOKEN.into(), Arc::clone(&host)).unwrap_or_else(|error| panic!("{error}"));
        (listening, port, host)
    }

    fn connect(port: u16) -> TcpStream {
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap_or_else(|error| panic!("{error}"));
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap_or_else(|error| panic!("{error}"));
        stream
    }

    fn send(stream: &mut TcpStream, port: u16, target: &str, headers: &str) {
        let text = format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{headers}\r\n");
        stream
            .write_all(text.as_bytes())
            .unwrap_or_else(|error| panic!("{error}"));
    }

    fn answer(stream: &mut TcpStream) -> String {
        let mut text = String::new();
        let _ = stream.read_to_string(&mut text);
        text
    }

    fn cayw(query: &str) -> String {
        format!("/{TOKEN}/better-bibtex/cayw{query}")
    }

    fn wait_for(what: impl Fn() -> bool) {
        let started = Instant::now();
        while !what() {
            assert!(started.elapsed() < Duration::from_secs(20), "timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_probe_answers_without_opening_the_picker() {
        let (_listening, port, host) = server(Fake::default());
        host.ready.store(true, Ordering::SeqCst);
        let mut stream = connect(port);
        send(&mut stream, port, &cayw("?probe=true"), "");
        let text = answer(&mut stream);
        assert!(
            text.starts_with("HTTP/1.1 200 OK\r\n") && text.ends_with("\r\n\r\nready"),
            "{text}"
        );
        assert_eq!(host.presented.load(Ordering::SeqCst), 0);

        host.ready.store(false, Ordering::SeqCst);
        let mut stream = connect(port);
        send(&mut stream, port, &cayw("?probe=true"), "");
        assert!(answer(&mut stream).ends_with("\r\n\r\nNo"));
    }

    #[test]
    fn a_pick_opens_the_picker_and_returns_what_the_person_chose() {
        let fake = Fake::default();
        *fake.then.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Box::new(|pending: &Pending| {
            let request = pending.current().unwrap_or_else(|| panic!("a request"));
            assert_eq!(request.label, "Pandoc");
            assert!(pending.answer(request.id, "@taylor2026"));
        }));
        let (_listening, port, host) = server(fake);
        let mut stream = connect(port);
        send(&mut stream, port, &cayw("?format=pandoc"), "");
        let text = answer(&mut stream);
        assert!(text.ends_with("\r\n\r\n@taylor2026"), "{text}");
        assert!(text.contains("Content-Type: text/plain; charset=utf-8"), "{text}");
        assert_eq!(host.presented.load(Ordering::SeqCst), 1);
        wait_for(|| host.pending.current().is_none());
    }

    #[test]
    fn hiding_the_picker_answers_with_nothing() {
        let fake = Fake::default();
        *fake.then.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Box::new(|pending: &Pending| {
            assert!(pending.cancel_any());
        }));
        let (_listening, port, _host) = server(fake);
        let mut stream = connect(port);
        send(&mut stream, port, &cayw(""), "");
        let text = answer(&mut stream);
        assert!(text.starts_with("HTTP/1.1 200 OK") && text.ends_with("Content-Length: 0\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n"), "{text}");
    }

    #[test]
    fn an_editor_that_gives_up_ends_its_request() {
        let (_listening, port, host) = server(Fake::default());
        let mut stream = connect(port);
        send(&mut stream, port, &cayw(""), "");
        wait_for(|| host.pending.current().is_some());
        drop(stream);
        wait_for(|| host.pending.current().is_none());
    }

    #[test]
    fn a_second_request_is_refused_and_the_first_still_gets_its_answer() {
        let (_listening, port, host) = server(Fake::default());
        let mut first = connect(port);
        send(&mut first, port, &cayw("?format=typst"), "");
        wait_for(|| host.pending.current().is_some());
        let mut second = connect(port);
        send(&mut second, port, &cayw("?format=pandoc"), "");
        assert!(answer(&mut second).starts_with("HTTP/1.1 409 Conflict"));

        let request = host.pending.current().unwrap_or_else(|| panic!("a request"));
        assert_eq!(request.label, "Typst", "the second changed nothing");
        assert!(host.pending.answer(request.id, "@a"));
        assert!(answer(&mut first).ends_with("@a"));
    }

    #[test]
    fn a_late_answer_for_an_ended_request_cannot_reach_a_newer_one() {
        let pending = Pending::default();
        let pick = match cayw::parse(
            format!("GET {} HTTP/1.1\r\nHost: 127.0.0.1:1234\r\n\r\n", cayw("")).as_bytes(),
            1234,
            TOKEN,
        ) {
            Ok(CaywRequest::Pick(pick)) => pick,
            other => panic!("{other:?}"),
        };
        let (first, _first_answer) = pending.begin(&pick, "apa").unwrap_or_else(|| panic!("a request"));
        assert!(pending.cancel(first));
        let (second, second_answer) = pending.begin(&pick, "apa").unwrap_or_else(|| panic!("a request"));
        assert!(!pending.answer(first, "late"), "A finished after B began");
        assert!(!pending.cancel(first));
        assert_eq!(pending.current().map(|request| request.id), Some(second));
        assert!(pending.answer(second, "newer"));
        assert_eq!(second_answer.recv().ok().flatten().as_deref(), Some("newer"));
    }

    #[test]
    fn refusals_arrive_as_fixed_text_and_open_nothing() {
        let (_listening, port, host) = server(Fake::default());
        for (target, headers, status) in [
            ("/better-bibtex/cayw?format=pandoc".to_owned(), "", "403"),
            (cayw("?format=eta"), "", "400"),
            (cayw("?format=pandoc"), "Origin: https://invented.example\r\n", "403"),
            (cayw("?format=pandoc"), "Sec-Fetch-Mode: no-cors\r\n", "403"),
            (format!("/{TOKEN}/nothing"), "", "404"),
        ] {
            let mut stream = connect(port);
            send(&mut stream, port, &target, headers);
            let text = answer(&mut stream);
            assert!(text.starts_with(&format!("HTTP/1.1 {status} ")), "{target}: {text}");
            assert!(!text.contains("invented.example") && !text.contains(TOKEN), "{text}");
            assert!(!text.to_ascii_lowercase().contains("access-control"), "{text}");
        }
        assert_eq!(host.presented.load(Ordering::SeqCst), 0);
        assert_eq!(host.pending.current(), None);
    }

    #[test]
    fn a_silent_or_oversized_client_is_dropped() {
        let (_listening, port, host) = server(Fake::default());
        let mut large = connect(port);
        let filler = vec![b'a'; cayw::MAX_HEAD_BYTES + 2048];
        let _ = large.write_all(&filler);
        assert!(answer(&mut large).starts_with("HTTP/1.1 431"));
        assert_eq!(host.presented.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn the_port_is_refused_to_others_and_freed_when_switched_off() {
        let (listening, port, _host) = server(Fake::default());
        assert!(
            TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_err(),
            "a busy port is reported, not shared"
        );
        let local = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap_or_else(|error| panic!("{error}"));
        assert!(local.peer_addr().is_ok_and(|peer| peer.ip().is_loopback()));
        drop(local);
        drop(listening);
        wait_for(|| TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok());
    }

    #[test]
    fn a_formatted_request_takes_the_persons_style_and_a_numeric_one_its_entry() {
        let pending = Pending::default();
        let pick = CaywPick {
            output: CaywOutput::Style,
            form: CitationForm::Normal,
            clipboard: false,
            label: "Formatted citation",
        };
        pending.begin(&pick, "ieee").unwrap_or_else(|| panic!("a request"));
        let request = pending.current().unwrap_or_else(|| panic!("a request"));
        assert_eq!(request.output, OutputSpec::Csl { style: "ieee".into() });
        assert_eq!(request.form, CitationForm::BibliographyOnly);
    }

    #[test]
    fn only_a_workable_setting_is_used() {
        let good = EditorEndpoint {
            port: DEFAULT_PORT,
            token: TOKEN.into(),
        };
        assert!(usable(&good));
        assert!(!usable(&EditorEndpoint {
            port: 80,
            ..good.clone()
        }));
        assert!(
            !usable(&EditorEndpoint {
                port: ZOTERO_PORT,
                ..good.clone()
            }),
            "never Zotero's"
        );
        assert!(!usable(&EditorEndpoint {
            token: "short".into(),
            ..good.clone()
        }));
        assert!(!usable(&EditorEndpoint {
            token: "../../etc/passwd/0000000000000000".into(),
            ..good
        }));
    }

    #[test]
    fn each_new_address_holds_a_new_token() {
        let (first, second) = (new_token(), new_token());
        assert_eq!(first.as_ref().map(String::len), Some(32));
        assert_ne!(first, second);
    }
}
