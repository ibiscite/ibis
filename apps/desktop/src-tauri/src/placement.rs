//! Where the picker appears: on the display the user is working on.
//!
//! Every display, the pointer and the window are compared in one coordinate
//! space: points on macOS, where each display's pixels use its own scale, and
//! pixels on Windows and X11, where pixels are shared by all displays.

use std::sync::atomic::AtomicBool;

use tauri::utils::config::WindowConfig;
use tauri::{AppHandle, Monitor, WebviewWindow};

use crate::diagnostics::{NativeIssue, checked, checked_once};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
use linux as native;
#[cfg(target_os = "macos")]
use macos as native;
#[cfg(target_os = "windows")]
use windows as native;

static POINTER_FAILURE_REPORTED: AtomicBool = AtomicBool::new(false);

/// A rectangle in the shared coordinate space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Area {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

impl Area {
    /// How far the point is from the rectangle; zero inside it.
    fn distance(&self, (x, y): (f64, f64)) -> f64 {
        let dx = (self.x - x).max(x - (self.x + self.width)).max(0.0);
        let dy = (self.y - y).max(y - (self.y + self.height)).max(0.0);
        dx.hypot(dy)
    }

    fn overlap(&self, other: &Area) -> f64 {
        let w = (self.x + self.width).min(other.x + other.width) - self.x.max(other.x);
        let h = (self.y + self.height).min(other.y + other.height) - self.y.max(other.y);
        w.max(0.0) * h.max(0.0)
    }

    #[cfg(any(target_os = "macos", test))]
    fn intersection(self, other: Area) -> Area {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        Area {
            x,
            y,
            width: (self.x + self.width).min(other.x + other.width) - x,
            height: (self.y + self.height).min(other.y + other.height) - y,
        }
    }

    /// Whether the other rectangle lies inside this one. Allows one unit of
    /// rounding, since positions pass through whole pixels.
    fn holds(&self, other: &Area) -> bool {
        other.x >= self.x - 1.0
            && other.y >= self.y - 1.0
            && other.x + other.width <= self.x + self.width + 1.0
            && other.y + other.height <= self.y + self.height + 1.0
    }
}

/// One display: all of it, and the part not taken by menu bars, docks and taskbars.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Display {
    pub(crate) bounds: Area,
    pub(crate) usable: Area,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Placement {
    /// The picker is fully visible on the display the user is working on.
    Keep,
    /// Centre the picker on this display.
    Centre(usize),
}

/// Choose where the picker goes when it is shown.
///
/// Prefer the focused window's display, then the pointer. Without either
/// (Wayland), request the picker's current display, then the primary display.
/// Only a remembered deliberate move overrides the default, in sizing.
pub(crate) fn choose(
    displays: &[Display],
    focused: Option<Area>,
    pointer: Option<(f64, f64)>,
    window: Option<Area>,
    primary: Option<usize>,
) -> Placement {
    let nearest = |point| {
        (0..displays.len()).min_by(|&a, &b| {
            displays[a]
                .bounds
                .distance(point)
                .total_cmp(&displays[b].bounds.distance(point))
        })
    };
    let showing = |window: Area| {
        (0..displays.len())
            .map(|i| (i, displays[i].bounds.overlap(&window)))
            .filter(|&(_, area)| area > 0.0)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };
    let target = focused
        .and_then(showing)
        .or_else(|| pointer.and_then(nearest))
        .or_else(|| window.and_then(showing))
        .or(primary.filter(|&i| i < displays.len()))
        .or(if displays.is_empty() { None } else { Some(0) });
    match (target, window) {
        (None, _) => Placement::Keep,
        (Some(i), _) => Placement::Centre(i),
    }
}

/// Debug previews may retain an explicitly positioned window on its display.
fn choose_for_show(
    displays: &[Display],
    focused: Option<Area>,
    pointer: Option<(f64, f64)>,
    window: impl Fn() -> Option<Area>,
    primary: Option<usize>,
    preview: Option<&WindowConfig>,
    debug: bool,
) -> Placement {
    let explicit = debug
        && preview.is_some_and(|c| !c.center && c.x.is_some_and(f64::is_finite) && c.y.is_some_and(f64::is_finite));
    if explicit && let Some(window) = window() {
        let target = choose(displays, None, None, Some(window), primary);
        return match target {
            Placement::Centre(i) if displays[i].usable.holds(&window) => Placement::Keep,
            target => target,
        };
    }
    choose(
        displays,
        focused,
        if explicit { None } else { pointer },
        window(),
        primary,
    )
}

/// The picker's top-left corner on a display: centred across its usable area,
/// with the search row's centre a quarter of the way down. Clamp the whole
/// frame; a picker larger than the area starts at its corner.
pub(crate) fn launcher_position(usable: Area, width: f64, height: f64, search_centre: f64) -> (f64, f64) {
    (
        usable.x + (usable.width - width).max(0.0) / 2.0,
        usable.y + (usable.height * 0.25 - search_centre).clamp(0.0, (usable.height - height).max(0.0)),
    )
}

const MACOS: bool = cfg!(target_os = "macos");

/// Divide native values by this to reach the shared space.
fn unit(scale: f64) -> f64 {
    if MACOS { scale } else { 1.0 }
}

fn area(x: i32, y: i32, width: u32, height: u32, scale: f64) -> Area {
    let s = unit(scale);
    Area {
        x: f64::from(x) / s,
        y: f64::from(y) / s,
        width: f64::from(width) / s,
        height: f64::from(height) / s,
    }
}

pub(crate) fn display(monitor: &Monitor) -> Display {
    let (position, size, work) = (monitor.position(), monitor.size(), monitor.work_area());
    let scale = monitor.scale_factor();
    let mut display = Display {
        bounds: area(position.x, position.y, size.width, size.height, scale),
        usable: area(
            work.position.x,
            work.position.y,
            work.size.width,
            work.size.height,
            scale,
        ),
    };
    if let Some(usable) = native::usable(monitor) {
        display.usable = usable;
    }
    display
}

pub(crate) fn display_key(monitor: &Monitor) -> Option<String> {
    native::display_key(monitor)
}

pub(crate) fn window_area(window: &WebviewWindow) -> Option<Area> {
    crate::sizing::window_area(window)
}

/// Resolve the display before sizing or moving the hidden picker. The bool
/// preserves an existing fully visible position (including debug previews).
pub(crate) fn active_monitor(app: &AppHandle, window: &WebviewWindow) -> Option<(Monitor, bool)> {
    let monitors = checked(app.available_monitors(), NativeIssue::Monitor)?;
    let displays: Vec<Display> = monitors.iter().map(display).collect();
    let primary = checked(app.primary_monitor(), NativeIssue::Monitor).flatten();
    // macOS reports the pointer in points times the primary display's scale.
    let pointer_scale = unit(primary.as_ref().map_or(1.0, Monitor::scale_factor));
    let pointer = checked_once(app.cursor_position(), NativeIssue::Pointer, &POINTER_FAILURE_REPORTED)
        .map(|p| (p.x / pointer_scale, p.y / pointer_scale));
    let primary = primary.and_then(|p| displays.iter().position(|d| *d == display(&p)));
    let config = app.config().app.windows.iter().find(|c| c.label == window.label());
    match choose_for_show(
        &displays,
        native::focused_window(),
        pointer,
        || window_area(window),
        primary,
        config,
        cfg!(debug_assertions),
    ) {
        Placement::Centre(i) => Some((monitors[i].clone(), false)),
        Placement::Keep => checked(window.current_monitor(), NativeIssue::Monitor)
            .flatten()
            .map(|monitor| (monitor, true)),
    }
}

#[cfg(test)]
mod tests;
