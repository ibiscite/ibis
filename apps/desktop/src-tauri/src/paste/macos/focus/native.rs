//! Public SDK AX/CF signatures only. The closed attribute enum prevents content
//! reads; none of these objects is formatted, serialized, logged or sent.

use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::time::Instant;

use ibis_core::destination::CopyReason;
use objc2_app_kit::NSRunningApplication;
use objc2_core_foundation::{CFBoolean, CFDictionary, CFGetTypeID, CFRetained, CFString, CFType};
use objc2_foundation::NSThread;
use tauri_nspanel::objc2::rc::Retained;

use super::{
    Attribute, Identity, LIMIT, Provider, capture_query, claim_process, diagnostics, missing_identity_attribute, query,
    same_live_process,
};

// SAFETY: AXUIElement.h declares AXError=int32_t, pid_t=int and Boolean=u8.
// AXUIElementRef/CFTypeRef/CFStringRef are opaque CF pointers. Copy/Create
// results are +1 owned; GetPid writes only to the supplied local output.
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> u8;
    static kAXTrustedCheckOptionPrompt: *const CFString;
    fn AXUIElementCreateSystemWide() -> *const c_void;
    fn AXUIElementGetTypeID() -> usize;
    fn AXUIElementGetPid(element: *const c_void, pid: *mut i32) -> i32;
    fn AXUIElementCopyAttributeValue(
        element: *const c_void,
        attribute: *const c_void,
        value: *mut *const c_void,
    ) -> i32;
    fn AXUIElementSetMessagingTimeout(element: *const c_void, seconds: f32) -> i32;
    fn AXUIElementSetAttributeValue(element: *const c_void, attribute: *const c_void, value: *const c_void) -> i32;
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

#[derive(Default)]
struct Native {
    application: RefCell<Option<Element>>,
    missing_identity: RefCell<Option<i32>>,
    activation_process: Option<Retained<NSRunningApplication>>,
}

impl Native {
    fn check_activation_process(&self, pid: i32) -> Result<&Retained<NSRunningApplication>, CopyReason> {
        let original = self.activation_process.as_ref().ok_or(CopyReason::PasteUnavailable)?;
        let current = NSRunningApplication::runningApplicationWithProcessIdentifier(pid);
        let terminated = original.isTerminated() || current.as_ref().is_some_and(|process| process.isTerminated());
        if original.processIdentifier() != pid || !same_live_process(original.as_ref(), current.as_deref(), terminated)
        {
            diagnostics::owner(false);
            return Err(CopyReason::TargetChanged);
        }
        Ok(original)
    }
}

// NSRunningApplication equality identifies a live instance, unlike a reusable
// PID. Retain the instance even if the write fails; never retry it in a loop.
thread_local! {
    static MANUAL_PROCESSES: RefCell<Vec<Retained<NSRunningApplication>>> = const { RefCell::new(Vec::new()) };
}

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
        let started = self.elapsed();
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(CopyReason::PasteUnavailable);
        }
        // SAFETY: Retained AX object and strictly positive finite timeout.
        // On a system object the SDK sets this process's default timeout;
        // every subsequent queried object also gets its remaining budget.
        let error = unsafe { AXUIElementSetMessagingTimeout(element.ptr(), remaining.as_secs_f32()) };
        if error != 0 {
            diagnostics::stage("ax.messaging_timeout");
            diagnostics::ax(error, self.elapsed().saturating_sub(started));
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
        // receives the SDK's retained result. Only the closed identity allowlist.
        let error = unsafe {
            AXUIElementCopyAttributeValue(element.ptr(), CFRetained::as_ptr(&name).as_ptr().cast(), &mut value)
        };
        diagnostics::ax(error, self.elapsed().saturating_sub(started));
        let owned = Element::owned(value);
        if error != 0 {
            // The only retryable absence is an unsupported/no-value document
            // or surface relation. Messaging failures and malformed data never
            // opt a process in. No attribute enumeration or content inspection.
            if missing_identity_attribute(attribute, error) {
                self.missing_identity.replace(self.pid(element).ok());
            }
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
            diagnostics::stage("ax.invalid_type");
            return Err(CopyReason::PasteUnavailable);
        }
        let element = Element(value, false);
        if attribute == Attribute::Application {
            self.application.replace(Some(element.clone()));
        }
        Ok(element)
    }
    fn pid(&self, element: &Element) -> Result<i32, CopyReason> {
        let started = self.elapsed();
        let mut pid = 0;
        // SAFETY: Retained, type-checked AX object and valid local pid output.
        let error = unsafe { AXUIElementGetPid(element.ptr(), &mut pid) };
        diagnostics::ax(error, self.elapsed().saturating_sub(started));
        if error != 0 || pid <= 0 {
            diagnostics::stage("ax.owner");
            Err(CopyReason::PasteUnavailable)
        } else {
            Ok(pid)
        }
    }

    fn enable_manual_accessibility(&self, pid: i32, deadline: Instant) -> Result<bool, CopyReason> {
        let Some(missing_owner) = *self.missing_identity.borrow() else {
            return Ok(false);
        };
        let Some(application) = self.application.borrow().clone() else {
            return Ok(false);
        };
        let matches = self.pid(&application)? == pid && missing_owner == pid;
        diagnostics::owner(matches);
        if !matches {
            return Err(CopyReason::TargetChanged);
        }
        // A TopLevel absence came from the focused control; require its owner
        // too, rather than letting a foreign nonactivating panel enable this app.
        if !self.trusted() {
            return Err(CopyReason::AccessibilityRequired);
        }
        let process = self.check_activation_process(pid)?;
        let claimed = MANUAL_PROCESSES.with(|processes| {
            let mut processes = processes.borrow_mut();
            processes.retain(|process| !process.isTerminated());
            claim_process(&mut processes, process.clone())
        });
        if !claimed {
            return Ok(false);
        }
        let started = self.elapsed();
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(CopyReason::PasteUnavailable);
        }
        diagnostics::stage("ax.manual_accessibility");
        // SAFETY: Live application element obtained only from AXFocusedApplication,
        // verified against the allowed target process; bounded remaining timeout.
        let error = unsafe { AXUIElementSetMessagingTimeout(application.ptr(), remaining.as_secs_f32()) };
        diagnostics::ax(error, self.elapsed().saturating_sub(started));
        if error != 0 {
            return Err(CopyReason::PasteUnavailable);
        }
        let name = CFString::from_static_str("AXManualAccessibility");
        // The timeout preparation was another native call. Recheck the instance
        // retained before eligibility immediately before the sole foreign write.
        self.check_activation_process(pid)?;
        if Instant::now() >= deadline {
            return Err(CopyReason::PasteUnavailable);
        }
        diagnostics::manual_accessibility();
        // SAFETY: This is the sole foreign attribute write: a CFBoolean true on
        // the verified application element, once per live process and only after
        // paste eligibility and missing identity attributes. No content is read.
        let error = unsafe {
            AXUIElementSetAttributeValue(
                application.ptr(),
                CFRetained::as_ptr(&name).as_ptr().cast(),
                (CFBoolean::new(true) as *const CFBoolean).cast(),
            )
        };
        diagnostics::ax(error, self.elapsed().saturating_sub(started));
        if error != 0 {
            return Err(CopyReason::PasteUnavailable);
        }
        Ok(true)
    }
}

#[derive(Default)]
struct Saved {
    next: u32,
    window: Option<(u32, Identity<Element>)>,
}

thread_local! { static SAVED: RefCell<Saved> = RefCell::default(); }

pub(super) fn trusted() -> bool {
    Native::default().trusted()
}

/// Called only by the existing guide's explicit Open System Settings action.
pub(super) fn request_trust() {
    // SAFETY: SDK-exported, process-lifetime CFString key. This is the only
    // prompting call; passive capture/delivery never use WithOptions.
    let Some(key) = (unsafe { kAXTrustedCheckOptionPrompt.as_ref() }) else {
        return;
    };
    let options = CFDictionary::from_slices(&[key], &[CFBoolean::new(true)]);
    // SAFETY: Typed retained dictionary with the SDK prompt key and Boolean true.
    unsafe { AXIsProcessTrustedWithOptions(CFRetained::as_ptr(&options).as_ptr().cast()) };
}

pub(super) fn capture(pid: i32, eligible: Option<Retained<NSRunningApplication>>) -> Result<u32, CopyReason> {
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
    let may_activate = eligible.is_some();
    let provider = Native {
        activation_process: eligible,
        ..Native::default()
    };
    let window = capture_query(&provider, pid, may_activate);
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
    expected: Option<u32>,
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
        if Some(*token) != expected {
            return Err(CopyReason::TargetChanged);
        }
        Ok((*token, captured.clone()))
    })?;
    // No RefCell borrow across provider IPC: a newer/reentrant capture retires
    // this identity and is checked again after the bounded read.
    let current = query(
        &Native::default(),
        pid,
        deadline.min(Instant::now() + LIMIT),
        before_final_read,
    );
    validate()?;
    let current = current?;
    let unchanged = SAVED.with(|saved| saved.borrow().window.as_ref().is_some_and(|(now, _)| *now == token));
    let document_equal = current.window == captured.window;
    let surface_equal = current.surface == captured.surface;
    diagnostics::stage("focus.compare");
    diagnostics::equality(Some(document_equal), Some(surface_equal), None);
    Ok((unchanged && document_equal && surface_equal).then_some(token))
}
