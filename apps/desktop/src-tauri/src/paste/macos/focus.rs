//! The identity-only AX exception in ADR 0029. No role, title or value reads,
//! but for the one selection read beside it: password subrole, selected range
//! and selected text of the focused control, with its own closed call set.

use std::time::{Duration, Instant};

use ibis_core::destination::CopyReason;
use objc2_app_kit::NSRunningApplication;
use tauri_nspanel::objc2::rc::Retained;

use super::super::capture::{OldCitation, SHORT_SELECTION, SelectedText, Selection};
use super::diagnostics;

const LIMIT: Duration = Duration::from_millis(50);
// Authorization/clipboard/security are refreshed before these adjacent reads. Do not let a
// slow provider turn that final snapshot into another long stale interval. Three sequential
// reads share 15 ms (5 ms each on average), still inside the whole query's 50 ms bound.
const TAIL_LIMIT: Duration = Duration::from_millis(15);
/// The selection read at opening: a slower app opens with an empty search,
/// inside the opening budget, and the final read answers instead.
const SELECTION_LIMIT: Duration = Duration::from_millis(30);
/// The read just before the paste. It decides whether the paste happens, so a
/// busy app gets longer, still well inside the 250 ms post limit.
const FINAL_SELECTION_LIMIT: Duration = Duration::from_millis(60);

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

fn same_live_process<P: PartialEq>(original: &P, current: Option<&P>, terminated: bool) -> bool {
    !terminated && current == Some(original)
}

/// The document and the actual input surface are distinct for sheets/drawers.
/// Neither identity can be refreshed by a later delivery.
#[derive(Clone, PartialEq, Eq)]
struct Identity<E> {
    window: E,
    surface: E,
}

/// One answer from the selection calls; `TooLong` needs no text to be copied.
#[derive(Debug, PartialEq, Eq)]
enum Reply<T> {
    Value(T),
    TooLong,
    /// The control has no selected text right now.
    NoValue,
    /// The control does not report this attribute at all.
    Unsupported,
    Failed,
}

/// What an AX error says about a selection attribute. A control that does not
/// implement the call has none, which is not a failed read.
fn selection_error(error: i32) -> Reply<()> {
    match error {
        0 => Reply::Value(()),
        -25212 => Reply::NoValue,
        -25205 | -25208 => Reply::Unsupported,
        _ => Reply::Failed,
    }
}

/// An empty range may carry `kCFNotFound` as its location; it still says that
/// nothing is selected.
fn range_length(read: bool, location: isize, length: isize) -> Reply<usize> {
    match usize::try_from(length) {
        Ok(0) if read => Reply::Value(0),
        Ok(length) if read && location >= 0 => Reply::Value(length),
        _ => Reply::Failed,
    }
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
    /// System-wide secure typing; `None` when it cannot be read here.
    fn secure_typing(&self) -> Option<bool>;
    fn password_field(&self, element: &Self::Element, deadline: Instant) -> Reply<bool>;
    /// Selected length in UTF-16 units, so long text is never copied across.
    fn selected_length(&self, element: &Self::Element, deadline: Instant) -> Reply<usize>;
    /// `limit` units at most; anything longer answers `TooLong` without a copy.
    fn selected_text(&self, element: &Self::Element, limit: usize, deadline: Instant) -> Reply<String>;
}

/// A budget for one selection read: a late success is still a timeout.
fn selection_budget<P: Provider>(
    provider: &P,
    limit: Duration,
    deadline: Instant,
) -> impl Fn() -> Result<(), CopyReason> {
    let started = provider.elapsed();
    move || {
        let age = provider
            .elapsed()
            .checked_sub(started)
            .ok_or(CopyReason::PasteUnavailable)?;
        if provider.now() >= deadline || age >= limit {
            diagnostics::stage("ax.selection_timeout");
            return Err(CopyReason::PasteUnavailable);
        }
        Ok(())
    }
}

/// The focused control, once it is safe to ask for its selected text: secure
/// typing off, Accessibility granted, owned by the target, not a password field.
fn readable_control<P: Provider>(
    provider: &P,
    pid: i32,
    limit: Duration,
    deadline: Instant,
) -> Result<P::Element, CopyReason> {
    match provider.secure_typing() {
        Some(false) => {}
        Some(true) => return Err(CopyReason::SecureInput),
        None => return Err(CopyReason::PasteUnavailable),
    }
    if !provider.trusted() {
        return Err(CopyReason::AccessibilityRequired);
    }
    let available = selection_budget(provider, limit, deadline);
    let system = provider.system()?;
    let focused = provider.read(&system, Attribute::FocusedElement, deadline)?;
    available()?;
    let owned = provider.pid(&focused)? == pid;
    diagnostics::owner(owned);
    if !owned {
        return Err(CopyReason::TargetChanged);
    }
    diagnostics::stage("ax.selection_field");
    match provider.password_field(&focused, deadline) {
        Reply::Value(false) | Reply::NoValue | Reply::Unsupported => {}
        Reply::Value(true) => return Err(CopyReason::SecureField),
        Reply::TooLong | Reply::Failed => return Err(CopyReason::PasteUnavailable),
    }
    available()?;
    Ok(focused)
}

/// The focused control's selection, or why it stays unread. Never touches the
/// clipboard or posts a key; every failure leaves the search empty.
fn read_selection<P: Provider>(
    provider: &P,
    pid: i32,
    limit: Duration,
    deadline: Instant,
) -> Result<(Selection, Option<SelectedText>), CopyReason> {
    let available = selection_budget(provider, limit, deadline);
    let focused = readable_control(provider, pid, limit, deadline)?;
    diagnostics::stage("ax.selection_range");
    let something_selected = match provider.selected_length(&focused, deadline) {
        Reply::Value(0) => return Ok((Selection::Nothing, None)),
        Reply::Value(length) if length > SHORT_SELECTION => return Ok((Selection::Ignored, None)),
        Reply::TooLong => return Ok((Selection::Ignored, None)),
        Reply::Failed => return Err(CopyReason::PasteUnavailable),
        Reply::Value(_) => true,
        Reply::NoValue | Reply::Unsupported => false,
    };
    available()?;
    diagnostics::stage("ax.selection_text");
    let selected = match provider.selected_text(&focused, SHORT_SELECTION, deadline) {
        Reply::Value(text) if !text.is_empty() => Selection::of_text(&text),
        Reply::TooLong => (Selection::Ignored, None),
        Reply::Failed => return Err(CopyReason::PasteUnavailable),
        // Text is selected but the app will not say what: it cannot be shown.
        Reply::Value(_) | Reply::NoValue | Reply::Unsupported if something_selected => (Selection::Ignored, None),
        Reply::Value(_) | Reply::NoValue => (Selection::Nothing, None),
        Reply::Unsupported => (Selection::Unknown, None),
    };
    // A late success is still a timeout: the app may have moved on.
    available()?;
    Ok(selected)
}

/// A changed citation replaces only the old one (#1245). Whatever cannot prove
/// that is "not selected": the citation stays copied and the notice says so.
fn old_citation_selected<P: Provider>(
    provider: &P,
    pid: i32,
    old: &OldCitation,
    deadline: Instant,
) -> Result<(), CopyReason> {
    let available = selection_budget(provider, FINAL_SELECTION_LIMIT, deadline);
    let focused = match readable_control(provider, pid, FINAL_SELECTION_LIMIT, deadline) {
        Err(CopyReason::SecureField | CopyReason::PasteUnavailable) => return Err(CopyReason::CitationNotSelected),
        focused => focused?,
    };
    let limit = old.limit();
    if matches!(provider.selected_length(&focused, deadline), Reply::Value(length) if length == 0 || length > limit) {
        return Err(CopyReason::CitationNotSelected);
    }
    available().map_err(|_| CopyReason::CitationNotSelected)?;
    let selected = provider.selected_text(&focused, limit, deadline);
    available().map_err(|_| CopyReason::CitationNotSelected)?;
    match selected {
        Reply::Value(text) if old.is(&text) => Ok(()),
        _ => Err(CopyReason::CitationNotSelected),
    }
}

/// The selection for the search, or `Unread` for the final read to settle:
/// refused apps are never asked, and no failure here stops the picker opening.
fn opening_selection<P: Provider>(
    provider: &P,
    pid: i32,
    allowed: bool,
    deadline: Instant,
) -> (Selection, Option<SelectedText>) {
    if !allowed {
        return (Selection::Unread, None);
    }
    read_selection(provider, pid, SELECTION_LIMIT, deadline).unwrap_or((Selection::Unread, None))
}

/// The final proof before posting. `Unknown` cannot answer, so it asks nothing.
fn check_selection<P: Provider>(
    provider: &P,
    pid: i32,
    expected: &Selection,
    deadline: Instant,
) -> Result<(), CopyReason> {
    match expected {
        Selection::Unknown => return Ok(()),
        Selection::Citation(old) => return old_citation_selected(provider, pid, old, deadline),
        Selection::Unread | Selection::Nothing | Selection::Ignored | Selection::Short(_) => {}
    }
    let (found, _text) = read_selection(provider, pid, FINAL_SELECTION_LIMIT, deadline)?;
    match (expected, found) {
        (Selection::Unread, Selection::Nothing | Selection::Unknown) => Ok(()),
        (_, Selection::Unknown) => Err(CopyReason::PasteUnavailable),
        (expected, found) if *expected == found => Ok(()),
        _ => Err(CopyReason::SelectionNotShown),
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

/// The capture, or why it failed in the person’s words.
fn capture_reason<P: Provider>(provider: &P, pid: i32, eligible: bool) -> Result<Identity<P::Element>, CopyReason> {
    capture_query(provider, pid, eligible)
        .map_err(|reason| refused_because(provider, pid, reason, provider.now() + SELECTION_LIMIT))
}

/// A paste already refused as unprovable or for secure typing says why: the
/// target's focused control hides typing, or secure typing is on. It runs only
/// after the refusal, so it changes the words and never the outcome; it reads
/// the focused control's subrole and nothing of its text (decision 0029).
fn refused_because<P: Provider>(provider: &P, pid: i32, refused: CopyReason, deadline: Instant) -> CopyReason {
    if !matches!(refused, CopyReason::PasteUnavailable | CopyReason::SecureInput) || !provider.trusted() {
        return refused;
    }
    let hides_typing = provider
        .system()
        .and_then(|system| provider.read(&system, Attribute::FocusedElement, deadline))
        .ok()
        .filter(|focused| provider.pid(focused) == Ok(pid) && provider.now() < deadline)
        .is_some_and(|focused| matches!(provider.password_field(&focused, deadline), Reply::Value(true)));
    if hides_typing {
        CopyReason::SecureField
    } else if provider.secure_typing() == Some(true) {
        CopyReason::SecureInput
    } else {
        refused
    }
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

pub(super) fn capture(
    pid: i32,
    eligible: Option<Retained<NSRunningApplication>>,
) -> Result<(u32, Selection, Option<SelectedText>), CopyReason> {
    native::capture(pid, eligible)
}

pub(super) fn selection_unchanged(pid: i32, expected: &Selection, deadline: Instant) -> Result<(), CopyReason> {
    native::selection_unchanged(pid, expected, deadline)
}

pub(super) fn refused(pid: i32, reason: CopyReason) -> CopyReason {
    native::refused(pid, reason)
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
        eligible_process: Cell<u32>,
        original_terminated: bool,
        process_after_missing: Option<u32>,
        process_after_prepare: Option<u32>,
        manual_processes: RefCell<Vec<u32>>,
        manual_failure: Option<CopyReason>,
        manual_delay: Duration,
        replace_after_manual: bool,
        secure_typing: Cell<Option<bool>>,
        password: Reply<bool>,
        selected_length: Cell<Option<usize>>,
        selected: RefCell<Reply<String>>,
        selection_reads: RefCell<Vec<&'static str>>,
        selection_delay: Duration,
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
                eligible_process: Cell::new(1),
                original_terminated: false,
                process_after_missing: None,
                process_after_prepare: None,
                manual_processes: RefCell::new(Vec::new()),
                manual_failure: None,
                manual_delay: Duration::ZERO,
                replace_after_manual: false,
                secure_typing: Cell::new(Some(false)),
                password: Reply::Value(false),
                selected_length: Cell::new(None),
                selected: RefCell::new(Reply::Unsupported),
                selection_reads: RefCell::default(),
                selection_delay: Duration::ZERO,
            }
        }

        fn selection(&self) -> Result<(Selection, Option<String>), CopyReason> {
            read_selection(self, 42, SELECTION_LIMIT, self.now() + SELECTION_LIMIT)
                .map(|(selection, text)| (selection, text.map(SelectedText::into_search)))
        }

        fn selecting(text: &str) -> Self {
            Self {
                selected_length: Cell::new(Some(text.encode_utf16().count())),
                selected: RefCell::new(Reply::Value(text.to_owned())),
                ..Self::new()
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
                if let Some(process) = self.process_after_missing {
                    self.process.set(process);
                }
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

        fn secure_typing(&self) -> Option<bool> {
            self.secure_typing.get()
        }
        fn password_field(&self, _: &u32, _: Instant) -> Reply<bool> {
            self.selection_reads.borrow_mut().push("subrole");
            match self.password {
                Reply::Value(value) => Reply::Value(value),
                Reply::NoValue => Reply::NoValue,
                Reply::Unsupported => Reply::Unsupported,
                Reply::TooLong | Reply::Failed => Reply::Failed,
            }
        }
        fn selected_length(&self, _: &u32, _: Instant) -> Reply<usize> {
            self.selection_reads.borrow_mut().push("range");
            self.selected_length.get().map_or(Reply::Unsupported, Reply::Value)
        }
        fn selected_text(&self, _: &u32, limit: usize, _: Instant) -> Reply<String> {
            self.selection_reads.borrow_mut().push("text");
            self.clock.set(self.clock.get() + self.selection_delay);
            self.elapsed.set(self.elapsed.get() + self.selection_delay);
            match &*self.selected.borrow() {
                Reply::Value(text) if text.encode_utf16().count() > limit => Reply::TooLong,
                Reply::Value(text) => Reply::Value(text.clone()),
                Reply::TooLong => Reply::TooLong,
                Reply::NoValue => Reply::NoValue,
                Reply::Unsupported => Reply::Unsupported,
                Reply::Failed => Reply::Failed,
            }
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
            let check = || {
                if same_live_process(
                    &self.eligible_process.get(),
                    Some(&self.process.get()),
                    self.original_terminated,
                ) {
                    Ok(())
                } else {
                    Err(CopyReason::TargetChanged)
                }
            };
            check()?;
            if !claim_process(&mut self.manual_processes.borrow_mut(), self.eligible_process.get()) {
                return Ok(false);
            }
            if let Some(process) = self.process_after_prepare {
                self.process.set(process);
            }
            check()?;
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
    fn short_selected_text_reaches_the_search_trimmed_in_one_closed_read() {
        let fake = Fake::selecting("smith 2020 ");
        let (selection, text) = fake.selection().unwrap_or_else(|reason| panic!("{reason:?}"));
        assert_eq!(selection, Selection::of_text("smith 2020 ").0);
        assert_eq!(text.as_deref(), Some("smith 2020"));
        assert_eq!(*fake.selection_reads.borrow(), ["subrole", "range", "text"]);
        assert_eq!(*fake.reads.borrow(), [Attribute::FocusedElement]);
    }

    #[test]
    fn a_long_selection_is_recognised_without_copying_its_text_across() {
        let fake = Fake {
            selected_length: Cell::new(Some(SHORT_SELECTION + 1)),
            selected: RefCell::new(Reply::Value("Invented private words".into())),
            ..Fake::new()
        };
        assert_eq!(fake.selection(), Ok((Selection::Ignored, None)));
        assert!(!fake.selection_reads.borrow().contains(&"text"));
    }

    #[test]
    fn nothing_selected_stops_at_the_range_and_apps_without_a_range_still_answer() {
        let empty = Fake::selecting("");
        assert_eq!(empty.selection(), Ok((Selection::Nothing, None)));
        assert!(!empty.selection_reads.borrow().contains(&"text"));
        for (reply, expected) in [
            (Reply::Value(String::new()), Selection::Nothing),
            (Reply::NoValue, Selection::Nothing),
            (Reply::TooLong, Selection::Ignored),
            (Reply::Unsupported, Selection::Unknown),
        ] {
            let fake = Fake {
                selected: RefCell::new(reply),
                ..Fake::new()
            };
            assert_eq!(fake.selection(), Ok((expected, None)));
        }
    }

    #[test]
    fn secure_typing_refuses_before_anything_is_asked_of_the_app() {
        for (state, reason) in [
            (Some(true), CopyReason::SecureInput),
            (None, CopyReason::PasteUnavailable),
        ] {
            let fake = Fake::selecting("smith 2020");
            fake.secure_typing.set(state);
            assert_eq!(fake.selection(), Err(reason));
            assert!(fake.reads.borrow().is_empty());
            assert!(fake.selection_reads.borrow().is_empty());
        }
    }

    #[test]
    fn a_password_field_is_refused_before_its_range_or_text_is_read() {
        let fake = Fake {
            password: Reply::Value(true),
            ..Fake::selecting("hunter2")
        };
        assert_eq!(fake.selection(), Err(CopyReason::SecureField));
        assert_eq!(*fake.selection_reads.borrow(), ["subrole"]);
        let unproved = Fake {
            password: Reply::Failed,
            ..Fake::selecting("hunter2")
        };
        assert_eq!(unproved.selection(), Err(CopyReason::PasteUnavailable));
        assert_eq!(*unproved.selection_reads.borrow(), ["subrole"]);
    }

    #[test]
    fn a_field_without_a_subrole_is_an_ordinary_field() {
        for password in [Reply::NoValue, Reply::Unsupported] {
            let fake = Fake {
                password,
                ..Fake::selecting("smith 2020")
            };
            assert!(fake.selection().is_ok_and(|(_, text)| text.is_some()));
        }
    }

    #[test]
    fn a_foreign_panel_failed_read_or_revoked_trust_keeps_the_search_empty() {
        let foreign = Fake {
            focused_pid: 99,
            ..Fake::selecting("smith 2020")
        };
        assert_eq!(foreign.selection(), Err(CopyReason::TargetChanged));
        assert!(foreign.selection_reads.borrow().is_empty());
        let failed = Fake {
            selected: RefCell::new(Reply::Failed),
            ..Fake::new()
        };
        assert_eq!(failed.selection(), Err(CopyReason::PasteUnavailable));
        let revoked = Fake::selecting("smith 2020");
        revoked.trusted.set(false);
        assert_eq!(revoked.selection(), Err(CopyReason::AccessibilityRequired));
    }

    #[test]
    fn a_late_answer_is_a_timeout_even_when_it_succeeded() {
        let fake = Fake {
            selection_delay: SELECTION_LIMIT,
            ..Fake::selecting("smith 2020")
        };
        assert_eq!(fake.selection(), Err(CopyReason::PasteUnavailable));
    }

    #[test]
    fn the_final_proof_passes_only_for_the_selection_the_picker_opened_on() {
        let opened = Selection::of_text("smith 2020").0;
        let check =
            |fake: &Fake, expected: &Selection| check_selection(fake, 42, expected, fake.now() + FINAL_SELECTION_LIMIT);
        assert_eq!(check(&Fake::selecting("smith 2020"), &opened), Ok(()));
        assert_eq!(
            check(&Fake::selecting("jones 2021"), &opened),
            Err(CopyReason::SelectionNotShown)
        );
        assert_eq!(check(&Fake::selecting(""), &opened), Err(CopyReason::SelectionNotShown));
        assert_eq!(
            check(&Fake::selecting("new"), &Selection::Nothing),
            Err(CopyReason::SelectionNotShown),
            "text selected after opening was never shown"
        );
        assert_eq!(check(&Fake::selecting(""), &Selection::Nothing), Ok(()));
        let unread = Fake::new();
        assert_eq!(check(&unread, &opened), Err(CopyReason::PasteUnavailable));
    }

    #[test]
    fn an_app_that_cannot_report_a_selection_is_asked_nothing_at_the_final_proof() {
        let fake = Fake::selecting("smith 2020");
        assert_eq!(
            check_selection(&fake, 42, &Selection::Unknown, fake.now() + SELECTION_LIMIT),
            Ok(())
        );
        assert!(fake.reads.borrow().is_empty());
    }

    #[test]
    fn controls_that_answer_oddly_are_not_failed_reads() {
        assert_eq!(selection_error(0), Reply::Value(()));
        assert_eq!(selection_error(-25212), Reply::NoValue);
        assert_eq!(selection_error(-25205), Reply::Unsupported);
        assert_eq!(selection_error(-25208), Reply::Unsupported, "not implemented");
        assert_eq!(selection_error(-25204), Reply::Failed, "cannot complete");
        assert_eq!(selection_error(-25202), Reply::Failed, "invalid element");
        assert_eq!(range_length(true, -1, 0), Reply::Value(0), "kCFNotFound with no length");
        assert_eq!(range_length(true, 3, 0), Reply::Value(0));
        assert_eq!(range_length(true, 3, 7), Reply::Value(7));
        assert_eq!(range_length(true, -1, 7), Reply::Failed);
        assert_eq!(range_length(true, 3, -1), Reply::Failed);
        assert_eq!(range_length(false, 3, 7), Reply::Failed);
    }

    #[test]
    fn a_range_that_says_text_is_selected_is_believed_when_the_text_is_withheld() {
        for reply in [Reply::NoValue, Reply::Unsupported, Reply::Value(String::new())] {
            let fake = Fake {
                selected_length: Cell::new(Some(5)),
                selected: RefCell::new(reply),
                ..Fake::new()
            };
            assert_eq!(fake.selection(), Ok((Selection::Ignored, None)));
        }
    }

    #[test]
    fn opening_never_fails_a_read_into_a_pass() {
        let opening = |fake: &Fake, allowed| {
            let (selection, text) = opening_selection(fake, 42, allowed, fake.now() + SELECTION_LIMIT);
            assert_eq!(text, None);
            selection
        };
        for fake in [
            Fake {
                selected: RefCell::new(Reply::Failed),
                ..Fake::new()
            },
            Fake {
                password: Reply::Value(true),
                ..Fake::selecting("hunter2")
            },
            Fake {
                focused_pid: 99,
                ..Fake::selecting("smith 2020")
            },
            Fake {
                selection_delay: SELECTION_LIMIT,
                ..Fake::selecting("smith 2020")
            },
        ] {
            assert_eq!(opening(&fake, true), Selection::Unread);
        }
        let secure = Fake::selecting("smith 2020");
        secure.secure_typing.set(Some(true));
        assert_eq!(opening(&secure, true), Selection::Unread);
        let refused = Fake::selecting("smith 2020");
        assert_eq!(opening(&refused, false), Selection::Unread);
        assert!(refused.selection_reads.borrow().is_empty());
        assert!(refused.reads.borrow().is_empty());
    }

    #[test]
    fn an_unread_opening_pastes_only_once_nothing_is_proved_selected() {
        let check = |fake: &Fake| check_selection(fake, 42, &Selection::Unread, fake.now() + FINAL_SELECTION_LIMIT);
        assert_eq!(check(&Fake::selecting("")), Ok(()));
        assert_eq!(
            check(&Fake::new()),
            Ok(()),
            "an app that cannot report pastes as before"
        );
        assert_eq!(
            check(&Fake::selecting("smith 2020")),
            Err(CopyReason::SelectionNotShown)
        );
        let paragraph = "Invented paragraph. ".repeat(20);
        assert_eq!(check(&Fake::selecting(&paragraph)), Err(CopyReason::SelectionNotShown));
        let still_failing = Fake {
            selected: RefCell::new(Reply::Failed),
            ..Fake::new()
        };
        assert_eq!(check(&still_failing), Err(CopyReason::PasteUnavailable));
        let password = Fake {
            password: Reply::Value(true),
            ..Fake::selecting("hunter2")
        };
        assert_eq!(check(&password), Err(CopyReason::SecureField));
        assert!(!password.selection_reads.borrow().contains(&"range"));
    }

    #[test]
    fn the_final_read_has_longer_than_the_opening_for_a_busy_app() {
        let fake = Fake {
            selection_delay: Duration::from_millis(40),
            selected: RefCell::new(Reply::NoValue),
            ..Fake::new()
        };
        assert_eq!(
            check_selection(&fake, 42, &Selection::Unread, fake.now() + FINAL_SELECTION_LIMIT),
            Ok(())
        );
        assert_eq!(
            opening_selection(&fake, 42, true, fake.now() + SELECTION_LIMIT),
            (Selection::Unread, None)
        );
    }

    #[test]
    fn secure_typing_at_the_final_proof_keeps_its_own_reason() {
        let fake = Fake::selecting("smith 2020");
        fake.secure_typing.set(Some(true));
        assert_eq!(
            check_selection(
                &fake,
                42,
                &Selection::of_text("smith 2020").0,
                fake.now() + FINAL_SELECTION_LIMIT
            ),
            Err(CopyReason::SecureInput)
        );
    }

    #[test]
    fn a_changed_citation_replaces_only_the_old_citation_proved_selected() {
        let old = OldCitation::of("(Smith, 2020, p. 12)");
        let check = |fake: &Fake| {
            check_selection(
                fake,
                42,
                &Selection::Citation(old.clone()),
                fake.now() + SELECTION_LIMIT,
            )
        };
        assert_eq!(check(&Fake::selecting("(Smith, 2020, p. 12)")), Ok(()));
        assert_eq!(check(&Fake::selecting("(Smith,\u{a0}2020, p.\u{a0}12)")), Ok(()));
        for fake in [
            Fake::selecting(""),
            Fake::selecting("(Jones, 2021)"),
            Fake::new(),
            Fake {
                selected: RefCell::new(Reply::Failed),
                ..Fake::new()
            },
            Fake {
                password: Reply::Value(true),
                ..Fake::selecting("(Smith, 2020, p. 12)")
            },
            Fake {
                selection_delay: FINAL_SELECTION_LIMIT,
                ..Fake::selecting("(Smith, 2020, p. 12)")
            },
        ] {
            assert_eq!(check(&fake), Err(CopyReason::CitationNotSelected));
        }
    }

    #[test]
    fn a_selection_much_longer_than_the_old_citation_is_never_copied_across() {
        let old = OldCitation::of("(Smith, 2020)");
        let fake = Fake::selecting(&"Invented paragraph. ".repeat(50));
        assert_eq!(
            check_selection(&fake, 42, &Selection::Citation(old), fake.now() + SELECTION_LIMIT),
            Err(CopyReason::CitationNotSelected)
        );
        assert!(!fake.selection_reads.borrow().contains(&"text"));
    }

    #[test]
    fn secure_typing_keeps_its_own_reason_when_replacing_a_citation_too() {
        let fake = Fake::selecting("(Smith, 2020)");
        fake.secure_typing.set(Some(true));
        assert_eq!(
            check_selection(
                &fake,
                42,
                &Selection::Citation(OldCitation::of("(Smith, 2020)")),
                fake.now() + SELECTION_LIMIT
            ),
            Err(CopyReason::SecureInput)
        );
        assert!(fake.reads.borrow().is_empty());
    }

    #[test]
    fn the_selection_read_cannot_touch_the_clipboard_or_post_a_key() {
        for source in [include_str!("focus.rs"), include_str!("focus/native.rs")] {
            let code = source.split("#[cfg(test)]").next().unwrap_or(source);
            for call in ["NSPasteboard", "CGEvent", "post_to_pid"] {
                assert!(!code.contains(call), "{call}");
            }
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
            fake.eligible_process.set(2); // A new eligibility decision retained this instance.
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
    fn a_reused_pid_or_terminated_eligible_instance_never_receives_the_ax_write() {
        for fake in [
            Fake {
                unsupported: Some(Attribute::FocusedWindow),
                process_after_missing: Some(2),
                ..Fake::new()
            },
            Fake {
                unsupported: Some(Attribute::TopLevel),
                original_terminated: true,
                ..Fake::new()
            },
            Fake {
                unsupported: Some(Attribute::FocusedWindow),
                process_after_prepare: Some(2),
                ..Fake::new()
            },
        ] {
            assert!(capture_query(&fake, 42, true).is_err());
            assert_eq!(
                fake.manual_calls.get(),
                0,
                "eligibility belongs to the retained instance, not PID 42"
            );
        }
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

    #[test]
    fn a_web_password_field_whose_window_cannot_be_proved_says_it_hides_typing() {
        // Web content can lack a top-level surface; the paste is refused
        // either way, and the words name the field rather than nothing.
        let field = Fake {
            unsupported: Some(Attribute::TopLevel),
            password: Reply::Value(true),
            secure_typing: Cell::new(Some(true)),
            ..Fake::new()
        };
        assert!(matches!(
            capture_reason(&field, 42, false),
            Err(CopyReason::SecureField)
        ));
        assert_eq!(
            *field.selection_reads.borrow(),
            ["subrole"],
            "never the range or the text"
        );
        let typing = Fake {
            unsupported: Some(Attribute::TopLevel),
            secure_typing: Cell::new(Some(true)),
            ..Fake::new()
        };
        assert!(matches!(
            capture_reason(&typing, 42, false),
            Err(CopyReason::SecureInput)
        ));
        let neither = Fake {
            unsupported: Some(Attribute::TopLevel),
            ..Fake::new()
        };
        assert!(matches!(
            capture_reason(&neither, 42, false),
            Err(CopyReason::PasteUnavailable)
        ));
    }

    #[test]
    fn only_an_unproved_or_secure_refusal_is_reworded_and_only_by_the_targets_own_field() {
        let foreign = Fake {
            focused_pid: 99,
            password: Reply::Value(true),
            ..Fake::new()
        };
        assert_eq!(
            refused_because(
                &foreign,
                42,
                CopyReason::PasteUnavailable,
                foreign.now() + SELECTION_LIMIT
            ),
            CopyReason::PasteUnavailable
        );
        let field = Fake {
            password: Reply::Value(true),
            ..Fake::new()
        };
        for kept in [
            CopyReason::TargetChanged,
            CopyReason::AccessibilityRequired,
            CopyReason::ClipboardChanged,
        ] {
            assert_eq!(refused_because(&field, 42, kept, field.now() + SELECTION_LIMIT), kept);
        }
        assert!(field.selection_reads.borrow().is_empty());
        assert_eq!(
            refused_because(&field, 42, CopyReason::SecureInput, field.now() + SELECTION_LIMIT),
            CopyReason::SecureField
        );
        let revoked = Fake {
            password: Reply::Value(true),
            trusted: Cell::new(false),
            ..Fake::new()
        };
        assert_eq!(
            refused_because(
                &revoked,
                42,
                CopyReason::PasteUnavailable,
                revoked.now() + SELECTION_LIMIT
            ),
            CopyReason::PasteUnavailable
        );
    }
}
