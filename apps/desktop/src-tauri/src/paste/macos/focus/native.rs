//! Public SDK AX/CF signatures only. The closed attribute enum prevents content
//! reads; none of these objects is formatted, serialized, logged or sent.

use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::time::Instant;

use ibis_core::destination::CopyReason;
use objc2_core_foundation::{CFGetTypeID, CFRetained, CFString, CFType};
use objc2_foundation::NSThread;

use super::{Attribute, Identity, LIMIT, Provider, query};

// SAFETY: AXUIElement.h declares AXError=int32_t, pid_t=int and Boolean=u8.
// AXUIElementRef/CFTypeRef/CFStringRef are opaque CF pointers. Copy/Create
// results are +1 owned; GetPid writes only to the supplied local output.
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXUIElementCreateSystemWide() -> *const c_void;
    fn AXUIElementGetTypeID() -> usize;
    fn AXUIElementGetPid(element: *const c_void, pid: *mut i32) -> i32;
    fn AXUIElementCopyAttributeValue(
        element: *const c_void,
        attribute: *const c_void,
        value: *mut *const c_void,
    ) -> i32;
    fn AXUIElementSetMessagingTimeout(element: *const c_void, seconds: f32) -> i32;
}

#[derive(Clone)]
struct Element(CFRetained<CFType>, bool);

impl Element {
    fn ptr(&self) -> *const c_void {
        CFRetained::as_ptr(&self.0).as_ptr().cast()
    }

    /// Only passed +1 Copy/Create results, never borrowed provider pointers.
    fn owned(value: *const c_void) -> Option<CFRetained<CFType>> {
        let pointer = NonNull::new(value.cast_mut().cast::<CFType>())?;
        // SAFETY: The SDK Copy/Create result is a non-null +1 CF object.
        Some(unsafe { CFRetained::from_raw(pointer) })
    }
}

impl Drop for Element {
    fn drop(&mut self) {
        if self.1 {
            // SAFETY: This is the live system object. SDK timeout=0 restores
            // the process default after this bounded operation, on all exits.
            unsafe { AXUIElementSetMessagingTimeout(self.ptr(), 0.0) };
        }
    }
}

impl PartialEq for Element {
    fn eq(&self, other: &Self) -> bool {
        // The safe binding uses CFEqual, never CFCopyDescription/Debug.
        self.0 == other.0
    }
}

struct Native;

impl Provider for Native {
    type Element = Element;
    fn now(&self) -> Instant {
        Instant::now()
    }
    fn elapsed(&self) -> std::time::Duration {
        super::super::safety::elapsed()
    }
    fn trusted(&self) -> bool {
        // SAFETY: No-argument preflight only; never uses WithOptions/prompt.
        unsafe { AXIsProcessTrusted() != 0 }
    }
    fn system(&self) -> Result<Element, CopyReason> {
        // SAFETY: Create returns a +1 system-wide AX object, checked non-null.
        Element::owned(unsafe { AXUIElementCreateSystemWide() })
            .map(|value| Element(value, true))
            .ok_or(CopyReason::PasteUnavailable)
    }
    fn read(&self, element: &Element, attribute: Attribute, deadline: Instant) -> Result<Element, CopyReason> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(CopyReason::PasteUnavailable);
        }
        // SAFETY: Retained AX object and strictly positive finite timeout.
        // On a system object the SDK sets this process's default timeout;
        // every subsequent queried object also gets its remaining budget.
        if unsafe { AXUIElementSetMessagingTimeout(element.ptr(), remaining.as_secs_f32()) } != 0 {
            return Err(CopyReason::PasteUnavailable);
        }
        let name = match attribute {
            Attribute::Application => "AXFocusedApplication",
            Attribute::FocusedWindow => "AXFocusedWindow",
            Attribute::FocusedElement => "AXFocusedUIElement",
            Attribute::Window => "AXWindow",
            Attribute::TopLevel => "AXTopLevelUIElement",
        };
        let name = CFString::from_static_str(name);
        let mut value = std::ptr::null();
        // SAFETY: Both inputs live through the call and the initialized local
        // receives the SDK's retained result. No mutable provider attributes.
        let error = unsafe {
            AXUIElementCopyAttributeValue(element.ptr(), CFRetained::as_ptr(&name).as_ptr().cast(), &mut value)
        };
        let owned = Element::owned(value);
        if error != 0 {
            // Missing/invalid attributes and failed messaging are unavailable
            // evidence, not proof that the captured app/window changed.
            return Err(if trusted() {
                CopyReason::PasteUnavailable
            } else {
                CopyReason::AccessibilityRequired
            });
        }
        let value = owned.ok_or(CopyReason::PasteUnavailable)?;
        // SAFETY: A non-null CF object; reject a malformed attribute type
        // before using the result as an AXUIElement.
        if CFGetTypeID(Some(&value)) != unsafe { AXUIElementGetTypeID() } {
            return Err(CopyReason::PasteUnavailable);
        }
        Ok(Element(value, false))
    }
    fn pid(&self, element: &Element) -> Result<i32, CopyReason> {
        let mut pid = 0;
        // SAFETY: Retained, type-checked AX object and valid local pid output.
        if unsafe { AXUIElementGetPid(element.ptr(), &mut pid) } != 0 || pid <= 0 {
            Err(CopyReason::PasteUnavailable)
        } else {
            Ok(pid)
        }
    }
}

#[derive(Default)]
struct Saved {
    next: u32,
    window: Option<(u32, Identity<Element>)>,
}

thread_local! { static SAVED: RefCell<Saved> = RefCell::default(); }

pub(super) fn trusted() -> bool {
    Native.trusted()
}

pub(super) fn capture(pid: i32) -> Result<u32, CopyReason> {
    if !NSThread::isMainThread_class() {
        return Err(CopyReason::PasteUnavailable);
    }
    // Reserve identity before IPC. Reentrant/newer capture cannot be
    // overwritten by this query when it eventually completes.
    let token = SAVED.with(|saved| {
        let mut saved = saved.borrow_mut();
        saved.window = None;
        saved.next = saved.next.checked_add(1).ok_or(CopyReason::PasteUnavailable)?;
        Ok(saved.next)
    })?;
    let window = query(&Native, pid, Instant::now() + LIMIT, || Ok(()));
    SAVED.with(|saved| {
        let mut saved = saved.borrow_mut();
        if saved.next != token {
            return Err(CopyReason::PasteUnavailable);
        }
        let window = window?;
        saved.window = Some((token, window));
        Ok(token)
    })
}

pub(super) fn current(
    pid: i32,
    deadline: Instant,
    before_final_read: impl FnOnce() -> Result<(), CopyReason>,
    validate: impl FnOnce() -> Result<(), CopyReason>,
) -> Result<Option<u32>, CopyReason> {
    if !NSThread::isMainThread_class() {
        return Err(CopyReason::PasteUnavailable);
    }
    let (token, captured) = SAVED.with(|saved| {
        let saved = saved.borrow();
        let Some((token, captured)) = &saved.window else {
            return Err(CopyReason::PasteUnavailable);
        };
        Ok((*token, captured.clone()))
    })?;
    // No RefCell borrow across provider IPC: a newer/reentrant capture retires
    // this identity and is checked again after the bounded read.
    let current = query(&Native, pid, deadline.min(Instant::now() + LIMIT), before_final_read);
    validate()?;
    let current = current?;
    let unchanged = SAVED.with(|saved| saved.borrow().window.as_ref().is_some_and(|(now, _)| *now == token));
    Ok((unchanged && current == captured).then_some(token))
}
