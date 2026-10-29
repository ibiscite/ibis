//! Test-only Notepad lifecycle and activation, also exercised without Windows.

use std::process::Child;
use std::time::{Duration, Instant};

pub(crate) fn wait_for<T>(limit: Duration, mut condition: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + limit;
    loop {
        if let Some(value) = condition() {
            return Some(value);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return None;
        }
        std::thread::sleep(remaining.min(Duration::from_millis(100)));
    }
}

/// Keep stable process handles, never infer ownership from a PID or HWND.
/// Even the launcher may restore unrelated documents, so neither process is
/// known to be exclusive to this test. Dropping a Child does not terminate it.
pub(crate) struct Session<P> {
    pub launcher: Child,
    pub process: Option<P>,
}

impl<P> Drop for Session<P> {
    fn drop(&mut self) {
        let _ = self.launcher.try_wait();
    }
}

/// Each attempt must wait for activation to complete before trying ALT.
/// Focus recovery runs after activation, including when ALT entered menu mode.
pub(crate) fn activate(
    limit: Duration,
    mut request: impl FnMut(),
    in_front: impl Fn() -> bool,
    tap_alt: impl FnOnce(),
    focus_document: impl FnOnce(bool),
) -> bool {
    request();
    let used_alt = wait_for(limit, || in_front().then_some(())).is_none();
    if used_alt {
        tap_alt();
        request();
        if wait_for(limit, || in_front().then_some(())).is_none() {
            return false;
        }
    }
    focus_document(used_alt);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};

    #[test]
    fn delayed_activation_finishes_without_alt() {
        let checks = Cell::new(0);
        let focused = Cell::new(false);
        assert!(activate(
            Duration::from_millis(1),
            || {},
            || {
                // The request returned before the target input queue handled it.
                checks.set(checks.get() + 1);
                checks.get() > 1
            },
            || panic!("ALT must not race an activation that is still completing"),
            |used_alt| {
                assert!(!used_alt);
                assert!(checks.get() > 1);
                focused.set(true);
            }
        ));
        assert!(focused.get());
    }

    #[test]
    fn alt_menu_mode_is_recovered_after_activation() {
        let attempts = Cell::new(0);
        let menu = Cell::new(false);
        let foreground = Cell::new(false);
        let focused = Cell::new(false);
        assert!(activate(
            Duration::ZERO,
            || {
                attempts.set(attempts.get() + 1);
                foreground.set(attempts.get() == 2);
            },
            || foreground.get(),
            || {
                assert_eq!(attempts.get(), 1);
                menu.set(true);
            },
            |used_alt| {
                assert!(used_alt);
                assert_eq!(attempts.get(), 2);
                assert!(menu.replace(false));
                focused.set(true);
            }
        ));
        assert!(!menu.get());
        assert!(focused.get());
    }

    #[test]
    fn denied_activation_does_not_focus_another_app() {
        let tapped = Cell::new(false);
        assert!(!activate(
            Duration::ZERO,
            || {},
            || false,
            || tapped.set(true),
            |_| panic!("must not focus or dismiss menus in another app")
        ));
        assert!(tapped.get());
    }

    #[test]
    fn cleanup_leaves_launcher_and_handed_off_process_alive() {
        let spawn = || {
            Command::new(std::env::current_exe().unwrap_or_else(|error| panic!("test executable: {error}")))
                .args(["notepad_process_child", "--ignored", "--nocapture"])
                .env("IBIS_NOTEPAD_TEST_CHILD", "1")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap_or_else(|error| panic!("test child: {error}"))
        };
        let mut launcher = spawn();
        let mut shared = spawn();
        let pipes = [&mut launcher, &mut shared].map(|child| (child.stdin.take(), child.stdout.take()));
        let session = Session {
            launcher,
            process: Some(shared),
        };
        assert!(session.process.is_some());
        drop(session);
        for (input, output) in pipes {
            let mut input = input.unwrap_or_else(|| panic!("child stdin"));
            writeln!(input, "synthetic-document-still-open")
                .unwrap_or_else(|error| panic!("child was killed: {error}"));
            drop(input);
            let mut reply = String::new();
            output
                .unwrap_or_else(|| panic!("child stdout"))
                .read_to_string(&mut reply)
                .unwrap_or_else(|error| panic!("child reply: {error}"));
            assert!(reply.contains("preserved: synthetic-document-still-open"));
        }
    }

    #[test]
    #[ignore = "subprocess fixture for the lifecycle regression test"]
    fn notepad_process_child() {
        if std::env::var("IBIS_NOTEPAD_TEST_CHILD").as_deref() != Ok("1") {
            return;
        }
        let mut message = String::new();
        std::io::stdin()
            .read_line(&mut message)
            .unwrap_or_else(|error| panic!("child input: {error}"));
        println!("preserved: {}", message.trim());
    }
}
