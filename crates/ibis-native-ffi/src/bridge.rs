use crate::publish_macos;
use dispatch2::DispatchQueue;
use ibis_core::contract::ClipboardContent;
use ibis_core::destination::{AppId, AppPaste, CopyReason, PastePermission, System, lookup};
use ibis_native::paste::{self, Captured, CopiedReason, CopyAttempt, Delivered, LastTarget, capture, macos};
use ibis_native::site::{self, Front};
use objc2::rc::Retained;
use objc2_app_kit::{NSApplication, NSEvent, NSPasteboard, NSWindow};
use objc2_core_graphics::{
    CGEvent, CGEventFlags, CGEventSource, CGEventSourceStateID, CGPreflightPostEventAccess, CGRequestPostEventAccess,
};
use objc2_foundation::MainThreadMarker;
use std::cell::{Cell, RefCell};
use std::sync::LazyLock;

static TARGET: LazyLock<LastTarget> = LazyLock::new(|| LastTarget::new(write_token));
fn write_token() -> Result<u32, String> {
    NEXT.with(|sequence| sequence.get().checked_add(1).map(|next| sequence.replace(next)))
        .ok_or_else(|| "Clipboard write identifiers exhausted.".into())
}

thread_local! {
    static NEXT: Cell<u32> = const { Cell::new(1) };
    static WINDOW: RefCell<Option<Retained<NSWindow>>> = const { RefCell::new(None) };
    static PENDING: RefCell<Option<(u32, isize, CopyAttempt)>> = const { RefCell::new(None) };
    /// The short selection taken at the last capture, handed to the search once.
    static SELECTED: RefCell<Option<String>> = const { RefCell::new(None) };
}

#[derive(Clone)]
struct Host;
impl macos::Host for Host {
    fn run_on_main(&self, work: impl FnOnce() + Send + 'static) -> bool {
        DispatchQueue::main().exec_async(work);
        true
    }

    fn last_target(&self) -> &LastTarget {
        &TARGET
    }

    fn picker_hidden(&self) -> bool {
        MainThreadMarker::new().is_some()
            && WINDOW.with_borrow(|window| window.as_ref().is_some_and(|window| !window.isVisible()))
    }

    fn open_paste_guide(&self) {}

    fn paste_key(&self) -> Option<u16> {
        command_key(0x09, b'v')
    }

    fn paste_choice(&self, _id: &AppId, _identity: Option<&str>) -> Option<(Option<AppPaste>, bool)> {
        // This picker only reads focus evidence. It has no UI for per-app
        // consent or for enabling Chromium's optional AX provider.
        None
    }
}

fn command_key(preferred: u16, letter: u8) -> Option<u16> {
    let _main = MainThreadMarker::new()?;
    let source = CGEventSource::new(CGEventSourceStateID::Private);
    let matches = |key| {
        let Some(event) = CGEvent::new_keyboard_event(source.as_deref(), key, true) else {
            return false;
        };
        CGEvent::set_flags(Some(&event), CGEventFlags::MaskCommand);
        NSEvent::eventWithCGEvent(&event)
            .and_then(|event| event.characters())
            .is_some_and(|text| text.to_string().eq_ignore_ascii_case(&char::from(letter).to_string()))
    };
    std::iter::once(preferred).chain(0..=0x32).find(|&key| matches(key))
}

// SAFETY: The ibis_native_* symbols are reserved for this C interface.
#[unsafe(no_mangle)]
extern "C" fn ibis_native_command_key(preferred: u16, lower_ascii: u8) -> i32 {
    command_key(preferred, lower_ascii).map_or(-1, i32::from)
}

// SAFETY: The exported symbol has a unique name and takes a window number,
// not an Objective-C object or caller-owned address.
#[unsafe(no_mangle)]
extern "C" fn ibis_native_capture(window_number: isize, expected_pid: i32) -> IbisUtf8 {
    let empty = IbisUtf8 {
        data: std::ptr::null(),
        len: 0,
    };
    let Some(main) = MainThreadMarker::new() else {
        return empty;
    };
    let app = NSApplication::sharedApplication(main);
    WINDOW.set(app.windowWithWindowNumber(window_number));
    cancel();
    let own = i32::try_from(std::process::id()).unwrap_or(i32::MAX);
    let (front, ()) = site::read(own, || {
        let mut front = macos::frontmost(&Host);
        SELECTED.set(
            front
                .as_mut()
                .and_then(|front| front.search.take())
                .map(capture::SelectedText::into_search),
        );
        let captured = capture::captured(System::Macos, front, own).unwrap_or_else(|| Captured::unknown(System::Macos));
        TARGET.begin_presentation(Some(captured));
        ((), TARGET.captured_window())
    });
    let Some(Front::App {
        app,
        pid,
        site: Some(site),
    }) = front
    else {
        return empty;
    };
    if pid != expected_pid
        || !TARGET
            .with_captured(|captured| {
                captured.is_some_and(|target| target.pid == Some(pid) && target.target.app.as_ref() == Some(&app))
            })
            .unwrap_or(false)
    {
        return empty;
    }
    // Only a canonical table name leaves this call. Its bytes are static.
    lookup(&site).map_or(empty, |entry| IbisUtf8 {
        data: entry.id.as_ptr(),
        len: entry.id.len(),
    })
}

/// Main thread, once after a capture: copies the short selection the person
/// had in the app in front into `out` and forgets it. Returns its length in
/// bytes, or 0 when there was none or it does not fit.
// SAFETY: This symbol is unique. The caller passes a writable buffer of
// `capacity` bytes that it owns for the duration of the call.
#[unsafe(no_mangle)]
unsafe extern "C" fn ibis_native_take_selection(out: *mut u8, capacity: usize) -> usize {
    if MainThreadMarker::new().is_none() || out.is_null() {
        return 0;
    }
    let Some(text) = SELECTED.take() else { return 0 };
    if text.len() > capacity {
        return 0;
    }
    // SAFETY: `out` is non-null and valid for `capacity` bytes, checked above
    // to hold all of `text`; the two allocations cannot overlap.
    unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), out, text.len()) };
    text.len()
}

fn cancel() {
    PENDING.set(None);
    SELECTED.set(None);
    let _ = TARGET.change_destination(true, |_| Ok(()));
}

// SAFETY: This unique symbol only acts on native process-local state.
#[unsafe(no_mangle)]
extern "C" fn ibis_native_cancel() {
    if MainThreadMarker::new().is_some() {
        cancel();
    }
}

// SAFETY: This unique symbol returns a value token, never a native pointer.
#[unsafe(no_mangle)]
extern "C" fn ibis_native_begin() -> u32 {
    if MainThreadMarker::new().is_none() {
        return 0;
    }
    let Ok(request) = write_token() else { return 0 };
    let board = NSPasteboard::generalPasteboard();
    let attempt = TARGET.begin_copy(|| 0, &[], |_| Ok(None));
    PENDING.set(Some((request, board.changeCount(), attempt)));
    request
}

#[repr(C)]
struct IbisUtf8 {
    data: *const u8,
    len: usize,
}

impl IbisUtf8 {
    // The value is borrowed only for the call made by withUtf8 in Swift.
    unsafe fn borrow(&self) -> Result<Option<&str>, ()> {
        if self.data.is_null() {
            return if self.len == 0 { Ok(None) } else { Err(()) };
        }
        if self.len > 16 * 1024 * 1024 {
            return Err(());
        }
        // SAFETY: The caller supplies a single live readable allocation
        // for this range of bytes and retains it through the call.
        let bytes = unsafe { std::slice::from_raw_parts(self.data, self.len) };
        std::str::from_utf8(bytes).map(Some).map_err(|_| ())
    }
}

// SAFETY: This symbol is unique. The caller keeps each non-null buffer
// immutable and readable for its specified length for the duration of the call.
#[unsafe(no_mangle)]
unsafe extern "C" fn ibis_native_write(
    request: u32,
    text: IbisUtf8,
    html: IbisUtf8,
    rtf: IbisUtf8,
    paste: bool,
) -> u32 {
    if MainThreadMarker::new().is_none() {
        return 0;
    }
    let pending = PENDING.with_borrow_mut(|pending| {
        if pending.as_ref().is_some_and(|(current, _, _)| *current == request) {
            pending.take()
        } else {
            None
        }
    });
    let Some((_, mark, attempt)) = pending else { return 0 };
    let board = NSPasteboard::generalPasteboard();
    if board.changeCount() != mark {
        return 0;
    }
    // SAFETY: Each passed buffer outlives this call under the C ABI contract.
    let (Ok(Some(text)), Ok(html), Ok(rtf)) = (unsafe { text.borrow() }, unsafe { html.borrow() }, unsafe {
        rtf.borrow()
    }) else {
        return 0;
    };
    TARGET
        .write_copy(paste.then_some(attempt), || {
            let count = publish_macos(&board, ClipboardContent { text, html, rtf })?;
            Ok(count.and_then(|count| i64::try_from(count).ok()))
        })
        .unwrap_or(0)
}

#[repr(C)]
enum IbisDelivery {
    Pasted = 0,
    Copied = 1,
    PermissionRequired = 2,
}

// SAFETY: This unique symbol consumes only an integer receipt. Native objects
// are checked and used by ibis-native on the main queue.
#[unsafe(no_mangle)]
extern "C" fn ibis_native_paste(token: u32) -> IbisDelivery {
    if token == 0 || MainThreadMarker::new().is_some() {
        return IbisDelivery::Copied;
    }
    match paste::deliver_copy(&Host, || Some(true), Some(token)) {
        Delivered::Pasted { .. } => IbisDelivery::Pasted,
        Delivered::Copied {
            reason: CopiedReason::Decided(CopyReason::NoPermission | CopyReason::AccessibilityRequired),
        } => IbisDelivery::PermissionRequired,
        Delivered::Copied { .. } => IbisDelivery::Copied,
    }
}

// SAFETY: This unique symbol performs a passive grant check on the main thread.
#[unsafe(no_mangle)]
extern "C" fn ibis_native_paste_granted() -> bool {
    MainThreadMarker::new().is_some() && macos::accessibility_granted() && CGPreflightPostEventAccess()
}

// SAFETY: This unique symbol reads the current capture and passive grants.
#[unsafe(no_mangle)]
extern "C" fn ibis_native_paste_destination(enabled: bool) -> i32 {
    TARGET
        .with_captured(|captured| {
            let captured = captured?;
            paste::offers_paste(
                captured,
                PastePermission {
                    enabled,
                    granted: enabled && ibis_native_paste_granted(),
                    app: None,
                },
            )
            .then_some(captured.pid)
            .flatten()
        })
        .flatten()
        .unwrap_or(0)
}

// SAFETY: This unique symbol can only request the app's own paste grants.
#[unsafe(no_mangle)]
extern "C" fn ibis_native_request_paste_access() {
    if MainThreadMarker::new().is_none() {
        return;
    }
    if !CGPreflightPostEventAccess() {
        CGRequestPostEventAccess();
    }
    if !macos::accessibility_granted() {
        macos::request_accessibility();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_known_target_can_offer_paste_while_waiting_for_permission() {
        let mut capture = Captured::unknown(System::Macos);
        capture.target.app = Some(AppId {
            system: System::Macos,
            id: "com.apple.TextEdit".into(),
        });
        capture.focus_failure = Some(CopyReason::AccessibilityRequired);
        let permission = PastePermission {
            enabled: true,
            granted: false,
            app: None,
        };
        assert!(paste::offers_paste(&capture, permission));
        assert!(!paste::intended_paste(Some(&capture), permission));
        assert!(!paste::offers_paste(
            &capture,
            PastePermission {
                enabled: false,
                ..permission
            }
        ));
        if let Some(app) = &mut capture.target.app {
            app.id = "com.apple.Terminal".into();
        }
        assert!(!paste::offers_paste(&capture, permission));
    }
}
