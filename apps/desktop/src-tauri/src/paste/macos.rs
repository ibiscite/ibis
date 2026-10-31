//! The macOS calls behind a paste. The secure-input FFI is isolated below;
//! other native calls use safe objc2 bindings.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use ibis_core::destination::CopyReason;
use objc2_app_kit::{NSPasteboard, NSWorkspace};
use objc2_core_graphics::{CGEvent, CGEventFlags, CGEventSource, CGEventSourceStateID, CGPreflightPostEventAccess};
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::Code;

use super::capture::Front;
use super::{CopyProof, DeliveryRequest, Desk, PendingPost, PostDesk, PostRequest, PostResult, checked_post};

#[allow(unsafe_code)]
mod safety;
#[allow(unsafe_code)]
mod secure_input;

pub(super) fn stamp() -> super::SafetyStamp {
    safety::stamp()
}

const ASK_LIMIT: Duration = Duration::from_millis(250);

/// The active app for output suggestions. Activation is not keyboard ownership.
pub(super) fn frontmost() -> Option<Front> {
    safety::install();
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    if !app.isActive() || app.isTerminated() {
        return None;
    }
    Some(Front {
        id: app.bundleIdentifier().map(|id| id.to_string()),
        pid: app.processIdentifier(),
        // PostEvent-only public metadata cannot identify the globally focused
        // window, especially with a foreign nonactivating panel. Do not infer
        // focus from activation/z-order. AX is forbidden by ADR 0029/#1241;
        // keep delivery unavailable pending an explicit maintainer decision.
        window: None,
        name: app.localizedName().map(|name| name.to_string()),
        elevated: false,
    })
}

/// Settings opens on the permission guide at most once per run.
static ASKED: AtomicBool = AtomicBool::new(false);

pub(super) struct Mac {
    app: AppHandle,
}

impl Mac {
    pub(super) fn new(app: &AppHandle) -> Self {
        Self { app: app.clone() }
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
    type Proof = CopyProof;

    fn may_post(&self) -> bool {
        CGPreflightPostEventAccess()
    }

    fn ask_to_post(&self) {
        if !ASKED.swap(true, Ordering::SeqCst) {
            crate::settings_window::open(&self.app, Some(crate::settings_window::Page::Paste));
        }
    }

    fn frontmost(&self) -> Result<Option<Front>, CopyReason> {
        self.on_main(frontmost).ok_or(CopyReason::PasteUnavailable)
    }

    fn finish(&self, request: DeliveryRequest, proof: &CopyProof) -> PostResult {
        // This deadline belongs to the queued task too, not just its caller.
        let request = PostRequest::new(request, proof).queued(Instant::now());
        let limit = request.deadline.saturating_duration_since(Instant::now());
        let pending = Arc::new(PendingPost::default());
        let queued = Arc::clone(&pending);
        let app = self.app.clone();
        self.on_main_for(limit, move || {
            if Instant::now() >= request.deadline {
                return PostResult::Unavailable;
            }
            // Resolve the layout and create the events before the final
            // guards. No separate main-thread wait may follow the flag.
            let Some(key) = crate::picker::command_key(Code::KeyV, crate::picker::typing()) else {
                return PostResult::Refused;
            };
            let source = CGEventSource::new(CGEventSourceStateID::Private);
            let events = [true, false].map(|down| CGEvent::new_keyboard_event(source.as_deref(), key, down));
            let [Some(down), Some(up)] = events else {
                return PostResult::Refused;
            };
            for event in [&down, &up] {
                CGEvent::set_flags(Some(event), CGEventFlags::MaskCommand);
            }
            queued.run(|| {
                app.state::<super::LastTarget>().post_if_current(&request, || {
                    checked_post(
                        &Ready {
                            app: &app,
                            down: &down,
                            up: &up,
                        },
                        &request,
                    )
                })
            })
        })
        .unwrap_or_else(|| pending.cancel())
    }

    fn wait(&self, time: Duration) {
        std::thread::sleep(time);
    }
}

/// Constructed and used only inside the final main-thread task.
struct Ready<'a> {
    app: &'a AppHandle,
    down: &'a CGEvent,
    up: &'a CGEvent,
}

impl PostDesk for Ready<'_> {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn stamp(&self) -> super::SafetyStamp {
        super::SafetyStamp::now()
    }

    fn current(&self, _presentation: u64) -> bool {
        // The caller holds LastTarget's ownership lock through checked_post.
        self.app
            .get_webview_window("main")
            .is_some_and(|window| window.is_visible().ok() == Some(false))
    }

    fn frontmost(&self) -> Option<Front> {
        frontmost()
    }

    fn change_count(&self) -> isize {
        NSPasteboard::generalPasteboard().changeCount()
    }

    fn secure_input(&self) -> Option<bool> {
        secure_input::enabled()
    }

    fn post(&self, pid: i32) -> bool {
        for event in [self.down, self.up] {
            CGEvent::post_to_pid(pid, Some(event));
        }
        true
    }
}
