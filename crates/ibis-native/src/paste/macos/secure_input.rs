//! The only raw macOS paste call (ADR 0029): a local Carbon flag query.
//! No AX access, focused-element attributes or additional permission.

use objc2_foundation::NSThread;

// SAFETY: The SDK declares a no-argument C function returning Boolean (u8).
#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn IsSecureEventInputEnabled() -> u8;
}

/// A safe entry point even if accidentally called away from the main thread.
pub(super) fn enabled() -> Option<bool> {
    if !NSThread::isMainThread_class() {
        return None;
    }
    // SAFETY: The no-argument Carbon query returns a Boolean (UInt8), and
    // the main-thread check above upholds its threading requirement.
    Some(unsafe { IsSecureEventInputEnabled() } != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carbon_is_never_queried_from_a_test_worker() {
        assert!(!NSThread::isMainThread_class());
        assert_eq!(enabled(), None);
    }
}
