//! Showing, hiding and quitting, and the global shortcut.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

use crate::diagnostics::{NativeIssue, checked, report};

mod dialogs;
mod full_screen;
mod layout;
#[cfg(target_os = "linux")]
mod portal;
pub(crate) mod wayland;
pub(crate) use wayland::{desktop_owned, start_shortcut};
mod registration;
#[cfg(target_os = "linux")]
mod x11_focus;

pub(crate) use dialogs::ChooserLease;
pub(crate) use dialogs::ChooserOpen;
pub(crate) use full_screen::FullScreenGuard;
pub(crate) use layout::{character, us_character};
#[cfg(target_os = "macos")]
pub(crate) use layout::{command_key, typing};
#[cfg(target_os = "macos")]
pub(crate) use registration::follow_layout;
#[cfg(test)]
pub(crate) use registration::replace_with;
pub(crate) use registration::{
    Registered, SHORTCUT_BACKEND_UNAVAILABLE, on_layout_with, register_shortcut, replace_shortcut,
};

/// Whether the global shortcut is working, shown in the interface.
#[derive(Default, Clone, Serialize, specta::Type)]
pub(crate) struct ShortcutStatus {
    pub(crate) shortcut: String,
    pub(crate) registered: bool,
    pub(crate) error: Option<String>,
    /// Present on Wayland, including when the portal is unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub(crate) desktop: Option<DesktopShortcut>,
}

#[derive(Clone, Serialize, specta::Type)]
pub(crate) struct DesktopShortcut {
    pub(crate) configurable: bool,
    pub(crate) guidance: String,
}

/// Preserve the recorder's intent: `Q` follows a letter, `KeyQ` stays physical.
/// The plugin's parser alone loses this distinction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OpeningShortcut {
    pub(crate) shortcut: Shortcut,
    pub(crate) logical_letter: bool,
}

impl std::str::FromStr for OpeningShortcut {
    type Err = <Shortcut as std::str::FromStr>::Err;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let shortcut = text.parse()?;
        let key = text.rsplit('+').next().unwrap_or_default().trim();
        Ok(Self {
            shortcut,
            logical_letter: key.len() == 1 && key.as_bytes()[0].is_ascii_alphabetic(),
        })
    }
}

/// One default letter everywhere: Cmd+Shift+Y on macOS, Ctrl+Shift+Y elsewhere.
/// macOS registration resolves Y on the active layout before reserving it,
/// avoiding QWERTZ's Redo key. Ctrl/Cmd+Shift+Space belongs to editors or Siri (#133).
pub(crate) fn default_shortcut() -> OpeningShortcut {
    let command = if cfg!(target_os = "macos") {
        Modifiers::SUPER
    } else {
        Modifiers::CONTROL
    };
    OpeningShortcut {
        shortcut: Shortcut::new(Some(command | Modifiers::SHIFT), Code::KeyY),
        logical_letter: true,
    }
}

pub(crate) fn shortcut_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Command+Shift+Y"
    } else {
        "Ctrl+Shift+Y"
    }
}

/// The shortcut from settings (for example `"CommandOrControl+Shift+Y"`), or the
/// default. An invalid setting falls back to the default with an explanation.
pub(crate) fn configured_shortcut(setting: Option<&str>) -> (OpeningShortcut, String, Option<String>) {
    configured_shortcut_with(setting, crate::shortcut_setting::check_shortcut)
}

#[cfg(test)]
pub(crate) fn configured_shortcut_for(
    setting: Option<&str>,
    windows: bool,
) -> (OpeningShortcut, String, Option<String>) {
    let command = if windows || !cfg!(target_os = "macos") {
        Modifiers::CONTROL
    } else {
        Modifiers::SUPER
    };
    configured_shortcut_with(setting, |text| {
        crate::shortcut_setting::check_shortcut_for(text, command, windows)
    })
}

pub(crate) fn configured_shortcut_with(
    setting: Option<&str>,
    validate: impl Fn(&str) -> Result<OpeningShortcut, crate::shortcut_setting::ShortcutRefusal>,
) -> (OpeningShortcut, String, Option<String>) {
    match setting {
        Some(text) => match validate(text) {
            Ok(shortcut) => (shortcut, text.to_string(), None),
            Err(e) => (
                default_shortcut(),
                shortcut_label().to_string(),
                Some(format!(
                    "The saved opening shortcut was refused: {e}; using {}",
                    shortcut_label()
                )),
            ),
        },
        None => (default_shortcut(), shortcut_label().to_string(), None),
    }
}

/// Show the picker on the display with the pointer, focused, and tell the interface to focus the search box.
pub(crate) fn show_picker(app: &AppHandle) {
    crate::updater::picker_activity(app);
    // Accept once, before dispatch/capture. A later request cannot donate its
    // ticket to this show between a separate advance and current() read.
    let owner = app.state::<PickerGeneration>().advance();
    // Frame preparation, show dispatch and activation scheduling must happen
    // together on the native thread. From a Linux shortcut/instance worker,
    // a GLib idle can otherwise beat Tauri's still-undispatched Show message.
    #[cfg(target_os = "macos")]
    let on_picker_thread = tauri_nspanel::objc2::MainThreadMarker::new().is_some();
    #[cfg(target_os = "linux")]
    let on_picker_thread = gtk::is_initialized_main_thread();
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let handle = app.clone();
        on_picker_thread_with(
            on_picker_thread,
            move || show_picker_on_thread(&handle, owner),
            |show| report(app.run_on_main_thread(show), NativeIssue::Show),
        );
    }
    #[cfg(target_os = "windows")]
    {
        let handle = app.clone();
        show_windows_picker_with(
            move || show_picker_on_thread(&handle, owner),
            |show| report(app.run_on_main_thread(show), NativeIssue::Show),
        );
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    show_picker_on_thread(app, owner);
}

#[cfg(any(target_os = "windows", test))]
fn show_windows_picker_with<A: FnOnce()>(action: A, dispatch: impl FnOnce(A)) {
    // A worker must not acquire the appearance cache before synchronously
    // requesting an HWND from a main thread that can be waiting on that cache.
    // Keep frame preparation, appearance refresh and show in one native task.
    dispatch(action);
}

#[cfg(any(target_os = "macos", target_os = "linux", test))]
fn on_picker_thread_with<A: FnOnce()>(on_thread: bool, action: A, dispatch: impl FnOnce(A)) {
    if on_thread {
        action();
    } else {
        dispatch(action);
    }
}

fn show_picker_on_thread(app: &AppHandle, owner: u64) {
    let visits = app.state::<PickerGeneration>();
    if !visits.owns(owner) {
        return;
    }
    // Before the panel takes the front: the app Enter will paste into (#1241).
    crate::paste::capture(app);
    // Native target capture may outlive its request, especially on Windows.
    if !visits.owns(owner) {
        return;
    }
    if let Some(window) = checked(app.get_webview_window("main").ok_or(()), NativeIssue::Window) {
        if !crate::sizing::prepare(app, &window, false, false) || !visits.owns(owner) {
            return;
        }
        crate::native_feel::before_show(app);
        report(window.unminimize(), NativeIssue::Unminimize);
        #[cfg(target_os = "macos")]
        crate::panel::show(app);
        #[cfg(not(target_os = "macos"))]
        {
            // Above always-on-top windows too (#1133); unpinned, it hides
            // once it loses focus, so it never covers anything for long.
            #[cfg(target_os = "windows")]
            report(window.set_always_on_top(true), NativeIssue::AlwaysOnTop);
            present_window(&window, true, owner);
            crate::updater::picker_presented(app);
        }
        report(window.emit("picker:shown", owner), NativeIssue::PickerEvent);
    }
    crate::library::refresh_zotero(app);
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn present_window(window: &tauri::WebviewWindow, focus: bool, owner: u64) {
    if !window.state::<PickerGeneration>().owns(owner) {
        return;
    }
    present_with(
        focus,
        || {
            // tao's GTK focus=false suppresses accept-focus until the first
            // draw, and set_focus can be discarded before its queued show.
            // Restore the hint before first map. Activation is separate and
            // must follow the complete frame and show already queued by tao.
            #[cfg(target_os = "linux")]
            report(window.set_focusable(true), NativeIssue::Focus);
        },
        || {
            report(window.show(), NativeIssue::Show);
            // Showing a hidden quick picker can restore its taskbar button.
            report(window.set_skip_taskbar(true), NativeIssue::SkipTaskbar);
        },
        || {
            #[cfg(target_os = "linux")]
            focus_after_show(window, owner);
            #[cfg(not(target_os = "linux"))]
            report(window.set_focus(), NativeIssue::Focus);
        },
    );
}

#[cfg(target_os = "linux")]
fn focus_after_show(window: &tauri::WebviewWindow, owner: u64) {
    use gtk::prelude::*;
    let pending = window.clone();
    // tao 0.37.1 drains Size/Visible requests on the default GTK context at
    // DEFAULT priority (0). This one-shot idle runs there at DEFAULT_IDLE
    // (200), after those requests. An immediate Tauri set_focus sees the still
    // hidden GTK window and drops the request, leaving the WM unnotified.
    gtk::glib::idle_add_once(move || {
        let Some(native) = checked(pending.gtk_window(), NativeIssue::Window) else {
            return;
        };
        activate_presented_with(
            &pending.state::<PickerGeneration>(),
            owner,
            &pending.state::<ChooserOpen>(),
            || native.get_visible() && native.is_mapped(),
            // This is the same GTK activation request as tao's Focus handler,
            // including Wayland's previously supplied one-use startup token.
            || native.present_with_time(0),
        );
    });
}

#[cfg(any(target_os = "linux", test))]
fn activate_presented_with(
    visits: &PickerGeneration,
    owner: u64,
    chooser: &ChooserOpen,
    mapped: impl FnOnce() -> bool,
    activate: impl FnOnce(),
) {
    // A hide or newer show owns the window now; an old callback must neither
    // reveal it again nor take keyboard focus from the new presentation.
    // A chooser/offer can open synchronously after Show without changing its
    // generation or visibility. Its live lease owns focus until it closes.
    if visits.still_owns_after(owner, mapped) && !chooser.is_open() {
        activate();
    }
}

#[cfg(any(not(target_os = "macos"), test))]
fn present_with(focus: bool, prepare_focus: impl FnOnce(), show: impl FnOnce(), request_focus: impl FnOnce()) {
    if focus {
        prepare_focus();
    }
    show();
    if focus {
        request_focus();
    }
}

/// Non-macOS windows must also start hidden until settings and display bounds
/// have supplied their complete frame. macOS's panel owns the same startup gate.
#[cfg(any(not(target_os = "macos"), test))]
pub(crate) fn prepare_startup<R: tauri::Runtime>(context: &mut tauri::Context<R>) -> (bool, bool) {
    let Some(window) = context
        .config_mut()
        .app
        .windows
        .iter_mut()
        .find(|window| window.label == "main")
    else {
        return (false, false);
    };
    let intent = (window.visible, window.focus);
    window.visible = false;
    window.focus = false;
    intent
}

#[derive(Debug, PartialEq, Eq)]
enum ToggleAction {
    Hide,
    Show,
}

fn toggle_action(visible: Option<bool>, focused: Option<bool>) -> ToggleAction {
    if visible == Some(true) && focused == Some(true) {
        ToggleAction::Hide
    } else {
        ToggleAction::Show
    }
}

// Keep this branch trace out of release binaries. Only fixed structural facts
// are useful for comparing the shortcut decision with native harness focus.
#[cfg(debug_assertions)]
fn toggle_trace(visible: Option<bool>, focused: Option<bool>) -> &'static str {
    match (visible, focused) {
        (Some(true), Some(true)) => "ibis-picker: toggle visible focused hide",
        (Some(true), Some(false)) => "ibis-picker: toggle visible unfocused show",
        (Some(true), None) => "ibis-picker: toggle visible focus-unavailable show",
        _ => "ibis-picker: toggle not-known-visible show",
    }
}

#[cfg(target_os = "linux")]
fn resolve_focus(native: Result<Option<bool>, ()>, toolkit: impl FnOnce() -> Option<bool>) -> Option<bool> {
    match checked(native, NativeIssue::FocusState)? {
        Some(focused) => Some(focused),
        None => toolkit(),
    }
}

pub(crate) fn focused_for_shortcut(window: &tauri::WebviewWindow) -> Option<bool> {
    #[cfg(target_os = "linux")]
    return resolve_focus(wayland::x11_state(desktop_owned(), || x11_focus::query(window)), || {
        checked(window.is_focused(), NativeIssue::FocusState)
    });
    #[cfg(target_os = "windows")]
    {
        // Tao loses its top-level focus cache when the child WebView2 takes
        // keyboard focus. Only this exact foreground HWND owns the shortcut.
        let picker = checked(window.hwnd(), NativeIssue::FocusState).map(|window| window.0 as usize);
        let foreground = winsafe::HWND::GetForegroundWindow().map(|window| window.ptr() as usize);
        owned_foreground(picker, foreground)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    checked(window.is_focused(), NativeIssue::FocusState)
}

#[cfg(any(target_os = "windows", test))]
pub(crate) fn owned_foreground(picker: Option<usize>, foreground: Option<usize>) -> Option<bool> {
    let picker = picker.filter(|window| *window != 0)?;
    let foreground = foreground.filter(|window| *window != 0)?;
    Some(picker == foreground)
}

/// Toggle the picker: hide it if it is visible and focused, show it otherwise.
pub(crate) fn toggle_picker(app: &AppHandle) {
    toggle_picker_with(app, || {});
}

fn toggle_picker_with(app: &AppHandle, before_show: impl FnOnce()) {
    if let Some(window) = app.get_webview_window("main") {
        let visible = checked(window.is_visible(), NativeIssue::Visibility);
        // Preserve the short circuit: a hidden picker does not query focus.
        let focused = if visible == Some(true) {
            focused_for_shortcut(&window)
        } else {
            None
        };
        #[cfg(debug_assertions)]
        eprintln!("{}", toggle_trace(visible, focused));
        match toggle_action(visible, focused) {
            ToggleAction::Hide => dismiss_picker(app),
            ToggleAction::Show if shortcut_allowed(app) => {
                before_show();
                show_picker(app);
            }
            ToggleAction::Show => {
                #[cfg(debug_assertions)]
                eprintln!("ibis-picker: toggle ignored over full-screen app");
            }
        }
    } else {
        before_show();
        show_picker(app);
    }
}

/// Whether the full-screen guard lets this press show the picker.
fn shortcut_allowed(app: &AppHandle) -> bool {
    app.try_state::<FullScreenGuard>()
        .is_none_or(|guard| guard.allows(foreground_full_screen(), std::time::Instant::now()))
}

/// Whether a full-screen window is in front. Only X11 can tell today; macOS
/// and Windows need native calls the current dependencies do not offer
/// safely, so there the shortcut always works (#495).
fn foreground_full_screen() -> Option<bool> {
    #[cfg(target_os = "linux")]
    {
        static FAILURE_REPORTED: AtomicBool = AtomicBool::new(false);
        crate::diagnostics::checked_once(
            wayland::x11_state(desktop_owned(), x11_focus::foreground_full_screen),
            NativeIssue::FullScreen,
            &FAILURE_REPORTED,
        )
        .flatten()
    }
    #[cfg(not(target_os = "linux"))]
    None
}

/// Read the native presentation after subscribing, including on webview reload.
#[tauri::command]
#[specta::specta]
pub(crate) fn picker_presentation(visits: State<'_, PickerGeneration>) -> u64 {
    visits.current()
}

/// Hide only the originating presentation; a delayed request cannot dismiss a reopen.
#[tauri::command]
#[specta::specta]
pub(crate) async fn hide_picker(app: AppHandle, presentation: Option<u64>) -> Result<bool, &'static str> {
    // An uninitialized webview can still dismiss. Freeze its native owner at
    // command entry, before dispatch: this is never an unqualified queued hide.
    let presentation = app.state::<PickerGeneration>().originating(presentation);
    let (send, mut receive) = tauri::async_runtime::channel(1);
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let hidden = dismiss_presented_with(&handle.state::<PickerGeneration>(), presentation, || {
            finish_dismiss(&handle);
        });
        let _ = send.try_send(hidden);
    })
    .map_err(|_| "The picker could not be hidden.")?;
    receive.recv().await.ok_or("The picker could not be hidden.")
}

fn dismiss_presented_with(visits: &PickerGeneration, presentation: u64, dismiss: impl FnOnce()) -> bool {
    if !visits.retire(presentation) {
        return false;
    }
    dismiss();
    true
}

/// A window hide alone leaves Ibis active on macOS, swallowing the next paste.
/// Every dismissal path must also release application focus, unless Settings
/// is open.
pub(crate) fn dismiss_picker(app: &AppHandle) {
    app.state::<PickerGeneration>().advance();
    finish_dismiss(app);
}

fn finish_dismiss(app: &AppHandle) {
    crate::updater::picker_activity(app);
    #[cfg(target_os = "macos")]
    crate::panel::cancel_initial_presentation(app);
    crate::sizing::flush(app, false);
    if let Some(window) = checked(app.get_webview_window("main").ok_or(()), NativeIssue::Window) {
        // The interface keeps its search for a minute after a real hide.
        report(window.emit("picker:hidden", ()), NativeIssue::PickerEvent);
        dismiss_with(
            || window.hide(),
            || {
                // Hiding the app would hide an open Settings window too. The
                // non-activating picker rarely took focus from the app in
                // front; when it did (Choose library…), Settings, the user's
                // other open Ibis window, keeps it.
                #[cfg(target_os = "macos")]
                if !crate::settings_window::is_open(app) {
                    return app.hide();
                }
                Ok::<(), tauri::Error>(())
            },
        );
    }
}

/// Still attempt to release application focus when a native window hide fails.
fn dismiss_with<W, A>(hide_window: impl FnOnce() -> Result<(), W>, hide_application: impl FnOnce() -> Result<(), A>) {
    report(hide_window(), NativeIssue::Hide);
    report(hide_application(), NativeIssue::HideApplication);
}

/// End the app. Closing the window only hides it, so this is the way out on
/// Windows and Linux, which have no application menu with Quit.
#[tauri::command]
#[specta::specta]
pub(crate) fn quit(app: AppHandle) {
    app.exit(0);
}

/// Current state of the global shortcut.
#[tauri::command]
#[specta::specta]
pub(crate) fn shortcut_status(status: State<'_, Mutex<ShortcutStatus>>) -> ShortcutStatus {
    status.lock().map(|s| s.clone()).unwrap_or_default()
}

/// Set while the user has pinned the picker: it then stays open beside the
/// document and floats above it. Unpinned, it hides when focus moves away.
#[derive(Default)]
pub(crate) struct Pinned(pub(crate) AtomicBool);

/// Pin or unpin the picker. Staying on top is best effort: Wayland does not
/// allow it, and there pinned means only that the picker stays open. The
/// Windows picker is always on top while shown.
#[tauri::command]
#[specta::specta]
pub(crate) fn set_pinned(app: AppHandle, pinned: State<'_, Pinned>, on: bool) {
    pinned.0.store(on, Ordering::SeqCst);
    // The macOS panel is already a floating panel; it only needs to stay open.
    #[cfg(target_os = "linux")]
    if let Some(window) = checked(app.get_webview_window("main").ok_or(()), NativeIssue::Window) {
        report(window.set_always_on_top(on), NativeIssue::AlwaysOnTop);
    }
    #[cfg(not(target_os = "linux"))]
    let _ = app;
}

/// Whether the picker hides when it loses focus. Release builds hide it, like
/// other launchers, except while its file chooser has the focus or it is
/// pinned; development builds never do, so the developer tools stay usable.
pub(crate) fn hides_on_blur(release_build: bool, chooser_open: bool, pinned: bool) -> bool {
    release_build && !chooser_open && !pinned
}

/// A show/dismiss attempt invalidates work owned by an earlier presentation,
/// even when the native operation fails; it never revives the old owner.
#[derive(Default)]
pub(crate) struct PickerGeneration(AtomicU64);

impl PickerGeneration {
    fn originating(&self, presentation: Option<u64>) -> u64 {
        presentation.unwrap_or_else(|| self.current())
    }

    pub(crate) fn current(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
    fn advance(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst).wrapping_add(1)
    }
    fn retire(&self, generation: u64) -> bool {
        self.0
            .compare_exchange(
                generation,
                generation.wrapping_add(1),
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_ok()
    }
    fn still_owns_after(&self, generation: u64, query: impl FnOnce() -> bool) -> bool {
        query() && self.owns(generation)
    }
    fn owns(&self, generation: u64) -> bool {
        self.current() == generation
    }
}

/// The blur suppressed by a native menu may be the last focus event received.
/// Call on the main thread after releasing its lease; reconcile only cancellation
/// against live state.
pub(crate) fn reconcile_cancelled_menu(window: &tauri::WebviewWindow, generation: u64) {
    let app = window.app_handle();
    let outside = app.state::<PickerGeneration>().still_owns_after(generation, || {
        window.is_visible().ok() == Some(true) && focused_for_shortcut(window) == Some(false)
    });
    if !outside
        || !hides_on_blur(
            !cfg!(debug_assertions),
            app.state::<ChooserOpen>().is_open(),
            app.state::<Pinned>().0.load(Ordering::SeqCst),
        )
    {
        return;
    }

    #[cfg(target_os = "macos")]
    if !crate::panel::allows_automatic_blur(app) {
        return;
    }
    #[cfg(target_os = "windows")]
    dismiss_unless_refocused(app, app.state::<FocusChanges>().record(), generation);
    #[cfg(not(target_os = "windows"))]
    dismiss_picker(app);
}

/// When Narrator or another UI Automation client presses a control, WebView2
/// can hand focus to its own browser process and back within tens of
/// milliseconds (#341). The picker hides only if focus has not come back
/// after this pause.
#[cfg(target_os = "windows")]
const REFOCUS_GRACE: std::time::Duration = std::time::Duration::from_millis(250);

/// Counts the picker's focus changes, so a delayed blur can tell whether
/// focus came back in the meantime.
#[cfg(any(target_os = "windows", test))]
#[derive(Default)]
pub(crate) struct FocusChanges(std::sync::atomic::AtomicU64);

#[cfg(any(target_os = "windows", test))]
impl FocusChanges {
    /// Record a focus change and return its number.
    pub(crate) fn record(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst).wrapping_add(1)
    }

    fn permits_dismissal(&self, blur: u64, visits: &PickerGeneration, generation: u64) -> bool {
        self.is_latest(blur) && visits.owns(generation)
    }

    /// Whether `change` is still the most recent focus change.
    fn is_latest(&self, change: u64) -> bool {
        self.0.load(Ordering::SeqCst) == change
    }
}

/// Hide the picker after the grace period unless focus came back or one of
/// its own dialogs opened or the user pinned it. Without a timer thread,
/// hide at once as before.
#[cfg(target_os = "windows")]
pub(crate) fn dismiss_unless_refocused(app: &AppHandle, blur: u64, generation: u64) {
    let delayed = app.clone();
    let spawned = std::thread::Builder::new().name("ibis-blur".into()).spawn(move || {
        std::thread::sleep(REFOCUS_GRACE);
        let owner = delayed.clone();
        report(
            delayed.run_on_main_thread(move || finish_blur(&owner, blur, generation)),
            NativeIssue::Hide,
        );
    });
    if spawned.is_err() {
        finish_blur(app, blur, generation);
    }
}

#[cfg(target_os = "windows")]
fn finish_blur(app: &AppHandle, blur: u64, generation: u64) {
    let chooser_open = app.state::<ChooserOpen>().is_open();
    let pinned = app.state::<Pinned>().0.load(Ordering::SeqCst);
    if hides_on_blur(true, chooser_open, pinned)
        && app
            .state::<FocusChanges>()
            .permits_dismissal(blur, &app.state::<PickerGeneration>(), generation)
    {
        dismiss_picker(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picker_unknown_hide_snapshots_its_native_owner_before_dispatch_and_cannot_dismiss_a_reopen() {
        use std::cell::Cell;
        let visits = PickerGeneration(AtomicU64::new(41));
        let visible = Cell::new(true);
        let unknown = visits.originating(None);
        assert_eq!(unknown, 41);
        assert!(dismiss_presented_with(&visits, unknown, || visible.set(false)));
        assert!(!visible.get(), "Escape works before webview initialization");

        let b = visits.advance();
        visible.set(true);
        let delayed = visits.originating(None);
        assert_eq!(delayed, b);
        let c = visits.advance();
        assert!(!dismiss_presented_with(&visits, delayed, || visible.set(false)));
        assert!(
            visible.get(),
            "B's unknown hide cannot dismiss C after native dispatch is delayed"
        );
        assert_eq!(visits.current(), c);
        assert_eq!(
            visits.originating(Some(41)),
            41,
            "known hides keep their original owner"
        );
        assert!(!dismiss_presented_with(&visits, 41, || visible.set(false)));
        assert!(dismiss_presented_with(&visits, visits.originating(None), || visible.set(false)));
        assert!(!visible.get());
    }

    #[test]
    fn picker_windows_refused_show_dispatch_never_touches_the_cache_or_presents_late() {
        use std::cell::Cell;
        let refreshed = Cell::new(false);
        let visible = Cell::new(false);
        show_windows_picker_with(
            || {
                refreshed.set(true);
                visible.set(true);
            },
            drop, // a failed native dispatch drops its task
        );
        assert!(!refreshed.get());
        assert!(!visible.get());
        show_windows_picker_with(
            || {
                refreshed.set(true);
                visible.set(true);
            },
            |show| show(),
        );
        assert!(refreshed.get());
        assert!(visible.get(), "a later successful show remains usable");
    }

    #[test]
    fn picker_windows_worker_show_cannot_lock_out_an_earlier_main_thread_appearance_refresh() {
        use std::cell::{Cell, RefCell};
        let cache = Mutex::new(());
        let on_main = Cell::new(false);
        let stalled = Cell::new(false);
        let visible = Cell::new(false);
        let pending = RefCell::new(Vec::new());
        let queued_refresh = || {
            // A real main-thread refresh would block on this cache while a
            // worker holding it synchronously waits for the HWND response.
            // Detect the cycle without letting the regression hang the suite.
            if cache.try_lock().is_err() {
                stalled.set(true);
            }
        };
        show_windows_picker_with(
            || {
                let _cache = cache.try_lock().unwrap_or_else(|e| panic!("{e}"));
                if !on_main.get() {
                    queued_refresh();
                }
                if !stalled.get() {
                    visible.set(true);
                }
            },
            |show| pending.borrow_mut().push(show),
        );
        on_main.set(true);
        queued_refresh();
        for show in pending.into_inner() {
            show();
        }
        assert!(
            !stalled.get(),
            "appearance refresh and synchronous HWND lookup must make progress"
        );
        assert!(visible.get(), "the picker opens after its appearance has refreshed");
    }

    #[test]
    fn ipc_hide_retires_only_its_presentation_including_failed_io_and_reopen() {
        use std::cell::Cell;
        let visits = PickerGeneration::default();
        let visible = Cell::new(true);
        let a = visits.current();
        assert!(dismiss_presented_with(&visits, a, || visible.set(false)));
        assert!(!visible.get(), "the current picker hides");
        let b = visits.advance();
        visible.set(true);
        assert!(!dismiss_presented_with(&visits, a, || visible.set(false)));
        assert!(visible.get(), "A's delayed hide cannot dismiss reopened B");
        assert_eq!(visits.current(), b, "refusal cannot retire B's activation work");
        assert!(dismiss_presented_with(&visits, b, || {
            // Native hide I/O fails, so the window is still visible. B's
            // ticket is nevertheless retired and cannot later hide C.
        }));
        assert!(visible.get());
        let c = visits.advance();
        assert!(!dismiss_presented_with(&visits, b, || visible.set(false)));
        assert!(visible.get());
        assert!(dismiss_presented_with(&visits, c, || visible.set(false)));
        assert!(!visible.get(), "a fresh hide still works after the refusal");
    }

    #[test]
    fn windows_shortcut_uses_the_exact_foreground_owner_even_after_webview_focus_and_reopen() {
        // Cached top-level focus is false when WebView2 owns keyboard input.
        // A foreign Ibis window is still a different HWND, even in this process.
        for (picker, foreground, focused, action) in [
            (Some(41), Some(41), Some(true), ToggleAction::Hide),
            (Some(41), Some(43), Some(false), ToggleAction::Show),
            (None, Some(41), None, ToggleAction::Show),
            (Some(41), None, None, ToggleAction::Show),
            (Some(0), Some(0), None, ToggleAction::Show),
            // Close/recreate B: a delayed foreground sample of A cannot hide B.
            (Some(43), Some(41), Some(false), ToggleAction::Show),
            (Some(43), Some(43), Some(true), ToggleAction::Hide),
        ] {
            assert_eq!(owned_foreground(picker, foreground), focused);
            assert_eq!(toggle_action(Some(true), focused), action);
        }
    }

    // Model the two GLib priorities, not automatic focus on map: tao queues
    // frame/visibility at DEFAULT; our GTK activation runs at DEFAULT_IDLE.
    // Eligibility and AT-SPI/WebView focus cannot prove keyboard ownership.
    #[derive(Default)]
    struct GtkPresentation {
        accepts_focus: std::cell::Cell<bool>,
        mapped: std::cell::Cell<bool>,
        owns_keyboard: std::cell::Cell<bool>,
        chooser: ChooserOpen,
        pending: std::cell::RefCell<Vec<Option<(u32, u32)>>>,
        worker: std::cell::Cell<bool>,
        worker_shows: std::cell::Cell<usize>,
        idle: std::cell::RefCell<Vec<u64>>,
        calls: std::cell::RefCell<Vec<&'static str>>,
        activations: std::cell::RefCell<Vec<(u32, u32)>>,
        frame: std::cell::Cell<(u32, u32)>,
        first_shown: std::cell::Cell<Option<(u32, u32)>>,
    }

    impl GtkPresentation {
        fn allow_focus(&self) {
            self.calls.borrow_mut().push("allow-focus");
            self.accepts_focus.set(true);
        }

        fn show(&self) {
            self.calls.borrow_mut().push("queue-show");
            if self.worker.get() {
                self.worker_shows.set(self.worker_shows.get() + 1);
            } else {
                self.pending.borrow_mut().push(None);
            }
        }

        fn focus_after_show(&self, owner: u64) {
            self.calls.borrow_mut().push("queue-activation");
            self.idle.borrow_mut().push(owner);
        }

        fn settle_native(&self) {
            for request in self.pending.borrow_mut().drain(..) {
                if let Some(frame) = request {
                    self.calls.borrow_mut().push("apply-frame");
                    self.frame.set(frame);
                } else {
                    self.calls.borrow_mut().push("map");
                    self.mapped.set(true);
                    self.first_shown.set(Some(self.frame.get()));
                    // Eligibility alone does not ask the WM to activate. A
                    // mapped window may leave input with the previous editor.
                }
            }
            // tao restores the hint on draw but does not retry activation.
            self.accepts_focus.set(true);
        }

        fn settle_worker_requests(&self) {
            // Tauri dispatches worker WindowMessages before tao can enqueue
            // their GTK operations. GLib's idle priority cannot order these
            // messages while they are still waiting in Tauri's event queue.
            for _ in 0..self.worker_shows.replace(0) {
                self.pending.borrow_mut().push(None);
            }
        }

        fn settle_activation(&self, visits: &PickerGeneration) {
            for owner in self.idle.borrow_mut().drain(..) {
                activate_presented_with(
                    visits,
                    owner,
                    &self.chooser,
                    || self.mapped.get(),
                    || {
                        self.calls.borrow_mut().push("present-with-current-time");
                        self.activations.borrow_mut().push(self.frame.get());
                        self.owns_keyboard.set(self.accepts_focus.get());
                    },
                );
            }
        }

        fn settle(&self, visits: &PickerGeneration) {
            self.settle_native();
            self.settle_activation(visits);
        }
    }

    #[test]
    fn a_prepared_hidden_linux_picker_maps_with_its_frame_and_native_keyboard_focus() {
        let native = GtkPresentation::default();
        let visits = PickerGeneration::default();
        native.frame.set((720, 480));
        // Sizing's complete frame is queued before presentation, never after it.
        native.pending.borrow_mut().push(Some((960, 640)));
        present_with(
            true,
            || native.allow_focus(),
            || native.show(),
            || native.focus_after_show(visits.current()),
        );
        assert!(
            !native.mapped.get(),
            "presentation must preserve the native geometry queue"
        );
        native.settle_native();
        assert_eq!(native.first_shown.get(), Some((960, 640)), "no default-frame flash");
        assert!(native.mapped.get());
        assert!(
            !native.owns_keyboard.get(),
            "mapping alone need not activate the picker"
        );
        native.settle_activation(&visits);
        assert_eq!(
            *native.calls.borrow(),
            [
                "allow-focus",
                "queue-show",
                "queue-activation",
                "apply-frame",
                "map",
                "present-with-current-time"
            ],
            "ask the WM to activate only after applying the frame and mapping"
        );
        assert_eq!(*native.activations.borrow(), [(960, 640)]);
        assert!(
            native.owns_keyboard.get(),
            "Search must receive actual keyboard input on first map"
        );

        // Hiding and reopening (including the first show after a login launch)
        // uses this same owned activation path.
        visits.advance();
        native.mapped.set(false);
        native.owns_keyboard.set(false);
        present_with(
            true,
            || native.allow_focus(),
            || native.show(),
            || native.focus_after_show(visits.current()),
        );
        native.settle(&visits);
        assert!(native.owns_keyboard.get());
    }

    #[test]
    fn an_explicitly_unfocused_startup_maps_without_taking_the_editors_keyboard() {
        let native = GtkPresentation::default();
        let visits = PickerGeneration::default();
        present_with(
            false,
            || native.allow_focus(),
            || native.show(),
            || native.focus_after_show(visits.current()),
        );
        native.settle(&visits);
        assert!(native.mapped.get());
        assert!(!native.owns_keyboard.get());
        assert_eq!(*native.calls.borrow(), ["queue-show", "map"]);
        assert!(native.activations.borrow().is_empty());
    }

    #[test]
    fn a_native_dialog_keeps_focus_when_it_opens_before_the_activation_idle() {
        for open_before_show in [true, false] {
            let native = GtkPresentation::default();
            let visits = PickerGeneration::default();
            let owner = visits.advance();
            native.frame.set((720, 480));
            // Explicit update offers acquire their lease before show_picker;
            // another chooser may acquire it after Show but before idle.
            let mut dialog = if open_before_show { native.chooser.begin() } else { None };
            present_with(
                true,
                || native.allow_focus(),
                || native.show(),
                || native.focus_after_show(owner),
            );
            if !open_before_show {
                dialog = native.chooser.begin();
            }
            assert!(dialog.is_some());
            native.settle_native();
            assert!(native.mapped.get());
            assert!(
                visits.owns(owner),
                "the offer changes neither visibility nor generation"
            );
            native.settle_activation(&visits);
            assert!(native.chooser.is_open());
            assert!(
                native.activations.borrow().is_empty(),
                "the picker must not rise over its native dialog"
            );
            assert!(!native.owns_keyboard.get());
            assert!(
                native.idle.borrow().is_empty(),
                "suppressed activation is consumed, never retried"
            );

            drop(dialog);
            assert!(!native.chooser.is_open());
            native.settle_activation(&visits);
            assert!(
                native.activations.borrow().is_empty(),
                "closing the dialog must not replay old activation"
            );

            let reopened = visits.advance();
            present_with(
                true,
                || native.allow_focus(),
                || native.show(),
                || native.focus_after_show(reopened),
            );
            native.settle(&visits);
            assert_eq!(*native.activations.borrow(), [(720, 480)]);
            assert!(
                native.owns_keyboard.get(),
                "a fresh show can focus once its dialog is closed"
            );
        }
    }

    #[test]
    fn a_hidden_failed_or_superseded_linux_show_cannot_activate_later() {
        for still_mapped in [false, true] {
            let native = GtkPresentation::default();
            let visits = PickerGeneration::default();
            present_with(
                true,
                || native.allow_focus(),
                || native.show(),
                || native.focus_after_show(visits.current()),
            );
            // A failed show never maps. Its activation must neither show the
            // hidden window itself nor wait around to consume a later show.
            native.pending.borrow_mut().clear();
            native.settle_activation(&visits);
            assert!(native.activations.borrow().is_empty());
            assert!(native.idle.borrow().is_empty());

            native.show();
            native.settle_native();
            native.focus_after_show(visits.current());
            visits.advance(); // Every hide cancels, even when native hide fails.
            native.mapped.set(still_mapped);
            native.settle_activation(&visits);
            assert!(native.activations.borrow().is_empty());
            assert!(!native.owns_keyboard.get());
        }
    }

    #[test]
    fn a_newer_linux_opening_alone_owns_activation_and_can_request_no_focus() {
        for focus in [false, true] {
            let native = GtkPresentation::default();
            let visits = PickerGeneration::default();
            native.pending.borrow_mut().push(Some((960, 640)));
            present_with(
                true,
                || native.allow_focus(),
                || native.show(),
                || native.focus_after_show(visits.current()),
            );
            visits.advance(); // A is replaced by B before either queue drains.
            native.pending.borrow_mut().push(Some((840, 600)));
            present_with(
                focus,
                || native.allow_focus(),
                || native.show(),
                || native.focus_after_show(visits.current()),
            );
            native.settle(&visits);
            assert_eq!(native.owns_keyboard.get(), focus);
            assert_eq!(
                *native.activations.borrow(),
                if focus { vec![(840, 600)] } else { vec![] }
            );
        }
    }

    #[test]
    fn a_show_replaced_before_activation_scheduling_cannot_borrow_the_new_ticket() {
        let native = GtkPresentation::default();
        let visits = PickerGeneration::default();
        let a = visits.advance();
        native.allow_focus();
        native.pending.borrow_mut().push(Some((960, 640)));
        native.show();
        native.settle_native();
        let b = visits.advance();
        assert_ne!(a, b);
        // A reached Show; B was accepted before A scheduled its GTK idle.
        // Reading current() here would incorrectly donate B's ticket to A.
        native.focus_after_show(a);
        native.settle_activation(&visits);
        assert!(native.activations.borrow().is_empty());
        assert!(!native.owns_keyboard.get());

        native.pending.borrow_mut().push(Some((840, 600)));
        native.show();
        native.focus_after_show(b);
        native.settle(&visits);
        assert_eq!(*native.activations.borrow(), [(840, 600)]);
        assert!(native.owns_keyboard.get());
    }

    #[test]
    fn native_and_worker_entrypoints_reach_activation_after_both_dispatch_queues() {
        // Counterexample from CI: the worker attaches GLib idle before Tauri
        // dispatches Show. Being idle on GTK does not mean Show was dispatched.
        let old = GtkPresentation::default();
        let visits = PickerGeneration::default();
        old.worker.set(true);
        present_with(
            true,
            || old.allow_focus(),
            || old.show(),
            || old.focus_after_show(visits.current()),
        );
        old.settle(&visits); // GTK drains while Tauri still has the worker Show.
        old.settle_worker_requests();
        old.settle(&visits);
        assert!(old.mapped.get());
        assert!(
            !old.owns_keyboard.get(),
            "the old order loses activation exactly as CI did"
        );

        // Shortcut, second-instance and library workers share the same native
        // entry as tray/menu, retry, startup and reopen on the main thread.
        for on_thread in [false, true] {
            for gtk_drains_first in [false, true] {
                let native = GtkPresentation::default();
                let visits = PickerGeneration::default();
                let owner = visits.advance();
                let tasks = std::cell::RefCell::new(Vec::<Box<dyn FnOnce() + '_>>::new());
                native.worker.set(!on_thread);
                on_picker_thread_with(
                    on_thread,
                    || {
                        assert!(!native.worker.get(), "GTK geometry and show must run on its thread");
                        native.pending.borrow_mut().push(Some((840, 600)));
                        present_with(
                            true,
                            || native.allow_focus(),
                            || native.show(),
                            || native.focus_after_show(owner),
                        );
                    },
                    |show| tasks.borrow_mut().push(Box::new(show)),
                );
                if gtk_drains_first {
                    native.settle(&visits);
                }
                native.worker.set(false);
                for show in tasks.borrow_mut().drain(..) {
                    show();
                }
                native.settle_worker_requests();
                native.settle(&visits);
                assert!(native.mapped.get());
                assert!(native.owns_keyboard.get());
                assert_eq!(
                    *native.activations.borrow(),
                    [(840, 600)],
                    "one activation of the complete frame"
                );
                assert_eq!(native.worker_shows.get(), 0);
            }
        }
    }

    #[test]
    fn an_owned_worker_show_cancelled_before_native_dispatch_cannot_present() {
        let native = GtkPresentation::default();
        let visits = PickerGeneration::default();
        let owner = visits.advance();
        let tasks = std::cell::RefCell::new(Vec::<Box<dyn FnOnce() + '_>>::new());
        on_picker_thread_with(
            false,
            || {
                if visits.owns(owner) {
                    present_with(
                        true,
                        || native.allow_focus(),
                        || native.show(),
                        || native.focus_after_show(owner),
                    );
                }
            },
            |show| tasks.borrow_mut().push(Box::new(show)),
        );
        visits.advance(); // Hide/newer show owns the picker before the task runs.
        for show in tasks.borrow_mut().drain(..) {
            show();
        }
        native.settle(&visits);
        assert!(native.calls.borrow().is_empty());
        assert!(!native.mapped.get());
        assert!(!native.owns_keyboard.get());
    }

    #[test]
    fn window_creation_waits_hidden_for_the_complete_frame_and_preserves_launch_intent() {
        let mut context = tauri::test::mock_context::<tauri::test::MockRuntime, _>(tauri::test::noop_assets());
        *context.config_mut() = serde_json::from_str(include_str!("../tauri.conf.json"))
            .unwrap_or_else(|error| panic!("invalid synthetic window config: {error}"));
        for visible in [true, false] {
            for focused in [true, false] {
                let window = &mut context.config_mut().app.windows[0];
                window.visible = visible;
                window.focus = focused;
                assert_eq!(prepare_startup(&mut context), (visible, focused));
                let window = &context.config().app.windows[0];
                assert!(!window.visible);
                assert!(!window.focus);
                assert_eq!((window.width, window.height), (720.0, 480.0));
                assert_eq!((window.min_width, window.min_height), (Some(480.0), Some(360.0)));
            }
        }
    }

    /// Only a settings window may appear in the taskbar or Alt+Tab (#495);
    /// macOS already uses an accessory panel, where this option does nothing.
    #[test]
    fn the_quick_picker_is_kept_out_of_the_taskbar() {
        let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap_or_default();
        assert_eq!(config["app"]["windows"][0]["label"], "main");
        assert_eq!(config["app"]["windows"][0]["skipTaskbar"], true);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn native_editor_focus_overrides_a_stale_positive_toolkit_focus() {
        assert_eq!(
            resolve_focus(Ok(Some(false)), || panic!("stale GTK focus must not be consulted")),
            Some(false)
        );
        assert_eq!(resolve_focus(Ok(Some(true)), || Some(false)), Some(true));
        assert_eq!(resolve_focus(Ok(None), || Some(true)), Some(true));
        assert_eq!(
            resolve_focus(Err(()), || panic!(
                "query failure must show rather than use stale focus"
            )),
            None
        );
    }

    #[test]
    fn shortcut_hides_only_a_known_visible_focused_picker() {
        assert_eq!(toggle_action(Some(true), Some(true)), ToggleAction::Hide);
        for (visible, focused) in [
            (Some(true), Some(false)),
            (Some(false), Some(true)), // A stale focused flag cannot hide a hidden picker.
            (Some(false), Some(false)),
            (Some(true), None), // An unavailable query must leave a way to summon it.
            (None, Some(true)),
            (None, None),
        ] {
            assert_eq!(toggle_action(visible, focused), ToggleAction::Show);
        }
    }

    #[cfg(debug_assertions)]
    #[test]
    fn shortcut_trace_contains_only_fixed_structural_states() {
        let mut messages = std::collections::BTreeSet::new();
        for visible in [Some(true), Some(false), None] {
            for focused in [Some(true), Some(false), None] {
                let message = toggle_trace(visible, focused);
                assert!(message.starts_with("ibis-picker: toggle "));
                assert!(!message.contains('\n'));
                messages.insert(message);
            }
        }
        assert_eq!(messages.len(), 4);
        assert_eq!(
            toggle_trace(Some(true), Some(true)),
            "ibis-picker: toggle visible focused hide"
        );
        assert_eq!(
            toggle_trace(Some(true), Some(false)),
            "ibis-picker: toggle visible unfocused show"
        );
        assert_eq!(
            toggle_trace(Some(true), None),
            "ibis-picker: toggle visible focus-unavailable show"
        );
        assert_eq!(
            toggle_trace(None, Some(true)),
            "ibis-picker: toggle not-known-visible show"
        );
    }

    #[test]
    fn saved_picker_shortcut_conflicts_fall_back_with_an_explanation() {
        for keys in [
            "CmdOrCtrl+Shift+F",
            "CmdOrCtrl+Shift+P",
            "CmdOrCtrl+Shift+Slash",
            "CmdOrCtrl+K",
        ] {
            let (shortcut, label, error) = configured_shortcut_for(Some(keys), cfg!(target_os = "windows"));
            assert_eq!(shortcut, default_shortcut(), "{keys}");
            assert_eq!(label, shortcut_label(), "{keys}");
            assert!(error.is_some_and(|error| error.contains("using")), "{keys}");
        }
    }

    #[test]
    fn shortcuts_from_settings_are_parsed_and_invalid_ones_fall_back() {
        let (_, label, error) = configured_shortcut_for(Some("Alt+Shift+K"), cfg!(target_os = "windows"));
        assert_eq!(label, "Alt+Shift+K");
        assert!(error.is_none());
        let (_, label, error) = configured_shortcut_for(Some("Control+Nonsense"), cfg!(target_os = "windows"));
        assert_eq!(label, shortcut_label());
        assert!(error.is_some());
        let (_, label, error) = configured_shortcut_for(None, cfg!(target_os = "windows"));
        assert_eq!(label, shortcut_label());
        assert!(error.is_none());
    }

    #[test]
    fn safe_configured_shortcuts_survive_startup_and_empty_settings_fall_back() {
        for label in ["Alt+Q", "CmdOrCtrl+Shift+Y", "Control+Command+R", "Control+Alt+F8"] {
            let (shortcut, shown, error) = configured_shortcut_for(Some(label), cfg!(target_os = "windows"));
            assert_eq!(Some(shortcut), label.parse::<OpeningShortcut>().ok());
            assert_eq!(shown, label);
            assert!(error.is_none());
        }
        let (shortcut, shown, error) = configured_shortcut_for(Some(""), cfg!(target_os = "windows"));
        assert_eq!(shortcut, default_shortcut());
        assert_eq!(shown, shortcut_label());
        assert!(error.is_some());
    }

    #[test]
    fn windows_shortcut_loading_refuses_altgr_but_keeps_other_explicit_choices() {
        for label in [
            "Control+Alt+C",
            "Control+Alt+KeyC",
            "Control+Alt+Shift+KeyC",
            "Control+Alt+Space",
        ] {
            let (shortcut, shown, error) = configured_shortcut_for(Some(label), true);
            assert_eq!(shortcut, default_shortcut());
            assert_eq!(shown, shortcut_label());
            assert!(error.is_some_and(|error| error.contains("AltGr")));
            let (shortcut, shown, error) = configured_shortcut_for(Some(label), false);
            assert_eq!(Some(shortcut), label.parse::<OpeningShortcut>().ok());
            assert_eq!(shown, label);
            assert!(error.is_none());
        }
        for label in ["Control+Shift+Y", "Control+Alt+F8", "Control+Alt+Command+KeyC"] {
            let (shortcut, shown, error) = configured_shortcut_for(Some(label), true);
            assert_eq!(Some(shortcut), label.parse::<OpeningShortcut>().ok());
            assert_eq!(shown, label);
            assert!(error.is_none());
        }
    }

    /// The label users see must name the shortcut that is registered.
    #[test]
    fn the_default_shortcut_label_matches_the_registered_shortcut() {
        assert_eq!(
            shortcut_label().parse::<OpeningShortcut>().ok(),
            Some(default_shortcut())
        );
    }

    /// Cmd+Shift+Space opens the macOS Siri typing field, which takes the
    /// keystrokes meant for the picker (#133).
    #[cfg(target_os = "macos")]
    #[test]
    fn the_macos_default_does_not_collide_with_siri() {
        assert_eq!(shortcut_label(), "Command+Shift+Y");
        assert_eq!(
            default_shortcut().shortcut,
            Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyY)
        );
    }

    /// Ctrl+Shift+Space types a non-breaking space in Word and LibreOffice
    /// Writer, and Ctrl+Alt is AltGr on many layouts (#133).
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn the_default_shortcut_takes_no_keys_editors_type_with() {
        assert_eq!(shortcut_label(), "Ctrl+Shift+Y");
        assert_eq!(
            default_shortcut().shortcut,
            Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyY)
        );
    }

    /// The file dialog takes the focus from the picker; hiding the picker then
    /// made the dialog disappear before it could be used.
    #[test]
    fn the_picker_stays_open_while_its_chooser_has_the_focus() {
        assert!(hides_on_blur(true, false, false));
        assert!(!hides_on_blur(true, true, false));
        assert!(!hides_on_blur(false, false, false));
    }

    #[test]
    fn a_pinned_picker_stays_open_when_focus_moves_to_the_document() {
        assert!(!hides_on_blur(true, false, true));
    }

    #[test]
    fn a_later_show_or_hide_cannot_be_dismissed_by_old_menu_or_deferred_blur() {
        let generation = PickerGeneration::default();
        let menu = generation.current();
        assert!(generation.owns(menu));
        assert!(
            !generation.still_owns_after(menu, || false),
            "Escape returns focus to the picker"
        );
        assert!(
            !generation.still_owns_after(menu, || {
                generation.advance();
                true
            }),
            "a show during a native query supersedes cancellation"
        );
        let menu = generation.current();
        generation.advance(); // hide, including a failed native hide
        assert!(!generation.owns(menu));
        let hidden = generation.current();
        generation.advance(); // reopen, including a failed show
        assert!(!generation.owns(menu));
        assert!(!generation.owns(hidden));
    }

    /// #341: pressing More through UI Automation on Windows blurs the picker
    /// and refocuses it within tens of milliseconds; that must not hide it.
    #[test]
    fn a_blur_followed_by_refocus_does_not_hide_the_picker() {
        let changes = FocusChanges::default();
        let visits = PickerGeneration::default();
        let blur = changes.record();
        assert!(changes.permits_dismissal(blur, &visits, 0), "a real blur still hides");
        changes.record();
        assert!(
            !changes.permits_dismissal(blur, &visits, 0),
            "focus came back before the grace period ended"
        );
        let second_blur = changes.record();
        assert!(
            changes.permits_dismissal(second_blur, &visits, 0),
            "a later real blur still hides"
        );
        visits.advance();
        assert!(
            !changes.permits_dismissal(second_blur, &visits, 0),
            "a new show supersedes deferred blur even without a focus event"
        );
    }

    #[test]
    fn dismissing_releases_the_application_after_hiding_the_window_even_on_failure() {
        use std::cell::RefCell;

        for window_result in [Ok(()), Err(())] {
            let operations = RefCell::new(Vec::new());
            dismiss_with(
                || {
                    operations.borrow_mut().push("window");
                    window_result
                },
                || {
                    operations.borrow_mut().push("application");
                    Ok::<(), ()>(())
                },
            );
            assert_eq!(*operations.borrow(), ["window", "application"]);
        }
    }
}
