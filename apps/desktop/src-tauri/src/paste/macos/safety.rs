//! The narrow native boundary for lifecycle guards (ADR 0029).
//! Never requests AX/screen capture, looks up a title, or reads window contents.

use std::ffi::{CStr, c_char, c_int};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, Once, OnceLock};
use std::time::Duration;

use block2::RcBlock;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSDistributedNotificationCenter, NSNotificationCenter, NSString, NSThread};

use super::super::{SafetyFailure, SafetyStamp};
use super::diagnostics;

// Two brackets contribute uncertainty to an offset comparison. Refuse each at
// 500 us to keep their combined skew below 1 ms (plus nanosecond rounding), the
// sub-deadline suspension threat tested below. This admits the 300 us skew case
// without allowing an unbounded bracket or widening previously established evidence.
const CLOCK_SAMPLE_LIMIT_NS: u64 = 500_000;

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

/// Used for AX's aggregate budget, even while the awake clock is suspended.
pub(super) fn elapsed() -> Duration {
    // SAFETY: SDK CLOCK_MONOTONIC_RAW=4; local, thread-safe, sleep-inclusive.
    Duration::from_nanos(unsafe { clock_gettime_nsec_np(4) })
}

/// Prepare shared state before AX; notification checks precede the last live
/// focus read. Finishing uses only atomics/clocks, never CHECKS' mutex or IPC.
pub(super) struct PostProbe {
    epoch: u64,
    offset: Option<(i128, i128)>,
    tokens: Vec<c_int>,
}

impl PostProbe {
    pub(super) fn new() -> Result<Self, SafetyFailure> {
        let stamp = stamp();
        if stamp.epoch == 0 || stamp.epoch == u64::MAX {
            return Err(SafetyFailure::Unavailable);
        }
        let checks = CHECKS.get().ok_or(SafetyFailure::Unavailable)?;
        let checks = checks.try_lock().map_err(|_| SafetyFailure::Unavailable)?;
        let state = checks.as_ref().ok_or(SafetyFailure::Unavailable)?;
        Ok(Self {
            epoch: stamp.epoch,
            offset: state.clock_offset,
            tokens: state.tokens.clone(),
        })
    }

    pub(super) fn before_focus(&self) -> Result<(), SafetyFailure> {
        for token in &self.tokens {
            match changed(*token) {
                Some(true) => invalidate(),
                Some(false) => {}
                None => return Err(SafetyFailure::Unavailable),
            }
        }
        Ok(())
    }

    pub(super) fn finish(&self) -> Result<SafetyStamp, SafetyFailure> {
        // SAFETY: Same bracketed SDK clocks as stamp(), but the correlation
        // is against this probe's owned offset, without shared-state access.
        let (before, awake, after) = unsafe {
            (
                clock_gettime_nsec_np(4),
                clock_gettime_nsec_np(8),
                clock_gettime_nsec_np(4),
            )
        };
        self.check_sample(EPOCH.load(Ordering::SeqCst), before, awake, after)
    }

    fn check_sample(&self, epoch: u64, before: u64, awake: u64, after: u64) -> Result<SafetyStamp, SafetyFailure> {
        let mut offset = self.offset;
        match clock_changed(&mut offset, before, awake, after) {
            Some(true) => return Err(SafetyFailure::Expired),
            Some(false) => {}
            None => return Err(SafetyFailure::Unavailable),
        }
        if epoch == 0 || epoch == u64::MAX {
            return Err(SafetyFailure::Unavailable);
        }
        if epoch != self.epoch {
            return Err(SafetyFailure::Expired);
        }
        Ok(SafetyStamp {
            epoch,
            elapsed: Duration::from_nanos(after),
        })
    }
}

fn clock_changed(offset: &mut Option<(i128, i128)>, before: u64, awake: u64, after: u64) -> Option<bool> {
    let width = after.checked_sub(before);
    if let Some(width) = width {
        diagnostics::clock(Duration::from_nanos(width));
    }
    // A slow/inconsistent sample cannot prove continuity. Refuse this check.
    if before == 0 || awake == 0 || width.is_none_or(|width| width >= CLOCK_SAMPLE_LIMIT_NS) {
        diagnostics::stage("lifecycle.clock_sample");
        return None;
    }
    // Allow nanosecond conversion rounding at either end of the interval.
    let next = (
        i128::from(before) - i128::from(awake) - 1,
        i128::from(after) - i128::from(awake) + 1,
    );
    let overlap = offset.map(|previous| (previous.0.max(next.0), previous.1.min(next.1)));
    let changed = overlap.is_some_and(|range| range.0 > range.1);
    if changed {
        diagnostics::stage("lifecycle.clock_offset");
    }
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
        for (before, awake, after) in [(0, 10, 11), (10, 0, 11), (10, 9, 8), (10, 9, 500_010)] {
            assert_eq!(clock_changed(&mut None, before, awake, after), None);
        }
    }

    #[test]
    fn scheduler_skew_over_one_hundred_microseconds_can_still_prove_continuity() {
        let mut offset = None;
        assert_eq!(clock_changed(&mut offset, 100, 95, 110), Some(false));
        assert_eq!(clock_changed(&mut offset, 200, 195, 300_200), Some(false));
        // The wide sample must not widen the previously established offset.
        assert_eq!(offset, Some((4, 16)));
        assert_eq!(clock_changed(&mut offset, 1_000_200, 195, 1_000_210), Some(true));
    }

    #[test]
    fn two_broad_samples_still_bound_ambiguous_suspension_below_a_millisecond_plus_rounding() {
        let mut offset = None;
        assert_eq!(clock_changed(&mut offset, 1_000, 1_000, 500_999), Some(false));
        // Worst opposite-edge skew in both brackets; a 1 ms + 9 ns offset
        // shift is still disjoint, before any observer can advance the epoch.
        assert_eq!(clock_changed(&mut offset, 2_000_010, 1_500_000, 2_500_009), Some(true));
    }

    #[test]
    fn a_sleep_inside_the_clock_bracket_cannot_hide_in_a_wide_offset_range() {
        let mut offset = None;
        assert_eq!(clock_changed(&mut offset, 100, 95, 110), Some(false));
        assert_eq!(clock_changed(&mut offset, 200, 195, 100_000_210), None);
    }

    #[test]
    fn a_sub_deadline_sleep_inside_ax_expires_without_waiting_for_an_epoch_callback() {
        let probe = PostProbe {
            epoch: 3,
            offset: Some((4, 16)),
            tokens: Vec::new(),
        };
        assert!(probe.check_sample(3, 250, 245, 260).is_ok());
        // Just 1 ms, shorter than even the final AX tail; awake time paused.
        assert!(matches!(
            probe.check_sample(3, 1_000_250, 245, 1_000_260),
            Err(SafetyFailure::Expired)
        ));
    }

    #[test]
    fn post_read_lifecycle_and_unavailable_sampling_keep_distinct_reasons() {
        let probe = PostProbe {
            epoch: 3,
            offset: Some((4, 16)),
            tokens: Vec::new(),
        };
        assert!(matches!(
            probe.check_sample(4, 250, 245, 260),
            Err(SafetyFailure::Expired)
        ));
        assert!(matches!(
            probe.check_sample(0, 250, 245, 260),
            Err(SafetyFailure::Unavailable)
        ));
        assert!(matches!(
            probe.check_sample(3, 0, 245, 260),
            Err(SafetyFailure::Unavailable)
        ));
    }
}
