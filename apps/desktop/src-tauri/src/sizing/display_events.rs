//! Native display/work-area invalidation only; the sizing owner does the read.
//! The bindings expose no safe notification/subclass registration (ADR 0029).
#![cfg_attr(any(target_os = "macos", target_os = "windows"), allow(unsafe_code))]

use super::*;

fn changed(app: &AppHandle) {
    let sizing = app.state::<Sizing>();
    owner::display_changed(&sizing.shared.state, Instant::now(), || {
        let task = app.clone();
        // In locked Wry, run_on_main_thread executes inline on the main
        // thread. Dispatch from the existing blocking pool so it sends a real
        // Tao event, even when the native notification itself queued none.
        tauri::async_runtime::spawn_blocking(move || {
            let handle = task.clone();
            let queued = task.run_on_main_thread(move || {
                let sizing = handle.state::<Sizing>();
                owner::display_settled(&sizing.shared.state, || read_settled(&handle));
            });
            if queued.is_err() {
                owner::display_wake_failed(&task.state::<Sizing>().shared.state);
            }
            report(queued, NativeIssue::Monitor);
        });
    });
    sizing.shared.wake.notify_all();
}

#[cfg(target_os = "macos")]
pub(super) fn install(app: &AppHandle, _window: &WebviewWindow) -> Option<()> {
    use block2::RcBlock;
    use objc2_foundation::{NSNotificationCenter, NSOperationQueue, NSString};

    let handle = app.clone();
    let block = RcBlock::new(move |_| changed(&handle));
    // SAFETY: No object filter; the main queue owns delivery. The sendable
    // callback captures a sendable app handle and schedules the sizing owner,
    // never borrowing AppKit objects or notification payloads.
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
    use std::sync::OnceLock;
    use winsafe::{HWND, co, msg::Wm, prelude::*};

    // No borrowed native pointer; the app handle only schedules the sizing owner.
    static APP: OnceLock<AppHandle> = OnceLock::new();
    const SUBCLASS: usize = 0x49424953;

    pub(super) fn install(app: &AppHandle, window: &WebviewWindow) -> Option<()> {
        let hwnd = checked(window.hwnd(), NativeIssue::Window)?;
        APP.set(app.clone()).ok()?;
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
            && let Some(app) = APP.get()
        {
            super::changed(app);
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

    fn watch(monitor: &gdk::Monitor, app: &AppHandle) {
        let handle = app.clone();
        monitor.connect_workarea_notify(move |_| changed(&handle));
        let handle = app.clone();
        monitor.connect_geometry_notify(move |_| changed(&handle));
        let handle = app.clone();
        monitor.connect_scale_factor_notify(move |_| changed(&handle));
    }

    let native = checked(window.gtk_window(), NativeIssue::Window)?;
    let display = WidgetExt::display(&native);
    for index in 0..display.n_monitors() {
        if let Some(monitor) = display.monitor(index) {
            watch(&monitor, app);
        }
    }
    let handle = app.clone();
    display.connect_monitor_added(move |_, monitor| {
        watch(monitor, &handle);
        changed(&handle);
    });
    let handle = app.clone();
    display.connect_monitor_removed(move |_, _| changed(&handle));
    Some(())
}
