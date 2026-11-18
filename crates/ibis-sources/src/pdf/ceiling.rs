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
            job: windows::Job::start(child, limit)?,
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

#[cfg(windows)]
mod windows {
    use std::io;
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::process::CommandExt;
    use std::process::{Child, Command};
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::IO::{CreateIoCompletionPort, GetQueuedCompletionStatus, OVERLAPPED};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOBOBJECT_ASSOCIATE_COMPLETION_PORT,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectAssociateCompletionPortInformation,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
    };
    use windows_sys::Win32::System::SystemServices::JOB_OBJECT_MSG_PROCESS_MEMORY_LIMIT;
    use windows_sys::Win32::System::Threading::{CREATE_SUSPENDED, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};

    /// A handle this module owns and closes once.
    struct Owned(HANDLE);
    impl Owned {
        fn new(raw: HANDLE) -> io::Result<Self> {
            if raw.is_null() || raw == INVALID_HANDLE_VALUE {
                Err(io::Error::last_os_error())
            } else {
                Ok(Self(raw))
            }
        }
    }
    impl Drop for Owned {
        fn drop(&mut self) {
            // SAFETY: a valid handle this wrapper owns, closed only here.
            unsafe { CloseHandle(self.0) };
        }
    }

    pub(super) fn suspended(command: &mut Command) {
        command.creation_flags(CREATE_SUSPENDED);
    }

    pub(super) struct Job {
        // Closing the job ends a child still in it.
        _job: Owned,
        port: Owned,
    }

    impl Job {
        pub(super) fn start(child: &Child, limit: u64) -> io::Result<Self> {
            let limit = usize::try_from(limit).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
            // SAFETY: no attributes and no name; the result is checked.
            let job = Owned::new(unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) })?;
            // SAFETY: a new completion port, not tied to a file; checked.
            let port = Owned::new(unsafe { CreateIoCompletionPort(INVALID_HANDLE_VALUE, std::ptr::null_mut(), 0, 1) })?;
            let association = JOBOBJECT_ASSOCIATE_COMPLETION_PORT {
                CompletionKey: std::ptr::null_mut(),
                CompletionPort: port.0,
            };
            // SAFETY: the structure matches the information class and its size.
            check(unsafe {
                SetInformationJobObject(
                    job.0,
                    JobObjectAssociateCompletionPortInformation,
                    std::ptr::from_ref(&association).cast(),
                    size_of::<JOBOBJECT_ASSOCIATE_COMPLETION_PORT>() as u32,
                )
            })?;
            // SAFETY: plain old data; all zeroes is a valid value.
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_PROCESS_MEMORY
                | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;
            limits.ProcessMemoryLimit = limit;
            // SAFETY: the structure matches the information class and its size.
            check(unsafe {
                SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    std::ptr::from_ref(&limits).cast(),
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            })?;
            // SAFETY: the child's process handle stays valid while `child` lives.
            check(unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle()) })?;
            resume(child.id())?;
            Ok(Self { _job: job, port })
        }

        /// Whether the job reported a commit refused at the process limit.
        pub(super) fn memory_limit_hit(&self) -> bool {
            let mut hit = false;
            loop {
                let mut message = 0u32;
                let mut key = 0usize;
                let mut overlapped: *mut OVERLAPPED = std::ptr::null_mut();
                // SAFETY: the out-pointers outlive the call; zero timeout.
                let got = unsafe { GetQueuedCompletionStatus(self.port.0, &mut message, &mut key, &mut overlapped, 0) };
                if got == 0 {
                    return hit;
                }
                hit |= message == JOB_OBJECT_MSG_PROCESS_MEMORY_LIMIT;
            }
        }
    }

    fn check(result: i32) -> io::Result<()> {
        if result == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    /// Resume the only thread of a process created suspended.
    fn resume(process: u32) -> io::Result<()> {
        // SAFETY: a snapshot of the system's threads; checked.
        let snapshot = Owned::new(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) })?;
        // SAFETY: plain old data; all zeroes is a valid value.
        let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
        entry.dwSize = size_of::<THREADENTRY32>() as u32;
        let mut resumed = 0;
        // SAFETY: `entry` is sized and outlives each call.
        let mut more = unsafe { Thread32First(snapshot.0, &mut entry) } != 0;
        while more {
            if entry.th32OwnerProcessID == process {
                // SAFETY: a thread of our own suspended child; checked.
                let thread = Owned::new(unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) })?;
                // SAFETY: a valid thread handle with resume access.
                if unsafe { ResumeThread(thread.0) } == u32::MAX {
                    return Err(io::Error::last_os_error());
                }
                resumed += 1;
            }
            // SAFETY: as for Thread32First.
            more = unsafe { Thread32Next(snapshot.0, &mut entry) } != 0;
        }
        if resumed == 1 {
            Ok(())
        } else {
            Err(io::Error::from(io::ErrorKind::NotFound))
        }
    }
}
