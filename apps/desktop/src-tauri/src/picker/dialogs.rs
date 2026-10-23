//! Picker-owned file choosers, macOS update consent and visible macOS login
//! reapproval sheets prevent hiding on blur.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
pub(crate) struct ChooserOpen(Arc<AtomicBool>);

#[must_use]
pub(crate) struct ChooserLease(Arc<AtomicBool>);

impl ChooserOpen {
    pub(crate) fn begin(&self) -> Option<ChooserLease> {
        self.0
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| ChooserLease(Arc::clone(&self.0)))
    }

    pub(crate) fn is_open(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

impl ChooserLease {
    pub(crate) fn during<T>(self, action: impl FnOnce() -> T) -> T {
        let _lease = self;
        action()
    }
}

impl Drop for ChooserLease {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::picker::hides_on_blur;

    #[test]
    fn only_the_owned_dialog_blocks_automatic_blur() {
        let chooser = ChooserOpen::default();
        assert!(hides_on_blur(true, chooser.is_open(), false));
        let lease = chooser.begin().unwrap_or_else(|| panic!("first chooser"));
        assert!(chooser.begin().is_none());
        lease.during(|| assert!(!hides_on_blur(true, chooser.is_open(), false)));
        assert!(hides_on_blur(true, chooser.is_open(), false));
    }

    #[test]
    fn concurrent_requests_acquire_only_one_chooser() {
        let chooser = ChooserOpen::default();
        let gate = std::sync::Barrier::new(2);
        let attempt = || {
            let lease = chooser.begin();
            gate.wait();
            lease.is_some()
        };
        let won = std::thread::scope(|scope| {
            let first = scope.spawn(attempt);
            let second = scope.spawn(attempt);
            usize::from(first.join().unwrap_or_else(|e| panic!("{e:?}")))
                + usize::from(second.join().unwrap_or_else(|e| panic!("{e:?}")))
        });
        assert_eq!(won, 1);
        assert!(!chooser.is_open());
    }

    #[test]
    fn discarded_and_panicking_callbacks_release_the_chooser() {
        let chooser = ChooserOpen::default();
        let lease = chooser.begin().unwrap_or_else(|| panic!("first chooser"));
        drop(move || lease.during(|| {}));
        assert!(!chooser.is_open());
        let lease = chooser.begin().unwrap_or_else(|| panic!("next chooser"));
        assert!(std::panic::catch_unwind(|| lease.during(|| panic!("synthetic callback"))).is_err());
        assert!(!chooser.is_open());
        assert!(chooser.begin().is_some());
    }
}
