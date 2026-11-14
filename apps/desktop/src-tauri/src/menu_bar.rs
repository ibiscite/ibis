//! The bird in the Mac menu bar: its poses, text or a library dropped on it, Search
//! with Ibis from the Services menu, and the trackpad's alignment tick.
#![allow(unsafe_code)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Once, OnceLock};
use std::time::Duration;

use objc2_app_kit::{
    NSAccessibility, NSDragOperation, NSDraggingDestination, NSDraggingInfo, NSHapticFeedbackManager,
    NSHapticFeedbackPattern, NSHapticFeedbackPerformanceTime, NSHapticFeedbackPerformer, NSPasteboard,
    NSPasteboardTypeFileURL, NSPasteboardTypeString, NSRegisterServicesProvider, NSWindowDelegate, NSWorkspace,
};
use objc2_foundation::{MainThreadMarker, NSArray, NSObject, NSObjectProtocol, NSRange, NSString, NSURL};
use tauri::image::Image;
use tauri::tray::TrayIcon;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_nspanel::objc2::{MainThreadOnly, define_class, msg_send, rc::Retained, runtime::ProtocolObject};

use crate::diagnostics::{NativeIssue, report};
use crate::library::{Source, accepts_library_file, start_open};
use crate::tray::SearchRequest;

/// The bird the system tints, from `brand/`.
pub(crate) const BIRD: Image<'static> = tauri::include_image!("../../../brand/exports/png/ibis-menu-bar.png");
const RAISED: Image<'static> = tauri::include_image!("../../../brand/exports/png/ibis-menu-bar-raised.png");
const LOWERING: Image<'static> = tauri::include_image!("../../../brand/exports/png/ibis-menu-bar-dip-1.png");
const LOWERED: Image<'static> = tauri::include_image!("../../../brand/exports/png/ibis-menu-bar-dip-2.png");

const DIP_FRAME: Duration = Duration::from_millis(50);
/// The selection is a search box's worth of text, not a document.
const SEARCH_LIMIT: usize = 200;
const TEXT_PREFIX: usize = SEARCH_LIMIT * 4;

static HOVERING: AtomicBool = AtomicBool::new(false);
static DIPPING: AtomicBool = AtomicBool::new(false);
static READING: AtomicBool = AtomicBool::new(false);
static APP: OnceLock<AppHandle> = OnceLock::new();
static SERVICES: Once = Once::new();

fn pose(app: &AppHandle, bird: Image<'static>) {
    let handle = app.clone();
    report(
        app.run_on_main_thread(move || {
            if let Some(tray) = handle.tray_by_id("ibis") {
                report(
                    tray.set_icon_with_as_template(
                        Some(if HOVERING.load(Ordering::SeqCst) { RAISED } else { bird }),
                        true,
                    ),
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

#[derive(Debug, PartialEq, Eq)]
enum Drop {
    Library(std::path::PathBuf),
    Search(String),
}

/// File representations take precedence: an unsupported or malformed file URL
/// must never become a search for its pasteboard's incidental string.
fn read_drop(file: bool, path: Option<std::path::PathBuf>, text: Option<&str>) -> Option<Drop> {
    if file {
        return path.filter(|path| accepts_library_file(path)).map(Drop::Library);
    }
    let query = search_query(text?);
    (!query.is_empty()).then_some(Drop::Search(query))
}

fn search_query(text: &str) -> String {
    text.trim()
        .replace(['\r', '\n'], "\t")
        .chars()
        .take(SEARCH_LIMIT)
        .collect()
}

fn normalize_prefix(text: &[u16]) -> String {
    search_query(&String::from_utf16_lossy(&text[..text.len().min(TEXT_PREFIX)]))
}

fn native_search_query(text: &NSString) -> String {
    let len = text.length().min(TEXT_PREFIX);
    let mut prefix = [0u16; TEXT_PREFIX];
    if len > 0 {
        // SAFETY: The range is within the NSString's UTF-16 length and the
        // buffer holds at least `len` UTF-16 units. Neither escapes this call.
        unsafe { text.getCharacters_range(std::ptr::NonNull::from(&mut prefix[0]), NSRange::new(0, len)) };
    }
    normalize_prefix(&prefix[..len])
}

fn dragged(sender: &ProtocolObject<dyn NSDraggingInfo>) -> Option<Drop> {
    let pasteboard = sender.draggingPasteboard();
    // SAFETY: Shared AppKit constants, read only on AppKit's main-thread callbacks.
    let (file_kind, text_kind) = unsafe { (NSPasteboardTypeFileURL, NSPasteboardTypeString) };
    let file = pasteboard.types().is_some_and(|types| types.containsObject(file_kind));
    let path = file
        .then(|| {
            pasteboard
                .stringForType(file_kind)
                .and_then(|url| NSURL::URLWithString(&url)?.filePathURL()?.path())
                .map(|path| std::path::PathBuf::from(path.to_string()))
        })
        .flatten();
    let text = if file {
        None
    } else {
        pasteboard
            .stringForType(text_kind)
            .map(|text| native_search_query(&text))
    };
    read_drop(file, path, text.as_deref())
}

fn hover_drag(sender: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
    let count = sender.draggingPasteboard().changeCount();
    let readable = HOVER_CACHE.with(|cache| {
        if let Some((cached_count, readable)) = cache.get()
            && cached_count == count
        {
            return readable;
        }
        let readable = dragged(sender).is_some();
        cache.set(Some((count, readable)));
        readable
    });
    hover(readable)
}

fn hover(readable: bool) -> NSDragOperation {
    if HOVERING.swap(readable, Ordering::SeqCst) != readable
        && let Some(app) = APP.get()
    {
        pose(app, resting());
    }
    if readable {
        NSDragOperation::Copy
    } else {
        NSDragOperation::None
    }
}

/// Services and drops share the picker's existing queued search delivery.
fn search(app: &AppHandle, query: String) {
    if let Ok(mut waiting) = app.state::<SearchRequest>().0.lock() {
        *waiting = Some(query);
    }
    crate::picker::show_picker(app);
    report(app.emit_to("main", "picker:search-requested", ()), NativeIssue::Notice);
}

/// The button is recreated after hiding. Set its spoken title without changing
/// its visible title (which would put words in the menu bar).
pub(crate) fn spoken_state<R: Runtime>(tray: &TrayIcon<R>, title: String) {
    report(
        tray.with_inner_tray_icon(move |inner| {
            let Some(main_thread) = MainThreadMarker::new() else {
                return;
            };
            if let Some(button) = inner.ns_status_item().and_then(|item| item.button(main_thread)) {
                button.setAccessibilityTitle(Some(&NSString::from_str(&title)));
            }
        }),
        NativeIssue::TrayCreation,
    );
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

    // SAFETY: All drag selectors match NSDraggingDestination exactly, including
    // nullable exit senders. AppKit calls this MainThreadOnly object on the main
    // thread; no sender, pasteboard or borrowed Objective-C value escapes.
    unsafe impl NSDraggingDestination for MenuBarTarget {
        #[unsafe(method(draggingEntered:))]
        fn entered(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
            HOVER_CACHE.with(|cache| cache.set(None));
            hover_drag(sender)
        }

        #[unsafe(method(draggingUpdated:))]
        fn updated(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
            hover_drag(sender)
        }

        #[unsafe(method(draggingExited:))]
        fn exited(&self, _sender: Option<&ProtocolObject<dyn NSDraggingInfo>>) {
            HOVER_CACHE.with(|cache| cache.set(None));
            hover(false);
        }

        #[unsafe(method(draggingEnded:))]
        fn ended(&self, _sender: &ProtocolObject<dyn NSDraggingInfo>) {
            HOVER_CACHE.with(|cache| cache.set(None));
            hover(false);
        }

        #[unsafe(method(performDragOperation:))]
        fn perform(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
            HOVER_CACHE.with(|cache| cache.set(None));
            let dropped = dragged(sender).zip(APP.get());
            hover(false);
            let accepted = dropped.is_some();
            if let Some((drop, app)) = dropped {
                match drop {
                    Drop::Library(path) => {
                        start_open(app, Source::File(path), true);
                        crate::picker::reveal_picker(app);
                    }
                    Drop::Search(query) => search(app, query),
                }
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
                search(app, search_query(&text));
            }
        }
    }
);

thread_local! {
    static HOVER_CACHE: std::cell::Cell<Option<(isize, bool)>> = const { std::cell::Cell::new(None) };
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
    HOVERING.store(false, Ordering::SeqCst);
    if let Some(app) = APP.get() {
        pose(app, resting());
    }
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
            // AppKit throws on a second registration, and showing the bird calls this again.
            SERVICES.call_once(|| {
                // SAFETY: The target lives for the whole app, on the main thread.
                unsafe { NSRegisterServicesProvider(Some(&target), &NSString::from_str("Ibis")) };
            });
            let window = inner
                .ns_status_item()
                .and_then(|item| item.button(main_thread))
                .and_then(|button| button.window());
            if let Some(window) = window {
                // SAFETY: A shared AppKit constant, read on the main thread.
                window.registerForDraggedTypes(&NSArray::from_slice(&unsafe {
                    [NSPasteboardTypeFileURL, NSPasteboardTypeString]
                }));
                window.setDelegate(Some(ProtocolObject::from_ref(&*target)));
            }
        }),
        NativeIssue::TrayCreation,
    );
    if let Some(app) = APP.get() {
        spoken_state(tray, crate::tray::spoken_title(&crate::library::current_status(app)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_is_normalized_only_within_the_utf16_prefix() {
        let mut text = vec![u16::from(b' '); TEXT_PREFIX];
        text.extend("hidden query".encode_utf16());
        text.extend(std::iter::repeat_n(u16::from(b'x'), 1_000_000));
        assert_eq!(normalize_prefix(&text), "");
        text[0] = u16::from(b'a');
        text[1] = u16::from(b'\n');
        text[2] = u16::from(b'b');
        assert_eq!(normalize_prefix(&text), "a\tb");
    }

    #[test]
    fn menu_bar_text_searches_but_an_unreadable_file_never_becomes_a_text_search() {
        assert_eq!(
            read_drop(false, None, Some("  invented\nquery  ")),
            Some(Drop::Search("invented\tquery".into()))
        );
        assert_eq!(
            read_drop(true, Some("paper.pdf".into()), Some("file:///paper.pdf")),
            None
        );
        assert_eq!(read_drop(true, None, Some("unreadable file URL")), None);
        assert_eq!(
            read_drop(true, Some("thesis.bib".into()), None),
            Some(Drop::Library("thesis.bib".into()))
        );
        assert_eq!(read_drop(false, None, Some(" \n ")), None);
        assert_eq!(read_drop(false, None, None), None);
    }
}
