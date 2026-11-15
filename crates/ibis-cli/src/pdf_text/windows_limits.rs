//! A private job owns the extractor's process-memory budget before any PDF
//! parsing. No safe binding for job objects is available in our dependencies.

use std::io;
use std::mem::size_of;
use std::ptr::null;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOBOBJECT_BASIC_LIMIT_INFORMATION,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation, SetInformationJobObject,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

pub(super) struct Limit(HANDLE);

impl Limit {
    pub(super) fn new(bytes: usize) -> io::Result<Self> {
        // SAFETY: null security attributes and name create an unnamed,
        // non-inheritable job. This guard owns the returned handle exclusively.
        let handle = unsafe { CreateJobObjectW(null(), null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = Self(handle);
        let limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
            BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION {
                LimitFlags: JOB_OBJECT_LIMIT_PROCESS_MEMORY,
                ..Default::default()
            },
            ProcessMemoryLimit: bytes,
            ..Default::default()
        };
        // SAFETY: the live job and initialized structure have the exact type
        // and size required by this information class. Win32 copies the input.
        if unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the job is live and GetCurrentProcess returns a valid pseudo
        // handle. Only this short-lived child joins; failure prevents parsing.
        if unsafe { AssignProcessToJobObject(job.0, GetCurrentProcess()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }
}

impl Drop for Limit {
    fn drop(&mut self) {
        // SAFETY: this guard owns the job handle, which is closed exactly once.
        unsafe { CloseHandle(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_memory_job_is_installed_in_an_isolated_child() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "pdf_text::windows_limits::tests::job_limit_helper",
                "--ignored",
                "--nocapture",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    #[ignore = "subprocess helper; changes only its own job membership"]
    fn job_limit_helper() {
        use windows_sys::Win32::System::JobObjects::{IsProcessInJob, QueryInformationJobObject};
        let job = Limit::new(super::super::MAX_MEMORY_BYTES as usize).unwrap();
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        // SAFETY: the live job is queried into an initialized, correctly sized
        // buffer of the information class's exact type. No pointer escapes.
        assert_ne!(
            unsafe {
                QueryInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    (&mut limits as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                    std::ptr::null_mut(),
                )
            },
            0
        );
        assert_ne!(
            limits.BasicLimitInformation.LimitFlags & JOB_OBJECT_LIMIT_PROCESS_MEMORY,
            0
        );
        assert_eq!(limits.ProcessMemoryLimit, super::super::MAX_MEMORY_BYTES as usize);
        let mut in_job = 0;
        // SAFETY: both handles are live; Win32 writes one BOOL to this local.
        assert_ne!(unsafe { IsProcessInJob(GetCurrentProcess(), job.0, &mut in_job) }, 0);
        assert_ne!(in_job, 0);
    }
}
