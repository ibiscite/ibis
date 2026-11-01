#[cfg(any(target_os = "windows", test))]
use std::time::Duration;

#[cfg(any(target_os = "windows", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PasteEvent {
    ControlDown,
    VDown,
    VUp,
    ControlUp,
}

/// Keys left down by Ibis's own injection. Windows keeps one process-wide
/// instance under a mutex, serializing injections and retaining failed key-ups.
#[cfg(any(target_os = "windows", test))]
#[derive(Default)]
pub(super) struct PasteKeys {
    pub(super) control: bool,
    pub(super) v: bool,
}

#[cfg(any(target_os = "windows", test))]
impl PasteKeys {
    pub(super) fn pending(&self) -> bool {
        self.control || self.v
    }

    pub(super) fn release(
        &mut self,
        send: impl Fn(&[PasteEvent]) -> u32,
        wait: impl Fn(Duration),
        warn: impl Fn(),
    ) -> bool {
        for attempt in 0..3 {
            if self.v && send(&[PasteEvent::VUp]) == 1 {
                self.v = false;
            }
            if self.control && send(&[PasteEvent::ControlUp]) == 1 {
                self.control = false;
            }
            if !self.pending() {
                return true;
            }
            if attempt < 2 {
                wait(Duration::from_millis(10));
            }
        }
        warn();
        false
    }

    pub(super) fn paste(
        &mut self,
        send: impl Fn(&[PasteEvent]) -> u32,
        wait: impl Fn(Duration),
        warn: impl Fn(),
        ready: impl Fn() -> bool,
    ) -> bool {
        if !self.release(&send, &wait, &warn) || !ready() {
            return false;
        }
        use PasteEvent::*;
        match send(&[ControlDown, VDown, VUp, ControlUp]) {
            4 => true,
            count @ 1..=3 => {
                self.control = true;
                self.v = count == 2;
                self.release(send, wait, warn);
                false
            }
            _ => false,
        }
    }
}
