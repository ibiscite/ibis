//! Keep the group leader's PID reserved until the final group signal.

use std::io::{self, Read, Write};
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock, mpsc};

use nix::errno::Errno;
use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::poll::{PollFd, PollFlags, poll};

static TERMINATING: AtomicI32 = AtomicI32::new(0);
static WAKE: AtomicI32 = AtomicI32::new(-1);
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
static GUARD_PROGRAM: OnceLock<PathBuf> = OnceLock::new();
type Stop = Box<dyn FnOnce() + Send>;
static INTERRUPTED: Mutex<Option<Stop>> = Mutex::new(None);

pub const GUARD_ARGUMENT: &str = "__guard-parser";

/// Make the service independent of the terminal that invoked its first client.
pub fn detach(command: &mut Command) {
    // SAFETY: the post-fork callback only makes a session and returns its errno.
    unsafe {
        command.pre_exec(|| nix::unistd::setsid().map(|_| ()).map_err(io::Error::from));
    }
}

/// The daemon enables a parent-death guard for each supervised parser group.
pub fn guard_daemon_children(program: PathBuf) {
    let _ = GUARD_PROGRAM.set(program);
}

/// The guard leads the group and only waits for the daemon's pipe to close.
/// Exec closes other inherited descriptors before this can start.
pub fn run_guard() -> u8 {
    use nix::sys::signal::{Signal, killpg};
    use nix::unistd::{getpgrp, getpid};
    let group = getpgrp();
    if group != getpid() {
        return 2;
    }
    let _ = io::stdin().read(&mut [0]);
    let _ = killpg(group, Signal::SIGKILL);
    70
}

pub(super) struct Guard {
    child: Child,
    // The write end is close-on-exec and only this process retains it.
    _alive: ChildStdin,
}

impl Guard {
    pub fn start(command: &mut Command) -> io::Result<Option<Self>> {
        let Some(program) = GUARD_PROGRAM.get() else {
            command.process_group(0);
            return Ok(None);
        };
        let mut guard = Command::new(program);
        guard
            .arg(GUARD_ARGUMENT)
            .env_clear()
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        crate::pdf::inheritance::restrict(&mut guard)?;
        let mut child = guard.spawn()?;
        let group = i32::try_from(child.id()).map_err(io::Error::other)?;
        let alive = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("parser guard pipe"))?;
        command.process_group(group);
        Ok(Some(Self { child, _alive: alive }))
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        use nix::sys::signal::{Signal, killpg};
        use nix::unistd::Pid;
        if let Ok(group) = i32::try_from(self.child.id()) {
            let _ = killpg(Pid::from_raw(group), Signal::SIGKILL);
        }
        let _ = self.child.wait();
    }
}

extern "C" fn terminate(signal: i32) {
    if TERMINATING
        .compare_exchange(0, signal, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        let fd = WAKE.load(Ordering::SeqCst);
        if fd != -1 {
            let errno = Errno::last_raw();
            // SAFETY: the handler uses a nonblocking pipe held open through
            // process exit. A single-byte write is async-signal-safe.
            unsafe { nix::libc::write(fd, [1_u8].as_ptr().cast(), 1) };
            Errno::set_raw(errno);
        }
    }
}

/// The executable's sole exit path after signal handling is installed.
#[must_use = "finish the executable through Termination::exit"]
pub struct Termination {
    completed: mpsc::Sender<u8>,
    wake: io::PipeWriter,
    waiter: std::thread::JoinHandle<()>,
}

impl Termination {
    /// Finish a command through the signal waiter. A pending signal takes
    /// priority over `status`, after all active adapter groups are reaped.
    pub fn exit(mut self, status: u8) -> ! {
        if self.completed.send(status).is_ok() && self.wake.write_all(&[1]).is_ok() {
            let _ = self.waiter.join();
        }
        // The waiter exits the process. Only a failed waiter reaches here.
        std::process::exit(70);
    }
}

/// Call once at executable startup, before starting threads, then finish
/// through the returned owner's `exit`. SIGINT and SIGTERM stop and reap
/// all active adapter groups before exiting with 128 + signal. Embedders
/// retain their own signal and cancellation policy.
pub fn exit_on_termination() -> io::Result<Termination> {
    use nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet, Signal, sigaction};
    let (mut reading, wake) = io::pipe()?;
    fcntl(&wake, FcntlArg::F_SETFL(OFlag::O_NONBLOCK))?;
    WAKE.store(wake.as_raw_fd(), Ordering::SeqCst);
    let action = SigAction::new(SigHandler::Handler(terminate), SaFlags::SA_RESTART, SigSet::empty());
    // SAFETY: the static handler accesses only lock-free atomics and the wake
    // pipe, and has the signal ABI. Install before any child exists.
    let interrupt = match unsafe { sigaction(Signal::SIGINT, &action) } {
        Ok(previous) => previous,
        Err(error) => {
            WAKE.store(-1, Ordering::SeqCst);
            return Err(error.into());
        }
    };
    // SAFETY: the same static handler; restore SIGINT on partial failure.
    let termination = match unsafe { sigaction(Signal::SIGTERM, &action) } {
        Ok(previous) => previous,
        Err(error) => {
            WAKE.store(-1, Ordering::SeqCst);
            // SAFETY: restore the exact action returned by sigaction.
            unsafe { sigaction(Signal::SIGINT, &interrupt) }?;
            return Err(error.into());
        }
    };
    let (completed, normal_exit) = mpsc::channel();
    let waiter = std::thread::Builder::new()
        .name("ibis-termination".into())
        .spawn(move || {
            let _ = reading.read_exact(&mut [0]);
            // A signal already recorded wins over a simultaneous normal exit.
            let signal = TERMINATING.load(Ordering::SeqCst);
            if signal != 0 {
                let stop = INTERRUPTED
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take();
                if let Some(stop) = stop {
                    stop();
                }
                while ACTIVE.load(Ordering::SeqCst) != 0 {
                    std::thread::sleep(super::POLL);
                }
                std::process::exit(128 + signal);
            }
            std::process::exit(i32::from(normal_exit.try_recv().unwrap_or(70)));
        });
    match waiter {
        Ok(waiter) => Ok(Termination {
            completed,
            wake,
            waiter,
        }),
        Err(error) => {
            WAKE.store(-1, Ordering::SeqCst);
            // SAFETY: restore the exact actions returned above.
            unsafe {
                sigaction(Signal::SIGINT, &interrupt)?;
                sigaction(Signal::SIGTERM, &termination)?;
            }
            Err(error)
        }
    }
}

/// Wait for one native client operation to settle on interrupt. The callback
/// closes request input so the daemon can cancel and reap before EOF arrives.
pub struct ClientOperation {
    _active: Active,
}

impl ClientOperation {
    pub fn start(stop: impl FnOnce() + Send + 'static) -> Self {
        let active = Active::new();
        let mut interrupted = INTERRUPTED.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if terminating() {
            stop();
        } else {
            *interrupted = Some(Box::new(stop));
        }
        Self { _active: active }
    }
}

impl Drop for ClientOperation {
    fn drop(&mut self) {
        INTERRUPTED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }
}

pub(super) fn terminating() -> bool {
    TERMINATING.load(Ordering::SeqCst) != 0
}

pub(super) struct Active;
impl Active {
    pub(super) fn new() -> Self {
        ACTIVE.fetch_add(1, Ordering::SeqCst);
        Self
    }
}
impl Drop for Active {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::SeqCst);
        // Do not return an interrupted read to main while another exchange
        // is still cleaning up: the waiter owns termination of the process.
        while terminating() {
            std::thread::sleep(super::POLL);
        }
    }
}

pub(super) fn exited(child: &Child) -> io::Result<bool> {
    use nix::libc;
    // SAFETY: waitid writes into this zero-initialized siginfo_t. WNOHANG can
    // leave it zero. WNOWAIT leaves the leader waitable so the group ID
    // cannot be recycled before we send SIGKILL.
    unsafe {
        let mut info: libc::siginfo_t = std::mem::zeroed();
        if libc::waitid(
            libc::P_PID,
            child.id(),
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        ) == -1
        {
            return Err(io::Error::last_os_error());
        }
        Ok(info.si_pid() != 0)
    }
}

// A stopped I/O worker can leave even if a Unix program hands off a pipe or
// leaves its group. Only our end is nonblocking.
pub(super) struct Pipe<'a, T> {
    stream: T,
    stop: &'a AtomicBool,
}

pub(super) fn pipe<T: AsFd>(stream: T, stop: &AtomicBool) -> io::Result<Pipe<'_, T>> {
    let flags = OFlag::from_bits_retain(fcntl(&stream, FcntlArg::F_GETFL)?);
    fcntl(&stream, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK))?;
    Ok(Pipe { stream, stop })
}

impl<T: AsFd> Pipe<'_, T> {
    fn io(&mut self, interest: PollFlags, mut operation: impl FnMut(&mut T) -> io::Result<usize>) -> io::Result<usize> {
        while !self.stop.load(Ordering::Relaxed) {
            match operation(&mut self.stream) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    let mut fds = [PollFd::new(self.stream.as_fd(), interest)];
                    match poll(&mut fds, super::POLL_MS) {
                        Ok(_) | Err(Errno::EINTR) => {}
                        Err(error) => return Err(error.into()),
                    }
                }
                result => return result,
            }
        }
        Err(io::ErrorKind::TimedOut.into())
    }
}

impl<T: Read + AsFd> Read for Pipe<'_, T> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.io(PollFlags::POLLIN, |stream| stream.read(buffer))
    }
}

impl<T: Write + AsFd> Write for Pipe<'_, T> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.io(PollFlags::POLLOUT, |stream| stream.write(buffer))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}
