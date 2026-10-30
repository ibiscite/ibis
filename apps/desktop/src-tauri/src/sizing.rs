//! One steady picker frame, restored before presentation (design C10).
//! Content never crosses this boundary. Native events read live geometry;
//! only explicit drags are remembered, through the existing settings owner.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use ibis_core::settings::{PickerWindow, Settings};
use tauri::{AppHandle, Emitter, Manager, Monitor, WebviewWindow};

#[cfg(not(target_os = "macos"))]
use tauri::{LogicalSize, PhysicalPosition, Position};

use crate::diagnostics::{NativeIssue, checked, report};
use crate::placement;
use crate::preferences::SettingsFile;

const DEFAULT: (u32, u32) = (720, 480);
const MINIMUM: (u32, u32) = (480, 360);
// Kept equal to --larger-text-scale; the regression checks the shared token.
const LARGER_TEXT_SCALE: f64 = 1.15;

#[derive(Default)]
pub(crate) struct Sizing {
    state: Mutex<State>,
    revision: AtomicU64,
    applying: AtomicBool,
    larger_text: AtomicBool,
}

#[derive(Default)]
struct State {
    windows: BTreeMap<String, PickerWindow>,
    // Includes our pre-show frame, so its delayed events are never a drag.
    last: Option<(String, PickerWindow)>,
}

struct Save {
    revision: u64,
    windows: BTreeMap<String, PickerWindow>,
}

/// All dimensions are logical pixels on the target display. A work area smaller
/// than the minimum wins: no edge may end up behind the dock or menu bar.
fn plan(saved: Option<PickerWindow>, larger: bool, room: (f64, f64)) -> PickerWindow {
    let custom = saved.filter(|frame| frame.custom_size);
    let width = custom.map_or(DEFAULT.0, |frame| frame.width);
    let height = custom.map_or_else(
        || (f64::from(DEFAULT.1) * if larger { LARGER_TEXT_SCALE } else { 1.0 }).round() as u32,
        |frame| frame.height,
    );
    let width = width.max(MINIMUM.0).min(room.0.floor().max(1.0) as u32);
    let height = height.max(MINIMUM.1).min(room.1.floor().max(1.0) as u32);
    let spare_x = (room.0 - f64::from(width)).max(0.0);
    let spare_y = (room.1 - f64::from(height)).max(0.0);
    let (x, y) = placement::launcher_position(
        placement::Area {
            x: 0.0,
            y: 0.0,
            width: room.0,
            height: room.1,
        },
        f64::from(width),
        f64::from(height),
    );
    PickerWindow {
        width,
        height,
        custom_size: custom.is_some(),
        x: saved.map_or(x, |frame| f64::from(frame.x)).clamp(0.0, spare_x).floor() as i32,
        y: saved.map_or(y, |frame| f64::from(frame.y)).clamp(0.0, spare_y).floor() as i32,
    }
}

impl Sizing {
    pub(crate) fn load(&self, settings: &Settings) {
        if let Ok(mut state) = self.state.lock() {
            state.windows = settings.picker_windows.clone();
        }
        self.set_larger_text(settings.larger_text.unwrap_or(false));
    }

    /// An accepted text choice applies until quit even if its flush fails.
    /// Updating at acceptance, never completion, keeps delayed saves harmless.
    pub(crate) fn set_larger_text(&self, larger: bool) {
        self.larger_text.store(larger, Ordering::SeqCst);
    }

    fn remembered(&self, display: &str) -> Option<PickerWindow> {
        self.state.lock().ok()?.windows.get(display).copied()
    }

    fn snapshot(&self, state: &State) -> Save {
        Save {
            revision: self.revision.fetch_add(1, Ordering::SeqCst) + 1,
            windows: state.windows.clone(),
        }
    }

    fn record(&self, display: String, mut live: PickerWindow) -> Option<Save> {
        let mut state = self.state.lock().ok()?;
        let size_changed = state
            .last
            .as_ref()
            .is_none_or(|(_, frame)| (frame.width, frame.height) != (live.width, live.height));
        live.custom_size = size_changed || state.windows.get(&display).is_some_and(|frame| frame.custom_size);
        if state
            .last
            .as_ref()
            .is_some_and(|(key, frame)| *key == display && *frame == live)
        {
            return None;
        }
        state.last = Some((display.clone(), live));
        let mut remembered = live;
        if !size_changed && let Some(previous) = state.windows.get(&display) {
            // Moving a still-open frame across displays must not replace B's
            // custom size with the size restored for A at this opening.
            remembered.width = previous.width;
            remembered.height = previous.height;
        }
        state.windows.insert(display, remembered);
        Some(self.snapshot(&state))
    }

    fn forget(&self, display: &str) -> Option<Save> {
        let mut state = self.state.lock().ok()?;
        state.windows.remove(display);
        Some(self.snapshot(&state))
    }

    fn persist(&self, file: &SettingsFile, save: Save) -> Option<bool> {
        // Check after entering the settings writer, not before waiting for it.
        // A late A cannot write over B, including B from another display: every
        // snapshot contains all accepted drags. Never hold sizing across I/O.
        let current = || self.revision.load(Ordering::SeqCst) == save.revision;
        let result = file.update_when(current, |settings| settings.picker_windows = save.windows);
        if !current() || matches!(result, Ok(None)) {
            return None;
        }
        Some(result.is_ok())
    }
}

/// Tauri exposes names and coordinates, not stable hardware display IDs.
/// Use the name plus the origin to distinguish identically named displays;
/// resolution/work-area changes at that origin retain and clamp the frame.
fn display_key(monitor: &Monitor) -> String {
    let bounds = placement::display(monitor).bounds;
    format!(
        "{}@{},{}",
        monitor.name().map_or("display", String::as_str),
        bounds.x,
        bounds.y
    )
}

#[cfg(not(target_os = "macos"))]
fn position(monitor: &Monitor, frame: PickerWindow) -> Position {
    let usable = placement::display(monitor).usable;
    let scale = monitor.scale_factor();
    PhysicalPosition::new(
        (usable.x + f64::from(frame.x) * scale).round() as i32,
        (usable.y + f64::from(frame.y) * scale).round() as i32,
    )
    .into()
}

fn live_frame(window: &WebviewWindow, monitor: &Monitor) -> Option<PickerWindow> {
    let scale = checked(window.scale_factor(), NativeIssue::Size)?;
    let size = checked(window.inner_size(), NativeIssue::Size)?.to_logical::<f64>(scale);
    let at = placement::window_area(window)?;
    let usable = placement::display(monitor).usable;
    let unit = if cfg!(target_os = "macos") {
        1.0
    } else {
        monitor.scale_factor()
    };
    Some(PickerWindow {
        width: size.width.round() as u32,
        height: size.height.round() as u32,
        custom_size: false,
        x: ((at.x - usable.x) / unit).round() as i32,
        y: ((at.y - usable.y) / unit).round() as i32,
    })
}

#[cfg(any(target_os = "macos", test))]
fn appkit_top_left(current: (f64, f64), native: placement::Area, target: (f64, f64)) -> (f64, f64) {
    (
        native.x + target.0 - current.0,
        native.y + native.height - (target.1 - current.1),
    )
}

/// Must complete before show/orderFront. Re-presenting an already open picker
/// keeps its frame, including while preferences are being changed.
pub(crate) fn prepare(app: &AppHandle, window: &WebviewWindow, first_show: bool, reset: bool) -> bool {
    if !reset && checked(window.is_visible(), NativeIssue::Visibility) == Some(true) {
        return true;
    }
    let Some((monitor, keep_position)) = placement::active_monitor(app, window, first_show || reset) else {
        return false;
    };
    let key = display_key(&monitor);
    let sizing = app.state::<Sizing>();
    let saved = if reset { None } else { sizing.remembered(&key) };
    let larger = sizing.larger_text.load(Ordering::SeqCst);
    let Some(scale) = checked(window.scale_factor(), NativeIssue::Size) else {
        return false;
    };
    let (Some(inner), Some(outer)) = (
        checked(window.inner_size(), NativeIssue::Size),
        checked(window.outer_size(), NativeIssue::Size),
    ) else {
        return false;
    };
    let chrome = (
        (f64::from(outer.width) - f64::from(inner.width)) / scale,
        (f64::from(outer.height) - f64::from(inner.height)) / scale,
    );
    let work = monitor.work_area().size.to_logical::<f64>(monitor.scale_factor());
    let room = ((work.width - chrome.0).max(1.0), (work.height - chrome.1).max(1.0));
    let mut frame = plan(saved, larger, room);
    // Explicitly positioned debug previews retain their place too.
    if saved.is_none()
        && keep_position
        && !reset
        && let Some(live) = live_frame(window, &monitor)
    {
        frame.x = live
            .x
            .clamp(0, (room.0 - f64::from(frame.width)).max(0.0).floor() as i32);
        frame.y = live
            .y
            .clamp(0, (room.1 - f64::from(frame.height)).max(0.0).floor() as i32);
    }
    if let Ok(mut state) = sizing.state.lock() {
        state.last = Some((key, frame));
    }
    sizing.applying.store(true, Ordering::SeqCst);
    let applied = apply_frame(app, window, &monitor, frame, room);

    if reset
        && applied.is_some()
        && let Some(save) = sizing.forget(&display_key(&monitor))
    {
        save_later(app, save);
    }
    sizing.applying.store(false, Ordering::SeqCst);
    applied.is_some()
}

/// AppKit's Tauri adapter queues geometry onto a later main-queue turn. Set
/// the existing panel synchronously instead: orderFront must see the final
/// frame, on first show and on every reopen, without a visible resize.
#[cfg(target_os = "macos")]
fn apply_frame(
    app: &AppHandle,
    window: &WebviewWindow,
    monitor: &Monitor,
    frame: PickerWindow,
    room: (f64, f64),
) -> Option<()> {
    use tauri_nspanel::{ManagerExt, NSPoint, NSSize, objc2::MainThreadMarker};
    checked(MainThreadMarker::new().ok_or(()), NativeIssue::Resize)?;
    let panel = checked(app.get_webview_panel("main"), NativeIssue::Window)?;
    let native = panel.as_panel();
    let at = placement::window_area(window)?;
    let old = native.frame();
    let usable = placement::display(monitor).usable;
    // Translate from Tauri's top-left coordinates using the current native
    // frame. This also works above/left of the primary display and avoids
    // mixing physical retina pixels with AppKit screen points.
    let (x, y) = appkit_top_left(
        (at.x, at.y),
        placement::Area {
            x: old.origin.x,
            y: old.origin.y,
            width: old.size.width,
            height: old.size.height,
        },
        (usable.x + f64::from(frame.x), usable.y + f64::from(frame.y)),
    );
    native.setContentMaxSize(NSSize::new(f64::from(f32::MAX), f64::from(f32::MAX)));
    native.setContentMinSize(NSSize::new(
        f64::from(MINIMUM.0).min(room.0),
        f64::from(MINIMUM.1).min(room.1),
    ));
    native.setContentMaxSize(NSSize::new(room.0, room.1));
    native.setContentSize(NSSize::new(f64::from(frame.width), f64::from(frame.height)));
    native.setFrameTopLeftPoint(NSPoint::new(x, y));
    Some(())
}

#[cfg(not(target_os = "macos"))]
fn apply_frame(
    _app: &AppHandle,
    window: &WebviewWindow,
    monitor: &Monitor,
    frame: PickerWindow,
    room: (f64, f64),
) -> Option<()> {
    // Remove the previous display's bounds before applying this display's.
    // Native size/position/show messages keep their order; no animation.
    checked(window.set_max_size(None::<LogicalSize<f64>>), NativeIssue::Resize)?;
    checked(
        window.set_min_size(Some(LogicalSize::new(
            f64::from(MINIMUM.0).min(room.0),
            f64::from(MINIMUM.1).min(room.1),
        ))),
        NativeIssue::Resize,
    )?;
    checked(
        window.set_max_size(Some(LogicalSize::new(room.0, room.1))),
        NativeIssue::Resize,
    )?;
    checked(
        window.set_size(LogicalSize::new(frame.width, frame.height)),
        NativeIssue::Resize,
    )?;
    // Wayland's compositor owns placement; refusal cannot hide the picker.
    #[cfg(target_os = "linux")]
    report(window.set_position(position(monitor, frame)), NativeIssue::Position);
    #[cfg(not(target_os = "linux"))]
    checked(window.set_position(position(monitor, frame)), NativeIssue::Position)?;
    Some(())
}

fn save_later(app: &AppHandle, save: Save) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let revision = save.revision;
        let result = app.state::<Sizing>().persist(&app.state::<SettingsFile>(), save);
        if let Some(saved) = result {
            let handle = app.clone();
            report(
                app.run_on_main_thread(move || {
                    if handle.state::<Sizing>().revision.load(Ordering::SeqCst) == revision {
                        report(
                            handle.emit_to(
                                "main",
                                "picker:window-saved",
                                serde_json::json!({ "revision": revision, "saved": saved }),
                            ),
                            NativeIssue::PickerEvent,
                        );
                    }
                }),
                NativeIssue::PickerEvent,
            );
        }
    });
}

/// Ignore the event payload: A can arrive after drag B or after hide/reopen.
/// The live window and monitor are the only source of the next immutable save.
pub(crate) fn changed(app: &AppHandle, window: &WebviewWindow) {
    if app.state::<Sizing>().applying.load(Ordering::SeqCst)
        || checked(window.is_visible(), NativeIssue::Visibility) != Some(true)
    {
        return;
    }
    let Some(monitor) = checked(window.current_monitor(), NativeIssue::Monitor).flatten() else {
        return;
    };
    let Some(live) = live_frame(window, &monitor) else {
        return;
    };
    if let Some(save) = app.state::<Sizing>().record(display_key(&monitor), live) {
        save_later(app, save);
    }
}

pub(crate) fn reset_position(app: &AppHandle) {
    let generation = app.state::<crate::picker::PickerGeneration>().current();
    let revision = app.state::<Sizing>().revision.load(Ordering::SeqCst);
    let handle = app.clone();
    report(
        app.run_on_main_thread(move || {
            if handle.state::<crate::picker::PickerGeneration>().current() == generation
                && handle.state::<Sizing>().revision.load(Ordering::SeqCst) == revision
                && let Some(window) = handle.get_webview_window("main")
            {
                prepare(&handle, &window, false, true);
            }
        }),
        NativeIssue::Resize,
    );
}

#[cfg(test)]
mod tests;
