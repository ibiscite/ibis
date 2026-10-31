//! The macOS calls behind a paste. The identity-only AX, lifecycle and
//! secure-input FFI are isolated; other calls use safe objc2 bindings.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use ibis_core::destination::CopyReason;
use objc2_app_kit::{NSApplication, NSPasteboard, NSWorkspace};
use objc2_core_graphics::{CGEvent, CGEventFlags, CGEventSource, CGEventSourceStateID, CGPreflightPostEventAccess};
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::Code;

use super::capture::Front;
use super::{CopyProof, DeliveryRequest, Desk, PendingPost, PostDesk, PostRequest, PostResult, checked_post};

mod focus;
#[allow(unsafe_code)]
mod safety;
#[allow(unsafe_code)]
mod secure_input;

pub(super) fn stamp() -> super::SafetyStamp {
    safety::stamp()
}

pub(super) fn accessibility_granted() -> bool {
    focus::trusted()
}

const ASK_LIMIT: Duration = Duration::from_millis(250);

/// The active app for output suggestions. Activation is not keyboard ownership.
fn active_app() -> Option<Front> {
    safety::install();
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    if !app.isActive() || app.isTerminated() {
        return None;
    }
    Some(Front {
        id: app.bundleIdentifier().map(|id| id.to_string()),
        pid: app.processIdentifier(),
        window: None,
        focus_failure: None,
        name: app.localizedName().map(|name| name.to_string()),
        elevated: false,
    })
}

/// Capture only a globally verified window owned by the active target. Ibis in
/// front keeps the old capture, so showing another Ibis window cannot replace it.
pub(super) fn frontmost() -> Option<Front> {
    let own = i32::try_from(std::process::id()).ok()?;
    let main = tauri_nspanel::objc2::MainThreadMarker::new()?;
    let own_window_key = NSApplication::sharedApplication(main)
        .keyWindow()
        .is_some_and(|window| window.isKeyWindow());
    capture_front(active_app(), own, own_window_key, |pid| {
        // A grant acquired later cannot authorize this earlier presentation.
        if !CGPreflightPostEventAccess() {
            return Err(CopyReason::NoPermission);
        }
        focus::capture(pid)
    })
}

pub(super) fn capture_front(
    active: Option<Front>,
    own: i32,
    own_window_key: bool,
    capture_window: impl FnOnce(i32) -> Result<u32, CopyReason>,
) -> Option<Front> {
    if own_window_key {
        // Ibis is itself nonactivating: NSWorkspace can still name the prior
        // external app. Return our PID so capture keeps that earlier target,
        // without reserving/clearing a new AX identity for our own picker.
        return Some(Front {
            id: None,
            pid: own,
            window: None,
            focus_failure: None,
            name: None,
            elevated: false,
        });
    }
    let mut front = active?;
    if front.pid != own {
        match capture_window(front.pid) {
            Ok(window) => {
                front.window = Some(window);
                front.focus_failure = None;
            }
            Err(reason) => {
                front.window = None;
                // No surface was ever proved. Keep actionable grant failures,
                // but do not call an unproven capture a changed target.
                front.focus_failure = Some(match reason {
                    CopyReason::NoPermission | CopyReason::AccessibilityRequired => reason,
                    _ => CopyReason::PasteUnavailable,
                });
            }
        }
    }
    Some(front)
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
        // Waiting for activation is not a focus proof; the bounded identity
        // query belongs to the final main-thread posting task.
        self.on_main(active_app).ok_or(CopyReason::PasteUnavailable)
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
                            request: &request,
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
    request: &'a PostRequest,
}

impl PostDesk for Ready<'_> {
    fn post_event_granted(&self) -> bool {
        CGPreflightPostEventAccess()
    }
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

    fn frontmost(&self, refresh: impl FnOnce() -> Result<(), CopyReason>) -> Result<Option<Front>, CopyReason> {
        let Some(mut front) = active_app() else {
            return Ok(None);
        };
        let probe = safety::PostProbe::new().map_err(|failure| failure.reason())?;
        front.window = focus::current(
            front.pid,
            self.request.deadline,
            || {
                probe.before_focus().map_err(|failure| failure.reason())?;
                refresh()
            },
            || {
                // A sleep/lock during AX must not escape expiry. This completion
                // guard uses no mutex; identity comparison immediately follows it.
                let stamp = probe.finish().map_err(|failure| failure.reason())?;
                self.request.stamp.check(stamp).map_err(|failure| failure.reason())?;
                if Instant::now() >= self.request.deadline {
                    return Err(CopyReason::PasteUnavailable);
                }
                Ok(())
            },
        )?;
        Ok(Some(front))
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

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::destination::System;
    use std::cell::Cell;

    #[test]
    fn repeated_show_while_the_ibis_panel_is_key_preserves_the_external_capture_and_identity_bank() {
        let external = Front {
            id: Some("com.microsoft.word".into()),
            pid: 42,
            window: Some(100),
            focus_failure: None,
            name: Some("Writer".into()),
            elevated: false,
        };
        let bank_kept = Cell::new(true);
        // NSWorkspace still names Word; the key NSPanel belongs to Ibis.
        let front = capture_front(Some(external), 7, true, |_| {
            bank_kept.set(false);
            Err(CopyReason::PasteUnavailable)
        });
        assert_eq!(
            super::super::capture::captured(System::Macos, front, 7),
            None,
            "an own key window must keep LastTarget's prior app/window"
        );
        assert!(bank_kept.get(), "a repeated show must not clear the AX identity bank");
    }

    #[test]
    fn a_failed_capture_cannot_claim_an_unproven_target_changed() {
        let active = Front {
            id: Some("com.microsoft.word".into()),
            pid: 42,
            window: None,
            focus_failure: None,
            name: Some("Writer".into()),
            elevated: false,
        };
        let front = capture_front(Some(active), 7, false, |_| Err(CopyReason::TargetChanged));
        let captured = super::super::capture::captured(System::Macos, front, 7).unwrap_or_else(|| panic!("capture"));
        assert_eq!(captured.window, None);
        assert_eq!(captured.focus_failure, Some(CopyReason::PasteUnavailable));
    }
}
