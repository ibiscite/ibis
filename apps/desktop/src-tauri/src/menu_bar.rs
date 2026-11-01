//! The bird in the Mac menu bar: its poses, a library dropped on it, Search
//! with Ibis from the Services menu, and the trackpad's alignment tick.
#![allow(unsafe_code)]

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use objc2_app_kit::{
    NSDragOperation, NSDraggingDestination, NSDraggingInfo, NSHapticFeedbackManager, NSHapticFeedbackPattern,
    NSHapticFeedbackPerformanceTime, NSHapticFeedbackPerformer, NSPasteboard, NSPasteboardTypeFileURL,
    NSPasteboardTypeString, NSRegisterServicesProvider, NSWindowDelegate, NSWorkspace,
};
use objc2_foundation::{MainThreadMarker, NSArray, NSObject, NSObjectProtocol, NSString, NSURL};
use tauri::image::Image;
use tauri::tray::TrayIcon;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_nspanel::objc2::{MainThreadOnly, define_class, msg_send, rc::Retained, runtime::ProtocolObject};

use crate::diagnostics::{NativeIssue, report};
use crate::library::{Source, accepts_library_file, start_open};
use crate::tray::SearchRequest;

/// The bird the system tints, from `brand/`.
pub(crate) const BIRD: Image<'static> = tauri::include_image!("../../../brand/exports/png/ibis-menu-bar.png");
const LOWERING: Image<'static> = tauri::include_image!("../../../brand/exports/png/ibis-menu-bar-dip-1.png");
const LOWERED: Image<'static> = tauri::include_image!("../../../brand/exports/png/ibis-menu-bar-dip-2.png");

const DIP_FRAME: Duration = Duration::from_millis(50);
/// The selection is a search box's worth of text, not a document.
const SEARCH_LIMIT: usize = 200;

static DIPPING: AtomicBool = AtomicBool::new(false);
static READING: AtomicBool = AtomicBool::new(false);
static APP: OnceLock<AppHandle> = OnceLock::new();

fn pose(app: &AppHandle, bird: Image<'static>) {
    let handle = app.clone();
    report(
        app.run_on_main_thread(move || {
            if let Some(tray) = handle.tray_by_id("ibis") {
                report(
                    tray.set_icon_with_as_template(Some(bird), true),
                    NativeIssue::TrayCreation,
                );
            }
        }),
        NativeIssue::TrayCreation,
    );
}

fn resting() -> Image<'static> {
    if READING.load(Ordering::SeqCst) { LOWERED } else { BIRD }
}

/// The bird dips its beak once after a paste or copy, in 200 ms. A second
/// citation inside the dip does not restart it, and Reduce Motion keeps it still.
pub(crate) fn dip(app: &AppHandle) {
    let reduced = NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion();
    if reduced || crate::tray::hidden() || READING.load(Ordering::SeqCst) || DIPPING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        for bird in [LOWERING, LOWERED, LOWERING] {
            pose(&app, bird);
            std::thread::sleep(DIP_FRAME);
        }
        // On the thread `show_reading` uses, so a reload ending now cannot be missed.
        let handle = app.clone();
        let finished = app.run_on_main_thread(move || {
            DIPPING.store(false, Ordering::SeqCst);
            pose(&handle, resting());
        });
        if finished.is_err() {
            DIPPING.store(false, Ordering::SeqCst);
        }
    });
}

/// The bird keeps its head down while a library reloads. A swap, so Reduce
/// Motion needs nothing different.
pub(crate) fn show_reading(app: &AppHandle, reading: bool) {
    if READING.swap(reading, Ordering::SeqCst) != reading && !DIPPING.load(Ordering::SeqCst) {
        pose(app, resting());
    }
}

/// The trackpad's alignment tick. Silent without a trackpad.
pub(crate) fn tick() {
    NSHapticFeedbackManager::defaultPerformer().performFeedbackPattern_performanceTime(
        NSHapticFeedbackPattern::Alignment,
        NSHapticFeedbackPerformanceTime::Now,
    );
}

/// The file a drag carries, when it is one Ibis can read.
fn dragged_library(sender: &ProtocolObject<dyn NSDraggingInfo>) -> Option<std::path::PathBuf> {
    // SAFETY: A shared AppKit constant, read on the main thread.
    let kind = unsafe { NSPasteboardTypeFileURL };
    let url = sender.draggingPasteboard().stringForType(kind)?;
    let path = NSURL::URLWithString(&url)?.filePathURL()?.path()?.to_string();
    let path = std::path::PathBuf::from(path);
    accepts_library_file(&path).then_some(path)
}

define_class!(
    // SAFETY: NSObject has no subclass requirements; no ivars or Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    struct MenuBarTarget;

    // SAFETY: NSObjectProtocol has no additional safety requirements.
    unsafe impl NSObjectProtocol for MenuBarTarget {}

    // SAFETY: Every method is optional and none is implemented.
    unsafe impl NSWindowDelegate for MenuBarTarget {}

    // SAFETY: The generated signatures are preserved, on the main thread.
    unsafe impl NSDraggingDestination for MenuBarTarget {
        #[unsafe(method(draggingEntered:))]
        fn entered(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
            if dragged_library(sender).is_some() { NSDragOperation::Copy } else { NSDragOperation::None }
        }

        #[unsafe(method(performDragOperation:))]
        fn perform(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
            let dropped = dragged_library(sender).zip(APP.get());
            let accepted = dropped.is_some();
            if let Some((path, app)) = dropped {
                start_open(app, Source::File(path), true);
                crate::picker::show_picker(app);
            }
            accepted
        }
    }

    impl MenuBarTarget {
        /// Named in `Info.plist`'s NSServices entry.
        #[unsafe(method(searchWithIbis:userData:error:))]
        fn search(&self, pasteboard: &NSPasteboard, _data: Option<&NSString>, _error: *mut *mut NSString) {
            // SAFETY: A shared AppKit constant, read on the main thread.
            let text = pasteboard.stringForType(unsafe { NSPasteboardTypeString });
            let Some(app) = APP.get() else { return };
            if let Some(text) = text.map(|text| text.to_string()) {
                // A search box holds one line; the picker's own paste does the same.
                let query: String = text.trim().replace(['\r', '\n'], "\t").chars().take(SEARCH_LIMIT).collect();
                if let Ok(mut waiting) = app.state::<SearchRequest>().0.lock() {
                    *waiting = Some(query);
                }
                crate::picker::show_picker(app);
                report(app.emit_to("main", "picker:search-requested", ()), NativeIssue::Notice);
            }
        }
    }
);

thread_local! {
    // AppKit holds delegates weakly: keep the one target for the app's lifetime.
    static TARGET: std::cell::OnceCell<Retained<MenuBarTarget>> = const { std::cell::OnceCell::new() };
}

/// Take a library dropped on the bird, and answer the Services menu. Hiding the
/// bird destroys its window, so showing it again must call `watch` again.
pub(crate) fn accept_drops(app: &AppHandle, tray: &TrayIcon) {
    let _ = APP.set(app.clone());
    watch(tray);
}

pub(crate) fn watch<R: Runtime>(tray: &TrayIcon<R>) {
    report(
        tray.with_inner_tray_icon(|inner| {
            let Some(main_thread) = MainThreadMarker::new() else {
                return;
            };
            let target = TARGET.with(|target| {
                // SAFETY: Plain NSObject initialisation on the main thread.
                target
                    .get_or_init(|| unsafe { msg_send![MenuBarTarget::alloc(main_thread), init] })
                    .clone()
            });
            // SAFETY: The target lives for the whole app, on the main thread.
            unsafe { NSRegisterServicesProvider(Some(&target), &NSString::from_str("Ibis")) };
            let window = inner
                .ns_status_item()
                .and_then(|item| item.button(main_thread))
                .and_then(|button| button.window());
            if let Some(window) = window {
                // SAFETY: A shared AppKit constant, read on the main thread.
                window.registerForDraggedTypes(&NSArray::from_slice(&[unsafe { NSPasteboardTypeFileURL }]));
                window.setDelegate(Some(ProtocolObject::from_ref(&*target)));
            }
        }),
        NativeIssue::TrayCreation,
    );
}
