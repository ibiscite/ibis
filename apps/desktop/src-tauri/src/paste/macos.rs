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

pub(super) mod diagnostics;
mod focus;
#[allow(unsafe_code)]
mod safety;
#[allow(unsafe_code)]
mod secure_input;

pub(super) fn stamp() -> super::SafetyStamp {
    safety::stamp()
}

pub(super) fn observe_write_stamp() -> (super::SafetyStamp, Option<diagnostics::Snapshot>) {
    let trace = diagnostics::Trace::new(None);
    let stamp = trace.with(|| {
        diagnostics::stage("lifecycle.write");
        safety::stamp()
    });
    (stamp, trace.snapshot())
}

pub(super) fn accessibility_granted() -> bool {
    focus::trusted()
}

pub(super) fn request_accessibility() {
    focus::request_trust();
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
pub(super) fn frontmost(app: &AppHandle) -> Option<Front> {
    let own = i32::try_from(std::process::id()).ok()?;
    let main = tauri_nspanel::objc2::MainThreadMarker::new()?;
    let own_window_key = NSApplication::sharedApplication(main)
        .keyWindow()
        .is_some_and(|window| window.isKeyWindow());
    let active = active_app();
    let activation_allowed = !own_window_key
        && active
            .as_ref()
            .is_some_and(|front| front.pid != own && manual_accessibility_allowed(app, front));
    capture_front(active, own, own_window_key, |pid| {
        let trace = diagnostics::Trace::new(None);
        // A grant acquired later cannot authorize this earlier presentation.
        let result = trace.with(|| {
            diagnostics::stage("capture.post_event");
            if !CGPreflightPostEventAccess() {
                return Err(CopyReason::NoPermission);
            }
            focus::capture(pid, activation_allowed)
        });
        trace.remember_capture();
        result
    })
}

fn manual_accessibility_allowed(app: &AppHandle, front: &Front) -> bool {
    use ibis_core::destination::{AppId, System};
    let Some(id) = &front.id else { return false };
    let id = AppId {
        system: System::Macos,
        id: id.clone(),
    };
    let Ok(preference) = app.state::<crate::destination::AppMemory>().paste(&id) else {
        return false;
    };
    let settings = ibis_core::settings::load(&app.state::<crate::preferences::SettingsFile>().path);
    if settings.problem.is_some() {
        return false;
    }
    manual_accessibility_policy(front, id, preference, settings.settings.paste.unwrap_or(true))
}

fn manual_accessibility_policy(
    front: &Front,
    id: ibis_core::destination::AppId,
    preference: Option<ibis_core::destination::AppPaste>,
    enabled: bool,
) -> bool {
    use ibis_core::destination::{Decision, PastePermission, System, Target, decide};
    // Activation follows paste policy, including explicit approval of unknown
    // apps. Refused classes, Copy only and paste-off never enable a provider.
    decide(
        &Target {
            system: System::Macos,
            app: Some(id),
            secure_field: false,
            elevated: front.elevated,
        },
        PastePermission {
            enabled,
            granted: true,
            app: preference,
        },
    ) == Decision::Paste
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
    trace: diagnostics::Trace,
    started: Duration,
    stamp_unavailable: bool,
}

impl Mac {
    pub(super) fn new(app: &AppHandle, proof: Option<&CopyProof>) -> Self {
        Self {
            app: app.clone(),
            trace: diagnostics::Trace::new(proof.and_then(|p| p.captured.as_ref()?.diagnostic.clone())),
            started: safety::elapsed(),
            stamp_unavailable: proof.is_some_and(|proof| proof.stamp.epoch == 0 || proof.stamp.epoch == u64::MAX),
        }
    }

    pub(super) fn complete(&self, outcome: &super::Delivered) {
        use super::Delivered;
        match outcome {
            Delivered::Pasted { .. } => self.trace.stage("posted"),
            Delivered::Copied { reason } => {
                let stage = match reason.as_deref() {
                    Some("accessibility-required") => Some("permission.accessibility"),
                    Some("no-permission") => Some("permission.post_event"),
                    Some("paste-off") => Some("policy.paste_off"),
                    Some("copy-only") => Some("policy.copy_only"),
                    Some("terminal") => Some("policy.terminal"),
                    Some("remote-desktop") => Some("policy.remote_desktop"),
                    Some("virtual-machine") => Some("policy.virtual_machine"),
                    Some("nonstandard-shortcut") => Some("policy.nonstandard_shortcut"),
                    Some("unknown-app") => Some("policy.unknown_app"),
                    Some("no-target") => Some("policy.no_target"),
                    Some("secure-input") => Some("secure_input"),
                    Some("clipboard-changed") => Some("clipboard"),
                    _ => None, // Keep the detailed AX/readiness/lifecycle failure stage.
                };
                if let Some(stage) = stage {
                    self.trace.stage(stage);
                }
            }
        }
        self.trace.finish(safety::elapsed().saturating_sub(self.started));
    }

    /// AppKit state and layouts are read on the main thread; the paste waits
    /// off it, never longer than a check step allows.
    fn on_main<T: Send + 'static>(&self, work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
        self.on_main_for(ASK_LIMIT, work)
    }

    fn on_main_for<T: Send + 'static>(&self, limit: Duration, work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
        let (sender, receiver) = mpsc::channel();
        let trace = self.trace.clone();
        self.app
            .run_on_main_thread(move || {
                let _ = sender.send(trace.with(work));
            })
            .ok()?;
        receiver.recv_timeout(limit).ok()
    }
}

impl Desk for Mac {
    type Proof = CopyProof;

    fn stamp(&self) -> super::SafetyStamp {
        // A failed pre-write sample is immutable evidence. Do not overwrite its
        // diagnostic with a later good sample while checking the same refusal.
        if self.stamp_unavailable {
            return safety::stamp();
        }
        self.trace.with(|| {
            diagnostics::stage("lifecycle");
            safety::stamp()
        })
    }

    fn may_post(&self) -> bool {
        CGPreflightPostEventAccess()
    }

    fn ask_to_post(&self) {
        if !ASKED.swap(true, Ordering::SeqCst) {
            crate::settings_window::open(&self.app, Some(crate::settings_window::Page::Paste));
        }
    }

    fn stage(&self, stage: &'static str) {
        self.trace.stage(stage);
    }

    fn settle(&self) {}

    fn frontmost(&self) -> Result<Option<Front>, CopyReason> {
        // The shared app query is activation only. macOS ready() additionally
        // proves the original document/surface; finish() repeats the full guards.
        self.on_main(active_app).ok_or(CopyReason::PasteUnavailable)
    }

    fn ready(&self, request: &DeliveryRequest, proof: &CopyProof) -> Result<bool, CopyReason> {
        let request = PostRequest::new(
            DeliveryRequest {
                pid: request.pid,
                id: request.id.clone(),
                deadline: request.deadline,
            },
            proof,
        )
        .queued(Instant::now());
        let limit = ASK_LIMIT.min(request.deadline.saturating_duration_since(Instant::now()));
        let app = self.app.clone();
        self.on_main_for(limit, move || {
            if Instant::now() >= request.deadline {
                return Err(CopyReason::PasteUnavailable);
            }
            let owns = || app.state::<super::LastTarget>().owns(&request);
            let hidden = || {
                app.get_webview_window("main")
                    .is_some_and(|window| window.is_visible().ok() == Some(false))
            };
            if !owns() {
                return Err(CopyReason::TargetChanged);
            }
            if !hidden() {
                return Ok(false);
            }
            request
                .stamp
                .check(safety::stamp())
                .map_err(|failure| failure.reason())?;
            let Some(front) = active_app() else { return Ok(false) };
            let matches = front.pid == request.pid && front.id.as_deref() == Some(&request.id);
            diagnostics::owner(matches);
            if !matches {
                return Ok(false);
            }
            let probe = safety::PostProbe::new().map_err(|failure| failure.reason())?;
            let current = focus::current(
                request.pid,
                request.window,
                request.deadline,
                || probe.before_focus().map_err(|failure| failure.reason()),
                || {
                    request
                        .stamp
                        .check(probe.finish().map_err(|failure| failure.reason())?)
                        .map_err(|failure| failure.reason())
                },
            );
            if !owns() {
                return Err(CopyReason::TargetChanged);
            }
            if !hidden() {
                return Ok(false);
            }
            // Only a focus mismatch is pending. AX failures retain their reason;
            // a replaced document/sheet is never captured or adopted here.
            match current {
                Ok(token) => Ok(token.is_some() && token == request.window),
                Err(CopyReason::TargetChanged) => Ok(false),
                Err(reason) => Err(reason),
            }
        })
        .ok_or(CopyReason::PasteUnavailable)?
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
        diagnostics::stage("permission.post_event");
        CGPreflightPostEventAccess()
    }
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn stamp(&self) -> super::SafetyStamp {
        diagnostics::stage("lifecycle");
        super::SafetyStamp::now()
    }

    fn current(&self, _presentation: u64) -> bool {
        diagnostics::stage("presentation");
        // The caller holds LastTarget's ownership lock through checked_post.
        self.app
            .get_webview_window("main")
            .is_some_and(|window| window.is_visible().ok() == Some(false))
    }

    fn frontmost(&self, refresh: impl FnOnce() -> Result<(), CopyReason>) -> Result<Option<Front>, CopyReason> {
        let Some(mut front) = active_app() else {
            diagnostics::owner(false);
            return Ok(None);
        };
        let matches = front.pid == self.request.pid && front.id.as_deref() == Some(&self.request.id);
        diagnostics::owner(matches);
        if !matches {
            return Err(CopyReason::TargetChanged);
        }
        let probe = safety::PostProbe::new().map_err(|failure| failure.reason())?;
        front.window = focus::current(
            front.pid,
            self.request.window,
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
        diagnostics::stage("clipboard");
        NSPasteboard::generalPasteboard().changeCount()
    }

    fn secure_input(&self) -> Option<bool> {
        diagnostics::stage("secure_input");
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

    #[test]
    fn manual_accessibility_runs_by_default_only_for_apps_allowed_to_paste() {
        use ibis_core::destination::{AppId, AppPaste, System};
        let front = Front {
            id: None,
            pid: 42,
            window: None,
            focus_failure: None,
            name: None,
            elevated: false,
        };
        for id in [
            "com.apple.terminal",
            "com.microsoft.rdc.macos",
            "com.parallels.desktop.console",
        ] {
            assert!(!manual_accessibility_policy(
                &front,
                AppId {
                    system: System::Macos,
                    id: id.into()
                },
                Some(AppPaste::Paste),
                true
            ));
        }
        let chrome = || AppId {
            system: System::Macos,
            id: "com.google.chrome".into(),
        };
        assert!(manual_accessibility_policy(
            &front,
            AppId {
                system: System::Macos,
                id: "com.anthropic.claudefordesktop".into()
            },
            None,
            true
        ));
        let unknown = || AppId {
            system: System::Macos,
            id: "org.example.unknown".into(),
        };
        assert!(manual_accessibility_policy(
            &front,
            unknown(),
            Some(AppPaste::Paste),
            true
        ));
        assert!(!manual_accessibility_policy(&front, unknown(), None, true));
        assert!(manual_accessibility_policy(&front, chrome(), None, true));
        assert!(!manual_accessibility_policy(
            &front,
            chrome(),
            Some(AppPaste::CopyOnly),
            true
        ));
        assert!(!manual_accessibility_policy(&front, chrome(), None, false));
    }
}
