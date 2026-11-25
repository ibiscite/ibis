//! The Windows read: the browser's address field, through UI Automation,
//! which needs no grant. Only the focused element's process, the window's
//! controls and the first address field's value are read; the walk never
//! enters a page, so no page content is read.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Instant;

use ibis_core::destination::{AppId, System};
use uiautomation::patterns::UIValuePattern;
use uiautomation::types::ControlType;
use uiautomation::{UIAutomation, UIElement};
use winsafe::{self as w, co};

/// Controls the walk may look at before giving up: a browser window's own
/// controls are a few dozen; a page is never entered.
const MAX_ELEMENTS: usize = 200;

/// Reads still running after their opening gave up on them. A browser that
/// hangs UI Automation costs at most this many threads, never one per opening.
static PENDING: AtomicUsize = AtomicUsize::new(0);
const MAX_PENDING: usize = 2;

/// Counts a running read until its thread ends.
struct Pending;

impl Pending {
    fn start() -> Option<Self> {
        if PENDING.fetch_add(1, Ordering::AcqRel) >= MAX_PENDING {
            PENDING.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        Some(Self)
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        PENDING.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(super) struct Native;

impl super::Source for Native {
    /// The foreground window's handle, as a number; it is only compared.
    type Window = usize;

    fn front(&self) -> Option<(i32, AppId)> {
        let window = w::HWND::GetForegroundWindow()?;
        let (_, pid) = window.GetWindowThreadProcessId();
        let process = w::HPROCESS::OpenProcess(co::PROCESS::QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let path = process.QueryFullProcessImageName(co::PROCESS_NAME::WIN32).ok()?;
        let id = path.rsplit(['\\', '/']).next()?.to_owned();
        Some((
            i32::try_from(pid).ok()?,
            AppId {
                system: System::Windows,
                id,
            },
        ))
    }

    fn allowed(&self) -> bool {
        true
    }

    fn window(&self, pid: i32, _deadline: Instant) -> Option<usize> {
        let window = w::HWND::GetForegroundWindow()?;
        let (_, owner) = window.GetWindowThreadProcessId();
        (i32::try_from(owner).ok()? == pid).then(|| window.ptr() as usize)
    }

    fn address(&self, window: &usize, deadline: Instant) -> Option<String> {
        let foreground = w::HWND::GetForegroundWindow()?;
        if foreground.ptr() as usize != *window {
            return None;
        }
        let (_, pid) = foreground.GetWindowThreadProcessId();
        let window = *window;
        // A thread of its own: UI Automation needs COM set up on the thread
        // that calls it, and a hung browser must not hold the picker. A late
        // answer is dropped with the channel.
        let pending = Pending::start()?;
        let (send, receive) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("ibis-site".into())
            .spawn(move || {
                let _pending = pending;
                let _ = send.send(address_field(window, pid));
            })
            .ok()?;
        receive
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .ok()
            .flatten()
    }
}

/// The value of the first address field in `window`, the foreground
/// top-level window checked before this read, when `pid` still owns it.
fn address_field(window: usize, pid: u32) -> Option<String> {
    let automation = UIAutomation::new().ok()?;
    let handle = windows::Win32::Foundation::HWND(window as *mut std::ffi::c_void);
    let window = automation.element_from_handle(uiautomation::types::Handle::from(handle)).ok()?;
    if window.get_process_id().ok()? != pid {
        return None;
    }
    let walker = automation.get_control_view_walker().ok()?;
    // Breadth first through the window's own controls; a document is a page,
    // and is never entered.
    let mut queue: VecDeque<UIElement> = walker.get_first_child(&window).ok().into_iter().collect();
    let mut seen = 0;
    while let Some(element) = queue.pop_front() {
        seen += 1;
        if seen > MAX_ELEMENTS {
            return None;
        }
        if let Ok(next) = walker.get_next_sibling(&element) {
            queue.push_back(next);
        }
        match element.get_control_type() {
            Ok(ControlType::Edit) => {
                return element.get_pattern::<UIValuePattern>().ok()?.get_value().ok();
            }
            // A page, or a control that will not say what it is, is never entered.
            Ok(ControlType::Document) | Err(_) => {}
            Ok(_) => {
                if let Ok(child) = walker.get_first_child(&element) {
                    queue.push_back(child);
                }
            }
        }
    }
    None
}
