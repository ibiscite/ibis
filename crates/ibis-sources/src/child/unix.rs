//! Keep the group leader's PID reserved until the final group signal.

use std::io;
use std::process::Child;

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
