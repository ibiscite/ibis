//! Bounded concurrent JSON-RPC execution. A token belongs to one request lifetime.
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

const MAX_WORKERS: usize = 8;
const MAX_BATCH: usize = 32;
type Dispatch = dyn Fn(Method, &Value, &Cancellation) -> Result<DispatchResult, EngineError> + Send + Sync;
struct Job {
    id: Option<RequestId>,
    method: Method,
    params: Value,
    cancel: Cancellation,
    generation: u64,
}
struct Active {
    generation: u64,
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
        let stopping = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let forwarding = changes.map(|changes| {
            let output = output.clone();
            let stopping = stopping.clone();
            scope.spawn(move || {
                loop {
                    let stopped = stopping.load(std::sync::atomic::Ordering::Acquire);
                    let event = if stopped {
                        changes.try_recv().ok()
                    } else {
                        match changes.recv_timeout(std::time::Duration::from_millis(20)) {
                            Ok(event) => Some(event),
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                            Err(_) => None,
                        }
                    };
                    let Some(event) = event else { break };
                    if output
                        .send(json!({"jsonrpc":"2.0","method":"ibis/libraryChanged","params":event}))
                        .is_err()
                    {
                        break;
                    }
                    if stopped {
                        break;
                    }
                }
            })
        });
        let (completed, finished) = mpsc::channel::<(RequestId, u64)>();
        let mut workers: Vec<thread::JoinHandle<()>> = vec![];
        let mut active = HashMap::<RequestId, Active>::new();
        let mut generation = 0_u64;
        let mut transport = Ok(());
        loop {
            let frame = match protocol::read_frame(&mut reader) {
                Ok(Some(frame)) => frame,
                Ok(None) => break,
                Err(error) => {
                    if output.send(rpc::parse_error()).is_err() {
                        transport = Err(FrameError::Io);
                        break;
                    }
                    if error == FrameError::InvalidJson {
                        continue;
                    }
                    transport = Err(error);
                    break;
                }
            };
            while let Ok((id, generation)) = finished.try_recv() {
                if active.get(&id).is_some_and(|request| request.generation == generation) {
                    active.remove(&id);
                }
            }
            let mut at = 0;
            while at < workers.len() {
                if workers[at].is_finished() {
                    let _ = workers.swap_remove(at).join();
                } else {
                    at += 1;
                }
            }
            let batch = frame.is_array();
            let messages = match frame {
                Value::Array(messages) if messages.is_empty() || messages.len() > MAX_BATCH => {
                    if output.send(rpc::invalid_request(None)).is_err() {
                        transport = Err(FrameError::Io);
                        break;
                    }
                    continue;
                }
                Value::Array(messages) => messages,
                message => vec![message],
            };
            let mut replies = vec![];
            let mut jobs = vec![];
            for message in &messages {
                match rpc::parse_message(message) {
                    Message::Cancel(id) => {
                        if let Some(request) = active.get(&id) {
                            request.cancel.cancel();
                        }
                    }
                    Message::Ignore => {}
                    Message::Reply(reply) => replies.push(reply),
                    Message::Request(request) => {
                        if request.id.as_ref().is_some_and(|id| active.contains_key(id)) {
                            replies.push(rpc::invalid_request(None));
                            continue;
                        }
                        if workers.len() >= MAX_WORKERS {
                            if let Some(id) = request.id {
                                replies.push(rpc::busy(&id));
                            }
                            continue;
                        }
                        generation = generation.wrapping_add(1);
                        let cancel = Cancellation::default();
                        if let Some(id) = &request.id {
                            active.insert(
                                id.clone(),
                                Active {
                                    generation,
                                    cancel: cancel.clone(),
                                },
                            );
                        }
                        jobs.push(Job {
                            id: request.id,
                            method: request.method,
                            params: request.params.clone(),
                            cancel,
                            generation,
                        });
                    }
                }
            }
            if jobs.is_empty() {
                if let Some(reply) = response(batch, replies)
                    && output.send(reply).is_err()
                {
                    transport = Err(FrameError::Io);
                    break;
                }
                continue;
            }
            let dispatch = dispatch.clone();
            let output = output.clone();
            let completed = completed.clone();
            // At most MAX_WORKERS batches (MAX_BATCH jobs each) can execute.
            let mut responses = Replies::new(
                batch,
                replies.len() + jobs.iter().filter(|job| job.id.is_some()).count(),
            );
            for reply in replies {
                responses.push(reply);
            }
            match thread::Builder::new().name("ibis-rpc".into()).spawn(move || {
                for job in jobs {
                    if let Some(id) = &job.id {
                        let _ = output.send(progress(id, job.generation, "begin"));
                    }
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
                        let reply = match result {
                            Ok(result) => rpc::success(&id, result.data),
                            Err(error) => rpc::failure(Some(&id), error),
                        };
                        responses.push(reply);
                        let _ = output.send(progress(&id, job.generation, "end"));
                        let _ = completed.send((id, job.generation));
                    }
                }
                if let Some(reply) = responses.finish() {
                    let _ = output.send(reply);
                }
            }) {
                Ok(worker) => workers.push(worker),
                Err(_) => {
                    transport = Err(FrameError::Io);
                    break;
                }
            }
        }
        // EOF drains accepted work. A new use of an ID always gets a new token.
        for worker in workers {
            let _ = worker.join();
        }
        stopping.store(true, std::sync::atomic::Ordering::Release);
        if let Some(forwarding) = forwarding {
            forwarding.join().map_err(|_| FrameError::Io)?;
        }
        drop(output);
        let result = writing.join().map_err(|_| FrameError::Io)?;
        transport.and(result)
    })
}
/// Aggregate actual bytes incrementally, reserving a bounded error for every
/// remaining response. Notifications consume no response budget.
struct Replies {
    batch: bool,
    remaining: usize,
    bytes: usize,
    messages: Vec<Value>,
}
impl Replies {
    fn new(batch: bool, remaining: usize) -> Self {
        Self {
            batch,
            remaining,
            bytes: if batch { 2 } else { 0 },
            messages: vec![],
        }
    }
    fn push(&mut self, mut message: Value) {
        self.remaining -= 1;
        let separator = usize::from(self.batch && !self.messages.is_empty());
        let available = protocol::MAX_RESPONSE_BYTES
            .saturating_sub(self.bytes + separator + self.remaining * protocol::RESPONSE_OVERHEAD);
        let size = match crate::wire::encode(&message, available) {
            Ok(bytes) => bytes.len(),
            Err(_) => {
                let id = message.get("id").and_then(RequestId::parse);
                message = rpc::failure(
                    id.as_ref(),
                    EngineError {
                        code: ErrorCode::ResourceLimit,
                    },
                );
                // IDs were bounded on input; this fixed error fits its reserved space.
                match crate::wire::encode(&message, protocol::RESPONSE_OVERHEAD) {
                    Ok(bytes) => bytes.len(),
                    Err(_) => {
                        message = rpc::failure(
                            None,
                            EngineError {
                                code: ErrorCode::Internal,
                            },
                        );
                        protocol::RESPONSE_OVERHEAD
                    }
                }
            }
        };
        self.bytes += separator + size;
        self.messages.push(message);
    }
    fn finish(mut self) -> Option<Value> {
        if self.messages.is_empty() {
            None
        } else if self.batch {
            Some(Value::Array(self.messages))
        } else {
            self.messages.pop()
        }
    }
}
fn response(batch: bool, messages: Vec<Value>) -> Option<Value> {
    let mut replies = Replies::new(batch, messages.len());
    for message in messages {
        replies.push(message);
    }
    replies.finish()
}

fn progress(id: &RequestId, generation: u64, kind: &str) -> Value {
    json!({"jsonrpc":"2.0","method":"$/progress","params":{"token":format!("ibis:{generation}"),"value":{"kind":kind,"title":"Processing","request":id.value()}}})
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
        loop {
            let bytes = output.recv_timeout(Duration::from_secs(5)).unwrap();
            let message = protocol::read_frame(&mut Cursor::new(bytes)).unwrap().unwrap();
            if message.get("id").is_some() || message.is_array() {
                return message;
            }
        }
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
        for (count, size, overflow) in [(1, 600_000, false), (32, 60_000, false), (32, 600_000, true)] {
            let dispatch = Arc::new(move |_: Method, _: &Value, _: &Cancellation| {
                Ok(DispatchResult {
                    data: json!("x".repeat(size)),
                    text: None,
                    source_findings: false,
                    exit_code: ibis_core::contract::ExitCode::Ok,
                })
            });
            let requests: Vec<_> = (0..count)
                .map(|id| json!({"jsonrpc":"2.0","id":id,"method":"show"}))
                .collect();
            let message = if count == 1 {
                requests[0].clone()
            } else {
                Value::Array(requests)
            };
            let mut input = vec![];
            protocol::write_frame(&mut input, &message).unwrap();
            let mut output = vec![];
            run(dispatch, Cursor::new(input), &mut output).unwrap();
            let mut reader = Cursor::new(output);
            let mut replies = vec![];
            while let Some(message) = protocol::read_response_frame(&mut reader).unwrap() {
                if let Value::Array(batch) = message {
                    replies.extend(batch);
                } else if message.get("id").is_some() {
                    replies.push(message);
                }
            }
            assert_eq!(replies.len(), count);
            let mut limited = 0;
            for reply in replies {
                if reply.get("error").is_some() {
                    limited += 1;
                    assert_eq!(reply["error"]["data"]["code"], "resource-limit");
                } else {
                    assert_eq!(reply["result"].as_str().map(str::len), Some(size));
                }
            }
            assert_eq!(limited > 0, overflow, "count={count} size={size}");
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
        assert_eq!(message["params"]["sequence"], 1);
        drop(input);
        assert!(server.join().unwrap().is_ok());
    }
    #[test]
    fn in_flight_work_and_batches_are_bounded() {
        let (input, chunks) = mpsc::channel();
        let (frames, output) = mpsc::channel();
        let (started, ready) = mpsc::channel();
        let dispatch = Arc::new(move |_: Method, _: &Value, cancel: &Cancellation| {
            started.send(()).unwrap();
            let began = Instant::now();
            while !cancel.is_cancelled() {
                assert!(began.elapsed() < Duration::from_secs(5));
                thread::yield_now();
            }
            Err(EngineError {
                code: ErrorCode::Cancelled,
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
        for id in 0..MAX_WORKERS {
            send(&input, json!({"jsonrpc":"2.0","id":id,"method":"search"}));
            ready.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        send(&input, json!({"jsonrpc":"2.0","id":"busy","method":"libraries"}));
        assert_eq!(reply(&output)["error"]["message"], "Server busy");
        send(
            &input,
            Value::Array(vec![
                json!({"jsonrpc":"2.0","id":0,"method":"libraries"});
                MAX_BATCH + 1
            ]),
        );
        assert_eq!(reply(&output)["error"]["code"], -32600);
        for id in 0..MAX_WORKERS {
            send(
                &input,
                json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":id}}),
            );
        }
        drop(input);
        for _ in 0..MAX_WORKERS {
            assert_eq!(reply(&output)["error"]["data"]["code"], "cancelled");
        }
        assert!(server.join().unwrap().is_ok());
    }
    #[test]
    fn worker_panics_are_fixed_errors_and_other_batch_members_complete() {
        const CHILD: &str = "IBIS_RPC_PANIC_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "server::tests::worker_panics_are_fixed_errors_and_other_batch_members_complete",
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
        protocol::write_frame(
            &mut input,
            &json!([
                {"jsonrpc":"2.0","id":1,"method":"search","params":{"panic":true}},
                {"jsonrpc":"2.0","id":2,"method":"libraries"}
            ]),
        )
        .unwrap();
        let mut output = vec![];
        run(dispatch, Cursor::new(input), &mut output).unwrap();
        let mut reader = Cursor::new(output);
        let mut batch = None;
        while let Some(message) = protocol::read_frame(&mut reader).unwrap() {
            if message.is_array() {
                batch = Some(message);
            }
        }
        let batch = batch.unwrap();
        assert_eq!(batch[0]["error"]["data"]["code"], "internal");
        assert_eq!(batch[1]["result"], json!([]));
        assert!(!batch.to_string().contains("synthetic worker failure"));
    }
}
