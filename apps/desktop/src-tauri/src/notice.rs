//! The short "Copied" notice shown after a clean copy hides the picker
//! (design rule 6, "Say what happened").
//!
//! It is a second, small window drawn by Ibis. It never takes focus, so the
//! editor that focus was just returned to keeps it, and it is left out of the
//! taskbar, the Dock and Alt+Tab. Each platform shows it where its own
//! notices appear: a pill near the top on macOS, a card at the bottom right
//! on Windows and a toast at the bottom on Linux, on the picker's display.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::diagnostics::{NativeIssue, checked, report};
use crate::placement::{Area, display};

pub(crate) const LABEL: &str = "notice";
/// The window title, which screen readers may read; never the citation.
const TITLE: &str = "Ibis notice";
/// As long as the prototype shows it: a 120 ms fade in, then the page's
/// 200 ms fade out, which this timer starts with `notice:leave`.
const SHOWN_FOR: Duration = Duration::from_millis(2_800);
/// The page's fade out (`notice.css`).
const FADE_OUT: Duration = Duration::from_millis(200);
/// Time for the `notice:leave` event to reach the page before the window goes.
const GRACE: Duration = Duration::from_millis(100);
/// A new window's timer starts when its page has loaded; this one takes it
/// away if the page never loads.
const LOAD_LIMIT: Duration = Duration::from_millis(8_000);
/// The notice names what was copied on one line; longer text is cut short.
const TEXT_LIMIT: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Platform {
    MacOs,
    Windows,
    Linux,
}

impl Platform {
    pub(crate) const CURRENT: Self = if cfg!(target_os = "macos") {
        Self::MacOs
    } else if cfg!(target_os = "windows") {
        Self::Windows
    } else {
        Self::Linux
    };
}

/// Window size in points. The window is transparent and a little larger than
/// the notice it draws on macOS and Windows, leaving room for its shadow; on
/// Linux it is the rounded toast itself.
pub(crate) fn notice_size(platform: Platform) -> (f64, f64) {
    match platform {
        Platform::MacOs => (400.0, 76.0),
        Platform::Windows => (420.0, 112.0),
        Platform::Linux => (360.0, 44.0),
    }
}

/// The window's top-left corner within the display's usable area, in the
/// same units as `usable`. A notice wider or taller than the area starts at
/// its corner rather than off screen.
pub(crate) fn notice_position(platform: Platform, usable: Area, width: f64, height: f64) -> (f64, f64) {
    let centre = usable.x + (usable.width - width).max(0.0) / 2.0;
    let right = usable.x + (usable.width - width).max(0.0);
    let bottom = |margin: f64| usable.y + (usable.height - height - margin).max(0.0);
    match platform {
        // The pill sits 40 points below the menu bar; 16 of them are the
        // window's own shadow margin.
        Platform::MacOs => (centre, usable.y + 24.0),
        // The card's own 16-point margin keeps it off the taskbar and edge.
        Platform::Windows => (right, bottom(0.0)),
        Platform::Linux => (centre, bottom(28.0)),
    }
}

/// The window's corner on a display in the shared space of `placement`
/// (points on macOS, pixels elsewhere). `scale` converts the notice's size in
/// points to that space: 1 on macOS, the display's scale elsewhere.
/// Margins and sizes are in points, so the area is measured in points first.
pub(crate) fn notice_corner(platform: Platform, usable: Area, scale: f64) -> (f64, f64) {
    let (width, height) = notice_size(platform);
    let points = Area {
        x: usable.x / scale,
        y: usable.y / scale,
        width: usable.width / scale,
        height: usable.height / scale,
    };
    let (x, y) = notice_position(platform, points, width, height);
    (x * scale, y * scale)
}

/// One line of the copied text, cut at a character boundary.
pub(crate) fn summary(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= TEXT_LIMIT {
        return line;
    }
    let mut cut: String = line.chars().take(TEXT_LIMIT - 1).collect();
    cut.push('…');
    cut
}

/// What the notice shows now, and which showing is the latest, so an
/// earlier notice's timer never hides a later one.
#[derive(Default)]
pub(crate) struct Notice {
    text: Mutex<Option<String>>,
    showing: AtomicU64,
    /// Held while the window is looked up or created, so two quick copies
    /// cannot both create it.
    window: Mutex<()>,
}

/// The text of the notice being shown, for a notice page that has just
/// loaded and missed the event that announced it.
#[tauri::command]
#[specta::specta]
pub(crate) fn copied_notice(notice: State<'_, Notice>) -> Option<String> {
    notice.text.lock().ok().and_then(|text| text.clone())
}

/// Say "Copied" after the picker has hidden. The interface calls this only
/// after a copy without warnings that hid the picker (see `copyFeedback.ts`);
/// copies with warnings keep their notice inside the picker. Failures are
/// reported as fixed diagnostics: the citation is on the clipboard either way.
#[tauri::command]
#[specta::specta]
pub(crate) async fn show_copied_notice(app: AppHandle, text: String) {
    show(&app, summary(&text));
}

fn show(app: &AppHandle, text: String) {
    let notice = app.state::<Notice>();
    if let Ok(mut current) = notice.text.lock() {
        *current = Some(text.clone());
    }
    let showing = notice.showing.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
    let timer = {
        let Ok(_creating) = notice.window.lock() else {
            return;
        };
        match app.get_webview_window(LABEL) {
            Some(window) => {
                // Already open (or, on macOS, kept hidden): move it to the
                // picker's display and replay it with the new text.
                if let Some(corner) = position(app) {
                    report(window.set_position(corner.position()), NativeIssue::Position);
                }
                report(window.emit_to(LABEL, "notice:show", &text), NativeIssue::Notice);
                present(app);
                SHOWN_FOR
            }
            // Its page starts the usual timer once it has loaded.
            None if create(app).is_some() => LOAD_LIMIT,
            None => return,
        }
    };
    dismiss_after(app, showing, timer);
}

/// When a notice in view for `shown` starts fading out, and how long after
/// that its window goes: the fade ends as the visible time does.
fn leave_schedule(shown: Duration) -> (Duration, Duration) {
    (shown.saturating_sub(FADE_OUT), FADE_OUT + GRACE)
}

/// Fade the notice out and take it away after `delay`, unless a later notice
/// replaced it.
fn dismiss_after(app: &AppHandle, showing: u64, delay: Duration) {
    let handle = app.clone();
    let (leave_at, close_after) = leave_schedule(delay);
    let timer = std::thread::Builder::new().name("ibis-notice".into()).spawn(move || {
        let current = || handle.state::<Notice>().showing.load(Ordering::SeqCst) == showing;
        std::thread::sleep(leave_at);
        if !current() {
            return;
        }
        if let Some(window) = handle.get_webview_window(LABEL) {
            report(window.emit_to(LABEL, "notice:leave", ()), NativeIssue::Notice);
        }
        std::thread::sleep(close_after);
        if current() {
            dismiss(&handle);
        }
    });
    if timer.is_err() {
        // Without a timer the notice would stay; take it away at once.
        dismiss(app);
    }
}

/// Where the notice goes on the display the picker was on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Corner {
    /// In the shared space of `placement`: points on macOS, pixels elsewhere.
    shared: (f64, f64),
    /// Points to pixels on the target display; 1 on macOS.
    scale: f64,
}

impl Corner {
    /// For moving an existing window. Off macOS the pixels are used as they
    /// are, like the picker does, so the window's current display scale
    /// never converts them (#517 review).
    fn position(self) -> tauri::Position {
        let (x, y) = self.shared;
        if cfg!(target_os = "macos") {
            tauri::LogicalPosition::new(x, y).into()
        } else {
            tauri::PhysicalPosition::new(x.round() as i32, y.round() as i32).into()
        }
    }

    /// For a new window's builder, which takes points; tao converts them back
    /// at the scale of the display that holds them.
    fn points(self) -> (f64, f64) {
        (self.shared.0 / self.scale, self.shared.1 / self.scale)
    }
}

fn position(app: &AppHandle) -> Option<Corner> {
    let picker = app.get_webview_window("main")?;
    let monitor = checked(picker.current_monitor(), NativeIssue::Monitor)
        .flatten()
        .or_else(|| checked(app.primary_monitor(), NativeIssue::Monitor).flatten())?;
    let scale = if cfg!(target_os = "macos") {
        1.0
    } else {
        monitor.scale_factor()
    };
    Some(Corner {
        shared: notice_corner(Platform::CURRENT, display(&monitor).usable, scale),
        scale,
    })
}

fn create(app: &AppHandle) -> Option<()> {
    let (width, height) = notice_size(Platform::CURRENT);
    // WebView2 refuses a second webview whose browser arguments differ from
    // the picker's, and they keep it offline (#201), so reuse them exactly.
    let browser_args = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == "main")
        .and_then(|window| window.additional_browser_args.clone());
    let rounded = rounded_surface();
    // The page squares its corners and fills the window when it cannot be transparent.
    let page = if rounded {
        "notice.html"
    } else {
        "notice.html?surface=opaque"
    };
    let mut builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App(page.into()))
        .title(TITLE)
        .inner_size(width, height)
        .min_inner_size(width, height)
        .max_inner_size(width, height)
        // GTK sizes a fixed-size window by its webview's request (200 px
        // tall), so on Linux the size limits above keep it to the toast.
        .resizable(cfg!(target_os = "linux"))
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .focused(false)
        .focusable(false);
    if let Some(corner) = position(app) {
        let (x, y) = corner.points();
        builder = builder.position(x, y);
    }
    if let Some(args) = browser_args.as_deref() {
        builder = builder.additional_browser_args(args);
    }
    // Transparent where something composites it, so the notice keeps its
    // rounded shape over an opaque surface the page draws. Without a
    // compositor (plain X11) a transparent window maps black, so it is an
    // opaque square toast in the toast's colour from the first frame.
    builder = if rounded {
        builder.transparent(true)
    } else {
        builder.background_color(tauri::webview::Color(0x3a, 0x3a, 0x3e, 0xff))
    };
    // Windows shows it as it is created: tao shows a window built unfocused
    // without activating it, which a later show would not do. macOS shows it
    // once it is a non-activating panel.
    // Linux maps it once its page has drawn, so no empty (black) frame shows
    // first; GTK never focuses it, since it does not accept focus.
    builder = builder.visible(cfg!(target_os = "windows"));
    // The page fades in once loaded, so the visible time counts from then.
    builder = builder.on_page_load(|window, payload| {
        if payload.event() != tauri::webview::PageLoadEvent::Finished {
            return;
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            report(window.show(), NativeIssue::Notice);
            report(window.set_ignore_cursor_events(true), NativeIssue::Notice);
        }
        let app = window.app_handle();
        let showing = app.state::<Notice>().showing.load(Ordering::SeqCst);
        dismiss_after(app, showing, SHOWN_FOR);
    });
    let window = checked(builder.build(), NativeIssue::Notice)?;
    #[cfg(target_os = "macos")]
    macos::configure(app, &window);
    // Clicks pass through, including its transparent margins, so it never
    // blocks what is under it (macOS does this in `configure`).
    #[cfg(target_os = "windows")]
    report(window.set_ignore_cursor_events(true), NativeIssue::Notice);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = window;
    Some(())
}

/// Whether the window can be transparent with rounded corners: always on
/// macOS and Windows, which composite every window; on Linux only under
/// Wayland or an X11 compositing manager.
fn rounded_surface() -> bool {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    return true;
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let wayland = std::env::var("WAYLAND_DISPLAY").ok();
        let backend = std::env::var("GDK_BACKEND").ok();
        let on_wayland = uses_wayland(wayland.as_deref(), backend.as_deref());
        linux_rounded(on_wayland, x11_composited)
    }
}

/// GTK uses Wayland when a Wayland display is present, unless GDK_BACKEND
/// asks for X11 first.
#[cfg(any(not(any(target_os = "macos", target_os = "windows")), test))]
fn uses_wayland(wayland_display: Option<&str>, gdk_backend: Option<&str>) -> bool {
    wayland_display.is_some_and(|display| !display.is_empty())
        && !gdk_backend.is_some_and(|backend| backend.trim_start().starts_with("x11"))
}

/// Every Wayland compositor composites. On X11 only a running compositing
/// manager does; an unknown answer keeps the opaque window, which is always
/// drawn correctly.
#[cfg(any(not(any(target_os = "macos", target_os = "windows")), test))]
fn linux_rounded(wayland: bool, x11_composited: impl FnOnce() -> Result<bool, ()>) -> bool {
    wayland || x11_composited() == Ok(true)
}

/// A compositing manager owns the `_NET_WM_CM_S<screen>` selection (EWMH),
/// which is what GDK's own `is_composited` checks on X11.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn x11_composited() -> Result<bool, ()> {
    use x11rb::protocol::xproto::ConnectionExt;
    let (connection, screen) = x11rb::connect(None).map_err(|_| ())?;
    let name = format!("_NET_WM_CM_S{screen}");
    let atom = connection
        .intern_atom(false, name.as_bytes())
        .map_err(|_| ())?
        .reply()
        .map_err(|_| ())?
        .atom;
    let owner = connection
        .get_selection_owner(atom)
        .map_err(|_| ())?
        .reply()
        .map_err(|_| ())?
        .owner;
    Ok(owner != x11rb::NONE)
}

/// Bring a kept notice back to the front without activating Ibis.
fn present(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    macos::present(app);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

/// Take the notice away. Windows and Linux close it, since showing a hidden
/// window again could activate it; macOS keeps its panel hidden for reuse.
fn dismiss(app: &AppHandle) {
    if let Ok(mut text) = app.state::<Notice>().text.lock() {
        *text = None;
    }
    #[cfg(target_os = "macos")]
    macos::hide(app);
    #[cfg(not(target_os = "macos"))]
    if let Some(window) = app.get_webview_window(LABEL) {
        report(window.destroy(), NativeIssue::Notice);
    }
}

/// macOS: a non-activating panel that cannot become key or main, joins every
/// Space including full-screen ones, ignores the mouse and stays visible
/// while Ibis is hidden, so the writing app stays active and in front.
#[cfg(target_os = "macos")]
mod macos {
    use tauri::AppHandle;
    use tauri_nspanel::{CollectionBehavior, ManagerExt, PanelLevel, WebviewWindowExt, tauri_panel};

    use crate::diagnostics::{NativeIssue, checked, report};

    tauri_panel! {
        panel!(NoticePanel {
            config: {
                can_become_key_window: false,
                can_become_main_window: false,
                is_floating_panel: true
            }
        })
    }

    pub(super) fn configure(app: &AppHandle, window: &tauri::WebviewWindow) {
        let window = window.clone();
        report(
            app.run_on_main_thread(move || {
                let Some(panel) = checked(window.to_panel::<NoticePanel>(), NativeIssue::Notice) else {
                    return;
                };
                if panel
                    .set_style_mask(
                        panel.as_panel().styleMask()
                            | tauri_nspanel::objc2_app_kit::NSWindowStyleMask::NonactivatingPanel,
                    )
                    .is_err()
                {
                    // Without non-activating behaviour it could take focus; never show it.
                    report(Err::<(), ()>(()), NativeIssue::Notice);
                    report(window.destroy(), NativeIssue::Notice);
                    return;
                }
                panel.set_collection_behavior(
                    CollectionBehavior::new()
                        .can_join_all_spaces()
                        .full_screen_auxiliary()
                        .ignores_cycle()
                        .value(),
                );
                panel.set_level(PanelLevel::Status.value());
                panel.set_hides_on_deactivate(false);
                panel.set_ignores_mouse_events(true);
                // The picker's dismissal hides the whole app; the notice must stay.
                panel.as_panel().setCanHide(false);
                panel.order_front_regardless();
            }),
            NativeIssue::Notice,
        );
    }

    pub(super) fn present(app: &AppHandle) {
        let handle = app.clone();
        report(
            app.run_on_main_thread(move || {
                if let Some(panel) = checked(handle.get_webview_panel(super::LABEL), NativeIssue::Notice) {
                    panel.order_front_regardless();
                }
            }),
            NativeIssue::Notice,
        );
    }

    pub(super) fn hide(app: &AppHandle) {
        let handle = app.clone();
        report(
            app.run_on_main_thread(move || {
                if let Some(panel) = checked(handle.get_webview_panel(super::LABEL), NativeIssue::Notice) {
                    panel.hide();
                }
            }),
            NativeIssue::Notice,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn rect(x: f64, y: f64, width: f64, height: f64) -> Area {
        Area { x, y, width, height }
    }

    /// A display with a menu bar or taskbar, and one left of it with
    /// negative coordinates.
    const USABLE: Area = rect(0.0, 38.0, 1728.0, 1079.0);
    const LEFT: Area = rect(-2560.0, 0.0, 2560.0, 1392.0);

    #[test]
    fn the_fade_out_ends_when_the_visible_time_does() {
        let (leave_at, close_after) = leave_schedule(SHOWN_FOR);
        assert_eq!(leave_at, Duration::from_millis(2_600));
        assert_eq!(leave_at + FADE_OUT, SHOWN_FOR);
        assert!(close_after > FADE_OUT);
        // A delay shorter than the fade starts it at once.
        assert_eq!(leave_schedule(Duration::from_millis(50)).0, Duration::ZERO);
    }

    #[test]
    fn macos_shows_a_pill_near_the_top_centre() {
        let (width, height) = notice_size(Platform::MacOs);
        let (x, y) = notice_position(Platform::MacOs, USABLE, width, height);
        assert_eq!(x + width / 2.0, USABLE.x + USABLE.width / 2.0);
        assert_eq!(y, 38.0 + 24.0);
    }

    #[test]
    fn windows_shows_a_card_at_the_bottom_right_above_the_taskbar() {
        let (width, height) = notice_size(Platform::Windows);
        let (x, y) = notice_position(Platform::Windows, LEFT, width, height);
        assert_eq!(x + width, LEFT.x + LEFT.width);
        assert_eq!(y + height, LEFT.y + LEFT.height);
    }

    #[test]
    fn linux_shows_a_toast_at_the_bottom_centre() {
        let (width, height) = notice_size(Platform::Linux);
        let (x, y) = notice_position(Platform::Linux, LEFT, width, height);
        assert_eq!(x + width / 2.0, LEFT.x + LEFT.width / 2.0);
        assert_eq!(y + height, LEFT.y + LEFT.height - 28.0);
    }

    #[test]
    fn a_notice_larger_than_the_display_starts_at_its_corner() {
        let tiny = rect(100.0, 50.0, 200.0, 30.0);
        for platform in [Platform::MacOs, Platform::Windows, Platform::Linux] {
            let (width, height) = notice_size(platform);
            let (x, y) = notice_position(platform, tiny, width, height);
            assert!(x >= tiny.x && y >= tiny.y, "{platform:?} placed it off the display");
        }
    }

    /// A second copy moves the open window with pixels, not points, so a
    /// display with a different scale than the window's current one cannot
    /// misplace it; a new window gets points for its builder.
    #[test]
    fn corners_are_in_display_pixels_and_points_for_new_windows() {
        let hidpi = rect(1920.0, 0.0, 3840.0, 2112.0);
        let corner = Corner {
            shared: notice_corner(Platform::Windows, hidpi, 2.0),
            scale: 2.0,
        };
        // The card's corner meets the display's bottom-right corner.
        assert_eq!(corner.shared, (1920.0 + 3840.0 - 840.0, 2112.0 - 224.0));
        assert_eq!(corner.points(), (2460.0, 944.0));
        // The toast's 28-point margin is 42 pixels at 150%.
        let linux = notice_corner(Platform::Linux, hidpi, 1.5);
        assert_eq!(linux, (1920.0 + (3840.0 - 540.0) / 2.0, 2112.0 - 66.0 - 42.0));
    }

    #[test]
    fn linux_rounds_the_toast_only_where_it_is_composited() {
        assert!(linux_rounded(true, || panic!("Wayland always composites")));
        assert!(linux_rounded(false, || Ok(true)));
        assert!(!linux_rounded(false, || Ok(false)), "plain X11 maps transparency black");
        assert!(!linux_rounded(false, || Err(())), "unknown keeps the opaque toast");
        assert!(uses_wayland(Some("wayland-0"), None));
        assert!(uses_wayland(Some("wayland-0"), Some("wayland,x11")));
        assert!(!uses_wayland(Some("wayland-0"), Some("x11")));
        assert!(!uses_wayland(Some("wayland-0"), Some("x11,wayland")));
        assert!(!uses_wayland(Some(""), None));
        assert!(!uses_wayland(None, None));
    }

    #[test]
    fn the_summary_is_one_line_cut_at_a_character_boundary() {
        assert_eq!(summary("[@a;\n  @b]"), "[@a; @b]");
        assert_eq!(summary(""), "");
        let long = "研".repeat(TEXT_LIMIT + 10);
        let cut = summary(&long);
        assert_eq!(cut.chars().count(), TEXT_LIMIT);
        assert!(cut.ends_with('…'));
        assert_eq!(summary(&"a".repeat(TEXT_LIMIT)).chars().count(), TEXT_LIMIT);
    }
}
