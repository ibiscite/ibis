//! Relay an MCP session to the daemon. The existing MCP runtime owns parsing,
//! cancellation and tool execution. Ordered packets carry bytes and a half-close
//! on both Unix streams and Windows pipes.
use super::{Framed, HELLO_TIMEOUT, Stream, WRITE_TIMEOUT};
use crate::headless::{Intent, Remote};
use ibis_core::contract::{EngineError, ErrorCode};
use ibis_engine::{Engine, mcp, protocol, rpc};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, Scope, ScopedJoinHandle};

const CHUNK: usize = 8192;

struct Input<'a, R> {
    stream: R,
    remaining: usize,
    ended: bool,
    draining: Option<&'a AtomicBool>,
}

impl<R> Input<'_, R> {
    fn new(stream: R) -> Self {
        Self {
            stream,
            remaining: 0,
            ended: false,
            draining: None,
        }
    }
}

impl<R: Read> Read for Input<'_, R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if self.ended || bytes.is_empty() {
            return Ok(0);
        }
        if self.remaining == 0 {
            let mut header = [0; 4];
            if self.stream.read(&mut header[..1])? == 0 {
                return if self.draining.is_some_and(|draining| draining.load(Ordering::Acquire)) {
                    Ok(0)
                } else {
                    Err(io::ErrorKind::UnexpectedEof.into())
                };
            }
            self.stream.read_exact(&mut header[1..])?;
            self.remaining = u32::from_be_bytes(header) as usize;
            if self.remaining > CHUNK {
                return Err(io::ErrorKind::InvalidData.into());
            }
            if self.remaining == 0 {
                self.ended = true;
                return Ok(0);
            }
        }
        let count = bytes.len().min(self.remaining);
        self.stream.read_exact(&mut bytes[..count])?;
        self.remaining -= count;
        Ok(count)
    }
}

struct Output(Stream);

impl Output {
    fn end(&mut self) -> io::Result<()> {
        self.0.write_all(&0u32.to_be_bytes())
    }
}

impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        for chunk in bytes.chunks(CHUNK) {
            self.0.write_all(&(chunk.len() as u32).to_be_bytes())?;
            self.0.write_all(chunk)?;
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

pub(super) fn serve(
    engine: Engine,
    mut framed: Framed,
    allow_writes: bool,
    library_problem: Option<EngineError>,
    draining: &AtomicBool,
) -> io::Result<()> {
    let library_problem = library_problem.map(mcp::Refusal::startup);
    let (session, missing_writes) = crate::mcp_session(allow_writes, library_problem);
    framed.send(&Ok::<_, EngineError>(missing_writes))?;
    let mut input = Input::new(framed.reader);
    input.draining = Some(draining);
    let mut reader = BufReader::new(input);
    let mut writer = Output(framed.writer);
    mcp::serve(&engine, session, &mut reader, &mut writer)?;
    if reader.get_ref().ended {
        writer.end()?;
        // Keep the pipe open until the relay has consumed the final reply.
        let input = &mut reader.get_mut().stream;
        input.get_mut().set_timeouts(Some(HELLO_TIMEOUT), WRITE_TIMEOUT)?;
        let mut acknowledged = [0];
        input.read_exact(&mut acknowledged)?;
    }
    Ok(())
}

type Pending = Mutex<Option<BTreeMap<String, rpc::RequestId>>>;

fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct Relay<'scope> {
    pending: &'scope Pending,
    to_daemon: Output,
    replies: ScopedJoinHandle<'scope, io::Result<()>>,
}

impl<'scope> Relay<'scope> {
    fn start<W: Write + Send>(
        scope: &'scope Scope<'scope, '_>,
        remote: &mut Remote,
        output: &'scope Mutex<W>,
        pending: &'scope Pending,
        allow_writes: bool,
        library_problem: Option<EngineError>,
        initialize: Option<&[u8]>,
    ) -> io::Result<(Self, bool)> {
        let connection = remote.connect().map_err(|_| io::ErrorKind::NotConnected)?;
        let missing_writes = connection
            .exchange::<Result<bool, EngineError>>(Intent::StartMcp {
                allow_writes,
                library_problem,
            })?
            .map_err(|_| io::ErrorKind::NotConnected)?;
        let connection = remote.connection.take().ok_or(io::ErrorKind::NotConnected)?;
        let mut from_daemon = BufReader::new(Input::new(connection.framed.reader));
        let mut to_daemon = Output(connection.framed.writer);
        if let Some(initialize) = initialize {
            from_daemon
                .get_mut()
                .stream
                .get_mut()
                .set_timeouts(Some(HELLO_TIMEOUT), WRITE_TIMEOUT)?;
            to_daemon.write_all(initialize)?;
            if read_line(&mut from_daemon, &mut Vec::new(), mcp::MAX_ANSWER)? == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            from_daemon
                .get_mut()
                .stream
                .get_mut()
                .set_timeouts(None, WRITE_TIMEOUT)?;
        }
        let mut ack = to_daemon.0.try_clone()?;
        *locked(pending) = Some(BTreeMap::new());
        let replies = thread::Builder::new()
            .name("ibis-mcp-replies".into())
            .spawn_scoped(scope, move || {
                let result = (|| {
                    let mut line = Vec::new();
                    while read_line(&mut from_daemon, &mut line, mcp::MAX_ANSWER)? != 0 {
                        let value: Value = serde_json::from_slice(&line).map_err(io::Error::other)?;
                        if let Some(id) = value.get("id")
                            && let Some(pending) = locked(pending).as_mut()
                        {
                            pending.remove(&id.to_string());
                        }
                        let mut output = locked(output);
                        output.write_all(&line)?;
                        output.flush()?;
                    }
                    if from_daemon.get_ref().ended {
                        ack.write_all(&[1])
                    } else {
                        Err(io::ErrorKind::UnexpectedEof.into())
                    }
                })();
                let unfinished = locked(pending).take().unwrap_or_default();
                if result.is_err() {
                    for id in unfinished.into_values() {
                        let error = rpc::failure(Some(&id), EngineError::new(ErrorCode::SourceUnavailable));
                        let mut output = locked(output);
                        serde_json::to_writer(&mut *output, &error).map_err(io::Error::other)?;
                        output.write_all(b"\n")?;
                        output.flush()?;
                    }
                }
                result
            })?;
        Ok((
            Self {
                pending,
                to_daemon,
                replies,
            },
            missing_writes,
        ))
    }

    /// False means the pipe closed before any part of this message was sent.
    fn send(&mut self, line: &[u8], message: Option<&Value>) -> io::Result<bool> {
        {
            let mut sharing = locked(self.pending);
            let Some(pending) = sharing.as_mut() else {
                return Ok(false);
            };
            if let Some(message) = message {
                if message["method"] == "notifications/cancelled"
                    && let Some(id) = message.pointer("/params/requestId")
                {
                    pending.remove(&id.to_string());
                } else if let Some(id) = message.get("id").and_then(rpc::RequestId::parse)
                    && pending.len() < 128
                {
                    pending.insert(id.value().to_string(), id);
                }
            }
        }
        self.to_daemon.write_all(line)?;
        Ok(true)
    }

    fn finish(mut self, clean: bool) -> io::Result<()> {
        let end = if clean { self.to_daemon.end() } else { Ok(()) };
        if !clean || end.is_err() {
            self.to_daemon.0.stop_reading();
        }
        let result = self.replies.join().map_err(|_| io::ErrorKind::BrokenPipe)?;
        end.and(result)
    }
}

fn read_line(reader: &mut impl BufRead, bytes: &mut Vec<u8>, limit: usize) -> io::Result<usize> {
    bytes.clear();
    reader.take(limit as u64 + 1).read_until(b'\n', bytes)
}

/// State to carry into the in-process server if a daemon cannot take over.
#[derive(Default)]
pub(crate) struct Resume {
    initialize: Option<Vec<u8>>,
    unsent: Vec<u8>,
}

impl Resume {
    pub(crate) fn serve(
        self,
        engine: &Engine,
        mut session: mcp::Session,
        reader: impl BufRead,
        writer: impl Write + Send,
    ) -> io::Result<()> {
        if let Some(initialize) = self.initialize
            && let Ok(message) = protocol::parse_json(initialize.trim_ascii())
        {
            // Restore the session state. The client already received its reply.
            let _ = mcp::handle(engine, &mut session, &message);
        }
        mcp::serve(engine, session, io::Cursor::new(self.unsent).chain(reader), writer)
    }
}

pub(crate) fn relay<R: BufRead, W: Write + Send>(
    remote: &mut Remote,
    mut reader: R,
    writer: W,
    allow_writes: bool,
    library_problem: Option<EngineError>,
    unavailable_writes: impl FnOnce() -> io::Result<()>,
) -> io::Result<Option<Resume>> {
    let output = Mutex::new(writer);
    let pending = Pending::default();
    thread::scope(|scope| {
        let Ok((connection, missing_writes)) =
            Relay::start(scope, remote, &output, &pending, allow_writes, library_problem, None)
        else {
            return Ok(Some(Resume::default()));
        };
        let mut connection = Some(connection);
        if missing_writes && let Err(error) = unavailable_writes() {
            if let Some(connection) = connection.take() {
                let _ = connection.finish(false);
            }
            return Err(error);
        }
        let mut initialize = None;
        let mut line = Vec::new();
        let mut oversized = false;
        loop {
            let read = match read_line(&mut reader, &mut line, protocol::MAX_REQUEST_BYTES) {
                Ok(read) => read,
                Err(error) => {
                    if let Some(connection) = connection.take() {
                        let _ = connection.finish(false);
                    }
                    return Err(error);
                }
            };
            if read == 0 {
                return connection.map_or(Ok(None), |connection| connection.finish(true).map(|()| None));
            }
            let message = (!oversized)
                .then(|| protocol::parse_json(line.trim_ascii()).ok())
                .flatten();
            oversized = !line.ends_with(b"\n") && (oversized || read > protocol::MAX_REQUEST_BYTES);
            // Retry only after a known disconnect before transmission. If a
            // write failed partway, the reply pump settles its outstanding id.
            for attempt in 0..2 {
                if connection.is_none() {
                    let Ok((resumed, _)) = Relay::start(
                        scope,
                        remote,
                        &output,
                        &pending,
                        allow_writes,
                        library_problem,
                        initialize.as_deref(),
                    ) else {
                        return Ok(Some(Resume {
                            initialize,
                            unsent: line,
                        }));
                    };
                    connection = Some(resumed);
                }
                let open = connection.as_mut().ok_or(io::ErrorKind::NotConnected)?;
                match open.send(&line, message.as_ref()) {
                    Ok(true) => {
                        if message
                            .as_ref()
                            .is_some_and(|message| message["method"] == "initialize" && message.get("id").is_some())
                        {
                            initialize = Some(line.clone());
                        }
                        break;
                    }
                    sent => {
                        if let Some(closed) = connection.take() {
                            let _ = closed.finish(false);
                        }
                        if sent.is_err() {
                            break;
                        }
                        if attempt == 1 {
                            return Ok(Some(Resume {
                                initialize,
                                unsent: line,
                            }));
                        }
                    }
                }
            }
        }
    })
}
