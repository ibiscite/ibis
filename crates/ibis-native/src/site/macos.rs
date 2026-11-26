//! The macOS read: the address of the page in the browser's captured window,
//! through the Accessibility grant alone. Public SDK AX/CF calls with a
//! closed attribute set; the only text read is the window's `AXDocument` or
//! the web area's `AXURL`, which goes straight back to the site reduction. Nothing here prompts,
//! sets an attribute or sends an Apple Event, and the walk never enters a
//! page, so no page content is read.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::time::Instant;

use ibis_core::destination::{AppId, System};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSWorkspace};
use objc2_core_foundation::{CFArray, CFGetTypeID, CFRetained, CFString, CFType, CFURL};

// SAFETY: AXUIElement.h declares AXError=int32_t, pid_t=int and Boolean=u8.
// AXUIElementRef/CFTypeRef/CFStringRef are opaque CF pointers. Copy/Create
// results are +1 owned.
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXUIElementGetTypeID() -> usize;
    fn AXUIElementCopyAttributeValue(
        element: *const c_void,
        attribute: *const c_void,
        value: *mut *const c_void,
    ) -> i32;
    fn AXUIElementSetMessagingTimeout(element: *const c_void, seconds: f32) -> i32;
}

/// Elements the walk may look at before giving up: a browser window's
/// chrome is a few dozen; toolbars and pages are never entered.
const MAX_ELEMENTS: usize = 200;
const MAX_DEPTH: usize = 16;

/// The only attributes the read may name.
#[derive(Clone, Copy)]
enum Attribute {
    Document,
    Children,
    Role,
    Url,
}

#[derive(Clone)]
pub(super) struct Element(CFRetained<CFType>);

impl Element {
    /// Only passed +1 Copy/Create results.
    fn owned(value: *const c_void) -> Option<CFRetained<CFType>> {
        let pointer = NonNull::new(value.cast_mut().cast::<CFType>())?;
        // SAFETY: The SDK Copy/Create result is a non-null +1 CF object.
        Some(unsafe { CFRetained::from_raw(pointer) })
    }

    fn ptr(&self) -> *const c_void {
        CFRetained::as_ptr(&self.0).as_ptr().cast()
    }

    /// An AX element, or nothing for any other CF type.
    fn of(value: CFRetained<CFType>) -> Option<Self> {
        // SAFETY: A type ID query with no arguments.
        (CFGetTypeID(Some(&value)) == unsafe { AXUIElementGetTypeID() }).then_some(Self(value))
    }

    fn copy(&self, attribute: Attribute, deadline: Instant) -> Option<CFRetained<CFType>> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return None;
        }
        // SAFETY: Retained AX object and strictly positive finite timeout.
        if unsafe { AXUIElementSetMessagingTimeout(self.ptr(), remaining.as_secs_f32()) } != 0 {
            return None;
        }
        let name = CFString::from_static_str(match attribute {
            Attribute::Document => "AXDocument",
            Attribute::Children => "AXChildren",
            Attribute::Role => "AXRole",
            Attribute::Url => "AXURL",
        });
        let mut value = std::ptr::null();
        // SAFETY: Both inputs live through the call and the initialized local
        // receives the SDK's retained result. Only the closed set above.
        let error =
            unsafe { AXUIElementCopyAttributeValue(self.ptr(), CFRetained::as_ptr(&name).as_ptr().cast(), &mut value) };
        let owned = Self::owned(value);
        (error == 0).then_some(owned).flatten()
    }

    fn children(&self, deadline: Instant) -> Vec<Self> {
        let Some(array) = self
            .copy(Attribute::Children, deadline)
            .and_then(|value| value.downcast::<CFArray>().ok())
        else {
            return Vec::new();
        };
        // SAFETY: AXChildren is documented as an array of AXUIElements; each
        // one is still checked by type before use.
        let array = unsafe { array.cast_unchecked::<CFType>() };
        (0..array.len().min(MAX_ELEMENTS))
            .filter_map(|index| array.get(index))
            .filter_map(Self::of)
            .collect()
    }

    fn role(&self, deadline: Instant) -> Option<String> {
        self.copy(Attribute::Role, deadline)
            .and_then(|value| value.downcast::<CFString>().ok())
            .map(|role| role.to_string())
    }

    /// An address attribute, which apps give as a URL or as its text.
    fn address(&self, attribute: Attribute, deadline: Instant) -> Option<String> {
        let value = self.copy(attribute, deadline)?;
        match value.downcast::<CFURL>() {
            Ok(url) => Some(url.string().to_string()),
            Err(value) => value.downcast::<CFString>().ok().map(|text| text.to_string()),
        }
    }
}

pub(super) struct Native;

impl super::Source for Native {
    fn front(&self) -> Option<(i32, AppId)> {
        // The picker panel does not activate Ibis: with one of its windows
        // key, the paste capture keeps the earlier app, and so does this read.
        let main = MainThreadMarker::new()?;
        if NSApplication::sharedApplication(main)
            .keyWindow()
            .is_some_and(|window| window.isKeyWindow())
        {
            let own = i32::try_from(std::process::id()).ok()?;
            return Some((
                own,
                AppId {
                    system: System::Macos,
                    id: String::new(),
                },
            ));
        }
        let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
        let id = app.bundleIdentifier()?.to_string();
        Some((
            app.processIdentifier(),
            AppId {
                system: System::Macos,
                id,
            },
        ))
    }

    fn allowed(&self) -> bool {
        // SAFETY: No-argument preflight only; never uses WithOptions/prompt.
        unsafe { AXIsProcessTrusted() != 0 }
    }

    fn address(&self, captured: super::CapturedWindow, pid: i32, deadline: Instant) -> Option<String> {
        let super::CapturedWindow::Macos(token) = captured else {
            return None;
        };
        let window = Element(crate::paste::macos::captured_window(token, pid)?);
        // A window may name the page it shows, as a document window names its file.
        if let Some(address) = window.address(Attribute::Document, deadline) {
            return Some(address);
        }
        // Breadth first through the window's own controls, to the first web
        // area, whose address is read; nothing inside it is.
        let mut queue = VecDeque::from([(window.clone(), 0)]);
        let mut seen = 0;
        while let Some((element, depth)) = queue.pop_front() {
            seen += 1;
            if seen > MAX_ELEMENTS || Instant::now() >= deadline {
                return None;
            }
            match element.role(deadline).as_deref() {
                Some("AXWebArea") => return element.address(Attribute::Url, deadline),
                // Only containers can hold the page; buttons, fields, lists
                // and toolbars are never entered.
                Some("AXWindow" | "AXGroup" | "AXSplitGroup" | "AXTabGroup" | "AXScrollArea" | "AXLayoutArea")
                    if depth < MAX_DEPTH =>
                {
                    queue.extend(element.children(deadline).into_iter().map(|child| (child, depth + 1)));
                }
                _ => {}
            }
        }
        None
    }
}
