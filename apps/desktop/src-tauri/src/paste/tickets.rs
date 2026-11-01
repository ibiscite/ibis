#[cfg(any(target_os = "windows", test))]
use std::time::{Duration, Instant};

#[cfg(any(target_os = "windows", test))]
use ibis_core::destination::CopyReason;

#[cfg(any(target_os = "windows", test))]
use super::capture::Front;
#[cfg(any(target_os = "windows", test))]
use super::{
    CopyEvidence, Delivered, DeliveryRequest, DeliveryTicket, Desk, PostResult, SafetyFailure, SafetyStamp, deliver,
};

/// Windows checks that can be exercised with fake HWNDs on every host.
#[cfg(any(target_os = "windows", test))]
pub(super) trait TicketDesk {
    type Window;
    fn stamp(&self) -> SafetyStamp {
        SafetyStamp::now()
    }
    fn release_keys(&self) -> bool;
    fn keys_unreleased(&self) -> bool;
    fn may_post(&self) -> bool;
    fn ask_to_post(&self);
    fn frontmost(&self) -> Option<Front>;
    /// UI Automation checks the focused field in the target process. Failure
    /// or timeout is unproven, not evidence of a password field.
    fn secure_field(&self, pid: i32) -> Option<bool>;
    fn paste_key(&self) -> Option<u16>;
    fn wait(&self, time: Duration);
    fn generation(&self) -> u64;
    /// Also called under destination and injection locks. Must use native
    /// state, with no dispatch to or wait on the UI thread.
    fn picker_hidden(&self) -> bool;
    fn restore(&self, window: &Self::Window, pid: i32) -> bool;
    fn window_in_front(&self, window: &Self::Window) -> bool;
    fn clipboard_sequence(&self) -> Option<u32>;
    /// Check the ticket after any injection lock/recovery wait, immediately
    /// before sending fresh key-downs. Releasing old keys needs no ticket.
    fn post_if(&self, pid: i32, key: u16, ready: impl Fn() -> bool) -> bool;
}

#[cfg(any(target_os = "windows", test))]
struct TicketDeskGuard<'a, D: TicketDesk> {
    desk: &'a D,
    ticket: &'a DeliveryTicket<D::Window>,
}

#[cfg(any(target_os = "windows", test))]
impl<D: TicketDesk> TicketDeskGuard<'_, D> {
    fn current(&self) -> Result<(), CopyReason> {
        if !self.ticket.policy.current() {
            return Err(CopyReason::PasteUnavailable);
        }
        // A stale presentation is distinct from replaced or unreadable clipboard evidence.
        if self.desk.generation() != self.ticket.target.generation.wrapping_add(1) || !self.desk.picker_hidden() {
            return Err(CopyReason::TargetChanged);
        }
        match self.desk.clipboard_sequence() {
            None => Err(CopyReason::PasteUnavailable),
            Some(sequence) if sequence != self.ticket.sequence => Err(CopyReason::ClipboardChanged),
            Some(_) => Ok(()),
        }
    }

    fn in_front(&self) -> bool {
        self.ticket
            .target
            .window
            .as_ref()
            .is_some_and(|window| self.desk.window_in_front(window))
    }

    fn target_in_front(&self, request: &DeliveryRequest) -> Result<(), CopyReason> {
        self.current()?;
        if self
            .desk
            .frontmost()
            .is_some_and(|front| front.pid == request.pid && front.id.as_deref() == Some(&request.id))
            && self.in_front()
        {
            Ok(())
        } else {
            Err(CopyReason::TargetChanged)
        }
    }

    fn checked(&self, request: &DeliveryRequest) -> Result<(), PostResult> {
        self.ticket
            .check_age(self.desk.stamp())
            .map_err(SafetyFailure::result)?;
        if Instant::now() >= request.deadline {
            return Err(PostResult::Expired);
        }
        self.target_in_front(request).map_err(|reason| match reason {
            CopyReason::TargetChanged => PostResult::TargetChanged,
            CopyReason::ClipboardChanged => PostResult::ClipboardChanged,
            _ => PostResult::Unavailable,
        })?;
        // Windows target/provider queries can wait. Recheck the immutable
        // pre-I/O deadline after them; SendInput has its own final native guard.
        self.ticket
            .check_age(self.desk.stamp())
            .map_err(SafetyFailure::result)?;
        if Instant::now() >= request.deadline {
            return Err(PostResult::Expired);
        }
        Ok(())
    }
}

#[cfg(any(target_os = "windows", test))]
impl<D: TicketDesk> Desk for TicketDeskGuard<'_, D> {
    type Proof = DeliveryTicket<D::Window>;

    fn stamp(&self) -> SafetyStamp {
        self.desk.stamp()
    }
    fn release_keys(&self) -> bool {
        self.desk.release_keys()
    }
    fn may_post(&self) -> bool {
        self.desk.may_post()
    }
    fn ask_to_post(&self) {
        self.desk.ask_to_post();
    }

    fn prepare(&self) -> Result<(), CopyReason> {
        self.current()?;
        if self
            .ticket
            .target
            .window
            .as_ref()
            .zip(self.ticket.target.captured.as_ref().and_then(|c| c.pid))
            .is_some_and(|(window, pid)| self.desk.restore(window, pid))
        {
            Ok(())
        } else {
            Err(CopyReason::TargetChanged)
        }
    }

    fn frontmost(&self) -> Result<Option<Front>, CopyReason> {
        self.current()?;
        Ok(self.in_front().then(|| self.desk.frontmost()).flatten())
    }

    fn finish(&self, request: DeliveryRequest, _: &Self::Proof) -> PostResult {
        if let Err(result) = self.checked(&request) {
            return result;
        }
        let secure = self.desk.secure_field(request.pid);
        if let Err(result) = self.checked(&request) {
            return result;
        }
        match secure {
            Some(true) => return PostResult::SecureField,
            None => return PostResult::Refused,
            Some(false) => {}
        }
        let Some(key) = self.desk.paste_key() else {
            return PostResult::Refused;
        };
        // Remember the failure that actually prevented injection, rather than
        // re-querying mutable evidence and inventing a different refusal reason.
        let refused = std::cell::Cell::new(None);
        if self.desk.post_if(request.pid, key, || match self.checked(&request) {
            Ok(()) => true,
            Err(result) => {
                refused.set(Some(result));
                false
            }
        }) {
            PostResult::Pasted
        } else if self.desk.keys_unreleased() {
            PostResult::KeyReleaseFailed
        } else {
            refused.get().unwrap_or(PostResult::Refused)
        }
    }

    fn wait(&self, time: Duration) {
        self.desk.wait(time);
    }
}

#[cfg(any(target_os = "windows", test))]
pub(super) fn deliver_ticket<D: TicketDesk>(
    desk: &D,
    ticket: &DeliveryTicket<D::Window>,
    _text: &str,
    enabled: bool,
) -> Delivered {
    deliver(&TicketDeskGuard { desk, ticket }, Some(ticket), enabled)
}
