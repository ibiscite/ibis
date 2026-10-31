//! The narrow native boundary for lifecycle guards (ADR 0029).
//! Never requests AX/screen capture, looks up a title, or reads window contents.

use std::ffi::{CStr, c_char, c_int};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, Once, OnceLock};
use std::time::Duration;

use block2::RcBlock;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSDistributedNotificationCenter, NSNotificationCenter, NSString, NSThread};

use super::super::SafetyStamp;

static INSTALLED: Once = Once::new();
static EPOCH: AtomicU64 = AtomicU64::new(0);
static CHECKS: OnceLock<Mutex<Option<Checks>>> = OnceLock::new();

struct Checks {
    tokens: Vec<c_int>,
    clock_offset: Option<(i128, i128)>,
}

// SAFETY: _time.h and notify.h declare these signatures. Clock IDs are the
// SDK enum's unsigned values; tokens/status are int/uint32_t respectively.
unsafe extern "C" {
    fn clock_gettime_nsec_np(clock: u32) -> u64;
    fn notify_register_check(name: *const c_char, token: *mut c_int) -> u32;
    fn notify_check(token: c_int, changed: *mut c_int) -> u32;
}

fn invalidate() {
    // Saturate instead of allowing an old epoch to become current again.
    let mut epoch = EPOCH.load(Ordering::SeqCst);
    while let Some(next) = epoch.checked_add(1) {
        match EPOCH.compare_exchange_weak(epoch, next, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return,
            Err(actual) => epoch = actual,
        }
    }
}

/// Installed once on the main thread before the first target capture. No timers
/// or hidden polling. Notification centers retain these process-lifetime blocks.
pub(super) fn install() {
    if !NSThread::isMainThread_class() {
        return;
    }
    INSTALLED.call_once(|| {
        let _ = CHECKS.set(Mutex::new(register_checks()));
        let workspace = NSWorkspace::sharedWorkspace().notificationCenter();
        for name in [
            "NSWorkspaceWillSleepNotification",
            "NSWorkspaceDidWakeNotification",
            "NSWorkspaceScreensDidSleepNotification",
            "NSWorkspaceScreensDidWakeNotification",
            "NSWorkspaceSessionDidResignActiveNotification",
            "NSWorkspaceSessionDidBecomeActiveNotification",
        ] {
            observe(&workspace, name);
        }
        let distributed = NSDistributedNotificationCenter::defaultCenter();
        for name in ["com.apple.screenIsLocked", "com.apple.screenIsUnlocked"] {
            observe(&distributed, name);
        }
        invalidate();
    });
}

fn register_checks() -> Option<Checks> {
    let mut tokens = Vec::new();
    for name in [
        c"com.apple.screenIsLocked",
        c"com.apple.screenIsUnlocked",
        c"com.apple.coregraphics.GUIConsoleSessionChanged",
        c"com.apple.coregraphics.GUISessionUserChanged",
    ] {
        tokens.push(register(name)?);
    }
    Some(Checks {
        tokens,
        clock_offset: None,
    })
}

fn register(name: &CStr) -> Option<c_int> {
    let mut token = 0;
    // SAFETY: name is NUL-terminated static storage, token is a valid output
    // pointer. No callbacks, ports or active notifications are requested.
    if unsafe { notify_register_check(name.as_ptr(), &mut token) } != 0 {
        return None;
    }
    // notify_check initially reports changed, even before the first event.
    changed(token)?;
    Some(token)
}

fn changed(token: c_int) -> Option<bool> {
    let mut changed = 0;
    // SAFETY: token is from a successful registration and retained for this
    // process's lifetime; the output points to an initialized local int.
    (unsafe { notify_check(token, &mut changed) } == 0).then_some(changed != 0)
}

fn observe(center: &NSNotificationCenter, name: &str) {
    let block = RcBlock::new(|_| invalidate());
    // SAFETY: No object filter or operation queue. The callback captures no
    // thread-affine objects and only advances an atomic. The center retains
    // the copied block and observer for this process's lifetime.
    let observer = unsafe {
        center.addObserverForName_object_queue_usingBlock(Some(&NSString::from_str(name)), None, None, &block)
    };
    std::mem::forget(observer);
}

pub(super) fn stamp() -> SafetyStamp {
    let invalid = SafetyStamp {
        epoch: 0,
        elapsed: Duration::ZERO,
    };
    let Some(checks) = CHECKS.get() else { return invalid };
    let Ok(mut checks) = checks.try_lock() else {
        return invalid;
    };
    let Some(state) = checks.as_mut() else { return invalid };
    for token in &state.tokens {
        match changed(*token) {
            Some(true) => invalidate(),
            Some(false) => {}
            None => {
                *checks = None;
                return invalid;
            }
        }
    }
    // SAFETY: SDK CLOCK_MONOTONIC_RAW=4 includes sleep, UPTIME_RAW=8 does
    // not. Each call is thread-safe. Bracketing bounds the sampling skew;
    // incompatible offsets prove a suspension even before observers run.
    let (before, awake, elapsed) = unsafe {
        (
            clock_gettime_nsec_np(4),
            clock_gettime_nsec_np(8),
            clock_gettime_nsec_np(4),
        )
    };
    match clock_changed(&mut state.clock_offset, before, awake, elapsed) {
        Some(true) => invalidate(),
        Some(false) => {}
        None => {
            // Unavailable sampling is not evidence of a lifecycle transition.
            // This request refuses; retain the old offset so the next sound
            // sample can still detect any real suspension.
            return invalid;
        }
    }
    SafetyStamp {
        epoch: if elapsed == 0 { 0 } else { EPOCH.load(Ordering::SeqCst) },
        elapsed: Duration::from_nanos(elapsed),
    }
}

fn clock_changed(offset: &mut Option<(i128, i128)>, before: u64, awake: u64, after: u64) -> Option<bool> {
    // A slow/inconsistent sample cannot prove continuity. Refuse this check.
    if before == 0 || awake == 0 || after.checked_sub(before)? > 100_000 {
        return None;
    }
    // Allow nanosecond conversion rounding at either end of the interval.
    let next = (
        i128::from(before) - i128::from(awake) - 1,
        i128::from(after) - i128::from(awake) + 1,
    );
    let overlap = offset.map(|previous| (previous.0.max(next.0), previous.1.min(next.1)));
    let changed = overlap.is_some_and(|range| range.0 > range.1);
    *offset = Some(overlap.filter(|range| range.0 <= range.1).unwrap_or(next));
    Some(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_sleep_changes_the_epoch_even_before_workspace_callbacks_run() {
        let mut offset = None;
        assert_eq!(clock_changed(&mut offset, 100, 95, 110), Some(false));
        assert_eq!(clock_changed(&mut offset, 200, 195, 210), Some(false));
        // Continuous time advances 100 ms across sleep, awake time does not.
        assert_eq!(clock_changed(&mut offset, 100_000_200, 195, 100_000_210), Some(true));
        assert_eq!(clock_changed(&mut offset, 100_000_300, 295, 100_000_310), Some(false));
    }

    #[test]
    fn clock_failures_backwards_time_and_slow_samples_are_unproven() {
        for (before, awake, after) in [(0, 10, 11), (10, 0, 11), (10, 9, 8), (10, 9, 100_011)] {
            assert_eq!(clock_changed(&mut None, before, awake, after), None);
        }
    }

    #[test]
    fn a_sleep_inside_the_clock_bracket_cannot_hide_in_a_wide_offset_range() {
        let mut offset = None;
        assert_eq!(clock_changed(&mut offset, 100, 95, 110), Some(false));
        assert_eq!(clock_changed(&mut offset, 200, 195, 100_000_210), None);
    }
}
