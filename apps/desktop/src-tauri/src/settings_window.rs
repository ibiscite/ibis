//! The Settings window (#776): a window of its own that, while open, shows in
//! the Dock and the app switcher like any Mac settings window. On macOS it is
//! made once and hidden on close (`close_decision`); `dismiss` is the only way
//! it goes away.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

use crate::diagnostics::{NativeIssue, checked, report};
use crate::picker::{PickerGeneration, PickerHideUi, Takeover, yield_to_window};

pub(crate) const LABEL: &str = "settings";
/// The design canvas's size (docs/design/settings.md).
const SIZE: (f64, f64) = (920.0, 560.0);
const MIN_SIZE: (f64, f64) = (800.0, 400.0);
/// Room kept round the window, for its frame and title bar, on a small screen.
const MARGIN: f64 = 32.0;

/// Open the native-owned settings path in its registered editor. An absent
/// file is created through the owner; unreadable existing bytes stay intact.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn open_settings_file(app: AppHandle) -> Result<(), crate::library::CommandError> {
    let file = app.state::<crate::preferences::SettingsFile>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        file.owner.ensure_file().map_err(crate::library::CommandError::from)?;
        let path = file
            .path
            .canonicalize()
            .map_err(|_| crate::errors::NativeOperation::OpenSettingsFile.failure())?;
        crate::system_open::open(path.as_os_str(), false)
            .map_err(|()| crate::errors::NativeOperation::OpenSettingsFile.failure())
    })
    .await
    .map_err(|_| crate::errors::NativeOperation::OpenSettingsFile.failure())?
}

/// Settings' size and minimum on a screen whose work area is `area` in logical
/// pixels: the design size where it fits, never larger than the screen (a 1440
/// × 900 display at GDK_SCALE=2 is 720 × 450), so no control is ever off it.
/// An unknown or empty area (displays changing) keeps the design size.
fn fitted(area: Option<(f64, f64)>) -> ((f64, f64), (f64, f64)) {
    let Some((width, height)) = area.filter(|&(width, height)| width >= 1.0 && height >= 1.0) else {
        return (SIZE, MIN_SIZE);
    };
    let room = ((width - 2.0 * MARGIN).max(1.0), (height - 2.0 * MARGIN).max(1.0));
    let size = (SIZE.0.min(room.0), SIZE.1.min(room.1));
    (size, (MIN_SIZE.0.min(size.0), MIN_SIZE.1.min(size.1)))
}

pub(crate) struct InitialFrame {
    origin: (f64, f64),
    size: (f64, f64),
    minimum: (f64, f64),
}

/// Pick the display and a creation position before the native window's first
/// size is converted using that display's DPI. No move after build is needed.
pub(crate) fn initial_frame(app: &AppHandle) -> Option<InitialFrame> {
    let picker = app
        .get_webview_window("main")
        .and_then(|window| window.current_monitor().ok().flatten());
    let monitor = picker.or_else(|| app.primary_monitor().ok().flatten())?;
    let monitors = checked(app.available_monitors(), NativeIssue::Monitor)?;
    let target = monitors
        .iter()
        .position(|m| m.position() == monitor.position() && m.size() == monitor.size())?;
    let screens: Vec<_> = monitors
        .iter()
        .map(|m| {
            let area =
                |position: tauri::PhysicalPosition<i32>, size: tauri::PhysicalSize<u32>| crate::placement::Area {
                    x: f64::from(position.x),
                    y: f64::from(position.y),
                    width: f64::from(size.width),
                    height: f64::from(size.height),
                };
            crate::placement::CreationDisplay {
                bounds: area(*m.position(), *m.size()),
                usable: area(m.work_area().position, m.work_area().size),
                scale: m.scale_factor(),
            }
        })
        .collect();
    let room = monitor.work_area().size.to_logical::<f64>(monitor.scale_factor());
    let (size, minimum) = fitted(Some((room.width, room.height)));
    // Some Windows layouts have no unambiguous logical creation point. Keep
    // the fitted target frame and the previous best-effort centre there; the
    // builder needs a physical-position API to guarantee that remaining case.
    let origin = crate::placement::creation_origin(&screens, target, size, MARGIN, cfg!(target_os = "windows"))
        .unwrap_or_else(|| {
            let at = monitor.work_area().position.to_logical::<f64>(monitor.scale_factor());
            (at.x + (room.width - size.0) / 2.0, at.y + (room.height - size.1) / 2.0)
        });
    Some(InitialFrame { origin, size, minimum })
}

/// Where Settings opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Page {
    General,
    /// General, scrolled to pasting and its permission.
    Paste,
    Look,
    Libraries,
    Advanced,
    Shortcuts,
    Citations,
    About,
}

/// Open Settings, or bring it forward, on `page` (General by default). Async so
/// the window is built from the event loop: a sync command runs inside the
/// WebView2 callback, where building a second webview deadlocks on Windows.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn open_settings(app: AppHandle, page: Option<Page>, presentation: Option<u64>) {
    // Tauri spawns async commands before their bodies run; the picker names
    // the opening that asked, so a newer one is not what gives way.
    let presentation = app.state::<PickerGeneration>().originating(presentation);
    show(&app, page, presentation);
}

/// Read the current presentation, acknowledging `revision` once its page has
/// rendered. The event listener is installed before the first read.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn settings_shown(app: AppHandle, revision: Option<u64>) -> SettingsPresentation {
    if let Some(revision) = revision {
        let handle = app.clone();
        let (send, mut receive) = tauri::async_runtime::channel(1);
        let dispatched = app.run_on_main_thread(move || {
            reveal(&handle, revision);
            let _ = send.try_send(());
        });
        if checked(dispatched, NativeIssue::Window).is_some() {
            let _ = receive.recv().await;
        }
    }
    app.state::<Presentations>().current()
}

/// Close the paste guide after both grants; a fresh external activation opens
/// the picker again. Granting permission never captures a target or retries Enter.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn finish_paste_guide(app: AppHandle, revision: u64) -> Result<bool, crate::errors::NativeFailure> {
    let (sender, mut receiver) = tauri::async_runtime::channel(1);
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let result = (|| {
            let Some(window) = handle.get_webview_window(LABEL) else {
                return Ok(false);
            };
            handle
                .state::<Presentations>()
                .finish_guide(
                    revision,
                    crate::paste::permission::paste_permission().granted,
                    window.is_focused().unwrap_or(false),
                    || dismiss(&handle),
                )
                .map_err(|_| crate::errors::NativeFailure)
        })();
        let _ = sender.try_send(result);
    })
    .map_err(|_| crate::errors::NativeFailure)?;
    receiver.recv().await.ok_or(crate::errors::NativeFailure)?
}

pub(crate) fn open(app: &AppHandle, page: Option<Page>) {
    show(app, page, app.state::<PickerGeneration>().current());
}

fn show(app: &AppHandle, page: Option<Page>, presentation: u64) {
    let handle = app.clone();
    report(
        app.run_on_main_thread(move || present(&handle, page, presentation)),
        NativeIssue::Window,
    );
}

/// Every way into Settings ends here, so each has one effect on the picker.
pub(crate) fn present<R: PickerHideUi>(app: &AppHandle<R>, page: Option<Page>, presentation: u64) {
    let requested = app.state::<Presentations>().ask(page.unwrap_or(Page::General));
    if app.get_webview_window(LABEL).is_none() && !build(app) {
        destroyed(app);
        return;
    }
    publish(app, requested);
    yield_to_window(app, presentation, Takeover::Covered);
}

fn build<R: PickerHideUi>(app: &AppHandle<R>) -> bool {
    let url = format!("settings.html?language={}", ibis_engine::catalogue::language().tag());
    let frame = R::settings_frame(app);
    let (size, min) = frame
        .as_ref()
        .map_or((SIZE, MIN_SIZE), |frame| (frame.size, frame.minimum));
    let mut builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App(url.into()))
        .title(ibis_engine::catalogue::text("app-window-settings", None, &[]).unwrap_or_default())
        .inner_size(size.0, size.1)
        .min_inner_size(min.0, min.1)
        .maximizable(false)
        .focused(false)
        // A system alert leaves Settings without the keyboard; the click that
        // returns to it must also reach the control under the pointer.
        .accept_first_mouse(true)
        // Only the rendered-page acknowledgement can bring it forward.
        .visible(false);
    builder = match frame {
        Some(frame) => builder.position(frame.origin.0, frame.origin.1),
        None => builder.center(),
    };
    // The sidebar runs under the traffic lights, as in System Settings, whose
    // window cannot be minimised and is named after its page (the page sets
    // `document.title`; the static English one never reaches other systems).
    #[cfg(target_os = "macos")]
    {
        builder = builder
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true)
            .minimizable(false)
            .on_document_title_changed(|window, title| {
                if !title.is_empty() {
                    report(window.set_title(&title), NativeIssue::Window);
                }
            });
    }
    // WebView2 refuses a second webview whose arguments differ from the
    // picker's, and they keep it offline (#201), so reuse them exactly.
    if let Some(args) = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == "main")
        .and_then(|window| window.additional_browser_args.clone())
    {
        builder = builder.additional_browser_args(&args);
    }
    let built = checked(builder.build(), NativeIssue::Window);
    #[cfg(target_os = "linux")]
    crate::desktop_theme::reapply(app);
    built.is_some()
}

fn publish<R: Runtime>(app: &AppHandle<R>, presentation: SettingsPresentation) {
    report(
        app.emit_to(LABEL, "settings:presentation", presentation),
        NativeIssue::Window,
    );
}

/// Runs on the native thread, after IPC scheduling. The revision is consumed
/// here so a newer page or a picker summon can still withdraw this reveal.
fn reveal<R: Runtime>(app: &AppHandle<R>, revision: u64) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    if !app.state::<Presentations>().claim(revision) {
        return;
    }
    #[cfg(target_os = "macos")]
    {
        report(
            app.set_activation_policy(tauri::ActivationPolicy::Regular),
            NativeIssue::Window,
        );
        // A dismissed picker may have hidden the whole app.
        report(app.show(), NativeIssue::Show);
    }
    report(window.unminimize(), NativeIssue::Unminimize);
    if checked(window.show(), NativeIssue::Show).is_none() && !window.is_visible().unwrap_or(false) {
        retire(app);
        closed(app);
        return;
    }
    report(window.set_focus(), NativeIssue::Focus);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub(crate) struct SettingsOpening {
    id: u64,
    page: Page,
}

/// Every page request and close advances the revision. An already open window
/// keeps its opening id when a request brings it forward on another page.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub(crate) struct SettingsPresentation {
    pub(crate) revision: u64,
    opening: Option<SettingsOpening>,
}

impl SettingsPresentation {
    pub(crate) fn opening_id(self) -> Option<u64> {
        self.opening.map(|opening| opening.id)
    }
}

#[derive(Default)]
struct PresentationState {
    current: SettingsPresentation,
    waiting: bool,
    shown: bool,
}

impl PresentationState {
    fn close(&mut self) -> Option<SettingsPresentation> {
        self.current.opening.take()?;
        self.current.revision += 1;
        self.waiting = false;
        self.shown = false;
        Some(self.current)
    }
}

#[derive(Default)]
pub(crate) struct Presentations(Mutex<PresentationState>);

impl Presentations {
    fn state(&self) -> std::sync::MutexGuard<'_, PresentationState> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn current(&self) -> SettingsPresentation {
        self.state().current
    }

    pub(crate) fn ask(&self, page: Page) -> SettingsPresentation {
        let mut state = self.state();
        state.current.revision += 1;
        let id = state.current.opening_id().unwrap_or(state.current.revision);
        state.current.opening = Some(SettingsOpening { id, page });
        state.waiting = true;
        state.current
    }

    pub(crate) fn claim(&self, revision: u64) -> bool {
        let mut state = self.state();
        if !state.waiting || state.current.revision != revision {
            return false;
        }
        state.waiting = false;
        state.shown = true;
        true
    }

    fn withdraw(&self) -> Option<SettingsPresentation> {
        let mut state = self.state();
        state.waiting = false;
        if state.shown { None } else { state.close() }
    }

    pub(crate) fn close(&self) -> Option<SettingsPresentation> {
        self.state().close()
    }

    /// Read or reserve state for the named opening in the same step as its
    /// lifetime check. The action may take a lock but must not call native UI.
    pub(crate) fn with_opening<T>(&self, id: u64, action: impl FnOnce() -> T) -> Option<T> {
        let state = self.state();
        (state.shown && state.current.opening_id() == Some(id)).then(action)
    }

    fn finish_guide<E>(
        &self,
        revision: u64,
        granted: bool,
        focused: bool,
        close: impl FnOnce() -> Result<(), E>,
    ) -> Result<bool, E> {
        let owns_guide = {
            let state = self.state();
            state.shown
                && !state.waiting
                && state.current.revision == revision
                && state.current.opening.is_some_and(|opening| opening.page == Page::Paste)
        };
        if !owns_guide || !granted || !focused {
            return Ok(false);
        }
        close()?;
        // The next deliberate summon captures an external target for a fresh Enter.
        Ok(true)
    }
}

/// The picker opened: a Settings window still loading must not take the
/// keyboard from it when its page arrives.
pub(crate) fn picker_opened(app: &AppHandle) {
    if let Some(hidden) = app.state::<Presentations>().withdraw() {
        publish(app, hidden);
    }
}

/// How Settings goes away when it is closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Close {
    /// Hidden and kept for the next opening, as a Mac app keeps its Settings
    /// window: AppKit holds on to the shell of every destroyed one.
    Hide,
    /// Destroyed once its page has saved; the next opening builds a new one.
    Destroy,
}

/// The one rule for closing Settings. Windows and Linux free a destroyed
/// window, while a hidden WebView2 or WebKitGTK view would keep its processes
/// and memory for a window that is rarely open.
pub(crate) const fn close_decision(macos: bool) -> Close {
    if macos { Close::Hide } else { Close::Destroy }
}

/// The only way Settings goes away: a closed window, a finished paste guide.
pub(crate) fn dismiss<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    dismiss_as(app, close_decision(cfg!(target_os = "macos")))
}

pub(crate) fn dismiss_as<R: Runtime>(app: &AppHandle<R>, close: Close) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window(LABEL) else {
        return Ok(());
    };
    match close {
        Close::Hide => {
            window.hide()?;
            retire(app);
            closed(app);
            Ok(())
        }
        // Through CloseRequested, so the page saves before it is destroyed.
        Close::Destroy => window.close(),
    }
}

pub(crate) fn destroyed<R: Runtime>(app: &AppHandle<R>) {
    end_opening(app);
    closed(app);
}

fn retire<R: Runtime>(app: &AppHandle<R>) {
    if let Some(hidden) = end_opening(app) {
        publish(app, hidden);
    }
}

fn end_opening<R: Runtime>(app: &AppHandle<R>) -> Option<SettingsPresentation> {
    let hidden = app.state::<Presentations>().close()?;
    app.state::<crate::settings_backup::PendingRestore>().clear();
    Some(hidden)
}

/// Settings is on screen, so hiding the whole app would hide it too.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn is_open(app: &AppHandle) -> bool {
    app.get_webview_window(LABEL)
        .is_some_and(|window| window.is_visible().unwrap_or(false))
}

/// Once Settings closes, Ibis leaves the Dock again and lives in the menu bar,
/// handing focus back to the app underneath unless the picker is up.
fn closed<R: Runtime>(app: &AppHandle<R>) {
    #[cfg(target_os = "macos")]
    {
        report(
            app.set_activation_policy(tauri::ActivationPolicy::Accessory),
            NativeIssue::Window,
        );
        let picker_shown = app
            .get_webview_window("main")
            .is_some_and(|window| window.is_visible().unwrap_or(false));
        if !picker_shown {
            report(app.hide(), NativeIssue::HideApplication);
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returning_requires_both_grants_and_focus_then_waits_for_a_fresh_external_activation() {
        use std::cell::RefCell;
        let presentations = Presentations::default();
        let guide = presentations.ask(Page::Paste);
        assert!(presentations.claim(guide.revision));
        for (granted, focused) in [(false, false), (false, true), (true, false), (true, true)] {
            let calls = RefCell::new(Vec::new());
            let result = presentations.finish_guide(guide.revision, granted, focused, || {
                calls.borrow_mut().push("close");
                Ok::<_, ()>(())
            });
            assert_eq!(result, Ok(granted && focused));
            assert_eq!(*calls.borrow(), if granted && focused { vec!["close"] } else { vec![] });
        }
        assert_eq!(
            presentations.finish_guide(guide.revision, true, true, || Err("close failed")),
            Err("close failed")
        );
    }

    #[test]
    fn only_macos_keeps_settings_when_it_closes() {
        assert_eq!(close_decision(true), Close::Hide);
        assert_eq!(close_decision(false), Close::Destroy);
    }

    #[test]
    fn settings_first_load_only_reveals_after_a_render_acknowledgement() {
        let presentations = Presentations::default();
        let first = presentations.ask(Page::General);
        assert_eq!(
            presentations.current(),
            first,
            "a page that misses the event can read it"
        );
        let id = first.opening_id().unwrap();
        assert_eq!(
            presentations.with_opening(id, || ()),
            None,
            "window creation and navigation did not reveal Settings"
        );
        assert!(presentations.claim(first.revision));
        assert_eq!(presentations.with_opening(id, || ()), Some(()));
        assert!(
            !presentations.claim(first.revision),
            "a duplicate acknowledgement cannot focus it again"
        );
    }

    #[test]
    fn settings_revalidates_a_queued_reveal_after_a_picker_summon_or_newer_request() {
        let presentations = Presentations::default();
        let first = presentations.ask(Page::Shortcuts);
        // The webview has acknowledged Shortcuts. Before its queued native
        // reveal runs, the picker is summoned and Settings is asked for again.
        let hidden = presentations.withdraw().unwrap();
        assert_eq!(hidden.opening, None);
        assert!(!presentations.claim(first.revision));
        let reopened = presentations.ask(Page::Shortcuts);
        assert_ne!(first.opening.unwrap().id, reopened.opening.unwrap().id);
        let newer = presentations.ask(Page::About);
        assert!(!presentations.claim(first.revision));
        assert!(!presentations.claim(reopened.revision));
        assert!(presentations.claim(newer.revision));
        assert!(!presentations.claim(newer.revision));
    }

    #[test]
    fn settings_rejects_guide_dismissals_after_close_or_another_page_request() {
        let presentations = Presentations::default();
        let first = presentations.ask(Page::Paste);
        assert!(presentations.claim(first.revision));
        let about = presentations.ask(Page::About);
        assert_eq!(about.opening.unwrap().id, first.opening.unwrap().id);
        assert!(presentations.claim(about.revision));
        assert_eq!(
            presentations.finish_guide(first.revision, true, true, || Err(())),
            Ok(false)
        );

        presentations.close();
        let second = presentations.ask(Page::Paste);
        assert_ne!(first.opening.unwrap().id, second.opening.unwrap().id);
        assert_eq!(
            presentations.finish_guide(second.revision, true, true, || Err(())),
            Ok(false)
        );
        assert!(presentations.claim(second.revision));
        assert_eq!(
            presentations.finish_guide(first.revision, true, true, || Err(())),
            Ok(false)
        );
        assert_eq!(
            presentations.finish_guide(second.revision, true, true, || Ok::<_, ()>(())),
            Ok(true)
        );
    }

    #[test]
    fn settings_fits_a_small_or_scaled_screen_and_keeps_its_size_elsewhere() {
        assert_eq!(
            fitted(None),
            (SIZE, MIN_SIZE),
            "an unknown screen keeps the design size"
        );
        assert_eq!(fitted(Some((1440.0, 900.0))), (SIZE, MIN_SIZE));
        // 1440 × 900 at GDK_SCALE=2: every control stays on the screen.
        let (size, min) = fitted(Some((720.0, 450.0)));
        assert!(
            size.0 + 2.0 * MARGIN <= 720.0 && size.1 + 2.0 * MARGIN <= 450.0,
            "{size:?}"
        );
        assert!(min.0 <= size.0 && min.1 <= size.1, "{min:?} fits within {size:?}");
        assert!(
            fitted(Some((10.0, 10.0))).0.0 > 0.0,
            "a tiny screen still gets a window"
        );
        assert_eq!(
            fitted(Some((0.0, 0.0))),
            (SIZE, MIN_SIZE),
            "an empty area while displays change is unknown"
        );
    }
}
