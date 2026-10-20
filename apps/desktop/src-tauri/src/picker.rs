//! Showing, hiding and quitting, and the global shortcut.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

use crate::diagnostics::{NativeIssue, checked, report};
use crate::placement::place_on_active_display;

#[cfg(target_os = "linux")]
mod x11_focus;

/// Whether the global shortcut is working, shown in the interface.
#[derive(Default, Clone, Serialize, specta::Type)]
pub(crate) struct ShortcutStatus {
    pub(crate) shortcut: String,
    pub(crate) registered: bool,
    pub(crate) error: Option<String>,
}

/// Default shortcut: Control+Command+R on macOS, where Cmd+Shift+Space opens
/// the Siri typing field; Ctrl+Shift+Y elsewhere, because Ctrl+Shift+Space
/// types a non-breaking space in Word and LibreOffice Writer (#133).
pub(crate) fn default_shortcut() -> Shortcut {
    if cfg!(target_os = "macos") {
        Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SUPER), Code::KeyR)
    } else {
        Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyY)
    }
}

pub(crate) fn shortcut_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Control+Command+R"
    } else {
        "Ctrl+Shift+Y"
    }
}

/// The shortcut from settings (for example `"Control+Command+R"`), or the
/// default. An invalid setting falls back to the default with an explanation.
pub(crate) fn configured_shortcut(setting: Option<&str>) -> (Shortcut, String, Option<String>) {
    match setting {
        Some(text) => match text.parse::<Shortcut>() {
            Ok(shortcut) => (shortcut, text.to_string(), None),
            Err(e) => (
                default_shortcut(),
                shortcut_label().to_string(),
                Some(format!(
                    "the shortcut \"{text}\" in settings is not valid ({e}); using {}",
                    shortcut_label()
                )),
            ),
        },
        None => (default_shortcut(), shortcut_label().to_string(), None),
    }
}

const SHORTCUT_BACKEND_UNAVAILABLE: &str =
    "Global shortcuts need a working X11 connection with XKB. Open Ibis from your application launcher.";
const SHORTCUT_REGISTRATION_FAILED: &str =
    "The shortcut could not be registered. Another app may be using it; choose a different shortcut in settings.";

/// The Linux plugin can return success after its X11 worker exits. Confirm its
/// required backend first; session labels alone do not identify usable X11.
pub(crate) fn register_shortcut(app: &AppHandle, shortcut: Shortcut) -> Result<(), &'static str> {
    register_with(
        || {
            #[cfg(target_os = "linux")]
            return x11_focus::shortcut_ready();
            #[cfg(not(target_os = "linux"))]
            Ok(())
        },
        || app.global_shortcut().register(shortcut),
    )
}

fn register_with<E>(
    backend: impl FnOnce() -> Result<(), ()>,
    register: impl FnOnce() -> Result<(), E>,
) -> Result<(), &'static str> {
    backend().map_err(|()| SHORTCUT_BACKEND_UNAVAILABLE)?;
    register().map_err(|_| SHORTCUT_REGISTRATION_FAILED)
}

/// Show the picker on the display with the pointer, focused, and tell the interface to focus the search box.
pub(crate) fn show_picker(app: &AppHandle) {
    if let Some(window) = checked(app.get_webview_window("main").ok_or(()), NativeIssue::Window) {
        report(window.unminimize(), NativeIssue::Unminimize);
        place_on_active_display(app, &window, false);
        #[cfg(target_os = "macos")]
        crate::panel::show(app);
        #[cfg(not(target_os = "macos"))]
        {
            report(window.show(), NativeIssue::Show);
            report(window.set_focus(), NativeIssue::Focus);
        }
        report(window.emit("picker:shown", ()), NativeIssue::PickerEvent);
    }
    crate::library::refresh_zotero(app);
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

fn focused_for_shortcut(window: &tauri::WebviewWindow) -> Option<bool> {
    #[cfg(target_os = "linux")]
    return resolve_focus(x11_focus::query(window), || {
        checked(window.is_focused(), NativeIssue::FocusState)
    });
    #[cfg(not(target_os = "linux"))]
    checked(window.is_focused(), NativeIssue::FocusState)
}

/// Toggle the picker: hide it if it is visible and focused, show it otherwise.
pub(crate) fn toggle_picker(app: &AppHandle) {
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
            ToggleAction::Show => show_picker(app),
        }
    } else {
        show_picker(app);
    }
}

/// Hide the picker (Escape in the interface).
#[tauri::command]
#[specta::specta]
pub(crate) fn hide_picker(app: AppHandle) {
    dismiss_picker(&app);
}

/// A window hide alone leaves Ibis active on macOS, swallowing the next paste.
/// Every dismissal path must also release application focus.
pub(crate) fn dismiss_picker(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    crate::panel::cancel_initial_presentation(app);
    if let Some(window) = checked(app.get_webview_window("main").ok_or(()), NativeIssue::Window) {
        // The interface keeps its search for a minute after a real hide.
        report(window.emit("picker:hidden", ()), NativeIssue::PickerEvent);
        dismiss_with(
            || window.hide(),
            || {
                #[cfg(target_os = "macos")]
                return app.hide();
                #[cfg(not(target_os = "macos"))]
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

/// Set while a system dialog that belongs to the picker (such as the file
/// dialog) is open. Focus moves to the dialog, which must not hide the picker.
#[derive(Default)]
pub(crate) struct DialogOpen(pub(crate) AtomicBool);

impl DialogOpen {
    /// Claim the dialog slot. Returns false when a dialog is already open, so
    /// only one is shown at a time and closing it cannot unprotect another.
    pub(crate) fn begin(&self) -> bool {
        self.0
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub(crate) fn end(&self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Set while the user has pinned the picker: it then stays open beside the
/// document and floats above it. Unpinned, it hides when focus moves away.
#[derive(Default)]
pub(crate) struct Pinned(pub(crate) AtomicBool);

/// Pin or unpin the picker. Staying on top is best effort: Wayland does not
/// allow it, and there pinned means only that the picker stays open.
#[tauri::command]
#[specta::specta]
pub(crate) fn set_pinned(app: AppHandle, pinned: State<'_, Pinned>, on: bool) {
    pinned.0.store(on, Ordering::SeqCst);
    // The macOS panel is already a floating panel; it only needs to stay open.
    #[cfg(not(target_os = "macos"))]
    if let Some(window) = checked(app.get_webview_window("main").ok_or(()), NativeIssue::Window) {
        report(window.set_always_on_top(on), NativeIssue::AlwaysOnTop);
    }
    #[cfg(target_os = "macos")]
    let _ = app;
}

/// Whether the picker hides when it loses focus. Release builds hide it, like
/// other launchers, except while one of its own dialogs has the focus or it is
/// pinned; development builds never do, so the developer tools stay usable.
pub(crate) fn hides_on_blur(release_build: bool, dialog_open: bool, pinned: bool) -> bool {
    release_build && !dialog_open && !pinned
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

    /// Whether `change` is still the most recent focus change.
    fn is_latest(&self, change: u64) -> bool {
        self.0.load(Ordering::SeqCst) == change
    }
}

/// Hide the picker after the grace period unless focus came back or one of
/// its own dialogs opened or the user pinned it. Without a timer thread,
/// hide at once as before.
#[cfg(target_os = "windows")]
pub(crate) fn dismiss_unless_refocused(app: &AppHandle, blur: u64) {
    let delayed = app.clone();
    let spawned = std::thread::Builder::new().name("ibis-blur".into()).spawn(move || {
        std::thread::sleep(REFOCUS_GRACE);
        let dialog_open = delayed.state::<DialogOpen>().0.load(Ordering::SeqCst);
        let pinned = delayed.state::<Pinned>().0.load(Ordering::SeqCst);
        if hides_on_blur(true, dialog_open, pinned) && delayed.state::<FocusChanges>().is_latest(blur) {
            dismiss_picker(&delayed);
        }
    });
    if spawned.is_err() {
        dismiss_picker(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_shortcut_backend_never_calls_the_registration_plugin() {
        assert_eq!(
            register_with::<()>(
                || Err(()),
                || panic!("a disconnected plugin can falsely return success")
            ),
            Err(SHORTCUT_BACKEND_UNAVAILABLE)
        );
        assert_eq!(register_with(|| Ok(()), || Ok::<(), ()>(())), Ok(()));
    }

    #[test]
    fn shortcut_registration_conflicts_remain_visible_without_raw_errors() {
        struct PrivateError;
        impl std::fmt::Display for PrivateError {
            fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                panic!("never format a native error");
            }
        }
        assert_eq!(
            register_with(|| Ok(()), || Err(PrivateError)),
            Err(SHORTCUT_REGISTRATION_FAILED)
        );
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
    fn shortcuts_from_settings_are_parsed_and_invalid_ones_fall_back() {
        let (_, label, error) = configured_shortcut(Some("Alt+Shift+K"));
        assert_eq!(label, "Alt+Shift+K");
        assert!(error.is_none());
        let (_, label, error) = configured_shortcut(Some("Control+Nonsense"));
        assert_eq!(label, shortcut_label());
        assert!(error.is_some());
        let (_, label, error) = configured_shortcut(None);
        assert_eq!(label, shortcut_label());
        assert!(error.is_none());
    }

    /// The label users see must name the shortcut that is registered.
    #[test]
    fn the_default_shortcut_label_matches_the_registered_shortcut() {
        assert_eq!(shortcut_label().parse::<Shortcut>().ok(), Some(default_shortcut()));
    }

    /// Cmd+Shift+Space opens the macOS Siri typing field, which takes the
    /// keystrokes meant for the picker (#133).
    #[cfg(target_os = "macos")]
    #[test]
    fn the_macos_default_does_not_collide_with_siri() {
        assert_eq!(shortcut_label(), "Control+Command+R");
        assert_eq!(
            default_shortcut(),
            Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SUPER), Code::KeyR)
        );
    }

    /// Ctrl+Shift+Space types a non-breaking space in Word and LibreOffice
    /// Writer, and Ctrl+Alt is AltGr on many layouts (#133).
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn the_default_shortcut_takes_no_keys_editors_type_with() {
        assert_eq!(shortcut_label(), "Ctrl+Shift+Y");
        assert_eq!(
            default_shortcut(),
            Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyY)
        );
    }

    /// The file dialog takes the focus from the picker; hiding the picker then
    /// made the dialog disappear before it could be used.
    #[test]
    fn the_picker_stays_open_while_its_own_dialog_has_the_focus() {
        assert!(hides_on_blur(true, false, false));
        assert!(!hides_on_blur(true, true, false));
        assert!(!hides_on_blur(false, false, false));
    }

    #[test]
    fn a_pinned_picker_stays_open_when_focus_moves_to_the_document() {
        assert!(!hides_on_blur(true, false, true));
    }

    /// #341: pressing More through UI Automation on Windows blurs the picker
    /// and refocuses it within tens of milliseconds; that must not hide it.
    #[test]
    fn a_blur_followed_by_refocus_does_not_hide_the_picker() {
        let changes = FocusChanges::default();
        let blur = changes.record();
        assert!(changes.is_latest(blur), "a real blur still hides");
        changes.record();
        assert!(
            !changes.is_latest(blur),
            "focus came back before the grace period ended"
        );
        let second_blur = changes.record();
        assert!(changes.is_latest(second_blur), "a later real blur still hides");
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

    #[test]
    fn only_one_dialog_is_open_at_a_time() {
        let dialog = DialogOpen::default();
        assert!(dialog.begin());
        assert!(!dialog.begin());
        dialog.end();
        assert!(dialog.begin());
    }
}
