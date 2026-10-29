//! Ordered JSON-RPC execution. The reader can cancel running or queued requests.
use crate::{
    Cancellation, DispatchResult, Engine, LibrarySubscription, Method,
    protocol::{self, FrameError},
    rpc::{self, Message, RequestId},
};
use ibis_core::contract::{EngineError, ErrorCode};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{BufRead, Write},
    sync::{Arc, mpsc},
    thread,
};

type Dispatch = dyn Fn(Method, &Value, &Cancellation) -> Result<DispatchResult, EngineError> + Send + Sync;
struct Job {
    id: Option<RequestId>,
    method: Method,
    params: Value,
    cancel: Cancellation,
}

pub fn serve<R: BufRead, W: Write + Send>(engine: Engine, reader: R, writer: W) -> Result<(), FrameError> {
    let changes = engine.subscribe().map_err(|_| FrameError::Io)?;
    run_with_changes(
        Arc::new(move |method, params, cancel| engine.dispatch(method, params, cancel)),
        reader,
        writer,
        Some(changes),
    )
}
#[cfg(test)]
fn run<R: BufRead, W: Write + Send>(dispatch: Arc<Dispatch>, reader: R, writer: W) -> Result<(), FrameError> {
    run_with_changes(dispatch, reader, writer, None)
}
fn run_with_changes<R: BufRead, W: Write + Send>(
    dispatch: Arc<Dispatch>,
    mut reader: R,
    writer: W,
    changes: Option<LibrarySubscription>,
) -> Result<(), FrameError> {
    thread::scope(|scope| {
        let (output, frames) = mpsc::sync_channel::<Value>(8);
        let writing = scope.spawn(move || {
            let mut writer = writer;
            for message in frames {
                protocol::write_frame(&mut writer, &message)?;
            }
            Ok(())
        });
        let close_changes = changes.as_ref().map(LibrarySubscription::close_handle);
        let forwarding = changes.map(|changes| {
            let output = output.clone();
            scope.spawn(move || {
                while changes.recv().is_ok() {
                    if output
                        .send(json!({"jsonrpc":"2.0","method":"ibis/libraryChanged","params":{}}))
                        .is_err()
                    {
                        break;
                    }
                }
            })
        });
        let (requests, jobs) = mpsc::sync_channel::<Job>(8);
        let (completed, finished) = mpsc::channel::<RequestId>();
        let worker_output = output.clone();
        let working = scope.spawn(move || {
            for job in jobs {
                let result = ibis_core::panic_guard::attempt(|| {
                    job.cancel.check()?;
                    let result = dispatch(job.method, &job.params, &job.cancel)?;
                    job.cancel.check()?;
                    Ok(result)
                })
                .unwrap_or(Err(EngineError {
                    code: ErrorCode::Internal,
                }));
                if let Some(id) = job.id {
                    let mut reply = match result {
                        Ok(result) => rpc::success(&id, result.data),
                        Err(error) => rpc::failure(Some(&id), error),
                    };
                    if crate::wire::encode(&reply, protocol::MAX_RESPONSE_BYTES).is_err() {
                        reply = rpc::failure(
                            Some(&id),
                            EngineError {
                                code: ErrorCode::ResourceLimit,
                            },
                        );
                    }
                    // Retire this token before exposing its reply, so ID reuse is fresh.
                    let _ = completed.send(id);
                    if worker_output.send(reply).is_err() {
                        break;
                    }
                }
            }
        });
        let mut active = HashMap::<RequestId, Cancellation>::new();
        let transport = (|| {
            loop {
                let frame = match protocol::read_frame(&mut reader) {
                    Ok(Some(frame)) => frame,
                    Ok(None) => return Ok(()),
                    Err(error) => {
                        output.send(rpc::parse_error()).map_err(|_| FrameError::Io)?;
                        if error == FrameError::InvalidJson {
                            continue;
                        }
                        return Err(error);
                    }
                };
                for id in finished.try_iter() {
                    active.remove(&id);
                }
                let reply = match rpc::parse_message(&frame) {
                    Message::Cancel(id) => {
                        if let Some(cancel) = active.get(&id) {
                            cancel.cancel();
                        }
                        None
                    }
                    Message::Ignore => None,
                    Message::Reply(reply) => Some(reply),
                    Message::Request(request) => {
                        if request.id.as_ref().is_some_and(|id| active.contains_key(id)) {
                            Some(rpc::invalid_request(None))
                        } else {
                            let cancel = Cancellation::default();
                            // Never block the reader on a full queue: it must still read cancels.
                            match requests.try_send(Job {
                                id: request.id.clone(),
                                method: request.method,
                                params: request.params.clone(),
                                cancel: cancel.clone(),
                            }) {
                                Ok(()) => {
                                    if let Some(id) = request.id {
                                        active.insert(id, cancel);
                                    }
                                    None
                                }
                                Err(mpsc::TrySendError::Full(_)) => request.id.as_ref().map(rpc::busy),
                                Err(mpsc::TrySendError::Disconnected(_)) => return Err(FrameError::Io),
                            }
                        }
                    }
                };
                if let Some(reply) = reply {
                    output.send(reply).map_err(|_| FrameError::Io)?;
                }
            }
        })();
        // EOF drains accepted work. Only the reader owns the active-token map.
        drop(requests);
        let worked = working.join().map_err(|_| FrameError::Io);
        drop(close_changes);
        if let Some(forwarding) = forwarding {
            forwarding.join().map_err(|_| FrameError::Io)?;
        }
        drop(output);
        let result = writing.join().map_err(|_| FrameError::Io)?;
        transport.and(worked).and(result)
    })
}
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::{
        io::{self, BufReader, Cursor, Read},
        time::{Duration, Instant},
    };
    struct Input {
        chunks: mpsc::Receiver<Vec<u8>>,
        current: Cursor<Vec<u8>>,
    }
    impl Read for Input {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            loop {
                let count = self.current.read(bytes)?;
                if count > 0 {
                    return Ok(count);
                }
                match self.chunks.recv() {
                    Ok(chunk) => self.current = Cursor::new(chunk),
                    Err(_) => return Ok(0),
                }
            }
        }
    }
    struct Output {
        frames: mpsc::Sender<Vec<u8>>,
        current: Vec<u8>,
    }
    impl Write for Output {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.current.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            self.frames
                .send(std::mem::take(&mut self.current))
                .map_err(|_| io::Error::other("closed"))
        }
    }
    fn send(input: &mpsc::Sender<Vec<u8>>, message: Value) {
        let mut bytes = vec![];
        protocol::write_frame(&mut bytes, &message).unwrap();
        input.send(bytes).unwrap();
    }
    fn reply(output: &mpsc::Receiver<Vec<u8>>) -> Value {
        let bytes = output.recv_timeout(Duration::from_secs(5)).unwrap();
        protocol::read_frame(&mut Cursor::new(bytes)).unwrap().unwrap()
    }
    #[test]
    fn cancelled_ids_are_fresh_on_reuse_and_duplicates_do_not_replace_active_work() {
        let (input, chunks) = mpsc::channel();
        let (frames, output) = mpsc::channel();
        let (started, ready) = mpsc::channel();
        let dispatch = Arc::new(move |_: Method, params: &Value, cancel: &Cancellation| {
            if params["wait"] == true {
                started.send(()).unwrap();
                let began = Instant::now();
                while !cancel.is_cancelled() {
                    assert!(began.elapsed() < Duration::from_secs(5));
                    thread::yield_now();
                }
                cancel.check()?;
            }
            Ok(DispatchResult {
                data: json!("fresh"),
                exit_code: ibis_core::contract::ExitCode::Ok,
                text: None,
                source_findings: false,
            })
        });
        let server = thread::spawn(move || {
            run(
                dispatch,
                BufReader::new(Input {
                    chunks,
                    current: Cursor::new(vec![]),
                }),
                Output {
                    frames,
                    current: vec![],
                },
            )
        });
        send(
            &input,
            json!({"jsonrpc":"2.0","id":7,"method":"search","params":{"wait":true}}),
        );
        ready.recv_timeout(Duration::from_secs(5)).unwrap();
        send(&input, json!({"jsonrpc":"2.0","id":7,"method":"libraries"}));
        let duplicate = reply(&output);
        assert!(duplicate["id"].is_null());
        assert_eq!(duplicate["error"]["code"], -32600);
        send(
            &input,
            json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":7}}),
        );
        let cancelled = reply(&output);
        assert_eq!(cancelled["id"], 7);
        assert_eq!(cancelled["error"]["data"]["code"], "cancelled");
        // A cancellation after completion does not persist into the next lifetime.
        send(
            &input,
            json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":7}}),
        );
        send(&input, json!({"jsonrpc":"2.0","id":7,"method":"libraries"}));
        assert_eq!(reply(&output)["result"], "fresh");
        drop(input);
        assert!(server.join().unwrap().is_ok());
    }
    #[test]
    fn replies_use_actual_frame_capacity_and_report_resource_limits() {
        for (size, overflow) in [(600_000, false), (protocol::MAX_RESPONSE_BYTES, true)] {
            let dispatch = Arc::new(move |_: Method, _: &Value, _: &Cancellation| {
                Ok(DispatchResult {
                    data: json!("x".repeat(size)),
                    text: None,
                    source_findings: false,
                    exit_code: ibis_core::contract::ExitCode::Ok,
                })
            });
            let mut input = vec![];
            protocol::write_frame(&mut input, &json!({"jsonrpc":"2.0","id":0,"method":"show"})).unwrap();
            let mut output = vec![];
            run(dispatch, Cursor::new(input), &mut output).unwrap();
            let mut reader = Cursor::new(output);
            let reply = protocol::read_response_frame(&mut reader).unwrap().unwrap();
            assert_eq!(reply["id"], 0);
            if overflow {
                assert_eq!(reply["error"]["data"]["code"], "resource-limit");
            } else {
                assert_eq!(reply["result"].as_str().map(str::len), Some(size));
            }
            assert_eq!(protocol::read_response_frame(&mut reader).unwrap(), None);
        }
    }
    #[test]
    fn library_changes_are_native_notifications_without_paths() {
        let engine = Engine::new().unwrap();
        let changes = engine.subscribe().unwrap();
        let (input, chunks) = mpsc::channel();
        let (frames, output) = mpsc::channel();
        let dispatch_engine = engine.clone();
        let server = thread::spawn(move || {
            run_with_changes(
                Arc::new(move |method, params, cancel| dispatch_engine.dispatch(method, params, cancel)),
                BufReader::new(Input {
                    chunks,
                    current: Cursor::new(vec![]),
                }),
                Output {
                    frames,
                    current: vec![],
                },
                Some(changes),
            )
        });
        engine
            .publish_library(
                ibis_core::contract::LibraryId::file(b"synthetic event"),
                Arc::new(ibis_core::contract::Snapshot::new(vec![]).unwrap()),
            )
            .unwrap();
        let bytes = output.recv_timeout(Duration::from_secs(5)).unwrap();
        let message = protocol::read_frame(&mut Cursor::new(bytes)).unwrap().unwrap();
        assert_eq!(message["method"], "ibis/libraryChanged");
        assert_eq!(message["params"], json!({}));
        assert!(message.get("id").is_none());
        let notification = message.to_string();
        assert!(!notification.contains("path"));
        assert!(!notification.contains("synthetic event"));
        drop(input);
        assert!(server.join().unwrap().is_ok());
    }
    #[test]
    fn queued_requests_are_ordered_bounded_and_cancellable() {
        let (input, chunks) = mpsc::channel();
        let (frames, output) = mpsc::channel();
        let (started, ready) = mpsc::channel();
        let dispatch = Arc::new(move |_: Method, params: &Value, cancel: &Cancellation| {
            started.send(params["position"].clone()).unwrap();
            if params["position"] == 0 {
                let began = Instant::now();
                while !cancel.is_cancelled() {
                    assert!(began.elapsed() < Duration::from_secs(5));
                    thread::yield_now();
                }
                cancel.check()?;
            }
            Ok(DispatchResult {
                data: params["position"].clone(),
                exit_code: ibis_core::contract::ExitCode::Ok,
                text: None,
                source_findings: false,
            })
        });
        let server = thread::spawn(move || {
            run(
                dispatch,
                BufReader::new(Input {
                    chunks,
                    current: Cursor::new(vec![]),
                }),
                Output {
                    frames,
                    current: vec![],
                },
            )
        });
        send(
            &input,
            json!({"jsonrpc":"2.0","id":0,"method":"search","params":{"position":0}}),
        );
        assert_eq!(ready.recv_timeout(Duration::from_secs(5)).unwrap(), 0);
        for id in 1..=8 {
            send(
                &input,
                json!({"jsonrpc":"2.0","id":id,"method":"search","params":{"position":id}}),
            );
        }
        send(&input, json!({"jsonrpc":"2.0","id":9,"method":"libraries"}));
        assert_eq!(reply(&output)["error"]["message"], "Server busy");
        send(&input, json!({"jsonrpc":"2.0","id":1,"method":"libraries"}));
        assert_eq!(reply(&output)["error"]["code"], -32600);
        for id in [1, 0] {
            send(
                &input,
                json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":id}}),
            );
        }
        for id in 0..=8 {
            let response = reply(&output);
            assert_eq!(response["id"], id);
            if id <= 1 {
                assert_eq!(response["error"]["data"]["code"], "cancelled");
            } else {
                assert_eq!(response["result"], id);
                assert_eq!(ready.recv_timeout(Duration::from_secs(5)).unwrap(), id);
            }
        }
        // A cancelled queued ID and a refused ID can both be reused.
        for id in [1, 9] {
            send(
                &input,
                json!({"jsonrpc":"2.0","id":id,"method":"search","params":{"position":id}}),
            );
            assert_eq!(reply(&output)["result"], id);
        }
        drop(input);
        assert!(server.join().unwrap().is_ok());
    }
    #[test]
    fn worker_panics_are_fixed_errors_and_later_requests_complete() {
        const CHILD: &str = "IBIS_RPC_PANIC_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "server::tests::worker_panics_are_fixed_errors_and_later_requests_complete",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            let began = Instant::now();
            while child.try_wait().unwrap().is_none() {
                if began.elapsed() > Duration::from_secs(10) {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("panic-guard subprocess timed out");
                }
                thread::sleep(Duration::from_millis(10));
            }
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success());
            assert!(output.stderr.is_empty(), "caught worker panic reached stderr");
            assert!(!String::from_utf8_lossy(&output.stdout).contains("synthetic worker failure"));
            return;
        }

        let dispatch = Arc::new(|_: Method, params: &Value, _: &Cancellation| {
            assert!(params["panic"] != true, "synthetic worker failure");
            Ok(DispatchResult {
                data: json!([]),
                exit_code: ibis_core::contract::ExitCode::Ok,
                text: None,
                source_findings: false,
            })
        });
        let mut input = vec![];
        for message in [
            json!({"jsonrpc":"2.0","id":1,"method":"search","params":{"panic":true}}),
            json!({"jsonrpc":"2.0","id":2,"method":"libraries"}),
        ] {
            protocol::write_frame(&mut input, &message).unwrap();
        }
        let mut output = vec![];
        run(dispatch, Cursor::new(input), &mut output).unwrap();
        let mut reader = Cursor::new(output);
        let failed = protocol::read_frame(&mut reader).unwrap().unwrap();
        assert_eq!(failed["id"], 1);
        assert_eq!(failed["error"]["code"], -32603);
        assert_eq!(failed["error"]["data"]["code"], "internal");
        assert!(!failed.to_string().contains("synthetic worker failure"));
        let next = protocol::read_frame(&mut reader).unwrap().unwrap();
        assert_eq!(next["id"], 2);
        assert_eq!(next["result"], json!([]));
        assert_eq!(protocol::read_frame(&mut reader).unwrap(), None);
    }
}
