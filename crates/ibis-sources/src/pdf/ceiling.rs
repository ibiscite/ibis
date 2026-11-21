//! The PDF reader child's memory ceiling, enforced from the parent.
//!
//! - Windows: the child starts suspended, joins a Job Object whose process
//!   memory limit fails any commit past the ceiling, and only then runs. The
//!   job's completion port says when the limit was hit. Closing the job ends
//!   the child.
//! - macOS: no resource limit bounds memory (`RLIMIT_AS` and `RLIMIT_DATA`
//!   are not enforced), so the parent reads the child's physical footprint,
//!   resident and compressed memory, every few milliseconds and stops it past
//!   the ceiling.
//! - Linux: the child's own 3 GiB address-space limit stays the hard bound;
//!   the parent reads the child's resident size as on macOS, so a child
//!   stopped for memory is reported as one.

use std::io;
use std::process::{Child, Command};

#[cfg(windows)]
use crate::child::windows;

pub(super) struct Ceiling {
    #[cfg(not(windows))]
    limit: u64,
    #[cfg(windows)]
    job: windows::Job,
    hit: std::cell::Cell<bool>,
}

impl Ceiling {
    /// Before the child is spawned.
    pub(super) fn prepare(command: &mut Command) {
        #[cfg(windows)]
        windows::suspended(command);
        #[cfg(not(windows))]
        let _ = command;
    }

    /// Bound the just-spawned child before it runs any of the file.
    pub(super) fn start(child: &Child, limit: u64) -> io::Result<Self> {
        #[cfg(not(windows))]
        let _ = child;
        Ok(Self {
            #[cfg(not(windows))]
            limit,
            #[cfg(windows)]
            job: windows::Job::start(child, Some(limit))?,
            hit: std::cell::Cell::new(false),
        })
    }

    /// Whether the child has passed the ceiling; once true, it stays so.
    pub(super) fn exceeded(&self, child: &Child) -> bool {
        if !self.hit.get() {
            #[cfg(windows)]
            let hit = {
                let _ = child;
                self.job.memory_limit_hit()
            };
            #[cfg(not(windows))]
            let hit = footprint(child.id()).is_some_and(|used| used > self.limit);
            self.hit.set(hit);
        }
        self.hit.get()
    }

    /// Once the child has been reaped: whether it passed the ceiling. Unix
    /// keeps only what was seen while it ran, since its pid may be reused.
    pub(super) fn was_exceeded(&self) -> bool {
        #[cfg(windows)]
        if !self.hit.get() {
            self.hit.set(self.job.memory_limit_hit());
        }
        self.hit.get()
    }
}

/// The child's physical footprint in bytes, while it runs.
#[cfg(target_os = "macos")]
fn footprint(pid: u32) -> Option<u64> {
    use nix::libc;
    let pid = libc::c_int::try_from(pid).ok()?;
    // SAFETY: plain old data; all zeroes is a valid value.
    let mut info: libc::rusage_info_v2 = unsafe { std::mem::zeroed() };
    // SAFETY: RUSAGE_INFO_V2 writes exactly one rusage_info_v2 into `info`,
    // which outlives the call.
    let read = unsafe {
        libc::proc_pid_rusage(
            pid,
            libc::RUSAGE_INFO_V2,
            std::ptr::from_mut(&mut info).cast::<libc::rusage_info_t>(),
        )
    };
    (read == 0).then_some(info.ri_phys_footprint)
}

/// The child's resident size in bytes, while it runs.
#[cfg(target_os = "linux")]
fn footprint(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let kib = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))?
        .trim()
        .strip_suffix("kB")?
        .trim()
        .parse::<u64>()
        .ok()?;
    kib.checked_mul(1024)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn footprint(_pid: u32) -> Option<u64> {
    None
}
