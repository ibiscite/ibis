//! The identity-only AX exception in ADR 0029. No role, title or value reads.

use std::time::{Duration, Instant};

use ibis_core::destination::CopyReason;

use super::diagnostics;

const LIMIT: Duration = Duration::from_millis(50);
// Authorization/clipboard/security are refreshed before these adjacent reads. Do not let a
// slow provider turn that final snapshot into another long stale interval. Three sequential
// reads share 15 ms (5 ms each on average), still inside the whole query's 50 ms bound.
const TAIL_LIMIT: Duration = Duration::from_millis(15);

/// This closed set is the entire attribute allowlist, including the containing
/// document and top-level surface of global focus, including foreign panels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Attribute {
    Application,
    FocusedWindow,
    FocusedElement,
    Window,
    TopLevel,
}

fn missing_identity_attribute(attribute: Attribute, error: i32) -> bool {
    matches!(attribute, Attribute::FocusedWindow | Attribute::TopLevel) && matches!(error, -25205 | -25212)
}

fn claim_process<P: PartialEq>(claimed: &mut Vec<P>, process: P) -> bool {
    if claimed.contains(&process) {
        return false;
    }
    claimed.push(process);
    true
}

/// The document and the actual input surface are distinct for sheets/drawers.
/// Neither identity can be refreshed by a later delivery.
#[derive(Clone, PartialEq, Eq)]
struct Identity<E> {
    window: E,
    surface: E,
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
    /// Capture-only exception: the native implementation requires a missing
    /// attribute on this process and claims that live process before writing.
    fn enable_manual_accessibility(&self, _pid: i32, _deadline: Instant) -> Result<bool, CopyReason> {
        Ok(false)
    }
}

fn capture_query<P: Provider>(provider: &P, pid: i32, eligible: bool) -> Result<Identity<P::Element>, CopyReason> {
    let started = provider.elapsed();
    let deadline = provider.now() + LIMIT;
    let result = query(provider, pid, deadline, || Ok(()));
    if !eligible || !matches!(result, Err(CopyReason::PasteUnavailable)) {
        return result;
    }
    let remaining = || {
        let age = provider
            .elapsed()
            .checked_sub(started)
            .ok_or(CopyReason::PasteUnavailable)?;
        LIMIT
            .checked_sub(age)
            .filter(|time| !time.is_zero())
            .ok_or(CopyReason::PasteUnavailable)
    };
    let budget = remaining()?;
    if !provider.enable_manual_accessibility(pid, deadline.min(provider.now() + budget))? {
        return result;
    }
    // Enabling AX never repairs partial evidence. Recreate the system element
    // and run every owner/relation/focus check under the ORIGINAL total budget.
    let budget = remaining()?;
    query(provider, pid, deadline.min(provider.now() + budget), || Ok(()))
}

fn query<P: Provider>(
    provider: &P,
    pid: i32,
    deadline: Instant,
    before_final_read: impl FnOnce() -> Result<(), CopyReason>,
) -> Result<Identity<P::Element>, CopyReason> {
    let started = provider.elapsed();
    let available = || {
        let age = provider
            .elapsed()
            .checked_sub(started)
            .ok_or(CopyReason::PasteUnavailable)?;
        if provider.now() >= deadline || age >= LIMIT {
            diagnostics::stage("ax.query_timeout");
            Err(CopyReason::PasteUnavailable)
        } else {
            Ok(LIMIT - age)
        }
    };
    available()?;
    if !provider.trusted() {
        diagnostics::stage("permission.accessibility");
        return Err(CopyReason::AccessibilityRequired);
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
            diagnostics::stage("ax.call_timeout");
            return Err(CopyReason::PasteUnavailable);
        }
        value
    };
    let read = |element, attribute| {
        diagnostics::stage(match attribute {
            Attribute::Application => "ax.application",
            Attribute::FocusedWindow => "ax.focused_window",
            Attribute::FocusedElement => "ax.focused_element",
            Attribute::Window => "ax.window",
            Attribute::TopLevel => "ax.top_level",
        });
        read_until(element, attribute, deadline)
    };
    let app = read(&system, Attribute::Application)?;
    let window = read(&app, Attribute::FocusedWindow)?;
    let focused = read(&system, Attribute::FocusedElement)?;
    let containing_window = read(&focused, Attribute::Window)?;
    let surface = read(&focused, Attribute::TopLevel)?;
    for element in [&app, &window, &focused, &containing_window, &surface] {
        let matches = provider.pid(element)? == pid;
        diagnostics::owner(matches);
        if !matches {
            return Err(CopyReason::TargetChanged);
        }
    }
    diagnostics::equality(Some(window == containing_window), None, None);
    if window != containing_window {
        return Err(CopyReason::TargetChanged);
    }
    // Cached/successful provider replies cannot carry a revoked grant into
    // the final tail. This passive preflight precedes all refreshed guards.
    if !provider.trusted() {
        diagnostics::stage("permission.accessibility");
        return Err(CopyReason::AccessibilityRequired);
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
        let remaining = TAIL_LIMIT
            .checked_sub(age)
            .filter(|remaining| !remaining.is_zero())
            .ok_or(CopyReason::PasteUnavailable);
        if remaining.is_err() {
            diagnostics::stage("ax.tail_timeout");
        }
        remaining
    };
    let tail_read = |element, attribute| {
        diagnostics::stage(match attribute {
            Attribute::Window => "ax.final_window",
            Attribute::TopLevel => "ax.final_surface",
            _ => "ax.final_focus",
        });
        let remaining = tail_available()?;
        let value = read_until(element, attribute, provider.now() + remaining);
        tail_available()?;
        value
    };
    // A control can keep its AX identity while moving to another window.
    // Revalidate both relations before the last global focus read.
    let document_equal = tail_read(&focused, Attribute::Window)? == window;
    diagnostics::equality(Some(document_equal), None, None);
    if !document_equal {
        return Err(CopyReason::TargetChanged);
    }
    let surface_equal = tail_read(&focused, Attribute::TopLevel)? == surface;
    diagnostics::equality(None, Some(surface_equal), None);
    if !surface_equal {
        return Err(CopyReason::TargetChanged);
    }
    let latest = tail_read(&system, Attribute::FocusedElement)?;
    diagnostics::equality(None, None, Some(latest == focused));
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
    Ok(Identity { window, surface })
}

// No AX object leaves this main-thread module; delivery workers carry only a
// nonwrapping token. Retaining the captured object prevents pointer/number reuse
// from masquerading as equality, and CFEqual supplies public AX identity.
#[allow(unsafe_code)]
mod native;

pub(super) fn capture(pid: i32, eligible: bool) -> Result<u32, CopyReason> {
    native::capture(pid, eligible)
}

pub(super) fn trusted() -> bool {
    native::trusted()
}
pub(super) fn request_trust() {
    native::request_trust();
}
pub(super) fn current(
    pid: i32,
    token: Option<u32>,
    deadline: Instant,
    before_final_read: impl FnOnce() -> Result<(), CopyReason>,
    validate: impl FnOnce() -> Result<(), CopyReason>,
) -> Result<Option<u32>, CopyReason> {
    native::current(pid, token, deadline, before_final_read, validate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    struct Fake {
        clock: Cell<Instant>,
        elapsed: Cell<Duration>,
        trusted: Cell<bool>,
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
        surface: Cell<Option<u32>>,
        surface_after_read: Option<u32>,
        unsupported: Option<Attribute>,
        missing_error: i32,
        revoke_trust_after_initial_reads: bool,
        manual: Cell<bool>,
        manual_calls: Cell<u32>,
        process: Cell<u32>,
        manual_processes: RefCell<Vec<u32>>,
        manual_failure: Option<CopyReason>,
        manual_delay: Duration,
        replace_after_manual: bool,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                clock: Cell::new(Instant::now()),
                elapsed: Cell::new(Duration::ZERO),
                trusted: Cell::new(true),
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
                surface: Cell::new(None),
                surface_after_read: None,
                unsupported: None,
                missing_error: -25205,
                revoke_trust_after_initial_reads: false,
                manual: Cell::new(false),
                manual_calls: Cell::new(0),
                process: Cell::new(1),
                manual_processes: RefCell::new(Vec::new()),
                manual_failure: None,
                manual_delay: Duration::ZERO,
                replace_after_manual: false,
            }
        }

        fn query(&self) -> Result<u32, CopyReason> {
            query(self, 42, self.now() + LIMIT, || Ok(())).map(|identity| identity.surface)
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
            self.trusted.get()
        }
        fn system(&self) -> Result<u32, CopyReason> {
            Ok(0)
        }
        fn read(&self, _: &u32, attribute: Attribute, _: Instant) -> Result<u32, CopyReason> {
            self.reads.borrow_mut().push(attribute);
            self.clock.set(self.clock.get() + self.delay);
            self.elapsed.set(self.elapsed.get() + self.delay);
            if self.reads.borrow().len() == 8 {
                self.elapsed.set(self.elapsed.get() + self.sleep_on_last);
            }
            if let Some(reason) = self.read_failure {
                return Err(reason);
            }
            if self.unsupported == Some(attribute) && !self.manual.get() {
                return Err(CopyReason::PasteUnavailable);
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
                Attribute::TopLevel => {
                    if self.revoke_trust_after_initial_reads {
                        self.trusted.set(false);
                    }
                    let surface = self.surface.get().unwrap_or(self.focused_window.get());
                    if let Some(next) = self.surface_after_read {
                        self.surface.set(Some(next));
                    }
                    surface
                }
            })
        }
        fn pid(&self, element: &u32) -> Result<i32, CopyReason> {
            Ok(if *element == 1 { 42 } else { self.focused_pid })
        }

        fn enable_manual_accessibility(&self, pid: i32, _: Instant) -> Result<bool, CopyReason> {
            if self.focused_pid != pid
                || !self
                    .unsupported
                    .is_some_and(|attribute| missing_identity_attribute(attribute, self.missing_error))
                || self.read_failure.is_some()
                || !self.trusted.get()
            {
                return Ok(false);
            }
            if !claim_process(&mut self.manual_processes.borrow_mut(), self.process.get()) {
                return Ok(false);
            }
            self.manual_calls.set(self.manual_calls.get() + 1);
            diagnostics::manual_accessibility();
            self.clock.set(self.clock.get() + self.manual_delay);
            self.elapsed.set(self.elapsed.get() + self.manual_delay);
            if let Some(reason) = self.manual_failure {
                return Err(reason);
            }
            if self.replace_after_manual {
                self.focused_window.set(200);
            }
            self.manual.set(true);
            Ok(true)
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
                    Attribute::TopLevel,
                    Attribute::Window,
                    Attribute::TopLevel,
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
    fn missing_accessibility_has_its_own_reason_without_querying_or_requesting_trust() {
        let fake = Fake {
            trusted: Cell::new(false),
            ..Fake::new()
        };
        assert_eq!(fake.query(), Err(CopyReason::AccessibilityRequired));
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
        assert_eq!(fake.reads.borrow().len(), 8);
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
    fn three_millisecond_identity_reads_fit_the_aggregate_and_final_budgets() {
        let fake = Fake {
            delay: Duration::from_millis(3),
            ..Fake::new()
        };
        assert_eq!(fake.query(), Ok(100));
        assert_eq!(fake.elapsed(), Duration::from_millis(24));
    }

    #[test]
    fn the_three_final_reads_still_have_a_shared_deadline() {
        let fake = Fake {
            delay: Duration::from_millis(5),
            ..Fake::new()
        };
        assert_eq!(fake.query(), Err(CopyReason::PasteUnavailable));
        assert_eq!(fake.reads.borrow().len(), 8);
    }

    #[test]
    fn opening_an_attached_sheet_cannot_borrow_the_documents_identity() {
        let fake = Fake::new();
        let captured = fake.query();
        fake.surface.set(Some(500));
        // The document AXFocusedWindow and AXWindow remain 100; only the
        // top-level surface distinguishes the attached sheet's input.
        assert_ne!(fake.query(), captured);
        assert_eq!(fake.query(), Ok(500));
    }

    #[test]
    fn sheets_and_drawers_are_distinct_surfaces_even_when_their_document_is_unchanged() {
        for (before, after) in [(500, 100), (500, 501), (100, 600), (600, 100)] {
            let fake = Fake {
                surface: Cell::new(Some(before)),
                ..Fake::new()
            };
            let captured = fake.query();
            assert_eq!(captured, Ok(before));
            fake.surface.set(Some(after));
            assert_eq!(fake.query(), Ok(after));
            assert_ne!(fake.query(), captured);
        }
    }

    #[test]
    fn the_same_focused_element_cannot_move_to_a_sheet_during_the_proof() {
        let fake = Fake {
            surface_after_read: Some(500),
            ..Fake::new()
        };
        assert_eq!(fake.query(), Err(CopyReason::TargetChanged));
    }

    #[test]
    fn an_unsupported_top_level_attribute_is_unavailable_without_falling_back_to_the_document() {
        let fake = Fake {
            unsupported: Some(Attribute::TopLevel),
            ..Fake::new()
        };
        assert_eq!(fake.query(), Err(CopyReason::PasteUnavailable));
    }

    #[test]
    fn revoking_accessibility_during_successful_provider_reads_keeps_its_permission_reason() {
        let fake = Fake {
            revoke_trust_after_initial_reads: true,
            ..Fake::new()
        };
        assert_eq!(fake.query(), Err(CopyReason::AccessibilityRequired));
    }

    #[test]
    fn eligible_capture_enables_manual_accessibility_for_missing_identity_then_starts_afresh() {
        for attribute in [Attribute::FocusedWindow, Attribute::TopLevel] {
            let fake = Fake {
                unsupported: Some(attribute),
                ..Fake::new()
            };
            assert!(capture_query(&fake, 42, true).is_ok());
            assert_eq!(fake.manual_calls.get(), 1);
            assert_eq!(
                fake.reads
                    .borrow()
                    .iter()
                    .filter(|&&a| a == Attribute::Application)
                    .count(),
                2
            );
            fake.manual.set(false); // A second missing result in the same live process.
            assert!(capture_query(&fake, 42, true).is_err());
            assert_eq!(fake.manual_calls.get(), 1);
            fake.process.set(2); // A restarted process, even with a reused PID.
            assert!(capture_query(&fake, 42, true).is_ok());
            assert_eq!(fake.manual_calls.get(), 2);
        }
    }

    #[test]
    fn healthy_or_refused_capture_never_activates_manual_accessibility() {
        let healthy = Fake::new();
        assert!(capture_query(&healthy, 42, true).is_ok());
        assert_eq!(healthy.manual_calls.get(), 0);
        let refused = Fake {
            unsupported: Some(Attribute::FocusedWindow),
            ..Fake::new()
        };
        assert!(capture_query(&refused, 42, false).is_err());
        assert_eq!(refused.manual_calls.get(), 0);
    }

    #[test]
    fn diagnostics_record_whether_capture_attempted_the_activation_write() {
        for failure in [None, Some(CopyReason::PasteUnavailable)] {
            let fake = Fake {
                unsupported: Some(Attribute::FocusedWindow),
                manual_failure: failure,
                ..Fake::new()
            };
            let trace = diagnostics::Trace::enabled();
            let result = trace.with(|| capture_query(&fake, 42, true));
            assert_eq!(result.is_ok(), failure.is_none());
            let record = serde_json::to_value(trace.snapshot()).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(record["manual_accessibility_ran"], true);
            let next = diagnostics::Trace::enabled();
            let _ = next.with(|| capture_query(&fake, 42, true));
            let record = serde_json::to_value(next.snapshot()).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(
                record["manual_accessibility_ran"], false,
                "no second write for this process"
            );
        }
        for eligible in [false, true] {
            let fake = Fake::new();
            let trace = diagnostics::Trace::enabled();
            assert!(trace.with(|| capture_query(&fake, 42, eligible)).is_ok());
            let record = serde_json::to_value(trace.snapshot()).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(record["manual_accessibility_ran"], false);
        }
    }

    #[test]
    fn manual_accessibility_does_not_repair_ax_errors_foreign_owners_or_missing_trust() {
        for fake in [
            Fake {
                read_failure: Some(CopyReason::PasteUnavailable),
                ..Fake::new()
            },
            Fake {
                focused_pid: 99,
                unsupported: Some(Attribute::TopLevel),
                ..Fake::new()
            },
            Fake {
                trusted: Cell::new(false),
                unsupported: Some(Attribute::TopLevel),
                ..Fake::new()
            },
        ] {
            assert!(capture_query(&fake, 42, true).is_err());
            assert_eq!(fake.manual_calls.get(), 0);
        }
    }

    #[test]
    fn only_missing_document_or_surface_attributes_qualify_for_manual_accessibility() {
        for (attribute, error, repairs) in [
            (Attribute::FocusedWindow, -25205, 1),
            (Attribute::TopLevel, -25212, 1),
            (Attribute::TopLevel, -25204, 0), // CannotComplete is failed I/O, not absence.
            (Attribute::FocusedWindow, -25202, 0), // InvalidUIElement is not absence.
            (Attribute::Window, -25212, 0),
            (Attribute::FocusedElement, -25205, 0),
            (Attribute::Application, -25205, 0),
        ] {
            let fake = Fake {
                unsupported: Some(attribute),
                missing_error: error,
                ..Fake::new()
            };
            let result = capture_query(&fake, 42, true);
            assert_eq!(fake.manual_calls.get(), repairs);
            assert_eq!(result.is_ok(), repairs == 1);
        }
    }

    #[test]
    fn failed_activation_is_not_repeated_and_activation_cannot_extend_capture_or_borrow_an_old_document() {
        let failed = Fake {
            unsupported: Some(Attribute::FocusedWindow),
            manual_failure: Some(CopyReason::PasteUnavailable),
            ..Fake::new()
        };
        for _ in 0..2 {
            assert!(capture_query(&failed, 42, true).is_err());
        }
        assert_eq!(failed.manual_calls.get(), 1);

        let late = Fake {
            unsupported: Some(Attribute::TopLevel),
            manual_delay: LIMIT,
            ..Fake::new()
        };
        assert!(capture_query(&late, 42, true).is_err());
        assert_eq!(late.reads.borrow().len(), 5, "no fresh budget after enabling");

        let replaced = Fake {
            unsupported: Some(Attribute::FocusedWindow),
            replace_after_manual: true,
            ..Fake::new()
        };
        assert!(matches!(
            capture_query(&replaced, 42, true),
            Err(CopyReason::TargetChanged)
        ));
    }
}
