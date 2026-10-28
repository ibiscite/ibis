//! The macOS calls behind a paste, through safe objc2 bindings only.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use objc2_app_kit::NSWorkspace;
use objc2_core_graphics::{CGEvent, CGEventFlags, CGEventSource, CGEventSourceStateID, CGPreflightPostEventAccess};
use tauri::{AppHandle, Manager};
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
        elevated: false,
    })
}

/// Settings opens on the permission guide at most once per run.
static ASKED: AtomicBool = AtomicBool::new(false);

pub(super) struct Mac {
    app: AppHandle,
    /// The picker showing that this copy belongs to; showing it again stops the paste.
    generation: u64,
}

impl Mac {
    pub(super) fn new(app: &AppHandle) -> Self {
        Self {
            app: app.clone(),
            generation: app.state::<crate::picker::PickerGeneration>().current(),
        }
    }

    fn picker_unchanged(&self) -> bool {
        self.app.state::<crate::picker::PickerGeneration>().current() == self.generation
    }

    /// AppKit state and layouts are read on the main thread; the paste waits
    /// off it, never longer than a check step allows.
    fn on_main<T: Send + 'static>(&self, work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
        let (sender, receiver) = mpsc::channel();
        self.app
            .run_on_main_thread(move || {
                let _ = sender.send(work());
            })
            .ok()?;
        receiver.recv_timeout(Duration::from_millis(250)).ok()
    }
}

impl Desk for Mac {
    fn may_post(&self) -> bool {
        CGPreflightPostEventAccess()
    }

    fn ask_to_post(&self) {
        if !ASKED.swap(true, Ordering::SeqCst) {
            crate::settings_window::open(&self.app, Some(crate::settings_window::Page::Paste));
        }
    }

    fn frontmost(&self) -> Option<Front> {
        if !self.picker_unchanged() {
            return None;
        }
        self.on_main(frontmost).flatten()
    }

    fn clipboard_matches(&self, text: &str) -> bool {
        self.app.clipboard().read_text().ok().as_deref() == Some(text)
    }

    fn secure_field(&self) -> Option<bool> {
        Some(false)
    }

    fn paste_key(&self) -> Option<u16> {
        self.on_main(|| crate::picker::command_key(Code::KeyV, crate::picker::typing()))
            .flatten()
    }

    fn post(&self, pid: i32, key: u16) -> bool {
        if !self.picker_unchanged() {
            return false;
        }
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
