//! One bounded request to a source adapter or its native helper.

#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "waitid observes exit without reaping the process group leader"
)]
mod unix;
#[cfg(windows)]
#[allow(unsafe_code, reason = "Win32 Job Objects contain descendants from launch")]
pub(crate) mod windows;

use std::io::{self, Read, Write};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(5);

/// Fixed reasons, never the child's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChildError {
    Unavailable,
    TimedOut,
    Failed,
    TooLarge,
}

pub(crate) struct Limits {
    pub time: Duration,
    pub stdout: u64,
    /// None discards diagnostics.
    pub stderr: Option<u64>,
}

/// Run an adapter as the leader of a process tree.
pub(crate) fn run(
    command: Command,
    request: &[u8],
    limits: Limits,
    cancelled: &dyn Fn() -> bool,
) -> Result<Output, ChildError> {
    exchange(command, request, limits, cancelled, true)
}

/// EndNote's fixed, leaf helper joins its adapter's process group so the
/// engine can kill both even when it cancels during an Apple Event. When
/// invoked outside that boundary, the helper gets its own group.
#[cfg(all(target_os = "macos", feature = "endnote"))]
pub(crate) fn run_helper(command: Command, request: &[u8], limits: Limits) -> Result<Output, ChildError> {
    let in_adapter_group = nix::unistd::getpgrp() == nix::unistd::getpid();
    exchange(command, request, limits, &|| false, !in_adapter_group)
}

fn exchange(
    mut command: Command,
    request: &[u8],
    limits: Limits,
    cancelled: &dyn Fn() -> bool,
    own_group: bool,
) -> Result<Output, ChildError> {
    let deadline = Instant::now() + limits.time;
    if cancelled() {
        return Err(ChildError::TimedOut);
    }
    command.env_clear();
    #[cfg(windows)]
    {
        if let Some(root) = std::env::var_os("SystemRoot") {
            command.env("SystemRoot", root);
        }
        windows::suspended(&mut command);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        if own_group {
            command.process_group(0);
        }
        crate::pdf::inheritance::restrict(&mut command).map_err(|_| ChildError::Unavailable)?;
    }
    let child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(if limits.stderr.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .spawn()
        .map_err(|_| ChildError::Unavailable)?;
    let mut tree = Tree::new(child, own_group)?;
    let stop = AtomicBool::new(false);
    let stopped = &stop;
    thread::scope(|scope| {
        let result = (|| {
            let input = tree.child.stdin.take().ok_or(ChildError::Unavailable)?;
            let output = tree.child.stdout.take().ok_or(ChildError::Unavailable)?;
            let diagnostics = tree.child.stderr.take();
            let (send, events) = mpsc::channel();
            worker(scope, &send, "stdin", || {
                let result = pipe(input, &stop).and_then(|mut pipe| pipe.write_all(request));
                Event::Input(result.map_err(|_| ChildError::Failed))
            })?;
            worker(scope, &send, "stdout", || {
                Event::Output(read_pipe(output, limits.stdout, &stop))
            })?;
            if let Some((pipe, limit)) = diagnostics.zip(limits.stderr) {
                worker(scope, &send, "stderr", move || {
                    Event::Diagnostics(read_pipe(pipe, limit, stopped))
                })?;
            }
            drop(send);

            let mut stdin_done = false;
            let mut stdout = None;
            let mut stderr = limits.stderr.is_none().then(Vec::new);
            let mut status = None;
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if cancelled() || remaining.is_zero() {
                    return Err(ChildError::TimedOut);
                }
                if status.is_none() && tree.exited()? {
                    status = Some(tree.finish(false)?);
                }
                if stdin_done {
                    if let (Some(status), Some(stdout), Some(stderr)) = (status, stdout.as_mut(), stderr.as_mut()) {
                        return Ok(Output {
                            status,
                            stdout: std::mem::take(stdout),
                            stderr: std::mem::take(stderr),
                        });
                    }
                }
                match events.recv_timeout(remaining.min(POLL)) {
                    Ok(Event::Input(result)) => {
                        result?;
                        stdin_done = true;
                    }
                    Ok(Event::Output(result)) => stdout = Some(result?),
                    Ok(Event::Diagnostics(result)) => stderr = Some(result?),
                    Err(RecvTimeoutError::Timeout) => {}
                    // All of the workers have returned. Only the leader is outstanding.
                    Err(RecvTimeoutError::Disconnected) if stdin_done && stdout.is_some() && stderr.is_some() => {
                        thread::sleep(remaining.min(POLL));
                    }
                    Err(RecvTimeoutError::Disconnected) => return Err(ChildError::Failed),
                }
            }
        })();
        stop.store(true, Ordering::Relaxed);
        if result.is_err() && !tree.reaped {
            let _ = tree.finish(true);
        }
        // The group or job has been stopped and stdin/stdout/stderr workers
        // return before the scope exits, including on timeout and overflow.
        result
    })
}

fn worker<'scope>(
    scope: &'scope thread::Scope<'scope, '_>,
    send: &mpsc::Sender<Event>,
    name: &str,
    task: impl FnOnce() -> Event + Send + 'scope,
) -> Result<(), ChildError> {
    let send = send.clone();
    thread::Builder::new()
        .name(format!("ibis-child-{name}"))
        .spawn_scoped(scope, move || {
            let _ = send.send(task());
        })
        .map(|_| ())
        .map_err(|_| ChildError::Unavailable)
}

enum Event {
    Input(Result<(), ChildError>),
    Output(Result<Vec<u8>, ChildError>),
    Diagnostics(Result<Vec<u8>, ChildError>),
}

fn read_pipe(stream: impl PipeStream, limit: u64, stop: &AtomicBool) -> Result<Vec<u8>, ChildError> {
    let stream = pipe(stream, stop).map_err(|_| ChildError::Failed)?;
    read_bounded(stream, limit)
}

/// Read at most one byte beyond the allowance, returning immediately at the limit
/// violation even if the producer is still writing.
pub(crate) fn read_bounded(stream: impl Read, limit: u64) -> Result<Vec<u8>, ChildError> {
    let mut bytes = Vec::new();
    stream
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ChildError::Failed)?;
    if bytes.len() as u64 > limit {
        return Err(ChildError::TooLarge);
    }
    Ok(bytes)
}

#[cfg(unix)]
trait PipeStream: Read + std::os::fd::AsFd {}
#[cfg(unix)]
impl PipeStream for std::process::ChildStdout {}
#[cfg(unix)]
impl PipeStream for std::process::ChildStderr {}
#[cfg(windows)]
trait PipeStream: Read {}
#[cfg(windows)]
impl PipeStream for std::process::ChildStdout {}
#[cfg(windows)]
impl PipeStream for std::process::ChildStderr {}

#[cfg(windows)]
fn pipe<T>(stream: T, _stop: &AtomicBool) -> io::Result<T> {
    Ok(stream)
}

// Even if a Unix program hands off a pipe or leaves the group, a worker can
// stop without waiting on that file descriptor. The child ends still block.
#[cfg(unix)]
struct Pipe<'a, T> {
    stream: T,
    stop: &'a AtomicBool,
}

#[cfg(unix)]
fn pipe<T: std::os::fd::AsFd>(stream: T, stop: &AtomicBool) -> io::Result<Pipe<'_, T>> {
    use nix::fcntl::{FcntlArg, OFlag, fcntl};
    let flags = OFlag::from_bits_retain(fcntl(&stream, FcntlArg::F_GETFL)?);
    fcntl(&stream, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK))?;
    Ok(Pipe { stream, stop })
}

#[cfg(unix)]
impl<T> Pipe<'_, T> {
    fn io(&mut self, mut operation: impl FnMut(&mut T) -> io::Result<usize>) -> io::Result<usize> {
        while !self.stop.load(Ordering::Relaxed) {
            match operation(&mut self.stream) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => thread::sleep(POLL),
                result => return result,
            }
        }
        Err(io::ErrorKind::TimedOut.into())
    }
}

#[cfg(unix)]
impl<T: Read> Read for Pipe<'_, T> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.io(|stream| stream.read(buffer))
    }
}

#[cfg(unix)]
impl<T: Write> Write for Pipe<'_, T> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.io(|stream| stream.write(buffer))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

struct Tree {
    child: Child,
    reaped: bool,
    #[cfg(unix)]
    own_group: bool,
    #[cfg(windows)]
    job: Option<windows::Job>,
}

impl Tree {
    fn new(child: Child, own_group: bool) -> Result<Self, ChildError> {
        #[cfg(windows)]
        let _ = own_group;
        let tree = Self {
            child,
            reaped: false,
            #[cfg(unix)]
            own_group,
            #[cfg(windows)]
            job: None,
        };
        #[cfg(windows)]
        let tree = {
            let mut tree = tree;
            tree.job = Some(windows::Job::start(&tree.child, None).map_err(|_| ChildError::Unavailable)?);
            tree
        };
        Ok(tree)
    }

    fn exited(&mut self) -> Result<bool, ChildError> {
        #[cfg(unix)]
        let exited = unix::exited(&self.child);
        #[cfg(windows)]
        let exited = self.child.try_wait().map(|status| status.is_some());
        match exited {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => Ok(false),
            result => result.map_err(|_| ChildError::Failed),
        }
    }

    fn finish(&mut self, abort: bool) -> Result<std::process::ExitStatus, ChildError> {
        #[cfg(unix)]
        if self.own_group {
            use nix::sys::signal::{Signal, killpg};
            use nix::unistd::Pid;
            if let Ok(id) = i32::try_from(self.child.id()) {
                let _ = killpg(Pid::from_raw(id), Signal::SIGKILL);
            }
        }
        #[cfg(windows)]
        drop(self.job.take());
        if abort {
            let _ = self.child.kill();
        }
        let status = self.child.wait().map_err(|_| ChildError::Failed)?;
        self.reaped = true;
        Ok(status)
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.finish(true);
        }
    }
}
