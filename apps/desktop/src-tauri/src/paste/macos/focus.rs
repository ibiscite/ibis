//! The identity-only AX exception in ADR 0029. No role, title or value reads.

use std::time::{Duration, Instant};

use ibis_core::destination::CopyReason;

const LIMIT: Duration = Duration::from_millis(50);
// Clipboard/security are refreshed before this adjacent pair. Do not let a
// slow provider turn that final snapshot into another long stale interval.
const TAIL_LIMIT: Duration = Duration::from_millis(5);

/// This closed set is the entire attribute allowlist, including the containing
/// window of the globally focused element to catch foreign nonactivating panels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Attribute {
    Application,
    FocusedWindow,
    FocusedElement,
    Window,
}

trait Provider {
    type Element: PartialEq;
    fn now(&self) -> Instant;
    fn elapsed(&self) -> Duration;
    fn trusted(&self) -> bool;
    fn system(&self) -> Result<Self::Element, CopyReason>;
    fn read(
        &self,
        element: &Self::Element,
        attribute: Attribute,
        deadline: Instant,
    ) -> Result<Self::Element, CopyReason>;
    fn pid(&self, element: &Self::Element) -> Result<i32, CopyReason>;
}

fn query<P: Provider>(
    provider: &P,
    pid: i32,
    deadline: Instant,
    before_final_read: impl FnOnce() -> Result<(), CopyReason>,
) -> Result<P::Element, CopyReason> {
    let started = provider.elapsed();
    let available = || {
        let age = provider
            .elapsed()
            .checked_sub(started)
            .ok_or(CopyReason::PasteUnavailable)?;
        if provider.now() >= deadline || age >= LIMIT {
            Err(CopyReason::PasteUnavailable)
        } else {
            Ok(LIMIT - age)
        }
    };
    available()?;
    if !provider.trusted() {
        return Err(CopyReason::PasteUnavailable);
    }
    let system = provider.system()?;
    let read_until = |element: &P::Element, attribute, call_deadline: Instant| {
        let remaining = available()?;
        let call_deadline = call_deadline.min(deadline).min(provider.now() + remaining);
        let value = provider.read(element, attribute, call_deadline);
        // A late success is still a timeout. The deadline belongs to the
        // whole query, not a fresh budget for each attribute/provider.
        available()?;
        if provider.now() >= call_deadline {
            return Err(CopyReason::PasteUnavailable);
        }
        value
    };
    let read = |element, attribute| read_until(element, attribute, deadline);
    let app = read(&system, Attribute::Application)?;
    let window = read(&app, Attribute::FocusedWindow)?;
    let focused = read(&system, Attribute::FocusedElement)?;
    let containing_window = read(&focused, Attribute::Window)?;
    for element in [&app, &window, &focused, &containing_window] {
        if provider.pid(element)? != pid {
            return Err(CopyReason::TargetChanged);
        }
    }
    if window != containing_window {
        return Err(CopyReason::TargetChanged);
    }
    // Synchronous notification/provider work must finish before observing
    // live focus for the last time. Only local completion checks follow it.
    before_final_read()?;
    let tail_started = provider.elapsed();
    let tail_available = || {
        let age = provider
            .elapsed()
            .checked_sub(tail_started)
            .ok_or(CopyReason::PasteUnavailable)?;
        TAIL_LIMIT
            .checked_sub(age)
            .filter(|remaining| !remaining.is_zero())
            .ok_or(CopyReason::PasteUnavailable)
    };
    let tail_read = |element, attribute| {
        let remaining = tail_available()?;
        let value = read_until(element, attribute, provider.now() + remaining);
        tail_available()?;
        value
    };
    // A control can keep its AX identity while moving to another window.
    // Revalidate the relation immediately before the last global focus read.
    if tail_read(&focused, Attribute::Window)? != window {
        return Err(CopyReason::TargetChanged);
    }
    let latest = tail_read(&system, Attribute::FocusedElement)?;
    if latest != focused {
        // A different control in the same window is not proof the window
        // changed. The assembled identity is unavailable; a foreign owner is
        // a known target change. Do not inspect either control's properties.
        return Err(if provider.pid(&latest)? != pid {
            CopyReason::TargetChanged
        } else {
            CopyReason::PasteUnavailable
        });
    }
    available()?;
    Ok(window)
}

// No AX object leaves this main-thread module; delivery workers carry only a
// nonwrapping token. Retaining the captured object prevents pointer/number reuse
// from masquerading as equality, and CFEqual supplies public AX identity.
#[allow(unsafe_code)]
mod native;

pub(super) fn capture(pid: i32) -> Option<u32> {
    native::capture(pid)
}
pub(super) fn current(
    pid: i32,
    deadline: Instant,
    before_final_read: impl FnOnce() -> Result<(), CopyReason>,
    validate: impl FnOnce() -> Result<(), CopyReason>,
) -> Result<Option<u32>, CopyReason> {
    native::current(pid, deadline, before_final_read, validate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    struct Fake {
        clock: Cell<Instant>,
        elapsed: Cell<Duration>,
        trusted: bool,
        active_window: u32,
        focused_window: Cell<u32>,
        focused_element: Cell<u32>,
        focused_pid: i32,
        read_failure: Option<CopyReason>,
        delay: Duration,
        reads: RefCell<Vec<Attribute>>,
        change_during_read: bool,
        sleep_on_last: Duration,
        reparent_during_read: bool,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                clock: Cell::new(Instant::now()),
                elapsed: Cell::new(Duration::ZERO),
                trusted: true,
                active_window: 100,
                focused_window: Cell::new(100),
                focused_element: Cell::new(1000),
                focused_pid: 42,
                read_failure: None,
                delay: Duration::ZERO,
                reads: RefCell::default(),
                change_during_read: false,
                sleep_on_last: Duration::ZERO,
                reparent_during_read: false,
            }
        }

        fn query(&self) -> Result<u32, CopyReason> {
            query(self, 42, self.now() + LIMIT, || Ok(()))
        }
    }

    impl Provider for Fake {
        type Element = u32;
        fn now(&self) -> Instant {
            self.clock.get()
        }
        fn elapsed(&self) -> Duration {
            self.elapsed.get()
        }
        fn trusted(&self) -> bool {
            self.trusted
        }
        fn system(&self) -> Result<u32, CopyReason> {
            Ok(0)
        }
        fn read(&self, _: &u32, attribute: Attribute, _: Instant) -> Result<u32, CopyReason> {
            self.reads.borrow_mut().push(attribute);
            self.clock.set(self.clock.get() + self.delay);
            self.elapsed.set(self.elapsed.get() + self.delay);
            if self.reads.borrow().len() == 6 {
                self.elapsed.set(self.elapsed.get() + self.sleep_on_last);
            }
            if let Some(reason) = self.read_failure {
                return Err(reason);
            }
            Ok(match attribute {
                Attribute::Application => 1,
                Attribute::FocusedWindow => self.active_window,
                Attribute::FocusedElement => self.focused_element.get(),
                Attribute::Window => {
                    let window = self.focused_window.get();
                    if self.change_during_read {
                        self.focused_element.set(2000);
                    }
                    if self.reparent_during_read {
                        self.focused_window.set(200);
                    }
                    window
                }
            })
        }
        fn pid(&self, element: &u32) -> Result<i32, CopyReason> {
            Ok(if *element == 1 { 42 } else { self.focused_pid })
        }
    }

    #[test]
    fn two_windows_of_one_app_capture_the_focused_window_identity() {
        // No enumeration or sole-window assumption: either document can be key.
        for window in [100, 200] {
            let fake = Fake {
                active_window: window,
                focused_window: Cell::new(window),
                ..Fake::new()
            };
            assert_eq!(fake.query(), Ok(window));
            assert_eq!(
                *fake.reads.borrow(),
                [
                    Attribute::Application,
                    Attribute::FocusedWindow,
                    Attribute::FocusedElement,
                    Attribute::Window,
                    Attribute::Window,
                    Attribute::FocusedElement
                ]
            );
        }
    }

    #[test]
    fn a_foreign_nonactivating_panel_cannot_borrow_the_active_apps_window() {
        let fake = Fake {
            focused_pid: 99,
            ..Fake::new()
        };
        assert_eq!(fake.query(), Err(CopyReason::TargetChanged));
    }

    #[test]
    fn unavailable_ax_does_not_query_a_provider_or_request_trust() {
        let fake = Fake {
            trusted: false,
            ..Fake::new()
        };
        assert_eq!(fake.query(), Err(CopyReason::PasteUnavailable));
        assert!(fake.reads.borrow().is_empty());
    }

    #[test]
    fn timeout_uses_one_aggregate_deadline_even_if_a_provider_returns_success() {
        let fake = Fake {
            delay: LIMIT,
            ..Fake::new()
        };
        assert_eq!(fake.query(), Err(CopyReason::PasteUnavailable));
        assert_eq!(fake.reads.borrow().len(), 1);
    }

    #[test]
    fn a_closed_window_or_a_changed_focus_is_not_a_valid_identity() {
        let closed = Fake {
            read_failure: Some(CopyReason::TargetChanged),
            ..Fake::new()
        };
        assert_eq!(closed.query(), Err(CopyReason::TargetChanged));
        let changed = Fake {
            change_during_read: true,
            ..Fake::new()
        };
        assert_eq!(changed.query(), Err(CopyReason::PasteUnavailable));
    }

    #[test]
    fn sleep_inside_the_last_read_cannot_return_a_timely_success() {
        let fake = Fake {
            sleep_on_last: LIMIT,
            ..Fake::new()
        };
        let active_time = fake.now();
        assert_eq!(fake.query(), Err(CopyReason::PasteUnavailable));
        assert_eq!(fake.now(), active_time, "the active clock paused across sleep");
        assert_eq!(fake.reads.borrow().len(), 6);
    }

    #[test]
    fn a_late_error_is_also_classified_as_a_timeout() {
        let fake = Fake {
            delay: LIMIT,
            read_failure: Some(CopyReason::TargetChanged),
            ..Fake::new()
        };
        assert_eq!(fake.query(), Err(CopyReason::PasteUnavailable));
        assert_eq!(fake.reads.borrow().len(), 1);
    }

    #[test]
    fn a_focused_element_that_moves_to_another_window_cannot_keep_its_old_window_proof() {
        let fake = Fake {
            reparent_during_read: true,
            ..Fake::new()
        };
        assert_eq!(fake.query(), Err(CopyReason::TargetChanged));
    }

    #[test]
    fn the_final_window_focus_pair_has_a_tighter_budget_than_the_whole_query() {
        let fake = Fake {
            delay: Duration::from_millis(3),
            ..Fake::new()
        };
        assert_eq!(fake.query(), Err(CopyReason::PasteUnavailable));
    }
}
