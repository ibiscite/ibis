//! Process-lifetime observers, with no timer or retained notification payload.

#![allow(unsafe_code)]

use block2::RcBlock;
use objc2_foundation::{NSNotificationCenter, NSString};

pub(crate) fn observe(center: &NSNotificationCenter, name: &str, changed: impl Fn() + Send + Sync + 'static) {
    let block = RcBlock::new(move |_| changed());
    // SAFETY: no object filter or operation queue is supplied. The sendable
    // callback captures no thread-affine objects and ignores the notification.
    // The center copies the block; its observer lives until process exit.
    let observer = unsafe {
        center.addObserverForName_object_queue_usingBlock(Some(&NSString::from_str(name)), None, None, &block)
    };
    std::mem::forget(observer);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    #[test]
    fn picker_notification_observer_filters_names_and_reads_current_values_after_reopen() {
        // A private center cannot change system preferences or notify the app.
        let center = NSNotificationCenter::new();
        let current = Arc::new(AtomicU64::new(41));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (read, observed) = (current.clone(), seen.clone());
        observe(&center, "ibis.synthetic.changed", move || {
            observed
                .lock()
                .unwrap_or_else(|e| panic!("{e}"))
                .push(read.load(Ordering::SeqCst));
        });
        let post = |name| {
            // SAFETY: the isolated center receives no object or user-info;
            // the synthetic notification name is a valid Foundation string.
            unsafe { center.postNotificationName_object(&NSString::from_str(name), None) };
        };
        post("ibis.synthetic.unrelated");
        assert!(seen.lock().unwrap_or_else(|e| panic!("{e}")).is_empty());
        post("ibis.synthetic.changed");
        current.store(43, Ordering::SeqCst); // close/reopen B before A's late notification
        post("ibis.synthetic.changed");
        assert_eq!(*seen.lock().unwrap_or_else(|e| panic!("{e}")), [41, 43]);
    }
}
