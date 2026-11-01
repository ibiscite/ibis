#[cfg(any(target_os = "macos", test))]
use std::sync::Mutex;
#[cfg(any(target_os = "macos", test))]
use std::time::Instant;

#[cfg(any(target_os = "macos", test))]
use ibis_core::destination::CopyReason;

#[cfg(any(target_os = "macos", test))]
use super::capture::Front;
#[cfg(any(target_os = "macos", test))]
use super::{CopyProof, DeliveryRequest, POST_LIMIT, SafetyStamp};

#[cfg(any(target_os = "macos", test))]
#[derive(Clone)]
pub(super) struct PostRequest {
    pub(super) token: u32,
    pub(super) pid: i32,
    pub(super) id: String,
    pub(super) change_count: isize,
    pub(super) presentation: u64,
    pub(super) stamp: SafetyStamp,
    pub(super) window: Option<u32>,
    pub(super) began: Instant,
    pub(super) deadline: Instant,
}

#[cfg(any(target_os = "macos", test))]
impl PostRequest {
    pub(super) fn new(request: DeliveryRequest, proof: &CopyProof) -> Self {
        Self {
            token: proof.token,
            pid: request.pid,
            id: request.id,
            change_count: proof.change_count,
            presentation: proof.presentation,
            stamp: proof.stamp,
            window: proof.captured.as_ref().and_then(|captured| captured.window),
            began: request.began,
            deadline: request.deadline,
        }
    }

    pub(super) fn queued(mut self, now: Instant) -> Self {
        self.deadline = self.deadline.min(now + POST_LIMIT);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
pub(super) enum PostResult {
    Pasted,
    Refused,
    SecureInput,
    Unavailable,
    Expired,
    TargetChanged,
    ClipboardChanged,
    AccessibilityRequired,
    NoPermission,
    #[cfg(any(target_os = "windows", test))]
    SecureField,
    #[cfg(any(target_os = "windows", test))]
    KeyReleaseFailed,
}

#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Copy)]
pub(super) enum InputPress {
    Key,
    LeftMouse,
    RightMouse,
    OtherMouse,
}

/// These calls run in one main-thread task. The only AX query is the bounded,
/// identity-only focused-window proof (ADR 0029); never clipboard contents.
#[cfg(any(target_os = "macos", test))]
pub(super) trait PostDesk {
    fn now(&self) -> Instant;
    fn stamp(&self) -> SafetyStamp;
    fn post_event_granted(&self) -> bool;
    fn current(&self, presentation: u64) -> bool;
    /// Run refresh after the initial provider reads, before the tightly
    /// bounded final window/global-focus pair. No lock follows that pair.
    fn frontmost(&self, refresh: impl FnOnce() -> Result<(), CopyReason>) -> Result<Option<Front>, CopyReason>;
    fn change_count(&self) -> isize;
    fn secure_input(&self) -> Option<bool>;
    fn seconds_since_input(&self, event: InputPress) -> f64;
    fn input_unchanged(&self, began: Instant) -> Result<(), PostResult> {
        check_input_since(self, began)
    }
    fn post(&self, pid: i32) -> bool;
}

#[cfg(any(target_os = "macos", test))]
pub(super) fn check_input_since(desk: &(impl PostDesk + ?Sized), began: Instant) -> Result<(), PostResult> {
    for event in [
        InputPress::Key,
        InputPress::LeftMouse,
        InputPress::RightMouse,
        InputPress::OtherMouse,
    ] {
        let age = desk.seconds_since_input(event);
        if !age.is_finite() || age < 0.0 {
            return Err(PostResult::Unavailable);
        }
        // Sample our clock AFTER the native read: skew may conservatively
        // refuse an earlier event, but cannot move the delivery boundary later.
        let elapsed = desk
            .now()
            .checked_duration_since(began)
            .ok_or(PostResult::Unavailable)?;
        if age <= elapsed.as_secs_f64() {
            return Err(PostResult::TargetChanged);
        }
    }
    Ok(())
}

#[cfg(any(target_os = "macos", test))]
pub(super) fn checked_post(desk: &impl PostDesk, request: &PostRequest) -> PostResult {
    if let Err(failure) = request.stamp.check(desk.stamp()) {
        return failure.result();
    }
    if desk.now() >= request.deadline {
        return PostResult::Unavailable;
    }
    if !desk.current(request.presentation) {
        return PostResult::Refused;
    }
    let target_matches = |refresh: bool| {
        let front = desk
            .frontmost(|| {
                if !refresh {
                    return Ok(());
                }
                // AX provider messaging can wait. Refresh clipboard/security after
                // it, then allow only bounded final identity reads before post.
                request.stamp.check(desk.stamp()).map_err(|failure| failure.reason())?;
                if !desk.post_event_granted() {
                    return Err(CopyReason::NoPermission);
                }
                let secure = desk.secure_input();
                if desk.now() >= request.deadline {
                    return Err(CopyReason::PasteUnavailable);
                }
                match secure {
                    Some(true) => return Err(CopyReason::SecureInput),
                    None => return Err(CopyReason::PasteUnavailable),
                    Some(false) => {}
                }
                if desk.change_count() != request.change_count {
                    return Err(CopyReason::ClipboardChanged);
                }
                if desk.now() >= request.deadline {
                    return Err(CopyReason::PasteUnavailable);
                }
                Ok(())
            })
            .map_err(|reason| match reason {
                CopyReason::TargetChanged => PostResult::TargetChanged,
                CopyReason::PasteExpired => PostResult::Expired,
                CopyReason::SecureInput => PostResult::SecureInput,
                CopyReason::ClipboardChanged => PostResult::ClipboardChanged,
                CopyReason::AccessibilityRequired => PostResult::AccessibilityRequired,
                CopyReason::NoPermission => PostResult::NoPermission,
                _ => PostResult::Unavailable,
            })?;
        if front.is_some_and(|front| {
            front.pid == request.pid
                && front.id.as_deref() == Some(&request.id)
                && request.window.is_some()
                && front.window == request.window
        }) {
            Ok(())
        } else {
            Err(PostResult::TargetChanged)
        }
    };
    if let Err(failure) = target_matches(false) {
        return failure;
    }
    let secure_input = desk.secure_input();
    if desk.now() >= request.deadline {
        return PostResult::Unavailable;
    }
    match secure_input {
        Some(true) => return PostResult::SecureInput,
        None => return PostResult::Unavailable,
        Some(false) => {}
    }
    if desk.change_count() != request.change_count {
        return PostResult::ClipboardChanged;
    }
    if desk.now() >= request.deadline {
        return PostResult::Unavailable;
    }
    if let Err(failure) = request.stamp.check(desk.stamp()) {
        return failure.result();
    }
    if desk.now() >= request.deadline {
        return PostResult::Unavailable;
    }
    // All shared-state locks precede this last target check. Its bounded AX
    // read includes lock-free completion/continuity checks before comparison.
    // Posting is still process-directed: public macOS APIs cannot atomically
    // bind Command-V to a foreign window. Keep this remaining gap explicit.
    if let Err(failure) = target_matches(true) {
        return failure;
    }
    if let Err(failure) = desk.input_unchanged(request.began) {
        return failure;
    }
    if desk.now() >= request.deadline {
        return PostResult::Unavailable;
    }
    if desk.post(request.pid) {
        PostResult::Pasted
    } else {
        PostResult::Refused
    }
}

/// Cancellation and posting share a lock: a timed-out queued task cannot
/// later post, and a completed post cannot be reported as merely copied.
#[cfg(any(target_os = "macos", test))]
#[derive(Default)]
pub(super) struct PendingPost(Mutex<Option<PostResult>>);

#[cfg(any(target_os = "macos", test))]
impl PendingPost {
    pub(super) fn run(&self, work: impl FnOnce() -> PostResult) -> PostResult {
        let Ok(mut result) = self.0.lock() else {
            return PostResult::Unavailable;
        };
        *result.get_or_insert_with(work)
    }

    pub(super) fn cancel(&self) -> PostResult {
        self.run(|| PostResult::Unavailable)
    }
}
