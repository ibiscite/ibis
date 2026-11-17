use super::*;
use std::sync::mpsc;
use std::time::Duration;

const LIMIT: Duration = Duration::from_secs(2);

struct Input {
    receiver: mpsc::Receiver<Vec<u8>>,
    current: io::Cursor<Vec<u8>>,
}
impl Input {
    fn new(receiver: mpsc::Receiver<Vec<u8>>) -> Self {
        Self {
            receiver,
            current: io::Cursor::new(Vec::new()),
        }
    }
}
impl Read for Input {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let read = self.current.read(buffer)?;
        if read != 0 {
            return Ok(read);
        }
        match self.receiver.recv_timeout(Duration::from_secs(5)) {
            Ok(bytes) => {
                self.current = io::Cursor::new(bytes);
                self.current.read(buffer)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Ok(0),
            Err(_) => Err(io::Error::new(io::ErrorKind::TimedOut, "synthetic client stalled")),
        }
    }
}

#[test]
fn mcp_cancellation_interrupts_csl_and_the_next_citation_finishes_promptly() {
    let (send, input) = mpsc::channel();
    let (output, replies) = mpsc::channel();
    let (entered, running) = mpsc::channel();
    let server = std::thread::spawn(move || {
        serve_with(
            &Engine::new().unwrap(),
            Session::default(),
            io::BufReader::new(Input::new(input)),
            Output(output),
            |engine, session, message, cancel| {
                if message["id"] == 1 {
                    entered.send(cancel.clone()).unwrap();
                }
                handle_with_cancellation(engine, session, message, cancel)
            },
        )
    });
    let request = |id, records: Value| {
        json!({"jsonrpc":"2.0", "id":id, "method":"tools/call", "params":{
            "name":"cite", "arguments":{"records":records, "style":"apa", "form":"bibliography-only"},
            "_meta":{VERSION_KEY:PROTOCOL_VERSION, CAPABILITIES_KEY:{}}
        }})
    };
    let send_message = |message: Value| send.send(format!("{message}\n").into_bytes()).unwrap();
    let records: Vec<_> = (0..1000)
        .map(|index| {
            json!({"id":format!("synthetic-{index}"), "type":"book",
        "title":format!("Synthetic {index}"), "author":[{"family":format!("Invented{index}")}],
        "issued":{"date-parts":[[2026]]}})
        })
        .collect();
    send_message(request(1, json!(records)));
    let cancel = running.recv_timeout(LIMIT).unwrap();
    let deadline = std::time::Instant::now() + LIMIT;
    while !cancel.has_formatting_task() && std::time::Instant::now() < deadline {
        std::thread::yield_now();
    }
    let started = cancel.has_formatting_task();
    send_message(json!({"jsonrpc":"2.0", "method":"notifications/cancelled", "params":{"requestId":1}}));
    send_message(request(2, json!([{"id":"b", "type":"book", "title":"Synthetic B"}])));
    let reply = replies.recv_timeout(LIMIT);
    drop(send);
    server.join().unwrap().unwrap();
    assert!(started, "the cancelled request must have reached CSL");
    let reply = reply.unwrap();
    assert_eq!(reply["id"], 2);
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    assert!(
        reply["result"]["structuredContent"]["text"]
            .as_str()
            .unwrap()
            .contains("Synthetic B")
    );
    assert!(replies.try_recv().is_err(), "cancelled CSL sends no late reply");
}
struct Output(mpsc::Sender<Value>);
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.send(serde_json::from_slice(bytes).unwrap()).unwrap();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn parallel_searches_queue_twenty_calls_and_overflow_is_a_retryable_tool_error() {
    for queued in [20, 65] {
        let (send, input) = mpsc::channel();
        let (output, replies) = mpsc::channel();
        let (entered, running) = mpsc::channel();
        let (release, blocked) = mpsc::channel();
        let blocked = Mutex::new(blocked);
        let server = std::thread::spawn(move || {
            let case: ibis_core::contract::GoldenCase =
                serde_json::from_str(include_str!("../../../../fixtures/cases/02-pandoc.json")).unwrap();
            serve_with(
                &Engine::from_snapshot(Arc::new(case.library)),
                Session::default(),
                io::BufReader::new(Input::new(input)),
                Output(output),
                |engine, session, message, cancel| {
                    if message["id"] == 0 {
                        entered.send(()).unwrap();
                        blocked.recover_lock().recv_timeout(Duration::from_secs(5)).unwrap();
                    }
                    handle_with_cancellation(engine, session, message, cancel)
                },
            )
        });
        let request = |id| {
            json!({"jsonrpc":"2.0", "id":id, "method":"tools/call", "params":{
                "name":"search", "arguments":{"query":"example"},
                "_meta":{VERSION_KEY:PROTOCOL_VERSION, CAPABILITIES_KEY:{}}
            }})
        };
        send.send(format!("{}\n", request(0)).into_bytes()).unwrap();
        running.recv_timeout(LIMIT).unwrap();
        for id in 1..=queued {
            send.send(format!("{}\n", request(id)).into_bytes()).unwrap();
        }
        // The ping is answered inline, proving every preceding call was queued.
        send.send(b"{\"jsonrpc\":\"2.0\",\"id\":1000,\"method\":\"ping\"}\n".to_vec())
            .unwrap();
        if queued > 64 {
            let busy = replies.recv_timeout(LIMIT).unwrap();
            assert_eq!(busy["id"], 65);
            assert!(busy.get("error").is_none(), "{busy}");
            assert_eq!(busy["result"]["isError"], true);
            assert_eq!(busy["result"]["resultType"], "complete");
            let text = busy["result"]["content"][0]["text"].as_str().unwrap();
            assert!(
                text.starts_with("error: ") && text.contains("Ibis is busy") && text.contains("Retry"),
                "{text}"
            );
            assert_eq!(
                busy["result"]["structuredContent"]["error"]["recovery"]["action"],
                "retry"
            );
        }
        assert_eq!(replies.recv_timeout(LIMIT).unwrap()["id"], 1000);
        release.send(()).unwrap();
        for id in 0..=queued.min(64) {
            let reply = replies.recv_timeout(LIMIT).unwrap();
            assert_eq!(reply["id"], id);
            assert_eq!(reply["result"]["isError"], false, "{reply}");
        }
        drop(send);
        server.join().unwrap().unwrap();
        assert!(replies.try_recv().is_err());
    }
}

#[test]
fn a_client_cancels_a_running_tool_then_gets_the_next_answer_without_a_late_result() {
    for id in [json!(7), json!("running")] {
        let (send, input) = mpsc::channel();
        let (output, replies) = mpsc::channel();
        let (entered, running) = mpsc::channel();
        let (stopped, cancelled) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let engine = Engine::new().unwrap();
            serve_with(
                &engine,
                Session::default(),
                io::BufReader::new(Input::new(input)),
                Output(output),
                |_, _, message, cancel| {
                    if message["params"]["name"] == "slow" {
                        entered.send(()).unwrap();
                        let deadline = std::time::Instant::now() + Duration::from_secs(5);
                        while !cancel.is_cancelled() && std::time::Instant::now() < deadline {
                            std::thread::park_timeout(Duration::from_millis(1));
                        }
                        stopped.send(cancel.is_cancelled()).unwrap();
                    }
                    Some(json!({"jsonrpc":"2.0", "id":message["id"], "result":{"text":message["params"]["name"]}}))
                },
            )
        });
        let send_message = |message: Value| send.send(format!("{message}\n").into_bytes()).unwrap();
        send_message(json!({"jsonrpc":"2.0", "id":id, "method":"tools/call", "params":{"name":"slow"}}));
        running.recv_timeout(LIMIT).unwrap();
        send_message(json!({"jsonrpc":"2.0", "method":"notifications/cancelled", "params":{"requestId":id}}));
        send_message(json!({"jsonrpc":"2.0", "id":id, "method":"tools/call", "params":{"name":"next"}}));
        let reply = replies.recv_timeout(LIMIT);
        let stopped_promptly = cancelled.recv_timeout(LIMIT);
        drop(send);
        server.join().unwrap().unwrap();
        assert!(stopped_promptly.unwrap(), "the running tool must observe cancellation");
        let reply = reply.unwrap();
        assert_eq!(reply["id"], id);
        assert_eq!(reply["result"]["text"], "next");
        assert!(replies.try_recv().is_err(), "cancelled requests send no late response");
    }
}

struct FailedOutput(mpsc::Sender<()>);
impl Write for FailedOutput {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        self.0.send(()).unwrap();
        Err(io::Error::new(io::ErrorKind::BrokenPipe, "synthetic closed output"))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_failed_tool_reply_stops_the_transport_on_the_next_input() {
    let (send, input) = mpsc::channel();
    let (failed, failure) = mpsc::channel();
    let (finished, result) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let result = serve_with(
            &Engine::new().unwrap(),
            Session::default(),
            io::BufReader::new(Input::new(input)),
            FailedOutput(failed),
            |_, _, message, _| Some(json!({"jsonrpc":"2.0", "id":message["id"], "result":{}})),
        );
        finished.send(result.map_err(|error| error.kind())).unwrap();
    });
    send.send(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\"}\n".to_vec())
        .unwrap();
    failure.recv_timeout(LIMIT).unwrap();
    send.send(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n".to_vec())
        .unwrap();
    let stopped = result.recv_timeout(LIMIT);
    drop(send);
    server.join().unwrap();
    assert_eq!(stopped.unwrap(), Err(io::ErrorKind::BrokenPipe));
}

#[test]
fn a_failed_tool_reply_after_input_eof_is_returned_to_the_caller() {
    struct EofInput(io::Cursor<Vec<u8>>, mpsc::SyncSender<()>);
    impl Read for EofInput {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            let read = self.0.read(bytes)?;
            if read == 0 {
                let _ = self.1.try_send(());
            }
            Ok(read)
        }
    }
    let (closed, eof) = mpsc::sync_channel(1);
    let eof = Mutex::new(eof);
    let (failed, _failure) = mpsc::channel();
    let input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\"}\n".to_vec();
    let result = serve_with(
        &Engine::new().unwrap(),
        Session::default(),
        io::BufReader::new(EofInput(io::Cursor::new(input), closed)),
        FailedOutput(failed),
        |_, _, message, _| {
            eof.recover_lock().recv_timeout(LIMIT).unwrap();
            Some(json!({"jsonrpc":"2.0", "id":message["id"], "result":{}}))
        },
    );
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::BrokenPipe);
}
