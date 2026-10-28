//! The scoped macOS native boundary (ADR 0029). objc2's AX bindings expose
//! unsafe calls too; reuse its locked Core Foundation ownership and checked
//! string downcasts without adding an unavailable binding package.

use std::ptr::{self, NonNull};
use std::time::Instant;

use objc2_core_foundation::{CFGetTypeID, CFRetained, CFString, CFType, CFTypeID};
use objc2_foundation::NSThread;

use super::super::security::{Accessibility, Attribute};

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn IsSecureEventInputEnabled() -> u8;
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> *const CFType;
    fn AXUIElementGetTypeID() -> CFTypeID;
    fn AXUIElementSetMessagingTimeout(element: &CFType, seconds: f32) -> i32;
    fn AXUIElementCopyAttributeValue(element: &CFType, attribute: &CFString, value: *mut *const CFType) -> i32;
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

/// Always a live, owned CF object whose AXUIElement type ID was checked.
pub(super) struct Element(CFRetained<CFType>);

impl Element {
    fn checked(value: CFRetained<CFType>) -> Option<Self> {
        // SAFETY: AXUIElementGetTypeID has no arguments or preconditions.
        let expected = unsafe { AXUIElementGetTypeID() };
        (CFGetTypeID(Some(&value)) == expected).then_some(Self(value))
    }
}

pub(super) struct Ax {
    pub(super) deadline: Instant,
}

impl Ax {
    /// Set a per-object timeout before every IPC call. Never alter the
    /// process-wide AX timeout, and never pass zero (which resets it).
    fn timeout(&self, element: &Element) -> Option<()> {
        let remaining = self.deadline.checked_duration_since(Instant::now())?;
        let seconds = remaining.as_secs_f32();
        if seconds <= 0.0 {
            return None;
        }
        // SAFETY: Element owns a type-checked AX object for the whole call;
        // the finite, positive timeout applies only to that object.
        (unsafe { AXUIElementSetMessagingTimeout(&element.0, seconds) } == 0).then_some(())
    }

    fn copy(&self, element: &Element, attribute: &str) -> Option<CFRetained<CFType>> {
        self.timeout(element)?;
        let attribute = CFString::from_str(attribute);
        let mut raw = ptr::null();
        // SAFETY: The AX element and CFString stay alive through this call.
        // `raw` is writable output storage, initialised to null. The Copy
        // function transfers a +1 CF reference when it supplies a value.
        let status = unsafe { AXUIElementCopyAttributeValue(&element.0, &attribute, &mut raw) };
        if status != 0 {
            return None;
        }
        let raw = NonNull::new(raw.cast_mut())?;
        // SAFETY: A successful non-null Copy result is an owned CF object.
        // Adopt it once, even on a late call, so every exit releases it.
        let value = unsafe { CFRetained::from_raw(raw) };
        (Instant::now() < self.deadline).then_some(value)
    }
}

impl Accessibility for Ax {
    type Element = Element;

    fn focused(&self, pid: i32) -> Option<Element> {
        if pid <= 0 || NSThread::isMainThread_class() || Instant::now() >= self.deadline {
            return None;
        }
        // SAFETY: This API accepts a process ID and returns a +1 AX object;
        // a vanished process will fail a later AX request, never prove safety.
        let raw = unsafe { AXUIElementCreateApplication(pid) };
        let raw = NonNull::new(raw.cast_mut())?;
        // SAFETY: The checked non-null Create result is an owned CF object.
        let app = Element::checked(unsafe { CFRetained::from_raw(raw) })?;
        Element::checked(self.copy(&app, "AXFocusedUIElement")?)
    }

    fn attribute(&self, element: &Element, attribute: Attribute) -> Option<String> {
        let value = self.copy(element, attribute.name())?;
        Some(value.downcast::<CFString>().ok()?.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_string_cannot_be_used_as_an_ax_element() {
        assert!(Element::checked(CFString::from_str("invented").into()).is_none());
    }

    #[test]
    fn carbon_is_never_queried_from_a_test_worker() {
        assert!(!NSThread::isMainThread_class());
        assert_eq!(enabled(), None);
    }
}
