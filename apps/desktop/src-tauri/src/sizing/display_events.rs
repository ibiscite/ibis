//! Native display/work-area invalidation only; the sizing owner does the read.
//! The bindings expose no safe notification/subclass registration (ADR 0029).
#![cfg_attr(any(target_os = "macos", target_os = "windows"), allow(unsafe_code))]

use super::*;

#[cfg(target_os = "macos")]
pub(super) fn install(app: &AppHandle, _window: &WebviewWindow) -> Option<()> {
    use block2::RcBlock;
    use objc2_foundation::{NSNotificationCenter, NSOperationQueue, NSString};

    let shared = app.state::<Sizing>().shared.clone();
    let block = RcBlock::new(move |_| shared.changed(Event::Geometry));
    // SAFETY: No object filter; the main queue owns delivery. The sendable
    // callback captures only Arc<Shared> and marks pending state, never AppKit.
    // The default center retains the copied block/observer for this app lifetime.
    let observer = unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(&NSString::from_str(
                "NSApplicationDidChangeScreenParametersNotification",
            )),
            None,
            Some(&NSOperationQueue::mainQueue()),
            &block,
        )
    };
    std::mem::forget(observer);
    Some(())
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use std::sync::{OnceLock, Weak};
    use winsafe::{HWND, co, msg::Wm, prelude::*};

    // No borrowed native pointer or AppHandle in the callback; one picker HWND.
    static OWNER: OnceLock<Weak<Shared>> = OnceLock::new();
    const SUBCLASS: usize = 0x49424953;

    pub(super) fn install(app: &AppHandle, window: &WebviewWindow) -> Option<()> {
        let hwnd = checked(window.hwnd(), NativeIssue::Window)?;
        OWNER.set(Arc::downgrade(&app.state::<Sizing>().shared)).ok()?;
        // SAFETY: Tauri supplies the live picker HWND; setup and subclass
        // installation run on its owning thread. The callback is static, holds
        // no native references, forwards every message, and removes itself at
        // WM_NCDESTROY. Its ref_data is unused, so no pointer can outlive setup.
        checked(
            unsafe { HWND::from_ptr(hwnd.0).SetWindowSubclass(changed, SUBCLASS, 0) },
            NativeIssue::Monitor,
        )
    }

    extern "system" fn changed(
        hwnd: HWND,
        message: co::WM,
        wparam: usize,
        lparam: isize,
        id: usize,
        _data: usize,
    ) -> isize {
        if (message == co::WM::DISPLAYCHANGE
            || message == co::WM::WININICHANGE && wparam == co::SPI::SETWORKAREA.raw() as usize)
            && let Some(shared) = OWNER.get().and_then(Weak::upgrade)
        {
            shared.changed(Event::Geometry);
        }
        if message == co::WM::NCDESTROY {
            let _ = hwnd.RemoveWindowSubclass(changed, id);
        }
        // SAFETY: Forward the original HWND/message/parameters unchanged from
        // the OS subclass callback, including destruction; no payload is read.
        unsafe { hwnd.DefSubclassProc(Wm::new(message, wparam, lparam)) }
    }
}

#[cfg(target_os = "windows")]
pub(super) fn install(app: &AppHandle, window: &WebviewWindow) -> Option<()> {
    windows::install(app, window)
}

#[cfg(target_os = "linux")]
pub(super) fn install(app: &AppHandle, window: &WebviewWindow) -> Option<()> {
    use gtk::{gdk, prelude::*};

    fn watch(monitor: &gdk::Monitor, shared: &Arc<Shared>) {
        let owner = shared.clone();
        monitor.connect_workarea_notify(move |_| owner.changed(Event::Geometry));
        let owner = shared.clone();
        monitor.connect_geometry_notify(move |_| owner.changed(Event::Geometry));
        let owner = shared.clone();
        monitor.connect_scale_factor_notify(move |_| owner.changed(Event::Geometry));
    }

    let native = checked(window.gtk_window(), NativeIssue::Window)?;
    let display = WidgetExt::display(&native);
    let shared = app.state::<Sizing>().shared.clone();
    for index in 0..display.n_monitors() {
        if let Some(monitor) = display.monitor(index) {
            watch(&monitor, &shared);
        }
    }
    let owner = shared.clone();
    display.connect_monitor_added(move |_, monitor| {
        watch(monitor, &owner);
        owner.changed(Event::Geometry);
    });
    display.connect_monitor_removed(move |_, _| shared.changed(Event::Geometry));
    Some(())
}
