//! Showing, hiding and quitting, and the global shortcut.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

use crate::diagnostics::{NativeIssue, checked, report};

/// Whether the global shortcut is working, shown in the interface.
#[derive(Default, Clone, Serialize, specta::Type)]
pub(crate) struct ShortcutStatus {
    pub(crate) shortcut: String,
    pub(crate) registered: bool,
    pub(crate) error: Option<String>,
}

/// Default shortcut: Control+Command+R on macOS, where Cmd+Shift+Space opens
/// the Siri typing field (#133); Ctrl+Shift+Space elsewhere.
pub(crate) fn default_shortcut() -> Shortcut {
    if cfg!(target_os = "macos") {
        Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SUPER), Code::KeyR)
    } else {
        Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space)
    }
}

pub(crate) fn shortcut_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Control+Command+R"
    } else {
        "Ctrl+Shift+Space"
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

/// Show the picker, focused, and tell the interface to focus the search box.
pub(crate) fn show_picker(app: &AppHandle) {
    if let Some(window) = checked(app.get_webview_window("main").ok_or(()), NativeIssue::Window) {
        report(window.unminimize(), NativeIssue::Unminimize);
        report(window.show(), NativeIssue::Show);
        report(window.set_focus(), NativeIssue::Focus);
        report(window.emit("picker:shown", ()), NativeIssue::PickerEvent);
    }
}

/// Toggle the picker: hide it if it is visible and focused, show it otherwise.
pub(crate) fn toggle_picker(app: &AppHandle) {
    match app.get_webview_window("main") {
        Some(w)
            if checked(w.is_visible(), NativeIssue::Visibility).unwrap_or(false)
                && checked(w.is_focused(), NativeIssue::FocusState).unwrap_or(false) =>
        {
            report(w.hide(), NativeIssue::Hide);
        }
        _ => show_picker(app),
    }
}

/// Hide the picker (Escape in the interface).
#[tauri::command]
#[specta::specta]
pub(crate) fn hide_picker(app: AppHandle) {
    if let Some(window) = checked(app.get_webview_window("main").ok_or(()), NativeIssue::Window) {
        report(window.hide(), NativeIssue::Hide);
    }
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

/// Whether the picker hides when it loses focus. Release builds hide it, like
/// other launchers, except while one of its own dialogs has the focus;
/// development builds never do, so the developer tools stay usable.
pub(crate) fn hides_on_blur(release_build: bool, dialog_open: bool) -> bool {
    release_build && !dialog_open
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// The file dialog takes the focus from the picker; hiding the picker then
    /// made the dialog disappear before it could be used.
    #[test]
    fn the_picker_stays_open_while_its_own_dialog_has_the_focus() {
        assert!(hides_on_blur(true, false));
        assert!(!hides_on_blur(true, true));
        assert!(!hides_on_blur(false, false));
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
