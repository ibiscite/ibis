//! Enter pastes into the app the picker was opened from (#27, #1241).
//!
//! After a clean copy has hidden the picker, Ibis posts one ⌘V (macOS) or
//! Ctrl+V (Windows, #1133), only to the captured process, once that app is in
//! front again and the clipboard still holds the citation.
//! `ibis_core::destination::decide` says when it must only copy, and the
//! notice says why. The citation stays on the clipboard either way (ADR 0009).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use ibis_core::destination::{AppId, CopyReason, Decision, PastePermission, System, decide, lookup};
use serde::Serialize;
use tauri::{AppHandle, Manager};

mod capture;
#[cfg(target_os = "macos")]
mod macos;
pub(crate) mod permission;
#[cfg(target_os = "windows")]
mod windows;

use capture::{Captured, Front};

const SYSTEM: System = if cfg!(target_os = "macos") {
    System::Macos
} else if cfg!(target_os = "windows") {
    System::Windows
} else {
    System::Linux
};

/// How long the captured app gets to come back to the front after the picker
/// hides, in checks and in real time; a paste later than `LATE` never happens.
const RETURN_LIMIT: Duration = Duration::from_millis(1_000);
const RETURN_STEP: Duration = Duration::from_millis(25);
const RETURN_CHECKS: u32 = 40;
const LATE: Duration = Duration::from_millis(1_500);
/// Time for the app to give its window the keyboard again.
const SETTLE: Duration = Duration::from_millis(50);

/// The app the picker was last opened from.
#[derive(Default)]
pub(crate) struct LastTarget(Mutex<LastCapture>);

#[derive(Default)]
struct LastCapture {
    #[cfg(not(target_os = "windows"))]
    captured: Option<Captured>,
    #[cfg(target_os = "windows")]
    target: Option<CopyTarget<windows::Window>>,
    #[cfg(target_os = "windows")]
    pending: Option<DeliveryTicket<windows::Window>>,
}

/// Remember the app in front, before the picker shows. Ibis in front keeps the earlier capture.
pub(crate) fn capture(app: &AppHandle) {
    let own = i32::try_from(std::process::id()).unwrap_or(i32::MAX);
    #[cfg(target_os = "macos")]
    let front = macos::frontmost();
    #[cfg(target_os = "windows")]
    let generation = app.state::<crate::picker::PickerGeneration>().current();
    #[cfg(target_os = "windows")]
    let (front, window) = windows::capture();
    #[cfg(target_os = "linux")]
    let front = None;
    let captured = capture::captured(SYSTEM, front, own);
    if let Ok(mut last) = app.state::<LastTarget>().0.lock() {
        #[cfg(not(target_os = "windows"))]
        if let Some(captured) = captured {
            last.captured = Some(captured);
        }
        #[cfg(target_os = "windows")]
        {
            // A newer show may finish its capture while this one's optional
            // name is waiting. Never stamp an older HWND with its generation.
            publish_capture(
                &mut last.target,
                CopyTarget {
                    captured,
                    window,
                    generation,
                },
                app.state::<crate::picker::PickerGeneration>().current(),
            );
        }
    }
}

/// The app the picker was last opened from, with the name the picker shows
/// for it: the app's own, else the destination table's, else its ID.
pub(crate) fn last_app(app: &AppHandle) -> Option<(AppId, String)> {
    let state = app.state::<LastTarget>();
    let last = state.0.lock().ok()?;
    #[cfg(not(target_os = "windows"))]
    let captured = last.captured.clone()?;
    #[cfg(target_os = "windows")]
    let captured = last.target.as_ref()?.captured.clone()?;
    let id = captured.target.app?;
    let name = captured
        .name
        .or_else(|| lookup(&id).map(|known| known.name.to_owned()))
        .unwrap_or_else(|| id.id.clone());
    Some((id, name))
}

/// What happened to a copied citation, for the notice to word.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub(crate) enum Delivered {
    Pasted {
        /// The app's own name; `None` when it has none.
        app: Option<String>,
    },
    Copied {
        /// Why Ibis did not paste, when the person can do something about it
        /// or would expect a paste: a `CopyReason` code such as `terminal`.
        reason: Option<String>,
    },
}

/// The system calls a paste needs, so the decision is testable without them.
pub(crate) trait Desk {
    /// Finish releasing keys from an earlier partial injection, even if this
    /// delivery will only copy. Other platforms have no pending releases.
    fn release_keys(&self) -> bool {
        true
    }
    fn keys_unreleased(&self) -> bool {
        false
    }
    /// Ibis may post key events (macOS Accessibility; always on Windows).
    fn may_post(&self) -> bool;
    /// Show where to give that permission; at most once per run.
    fn ask_to_post(&self);
    /// Restore a proven target before waiting for it to come to the front.
    fn prepare(&self) -> bool {
        true
    }
    /// The app in front, or `None` once the picker has been shown again.
    fn frontmost(&self) -> Option<Front>;
    fn clipboard_matches(&self, text: &str) -> bool;
    /// The focused field of the app in front takes a password (Windows UI
    /// Automation). macOS does not check yet.
    /// `None` means the check failed or timed out: it has not proved a safe field.
    fn secure_field(&self, pid: i32) -> Option<bool>;
    /// The key that types V with Command on the current layout; Windows' V key.
    fn paste_key(&self) -> Option<u16>;
    /// Post ⌘ or Ctrl and `key` down and up to `pid` only; false on failure.
    fn post(&self, pid: i32, key: u16) -> bool;
    /// Windows rechecks this after its injection lock and recovery waits.
    /// Other platforms retain their existing post behavior.
    fn post_before(&self, pid: i32, key: u16, _deadline: Instant) -> bool {
        self.post(pid, key)
    }
    fn wait(&self, time: Duration);
}

/// Paste `text`, just copied, into the captured app, or say why it stays copied.
pub(crate) fn deliver(desk: &impl Desk, captured: Option<&Captured>, text: &str, enabled: bool) -> Delivered {
    if !desk.release_keys() {
        return key_release_failed();
    }
    let unknown = Captured::unknown(SYSTEM);
    let captured = captured.unwrap_or(&unknown);
    let permission = PastePermission {
        enabled,
        granted: enabled && desk.may_post(),
    };
    let copied = |reason: Option<CopyReason>| Delivered::Copied {
        reason: reason
            .and_then(|reason| serde_json::to_value(reason).ok())
            .and_then(|code| code.as_str().map(str::to_owned)),
    };
    match decide(&captured.target, permission) {
        Decision::Copy(CopyReason::NoPermission) => {
            desk.ask_to_post();
            copied(Some(CopyReason::NoPermission))
        }
        // Nothing the person chose or would expect a paste for.
        Decision::Copy(CopyReason::PasteOff | CopyReason::UnsupportedSystem | CopyReason::NoTarget) => copied(None),
        Decision::Copy(reason) => copied(Some(reason)),
        Decision::Paste => {
            if !desk.prepare() {
                return copied(None);
            }
            let Some(pid) = captured.pid else {
                return copied(None);
            };
            // The same process and app: a process number alone can be reused.
            let id = captured.target.app.as_ref().map(|app| app.id.as_str());
            let back = |front: Option<Front>| front.is_some_and(|front| front.pid == pid && front.id.as_deref() == id);
            let started = Instant::now();
            let mut checks = 0;
            while !back(desk.frontmost()) {
                if checks >= RETURN_CHECKS || started.elapsed() >= RETURN_LIMIT {
                    return copied(None);
                }
                desk.wait(RETURN_STEP);
                checks += 1;
            }
            desk.wait(SETTLE);
            match desk.secure_field(pid) {
                Some(false) => {}
                Some(true) => return copied(Some(CopyReason::SecureField)),
                None => return copied(None),
            }
            let key = desk.paste_key();
            // Another app may have taken the clipboard since the copy, and a
            // late paste would land in whatever the person is typing now.
            if !desk.clipboard_matches(text) || started.elapsed() > LATE {
                return copied(None);
            }
            match key {
                Some(key) if desk.post_before(pid, key, started + LATE) => Delivered::Pasted {
                    app: captured.name.clone(),
                },
                _ if desk.keys_unreleased() => key_release_failed(),
                _ => copied(None),
            }
        }
    }
}

fn key_release_failed() -> Delivered {
    Delivered::Copied {
        reason: Some("key-release-failed".into()),
    }
}

/// A Windows copy freezes its destination before writing, then adds the
/// sequence sampled while that write still has the clipboard open. Neither
/// the notice nor its worker may adopt a later capture.
#[cfg(any(target_os = "windows", test))]
#[derive(Clone)]
pub(crate) struct CopyTarget<W> {
    captured: Option<Captured>,
    window: Option<W>,
    generation: u64,
}

/// Publish a complete capture only for the presentation that requested it.
/// Ibis in front keeps the previous app/window under the new generation.
#[cfg(any(target_os = "windows", test))]
fn publish_capture<W>(kept: &mut Option<CopyTarget<W>>, next: CopyTarget<W>, current: u64) {
    if next.generation != current {
        return;
    }
    let target = match (next.captured.is_none(), kept.take()) {
        (true, Some(previous)) => CopyTarget {
            generation: next.generation,
            ..previous
        },
        _ => next,
    };
    *kept = Some(target);
}

#[cfg(any(target_os = "windows", test))]
pub(crate) struct DeliveryTicket<W> {
    target: CopyTarget<W>,
    sequence: u32,
}

#[cfg(target_os = "windows")]
pub(crate) fn copy_target(app: &AppHandle) -> Option<CopyTarget<windows::Window>> {
    let state = app.state::<LastTarget>();
    let last = state.0.lock().ok()?;
    last.target.clone()
}

#[cfg(target_os = "windows")]
pub(crate) fn remember_copy(
    app: &AppHandle,
    target: Option<CopyTarget<windows::Window>>,
    sequence: Option<u32>,
) -> Option<u32> {
    let state = app.state::<LastTarget>();
    let mut last = state.0.lock().ok()?;
    last.pending = target
        .zip(sequence)
        .map(|(target, sequence)| DeliveryTicket { target, sequence });
    last.pending.as_ref().map(|ticket| ticket.sequence)
}

/// A stale notice must leave a newer copy's ticket alone.
#[cfg(any(target_os = "windows", test))]
fn take_ticket<W>(pending: &mut Option<DeliveryTicket<W>>, receipt: Option<u32>) -> Option<DeliveryTicket<W>> {
    if pending.as_ref().map(|ticket| ticket.sequence) == receipt {
        pending.take()
    } else {
        None
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn take_copy(app: &AppHandle, receipt: Option<u32>) -> Option<DeliveryTicket<windows::Window>> {
    take_ticket(&mut app.state::<LastTarget>().0.lock().ok()?.pending, receipt)
}

/// Windows checks that can be exercised with fake HWNDs on every host.
#[cfg(any(target_os = "windows", test))]
trait TicketDesk: Desk {
    type Window;
    fn generation(&self) -> u64;
    fn picker_hidden(&self) -> bool;
    fn restore(&self, window: &Self::Window, pid: i32) -> bool;
    fn window_in_front(&self, window: &Self::Window) -> bool;
    fn clipboard_sequence(&self) -> Option<u32>;
    /// Check the ticket after any injection lock/recovery wait, immediately
    /// before sending fresh key-downs. Releasing old keys needs no ticket.
    fn post_if(&self, pid: i32, key: u16, ready: impl Fn() -> bool) -> bool;
}

#[cfg(any(target_os = "windows", test))]
struct TicketDeskGuard<'a, D: TicketDesk> {
    desk: &'a D,
    ticket: &'a DeliveryTicket<D::Window>,
}

#[cfg(any(target_os = "windows", test))]
impl<D: TicketDesk> TicketDeskGuard<'_, D> {
    fn current(&self) -> bool {
        // Copy precedes exactly one dismiss_picker, which advances the
        // generation. Any additional show/dismiss cancels this delivery.
        self.desk.generation() == self.ticket.target.generation.wrapping_add(1)
            && self.desk.picker_hidden()
            && self.desk.clipboard_sequence() == Some(self.ticket.sequence)
    }

    fn in_front(&self) -> bool {
        self.ticket
            .target
            .window
            .as_ref()
            .is_some_and(|window| self.desk.window_in_front(window))
    }
}

#[cfg(any(target_os = "windows", test))]
impl<D: TicketDesk> Desk for TicketDeskGuard<'_, D> {
    fn release_keys(&self) -> bool {
        self.desk.release_keys()
    }
    fn keys_unreleased(&self) -> bool {
        self.desk.keys_unreleased()
    }
    fn may_post(&self) -> bool {
        self.desk.may_post()
    }
    fn ask_to_post(&self) {
        self.desk.ask_to_post();
    }
    fn prepare(&self) -> bool {
        self.current()
            && self
                .ticket
                .target
                .window
                .as_ref()
                .zip(self.ticket.target.captured.as_ref().and_then(|c| c.pid))
                .is_some_and(|(window, pid)| self.desk.restore(window, pid))
    }
    fn frontmost(&self) -> Option<Front> {
        (self.current() && self.in_front())
            .then(|| self.desk.frontmost())
            .flatten()
    }
    fn clipboard_matches(&self, _: &str) -> bool {
        // No GetClipboardData/read_text: even a read following a sequence
        // check could race with a foreign delayed-rendering owner.
        self.current()
    }
    fn secure_field(&self, pid: i32) -> Option<bool> {
        if !self.current() || !self.in_front() {
            return None;
        }
        let secure = self.desk.secure_field(pid);
        (self.current() && self.in_front()).then_some(secure).flatten()
    }
    fn paste_key(&self) -> Option<u16> {
        self.desk.paste_key()
    }
    fn post(&self, pid: i32, key: u16) -> bool {
        self.desk.post_if(pid, key, || self.current() && self.in_front())
    }
    fn post_before(&self, pid: i32, key: u16, deadline: Instant) -> bool {
        self.desk.post_if(pid, key, || {
            self.current() && self.in_front() && Instant::now() <= deadline
        })
    }
    fn wait(&self, time: Duration) {
        self.desk.wait(time);
    }
}

#[cfg(any(target_os = "windows", test))]
fn deliver_ticket<D: TicketDesk>(desk: &D, ticket: &DeliveryTicket<D::Window>, text: &str, enabled: bool) -> Delivered {
    deliver(
        &TicketDeskGuard { desk, ticket },
        ticket.target.captured.as_ref(),
        text,
        enabled,
    )
}

#[cfg(target_os = "windows")]
pub(crate) fn deliver_copy(app: &AppHandle, text: &str, ticket: Option<DeliveryTicket<windows::Window>>) -> Delivered {
    let desk = windows::Win::new(app);
    // Missing/stale receipts must not strand keys from an earlier copy.
    if !desk.release_keys() {
        return key_release_failed();
    }
    let Some(ticket) = ticket else {
        return Delivered::Copied { reason: None };
    };
    let enabled = ibis_core::settings::load(&app.state::<crate::preferences::SettingsFile>().path)
        .settings
        .paste
        .unwrap_or(true);
    deliver_ticket(&desk, &ticket, text, enabled)
}

#[cfg(any(target_os = "windows", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PasteEvent {
    ControlDown,
    VDown,
    VUp,
    ControlUp,
}

/// Keys left down by Ibis's own injection. Windows keeps one process-wide
/// instance under a mutex, serializing injections and retaining failed key-ups.
#[cfg(any(target_os = "windows", test))]
#[derive(Default)]
struct PasteKeys {
    control: bool,
    v: bool,
}

#[cfg(any(target_os = "windows", test))]
impl PasteKeys {
    fn pending(&self) -> bool {
        self.control || self.v
    }

    fn release(&mut self, send: impl Fn(&[PasteEvent]) -> u32, wait: impl Fn(Duration), warn: impl Fn()) -> bool {
        for attempt in 0..3 {
            if self.v && send(&[PasteEvent::VUp]) == 1 {
                self.v = false;
            }
            if self.control && send(&[PasteEvent::ControlUp]) == 1 {
                self.control = false;
            }
            if !self.pending() {
                return true;
            }
            if attempt < 2 {
                wait(Duration::from_millis(10));
            }
        }
        warn();
        false
    }

    fn paste(
        &mut self,
        send: impl Fn(&[PasteEvent]) -> u32,
        wait: impl Fn(Duration),
        warn: impl Fn(),
        ready: impl Fn() -> bool,
    ) -> bool {
        if !self.release(&send, &wait, &warn) || !ready() {
            return false;
        }
        use PasteEvent::*;
        match send(&[ControlDown, VDown, VUp, ControlUp]) {
            4 => true,
            count @ 1..=3 => {
                self.control = true;
                self.v = count == 2;
                self.release(send, wait, warn);
                false
            }
            _ => false,
        }
    }
}

/// One in-flight native query, with no queue. A timed-out worker retains its
/// permit until it actually exits; busy callers immediately use their fallback.
#[cfg(any(target_os = "windows", test))]
#[derive(Default)]
struct QueryWorker(std::sync::Arc<std::sync::atomic::AtomicBool>);

#[cfg(any(target_os = "windows", test))]
struct QueryPermit(std::sync::Arc<std::sync::atomic::AtomicBool>);

#[cfg(any(target_os = "windows", test))]
impl Drop for QueryPermit {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Release);
    }
}

#[cfg(any(target_os = "windows", test))]
impl QueryWorker {
    fn run<T: Send + 'static>(&self, limit: Duration, query: impl FnOnce() -> T + Send + 'static) -> Option<T> {
        self.0
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::Acquire,
                std::sync::atomic::Ordering::Relaxed,
            )
            .ok()?;
        run_query(QueryPermit(self.0.clone()), limit, query)
    }
}

/// A stuck provider blocks only its target process. No queue or idle entries;
/// at most four native queries may remain outstanding across all processes.
#[cfg(any(target_os = "windows", test))]
#[derive(Default)]
struct PasswordQueries(std::sync::Arc<Mutex<Vec<i32>>>);

#[cfg(any(target_os = "windows", test))]
const PASSWORD_QUERY_LIMIT: usize = 4;

#[cfg(any(target_os = "windows", test))]
struct PasswordPermit {
    active: std::sync::Arc<Mutex<Vec<i32>>>,
    pid: i32,
}

#[cfg(any(target_os = "windows", test))]
impl Drop for PasswordPermit {
    fn drop(&mut self) {
        if let Ok(mut active) = self.active.lock() {
            active.retain(|pid| *pid != self.pid);
        }
    }
}

#[cfg(any(target_os = "windows", test))]
impl PasswordQueries {
    fn run<T: Send + 'static>(
        &self,
        pid: i32,
        limit: Duration,
        query: impl FnOnce() -> T + Send + 'static,
    ) -> Option<T> {
        let mut active = self.0.lock().ok()?;
        if active.len() >= PASSWORD_QUERY_LIMIT || active.contains(&pid) {
            return None;
        }
        active.push(pid);
        drop(active);
        run_query(
            PasswordPermit {
                active: self.0.clone(),
                pid,
            },
            limit,
            query,
        )
    }
}

#[cfg(any(target_os = "windows", test))]
fn run_query<T: Send + 'static>(
    permit: impl Send + 'static,
    limit: Duration,
    query: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .spawn(move || {
            let _permit = permit;
            let _ = sender.send(query());
        })
        .ok()?;
    receiver.recv_timeout(limit).ok()
}

#[cfg(any(target_os = "windows", test))]
fn display_name(
    worker: &QueryWorker,
    executable: String,
    query: impl FnOnce() -> Option<String> + Send + 'static,
) -> String {
    worker
        .run(Duration::from_millis(50), query)
        .flatten()
        .unwrap_or(executable)
}

/// Paste into the captured app when it is safe to; the notice then says what happened.
#[cfg(not(target_os = "windows"))]
pub(crate) fn deliver_copy(app: &AppHandle, text: &str) -> Delivered {
    let captured = app
        .state::<LastTarget>()
        .0
        .lock()
        .ok()
        .and_then(|last| last.captured.clone());
    #[cfg(target_os = "macos")]
    let desk = macos::Mac::new(app);
    #[cfg(target_os = "linux")]
    let desk = CopyOnly;
    let enabled = ibis_core::settings::load(&app.state::<crate::preferences::SettingsFile>().path)
        .settings
        .paste
        .unwrap_or(true);
    deliver(&desk, captured.as_ref(), text, enabled)
}

/// Linux captures no target, so it only copies.
#[cfg(target_os = "linux")]
struct CopyOnly;

#[cfg(target_os = "linux")]
impl Desk for CopyOnly {
    fn may_post(&self) -> bool {
        false
    }
    fn ask_to_post(&self) {}
    fn frontmost(&self) -> Option<Front> {
        None
    }
    fn clipboard_matches(&self, _: &str) -> bool {
        false
    }
    fn secure_field(&self, _: i32) -> Option<bool> {
        Some(false)
    }
    fn paste_key(&self) -> Option<u16> {
        None
    }
    fn post(&self, _: i32, _: u16) -> bool {
        false
    }
    fn wait(&self, _: Duration) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::destination::{AppId, Target};
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;

    struct Fake {
        may_post: bool,
        asked: Cell<u32>,
        /// Front-most process after each wait.
        fronts: RefCell<Vec<i32>>,
        front_id: &'static str,
        clipboard: String,
        clipboard_reads: Cell<u32>,
        password: Option<bool>,
        password_queries: Option<std::sync::Arc<PasswordQueries>>,
        after_password: Option<fn(&Fake)>,
        posts: RefCell<Vec<(i32, u16)>>,
        generation: Cell<u64>,
        hidden: Cell<bool>,
        window: Cell<usize>,
        sequence: Cell<Option<u32>>,
        restores: RefCell<Vec<(usize, i32)>>,
        input_results: RefCell<VecDeque<u32>>,
        input_requests: RefCell<Vec<Vec<PasteEvent>>>,
        keys: RefCell<PasteKeys>,
        recovery_waits: RefCell<Vec<Duration>>,
        release_warnings: Cell<u32>,
        before_post: Option<fn(&Fake)>,
        after_recovery_wait: Option<fn(&Fake)>,
        events: RefCell<Vec<PasteEvent>>,
        control_down: Cell<bool>,
        v_down: Cell<bool>,
    }

    impl Fake {
        fn new(clipboard: &str) -> Self {
            Self {
                may_post: true,
                asked: Cell::new(0),
                fronts: RefCell::new(vec![42]),
                front_id: "org.example.writer",
                clipboard: clipboard.into(),
                clipboard_reads: Cell::new(0),
                password: Some(false),
                password_queries: None,
                after_password: None,
                posts: RefCell::default(),
                generation: Cell::new(11),
                hidden: Cell::new(true),
                window: Cell::new(100),
                sequence: Cell::new(Some(17)),
                restores: RefCell::default(),
                input_results: RefCell::default(),
                input_requests: RefCell::default(),
                keys: RefCell::default(),
                recovery_waits: RefCell::default(),
                release_warnings: Cell::new(0),
                before_post: None,
                after_recovery_wait: None,
                events: RefCell::default(),
                control_down: Cell::new(false),
                v_down: Cell::new(false),
            }
        }

        fn input(&self, events: &[PasteEvent]) -> u32 {
            self.input_requests.borrow_mut().push(events.to_vec());
            let count = self
                .input_results
                .borrow_mut()
                .pop_front()
                .unwrap_or(events.len() as u32);
            assert!(count as usize <= events.len());
            for event in events.iter().take(count as usize) {
                self.events.borrow_mut().push(*event);
                match event {
                    PasteEvent::ControlDown => self.control_down.set(true),
                    PasteEvent::ControlUp => self.control_down.set(false),
                    PasteEvent::VDown => self.v_down.set(true),
                    PasteEvent::VUp => self.v_down.set(false),
                }
            }
            count
        }

        fn release_warning(&self) {
            self.release_warnings.set(self.release_warnings.get() + 1);
        }

        fn recovery_wait(&self, time: Duration) {
            self.recovery_waits.borrow_mut().push(time);
            if let Some(change) = self.after_recovery_wait {
                change(self);
            }
        }

        fn post_when_ready(&self, pid: i32, key: u16, ready: impl Fn() -> bool) -> bool {
            self.posts.borrow_mut().push((pid, key));
            if let Some(change) = self.before_post {
                change(self);
            }
            self.keys.borrow_mut().paste(
                |events| self.input(events),
                |time| self.recovery_wait(time),
                || self.release_warning(),
                ready,
            )
        }
    }

    impl Desk for Fake {
        fn release_keys(&self) -> bool {
            self.keys.borrow_mut().release(
                |events| self.input(events),
                |time| self.recovery_wait(time),
                || self.release_warning(),
            )
        }
        fn keys_unreleased(&self) -> bool {
            self.keys.borrow().pending()
        }
        fn may_post(&self) -> bool {
            self.may_post
        }
        fn ask_to_post(&self) {
            self.asked.set(self.asked.get() + 1);
        }
        fn frontmost(&self) -> Option<Front> {
            self.fronts.borrow().first().map(|&pid| Front {
                id: Some(self.front_id.into()),
                pid,
                name: None,
                elevated: false,
            })
        }
        fn clipboard_matches(&self, text: &str) -> bool {
            self.clipboard_reads.set(self.clipboard_reads.get() + 1);
            self.clipboard == text
        }
        fn secure_field(&self, pid: i32) -> Option<bool> {
            if let Some(change) = self.after_password {
                change(self);
            }
            let password = self.password;
            match &self.password_queries {
                Some(queries) => queries.run(pid, Duration::from_secs(1), move || password).flatten(),
                None => password,
            }
        }
        fn paste_key(&self) -> Option<u16> {
            Some(0x09)
        }
        fn post(&self, pid: i32, key: u16) -> bool {
            self.post_when_ready(pid, key, || true)
        }
        fn wait(&self, _: Duration) {
            let mut fronts = self.fronts.borrow_mut();
            if fronts.len() > 1 {
                fronts.remove(0);
            }
        }
    }

    impl TicketDesk for Fake {
        type Window = usize;
        fn generation(&self) -> u64 {
            self.generation.get()
        }
        fn picker_hidden(&self) -> bool {
            self.hidden.get()
        }
        fn restore(&self, window: &usize, pid: i32) -> bool {
            self.restores.borrow_mut().push((*window, pid));
            true
        }
        fn window_in_front(&self, window: &usize) -> bool {
            self.window.get() == *window
        }
        fn clipboard_sequence(&self) -> Option<u32> {
            self.sequence.get()
        }
        fn post_if(&self, pid: i32, key: u16, ready: impl Fn() -> bool) -> bool {
            self.post_when_ready(pid, key, ready)
        }
    }

    fn ticket() -> DeliveryTicket<usize> {
        let mut captured = writer("org.example.writer");
        // Exercise Windows policy even on a Linux test host.
        captured.target.system = System::Windows;
        if let Some(app) = &mut captured.target.app {
            app.system = System::Windows;
        }
        DeliveryTicket {
            target: CopyTarget {
                captured: Some(captured),
                window: Some(100),
                generation: 10,
            },
            sequence: 17,
        }
    }

    fn writer(id: &str) -> Captured {
        Captured {
            target: Target {
                system: SYSTEM,
                app: Some(AppId {
                    system: SYSTEM,
                    id: id.into(),
                }),
                secure_field: false,
                elevated: false,
            },
            pid: Some(42),
            name: Some("Writer".into()),
        }
    }

    fn copied(reason: Option<&str>) -> Delivered {
        Delivered::Copied {
            reason: reason.map(str::to_owned),
        }
    }

    #[test]
    #[cfg_attr(target_os = "linux", ignore = "Linux never pastes")]
    fn enter_posts_exactly_one_paste_to_the_captured_app_once_it_is_back_in_front() {
        let desk = Fake::new("[@synthetic]");
        // The picker is still in front for two checks.
        desk.fronts.replace(vec![7, 7, 42]);
        let delivered = deliver(&desk, Some(&writer("org.example.writer")), "[@synthetic]", true);
        assert_eq!(
            delivered,
            Delivered::Pasted {
                app: Some("Writer".into())
            }
        );
        assert_eq!(*desk.posts.borrow(), [(42, 0x09)]);
    }

    #[test]
    fn nothing_is_posted_when_the_paste_is_unsafe_or_unproven() {
        let cases: [(Fake, Captured, bool, Delivered); 5] = [
            // The process number now belongs to another app.
            (
                Fake::new("[@synthetic]"),
                writer("org.example.other"),
                true,
                copied(None),
            ),
            // Another app replaced the clipboard.
            (
                Fake::new("other text"),
                writer("org.example.writer"),
                true,
                copied(None),
            ),
            // The switch is off.
            (
                Fake::new("[@synthetic]"),
                writer("org.example.writer"),
                false,
                copied(None),
            ),
            // The captured app never came back.
            (
                Fake {
                    fronts: RefCell::new(vec![7]),
                    ..Fake::new("[@synthetic]")
                },
                writer("org.example.writer"),
                true,
                copied(None),
            ),
            // Nothing was in front.
            (Fake::new("[@synthetic]"), Captured::unknown(SYSTEM), true, copied(None)),
        ];
        for (desk, captured, enabled, expected) in cases {
            assert_eq!(deliver(&desk, Some(&captured), "[@synthetic]", enabled), expected);
            assert!(desk.posts.borrow().is_empty());
        }
    }

    #[test]
    #[cfg_attr(target_os = "linux", ignore = "Linux never pastes")]
    fn password_fields_and_apps_with_more_rights_copy_with_a_reason() {
        let desk = Fake {
            password: Some(true),
            ..Fake::new("[@synthetic]")
        };
        assert_eq!(
            deliver(&desk, Some(&writer("org.example.writer")), "[@synthetic]", true),
            copied(Some("secure-field"))
        );
        assert!(desk.posts.borrow().is_empty());

        let desk = Fake::new("[@synthetic]");
        let mut admin = writer("org.example.writer");
        admin.target.elevated = true;
        assert_eq!(
            deliver(&desk, Some(&admin), "[@synthetic]", true),
            copied(Some("elevated-app"))
        );
        assert!(desk.posts.borrow().is_empty());
    }

    #[test]
    fn a_delayed_copy_cannot_adopt_the_target_of_a_later_open_then_dismiss() {
        let desk = Fake::new("[@synthetic]");
        let mut pending = Some(ticket());
        // Claim the successful A copy before scheduling the worker.
        let frozen = take_ticket(&mut pending, Some(17)).unwrap_or_else(|| panic!("copy ticket"));
        // Open from B, then dismiss, while A's citation stays on the clipboard.
        desk.generation.set(12);
        desk.hidden.set(false);
        desk.window.set(200);
        desk.fronts.replace(vec![84]);
        desk.generation.set(13);
        desk.hidden.set(true);
        assert_eq!(deliver_ticket(&desk, &frozen, "[@synthetic]", true), copied(None));
        assert!(desk.restores.borrow().is_empty());
        assert!(desk.posts.borrow().is_empty());
        assert_eq!(desk.clipboard_reads.get(), 0);
        assert_eq!(frozen.target.window, Some(100));
        assert_eq!(frozen.target.captured.and_then(|c| c.pid), Some(42));
    }

    #[test]
    fn a_delayed_capture_cannot_overwrite_or_adopt_the_generation_of_a_newer_capture() {
        let delayed_a = ticket().target;
        let mut app_b = writer("org.example.other");
        app_b.pid = Some(84);
        let mut kept = None;
        // B finishes while A's executable name is still being looked up.
        publish_capture(
            &mut kept,
            CopyTarget {
                captured: Some(app_b.clone()),
                window: Some(200),
                generation: 11,
            },
            11,
        );
        publish_capture(&mut kept, delayed_a, 11);
        let b = kept.as_ref().unwrap_or_else(|| panic!("B capture"));
        assert_eq!(b.captured.as_ref(), Some(&app_b));
        assert_eq!(b.window, Some(200));
        assert_eq!(b.generation, 11);
        // Showing again with Ibis in front retains B, with that show's epoch.
        publish_capture(
            &mut kept,
            CopyTarget {
                captured: None,
                window: Some(7),
                generation: 12,
            },
            12,
        );
        let retained = kept.unwrap_or_else(|| panic!("retained B"));
        assert_eq!(retained.captured, Some(app_b));
        assert_eq!(retained.window, Some(200));
        assert_eq!(retained.generation, 12);
    }

    #[test]
    fn a_notice_can_consume_only_its_own_copy_receipt_once() {
        let mut pending = Some(DeliveryTicket {
            sequence: 18,
            ..ticket()
        });
        assert!(take_ticket(&mut pending, Some(17)).is_none());
        assert!(take_ticket(&mut pending, None).is_none());
        assert!(take_ticket(&mut pending, Some(18)).is_some());
        assert!(take_ticket(&mut pending, Some(18)).is_none());
    }

    #[test]
    fn a_foreign_clipboard_owner_is_never_read_or_adopted_as_the_baseline() {
        for sequence in [Some(18), None] {
            let desk = Fake::new("foreign delayed text");
            let frozen = ticket(); // Sequence 17, sampled during Ibis's write.
            // Replacement occurs before the worker even starts.
            desk.sequence.set(sequence);
            assert_eq!(deliver_ticket(&desk, &frozen, "[@synthetic]", true), copied(None));
            assert_eq!(desk.clipboard_reads.get(), 0);
            assert!(desk.restores.borrow().is_empty());
            assert!(desk.posts.borrow().is_empty());
        }
        let desk = Fake::new("contents must not be requested even for our sequence");
        assert!(matches!(
            deliver_ticket(&desk, &ticket(), "[@synthetic]", true),
            Delivered::Pasted { .. }
        ));
        assert_eq!(desk.clipboard_reads.get(), 0);
        assert_eq!(*desk.restores.borrow(), [(100, 42)]);
    }

    #[test]
    fn a_ticket_requires_its_window_and_the_single_copy_dismissal() {
        for (generation, hidden, window) in [(10, true, 100), (11, false, 100), (11, true, 200)] {
            let desk = Fake::new("[@synthetic]");
            desk.generation.set(generation);
            desk.hidden.set(hidden);
            desk.window.set(window);
            assert_eq!(deliver_ticket(&desk, &ticket(), "[@synthetic]", true), copied(None));
            assert!(desk.posts.borrow().is_empty());
        }
    }

    #[test]
    fn partial_send_input_releases_every_key_left_down_by_the_inserted_prefix() {
        use PasteEvent::*;
        for (inserted, expected) in [
            (0, vec![]),
            (1, vec![ControlDown, ControlUp]),
            (2, vec![ControlDown, VDown, VUp, ControlUp]),
            (3, vec![ControlDown, VDown, VUp, ControlUp]),
            (4, vec![ControlDown, VDown, VUp, ControlUp]),
        ] {
            let desk = Fake::new("[@synthetic]");
            desk.input_results.borrow_mut().push_back(inserted);
            let delivered = deliver_ticket(&desk, &ticket(), "[@synthetic]", true);
            if inserted == 4 {
                assert!(matches!(delivered, Delivered::Pasted { .. }));
            } else {
                assert_eq!(delivered, copied(None), "prefix {inserted}");
            }
            assert_eq!(*desk.events.borrow(), expected, "prefix {inserted}");
            assert!(!desk.control_down.get(), "Control left down by prefix {inserted}");
            assert!(!desk.v_down.get(), "V left down by prefix {inserted}");
        }
    }

    #[test]
    fn a_failed_v_key_up_is_retried_after_control_is_released() {
        use PasteEvent::*;
        let desk = Fake::new("[@synthetic]");
        desk.input_results.replace(VecDeque::from([2, 0, 1]));
        assert_eq!(deliver_ticket(&desk, &ticket(), "[@synthetic]", true), copied(None));
        assert_eq!(
            *desk.input_requests.borrow(),
            [
                vec![ControlDown, VDown, VUp, ControlUp],
                vec![VUp],
                vec![ControlUp],
                vec![VUp],
            ]
        );
        assert!(!desk.control_down.get());
        assert!(!desk.v_down.get());
        assert!(!desk.keys_unreleased());
        assert_eq!(desk.release_warnings.get(), 0);
        assert_eq!(*desk.recovery_waits.borrow(), [Duration::from_millis(10)]);
    }

    #[test]
    fn blocked_key_ups_are_retained_warned_and_recovered_before_any_later_paste() {
        for prefix in [1, 2, 3] {
            let desk = Fake::new("[@synthetic]");
            let releases = if prefix == 2 { 2 } else { 1 };
            desk.input_results.borrow_mut().push_back(prefix);
            desk.input_results
                .borrow_mut()
                .extend(std::iter::repeat_n(0, 3 * releases));
            assert_eq!(
                deliver_ticket(&desk, &ticket(), "[@synthetic]", true),
                copied(Some("key-release-failed"))
            );
            assert!(desk.control_down.get());
            assert_eq!(desk.v_down.get(), prefix == 2);
            assert!(desk.keys_unreleased());
            assert_eq!(desk.release_warnings.get(), 1);
            assert_eq!(desk.recovery_waits.borrow().len(), 2);

            // Another delivery cannot inject new key-downs while cleanup is blocked.
            let requests = desk.input_requests.borrow().len();
            desk.input_results
                .borrow_mut()
                .extend(std::iter::repeat_n(0, 3 * releases));
            assert_eq!(
                deliver_ticket(&desk, &ticket(), "[@synthetic]", true),
                copied(Some("key-release-failed"))
            );
            assert_eq!(desk.posts.borrow().len(), 1);
            assert_eq!(desk.input_requests.borrow().len() - requests, 3 * releases);
            assert_eq!(desk.release_warnings.get(), 2);

            // The next copy-only delivery still releases the retained keys.
            assert_eq!(deliver_ticket(&desk, &ticket(), "[@synthetic]", false), copied(None));
            assert!(!desk.keys_unreleased());
            assert!(!desk.control_down.get());
            assert!(!desk.v_down.get());
            assert_eq!(desk.posts.borrow().len(), 1);
            assert!(matches!(
                deliver_ticket(&desk, &ticket(), "[@synthetic]", true),
                Delivered::Pasted { .. }
            ));
        }
    }

    #[test]
    fn recovery_retains_only_the_key_up_that_windows_has_not_accepted() {
        use PasteEvent::*;
        let desk = Fake::new("[@synthetic]");
        // V-up is blocked on every attempt; Control-up succeeds on the first.
        desk.input_results.replace(VecDeque::from([2, 0, 1, 0, 0]));
        assert_eq!(
            deliver_ticket(&desk, &ticket(), "[@synthetic]", true),
            copied(Some("key-release-failed"))
        );
        assert!(desk.v_down.get());
        assert!(!desk.control_down.get());
        assert_eq!(desk.release_warnings.get(), 1);
        let requests = desk.input_requests.borrow().len();
        assert_eq!(deliver_ticket(&desk, &ticket(), "[@synthetic]", false), copied(None));
        assert_eq!(&desk.input_requests.borrow()[requests..], [vec![VUp]]);
        assert!(!desk.keys_unreleased());
        assert!(!desk.v_down.get());
    }

    #[test]
    fn recovery_rechecks_the_ticket_after_lock_and_retry_waits_before_new_key_downs() {
        let changes: [fn(&Fake); 3] = [
            |desk| desk.generation.set(13),
            |desk| desk.window.set(200),
            |desk| desk.sequence.set(Some(18)),
        ];
        for change in changes {
            let desk = Fake {
                // Model another delivery leaving Control down while this
                // delivery waits to acquire the injection mutex.
                before_post: Some(|desk| {
                    desk.keys.borrow_mut().control = true;
                    desk.control_down.set(true);
                }),
                after_recovery_wait: Some(change),
                ..Fake::new("[@synthetic]")
            };
            desk.input_results.replace(VecDeque::from([0, 1]));
            assert_eq!(deliver_ticket(&desk, &ticket(), "[@synthetic]", true), copied(None));
            assert_eq!(
                *desk.input_requests.borrow(),
                [vec![PasteEvent::ControlUp], vec![PasteEvent::ControlUp]]
            );
            assert!(!desk.keys_unreleased());
            assert!(!desk.control_down.get());
            assert_eq!(desk.release_warnings.get(), 0);
        }
    }

    #[test]
    fn an_expired_delivery_can_release_pending_keys_but_cannot_inject_a_new_paste() {
        let desk = Fake::new("[@synthetic]");
        desk.keys.borrow_mut().control = true;
        desk.control_down.set(true);
        let ticket = ticket();
        let guarded = TicketDeskGuard {
            desk: &desk,
            ticket: &ticket,
        };
        assert!(!guarded.post_before(42, 0x09, Instant::now() - Duration::from_millis(1)));
        assert_eq!(*desk.input_requests.borrow(), [vec![PasteEvent::ControlUp]]);
        assert!(!desk.keys_unreleased());
        assert!(!desk.control_down.get());
    }

    #[test]
    fn password_check_timeout_and_provider_error_copy_without_pasting() {
        let (release, blocked) = std::sync::mpsc::channel::<()>();
        let timeout = PasswordQueries::default()
            .run(42, Duration::ZERO, move || {
                let _ = blocked.recv_timeout(Duration::from_secs(1));
                Some(false)
            })
            .flatten();
        let _ = release.send(());
        let provider_error = PasswordQueries::default()
            .run(42, Duration::from_secs(1), || None::<bool>)
            .flatten();
        assert_eq!(timeout, None);
        assert_eq!(provider_error, None);
        for password in [timeout, provider_error, Some(true)] {
            let desk = Fake {
                password,
                ..Fake::new("[@synthetic]")
            };
            let expected = if password == Some(true) {
                copied(Some("secure-field"))
            } else {
                copied(None)
            };
            assert_eq!(deliver_ticket(&desk, &ticket(), "[@synthetic]", true), expected);
            assert!(desk.posts.borrow().is_empty());
        }
    }

    #[test]
    fn a_password_query_cannot_outlive_its_window_generation_or_clipboard_proof() {
        let changes: [fn(&Fake); 3] = [
            |desk| desk.window.set(200),
            |desk| desk.generation.set(13),
            |desk| desk.sequence.set(Some(18)),
        ];
        for change in changes {
            let desk = Fake {
                after_password: Some(change),
                ..Fake::new("[@synthetic]")
            };
            assert_eq!(deliver_ticket(&desk, &ticket(), "[@synthetic]", true), copied(None));
            assert!(desk.posts.borrow().is_empty());
            assert_eq!(desk.clipboard_reads.get(), 0);
        }
    }

    #[test]
    fn executable_display_names_run_off_thread_and_fall_back_on_failure_or_timeout() {
        let caller = std::thread::current().id();
        assert_eq!(
            display_name(&QueryWorker::default(), "writer.exe".into(), move || {
                assert_ne!(std::thread::current().id(), caller);
                Some("Synthetic Writer".into())
            }),
            "Synthetic Writer"
        );
        assert_eq!(
            display_name(&QueryWorker::default(), "writer.exe".into(), || None),
            "writer.exe"
        );
        let (release, blocked) = std::sync::mpsc::channel::<()>();
        let started = Instant::now();
        let name = display_name(&QueryWorker::default(), "writer.exe".into(), move || {
            let _ = blocked.recv_timeout(Duration::from_secs(1));
            Some("Late description".into())
        });
        let elapsed = started.elapsed();
        let _ = release.send(());
        assert_eq!(name, "writer.exe");
        assert!(
            elapsed < Duration::from_millis(500),
            "optional name blocked capture for {elapsed:?}"
        );
    }

    fn wait_for_query_exit(worker: &QueryWorker) {
        let started = Instant::now();
        while worker.0.load(std::sync::atomic::Ordering::Acquire) {
            assert!(started.elapsed() < Duration::from_secs(1), "query did not exit");
            std::thread::yield_now();
        }
    }

    #[test]
    fn timed_out_query_workers_keep_their_slots_and_never_queue_busy_requests() {
        let metadata = QueryWorker::default();
        let password = PasswordQueries::default();
        let (release_name, name_blocked) = std::sync::mpsc::channel::<()>();
        let (name_started, name_entered) = std::sync::mpsc::channel();
        assert_eq!(
            display_name(&metadata, "writer.exe".into(), move || {
                let _ = name_started.send(());
                let _ = name_blocked.recv_timeout(Duration::from_secs(5));
                Some("Late name".into())
            }),
            "writer.exe"
        );
        name_entered
            .recv_timeout(Duration::from_secs(1))
            .unwrap_or_else(|e| panic!("name worker: {e}"));
        // A hung metadata worker does not consume the password worker's slot.
        assert_eq!(
            password.run(42, Duration::from_secs(1), || Some(false)),
            Some(Some(false))
        );
        wait_for_password_queries(&password);
        let (release_password, password_blocked) = std::sync::mpsc::channel::<()>();
        let (password_started, password_entered) = std::sync::mpsc::channel();
        assert_eq!(
            password.run(42, Duration::ZERO, move || {
                let _ = password_started.send(());
                let _ = password_blocked.recv_timeout(Duration::from_secs(5));
                Some(false)
            }),
            None
        );
        password_entered
            .recv_timeout(Duration::from_secs(1))
            .unwrap_or_else(|e| panic!("password worker: {e}"));
        let ran = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        for _ in 0..100 {
            let name_ran = ran.clone();
            assert_eq!(
                display_name(&metadata, "writer.exe".into(), move || {
                    name_ran.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    Some("Unexpected name".into())
                }),
                "writer.exe"
            );
            let password_ran = ran.clone();
            let checked = password
                .run(42, Duration::from_secs(1), move || {
                    password_ran.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    Some(false)
                })
                .flatten();
            let desk = Fake {
                password: checked,
                ..Fake::new("[@synthetic]")
            };
            assert_eq!(deliver_ticket(&desk, &ticket(), "[@synthetic]", true), copied(None));
            assert!(desk.posts.borrow().is_empty());
        }
        let _ = release_name.send(());
        let _ = release_password.send(());
        wait_for_query_exit(&metadata);
        wait_for_password_queries(&password);
        assert_eq!(
            ran.load(std::sync::atomic::Ordering::Relaxed),
            0,
            "busy requests must never run later"
        );
        assert_eq!(
            display_name(&metadata, "writer.exe".into(), || Some("New name".into())),
            "New name"
        );
        assert_eq!(
            password.run(42, Duration::from_secs(1), || Some(false)),
            Some(Some(false))
        );
    }

    #[test]
    fn a_panicking_query_releases_its_worker_slot() {
        let worker = QueryWorker::default();
        assert_eq!(
            worker.run(Duration::from_secs(1), || -> bool {
                panic!("synthetic provider failure")
            }),
            None
        );
        wait_for_query_exit(&worker);
        assert_eq!(worker.run(Duration::from_secs(1), || true), Some(true));
    }

    fn wait_for_password_queries(queries: &PasswordQueries) {
        let started = Instant::now();
        while !queries.0.lock().is_ok_and(|active| active.is_empty()) {
            assert!(
                started.elapsed() < Duration::from_secs(1),
                "password queries did not exit"
            );
            std::thread::yield_now();
        }
    }

    #[test]
    fn a_stuck_password_provider_in_app_a_does_not_stop_app_b_from_pasting() {
        let queries = std::sync::Arc::new(PasswordQueries::default());
        let (release_a, stuck_a) = std::sync::mpsc::channel::<()>();
        let (started, entered) = std::sync::mpsc::channel();
        assert_eq!(
            queries.run(42, Duration::ZERO, move || {
                let _ = started.send(());
                // No provider deadline: A remains stuck throughout B's delivery.
                let _ = stuck_a.recv();
                Some(false)
            }),
            None
        );
        entered
            .recv_timeout(Duration::from_secs(1))
            .unwrap_or_else(|e| panic!("app A provider: {e}"));
        let a = Fake {
            password_queries: Some(queries.clone()),
            ..Fake::new("[@synthetic]")
        };
        assert_eq!(deliver_ticket(&a, &ticket(), "[@synthetic]", true), copied(None));
        assert!(a.posts.borrow().is_empty());

        let b = Fake {
            password_queries: Some(queries.clone()),
            front_id: "org.example.other",
            ..Fake::new("[@synthetic]")
        };
        b.fronts.replace(vec![84]);
        b.window.set(200);
        let mut b_ticket = ticket();
        b_ticket.target.window = Some(200);
        let captured = b_ticket.target.captured.as_mut().unwrap_or_else(|| panic!("B capture"));
        captured.pid = Some(84);
        captured.target.app = Some(AppId {
            system: System::Windows,
            id: "org.example.other".into(),
        });
        captured.name = Some("Other Writer".into());
        assert_eq!(
            deliver_ticket(&b, &b_ticket, "[@synthetic]", true),
            Delivered::Pasted {
                app: Some("Other Writer".into())
            }
        );
        assert_eq!(*b.posts.borrow(), [(84, 0x09)]);
        assert!(queries.0.lock().is_ok_and(|active| active.contains(&42)));
        assert_eq!(deliver_ticket(&a, &ticket(), "[@synthetic]", true), copied(None));
        assert!(a.posts.borrow().is_empty());
        // Release only for test cleanup, after proving B pasted while A was stuck.
        let _ = release_a.send(());
        wait_for_password_queries(&queries);
    }

    #[test]
    fn password_queries_cap_outstanding_processes_without_queueing_or_retaining_idle_entries() {
        let queries = PasswordQueries::default();
        let mut releases = Vec::new();
        for pid in [42, 84, 126, 168] {
            let (release, blocked) = std::sync::mpsc::channel::<()>();
            let (started, entered) = std::sync::mpsc::channel();
            assert_eq!(
                queries.run(pid, Duration::ZERO, move || {
                    let _ = started.send(());
                    let _ = blocked.recv();
                    Some(false)
                }),
                None
            );
            entered
                .recv_timeout(Duration::from_secs(1))
                .unwrap_or_else(|e| panic!("provider started: {e}"));
            releases.push(release);
        }
        let refused_runs = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        for _ in 0..100 {
            for pid in [42, 210] {
                let ran = refused_runs.clone();
                assert_eq!(
                    queries.run(pid, Duration::from_secs(1), move || {
                        ran.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        Some(false)
                    }),
                    None
                );
            }
        }
        drop(releases);
        wait_for_password_queries(&queries);
        assert_eq!(refused_runs.load(std::sync::atomic::Ordering::Relaxed), 0);
        // The cap counts only in-flight work, not the number of apps ever checked.
        for pid in [210, 252, 294, 336, 378] {
            assert_eq!(
                queries.run(pid, Duration::from_secs(1), || Some(false)),
                Some(Some(false))
            );
            wait_for_password_queries(&queries);
        }
    }

    #[test]
    fn password_process_permits_are_released_after_provider_failure_or_panic() {
        let queries = PasswordQueries::default();
        assert_eq!(queries.run(42, Duration::from_secs(1), || None::<bool>), Some(None));
        wait_for_password_queries(&queries);
        assert_eq!(
            queries.run(42, Duration::from_secs(1), || -> bool {
                panic!("synthetic provider failure")
            }),
            None
        );
        wait_for_password_queries(&queries);
        assert_eq!(
            queries.run(42, Duration::from_secs(1), || Some(false)),
            Some(Some(false))
        );
        wait_for_password_queries(&queries);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn windows_terminals_copy_with_a_reason() {
        let desk = Fake::new("[@synthetic]");
        assert_eq!(
            deliver(&desk, Some(&writer("WindowsTerminal.exe")), "[@synthetic]", true),
            copied(Some("terminal"))
        );
        assert!(desk.posts.borrow().is_empty());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn terminals_and_missing_permission_copy_with_a_reason_and_ask_once_per_copy() {
        let desk = Fake::new("[@synthetic]");
        assert_eq!(
            deliver(&desk, Some(&writer("com.apple.Terminal")), "[@synthetic]", true),
            copied(Some("terminal"))
        );
        let desk = Fake {
            may_post: false,
            ..Fake::new("[@synthetic]")
        };
        assert_eq!(
            deliver(&desk, Some(&writer("org.example.writer")), "[@synthetic]", true),
            copied(Some("no-permission"))
        );
        assert_eq!(desk.asked.get(), 1);
        assert!(desk.posts.borrow().is_empty());
    }
}
