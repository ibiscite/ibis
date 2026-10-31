//! One steady picker frame, restored before presentation (design C10).
//! One owner accepts settled native geometry and debounces its settings writes.
//! Content never crosses this boundary; presentation is complete before show.

mod display_events;
mod owner;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use ibis_core::settings::{PickerWindow, Settings};
use owner::{Display, Event, MINIMUM, Owner, Sample, Save, content_room};
use tauri::{AppHandle, Emitter, Manager, Monitor, WebviewWindow, WindowEvent};

#[cfg(not(target_os = "macos"))]
use tauri::{LogicalSize, PhysicalPosition, Position};

use crate::diagnostics::{NativeIssue, checked, report};
use crate::placement;
use crate::preferences::SettingsFile;

const DEFAULT: (u32, u32) = (720, 480);
// Kept equal to --larger-text-scale; the regression checks the shared token.
const LARGER_TEXT_SCALE: f64 = 1.15;
const FLUSH_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Default)]
pub(crate) struct Sizing {
    shared: Arc<Shared>,
    larger_text: AtomicBool,
}

#[derive(Default)]
struct Shared {
    state: Mutex<Owner>,
    wake: Condvar,
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
        if let Ok(mut state) = self.shared.state.lock() {
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
        self.shared.state.lock().ok()?.windows.get(display).copied()
    }

    fn revision(&self) -> u64 {
        self.shared.state.lock().map_or(0, |state| state.revision)
    }
}

impl Shared {
    fn changed(&self, event: Event) {
        if let Ok(mut state) = self.state.lock() {
            state.event(event, Instant::now());
        }
        self.wake.notify_all();
    }

    fn persist(&self, file: &SettingsFile, save: Save) -> Option<bool> {
        // Check after entering the settings writer, not before waiting for it.
        // A late A cannot write over B, including B from another display: every
        // snapshot contains all accepted drags. Never hold sizing across I/O.
        let current = || self.state.lock().is_ok_and(|state| state.revision == save.revision);
        let result = file.update_when(current, |settings| settings.picker_windows = save.windows);
        if !current() || matches!(result, Ok(None)) {
            return None;
        }
        Some(result.is_ok())
    }

    /// The sole writer sleeps until a deadline or an explicit flush. No timer,
    /// worker, backup or fsync is created for each step of a native drag.
    fn run(&self, file: &SettingsFile, notify: impl Fn(u64, bool)) {
        loop {
            let save = {
                let Ok(mut state) = self.state.lock() else {
                    return;
                };
                loop {
                    if state.stopping && state.pending_save.is_none() {
                        state.finished = true;
                        self.wake.notify_all();
                        return;
                    }
                    if let Some(save) = state.take_save(Instant::now()) {
                        break save;
                    }
                    let waiting = if let Some(save) = &state.pending_save {
                        let delay = save.due.saturating_duration_since(Instant::now());
                        self.wake
                            .wait_timeout(state, delay)
                            .map(|(state, _)| state)
                            .map_err(|_| ())
                    } else {
                        self.wake.wait(state).map_err(|_| ())
                    };
                    let Ok(next) = waiting else {
                        return;
                    };
                    state = next;
                }
            };
            let revision = save.revision;
            let result = self.persist(file, save);
            if let Ok(mut state) = self.state.lock() {
                state.complete(revision);
                self.wake.notify_all();
            }
            if let Some(saved) = result {
                notify(revision, saved);
            }
        }
    }

    fn request_flush(&self, stop: bool) -> Option<u64> {
        let Ok(mut state) = self.state.lock() else {
            return None;
        };
        let revision = state.flush(Instant::now(), stop);
        self.wake.notify_all();
        Some(revision)
    }

    fn flush(&self, timeout: Duration, stop: bool) -> bool {
        let deadline = Instant::now() + timeout;
        let Some(revision) = self.request_flush(stop) else {
            return false;
        };
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        while !state.drained(revision, stop) {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            let Ok((next, _)) = self.wake.wait_timeout(state, left) else {
                return false;
            };
            state = next;
        }
        true
    }
}

pub(crate) fn start(app: &AppHandle) -> bool {
    let sizing = app.state::<Sizing>();
    let Ok(mut state) = sizing.shared.state.lock() else {
        return false;
    };
    if state.started {
        return true;
    }
    let shared = sizing.shared.clone();
    let file = app.state::<SettingsFile>().inner().clone();
    let handle = app.clone();
    let spawned = std::thread::Builder::new()
        .name("ibis-window-settings".into())
        .spawn(move || {
            shared.run(&file, |revision, saved| {
                if handle.state::<Sizing>().revision() == revision {
                    report(
                        handle.emit_to(
                            "main",
                            "picker:window-saved",
                            serde_json::json!({ "revision": revision, "saved": saved }),
                        ),
                        NativeIssue::PickerEvent,
                    );
                }
            });
        });
    state.started = checked(spawned, NativeIssue::Startup).is_some();
    state.started
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

struct Metrics {
    scale: f64,
    inner: (f64, f64),
    outer: (f64, f64),
    area: placement::Area,
}

#[cfg(target_os = "linux")]
fn metrics(window: &WebviewWindow) -> Option<Metrics> {
    use gtk::prelude::*;
    let native = checked(window.gtk_window(), NativeIssue::Window)?;
    // tao 0.37.1 initializes outer_size with root_origin until configure. GDK
    // queries the realized native frame directly, including before first show.
    native.realize();
    let gdk = native.window()?;
    let extents = gdk.frame_extents();
    // Match tao's client-size/set_size contract. GDK geometry can include GTK
    // client-side decoration; subtracting it would incorrectly erase chrome.
    let (width, height) = GtkWindowExt::size(&native);
    let scale = f64::from(WidgetExt::scale_factor(&native));
    Some(Metrics {
        scale,
        inner: (f64::from(width), f64::from(height)),
        outer: (f64::from(extents.width()), f64::from(extents.height())),
        area: placement::Area {
            x: f64::from(extents.x()) * scale,
            y: f64::from(extents.y()) * scale,
            width: f64::from(extents.width()) * scale,
            height: f64::from(extents.height()) * scale,
        },
    })
}

#[cfg(not(target_os = "linux"))]
fn metrics(window: &WebviewWindow) -> Option<Metrics> {
    let scale = checked(window.scale_factor(), NativeIssue::Size)?;
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let inner = checked(window.inner_size(), NativeIssue::Size)?.to_logical::<f64>(scale);
    let outer = checked(window.outer_size(), NativeIssue::Size)?;
    let at = checked(window.outer_position(), NativeIssue::Position)?;
    let unit = if cfg!(target_os = "macos") { scale } else { 1.0 };
    Some(Metrics {
        scale,
        inner: (inner.width, inner.height),
        outer: (f64::from(outer.width) / scale, f64::from(outer.height) / scale),
        area: placement::Area {
            x: f64::from(at.x) / unit,
            y: f64::from(at.y) / unit,
            width: f64::from(outer.width) / unit,
            height: f64::from(outer.height) / unit,
        },
    })
}

pub(crate) fn window_area(window: &WebviewWindow) -> Option<placement::Area> {
    Some(metrics(window)?.area)
}

fn display(monitor: &Monitor, metrics: &Metrics) -> Option<Display> {
    let work = monitor.work_area().size.to_logical::<f64>(monitor.scale_factor());
    let usable = placement::display(monitor).usable;
    let room = checked(
        content_room(metrics.inner, metrics.outer, (work.width, work.height)).ok_or(()),
        NativeIssue::Size,
    )?;
    Some(Display {
        key: display_key(monitor),
        scale: monitor.scale_factor(),
        origin: (usable.x, usable.y),
        room,
    })
}

fn live_frame(metrics: &Metrics, monitor: &Monitor) -> PickerWindow {
    let at = metrics.area;
    let usable = placement::display(monitor).usable;
    let unit = if cfg!(target_os = "macos") {
        1.0
    } else {
        monitor.scale_factor()
    };
    PickerWindow {
        width: metrics.inner.0.round() as u32,
        height: metrics.inner.1.round() as u32,
        custom_size: false,
        x: ((at.x - usable.x) / unit).round() as i32,
        y: ((at.y - usable.y) / unit).round() as i32,
    }
}

fn observed(metrics: &Metrics, monitor: &Monitor) -> Option<Sample> {
    if metrics.scale != monitor.scale_factor() {
        return None;
    }
    Some(Sample {
        display: display(monitor, metrics)?,
        frame: live_frame(metrics, monitor),
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
    let sizing = app.state::<Sizing>();
    let saved = if reset {
        None
    } else {
        sizing.remembered(&display_key(&monitor))
    };
    let larger = sizing.larger_text.load(Ordering::SeqCst);
    let Some(metrics) = metrics(window) else {
        return false;
    };
    let Some(display) = display(&monitor, &metrics) else {
        return false;
    };
    let room = display.room;
    let reset_key = display.key.clone();
    let previous = if reset {
        let before = checked(window.current_monitor(), NativeIssue::Monitor)
            .flatten()
            .and_then(|current| observed(&metrics, &current));
        let Ok(mut state) = sizing.shared.state.lock() else {
            return false;
        };
        if let Some(before) = before {
            state.observe(before, Instant::now(), false);
        }
        // A disconnected screen or a pending DPI turn still has a last accepted
        // frame. Reset must remain available to recover that off-screen window.
        let Some(previous) = state.last.clone() else {
            return false;
        };
        Some(previous)
    } else {
        None
    };
    let mut frame = plan(saved, larger, room);
    // Explicitly positioned debug previews retain their place too.
    if saved.is_none() && keep_position && !reset {
        let live = live_frame(&metrics, &monitor);
        frame.x = live
            .x
            .clamp(0, (room.0 - f64::from(frame.width)).max(0.0).floor() as i32);
        frame.y = live
            .y
            .clamp(0, (room.1 - f64::from(frame.height)).max(0.0).floor() as i32);
    }
    if let Ok(mut state) = sizing.shared.state.lock() {
        if state.stopping {
            return false;
        }
        state.applying = true;
    }
    let applied = apply_frame(app, window, &monitor, frame, room);
    if let Ok(mut state) = sizing.shared.state.lock() {
        if applied.is_some() {
            state.prepared(Sample { display, frame });
            if let Some(previous) = previous {
                state.reset(&reset_key, previous, Instant::now());
            }
        }
        state.applying = false;
    }
    if reset {
        // The preceding sample may have accepted a drag even if applying the
        // reset failed. Its save and any retired Undo still need delivery.
        emit_reset_undo(app);
        sizing.shared.wake.notify_all();
    }
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
    apply_constraints(app, window, room)?;
    native.setContentSize(NSSize::new(f64::from(frame.width), f64::from(frame.height)));
    native.setFrameTopLeftPoint(NSPoint::new(x, y));
    Some(())
}

#[cfg(not(target_os = "macos"))]
fn apply_frame(
    app: &AppHandle,
    window: &WebviewWindow,
    monitor: &Monitor,
    frame: PickerWindow,
    room: (f64, f64),
) -> Option<()> {
    apply_constraints(app, window, room)?;
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

/// Only constraints change during an opening; a display/work-area refresh
/// does not reapply the opening frame or its remembered position.
#[cfg(target_os = "macos")]
fn apply_constraints(app: &AppHandle, _window: &WebviewWindow, room: (f64, f64)) -> Option<()> {
    use tauri_nspanel::{ManagerExt, NSSize, objc2::MainThreadMarker};
    checked(MainThreadMarker::new().ok_or(()), NativeIssue::Resize)?;
    let panel = checked(app.get_webview_panel("main"), NativeIssue::Window)?;
    let native = panel.as_panel();
    native.setContentMaxSize(NSSize::new(f64::from(f32::MAX), f64::from(f32::MAX)));
    native.setContentMinSize(NSSize::new(
        f64::from(MINIMUM.0).min(room.0),
        f64::from(MINIMUM.1).min(room.1),
    ));
    native.setContentMaxSize(NSSize::new(room.0, room.1));
    Some(())
}

#[cfg(not(target_os = "macos"))]
fn apply_constraints(_app: &AppHandle, window: &WebviewWindow, room: (f64, f64)) -> Option<()> {
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
    Some(())
}

/// Event payloads never become saved frames. DPI's suggested size identifies
/// its automatic resize only; the eventual frame comes from a settled read.
pub(crate) fn changed(app: &AppHandle, event: &WindowEvent) {
    let event = match event {
        WindowEvent::ScaleFactorChanged {
            scale_factor,
            new_inner_size,
            ..
        } => {
            let size = new_inner_size.to_logical::<u32>(*scale_factor);
            Event::Dpi {
                scale: *scale_factor,
                logical: (size.width, size.height),
            }
        }
        _ => Event::Geometry,
    };
    app.state::<Sizing>().shared.changed(event);
}

pub(crate) fn watch_display(app: &AppHandle, window: &WebviewWindow) {
    // If registration fails, prepare() still reads the work area on next show.
    report(display_events::install(app, window).ok_or(()), NativeIssue::Monitor);
}

fn sample(app: &AppHandle, window: &WebviewWindow, settled: bool) {
    let sizing = app.state::<Sizing>();
    let Some(monitor) = checked(window.current_monitor(), NativeIssue::Monitor).flatten() else {
        return;
    };
    let Some(metrics) = metrics(window) else {
        return;
    };
    let Some(sample) = observed(&metrics, &monitor) else {
        return;
    };
    let (constraints, undo_changed) = sizing.shared.state.lock().map_or((None, false), |mut state| {
        let previous = state.undo.as_ref().map(|undo| undo.revision);
        let constraints = state.observe(sample, Instant::now(), settled);
        (constraints, previous != state.undo.as_ref().map(|undo| undo.revision))
    });
    if let Some(display) = constraints
        && apply_constraints(app, window, display.room).is_some()
        && let Ok(mut state) = sizing.shared.state.lock()
    {
        state.constraints = Some(display);
    }
    sizing.shared.wake.notify_all();
    if undo_changed {
        emit_reset_undo(app);
    }
}

/// Windows sends ScaleFactorChanged inside WM_DPICHANGED, before SetWindowPos.
/// MainEventsCleared is after that native turn, rather than a racing async task.
/// Only a geometry/DPI/display event can request this read. Consume it before
/// looking up the window: cloning its Tao proxy wakes the macOS loop itself.
pub(crate) fn settled(app: &AppHandle) {
    let sizing = app.state::<Sizing>();
    owner::settled(&sizing.shared.state, || read_settled(app));
}

fn read_settled(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if checked(window.is_visible(), NativeIssue::Visibility) == Some(true) {
        sample(app, &window, true);
    }
}

pub(crate) fn flush(app: &AppHandle, stop: bool) {
    if let Some(window) = app.get_webview_window("main")
        && checked(window.is_visible(), NativeIssue::Visibility) == Some(true)
    {
        sample(app, &window, false);
    }
    let sizing = app.state::<Sizing>();
    if !stop {
        // Hide forces the existing worker immediately without delaying focus
        // handoff on disk I/O. Every exit route drains this same revision.
        report(sizing.shared.request_flush(false).ok_or(()), NativeIssue::WindowSave);
        return;
    }
    report(
        sizing.shared.flush(FLUSH_TIMEOUT, stop).then_some(()).ok_or(()),
        NativeIssue::WindowSave,
    );
}

pub(crate) fn reset_position(app: &AppHandle) {
    let generation = app.state::<crate::picker::PickerGeneration>().current();
    let revision = app.state::<Sizing>().revision();
    let handle = app.clone();
    report(
        app.run_on_main_thread(move || {
            if handle.state::<crate::picker::PickerGeneration>().current() == generation
                && handle.state::<Sizing>().revision() == revision
                && let Some(window) = handle.get_webview_window("main")
                && prepare(&handle, &window, false, true)
            {
                crate::picker::show_picker(&handle);
            }
        }),
        NativeIssue::Resize,
    );
}

fn emit_reset_undo(app: &AppHandle) {
    let sizing = app.state::<Sizing>();
    let payload = sizing.shared.state.lock().ok().map(|state| {
        serde_json::json!({
            "revision": state.revision, "undo": state.undo.as_ref().map(|undo| undo.revision)
        })
    });
    if let Some(payload) = payload {
        report(
            app.emit_to("main", "picker:window-reset", payload),
            NativeIssue::PickerEvent,
        );
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) fn read_window_position_undo(sizing: tauri::State<'_, Sizing>) -> Option<u64> {
    let state = sizing.shared.state.lock().ok()?;
    state.undo_for(state.revision).map(|undo| undo.revision)
}

#[tauri::command]
#[specta::specta]
pub(crate) fn dismiss_window_position_undo(app: AppHandle, revision: u64) {
    if let Ok(mut state) = app.state::<Sizing>().shared.state.lock() {
        state.dismiss_undo(revision);
    }
    emit_reset_undo(&app);
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn undo_window_position(app: AppHandle, revision: u64) -> Result<bool, String> {
    let generation = app.state::<crate::picker::PickerGeneration>().current();
    let (sent, mut received) = tauri::async_runtime::channel(1);
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let result = if handle.state::<crate::picker::PickerGeneration>().current() == generation {
            restore_position(&handle, revision)
        } else {
            Ok(false)
        };
        let _ = sent.try_send(result);
    })
    .map_err(|_| "Ibis could not restore the window position. Try again.".to_string())?;
    received
        .recv()
        .await
        .unwrap_or_else(|| Err("Ibis could not restore the window position. Try again.".into()))
}

fn restore_position(app: &AppHandle, revision: u64) -> Result<bool, String> {
    const FAILED: &str = "Ibis could not restore the window position. Try again.";
    let window = app.get_webview_window("main").ok_or(FAILED)?;
    // Accept an edge drag whose native event arrived before this action, even
    // if MainEventsCleared has not sampled it yet. It retires the old Undo.
    sample(app, &window, false);
    let sizing = app.state::<Sizing>();
    let undo = {
        let mut state = sizing.shared.state.lock().map_err(|_| FAILED)?;
        let Some(undo) = state.undo_for(revision) else {
            return Ok(false);
        };
        state.applying = true;
        undo
    };
    let applied = (|| {
        let monitors = checked(app.available_monitors(), NativeIssue::Monitor).ok_or(FAILED)?;
        let monitor = monitors
            .into_iter()
            .find(|monitor| display_key(monitor) == undo.previous.display.key)
            .or_else(|| checked(window.current_monitor(), NativeIssue::Monitor).flatten())
            .ok_or(FAILED)?;
        let metrics = metrics(&window).ok_or(FAILED)?;
        let display = display(&monitor, &metrics).ok_or(FAILED)?;
        let mut frame = plan(
            Some(PickerWindow {
                custom_size: true,
                ..undo.previous.frame
            }),
            false,
            display.room,
        );
        frame.custom_size = undo.previous.frame.custom_size;
        apply_frame(app, &window, &monitor, frame, display.room).ok_or(FAILED)?;
        Ok::<_, &str>(Sample { display, frame })
    })();
    let mut state = sizing.shared.state.lock().map_err(|_| FAILED)?;
    state.applying = false;
    // Clear the applying guard even when the native restore failed, so retry
    // and subsequent geometry events still belong to the owner.
    state.prepared(applied?);
    let restored = state.restore(revision, Instant::now());
    drop(state);
    sizing.shared.wake.notify_all();
    emit_reset_undo(app);
    Ok(restored)
}

#[cfg(test)]
mod tests;
