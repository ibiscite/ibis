//! Start the child suspended and contain its entire process tree before resuming it.

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

pub(crate) fn suspended(command: &mut Command) {
    command.creation_flags(CREATE_SUSPENDED);
}

pub(crate) struct Job {
    // Closing the job ends a child still in it.
    _job: Owned,
    port: Option<Owned>,
}

impl Job {
    pub(crate) fn start(child: &Child, memory_limit: Option<u64>) -> io::Result<Self> {
        // SAFETY: no attributes and no name; the result is checked.
        let job = Owned::new(unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) })?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;
        let port = if let Some(limit) = memory_limit {
            limits.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_PROCESS_MEMORY;
            limits.ProcessMemoryLimit = usize::try_from(limit).map_err(|_| io::ErrorKind::InvalidInput)?;
            Some(memory_notifications(&job)?)
        } else {
            None
        };
        // SAFETY: the structure matches the information class and its size.
        check(unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&limits).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        })?;
        // SAFETY: the process handle is valid. The child is still suspended.
        check(unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle()) })?;
        resume(child.id())?;
        Ok(Self { _job: job, port })
    }

    /// Whether the job reported a commit refused at the process limit.
    pub(crate) fn memory_limit_hit(&self) -> bool {
        let Some(port) = &self.port else {
            return false;
        };
        let mut hit = false;
        loop {
            let mut message = 0u32;
            let mut key = 0usize;
            let mut overlapped: *mut OVERLAPPED = std::ptr::null_mut();
            // SAFETY: the out-pointers outlive the call; zero timeout.
            let got = unsafe { GetQueuedCompletionStatus(port.0, &mut message, &mut key, &mut overlapped, 0) };
            if got == 0 {
                return hit;
            }
            hit |= message == JOB_OBJECT_MSG_PROCESS_MEMORY_LIMIT;
        }
    }
}

/// A completion port reports memory refusals to the PDF reader.
fn memory_notifications(job: &Owned) -> io::Result<Owned> {
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
    Ok(port)
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
