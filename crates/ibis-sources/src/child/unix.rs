//! Keep the group leader's PID reserved until the final group signal.

use std::io::{self, Read, Write};
use std::os::fd::AsFd;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};

use nix::errno::Errno;
use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::poll::{PollFd, PollFlags, poll};

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
