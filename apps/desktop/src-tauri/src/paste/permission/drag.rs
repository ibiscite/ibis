//! A native file-URL drag, restricted to the running bundle and a Settings
//! mouse gesture. Never a webview path, file promise, clipboard write or move.

use std::cell::OnceCell;

use objc2_app_kit::{
    NSApplication, NSDragOperation, NSDraggingContext, NSDraggingItem, NSDraggingSession, NSDraggingSource, NSEvent,
    NSEventType, NSWorkspace,
};
use objc2_foundation::{
    MainThreadMarker, NSArray, NSBundle, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSURL,
};
use tauri_nspanel::objc2::{AnyThread, MainThreadOnly, define_class, msg_send, rc::Retained, runtime::ProtocolObject};

define_class!(
    // SAFETY: NSObject has no subclass requirements; no ivars or Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    struct AppDragSource;

    // SAFETY: NSObjectProtocol has no additional safety requirements.
    unsafe impl NSObjectProtocol for AppDragSource {}

    // SAFETY: The generated NSDraggingSource signatures are preserved. The
    // source is kept alive on the main thread for the whole dragging session.
    unsafe impl NSDraggingSource for AppDragSource {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn operations(&self, _session: &NSDraggingSession, _context: NSDraggingContext) -> NSDragOperation {
            NSDragOperation::Copy
        }

        #[unsafe(method(ignoreModifierKeysForDraggingSession:))]
        fn ignore_modifiers(&self, _session: &NSDraggingSession) -> bool {
            true
        }
    }
);

thread_local! {
    // Keep one small, stateless source alive for this thread's lifetime,
    // including after beginDraggingSession returns.
    static SOURCE: OnceCell<Retained<AppDragSource>> = const { OnceCell::new() };
}

fn app_item(url: Retained<NSURL>) -> Result<Retained<NSDraggingItem>, &'static str> {
    if !url.isFileURL()
        || url
            .pathExtension()
            .is_none_or(|ext| !ext.to_string().eq_ignore_ascii_case("app"))
    {
        return Err("Run the bundled Ibis app to drag it into System Settings.");
    }
    Ok(NSDraggingItem::initWithPasteboardWriter(
        NSDraggingItem::alloc(),
        ProtocolObject::from_ref(&*url),
    ))
}

fn accepts_drag(settings: bool, same_window: bool, key_window: bool, kind: NSEventType, buttons: usize) -> bool {
    settings
        && same_window
        && key_window
        && matches!(kind, NSEventType::LeftMouseDown | NSEventType::LeftMouseDragged)
        && buttons & 1 != 0
}

pub(super) fn start(window: &tauri::WebviewWindow) -> Result<(), &'static str> {
    let unavailable = "Start the app drag from Ibis Settings.";
    let mtm = MainThreadMarker::new().ok_or(unavailable)?;
    let application = NSApplication::sharedApplication(mtm);
    let event = application.currentEvent().ok_or(unavailable)?;
    let native = event.window(mtm).ok_or(unavailable)?;
    let expected = window.ns_window().map_err(|_| unavailable)?;
    if !accepts_drag(
        window.label() == "settings",
        Retained::as_ptr(&native).cast::<std::ffi::c_void>() == expected.cast_const(),
        native.isKeyWindow(),
        event.r#type(),
        NSEvent::pressedMouseButtons(),
    ) {
        return Err(unavailable);
    }
    let view = native.contentView().ok_or(unavailable)?;
    // The system supplies this URL for the executing bundle. Do not find an
    // installed copy by name/identifier, resolve links or accept a path in IPC.
    let url = NSBundle::mainBundle().bundleURL();
    let item = app_item(url.clone())?;
    let path = url.path().ok_or(unavailable)?;
    let icon = NSWorkspace::sharedWorkspace().iconForFile(&path);
    let point = view.convertPoint_fromView(event.locationInWindow(), None);
    let frame = NSRect::new(NSPoint::new(point.x - 16.0, point.y - 16.0), NSSize::new(32.0, 32.0));
    // SAFETY: AppKit accepts an NSImage as these contents; iconForFile returns
    // a retained NSImage, which the dragging item retains for its own lifetime.
    unsafe { item.setDraggingFrame_contents(frame, Some(&icon)) };
    SOURCE.with(|cell| {
        let source = cell.get_or_init(|| {
            // SAFETY: The main-thread-only NSObject subclass has no ivars or
            // custom initializer; NSObject's init initializes this allocation.
            unsafe { msg_send![AppDragSource::alloc(mtm), init] }
        });
        view.beginDraggingSessionWithItems_event_source(
            &NSArray::from_slice(&[&*item]),
            &event,
            ProtocolObject::from_ref(&**source),
        );
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_app_kit::NSPasteboardWriting;
    use objc2_foundation::NSString;

    fn url(text: &str) -> Retained<NSURL> {
        NSURL::URLWithString(&NSString::from_str(text)).unwrap_or_else(|| panic!("invalid synthetic URL"))
    }

    #[test]
    fn the_drag_preserves_the_exact_running_bundle_file_url() {
        for path in [
            "file:///Applications/Ibis.app/",
            "file:///Applications/Ibis%20Nightly.app/",
            "file:///Applications/Invented%20Copy.APP/",
            "file:///Volumes/Invented%20Disk/Renamed%20%E9%B3%A5.app/",
            "file:///tmp/Invented%20Build/Ibis.app/",
        ] {
            let original = url(path);
            let item = app_item(original.clone()).unwrap_or_else(|e| panic!("{e}"));
            let dragged = item
                .item()
                .downcast::<NSURL>()
                .unwrap_or_else(|_| panic!("expected a file URL"));
            assert_eq!(dragged.absoluteString(), original.absoluteString());
            assert_eq!(Retained::as_ptr(&dragged), Retained::as_ptr(&original));
            let payload = dragged
                .pasteboardPropertyListForType(&NSString::from_str("public.file-url"))
                .and_then(|data| data.downcast::<NSString>().ok());
            assert_eq!(payload, original.absoluteString());
        }
    }

    #[test]
    fn an_unbundled_development_binary_or_non_file_url_is_never_dragged() {
        for path in [
            "file:///tmp/invented/target/debug/",
            "file:///tmp/invented.txt",
            "https://example.invalid/Ibis.app",
        ] {
            assert!(app_item(url(path)).is_err());
        }
    }

    #[test]
    fn only_a_live_left_drag_from_the_settings_window_can_start_a_session() {
        let dragged = NSEventType::LeftMouseDragged;
        assert!(accepts_drag(true, true, true, dragged, 1));
        // WebKit may keep mouseDown as currentEvent while it tracks the drag.
        assert!(accepts_drag(true, true, true, NSEventType::LeftMouseDown, 1));
        assert!(!accepts_drag(false, true, true, dragged, 1));
        assert!(!accepts_drag(true, false, true, dragged, 1));
        assert!(!accepts_drag(true, true, false, dragged, 1));
        assert!(!accepts_drag(true, true, true, dragged, 0));
        assert!(!accepts_drag(true, true, true, dragged, 2));
        for kind in [
            NSEventType::LeftMouseUp,
            NSEventType::RightMouseDragged,
            NSEventType::KeyDown,
        ] {
            assert!(!accepts_drag(true, true, true, kind, 1));
        }
    }
}
