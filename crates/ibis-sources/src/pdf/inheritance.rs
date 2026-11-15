//! No caller descriptors survive the extractor's exec, including descriptors
//! opened outside Rust. Mark them close-on-exec rather than closing Rust's
//! private exec-error pipe: a missing executable must still fail `spawn`.

use nix::libc;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::Command;

pub(super) fn restrict(command: &mut Command) -> io::Result<()> {
    use nix::sys::resource::{Resource, getrlimit};
    let (soft, _) = getrlimit(Resource::RLIMIT_NOFILE)?;
    let mut end = i32::try_from(soft).map_err(|_| io::Error::from(io::ErrorKind::Unsupported))?;
    // Include descriptors already open above a subsequently lowered soft
    // limit. New opens in other threads remain bounded by the soft limit.
    let directory = if cfg!(target_os = "linux") {
        "/proc/self/fd"
    } else {
        "/dev/fd"
    };
    for entry in std::fs::read_dir(directory)? {
        if let Some(fd) = entry?.file_name().to_str().and_then(|name| name.parse::<i32>().ok()) {
            end = end.max(fd.saturating_add(1));
        }
    }
    // SAFETY: after fork the closure uses only async-signal-safe syscalls and
    // raw OS errors, without allocation, locks or inherited Rust state. Only
    // the child's descriptor table is changed, after stdio has been installed.
    unsafe {
        command.pre_exec(move || {
            #[cfg(target_os = "linux")]
            {
                // CLOSE_RANGE_CLOEXEC (Linux 5.11+), including all high fds.
                let result = libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, 4u32);
                if result == 0 {
                    return Ok(());
                }
                let error = io::Error::last_os_error();
                if !matches!(error.raw_os_error(), Some(libc::ENOSYS | libc::EINVAL)) {
                    return Err(error);
                }
            }
            for fd in 3..end {
                if libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) == -1 {
                    let error = io::Error::last_os_error();
                    if error.raw_os_error() != Some(libc::EBADF) {
                        return Err(error);
                    }
                }
            }
            Ok(())
        });
    }
    Ok(())
}
