//! The picker is as tall as its content (docs/design.md, C10). The interface
//! asks for its content height, or for the results height; the window grows
//! with its top edge fixed, never shrinks while open and starts compact again.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tauri::{AppHandle, LogicalSize, Manager, PhysicalPosition, WebviewWindow};

use crate::diagnostics::{NativeIssue, checked, report};

/// The results height until the user drags the window to another (tauri.conf.json).
const RESULTS_HEIGHT: f64 = 480.0;
/// Search and footer alone are never squeezed below this.
const SMALLEST: f64 = 88.0;
const GROW: Duration = Duration::from_millis(220);
const FRAME: Duration = Duration::from_millis(16);
/// Resize events this long after one of ours are ours, not the user's.
const SETTLE: Duration = Duration::from_millis(250);

#[derive(Default)]
pub(crate) struct Sizing {
    state: Mutex<State>,
    /// A newer request or a hide stops a growing animation.
    generation: AtomicU64,
    /// Orders our resizes, so a hide's compact height is the last one.
    resizing: Mutex<()>,
}

#[derive(Default)]
struct State {
    /// The height the user last dragged the window to.
    remembered: Option<f64>,
    /// The tallest height of this opening; the window never goes below it.
    floor: f64,
    /// The last content height asked for, used when the picker opens again.
    compact: Option<f64>,
    /// The height we last set, and until when resize events are ours; each
    /// frame renews it, so a late frame is never taken for a drag.
    ours: Option<(f64, Instant)>,
}

/// The height to show: the content's own (`Some`) or the results height
/// (`None`), never below this opening's tallest and never past the screen.
fn target(wanted: Option<f64>, remembered: Option<f64>, floor: f64, room: f64) -> f64 {
    wanted
        .unwrap_or(remembered.unwrap_or(RESULTS_HEIGHT))
        .max(floor)
        .min(room)
        .max(SMALLEST)
}

/// Heights for each frame of a 220 ms ease-out from `from` to `to`; one step
/// with reduced motion or when shrinking (only a hide shrinks).
fn steps(from: f64, to: f64, animate: bool) -> Vec<f64> {
    if !animate || to <= from {
        return vec![to];
    }
    let n = GROW.as_millis().div_ceil(FRAME.as_millis()) as u32;
    (1..=n)
        .map(|i| {
            let t = f64::from(i) / f64::from(n);
            (from + (to - from) * (1.0 - (1.0 - t).powi(3))).round()
        })
        .collect()
}

/// Whether a resize to `height` came from the user rather than from us.
fn by_user(ours: Option<(f64, Instant)>, height: f64, now: Instant) -> bool {
    ours.is_none_or(|(set, until)| now > until && (height - set).abs() > 1.0)
}

struct Frame {
    width: f64,
    height: f64,
    #[cfg_attr(target_os = "linux", expect(dead_code, reason = "GTK keeps the top edge itself"))]
    top_left: PhysicalPosition<i32>,
    /// Logical height from the window's top edge to the bottom of the work area.
    room: f64,
}

fn frame(window: &WebviewWindow) -> Option<Frame> {
    let scale = checked(window.scale_factor(), NativeIssue::Size)?;
    let inner = checked(window.inner_size(), NativeIssue::Size)?.to_logical::<f64>(scale);
    let outer = checked(window.outer_size(), NativeIssue::Size)?;
    let top_left = checked(window.outer_position(), NativeIssue::Position)?;
    let room = checked(window.current_monitor(), NativeIssue::Monitor)
        .flatten()
        .map_or(f64::INFINITY, |monitor| {
            let work = monitor.work_area();
            let bottom = f64::from(work.position.y) + f64::from(work.size.height);
            let chrome = f64::from(outer.height) - inner.height * scale;
            (bottom - f64::from(top_left.y) - chrome) / scale
        });
    Some(Frame {
        width: inner.width,
        height: inner.height,
        top_left,
        room,
    })
}

/// Set the height and put the top edge back, since some systems keep the
/// bottom. GTK keeps the top itself, and Wayland does not allow moving.
fn resize(window: &WebviewWindow, at: &Frame, height: f64) {
    report(window.set_size(LogicalSize::new(at.width, height)), NativeIssue::Resize);
    #[cfg(not(target_os = "linux"))]
    report(window.set_position(at.top_left), NativeIssue::Position);
}

/// One resize of ours, unless a newer request or a hide replaced `generation`.
/// Window reads happen before any lock: from another thread they wait for the
/// main thread, which takes the state lock for resize events.
fn step(app: &AppHandle, window: &WebviewWindow, generation: u64, height: f64) {
    let Some(at) = frame(window) else { return };
    let sizing = app.state::<Sizing>();
    let Ok(_order) = sizing.resizing.lock() else { return };
    if sizing.generation.load(Ordering::SeqCst) != generation {
        return;
    }
    if let Ok(mut state) = sizing.state.lock() {
        state.ours = Some((height, Instant::now() + SETTLE));
    }
    resize(window, &at, height);
}

/// Fit the picker to `height` CSS px of content, or to the results height.
#[tauri::command]
#[specta::specta]
pub(crate) fn fit_picker(app: AppHandle, height: Option<f64>, animate: bool) {
    let Some(window) = checked(app.get_webview_window("main").ok_or(()), NativeIssue::Window) else {
        return;
    };
    let Some(at) = frame(&window) else { return };
    let sizing = app.state::<Sizing>();
    let height = height.filter(|h| h.is_finite());
    let frames = {
        let Ok(mut state) = sizing.state.lock() else { return };
        if height.is_some() {
            state.compact = height;
        }
        let to = target(height, state.remembered, state.floor, at.room);
        state.floor = to;
        if (to - at.height).abs() < 1.0 {
            return;
        }
        steps(at.height, to, animate)
    };
    let generation = sizing.generation.fetch_add(1, Ordering::SeqCst) + 1;
    if let [height] = frames[..] {
        step(&app, &window, generation, height);
        return;
    }
    std::thread::spawn(move || {
        for height in frames {
            step(&app, &window, generation, height);
            std::thread::sleep(FRAME);
        }
    });
}

/// After a hide: forget this opening's height and start compact next time.
pub(crate) fn reset(app: &AppHandle, window: &WebviewWindow) {
    let at = frame(window);
    let sizing = app.state::<Sizing>();
    let Ok(_order) = sizing.resizing.lock() else { return };
    // Stops a growing animation before its next frame.
    sizing.generation.fetch_add(1, Ordering::SeqCst);
    let to = {
        let Ok(mut state) = sizing.state.lock() else { return };
        state.floor = 0.0;
        let (Some(compact), Some(at)) = (state.compact, at.as_ref()) else {
            return;
        };
        let to = target(Some(compact), None, 0.0, at.room);
        if (to - at.height).abs() < 1.0 {
            return;
        }
        state.ours = Some((to, Instant::now() + SETTLE));
        to
    };
    if let Some(at) = at {
        resize(window, &at, to);
    }
}

/// A drag to a new height becomes the results height from now on.
pub(crate) fn resized(app: &AppHandle, window: &WebviewWindow) {
    let (Some(scale), Some(size), Ok(true)) = (
        checked(window.scale_factor(), NativeIssue::Size),
        checked(window.inner_size(), NativeIssue::Size),
        window.is_visible(),
    ) else {
        return;
    };
    let height = size.to_logical::<f64>(scale).height;
    let sizing = app.state::<Sizing>();
    let Ok(mut state) = sizing.state.lock() else { return };
    if by_user(state.ours, height, Instant::now()) {
        state.remembered = Some(height);
        state.floor = height;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_sets_the_height_and_results_use_the_remembered_one() {
        assert_eq!(target(Some(140.0), None, 0.0, 900.0), 140.0);
        assert_eq!(target(None, None, 140.0, 900.0), RESULTS_HEIGHT);
        assert_eq!(target(None, Some(620.0), 140.0, 900.0), 620.0);
    }

    #[test]
    fn typing_never_shrinks_the_window_while_it_is_open() {
        // Results, then no match, then a shorter hint: each keystroke asks
        // for less, and the window keeps the tallest height of this opening.
        let mut floor = 0.0;
        for wanted in [Some(140.0), None, Some(200.0), Some(120.0), None, Some(96.0)] {
            let next = target(wanted, None, floor, 900.0);
            assert!(next >= floor, "{wanted:?} shrank the window from {floor} to {next}");
            floor = next;
        }
        assert_eq!(floor, RESULTS_HEIGHT);
    }

    #[test]
    fn the_window_stays_on_screen_and_keeps_room_for_search_and_footer() {
        assert_eq!(target(None, Some(2000.0), 0.0, 700.0), 700.0);
        assert_eq!(target(Some(10.0), None, 0.0, 900.0), SMALLEST);
        assert_eq!(target(None, None, 0.0, 40.0), SMALLEST);
    }

    #[test]
    fn growing_eases_out_over_220_ms_and_reduced_motion_jumps() {
        let frames = steps(140.0, 480.0, true);
        assert_eq!(frames.len(), 14);
        assert_eq!(frames.last(), Some(&480.0));
        assert!(frames.windows(2).all(|w| w[0] <= w[1]));
        // Ease-out: the first frame covers more than an even share.
        assert!(frames[0] - 140.0 > (480.0 - 140.0) / 14.0);
        assert_eq!(steps(140.0, 480.0, false), [480.0]);
        assert_eq!(steps(480.0, 140.0, true), [140.0]);
    }

    #[test]
    fn only_a_resize_we_did_not_make_is_remembered() {
        let now = Instant::now();
        let later = now + SETTLE * 2;
        assert!(by_user(None, 500.0, now));
        assert!(
            !by_user(Some((480.0, now + SETTLE)), 300.0, now),
            "our animation's frames are ours"
        );
        assert!(!by_user(Some((480.0, now)), 480.5, later), "our final size is ours");
        assert!(by_user(Some((480.0, now)), 560.0, later));
    }
}
