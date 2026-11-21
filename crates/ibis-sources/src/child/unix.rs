//! Keep the group leader's PID reserved until the final group signal.

use std::io::{self, Read, Write};
use std::os::fd::AsFd;
use std::process::Child;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering};
use std::sync::mpsc;

use nix::errno::Errno;
use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::poll::{PollFd, PollFlags, poll};

static TERMINATING: AtomicI32 = AtomicI32::new(0);
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

extern "C" fn terminate(signal: i32) {
    // Only a lock-free atomic operation runs in the signal handler. The
    // ordinary worker threads perform all process and pipe cleanup.
    let _ = TERMINATING.compare_exchange(0, signal, Ordering::SeqCst, Ordering::SeqCst);
}

/// The executable's sole exit path after signal handling is installed.
#[must_use = "finish the executable through Termination::exit"]
pub struct Termination {
    completed: mpsc::Sender<u8>,
    waiter: std::thread::JoinHandle<()>,
}

impl Termination {
    /// Finish a command through the signal waiter. A pending signal takes
    /// priority over `status`, after all active adapter groups are reaped.
    pub fn exit(self, status: u8) -> ! {
        if self.completed.send(status).is_ok() {
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
    let action = SigAction::new(SigHandler::Handler(terminate), SaFlags::SA_RESTART, SigSet::empty());
    // SAFETY: the static handler touches only a lock-free atomic and has the
    // signal ABI. Install at startup, before any child exists.
    let interrupt = unsafe { sigaction(Signal::SIGINT, &action) }?;
    // SAFETY: the same static handler; restore SIGINT on partial failure.
    let termination = match unsafe { sigaction(Signal::SIGTERM, &action) } {
        Ok(previous) => previous,
        Err(error) => {
            // SAFETY: restore the exact action returned by sigaction.
            unsafe { sigaction(Signal::SIGINT, &interrupt) }?;
            return Err(error.into());
        }
    };
    let (completed, normal_exit) = mpsc::channel();
    let waiter = std::thread::Builder::new()
        .name("ibis-termination".into())
        .spawn(move || {
            loop {
                let normal = normal_exit.recv_timeout(super::POLL).ok();
                // Normal completion cannot bypass a signal the handler recorded
                // while this waiter was asleep, even with no active exchanges.
                let signal = TERMINATING.load(Ordering::SeqCst);
                if signal != 0 {
                    while ACTIVE.load(Ordering::SeqCst) != 0 {
                        std::thread::sleep(super::POLL);
                    }
                    std::process::exit(128 + signal);
                }
                if let Some(status) = normal {
                    std::process::exit(i32::from(status));
                }
            }
        });
    match waiter {
        Ok(waiter) => Ok(Termination { completed, waiter }),
        Err(error) => {
            // SAFETY: restore the exact actions returned above.
            unsafe {
                sigaction(Signal::SIGINT, &interrupt)?;
                sigaction(Signal::SIGTERM, &termination)?;
            }
            Err(error)
        }
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
