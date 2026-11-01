use std::time::{Duration, Instant};

use ibis_core::destination::{CopyReason, Decision, PastePermission};

#[cfg(any(not(target_os = "windows"), test))]
use super::CopyProof;
use super::capture::Front;
use super::destination::delivery_decision;
use super::{
    Captured, Delivered, LATE, PostResult, RETURN_CHECKS, RETURN_LIMIT, RETURN_STEP, SETTLE, SYSTEM, SafetyFailure,
    SafetyStamp,
};

/// The system calls a paste needs, so the decision is testable without them.
pub(super) trait Desk {
    type Proof: CopyEvidence;
    /// One immutable start covers policy work, readiness and the queued post.
    #[cfg(any(target_os = "macos", test))]
    fn delivery_began(&self) -> Instant {
        Instant::now()
    }
    /// Release keys left by an earlier partial Windows injection, including
    /// on deliveries that will only copy. macOS has no pending releases.
    fn release_keys(&self) -> bool {
        true
    }
    fn stamp(&self) -> SafetyStamp {
        SafetyStamp::now()
    }
    /// The clock the readiness limit is measured on.
    fn delivery_clock(&self) -> Instant {
        Instant::now()
    }
    /// Ibis may post key events (macOS PostEvent, independent of AX trust;
    /// Windows requires no permission).
    fn may_post(&self) -> bool;
    /// Show where to give that permission; at most once per run.
    fn ask_to_post(&self);
    fn stage(&self, _stage: &'static str) {}
    /// Windows retains its existing restoration settle. macOS waits for the
    /// captured document/surface instead, without an unconditional sleep.
    fn settle(&self) {
        self.wait(SETTLE);
    }
    /// Restore only a proven target before waiting for it to come forward.
    fn prepare(&self) -> Result<(), CopyReason> {
        Ok(())
    }
    /// The app in front, or `None` once the picker has been shown again.
    /// A failed proof must keep its reason while waiting for that app.
    fn frontmost(&self) -> Result<Option<Front>, CopyReason>;
    fn ready(&self, request: &DeliveryRequest, _proof: &Self::Proof) -> Result<bool, CopyReason> {
        Ok(self
            .frontmost()?
            .is_some_and(|front| front.pid == request.pid && front.id.as_deref() == Some(&request.id)))
    }
    /// Finish with this platform's proof and final guards. On macOS all
    /// checks and posting run together in one main-thread task.
    fn finish(&self, request: DeliveryRequest, proof: &Self::Proof) -> PostResult;
    fn wait(&self, time: Duration);
}

/// Each system keeps immutable target evidence and lifetime anchors for the
/// shared policy. Windows starts its lifetime before clipboard I/O.
pub(super) trait CopyEvidence {
    fn policy_current(&self) -> bool;
    fn captured(&self) -> Option<&Captured>;
    fn written(&self) -> Instant;
    fn stamp(&self) -> SafetyStamp;

    fn check_age(&self, now: SafetyStamp) -> Result<(), SafetyFailure> {
        self.stamp().check(now)?;
        if Instant::now() >= self.written() + LATE {
            return Err(SafetyFailure::Expired);
        }
        Ok(())
    }
}

#[cfg(any(not(target_os = "windows"), test))]
impl CopyEvidence for CopyProof {
    fn policy_current(&self) -> bool {
        self.policy.current()
    }
    fn captured(&self) -> Option<&Captured> {
        self.captured.as_ref()
    }
    fn written(&self) -> Instant {
        self.written
    }
    fn stamp(&self) -> SafetyStamp {
        self.stamp
    }
}

#[cfg_attr(not(any(target_os = "macos", target_os = "windows", test)), allow(dead_code))]
pub(super) struct DeliveryRequest {
    pub(super) pid: i32,
    pub(super) id: String,
    #[cfg(any(target_os = "macos", test))]
    pub(super) began: Instant,
    pub(super) deadline: Instant,
}

/// Paste a proven copy into its captured app, or say why it stays copied.
pub(super) fn deliver<D: Desk>(desk: &D, proof: Option<&D::Proof>, enabled: bool) -> Delivered {
    #[cfg(any(target_os = "macos", test))]
    let began = desk.delivery_began();
    if !desk.release_keys() {
        return key_release_failed();
    }
    let unknown = Captured::unknown(SYSTEM);
    let captured = proof.and_then(CopyEvidence::captured).unwrap_or(&unknown);
    let permission = PastePermission {
        enabled,
        granted: enabled && desk.may_post(),
        app: captured.paste.unwrap_or(None),
    };
    let copied = |reason: Option<CopyReason>| Delivered::Copied {
        reason: reason
            .and_then(|reason| serde_json::to_value(reason).ok())
            .and_then(|code| code.as_str().map(str::to_owned)),
    };
    if proof.is_some_and(|proof| !proof.policy_current()) {
        return copied(Some(CopyReason::PasteUnavailable));
    }
    match delivery_decision(captured, permission) {
        Decision::Copy(reason @ (CopyReason::NoPermission | CopyReason::AccessibilityRequired)) => {
            desk.ask_to_post();
            copied(Some(reason))
        }
        Decision::Copy(reason) => copied(Some(reason)),
        Decision::Paste => {
            let Some(proof) = proof else {
                return copied(Some(CopyReason::NoTarget));
            };
            let deadline = proof.written() + LATE;
            if let Err(failure) = proof.check_age(desk.stamp()) {
                return copied(Some(failure.reason()));
            }
            if let Err(reason) = desk.prepare() {
                return copied(Some(reason));
            }
            let Some(pid) = captured.pid else {
                return copied(None);
            };
            // The same process and app: a process number alone can be reused.
            let id = captured.target.app.as_ref().map(|app| app.id.as_str());
            let started = desk.delivery_clock();
            let request = DeliveryRequest {
                pid,
                id: id.unwrap_or_default().to_owned(),
                #[cfg(any(target_os = "macos", test))]
                began,
                deadline: deadline.min(started + RETURN_LIMIT),
            };
            let mut checks = 0;
            loop {
                if let Err(failure) = proof.check_age(desk.stamp()) {
                    return copied(Some(failure.reason()));
                }
                desk.stage("readiness");
                let ready = match desk.ready(&request, proof) {
                    Ok(ready) => ready,
                    Err(reason) => return copied(Some(reason)),
                };
                if let Err(failure) = proof.check_age(desk.stamp()) {
                    return copied(Some(failure.reason()));
                }
                // A check that proved the target ready counts even when a busy
                // system made it finish after the limit; the final guards and
                // the paste's own deadline still decide. Only a target that is
                // not back in time has changed.
                if ready {
                    break;
                }
                if desk.delivery_clock().duration_since(started) >= RETURN_LIMIT {
                    desk.stage("readiness.timeout");
                    return copied(Some(CopyReason::TargetChanged));
                }
                if checks >= RETURN_CHECKS {
                    desk.stage("readiness.timeout");
                    return copied(Some(CopyReason::TargetChanged));
                }
                desk.wait(RETURN_STEP);
                checks += 1;
            }
            desk.settle();
            desk.stage("post.queue");
            let request = DeliveryRequest {
                pid,
                id: id.unwrap_or_default().to_owned(),
                #[cfg(any(target_os = "macos", test))]
                began,
                deadline,
            };
            match desk.finish(request, proof) {
                PostResult::Pasted => Delivered::Pasted {
                    app: captured.name.clone(),
                },
                PostResult::SecureInput => copied(Some(CopyReason::SecureInput)),
                PostResult::Unavailable => copied(Some(CopyReason::PasteUnavailable)),
                PostResult::Expired => copied(Some(CopyReason::PasteExpired)),
                PostResult::TargetChanged => copied(Some(CopyReason::TargetChanged)),
                PostResult::ClipboardChanged => copied(Some(CopyReason::ClipboardChanged)),
                PostResult::AccessibilityRequired => {
                    desk.ask_to_post();
                    copied(Some(CopyReason::AccessibilityRequired))
                }
                PostResult::NoPermission => {
                    desk.ask_to_post();
                    copied(Some(CopyReason::NoPermission))
                }
                #[cfg(any(target_os = "windows", test))]
                PostResult::SecureField => copied(Some(CopyReason::SecureField)),
                #[cfg(any(target_os = "windows", test))]
                PostResult::KeyReleaseFailed => key_release_failed(),
                PostResult::Refused => copied(None),
            }
        }
    }
}

pub(super) fn with_paste_settings(path: &std::path::Path, deliver: impl FnOnce(bool) -> Delivered) -> Delivered {
    let loaded = ibis_core::settings::load(path);
    if loaded.problem.is_some() {
        return Delivered::Copied {
            reason: Some("paste-unavailable".into()),
        };
    }
    deliver(loaded.settings.paste.unwrap_or(true))
}

pub(super) fn key_release_failed() -> Delivered {
    Delivered::Copied {
        reason: Some("key-release-failed".into()),
    }
}
