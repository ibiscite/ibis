//! Fixed diagnostics for native operations. Never format the underlying error.

#[derive(Clone, Copy)]
pub(crate) enum NativeIssue {
    Window,
    Unminimize,
    Show,
    Focus,
    PickerEvent,
    Visibility,
    FocusState,
    Hide,
    Monitor,
    Size,
    Position,
    Pointer,
    TrayLibrary,
    LibraryEvent,
}

impl NativeIssue {
    fn message(self) -> &'static str {
        match self {
            Self::Window => "the picker window is unavailable",
            Self::Unminimize => "the picker could not be restored",
            Self::Show => "the picker could not be shown",
            Self::Focus => "the picker could not be focused",
            Self::PickerEvent => "the picker-shown event could not be sent",
            Self::Visibility => "the picker visibility could not be read",
            Self::FocusState => "the picker focus state could not be read",
            Self::Hide => "the picker could not be hidden",
            Self::Monitor => "the picker monitor could not be read",
            Self::Size => "the picker size could not be read",
            Self::Position => "the picker could not be positioned",
            Self::Pointer => "the pointer position could not be read",
            Self::TrayLibrary => "the tray library status could not be updated",
            Self::LibraryEvent => "the library-changed event could not be sent",
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
            NativeIssue::Window,
            NativeIssue::Unminimize,
            NativeIssue::Show,
            NativeIssue::Focus,
            NativeIssue::PickerEvent,
            NativeIssue::Visibility,
            NativeIssue::FocusState,
            NativeIssue::Hide,
            NativeIssue::Monitor,
            NativeIssue::Size,
            NativeIssue::Position,
            NativeIssue::Pointer,
            NativeIssue::TrayLibrary,
            NativeIssue::LibraryEvent,
        ] {
            let mut messages = Vec::new();
            let value = checked_with(Err::<(), _>(PrivateError), issue, |message| messages.push(message));
            assert!(value.is_none());
            assert_eq!(messages, [issue.message()]);
        }
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
        assert!(
            checked_with(Err::<(), _>(PrivateError), NativeIssue::Show, |message| {
                write_diagnostic(BrokenSink, message)
            })
            .is_none()
        );
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
