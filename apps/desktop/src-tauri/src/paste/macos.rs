//! The macOS calls behind a paste. The secure-input FFI is isolated below;
//! other native calls use safe objc2 bindings.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use objc2_app_kit::NSWorkspace;
use objc2_core_graphics::{CGEvent, CGEventFlags, CGEventSource, CGEventSourceStateID, CGPreflightPostEventAccess};
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::Code;

use super::Desk;
use super::capture::Front;

#[allow(unsafe_code)]
mod secure_input;

const ASK_LIMIT: Duration = Duration::from_millis(250);

/// At most one AX query can outlive its caller. A stuck provider must not
/// accumulate threads when the person tries another paste.
static CHECKING: AtomicBool = AtomicBool::new(false);

struct Checking;

impl Drop for Checking {
    fn drop(&mut self) {
        CHECKING.store(false, Ordering::SeqCst);
    }
}

/// The app in front, by bundle ID, process and name; never its windows.
pub(super) fn frontmost() -> Option<Front> {
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    Some(Front {
        id: app.bundleIdentifier().map(|id| id.to_string()),
        pid: app.processIdentifier(),
        name: app.localizedName().map(|name| name.to_string()),
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
        self.on_main_for(ASK_LIMIT, work)
    }

    fn on_main_for<T: Send + 'static>(&self, limit: Duration, work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
        let (sender, receiver) = mpsc::channel();
        self.app
            .run_on_main_thread(move || {
                let _ = sender.send(work());
            })
            .ok()?;
        receiver.recv_timeout(limit).ok()
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

    fn clipboard(&self) -> Option<String> {
        self.app.clipboard().read_text().ok()
    }

    fn secure_field(&self, pid: i32) -> Option<bool> {
        let deadline = Instant::now() + ASK_LIMIT;
        // Carbon documents this flag as not thread safe. Only this quick
        // local read runs on the main thread; AX messaging never does.
        if self.on_main_for(
            deadline.saturating_duration_since(Instant::now()),
            secure_input::enabled,
        )?? {
            return Some(true);
        }
        CHECKING
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()?;
        let checking = Checking;
        let secure = super::bounded(deadline.saturating_duration_since(Instant::now()), move || {
            let _checking = checking;
            super::security::secure_field(&secure_input::Ax { deadline }, pid)
        })??;
        if secure {
            return Some(true);
        }
        // Secure Event Input may have been enabled while the AX provider
        // answered. Do not paste on a stale negative result.
        let enabled = self.on_main_for(
            deadline.saturating_duration_since(Instant::now()),
            secure_input::enabled,
        )??;
        (Instant::now() < deadline).then_some(enabled)
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
