//! Geometry only: no window titles, document values, text or permission prompts.
//! SDK-only calls without safe bindings are confined to this module (0029).
#![allow(unsafe_code)]

use std::{
    ffi::c_void,
    ptr::NonNull,
    time::{Duration, Instant},
};

use objc2_app_kit::{NSScreen, NSWorkspace};
use objc2_core_foundation::{CFGetTypeID, CFRetained, CFString, CFType, CFUUID};
use objc2_core_graphics::CGDisplayBounds;
use objc2_foundation::{NSEdgeInsets, NSNumber, NSObjectProtocol, NSRect, NSString};
use tauri::Monitor;
use tauri_nspanel::{
    NSPoint, NSSize,
    objc2::{MainThreadMarker, rc::Retained, sel},
};

use super::Area;

// SAFETY: Public ColorSync/AX SDK signatures. Copy/Create return +1 CF objects;
// AXValue kinds are CGPoint=1 and CGSize=2. AXError=int32_t, Boolean=u8.
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn CGDisplayCreateUUIDFromDisplayID(display: u32) -> *mut CFUUID;
    fn AXIsProcessTrusted() -> u8;
    fn AXUIElementCreateApplication(pid: i32) -> *mut CFType;
    fn AXUIElementGetTypeID() -> usize;
    fn AXUIElementSetMessagingTimeout(element: *const c_void, seconds: f32) -> i32;
    fn AXUIElementCopyAttributeValue(
        element: *const c_void,
        attribute: *const c_void,
        output: *mut *const c_void,
    ) -> i32;
    fn AXValueGetTypeID() -> usize;
    fn AXValueGetType(value: *const c_void) -> u32;
    fn AXValueGetValue(value: *const c_void, kind: u32, output: *mut c_void) -> u8;
}

fn screen(monitor: &Monitor) -> Option<(Retained<NSScreen>, u32)> {
    let mtm = MainThreadMarker::new()?;
    let scale = monitor.scale_factor();
    let at = monitor.position();
    for screen in NSScreen::screens(mtm) {
        let number = screen
            .deviceDescription()
            .objectForKey(&NSString::from_str("NSScreenNumber"))?
            .downcast::<NSNumber>()
            .ok()?
            .unsignedIntValue();
        let bounds = CGDisplayBounds(number);
        if (bounds.origin.x - f64::from(at.x) / scale).abs() <= 1.0
            && (bounds.origin.y - f64::from(at.y) / scale).abs() <= 1.0
        {
            return Some((screen, number));
        }
    }
    None
}

pub(super) fn display_key(monitor: &Monitor) -> Option<String> {
    let (_, id) = screen(monitor)?;
    // SAFETY: A live display ID from NSScreen; adopt the non-null +1 UUID.
    let uuid = unsafe { CFRetained::from_raw(NonNull::new(CGDisplayCreateUUIDFromDisplayID(id))?) };
    Some(format!("macos:{}", CFUUID::new_string(None, Some(&uuid))?))
}

pub(super) fn usable(monitor: &Monitor) -> Option<Area> {
    let (screen, id) = screen(monitor)?;
    let bounds = CGDisplayBounds(id);
    let frame = screen.frame();
    let visible = screen.visibleFrame();
    Some(usable_area(
        bounds,
        frame,
        visible,
        screen.respondsToSelector(sel!(safeAreaInsets)),
        || screen.safeAreaInsets(),
    ))
}

fn usable_area(
    bounds: NSRect,
    frame: NSRect,
    visible: NSRect,
    has_safe_area: bool,
    insets: impl FnOnce() -> NSEdgeInsets,
) -> Area {
    // NSScreen.safeAreaInsets was added in macOS 12; older supported systems
    // still provide visibleFrame, with no additional safe-area restriction.
    let safe = if has_safe_area {
        insets()
    } else {
        NSEdgeInsets {
            top: 0.0,
            left: 0.0,
            bottom: 0.0,
            right: 0.0,
        }
    };
    let visible = Area {
        x: bounds.origin.x + visible.origin.x - frame.origin.x,
        y: bounds.origin.y + frame.origin.y + frame.size.height - visible.origin.y - visible.size.height,
        width: visible.size.width,
        height: visible.size.height,
    };
    visible.intersection(Area {
        x: bounds.origin.x + safe.left,
        y: bounds.origin.y + safe.top,
        width: frame.size.width - safe.left - safe.right,
        height: frame.size.height - safe.top - safe.bottom,
    })
}

fn read(element: &CFType, attribute: &'static str, deadline: Instant) -> Option<CFRetained<CFType>> {
    let remaining = deadline.checked_duration_since(Instant::now())?;
    let mut value = std::ptr::null();
    let name = CFString::from_static_str(attribute);
    let pointer = (element as *const CFType).cast();
    // SAFETY: Live retained AX element, finite positive timeout and a local
    // initialized out pointer. Only the three fixed geometry attributes below.
    let error = unsafe {
        if AXUIElementSetMessagingTimeout(pointer, remaining.as_secs_f32()) != 0 {
            return None;
        }
        AXUIElementCopyAttributeValue(pointer, CFRetained::as_ptr(&name).as_ptr().cast(), &mut value)
    };
    // SAFETY: Copy result is +1 when non-null, including any error result.
    let owned = NonNull::new(value.cast_mut().cast::<CFType>()).map(|value| unsafe { CFRetained::from_raw(value) });
    (error == 0).then_some(owned).flatten()
}

pub(super) fn focused_window() -> Option<Area> {
    let _mtm = MainThreadMarker::new()?;
    // SAFETY: Passive no-argument preflight; never prompts for trust.
    if unsafe { AXIsProcessTrusted() } == 0 {
        return None;
    }
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let pid = app.processIdentifier();
    if pid <= 0 || u32::try_from(pid).ok()? == std::process::id() {
        return None;
    }
    let deadline = Instant::now() + Duration::from_millis(40);
    // SAFETY: Live PID; Create supplies a +1 opaque AX application.
    let application = unsafe { CFRetained::from_raw(NonNull::new(AXUIElementCreateApplication(pid))?) };
    let window = read(&application, "AXFocusedWindow", deadline)?;
    // SAFETY: Type-check the retained object before treating it as AXUIElement.
    if CFGetTypeID(Some(&window)) != unsafe { AXUIElementGetTypeID() } {
        return None;
    }
    let position = read(&window, "AXPosition", deadline)?;
    let size = read(&window, "AXSize", deadline)?;
    let mut point = NSPoint::new(0.0, 0.0);
    let mut extent = NSSize::new(0.0, 0.0);
    let position_ptr = CFRetained::as_ptr(&position).as_ptr().cast();
    let size_ptr = CFRetained::as_ptr(&size).as_ptr().cast();
    // SAFETY: Verify both opaque objects and AXValue kinds before writing
    // their fixed-size CGPoint/CGSize values to matching stack storage.
    let valid = unsafe {
        CFGetTypeID(Some(&position)) == AXValueGetTypeID()
            && CFGetTypeID(Some(&size)) == AXValueGetTypeID()
            && AXValueGetType(position_ptr) == 1
            && AXValueGetType(size_ptr) == 2
            && AXValueGetValue(position_ptr, 1, (&raw mut point).cast()) != 0
            && AXValueGetValue(size_ptr, 2, (&raw mut extent).cast()) != 0
    };
    // A changed foreground app or timed-out reply is unknown, never a target.
    if !valid
        || Instant::now() > deadline
        || app.isTerminated()
        || NSWorkspace::sharedWorkspace().frontmostApplication().as_deref() != Some(&*app)
        || ![point.x, point.y, extent.width, extent.height]
            .iter()
            .all(|v| v.is_finite())
        || extent.width <= 0.0
        || extent.height <= 0.0
    {
        return None;
    }
    Some(Area {
        x: point.x,
        y: point.y,
        width: extent.width,
        height: extent.height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_macos_11_uses_visible_frame_without_reading_safe_insets_regression() {
        // An external display above/left of the primary, with a left Dock.
        let frame = NSRect::new(NSPoint::new(-1600.0, 900.0), NSSize::new(1600.0, 1000.0));
        let bounds = NSRect::new(NSPoint::new(-1600.0, -1000.0), frame.size);
        let visible = NSRect::new(NSPoint::new(-1520.0, 900.0), NSSize::new(1520.0, 975.0));
        assert_eq!(
            usable_area(bounds, frame, visible, false, || {
                panic!("safeAreaInsets is unavailable before macOS 12")
            }),
            Area {
                x: -1520.0,
                y: -975.0,
                width: 1520.0,
                height: 975.0,
            }
        );
        // Where available, the notch still restricts visibleFrame once.
        assert_eq!(
            usable_area(bounds, frame, visible, true, || NSEdgeInsets {
                top: 38.0,
                left: 0.0,
                bottom: 0.0,
                right: 0.0,
            }),
            Area {
                x: -1520.0,
                y: -962.0,
                width: 1520.0,
                height: 962.0,
            }
        );
    }
}
