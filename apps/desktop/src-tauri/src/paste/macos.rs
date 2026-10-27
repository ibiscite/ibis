//! The macOS calls behind a paste, through safe objc2 bindings only.

use std::sync::mpsc;
use std::time::Duration;

use objc2_app_kit::NSWorkspace;
use objc2_core_graphics::{
    CGEvent, CGEventFlags, CGEventSource, CGEventSourceStateID, CGPreflightPostEventAccess, CGRequestPostEventAccess,
};
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::Code;

use super::Desk;
use super::capture::Front;

/// The app in front, by bundle ID, process and name; never its windows.
pub(super) fn frontmost() -> Option<Front> {
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    Some(Front {
        id: app.bundleIdentifier().map(|id| id.to_string()),
        pid: app.processIdentifier(),
        name: app.localizedName().map(|name| name.to_string()),
    })
}

pub(super) struct Mac(pub(super) AppHandle);

impl Mac {
    /// AppKit state and layouts are read on the main thread; the paste waits off it.
    fn on_main<T: Send + 'static>(&self, work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
        let (sender, receiver) = mpsc::channel();
        self.0
            .run_on_main_thread(move || {
                let _ = sender.send(work());
            })
            .ok()?;
        receiver.recv_timeout(Duration::from_secs(2)).ok()
    }
}

impl Desk for Mac {
    fn may_post(&self) -> bool {
        CGPreflightPostEventAccess()
    }

    fn ask_to_post(&self) {
        CGRequestPostEventAccess();
    }

    fn frontmost(&self) -> Option<i32> {
        self.on_main(|| frontmost().map(|front| front.pid)).flatten()
    }

    fn clipboard(&self) -> Option<String> {
        self.0.clipboard().read_text().ok()
    }

    fn paste_key(&self) -> Option<u16> {
        self.on_main(|| crate::picker::command_key(Code::KeyV, crate::picker::typing()))
            .flatten()
    }

    fn post(&self, pid: i32, key: u16) -> bool {
        // A source of its own, so keys the person still holds are not added.
        let source = CGEventSource::new(CGEventSourceStateID::Private);
        let events = [true, false].map(|down| CGEvent::new_keyboard_event(source.as_deref(), key, down));
        let [Some(down), Some(up)] = events else {
            return false;
        };
        for event in [&down, &up] {
            CGEvent::set_flags(Some(event), CGEventFlags::MaskCommand);
            CGEvent::post_to_pid(pid, Some(event));
        }
        true
    }

    fn wait(&self, time: Duration) {
        std::thread::sleep(time);
    }
}
