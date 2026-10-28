//! Enter pastes into the app the picker was opened from (#27, #1241).
//!
//! After a clean copy has hidden the picker, Ibis posts one ⌘V, only to the
//! captured process, once that app is in front again and the clipboard still
//! holds the citation. `ibis_core::destination::decide` says when it must only
//! copy, and the notice says why. The citation stays on the clipboard either
//! way (ADR 0009).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use ibis_core::destination::{AppId, CopyReason, Decision, PastePermission, System, decide, lookup};
use serde::Serialize;
use tauri::{AppHandle, Manager};

mod capture;
#[cfg(target_os = "macos")]
mod macos;
pub(crate) mod permission;

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
#[cfg(any(target_os = "macos", test))]
const POST_LIMIT: Duration = Duration::from_millis(250);

/// The app the picker was last opened from.
#[derive(Default)]
pub(crate) struct LastTarget(Mutex<TargetState>);

#[derive(Default)]
struct TargetState {
    captured: Option<Captured>,
    copied: Option<CopyProof>,
    presentation: u64,
}

/// Evidence from Ibis's own successful clipboard write, consumed once. The
/// text is Ibis's citation, never read back from another clipboard owner.
struct CopyProof {
    captured: Option<Captured>,
    text: String,
    change_count: isize,
    presentation: u64,
    written: Instant,
}

impl LastTarget {
    /// Showing the picker invalidates an earlier delivery. Hide/blur events
    /// may happen more than once and must not retire the successful copy.
    fn begin_presentation(&self, captured: Option<Captured>) {
        if let Ok(mut last) = self.0.lock() {
            last.presentation = last.presentation.wrapping_add(1);
            last.copied = None;
            if let Some(captured) = captured {
                last.captured = Some(captured);
            }
        }
    }

    #[cfg(any(target_os = "macos", test))]
    fn current(&self, presentation: u64) -> bool {
        self.0.lock().is_ok_and(|last| last.presentation == presentation)
    }

    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn write_copy(
        &self,
        text: &str,
        write: impl FnOnce() -> Result<Option<isize>, String>,
    ) -> Result<(), String> {
        let mut last = self.0.lock().map_err(|_| "The clipboard could not be written.")?;
        last.copied = None;
        if let Some(change_count) = write()? {
            last.copied = Some(CopyProof {
                captured: last.captured.clone(),
                text: text.to_owned(),
                change_count,
                presentation: last.presentation,
                written: Instant::now(),
            });
        }
        Ok(())
    }

    fn take_copy(&self, text: &str) -> Option<CopyProof> {
        self.0.lock().ok()?.copied.take().filter(|proof| proof.text == text)
    }
}

/// Remember the app in front, before the picker shows. Ibis in front keeps the earlier capture.
pub(crate) fn capture(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let front = macos::frontmost();
    #[cfg(not(target_os = "macos"))]
    let front = None;
    let own = i32::try_from(std::process::id()).unwrap_or(i32::MAX);
    app.state::<LastTarget>()
        .begin_presentation(capture::captured(SYSTEM, front, own));
}

/// The app the picker was last opened from, with the name the picker shows
/// for it: the app's own, else the destination table's, else its ID.
pub(crate) fn last_app(app: &AppHandle) -> Option<(AppId, String)> {
    let captured = app.state::<LastTarget>().0.lock().ok()?.captured.clone()?;
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
trait Desk {
    /// Ibis may post key events (PostEvent, independent of AX trust).
    fn may_post(&self) -> bool;
    /// Show where to give that permission; at most once per run.
    fn ask_to_post(&self);
    /// The app in front, or `None` once the picker has been shown again.
    fn frontmost(&self) -> Option<Front>;
    /// All final guards and posting run together on the main thread.
    fn finish(&self, request: PostRequest) -> PostResult;
    fn wait(&self, time: Duration);
}

#[derive(Clone)]
// Other systems only copy until their native delivery implementation lands.
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
struct PostRequest {
    pid: i32,
    id: String,
    change_count: isize,
    presentation: u64,
    deadline: Instant,
}

#[cfg(any(target_os = "macos", test))]
impl PostRequest {
    fn queued(mut self, now: Instant) -> Self {
        self.deadline = self.deadline.min(now + POST_LIMIT);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
enum PostResult {
    Pasted,
    Refused,
    SecureInput,
    Unavailable,
}

/// These calls run in one main-thread task; none reads clipboard contents,
/// the AX tree or a remote provider. Event creation/layout work runs first.
#[cfg(any(target_os = "macos", test))]
trait PostDesk {
    fn now(&self) -> Instant;
    fn current(&self, presentation: u64) -> bool;
    fn frontmost(&self) -> Option<Front>;
    fn change_count(&self) -> isize;
    fn secure_input(&self) -> Option<bool>;
    fn post(&self, pid: i32) -> bool;
}

#[cfg(any(target_os = "macos", test))]
fn checked_post(desk: &impl PostDesk, request: &PostRequest) -> PostResult {
    if desk.now() >= request.deadline {
        return PostResult::Unavailable;
    }
    if !desk.current(request.presentation)
        || !desk
            .frontmost()
            .is_some_and(|front| front.pid == request.pid && front.id.as_deref() == Some(&request.id))
    {
        return PostResult::Refused;
    }
    let secure_input = desk.secure_input();
    if desk.now() >= request.deadline {
        return PostResult::Unavailable;
    }
    match secure_input {
        Some(true) => return PostResult::SecureInput,
        None => return PostResult::Unavailable,
        Some(false) => {}
    }
    if desk.change_count() != request.change_count {
        return PostResult::Refused;
    }
    if desk.now() >= request.deadline {
        return PostResult::Unavailable;
    }
    if desk.post(request.pid) {
        PostResult::Pasted
    } else {
        PostResult::Refused
    }
}

/// Cancellation and posting share a lock: a timed-out queued task cannot
/// later post, and a completed post cannot be reported as merely copied.
#[cfg(any(target_os = "macos", test))]
#[derive(Default)]
struct PendingPost(Mutex<Option<PostResult>>);

#[cfg(any(target_os = "macos", test))]
impl PendingPost {
    fn run(&self, work: impl FnOnce() -> PostResult) -> PostResult {
        let Ok(mut result) = self.0.lock() else {
            return PostResult::Unavailable;
        };
        *result.get_or_insert_with(work)
    }

    fn cancel(&self) -> PostResult {
        self.run(|| PostResult::Unavailable)
    }
}

/// Paste a proven copy into its captured app, or say why it stays copied.
fn deliver(desk: &impl Desk, proof: Option<&CopyProof>, enabled: bool) -> Delivered {
    let unknown = Captured::unknown(SYSTEM);
    let captured = proof.and_then(|proof| proof.captured.as_ref()).unwrap_or(&unknown);
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
            let Some(pid) = captured.pid else {
                return copied(None);
            };
            // The same process and app: a process number alone can be reused.
            let id = captured.target.app.as_ref().map(|app| app.id.as_str());
            let back = |front: Option<Front>| front.is_some_and(|front| front.pid == pid && front.id.as_deref() == id);
            let Some(proof) = proof else {
                return copied(None);
            };
            let deadline = proof.written + LATE;
            let started = Instant::now();
            let mut checks = 0;
            while !back(desk.frontmost()) {
                if checks >= RETURN_CHECKS || started.elapsed() >= RETURN_LIMIT || Instant::now() >= deadline {
                    return copied(None);
                }
                desk.wait(RETURN_STEP);
                checks += 1;
            }
            desk.wait(SETTLE);
            let request = PostRequest {
                pid,
                id: id.unwrap_or_default().to_owned(),
                change_count: proof.change_count,
                presentation: proof.presentation,
                deadline,
            };
            match desk.finish(request) {
                PostResult::Pasted => Delivered::Pasted {
                    app: captured.name.clone(),
                },
                PostResult::SecureInput => copied(Some(CopyReason::SecureInput)),
                PostResult::Unavailable => copied(Some(CopyReason::PasteUnavailable)),
                PostResult::Refused => copied(None),
            }
        }
    }
}

/// Paste into the captured app when it is safe to; the notice then says what happened.
pub(crate) fn deliver_copy(app: &AppHandle, text: &str) -> Delivered {
    let proof = app.state::<LastTarget>().take_copy(text);
    #[cfg(target_os = "macos")]
    let desk = macos::Mac::new(app);
    #[cfg(not(target_os = "macos"))]
    let desk = CopyOnly;
    let enabled = ibis_core::settings::load(&app.state::<crate::preferences::SettingsFile>().path)
        .settings
        .paste
        .unwrap_or(true);
    deliver(&desk, proof.as_ref(), enabled)
}

/// Windows (until #1133) and Linux capture no target, so they only copy.
#[cfg(not(target_os = "macos"))]
struct CopyOnly;

#[cfg(not(target_os = "macos"))]
impl Desk for CopyOnly {
    fn may_post(&self) -> bool {
        false
    }
    fn ask_to_post(&self) {}
    fn frontmost(&self) -> Option<Front> {
        None
    }
    fn finish(&self, _: PostRequest) -> PostResult {
        PostResult::Refused
    }
    fn wait(&self, _: Duration) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::destination::{AppId, Target};
    use std::cell::{Cell, RefCell};

    const TEXT: &str = "[@invented]";
    const COUNT: isize = 11;
    const PRESENTATION: u64 = 8;

    struct Fake {
        may_post: bool,
        asked: Cell<u32>,
        fronts: RefCell<Vec<i32>>,
        clock: Cell<Instant>,
        presentation: Cell<u64>,
        hidden: bool,
        change_count: Cell<isize>,
        secure: Cell<Option<bool>>,
        query_delay: Duration,
        queue_delay: Duration,
        secure_after_queue: Option<bool>,
        front_after_queue: Option<i32>,
        replace_during_check: bool,
        checks: RefCell<Vec<&'static str>>,
        posts: RefCell<Vec<i32>>,
        pending: PendingPost,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                may_post: true,
                asked: Cell::new(0),
                fronts: RefCell::new(vec![42]),
                clock: Cell::new(Instant::now()),
                presentation: Cell::new(PRESENTATION),
                hidden: true,
                change_count: Cell::new(COUNT),
                secure: Cell::new(Some(false)),
                query_delay: Duration::ZERO,
                queue_delay: Duration::ZERO,
                secure_after_queue: None,
                front_after_queue: None,
                replace_during_check: false,
                checks: RefCell::default(),
                posts: RefCell::default(),
                pending: PendingPost::default(),
            }
        }

        fn front(&self) -> Option<Front> {
            self.fronts.borrow().first().map(|&pid| Front {
                id: Some("org.example.writer".into()),
                pid,
                name: None,
            })
        }
    }

    impl Desk for Fake {
        fn may_post(&self) -> bool {
            self.may_post
        }
        fn ask_to_post(&self) {
            self.asked.set(self.asked.get() + 1);
        }
        fn frontmost(&self) -> Option<Front> {
            self.front()
        }
        fn finish(&self, request: PostRequest) -> PostResult {
            let request = request.queued(self.clock.get());
            self.clock.set(self.clock.get() + self.queue_delay);
            if let Some(secure) = self.secure_after_queue {
                self.secure.set(Some(secure));
            }
            if let Some(pid) = self.front_after_queue {
                self.fronts.replace(vec![pid]);
            }
            self.pending.run(|| checked_post(self, &request))
        }
        fn wait(&self, time: Duration) {
            self.clock.set(self.clock.get() + time);
            let mut fronts = self.fronts.borrow_mut();
            if fronts.len() > 1 {
                fronts.remove(0);
            }
        }
    }

    // Deliberately no clipboard-content or AX API: neither is needed by
    // ordinary controls without subroles, denied AX trust or lazy providers.
    impl PostDesk for Fake {
        fn now(&self) -> Instant {
            self.clock.get()
        }
        fn current(&self, presentation: u64) -> bool {
            self.hidden && self.presentation.get() == presentation
        }
        fn frontmost(&self) -> Option<Front> {
            self.checks.borrow_mut().push("foreground");
            self.front()
        }
        fn change_count(&self) -> isize {
            self.checks.borrow_mut().push("change-count");
            self.change_count.get()
        }
        fn secure_input(&self) -> Option<bool> {
            self.checks.borrow_mut().push("secure-input");
            self.clock.set(self.clock.get() + self.query_delay);
            if self.replace_during_check {
                self.change_count.set(COUNT + 1);
            }
            self.secure.get()
        }
        fn post(&self, pid: i32) -> bool {
            self.checks.borrow_mut().push("post");
            self.posts.borrow_mut().push(pid);
            true
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

    fn proof() -> CopyProof {
        CopyProof {
            captured: Some(writer("org.example.writer")),
            text: TEXT.into(),
            change_count: COUNT,
            presentation: PRESENTATION,
            written: Instant::now(),
        }
    }

    fn copied(reason: Option<&str>) -> Delivered {
        Delivered::Copied {
            reason: reason.map(str::to_owned),
        }
    }

    fn pasted() -> Delivered {
        Delivered::Pasted {
            app: Some("Writer".into()),
        }
    }

    #[test]
    #[cfg_attr(target_os = "linux", ignore = "Linux never pastes")]
    fn ordinary_editor_without_subrole_or_ax_trust_pastes_once_after_returning() {
        let desk = Fake::new();
        desk.fronts.replace(vec![7, 7, 42]);
        assert_eq!(deliver(&desk, Some(&proof()), true), pasted());
        assert_eq!(*desk.posts.borrow(), [42]);
        assert_eq!(
            *desk.checks.borrow(),
            ["foreground", "secure-input", "change-count", "post"]
        );
    }

    #[test]
    #[cfg_attr(target_os = "linux", ignore = "Linux never pastes")]
    fn secure_typing_and_unavailable_checks_have_distinct_copy_reasons() {
        for (secure, reason) in [(Some(true), "secure-input"), (None, "paste-unavailable")] {
            let desk = Fake {
                secure: Cell::new(secure),
                ..Fake::new()
            };
            assert_eq!(deliver(&desk, Some(&proof()), true), copied(Some(reason)));
            assert!(desk.posts.borrow().is_empty());
        }
    }

    #[test]
    #[cfg_attr(target_os = "linux", ignore = "Linux never pastes")]
    fn secure_input_enabled_while_the_final_task_waits_refuses_the_paste() {
        let desk = Fake {
            queue_delay: Duration::from_millis(100),
            secure_after_queue: Some(true),
            ..Fake::new()
        };
        assert_eq!(deliver(&desk, Some(&proof()), true), copied(Some("secure-input")));
        assert!(desk.posts.borrow().is_empty());
    }

    #[test]
    #[cfg_attr(target_os = "linux", ignore = "Linux never pastes")]
    fn clipboard_replaced_during_the_final_check_is_never_read_or_pasted() {
        let desk = Fake {
            replace_during_check: true,
            ..Fake::new()
        };
        assert_eq!(deliver(&desk, Some(&proof()), true), copied(None));
        assert!(desk.posts.borrow().is_empty());
        assert_eq!(*desk.checks.borrow(), ["foreground", "secure-input", "change-count"]);
    }

    #[test]
    #[cfg_attr(target_os = "linux", ignore = "Linux never pastes")]
    fn an_expired_main_thread_task_or_slow_flag_check_cannot_paste() {
        for (queue_delay, query_delay, secure) in [
            (POST_LIMIT, Duration::ZERO, Some(false)),
            (Duration::ZERO, POST_LIMIT, Some(false)),
            // A positive flag that arrives late is still a timed-out check.
            (Duration::ZERO, POST_LIMIT, Some(true)),
        ] {
            let desk = Fake {
                queue_delay,
                query_delay,
                secure: Cell::new(secure),
                ..Fake::new()
            };
            assert_eq!(deliver(&desk, Some(&proof()), true), copied(Some("paste-unavailable")));
            assert!(desk.posts.borrow().is_empty());
        }
    }

    #[test]
    fn cancellation_prevents_a_queued_task_from_running_and_preserves_completed_results() {
        let pending = PendingPost::default();
        assert_eq!(pending.cancel(), PostResult::Unavailable);
        assert_eq!(
            pending.run(|| panic!("cancelled task must not post")),
            PostResult::Unavailable
        );
        for outcome in [PostResult::Pasted, PostResult::SecureInput] {
            let completed = PendingPost::default();
            assert_eq!(completed.run(|| outcome), outcome);
            assert_eq!(completed.cancel(), outcome);
        }
    }

    #[test]
    #[cfg_attr(target_os = "linux", ignore = "Linux never pastes")]
    fn final_task_revalidates_foreground_picker_presentation_and_visibility() {
        for desk in [
            Fake {
                front_after_queue: Some(7),
                ..Fake::new()
            },
            Fake {
                presentation: Cell::new(PRESENTATION + 1),
                ..Fake::new()
            },
            Fake {
                hidden: false,
                ..Fake::new()
            },
        ] {
            assert_eq!(deliver(&desk, Some(&proof()), true), copied(None));
            assert!(desk.posts.borrow().is_empty());
        }
    }

    #[test]
    fn only_our_write_supplies_a_snapshot_and_the_proof_is_consumed_once() {
        let last = LastTarget::default();
        {
            let mut state = last.0.lock().unwrap_or_else(|e| panic!("{e}"));
            state.captured = Some(writer("org.example.writer"));
        }
        assert!(last.write_copy(TEXT, || Ok(Some(COUNT))).is_ok());
        let ticket = last.take_copy(TEXT).unwrap_or_else(|| panic!("copy proof"));
        assert_eq!(ticket.change_count, COUNT);
        assert_eq!(ticket.presentation, 0);
        assert_eq!(ticket.captured, Some(writer("org.example.writer")));
        assert!(last.take_copy(TEXT).is_none());
        assert!(last.write_copy(TEXT, || Ok(Some(COUNT))).is_ok());
        assert!(last.take_copy("different citation").is_none());
        assert!(last.take_copy(TEXT).is_none());
        // A failed write or a concurrent replacement cannot reuse an old proof.
        for result in [Err("write failed".into()), Ok(None)] {
            assert!(last.write_copy(TEXT, || Ok(Some(COUNT))).is_ok());
            let _ = last.write_copy(TEXT, || result);
            assert!(last.take_copy(TEXT).is_none());
        }
    }

    #[test]
    #[cfg_attr(target_os = "linux", ignore = "Linux never pastes")]
    fn delivery_deadline_starts_at_the_own_write_not_at_notice_or_queue_time() {
        let mut stale = proof();
        stale.written -= LATE;
        let desk = Fake::new();
        assert_eq!(deliver(&desk, Some(&stale), true), copied(Some("paste-unavailable")));
        assert!(desk.posts.borrow().is_empty());
    }

    #[test]
    fn a_new_presentation_invalidates_delivery_even_when_it_keeps_the_same_target() {
        let last = LastTarget::default();
        last.begin_presentation(Some(writer("org.example.writer")));
        assert!(last.write_copy(TEXT, || Ok(Some(COUNT))).is_ok());
        let ticket = last.take_copy(TEXT).unwrap_or_else(|| panic!("copy proof"));
        // Hide/blur can advance PickerGeneration repeatedly. This proof
        // stays current until the next presentation, independently of that.
        assert!(last.current(ticket.presentation));
        last.begin_presentation(None);
        assert!(!last.current(ticket.presentation));
        {
            let state = last.0.lock().unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(state.captured, ticket.captured);
        }
        last.begin_presentation(Some(writer("org.example.other")));
        assert!(!last.current(ticket.presentation));
        assert_eq!(ticket.captured, Some(writer("org.example.writer")));
    }

    #[test]
    fn unsafe_unproven_and_disabled_targets_only_copy() {
        for (captured, enabled) in [
            (Some(writer("org.example.other")), true),
            (Some(writer("org.example.writer")), false),
            (None, true),
        ] {
            let desk = Fake::new();
            let ticket = CopyProof { captured, ..proof() };
            assert_eq!(deliver(&desk, Some(&ticket), enabled), copied(None));
            assert!(desk.posts.borrow().is_empty());
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn terminals_and_missing_postevent_permission_keep_the_existing_flow() {
        let desk = Fake::new();
        let terminal = CopyProof {
            captured: Some(writer("com.apple.Terminal")),
            ..proof()
        };
        assert_eq!(deliver(&desk, Some(&terminal), true), copied(Some("terminal")));
        let desk = Fake {
            may_post: false,
            ..Fake::new()
        };
        assert_eq!(deliver(&desk, Some(&proof()), true), copied(Some("no-permission")));
        assert_eq!(desk.asked.get(), 1);
        assert!(desk.posts.borrow().is_empty());
    }
}
