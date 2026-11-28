//! The PDF extractor's child process (decision 0055): `ibis __extract-pdf`.
//!
//! The parent hands over an opened PDF as standard input and nothing else: no
//! path, no environment, no other handle. This process lowers its own limits,
//! then [`ibis_sources::pdf::read::child`] parses the bytes and answers on
//! standard output. A crash here only fails this one file.

use std::ffi::OsString;

/// Address space on Linux, a hard backstop behind the parent's memory
/// ceiling (`ibis_sources::pdf::MAX_MEMORY_BYTES`).
#[cfg(unix)]
const MAX_MEMORY_BYTES: u64 = 3 * 1024 * 1024 * 1024;
/// Processor seconds, a backstop behind the parent's ten-second deadline.
#[cfg(unix)]
const MAX_CPU_SECONDS: u64 = 20;

/// The child process itself: bounded first, then [`ibis_sources::pdf::read::child`] over its own
/// standard input and output.
pub fn run_child(arguments: &[OsString]) -> u8 {
    #[cfg(unix)]
    if limit_resources().is_err() {
        return 1;
    }
    ibis_sources::pdf::read::child(arguments, std::io::stdin().lock(), std::io::stdout().lock())
}

/// Lower the child's own limits before parsing: no core dumps, processor
/// seconds and, on Linux, address space. The parent's deadline and memory
/// ceiling, the per-stream inflation cap and the file-size cap bound the rest
/// everywhere.
#[cfg(unix)]
pub(crate) fn limit_resources() -> nix::Result<()> {
    use nix::sys::resource::{Resource, getrlimit, setrlimit};
    let limits = [
        (Resource::RLIMIT_CPU, MAX_CPU_SECONDS),
        (Resource::RLIMIT_CORE, 0),
        (Resource::RLIMIT_AS, MAX_MEMORY_BYTES),
    ];
    // Darwin may reject a finite address-space limit, so only Linux sets it.
    let count = if cfg!(target_os = "linux") { 3 } else { 2 };
    for (resource, limit) in limits.into_iter().take(count) {
        let (soft, hard) = getrlimit(resource)?;
        let hard = hard.min(limit);
        setrlimit(resource, soft.min(hard), hard)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "pdf_text_tests.rs"]
mod tests;
