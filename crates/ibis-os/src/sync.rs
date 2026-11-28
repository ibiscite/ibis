//! Recover shared state after a contained worker panic, and keep the panic's
//! message for diagnosis.
use std::io::Write;
use std::sync::{LockResult, Mutex, MutexGuard};

/// The value behind a lock, whether or not a contained panic poisoned it.
pub fn recover<T>(result: LockResult<T>) -> T {
    result.unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Lock a mutex whether or not a contained panic poisoned it.
pub trait MutexExt<T> {
    fn recover_lock(&self) -> MutexGuard<'_, T>;
}
impl<T> MutexExt<T> for Mutex<T> {
    fn recover_lock(&self) -> MutexGuard<'_, T> {
        let guard = recover(self.lock());
        self.clear_poison();
        guard
    }
}

/// Contain a panic and retain its message for diagnosis.
pub fn attempt<T>(work: impl FnOnce() -> T) -> std::thread::Result<T> {
    crate::panic_guard::attempt(work).inspect_err(|payload| {
        let message = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("non-text panic");
        log(format_args!("Ibis worker panic: {message}"));
    })
}

/// Diagnostics must never turn a recovered panic or a committed write into a failure.
pub fn log(message: std::fmt::Arguments<'_>) {
    let _ = writeln!(std::io::stderr(), "{message}");
}
