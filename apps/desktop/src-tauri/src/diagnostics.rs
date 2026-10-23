//! Fixed diagnostics for native operations. Never format the underlying error.

use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy)]
pub(crate) enum NativeIssue {
    Startup,
    LoginRegistrationRefresh,
    #[cfg(target_os = "macos")]
    ReapprovalNotice,
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
    Position,
    Pointer,
    TrayLibrary,
    LoginMenuUpdate,
    LoginMenuRead,
    AutomaticMenuUpdate,
    LibraryEvent,
    Notice,
    #[cfg(debug_assertions)]
    GalleryNavigation,
}

impl NativeIssue {
    fn message(self) -> &'static str {
        match self {
            Self::Startup => "the app could not start",
            Self::LoginRegistrationRefresh => "the login registration could not be refreshed",
            #[cfg(target_os = "macos")]
            Self::ReapprovalNotice => "the Open at Login notice remains pending",
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
            Self::Position => "the picker could not be positioned",
            Self::Pointer => "the pointer position could not be read",
            Self::TrayLibrary => "the tray library status could not be updated",
            Self::LoginMenuUpdate => "the login menu check could not be updated",
            Self::LoginMenuRead => "the login menu check could not be read",
            Self::AutomaticMenuUpdate => "the automatic update menu check could not be updated",
            Self::LibraryEvent => "the library-changed event could not be sent",
            Self::Notice => "the Copied notice could not be shown",
            #[cfg(debug_assertions)]
            Self::GalleryNavigation => "the debug gallery could not be opened",
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
            NativeIssue::Position,
            NativeIssue::Pointer,
            NativeIssue::TrayLibrary,
            NativeIssue::LibraryEvent,
            #[cfg(debug_assertions)]
            NativeIssue::GalleryNavigation,
        ] {
            let mut messages = Vec::new();
            let value = checked_with(Err::<(), _>(PrivateError), issue, |message| messages.push(message));
            assert!(value.is_none());
            assert_eq!(messages, [issue.message()]);
        }
    }

    #[test]
    fn menu_diagnostics_keep_fixed_labels_and_successful_check_values() {
        for (issue, expected) in [
            (
                NativeIssue::LoginMenuUpdate,
                "ibis: the login menu check could not be updated\n",
            ),
            (
                NativeIssue::LoginMenuRead,
                "ibis: the login menu check could not be read\n",
            ),
            (
                NativeIssue::AutomaticMenuUpdate,
                "ibis: the automatic update menu check could not be updated\n",
            ),
        ] {
            let mut output = Vec::new();
            let value = checked_with(Err::<bool, _>(PrivateError), issue, |message| {
                write_diagnostic(&mut output, message)
            });
            assert_eq!(value, None);
            assert_eq!(output, expected.as_bytes());
        }
        for value in [false, true] {
            assert_eq!(
                checked_with(Ok::<_, PrivateError>(value), NativeIssue::LoginMenuRead, |_| {
                    panic!("a successful check read must not report a failure")
                }),
                Some(value)
            );
        }
    }

    #[test]
    fn repeated_pointer_failures_report_once_without_caching_the_result() {
        use std::sync::atomic::AtomicBool;

        let reported = AtomicBool::new(false);
        let mut messages = Vec::new();
        for result in [Ok(7), Err(PrivateError), Err(PrivateError), Ok(9), Err(PrivateError)] {
            let expected = result.as_ref().ok().copied();
            let actual = checked_once_with(result, NativeIssue::Pointer, &reported, |message| {
                messages.push(message)
            });
            assert_eq!(actual, expected);
        }
        assert_eq!(messages, [NativeIssue::Pointer.message()]);
    }

    #[test]
    fn concurrent_pointer_failures_emit_a_single_fixed_message() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

        let reported = AtomicBool::new(false);
        let messages = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..16 {
                scope.spawn(|| {
                    assert!(
                        checked_once_with(Err::<(), _>(PrivateError), NativeIssue::Pointer, &reported, |message| {
                            assert_eq!(message, NativeIssue::Pointer.message());
                            messages.fetch_add(1, Ordering::Relaxed);
                        })
                        .is_none()
                    );
                });
            }
        });
        assert_eq!(messages.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn login_registration_refresh_failure_uses_only_the_fixed_message() {
        let mut output = Vec::new();
        let value = checked_with(
            Err::<(), _>(PrivateError),
            NativeIssue::LoginRegistrationRefresh,
            |message| write_diagnostic(&mut output, message),
        );
        assert_eq!(value, None);
        assert_eq!(output, b"ibis: the login registration could not be refreshed\n");
    }

    #[test]
    fn successful_native_values_are_preserved_without_a_diagnostic() {
        let mut messages = Vec::new();
        let value = checked_with(Ok::<_, PrivateError>(false), NativeIssue::Visibility, |message| {
            messages.push(message)
        });
        assert_eq!(value, Some(false));
        assert!(messages.is_empty());
        assert_eq!(checked(Ok::<_, PrivateError>(42), NativeIssue::Size), Some(42));
        report(Ok::<_, PrivateError>(()), NativeIssue::Show);
    }

    #[test]
    fn an_unwritable_diagnostic_sink_does_not_interrupt_recovery() {
        struct BrokenSink;
        impl std::io::Write for BrokenSink {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("synthetic diagnostic sink failure"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        for issue in [
            NativeIssue::Show,
            NativeIssue::LoginRegistrationRefresh,
            NativeIssue::Startup,
            NativeIssue::TrayCreation,
            NativeIssue::Shortcut,
            NativeIssue::LoginMenuUpdate,
            NativeIssue::LoginMenuRead,
            NativeIssue::AutomaticMenuUpdate,
        ] {
            assert!(
                checked_with(Err::<(), _>(PrivateError), issue, |message| {
                    write_diagnostic(BrokenSink, message)
                })
                .is_none()
            );
        }
        let application = String::from("the chosen file is not on this computer");
        write_diagnostic(BrokenSink, &application);
        let mut output = Vec::new();
        write_diagnostic(&mut output, NativeIssue::Show.message());
        write_diagnostic(&mut output, &application);
        assert_eq!(
            output,
            b"ibis: the picker could not be shown\nibis: the chosen file is not on this computer\n"
        );
    }
}
