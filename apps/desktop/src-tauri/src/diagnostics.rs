//! Fixed diagnostics for native operations. Never format the underlying error.

use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy)]
pub(crate) enum NativeIssue {
    Startup,
    LoginRegistrationRefresh,
    TrayCreation,
    Shortcut,
    Window,
    Unminimize,
    Show,
    Focus,
    PickerEvent,
    Visibility,
    FocusState,
    Hide,
    HideApplication,
    #[cfg(not(target_os = "macos"))]
    AlwaysOnTop,
    #[cfg(not(target_os = "macos"))]
    SkipTaskbar,
    #[cfg(target_os = "linux")]
    FullScreen,
    Monitor,
    Size,
    WindowSave,
    Resize,
    Position,
    Pointer,
    TrayLibrary,
    UpdateSetting,
    LibraryEvent,
    Notice,
    #[cfg(any(target_os = "windows", test))]
    PasteKeyRelease,
}

impl NativeIssue {
    fn message(self) -> &'static str {
        match self {
            Self::Startup => "the app could not start",
            Self::LoginRegistrationRefresh => "the login registration could not be refreshed",
            Self::TrayCreation => "the menu bar item could not be created",
            Self::Shortcut => "the shortcut needs attention; check the picker for details",
            Self::Window => "the picker window is unavailable",
            Self::Unminimize => "the picker could not be restored",
            Self::Show => "the picker could not be shown",
            Self::Focus => "the picker could not be focused",
            Self::PickerEvent => "the picker shown or hidden event could not be sent",
            Self::Visibility => "the picker visibility could not be read",
            Self::FocusState => "the picker focus state could not be read",
            Self::Hide => "the picker could not be hidden",
            Self::HideApplication => "the application could not release focus",
            #[cfg(not(target_os = "macos"))]
            Self::AlwaysOnTop => "the picker could not change whether it stays on top",
            #[cfg(not(target_os = "macos"))]
            Self::SkipTaskbar => "the picker could not stay out of the taskbar",
            #[cfg(target_os = "linux")]
            Self::FullScreen => "whether a full-screen app is in front could not be read",
            Self::Monitor => "the picker monitor could not be read",
            Self::Size => "the picker size could not be read",
            Self::WindowSave => "the picker frame save did not finish before closing",
            Self::Resize => "the picker could not be resized",
            Self::Position => "the picker could not be positioned",
            Self::Pointer => "the pointer position could not be read",
            Self::TrayLibrary => "the tray library status could not be updated",
            Self::UpdateSetting => "the automatic update setting could not be saved or announced",
            Self::LibraryEvent => "the library-changed event could not be sent",
            Self::Notice => "the Copied notice could not be shown",
            #[cfg(any(target_os = "windows", test))]
            Self::PasteKeyRelease => "paste keys could not be released; press and release Ctrl and V",
        }
    }
}

pub(crate) fn checked<T, E>(result: Result<T, E>, issue: NativeIssue) -> Option<T> {
    checked_with(result, issue, application_message)
}

/// Existing application messages have already passed their source's allowlist.
/// Native errors must use `checked`/`report`, never this function.
pub(crate) fn application_message(message: &str) {
    write_diagnostic(std::io::stderr().lock(), message);
}

fn write_diagnostic(mut output: impl std::io::Write, message: &str) {
    // A closed/full diagnostic sink must not interrupt native recovery or events.
    let _ = writeln!(output, "ibis: {message}");
}

pub(crate) fn report<T, E>(result: Result<T, E>, issue: NativeIssue) {
    let _ = checked(result, issue);
}

/// Only persistent capability failures use this; retry the native operation on
/// every call, but report its first failure once for this process.
pub(crate) fn checked_once<T, E>(result: Result<T, E>, issue: NativeIssue, reported: &AtomicBool) -> Option<T> {
    checked_once_with(result, issue, reported, application_message)
}

fn checked_once_with<T, E>(
    result: Result<T, E>,
    issue: NativeIssue,
    reported: &AtomicBool,
    log: impl FnOnce(&'static str),
) -> Option<T> {
    checked_with(result, issue, |message| {
        // Only the flag is shared; it does not guard any other memory.
        if !reported.swap(true, Ordering::Relaxed) {
            log(message);
        }
    })
}

fn checked_with<T, E>(result: Result<T, E>, issue: NativeIssue, log: impl FnOnce(&'static str)) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(_) => {
            log(issue.message());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct PrivateError;

    impl std::fmt::Display for PrivateError {
        fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            panic!("the synthetic private error must never be formatted")
        }
    }

    impl std::fmt::Debug for PrivateError {
        fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            panic!("the synthetic private error must never be debug-formatted")
        }
    }

    #[test]
    fn native_failures_report_only_the_allowlisted_operation() {
        for issue in [
            NativeIssue::Startup,
            NativeIssue::LoginRegistrationRefresh,
            NativeIssue::TrayCreation,
            NativeIssue::Shortcut,
            NativeIssue::Window,
            NativeIssue::Unminimize,
            NativeIssue::Show,
            NativeIssue::Focus,
            NativeIssue::PickerEvent,
            NativeIssue::Visibility,
            NativeIssue::FocusState,
            NativeIssue::Hide,
            NativeIssue::HideApplication,
            #[cfg(not(target_os = "macos"))]
            NativeIssue::AlwaysOnTop,
            #[cfg(not(target_os = "macos"))]
            NativeIssue::SkipTaskbar,
            #[cfg(target_os = "linux")]
            NativeIssue::FullScreen,
            NativeIssue::Monitor,
            NativeIssue::Size,
            NativeIssue::Resize,
            NativeIssue::Position,
            NativeIssue::Pointer,
            NativeIssue::TrayLibrary,
            NativeIssue::LibraryEvent,
            NativeIssue::PasteKeyRelease,
        ] {
            let mut messages = Vec::new();
            let value = checked_with(Err::<(), _>(PrivateError), issue, |message| messages.push(message));
            assert!(value.is_none());
            assert_eq!(messages, [issue.message()]);
        }
    }
}
