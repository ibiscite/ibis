//! A short-lived child process that answers one request (decisions 0055 and
//! 0056): the request goes to its standard input, the answer comes from its
//! standard output, bounded in size and time.
//!
//! The child gets no environment and no inherited descriptors besides its
//! three streams, and its diagnostics are discarded, since they may hold the
//! person's data. On Unix it leads its own process group, so stopping it at
//! the deadline also stops any program it started. The PDF reader moves onto
//! this runner once the reading lane that changes it has landed.

use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

// SAFETY: only the post-fork descriptor boundary needs unsafe calls.
#[cfg(unix)]
#[allow(unsafe_code)]
#[path = "pdf/inheritance.rs"]
mod inheritance;

/// Why a child gave no answer: fixed reasons, never its output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChildError {
    /// The program could not be started.
    Unavailable,
    /// It did not finish before the deadline, or the caller cancelled.
    TimedOut,
    /// It ended without success.
    Failed,
    /// Its answer was larger than allowed.
    TooLarge,
}

/// One request to `program` with `arguments`, answered within `deadline`
/// and `max_answer` bytes.
pub(crate) fn run(
    program: &Path,
    arguments: &[&OsStr],
    request: Vec<u8>,
    deadline: Duration,
    max_answer: u64,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<u8>, ChildError> {
    let started = Instant::now();
    if cancelled() {
        return Err(ChildError::TimedOut);
    }
    let mut command = Command::new(program);
    command.args(arguments);
    command.env_clear();
    // Windows needs its system folder to start a process.
    #[cfg(windows)]
    if let Some(root) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", root);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
        inheritance::restrict(&mut command).map_err(|_| ChildError::Unavailable)?;
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| ChildError::Unavailable)?;
    let stop = |child: &mut std::process::Child| {
        #[cfg(unix)]
        if let Ok(id) = i32::try_from(child.id()) {
            let _ = nix::sys::signal::killpg(nix::unistd::Pid::from_raw(id), nix::sys::signal::Signal::SIGKILL);
        }
        let _ = child.kill();
        let _ = child.wait();
    };
    let (Some(mut input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
        stop(&mut child);
        return Err(ChildError::Unavailable);
    };
    // Write and read beside the wait, so neither full pipe stalls the child.
    let writer = std::thread::Builder::new()
        .name("ibis-child-request".into())
        .spawn(move || {
            let _ = input.write_all(&request);
        });
    let (answer, answered) = std::sync::mpsc::sync_channel(1);
    let reader = std::thread::Builder::new()
        .name("ibis-child-answer".into())
        .spawn(move || {
            let mut bytes = Vec::new();
            let read = output.take(max_answer + 1).read_to_end(&mut bytes).map(|_| bytes);
            let _ = answer.send(read);
        });
    if writer.is_err() || reader.is_err() {
        stop(&mut child);
        return Err(ChildError::Unavailable);
    }
    let status = loop {
        if cancelled() || started.elapsed() >= deadline {
            break Err(ChildError::TimedOut);
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(_) => break Err(ChildError::Failed),
        }
    };
    // Also stops any program it started, which may still hold the pipes.
    stop(&mut child);
    if !status?.success() {
        return Err(ChildError::Failed);
    }
    // A program that escaped the group may keep the pipe open: wait for
    // the answer no longer than the deadline, never join that thread.
    let remaining = deadline.saturating_sub(started.elapsed()).max(Duration::from_millis(100));
    let bytes = match answered.recv_timeout(remaining) {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(_)) => return Err(ChildError::Failed),
        Err(_) => return Err(ChildError::TimedOut),
    };
    if bytes.len() as u64 > max_answer {
        return Err(ChildError::TooLarge);
    }
    Ok(bytes)
}
