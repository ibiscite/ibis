//! Recover engine state after a contained worker panic.
use std::io::Write;
use std::sync::{LockResult, Mutex, MutexGuard};

pub(crate) fn recover<T>(result: LockResult<T>) -> T {
    result.unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) trait MutexExt<T> {
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
pub(crate) fn attempt<T>(work: impl FnOnce() -> T) -> std::thread::Result<T> {
    ibis_core::panic_guard::attempt(work).inspect_err(|payload| {
        let message = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("non-text panic");
        log(format_args!("Ibis worker panic: {message}"));
    })
}

/// Diagnostics must never turn a recovered panic or a committed write into a failure.
pub(crate) fn log(message: std::fmt::Arguments<'_>) {
    let _ = writeln!(std::io::stderr(), "{message}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_caught_panic_does_not_disable_later_state_changes() {
        let state = Mutex::new(Vec::new());
        assert!(
            attempt(|| {
                state.recover_lock().push("before panic");
                let _guard = state.recover_lock();
                panic!("invented worker failure");
            })
            .is_err()
        );
        state.recover_lock().push("after panic");
        assert_eq!(*state.recover_lock(), ["before panic", "after panic"]);
        assert!(!state.is_poisoned());
    }
}
