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

/// A handle can be reused within the same browser process. Keep the UIA
/// identity too, so the confirmation cannot give a new window the old site.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Window {
    handle: usize,
    pid: u32,
    runtime_id: Vec<i32>,
}

impl super::Source for Native {
    type Window = Window;

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

    fn window(&self, pid: i32, deadline: Instant) -> Option<Window> {
        let window = w::HWND::GetForegroundWindow()?;
        let (_, owner) = window.GetWindowThreadProcessId();
        if i32::try_from(owner).ok()? != pid {
            return None;
        }
        let handle = window.ptr() as usize;
        with_window(handle, owner, deadline, move |_, element| {
            let runtime_id = element.get_runtime_id().ok()?;
            (!runtime_id.is_empty()).then_some(Window {
                handle,
                pid: owner,
                runtime_id,
            })
        })
    }

    fn matches_capture(&self, window: &Window, captured: super::CapturedWindow) -> bool {
        matches!(captured, super::CapturedWindow::Windows(handle) if handle == window.handle)
    }

    fn address(&self, window: &Window, deadline: Instant) -> Option<String> {
        let foreground = w::HWND::GetForegroundWindow()?;
        if foreground.ptr() as usize != window.handle {
            return None;
        }
        let window = window.clone();
        with_window(window.handle, window.pid, deadline, move |automation, element| {
            if element.get_runtime_id().ok()? != window.runtime_id {
                return None;
            }
            address_field(automation, element)
        })
    }
}

/// Both identity and address reads need COM on their own thread. Keep all
/// UIA calls under the same worker limit and the caller's deadline; a late
/// answer is dropped with the channel.
fn with_window<T: Send + 'static>(
    handle: usize,
    pid: u32,
    deadline: Instant,
    read: impl FnOnce(&UIAutomation, &UIElement) -> Option<T> + Send + 'static,
) -> Option<T> {
    if Instant::now() >= deadline {
        return None;
    }
    let pending = Pending::start()?;
    let (send, receive) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("ibis-site".into())
        .spawn(move || {
            let _pending = pending;
            let result = (|| {
                let automation = UIAutomation::new().ok()?;
                let handle = windows::Win32::Foundation::HWND(handle as *mut std::ffi::c_void);
                let window = automation
                    .element_from_handle(uiautomation::types::Handle::from(handle))
                    .ok()?;
                if window.get_process_id().ok()? != pid {
                    return None;
                }
                read(&automation, &window)
            })();
            let _ = send.send(result);
        })
        .ok()?;
    receive
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .ok()
        .flatten()
}

/// The value of the first address field in the checked top-level window.
fn address_field(automation: &UIAutomation, window: &UIElement) -> Option<String> {
    let walker = automation.get_control_view_walker().ok()?;
    // Breadth first through the window's own controls; a document is a page,
    // and is never entered.
    let mut queue: VecDeque<UIElement> = walker.get_first_child(window).ok().into_iter().collect();
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
