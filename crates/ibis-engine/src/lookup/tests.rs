//! No test reaches the network: a scripted transport stands in for the
//! services, and the real client talks only to a server on 127.0.0.1.
//! DOIs use the 10.5555 test prefix; every record is invented.
use super::*;
use crate::request::RecordsRequest;
use ibis_core::contract::{LibraryObject, Snapshot};
use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex as StdMutex};

const CROSSREF: &str = "https://api.crossref.org/v1/works/10.5555%2Fnew-1/transform";

#[derive(Clone)]
enum Scripted {
    Answer {
        status: u16,
        location: Option<&'static str>,
        content_type: Option<&'static str>,
        body: Vec<u8>,
    },
    Fail(Failure),
}

/// Answers in order and remembers every URL it was asked for.
#[derive(Clone, Default)]
struct Fake {
    script: Arc<StdMutex<Vec<Scripted>>>,
    asked: Arc<StdMutex<Vec<String>>>,
}

impl Fake {
    fn new(script: Vec<Scripted>) -> Self {
        Self {
            script: Arc::new(StdMutex::new(script)),
            asked: Arc::default(),
        }
    }
    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

impl Transport for Fake {
    fn get(&self, url: &str, within: Duration) -> Result<Reply, Failure> {
        assert!(within <= DEADLINE && !within.is_zero());
        self.asked.lock().unwrap().push(url.to_owned());
        let mut script = self.script.lock().unwrap();
        assert!(!script.is_empty(), "an unexpected request to {url}");
        match script.remove(0) {
            Scripted::Answer {
                status,
                location,
                content_type,
                body,
            } => Ok(Reply {
                status,
                location: location.map(str::to_owned),
                content_type: content_type.map(str::to_owned),
                body: Box::new(std::io::Cursor::new(body)),
            }),
            Scripted::Fail(failure) => Err(failure),
        }
    }
}

fn redirect(location: &'static str) -> Scripted {
    Scripted::Answer {
        status: 302,
        location: Some(location),
        content_type: Some("text/html;charset=utf-8"),
        body: b"<html>moved</html>".to_vec(),
    }
}
fn csl(body: Value) -> Scripted {
    Scripted::Answer {
        status: 200,
        location: None,
        content_type: Some("application/vnd.citationstyles.csl+json; charset=utf-8"),
        body: serde_json::to_vec(&body).unwrap(),
    }
}
fn status_only(status: u16) -> Scripted {
    Scripted::Answer {
        status,
        location: None,
        content_type: Some("text/plain"),
        body: vec![],
    }
}
fn answer() -> Value {
    json!({
        "type": "article-journal",
        "id": "https://doi.org/10.5555/new-1",
        "DOI": "10.5555/NEW-1",
        "title": "Lanterns over Invented Harbours",
        "author": [{"family": "Vale", "given": "Ada"}],
        "issued": {"date-parts": [[2024]]},
        "container-title": "Journal of Examples",
        "ISSN": ["0000-0000"],
        "indexed": {"date-time": "2026-01-01T00:00:00Z"},
        "citation-key": "planted",
        "note": "a service's note",
        "file": "/home/someone/planted.pdf",
    })
}

/// A library holding one invented record with DOI 10.5555/held-1.
fn engine() -> Engine {
    let records = ibis_sources::request_records(
        &LibraryId::digest(b"lookup tests"),
        &[json!({"id": "held", "type": "book", "title": "Held Lanterns", "DOI": "10.5555/HELD-1"})],
    )
    .unwrap()
    .records;
    Engine::from_snapshot(Arc::new(
        Snapshot::new(records.into_iter().map(LibraryObject::Record).collect()).unwrap(),
    ))
}

struct Settings {
    _folder: tempfile::TempDir,
    owner: settings::Owner,
}
fn settings(device: Value) -> Settings {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("settings.json");
    let mut file = json!({"schema_version": 2});
    file.as_object_mut()
        .unwrap()
        .extend(device.as_object().unwrap().clone());
    std::fs::write(&path, file.to_string()).unwrap();
    Settings {
        _folder: folder,
        owner: settings::Owner::open(path),
    }
}
fn on() -> Settings {
    settings(json!({"online_lookup": true, "doi_lookup_confirmed": true}))
}

fn look(fake: &Fake, settings: &Settings, query: &str, ask: Ask<'_>) -> Result<Found, EngineError> {
    Lookup::with_transport(fake.clone()).doi(&engine(), &settings.owner, query, ask, &Cancellation::default())
}
fn never(_: &Disclosure) -> Option<bool> {
    unreachable!("Ibis asked although nothing needed asking")
}
fn reason(result: Result<Found, EngineError>) -> Option<ErrorReason> {
    result.err().and_then(|error| error.reason)
}
fn looked_up(result: Result<Found, EngineError>) -> LookedUp {
    match result {
        Ok(Found::LookedUp(found)) => found,
        other => panic!("expected a looked-up record, got {other:?}"),
    }
}

#[test]
fn a_doi_already_in_a_library_is_found_in_any_spelling_and_nothing_is_sent() {
    let fake = Fake::new(vec![]);
    for query in [
        "10.5555/held-1",
        "10.5555/HELD-1",
        "doi:10.5555/held-1",
        "DOI: 10.5555/Held-1",
        "https://doi.org/10.5555/held-1",
        "HTTPS://DX.DOI.ORG/10.5555%2Fheld-1",
    ] {
        let Ok(Found::InLibrary(result)) = look(&fake, &on(), query, &mut never) else {
            panic!("{query} was not found in the library");
        };
        assert_eq!(result.hits.len(), 1, "{query}");
    }
    // Even with lookup off: a held DOI never needs the network.
    assert!(matches!(
        look(&fake, &settings(json!({})), "10.5555/held-1", &mut never),
        Ok(Found::InLibrary(_))
    ));
    assert!(fake.asked().is_empty());
}

#[test]
fn text_that_is_not_a_doi_is_refused_before_anything_else() {
    let fake = Fake::new(vec![]);
    for query in [
        "lanterns",
        "10.5555",
        "arXiv:2101.00001",
        "https://example.org/10.5555/x",
    ] {
        assert_eq!(
            reason(look(&fake, &on(), query, &mut never)),
            Some(ErrorReason::NotADoi),
            "{query}"
        );
    }
    assert!(fake.asked().is_empty());
}

#[test]
fn nothing_is_sent_while_lookup_is_off() {
    let fake = Fake::new(vec![]);
    for device in [
        json!({}),
        json!({"online_lookup": false}),
        json!({"online_lookup": false, "doi_lookup_confirmed": true}),
    ] {
        assert_eq!(
            reason(look(&fake, &settings(device), "10.5555/new-1", &mut never)),
            Some(ErrorReason::LookupOff)
        );
    }
    assert!(fake.asked().is_empty());
}

#[test]
fn the_first_doi_lookup_asks_saying_what_is_sent_where_and_remembers_a_yes() {
    let fake = Fake::new(vec![redirect(CROSSREF), csl(answer())]);
    let unconfirmed = settings(json!({"online_lookup": true}));
    let mut told = Vec::new();
    let mut cannot = |disclosure: &Disclosure| {
        told.push(disclosure.clone());
        None
    };
    assert_eq!(
        reason(look(&fake, &unconfirmed, "doi:10.5555/NEW-1", &mut cannot)),
        Some(ErrorReason::LookupUnconfirmed)
    );
    assert_eq!(
        told,
        [Disclosure {
            doi: "10.5555/new-1".into(),
            to: "doi.org",
            then: [LookupService::Crossref, LookupService::Datacite],
        }]
    );
    assert_eq!(
        reason(look(&fake, &unconfirmed, "10.5555/new-1", &mut |_| Some(false))),
        Some(ErrorReason::LookupDeclined)
    );
    assert!(fake.asked().is_empty());
    assert!(!unconfirmed.owner.read().cites().unwrap().device.doi_lookup_confirmed);

    looked_up(look(&fake, &unconfirmed, "10.5555/new-1", &mut |_| Some(true)));
    assert!(unconfirmed.owner.read().cites().unwrap().device.doi_lookup_confirmed);
    let fake = Fake::new(vec![redirect(CROSSREF), csl(answer())]);
    looked_up(look(&fake, &unconfirmed, "10.5555/new-1", &mut never));
}

#[test]
fn only_the_doi_goes_to_doi_org_and_then_to_the_one_named_service() {
    let fake = Fake::new(vec![redirect(CROSSREF), csl(answer())]);
    let found = looked_up(look(&fake, &on(), "https://doi.org/10.5555/New-1", &mut never));
    assert_eq!(fake.asked(), ["https://doi.org/10.5555/new-1", CROSSREF]);
    assert_eq!(
        found.origin,
        Origin::Lookup {
            doi: "10.5555/new-1".into(),
            service: LookupService::Crossref
        }
    );
    let fake = Fake::new(vec![
        redirect("https://data.crosscite.org/10.5555%2Fnew-1"),
        csl(answer()),
    ]);
    let found = looked_up(look(&fake, &on(), "10.5555/new-1", &mut never));
    assert!(matches!(
        found.origin,
        Origin::Lookup {
            service: LookupService::Datacite,
            ..
        }
    ));
}

#[test]
fn the_answer_keeps_only_bibliographic_csl_and_is_cited_through_the_records_path() {
    let fake = Fake::new(vec![redirect(CROSSREF), csl(answer())]);
    let found = looked_up(look(&fake, &on(), "10.5555/new-1", &mut never));
    let record = found.record.as_object().unwrap();
    let mut fields: Vec<&str> = record.keys().map(String::as_str).collect();
    fields.sort_unstable();
    assert_eq!(
        fields,
        ["DOI", "author", "container-title", "id", "issued", "title", "type"]
    );
    assert_eq!(record["id"], "lookup");
    let citation = engine()
        .cite_records(
            &RecordsRequest {
                strict: true,
                records: vec![found.record],
                output: serde_json::from_value(json!({"kind": "preset", "name": "apa"})).unwrap(),
                form: Default::default(),
                output_locale: None,
                items: None,
            },
            &Cancellation::default(),
        )
        .unwrap();
    assert_eq!(citation.text, "(Vale, 2024)");
}

#[test]
fn a_redirect_anywhere_but_a_named_service_is_refused_and_never_followed() {
    for location in [
        "http://api.crossref.org/v1/works/10.5555%2Fnew-1/transform",
        "https://api.crossref.org:8443/v1/works/10.5555%2Fnew-1/transform",
        "https://someone@api.crossref.org/v1/works/10.5555%2Fnew-1/transform",
        "https://api.crossref.org.example.org/v1/works",
        "https://example.org/10.5555/new-1",
        "https://api.crossref.org/v1/works#fragment",
        "/v1/works/10.5555%2Fnew-1/transform",
        "",
    ] {
        let fake = Fake::new(vec![redirect(location)]);
        assert_eq!(
            reason(look(&fake, &on(), "10.5555/new-1", &mut never)),
            Some(ErrorReason::LookupMalformed),
            "{location}"
        );
        assert_eq!(fake.asked().len(), 1, "{location} was followed");
    }
    let fake = Fake::new(vec![status_only(302)]);
    assert_eq!(
        reason(look(&fake, &on(), "10.5555/new-1", &mut never)),
        Some(ErrorReason::LookupMalformed)
    );
}

#[test]
fn each_failure_is_its_own_typed_refusal() {
    let cases = [
        (vec![status_only(404)], ErrorReason::DoiNotFound),
        (vec![status_only(429)], ErrorReason::LookupRateLimited),
        (vec![status_only(503)], ErrorReason::LookupUnavailable),
        // `doi.org` itself never gives the record.
        (vec![csl(answer())], ErrorReason::LookupMalformed),
        (vec![redirect(CROSSREF), status_only(404)], ErrorReason::DoiNotFound),
        (
            vec![redirect(CROSSREF), status_only(429)],
            ErrorReason::LookupRateLimited,
        ),
        (
            vec![redirect(CROSSREF), redirect(CROSSREF)],
            ErrorReason::LookupMalformed,
        ),
        (
            vec![Scripted::Fail(Failure::Unreachable)],
            ErrorReason::LookupUnreachable,
        ),
        (vec![Scripted::Fail(Failure::Timeout)], ErrorReason::LookupTimeout),
        (
            vec![redirect(CROSSREF), Scripted::Fail(Failure::Timeout)],
            ErrorReason::LookupTimeout,
        ),
        (
            vec![
                redirect(CROSSREF),
                Scripted::Answer {
                    status: 200,
                    location: None,
                    content_type: Some("text/html"),
                    body: serde_json::to_vec(&answer()).unwrap(),
                },
            ],
            ErrorReason::LookupMalformed,
        ),
        (
            vec![redirect(CROSSREF), csl(json!([answer()]))],
            ErrorReason::LookupMalformed,
        ),
        (
            vec![redirect(CROSSREF), csl(json!({"title": "No type"}))],
            ErrorReason::LookupMalformed,
        ),
        (
            vec![redirect(CROSSREF), csl(json!({"type": "book", "DOI": "10.5555/other"}))],
            ErrorReason::LookupMalformed,
        ),
        (
            vec![
                redirect(CROSSREF),
                Scripted::Answer {
                    status: 200,
                    location: None,
                    content_type: Some(CSL_JSON),
                    body: b"{\"type\": \"book\", ".to_vec(),
                },
            ],
            ErrorReason::LookupMalformed,
        ),
    ];
    for (script, expected) in cases {
        let fake = Fake::new(script);
        assert_eq!(reason(look(&fake, &on(), "10.5555/new-1", &mut never)), Some(expected));
    }
}

#[test]
fn an_answer_over_one_mebibyte_is_refused_whatever_its_length_header_says() {
    let mut large = serde_json::to_vec(&answer()).unwrap();
    large.pop();
    large.extend(b",\"abstract\":\"");
    large.resize(MAX_ANSWER_BYTES as usize + 1, b'a');
    let fake = Fake::new(vec![
        redirect(CROSSREF),
        Scripted::Answer {
            status: 200,
            location: None,
            content_type: Some(CSL_JSON),
            body: large,
        },
    ]);
    let error = look(&fake, &on(), "10.5555/new-1", &mut never).unwrap_err();
    assert_eq!(error.code, ErrorCode::ResourceLimit);
    assert_eq!(error.reason, Some(ErrorReason::LookupTooLarge));
}

#[test]
fn lookup_is_no_engine_method_and_no_mcp_tool() {
    assert!(
        crate::Method::ALL
            .iter()
            .all(|method| !method.as_str().contains("lookup"))
    );
    let tools = include_str!("../../../../fixtures/mcp-tools.json");
    assert!(!tools.contains("lookup") && !tools.contains("doi.org"));
}

/// A server on 127.0.0.1 that keeps the request it was sent and answers with `reply`.
fn server(reply: impl FnOnce(&mut std::net::TcpStream) + Send + 'static) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut head = String::new();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                break;
            }
            head.push_str(&line);
        }
        reply(&mut stream);
        head
    });
    (format!("http://{address}/10.5555/new-1"), handle)
}

#[test]
fn the_real_client_sends_only_the_doi_and_a_generic_name_and_follows_no_redirect() {
    let (url, sent) = server(|stream| {
        let _ = stream.write_all(
            b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/elsewhere\r\nSet-Cookie: a=b\r\nContent-Length: 0\r\n\r\n",
        );
    });
    let reply = Https::new(false).get(&url, Duration::from_secs(5)).unwrap();
    assert_eq!(reply.status, 302);
    assert_eq!(reply.location.as_deref(), Some("http://127.0.0.1:9/elsewhere"));
    let head = sent.join().unwrap();
    let mut lines = head.lines();
    assert_eq!(lines.next(), Some("GET /10.5555/new-1 HTTP/1.1"));
    let mut headers: Vec<String> = lines.map(str::to_ascii_lowercase).collect();
    headers.sort();
    let host = url.trim_start_matches("http://").split('/').next().unwrap().to_owned();
    assert_eq!(
        headers,
        [
            format!("accept: {CSL_JSON}"),
            format!("host: {host}"),
            format!("user-agent: ibis/{}", env!("CARGO_PKG_VERSION")),
        ]
    );
}

#[test]
fn the_real_client_gives_up_on_a_silent_server_within_its_time() {
    let (url, _sent) = server(|_| std::thread::sleep(Duration::from_secs(3)));
    let start = Instant::now();
    let failure = Https::new(false).get(&url, Duration::from_millis(300)).err();
    assert_eq!(failure, Some(Failure::Timeout));
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn the_real_client_gives_up_on_an_answer_that_trickles_past_its_time() {
    let (url, _sent) = server(|stream| {
        let _ = stream.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: application/vnd.citationstyles.csl+json\r\nContent-Length: 100\r\n\r\n",
        );
        for _ in 0..30 {
            if stream.write_all(b" ").is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    let start = Instant::now();
    let mut reply = Https::new(false).get(&url, Duration::from_millis(500)).unwrap();
    let mut body = Vec::new();
    let failure = reply.body.read_to_end(&mut body).map_err(Failure::from).err();
    assert_eq!(failure, Some(Failure::Timeout));
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn the_real_client_never_speaks_plain_http_to_a_service() {
    let failure = Https::new(true)
        .get("http://127.0.0.1:9/10.5555/new-1", Duration::from_secs(1))
        .err();
    assert_eq!(failure, Some(Failure::Unreachable));
}
