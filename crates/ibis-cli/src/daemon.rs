//! Same-user connections to the warm engine. The handshake precedes all RPC.
use ibis_core::contract::Fingerprint;
use ibis_engine::{Cancellation, protocol};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{self, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant, UNIX_EPOCH};

pub(crate) mod mcp;
pub(crate) mod picker;

#[cfg(unix)]
#[path = "daemon/unix.rs"]
mod native;
#[cfg(windows)]
#[path = "daemon/windows.rs"]
mod native;

use native::{Address, Stream};

const PROTOCOL: u32 = 1;
const START_TIMEOUT: Duration = Duration::from_secs(8);
const HELLO_TIMEOUT: Duration = Duration::from_secs(2);
pub(super) const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const IDLE: Duration = Duration::from_secs(60);
const ACCEPT_WAIT: Duration = Duration::from_secs(2);
const ACCEPT_RETRY: Duration = Duration::from_millis(100);
const MAX_CONNECTIONS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Build {
    protocol: u32,
    version: String,
    modified_ns: u64,
    bytes: u64,
}

impl Build {
    fn here() -> &'static Self {
        static BUILD: OnceLock<Build> = OnceLock::new();
        BUILD.get_or_init(|| Self::of(std::env::current_exe().ok().as_deref()))
    }

    fn of(program: Option<&Path>) -> Self {
        let file = program.and_then(|program| std::fs::metadata(program).ok());
        let modified_ns = file
            .as_ref()
            .and_then(|file| file.modified().ok())
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .and_then(|duration| u64::try_from(duration.as_nanos()).ok())
            .unwrap_or_default();
        Build {
            protocol: PROTOCOL,
            version: ibis_core::VERSION.to_owned(),
            modified_ns,
            bytes: file.map_or(0, |file| file.len()),
        }
    }

    fn newer_than(&self, other: &Self) -> bool {
        fn version(version: &str) -> (Vec<u64>, bool, &str) {
            let version = version.split('+').next().unwrap_or(version);
            let (number, pre) = version.split_once('-').unwrap_or((version, ""));
            let number = number
                .split('.')
                .map(str::parse)
                .collect::<Result<Vec<u64>, _>>()
                .unwrap_or_default();
            (number, pre.is_empty(), pre)
        }
        (self.protocol, version(&self.version), self.modified_ns, self.bytes)
            > (other.protocol, version(&other.version), other.modified_ns, other.bytes)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Mode {
    Rpc,
    Cli,
    #[cfg(target_os = "macos")]
    Picker,
}

#[derive(Serialize, Deserialize)]
struct Hello {
    build: Build,
    mode: Mode,
}

/// Identifies the running engine and counts file parses for diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub pid: u32,
    pub library_loads: u64,
    pub cli_connections: u64,
    pub mcp_sessions: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Welcome {
    Ready(Status),
    DifferentBuild(Build),
    Draining,
    Busy,
}

/// An isolated configuration selects its own instance within this user's runtime.
#[derive(Clone)]
pub struct Endpoint(Address);

impl Endpoint {
    pub fn new(config: &Path, runtime_root: Option<&Path>) -> io::Result<Self> {
        let digest = Fingerprint::of_bytes(config.as_os_str().as_encoded_bytes());
        Address::new(&digest.as_str()[..16], runtime_root).map(Self)
    }

    pub fn current() -> io::Result<Self> {
        let config = ibis_engine::identity::config_dir().ok_or(io::ErrorKind::NotFound)?;
        let root = if cfg!(target_os = "linux") {
            std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from)
        } else if cfg!(windows) {
            std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
        } else {
            None
        };
        Self::new(&std::path::absolute(config)?, root.as_deref())
    }

    /// Connect to an existing daemon's public method-table transport.
    pub fn connect(&self, program: &Path) -> io::Result<Connection> {
        self.0.prepare()?;
        self.open(Mode::Rpc, &Build::of(Some(program)))
    }

    pub(super) fn client() -> io::Result<Connection> {
        Self::current()?.connect_or_start(Mode::Cli)
    }

    fn open(&self, mode: Mode, build: &Build) -> io::Result<Connection> {
        Self::handshake(self.0.connect()?, mode, build, HELLO_TIMEOUT)
    }

    fn handshake(stream: Stream, mode: Mode, build: &Build, timeout: Duration) -> io::Result<Connection> {
        let mut framed = Framed::hello_with_timeout(stream, timeout)?;
        framed.send(&Hello {
            build: build.clone(),
            mode,
        })?;
        match framed.receive()? {
            Welcome::Ready(status) => {
                framed.session()?;
                Ok(Connection { framed, status })
            }
            Welcome::DifferentBuild(server) if build.newer_than(&server) => {
                framed.send(&"drain")?;
                if !matches!(framed.receive()?, Welcome::Draining) {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                Err(io::ErrorKind::WouldBlock.into())
            }
            Welcome::DifferentBuild(_) => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "a newer Ibis engine is running",
            )),
            Welcome::Draining => Err(io::ErrorKind::WouldBlock.into()),
            Welcome::Busy => Err(io::ErrorKind::ResourceBusy.into()),
        }
    }

    fn connect_or_start(&self, mode: Mode) -> io::Result<Connection> {
        self.0.prepare()?;
        let build = Build::here();
        let deadline = Instant::now() + START_TIMEOUT;
        let mut pause = Duration::from_millis(10);
        let mut launch: Option<native::Process> = None;
        let mut starter = None;
        let mut launches = 0;
        let result = (|| {
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(io::ErrorKind::TimedOut.into());
                }
                match self.0.connect() {
                    Ok(stream) => match Self::handshake(stream, mode, build, remaining.min(HELLO_TIMEOUT)) {
                        Ok(connection) => return Ok(connection),
                        Err(error) if unavailable(&error) => {}
                        Err(error) => return Err(error),
                    },
                    Err(error)
                        if matches!(error.kind(), io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused) =>
                    {
                        if starter.is_none() {
                            starter = self.0.lock("start")?;
                            if starter.is_some() {
                                // Another starter may have just published its listener.
                                continue;
                            }
                        } else {
                            if let Some(child) = launch.as_mut()
                                && child.has_exited()?
                            {
                                launch = None;
                            }
                            if launch.is_none() {
                                // Only the daemon takes the lifetime lease. It may
                                // still be held briefly as a predecessor exits.
                                if launches == 2 {
                                    return Err(io::ErrorKind::ConnectionRefused.into());
                                }
                                launch = Some(native::Process::spawn(&std::env::current_exe()?)?);
                                launches += 1;
                            }
                        }
                    }
                    Err(error) if unavailable(&error) => {}
                    Err(error) => return Err(error),
                }
                std::thread::sleep(pause.min(deadline.saturating_duration_since(Instant::now())));
                pause = (pause * 2).min(Duration::from_millis(160));
            }
        })();
        if let Some(child) = launch {
            child.reap();
        }
        result
    }
}

fn unavailable(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound
            | io::ErrorKind::ConnectionRefused
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::UnexpectedEof
            | io::ErrorKind::WouldBlock
            | io::ErrorKind::TimedOut
    )
}

/// A handshaken daemon connection with the same framing as `ibis serve`.
pub struct Connection {
    framed: Framed,
    status: Status,
}

impl Connection {
    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn send(&mut self, message: &Value) -> io::Result<()> {
        self.framed.send(message)
    }

    pub fn receive(&mut self) -> io::Result<Value> {
        self.framed.receive()
    }

    pub(super) fn exchange<T: DeserializeOwned>(&mut self, request: impl Serialize) -> io::Result<T> {
        #[cfg(unix)]
        let _operation = self.framed.writer.settle_on_interrupt()?;
        self.framed.send(&request)?;
        self.framed.receive()
    }
}

struct Framed {
    reader: BufReader<Stream>,
    writer: Stream,
}

impl Framed {
    fn hello(stream: Stream) -> io::Result<Self> {
        Self::hello_with_timeout(stream, HELLO_TIMEOUT)
    }

    fn hello_with_timeout(mut stream: Stream, timeout: Duration) -> io::Result<Self> {
        stream.set_timeouts(Some(timeout), timeout)?;
        Ok(Self {
            reader: BufReader::new(stream.try_clone()?),
            writer: stream,
        })
    }

    fn session(&mut self) -> io::Result<()> {
        self.reader.get_mut().set_timeouts(None, WRITE_TIMEOUT)?;
        self.writer.set_timeouts(None, WRITE_TIMEOUT)
    }

    fn send(&mut self, message: &impl Serialize) -> io::Result<()> {
        send_frame(&mut self.writer, message)
    }

    fn receive<T: DeserializeOwned>(&mut self) -> io::Result<T> {
        decode(protocol::read_response_frame(&mut self.reader))?.ok_or_else(|| io::ErrorKind::UnexpectedEof.into())
    }

    fn request<T: DeserializeOwned>(&mut self) -> io::Result<Option<T>> {
        decode(protocol::read_frame(&mut self.reader))
    }
}

fn send_frame(writer: &mut Stream, message: &impl Serialize) -> io::Result<()> {
    let message = serde_json::to_value(message).map_err(io::Error::other)?;
    protocol::write_frame(writer, &message).map_err(frame_error)
}

fn decode<T: DeserializeOwned>(frame: Result<Option<Value>, protocol::FrameError>) -> io::Result<Option<T>> {
    frame
        .map_err(frame_error)?
        .map(|value| serde_json::from_value(value).map_err(io::Error::other))
        .transpose()
}

fn frame_error(error: protocol::FrameError) -> io::Error {
    let kind = match error {
        protocol::FrameError::UnexpectedEof => io::ErrorKind::UnexpectedEof,
        protocol::FrameError::Io => io::ErrorKind::ConnectionReset,
        _ => io::ErrorKind::InvalidData,
    };
    io::Error::new(kind, error)
}

#[derive(Default)]
struct Connections {
    draining: AtomicBool,
    cli_connections: AtomicU64,
    mcp_sessions: AtomicU64,
    active: Mutex<Active>,
    closed: Condvar,
}

struct Active {
    streams: BTreeMap<u64, ActiveStream>,
    idle_since: Instant,
}

struct ActiveStream {
    stream: Stream,
    /// A granted native clipboard write must end before closing its reader.
    delivery: Arc<Mutex<()>>,
}

impl Default for Active {
    fn default() -> Self {
        Self {
            streams: BTreeMap::new(),
            idle_since: Instant::now(),
        }
    }
}

impl Connections {
    fn active(&self) -> std::sync::MutexGuard<'_, Active> {
        self.active.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
    fn drain(&self) {
        self.draining.store(true, Ordering::Release);
        let active = self.active();
        for link in active.streams.values() {
            if let Ok(_turn) = link.delivery.try_lock() {
                link.stream.stop_reading();
            }
        }
    }
}

struct Connected(Arc<Connections>, u64);
impl Drop for Connected {
    fn drop(&mut self) {
        let mut active = self.0.active();
        active.streams.remove(&self.1);
        if active.streams.is_empty() {
            active.idle_since = Instant::now();
        }
        self.0.closed.notify_all();
    }
}

pub(super) fn run() -> io::Result<()> {
    let _ = Build::here();
    let endpoint = Endpoint::current()?;
    endpoint.0.prepare()?;
    std::env::set_current_dir(endpoint.0.runtime_dir())?;
    let _lifetime = endpoint.0.lock("engine")?.ok_or(io::ErrorKind::AlreadyExists)?;
    #[cfg(target_os = "macos")]
    let delivery_fence = {
        let deadline = Instant::now() + START_TIMEOUT;
        loop {
            if let Some(fence) = endpoint.0.lock("delivery")? {
                break fence;
            }
            if Instant::now() >= deadline {
                return Err(io::ErrorKind::TimedOut.into());
            }
            std::thread::sleep(ACCEPT_RETRY);
        }
    };
    let mut listener = endpoint.0.listen()?;
    #[cfg(target_os = "macos")]
    drop(delivery_fence);
    ibis_engine::identity::use_saved_key(true).map_err(io::Error::other)?;
    #[cfg(unix)]
    ibis_sources::adapter::guard_daemon_children(std::env::current_exe()?);
    let host = Arc::new(crate::headless::Host::new().map_err(|_| io::Error::other("engine unavailable"))?);
    let clients = Arc::new(Connections::default());
    let mut serial = 0;
    while !clients.draining.load(Ordering::Acquire) {
        let timeout = {
            let active = clients.active();
            if active.streams.is_empty() {
                ACCEPT_WAIT.min(IDLE.saturating_sub(active.idle_since.elapsed()))
            } else {
                ACCEPT_WAIT
            }
        };
        if timeout.is_zero() {
            break;
        }
        let stream = match listener.accept(timeout) {
            Ok(Some(stream)) => stream,
            Ok(None) => continue,
            Err(error) => {
                accept_failure("accept", &error);
                continue;
            }
        };
        let mut active = clients.active();
        if active.streams.len() >= MAX_CONNECTIONS {
            drop(active);
            let _ = reject_busy(stream);
            continue;
        }
        let control = match stream.try_clone() {
            Ok(control) => control,
            Err(error) => {
                drop(active);
                accept_failure("clone connection", &error);
                continue;
            }
        };
        serial += 1;
        let delivery = Arc::new(Mutex::new(()));
        active.streams.insert(
            serial,
            ActiveStream {
                stream: control,
                delivery: delivery.clone(),
            },
        );
        drop(active);
        let established = Connected(clients.clone(), serial);
        let host = host.clone();
        if let Err(error) = std::thread::Builder::new()
            .name("ibis-connection".into())
            .spawn(move || {
                let established = established;
                let _ = serve(stream, &host, &established.0, &delivery);
            })
        {
            accept_failure("spawn connection handler", &error);
        }
    }
    clients.drain();
    drop(
        clients
            .closed
            .wait_while(clients.active(), |active| !active.streams.is_empty()),
    );
    Ok(())
}

fn accept_failure(operation: &str, error: &io::Error) {
    let _ = writeln!(io::stderr(), "ibis daemon: {operation}: {error}");
    std::thread::sleep(ACCEPT_RETRY);
}

fn reject_busy(stream: Stream) -> io::Result<()> {
    let mut framed = Framed::hello(stream)?;
    if framed.request::<Hello>()?.is_some() {
        framed.send(&Welcome::Busy)?;
        // On Windows closing the server end can discard unread pipe bytes.
        let _ = framed.request::<Value>();
    }
    Ok(())
}

fn serve(
    stream: Stream,
    host: &crate::headless::Host,
    connections: &Connections,
    _delivery: &Mutex<()>,
) -> io::Result<()> {
    let mut framed = Framed::hello(stream)?;
    let Some(hello): Option<Hello> = framed.request()? else {
        return Ok(());
    };
    let local = Build::here();
    if connections.draining.load(Ordering::Acquire) {
        return framed.send(&Welcome::Draining);
    }
    if hello.build != *local {
        framed.send(&Welcome::DifferentBuild(local.clone()))?;
        // Keep the pipe alive while either client reads the mismatch reply.
        let next = framed.request::<String>()?;
        if hello.build.newer_than(local) && next.as_deref() == Some("drain") {
            framed.send(&Welcome::Draining)?;
            connections.drain();
        }
        return Ok(());
    }
    framed.send(&Welcome::Ready(Status {
        pid: std::process::id(),
        library_loads: host.library_loads(),
        cli_connections: connections.cli_connections.load(Ordering::Relaxed),
        mcp_sessions: connections.mcp_sessions.load(Ordering::Relaxed),
    }))?;
    framed.session()?;
    match hello.mode {
        Mode::Rpc => crate::server::serve(host.rpc_engine(), framed.reader, framed.writer).map_err(frame_error),
        Mode::Cli => {
            connections.cli_connections.fetch_add(1, Ordering::Relaxed);
            serve_cli(framed, host, connections)
        }
        #[cfg(target_os = "macos")]
        Mode::Picker => {
            let endpoint = Endpoint::current()?;
            let fence = picker::Fence {
                turn: _delivery,
                endpoint: &endpoint,
            };
            picker::serve(&mut framed, host, &connections.draining, Some(fence))
        }
    }
}

/// Start or join the daemon using the normal locks and skew policy. The app
/// opens its own authenticated socket with the same build identity.
#[cfg(target_os = "macos")]
pub fn picker_endpoint() -> io::Result<()> {
    let endpoint = Endpoint::current()?;
    let _connection = endpoint.connect_or_start(Mode::Rpc)?;
    let socket = endpoint.0.socket_path();
    let result = serde_json::json!({
        "socket": socket,
        "delivery_lock": socket.with_extension("delivery"),
        "hello": Hello { build: Build::here().clone(), mode: Mode::Picker },
    });
    serde_json::to_writer(io::stdout().lock(), &result).map_err(io::Error::other)
}

/// A picker engine of the shell's own over its standard streams, for when the
/// daemon cannot be reached: the same turns and files, without the socket.
pub fn picker_stdio() -> io::Result<()> {
    ibis_engine::identity::use_saved_key(true).map_err(io::Error::other)?;
    let host = crate::headless::Host::new().map_err(|_| io::Error::other("engine unavailable"))?;
    picker::serve(&mut picker::Pipes::standard(), &host, &AtomicBool::new(false), None)
}

fn serve_cli(framed: Framed, host: &crate::headless::Host, connections: &Connections) -> io::Result<()> {
    use crate::headless::Intent;
    let mut engine = host.cli_engine().map_err(|_| io::Error::other("engine unavailable"))?;
    let Framed { mut reader, mut writer } = framed;
    let cancel = Cancellation::default();
    let switch = std::thread::scope(|scope| {
        let (requests, work) = mpsc::sync_channel(1);
        let control = writer.try_clone()?;
        let (engine, writer, cancelled) = (&mut engine, &mut writer, &cancel);
        let working = std::thread::Builder::new()
            .name("ibis-cli-request".into())
            .spawn_scoped(scope, move || {
                for request in work {
                    let reply = host.answer(engine, request, cancelled);
                    if let Err(error) = send_frame(writer, &reply) {
                        control.stop_reading();
                        return Err(error);
                    }
                }
                Ok(())
            })?;
        let input = loop {
            match decode(protocol::read_response_frame(&mut reader)) {
                Ok(Some(Intent::StartMcp {
                    allow_writes,
                    library_problem,
                })) => break Ok(Some((allow_writes, library_problem))),
                Ok(Some(request)) => {
                    if requests.try_send(request).is_err() {
                        break Err(io::ErrorKind::InvalidData.into());
                    }
                }
                Ok(None) => break Ok(None),
                Err(error) => break Err(error),
            }
        };
        if !matches!(input, Ok(Some(_))) && !connections.draining.load(Ordering::Acquire) {
            cancel.cancel();
        }
        drop(requests);
        working.join().map_err(|_| io::ErrorKind::BrokenPipe)??;
        input
    })?;
    if let Some((allow_writes, library_problem)) = switch {
        connections.mcp_sessions.fetch_add(1, Ordering::Relaxed);
        mcp::serve(
            engine,
            Framed { reader, writer },
            allow_writes,
            library_problem,
            &connections.draining,
        )?;
    }
    Ok(())
}
