//! Where the picker appears: on the display the user is working on.
//!
//! Every display, the pointer and the window are compared in one coordinate
//! space: points on macOS, where each display's pixels use its own scale, and
//! pixels on Windows and X11, where pixels are shared by all displays.

use std::sync::atomic::AtomicBool;

use tauri::{AppHandle, LogicalPosition, Monitor, PhysicalPosition, Position, WebviewWindow};

use crate::diagnostics::{NativeIssue, checked, checked_once, report};

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
/// The target is the display with the pointer, or the nearest one when the
/// pointer is in a gap between displays. Without a pointer position (Wayland)
/// it is the display showing most of the picker, then the primary display. A
/// picker the user moved keeps its place while it is fully visible there; one
/// left on another display, on a disconnected display or partly off the screen
/// after a resolution change is centred on the target.
pub(crate) fn choose(
    displays: &[Display],
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
    let target = pointer
        .and_then(nearest)
        .or_else(|| window.and_then(showing))
        .or(primary.filter(|&i| i < displays.len()))
        .or(if displays.is_empty() { None } else { Some(0) });
    match (target, window) {
        (None, _) => Placement::Keep,
        (Some(i), Some(window)) if displays[i].usable.holds(&window) => Placement::Keep,
        (Some(i), _) => Placement::Centre(i),
    }
}

/// The picker's top-left corner on a display: centred across its usable area,
/// a fifth of the way down. A picker larger than the area starts at its corner.
pub(crate) fn launcher_position(usable: Area, width: f64, height: f64) -> (f64, f64) {
    (
        usable.x + (usable.width - width).max(0.0) / 2.0,
        usable.y + (usable.height - height).max(0.0) / 5.0,
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

fn display(monitor: &Monitor) -> Display {
    let (position, size, work) = (monitor.position(), monitor.size(), monitor.work_area());
    let scale = monitor.scale_factor();
    Display {
        bounds: area(position.x, position.y, size.width, size.height, scale),
        usable: area(
            work.position.x,
            work.position.y,
            work.size.width,
            work.size.height,
            scale,
        ),
    }
}

fn window_area(window: &WebviewWindow) -> Option<Area> {
    let position = checked(window.outer_position(), NativeIssue::Position)?;
    let size = checked(window.outer_size(), NativeIssue::Size)?;
    let scale = checked(window.scale_factor(), NativeIssue::Size)?;
    Some(area(position.x, position.y, size.width, size.height, scale))
}

/// Move the picker to the display the user is working on (see [`choose`]).
/// On first show it has no place of its own yet, so it is always centred.
pub(crate) fn place_on_active_display(app: &AppHandle, window: &WebviewWindow, first_show: bool) {
    let Some(monitors) = checked(app.available_monitors(), NativeIssue::Monitor) else {
        return;
    };
    let displays: Vec<Display> = monitors.iter().map(display).collect();
    let primary = checked(app.primary_monitor(), NativeIssue::Monitor).flatten();
    // macOS reports the pointer in points times the primary display's scale.
    let pointer_scale = unit(primary.as_ref().map_or(1.0, Monitor::scale_factor));
    let pointer = checked_once(app.cursor_position(), NativeIssue::Pointer, &POINTER_FAILURE_REPORTED)
        .map(|p| (p.x / pointer_scale, p.y / pointer_scale));
    let primary = primary.and_then(|p| displays.iter().position(|d| *d == display(&p)));
    if let Placement::Centre(i) = choose(
        &displays,
        pointer,
        if first_show { None } else { window_area(window) },
        primary,
    ) {
        place_on(window, &monitors[i]);
    }
}

/// Put the picker at its launcher position on the given display.
fn place_on(window: &WebviewWindow, monitor: &Monitor) {
    let (Some(size), Some(window_scale)) = (
        checked(window.outer_size(), NativeIssue::Size),
        checked(window.scale_factor(), NativeIssue::Size),
    ) else {
        return;
    };
    // The picker keeps its size in points, so measure it at the target's scale.
    let points = size.to_logical::<f64>(window_scale);
    let scale = if MACOS { 1.0 } else { monitor.scale_factor() };
    let (x, y) = launcher_position(display(monitor).usable, points.width * scale, points.height * scale);
    let position: Position = if MACOS {
        LogicalPosition::new(x, y).into()
    } else {
        PhysicalPosition::new(x.round() as i32, y.round() as i32).into()
    };
    report(window.set_position(position), NativeIssue::Position);
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn rect(x: f64, y: f64, width: f64, height: f64) -> Area {
        Area { x, y, width, height }
    }

    /// A laptop with a menu bar, and a larger display above and to its left,
    /// so the second display has negative coordinates.
    const LAPTOP: Display = Display {
        bounds: rect(0.0, 0.0, 1728.0, 1117.0),
        usable: rect(0.0, 38.0, 1728.0, 1079.0),
    };
    const EXTERNAL: Display = Display {
        bounds: rect(-3440.0, -1440.0, 3440.0, 1440.0),
        usable: rect(-3440.0, -1415.0, 3440.0, 1360.0),
    };
    const DISPLAYS: [Display; 2] = [LAPTOP, EXTERNAL];
    const PICKER_ON_LAPTOP: Area = rect(504.0, 246.0, 720.0, 480.0);

    #[test]
    fn the_picker_moves_to_the_display_with_the_pointer() {
        assert_eq!(
            choose(&DISPLAYS, Some((-100.0, -100.0)), Some(PICKER_ON_LAPTOP), Some(0)),
            Placement::Centre(1)
        );
        assert_eq!(
            choose(
                &DISPLAYS,
                Some((100.0, 100.0)),
                Some(rect(-2000.0, -1000.0, 720.0, 480.0)),
                Some(0)
            ),
            Placement::Centre(0)
        );
    }

    #[test]
    fn a_picker_the_user_placed_keeps_its_place_on_that_display() {
        let moved = rect(1000.0, 600.0, 720.0, 480.0);
        assert_eq!(
            choose(&DISPLAYS, Some((100.0, 100.0)), Some(moved), Some(0)),
            Placement::Keep
        );
    }

    #[test]
    fn a_picker_partly_off_the_usable_area_is_centred_again() {
        // After a resolution change, or under the menu bar.
        let partly_off = rect(1500.0, 600.0, 720.0, 480.0);
        assert_eq!(
            choose(&DISPLAYS, Some((100.0, 100.0)), Some(partly_off), Some(0)),
            Placement::Centre(0)
        );
        let under_menu_bar = rect(500.0, 0.0, 720.0, 480.0);
        assert_eq!(
            choose(&DISPLAYS, Some((100.0, 100.0)), Some(under_menu_bar), Some(0)),
            Placement::Centre(0)
        );
    }

    #[test]
    fn edges_and_gaps_between_displays_choose_the_nearest_display() {
        assert_eq!(choose(&DISPLAYS, Some((0.0, 0.0)), None, None), Placement::Centre(0));
        assert_eq!(choose(&DISPLAYS, Some((-0.5, -0.5)), None, None), Placement::Centre(1));
        // Displays of different heights leave a gap the pointer can round into.
        assert_eq!(choose(&DISPLAYS, Some((-10.0, 50.0)), None, None), Placement::Centre(0));
        assert_eq!(
            choose(&DISPLAYS, Some((-500.0, 50.0)), None, None),
            Placement::Centre(1)
        );
    }

    #[test]
    fn without_a_pointer_the_picker_stays_on_its_display_or_returns_to_the_primary_one() {
        assert_eq!(
            choose(&DISPLAYS, None, Some(PICKER_ON_LAPTOP), Some(1)),
            Placement::Keep
        );
        // Its display was disconnected: the picker is nowhere to be seen.
        let lost = rect(5000.0, 5000.0, 720.0, 480.0);
        assert_eq!(choose(&DISPLAYS, None, Some(lost), Some(1)), Placement::Centre(1));
        assert_eq!(choose(&DISPLAYS, None, Some(lost), None), Placement::Centre(0));
        assert_eq!(choose(&DISPLAYS, None, None, Some(7)), Placement::Centre(0));
    }

    #[test]
    fn with_no_display_information_the_picker_is_left_alone() {
        assert_eq!(
            choose(&[], Some((0.0, 0.0)), Some(PICKER_ON_LAPTOP), Some(0)),
            Placement::Keep
        );
    }

    #[test]
    fn the_picker_is_centred_high_on_the_usable_area() {
        assert_eq!(launcher_position(EXTERNAL.usable, 720.0, 480.0), (-2080.0, -1239.0));
    }

    #[test]
    fn a_picker_larger_than_the_display_starts_at_its_corner() {
        assert_eq!(
            launcher_position(rect(10.0, 20.0, 300.0, 200.0), 720.0, 480.0),
            (10.0, 20.0)
        );
    }

    /// Native values reach the shared space the way each platform reports
    /// them: per-display points on macOS, shared pixels elsewhere.
    #[test]
    fn native_values_are_compared_in_one_space() {
        let retina = area(-6880, -2880, 6880, 2880, 2.0);
        if MACOS {
            assert_eq!(retina, rect(-3440.0, -1440.0, 3440.0, 1440.0));
        } else {
            assert_eq!(retina, rect(-6880.0, -2880.0, 6880.0, 2880.0));
        }
        assert_eq!(area(10, 20, 300, 200, 1.0), rect(10.0, 20.0, 300.0, 200.0));
    }
}
