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
#[cfg(test)]
mod notepad_test;
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
#[cfg(any(target_os = "macos", test))]
const POST_LIMIT: Duration = Duration::from_millis(250);

/// A suspend-aware time and lifecycle identity, frozen by the successful write.
#[derive(Clone, Copy)]
struct SafetyStamp {
    epoch: u64,
    elapsed: Duration,
}

#[derive(Clone, Copy)]
enum SafetyFailure {
    Expired,
    Unavailable,
}

impl SafetyFailure {
    fn reason(self) -> CopyReason {
        match self {
            Self::Expired => CopyReason::PasteExpired,
            Self::Unavailable => CopyReason::PasteUnavailable,
        }
    }

    #[cfg(any(target_os = "macos", target_os = "windows", test))]
    fn result(self) -> PostResult {
        match self {
            Self::Expired => PostResult::Expired,
            Self::Unavailable => PostResult::Unavailable,
        }
    }
}

impl SafetyStamp {
    fn now() -> Self {
        #[cfg(target_os = "macos")]
        return macos::stamp();
        #[cfg(target_os = "windows")]
        return Self {
            epoch: 1,
            elapsed: Duration::from_millis(winsafe::GetTickCount64()),
        };
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        Self {
            epoch: 1,
            elapsed: Duration::ZERO,
        }
    }

    fn check(self, now: Self) -> Result<(), SafetyFailure> {
        if [self.epoch, now.epoch]
            .iter()
            .any(|epoch| *epoch == 0 || *epoch == u64::MAX)
        {
            return Err(SafetyFailure::Unavailable);
        }
        let age = now
            .elapsed
            .checked_sub(self.elapsed)
            .ok_or(SafetyFailure::Unavailable)?;
        if self.epoch != now.epoch || age >= LATE {
            return Err(SafetyFailure::Expired);
        }
        Ok(())
    }

    #[cfg(test)]
    fn valid_at(self, now: Self) -> bool {
        self.check(now).is_ok()
    }
}

/// Receipts identify writes, independently of citation text and native
/// clipboard counters. Never wrap and let a stale notice claim a new proof.
#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn write_token() -> Option<u32> {
    static NEXT: Mutex<u32> = Mutex::new(0);
    let mut token = NEXT.lock().ok()?;
    *token = token.checked_add(1)?;
    Some(*token)
}

/// The app the picker was last opened from.
#[derive(Default)]
pub(crate) struct LastTarget(Mutex<TargetState>);

#[derive(Default)]
struct TargetState {
    #[cfg(any(not(target_os = "windows"), test))]
    owner: Option<u32>,
    #[cfg(any(not(target_os = "windows"), test))]
    captured: Option<Captured>,
    #[cfg(any(not(target_os = "windows"), test))]
    copied: Option<CopyProof>,
    #[cfg(any(not(target_os = "windows"), test))]
    presentation: u64,
    #[cfg(target_os = "windows")]
    target: Option<CopyTarget<windows::Window>>,
    #[cfg(target_os = "windows")]
    pending: Option<DeliveryTicket<windows::Window>>,
}

/// Evidence from Ibis's own successful clipboard write, consumed once. The
/// text is Ibis's citation, never read back from another clipboard owner.
#[cfg(any(not(target_os = "windows"), test))]
struct CopyProof {
    token: u32,
    captured: Option<Captured>,
    text: String,
    #[cfg(any(target_os = "macos", test))]
    change_count: isize,
    #[cfg(any(target_os = "macos", test))]
    presentation: u64,
    written: Instant,
    stamp: SafetyStamp,
}

#[cfg(any(not(target_os = "windows"), test))]
impl LastTarget {
    /// Showing the picker invalidates an earlier delivery. Hide/blur events
    /// may happen more than once and must not retire the successful copy.
    fn begin_presentation(&self, captured: Option<Captured>) {
        if let Ok(mut last) = self.0.lock() {
            last.presentation = last.presentation.wrapping_add(1);
            last.copied = None;
            last.owner = None;
            if let Some(captured) = captured {
                last.captured = Some(captured);
            }
        }
    }

    #[cfg(test)]
    fn current(&self, presentation: u64) -> bool {
        self.0.lock().is_ok_and(|last| last.presentation == presentation)
    }

    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn write_copy(
        &self,
        text: &str,
        write: impl FnOnce() -> Result<Option<isize>, String>,
    ) -> Result<Option<u32>, String> {
        self.write_copy_at(text, SafetyStamp::now(), write)
    }

    #[cfg(any(target_os = "macos", test))]
    fn write_copy_at(
        &self,
        text: &str,
        stamp: SafetyStamp,
        write: impl FnOnce() -> Result<Option<isize>, String>,
    ) -> Result<Option<u32>, String> {
        let mut last = self.0.lock().map_err(|_| "The clipboard could not be written.")?;
        last.copied = None;
        last.owner = None;
        let change_count = write()?;
        if let Some((change_count, token)) = change_count.zip(write_token()) {
            last.owner = Some(token);
            last.copied = Some(CopyProof {
                token,
                captured: last.captured.clone(),
                text: text.to_owned(),
                change_count,
                presentation: last.presentation,
                written: Instant::now(),
                stamp,
            });
            return Ok(Some(token));
        }
        Ok(None)
    }

    fn take_copy(&self, text: &str, token: Option<u32>) -> Option<CopyProof> {
        self.0
            .lock()
            .ok()?
            .copied
            .take_if(|proof| Some(proof.token) == token && proof.text == text)
    }

    /// Serialize own writes, presentation retirement and posting. Taking a
    /// receipt does not surrender its ownership to a superseded worker.
    #[cfg(any(target_os = "macos", test))]
    fn post_if_current(&self, request: &PostRequest, post: impl FnOnce() -> PostResult) -> PostResult {
        let Ok(mut last) = self.0.try_lock() else {
            return PostResult::Unavailable;
        };
        if last.presentation != request.presentation || last.owner != Some(request.token) {
            return PostResult::Refused;
        }
        last.owner = None;
        post()
    }
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
    #[cfg(not(target_os = "windows"))]
    app.state::<LastTarget>()
        .begin_presentation(capture::captured(SYSTEM, front, own));
    #[cfg(target_os = "windows")]
    if let Ok(mut last) = app.state::<LastTarget>().0.lock() {
        // A newer show may finish its capture while this one's optional
        // name is waiting. Never stamp an older HWND with its generation.
        publish_capture(
            &mut last.target,
            CopyTarget {
                captured: capture::captured(SYSTEM, front, own),
                window,
                generation,
            },
            app.state::<crate::picker::PickerGeneration>().current(),
        );
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

/// The footer's intended action, from the same policy as delivery. This only
/// reads the captured target and permission; secure input is checked at delivery.
pub(crate) fn intends_paste(app: &AppHandle, enabled: bool) -> bool {
    let state = app.state::<LastTarget>();
    let Ok(last) = state.0.lock() else { return false };
    #[cfg(not(target_os = "windows"))]
    let captured = last.captured.as_ref();
    #[cfg(target_os = "windows")]
    let captured = last.target.as_ref().and_then(|target| target.captured.as_ref());
    intended_paste(
        captured,
        PastePermission {
            enabled,
            granted: enabled && permission::paste_permission().granted,
        },
    )
}

fn intended_paste(captured: Option<&Captured>, permission: PastePermission) -> bool {
    captured.is_some_and(|captured| delivery_decision(captured, permission) == Decision::Paste)
}

fn delivery_decision(captured: &Captured, permission: PastePermission) -> Decision {
    match decide(&captured.target, permission) {
        Decision::Paste | Decision::Copy(CopyReason::NoPermission)
            if captured.target.system == System::Macos && captured.window.is_none() =>
        {
            Decision::Copy(captured.focus_failure.unwrap_or(CopyReason::PasteUnavailable))
        }
        decision => decision,
    }
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
    type Proof: CopyEvidence;
    /// Release keys left by an earlier partial Windows injection, including
    /// on deliveries that will only copy. macOS has no pending releases.
    fn release_keys(&self) -> bool {
        true
    }
    fn stamp(&self) -> SafetyStamp {
        SafetyStamp::now()
    }
    /// Ibis may post key events (macOS PostEvent, independent of AX trust;
    /// Windows requires no permission).
    fn may_post(&self) -> bool;
    /// Show where to give that permission; at most once per run.
    fn ask_to_post(&self);
    /// Restore only a proven target before waiting for it to come forward.
    fn prepare(&self) -> Result<(), CopyReason> {
        Ok(())
    }
    /// The app in front, or `None` once the picker has been shown again.
    /// A failed proof must keep its reason while waiting for that app.
    fn frontmost(&self) -> Result<Option<Front>, CopyReason>;
    /// Finish with this platform's proof and final guards. On macOS all
    /// checks and posting run together in one main-thread task.
    fn finish(&self, request: DeliveryRequest, proof: &Self::Proof) -> PostResult;
    fn wait(&self, time: Duration);
}

/// Each system keeps immutable target evidence and lifetime anchors for the
/// shared policy. Windows starts its lifetime before clipboard I/O.
trait CopyEvidence {
    fn captured(&self) -> Option<&Captured>;
    fn written(&self) -> Instant;
    fn stamp(&self) -> SafetyStamp;

    fn check_age(&self, now: SafetyStamp) -> Result<(), SafetyFailure> {
        self.stamp().check(now)?;
        if Instant::now() >= self.written() + LATE {
            return Err(SafetyFailure::Expired);
        }
        Ok(())
    }
}

#[cfg(any(not(target_os = "windows"), test))]
impl CopyEvidence for CopyProof {
    fn captured(&self) -> Option<&Captured> {
        self.captured.as_ref()
    }
    fn written(&self) -> Instant {
        self.written
    }
    fn stamp(&self) -> SafetyStamp {
        self.stamp
    }
}

#[cfg_attr(not(any(target_os = "macos", target_os = "windows", test)), allow(dead_code))]
struct DeliveryRequest {
    pid: i32,
    id: String,
    deadline: Instant,
}

#[cfg(any(target_os = "macos", test))]
#[derive(Clone)]
struct PostRequest {
    token: u32,
    pid: i32,
    id: String,
    change_count: isize,
    presentation: u64,
    stamp: SafetyStamp,
    window: Option<u32>,
    deadline: Instant,
}

#[cfg(any(target_os = "macos", test))]
impl PostRequest {
    fn new(request: DeliveryRequest, proof: &CopyProof) -> Self {
        Self {
            token: proof.token,
            pid: request.pid,
            id: request.id,
            change_count: proof.change_count,
            presentation: proof.presentation,
            stamp: proof.stamp,
            window: proof.captured.as_ref().and_then(|captured| captured.window),
            deadline: request.deadline,
        }
    }

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
    Expired,
    TargetChanged,
    ClipboardChanged,
    AccessibilityRequired,
    NoPermission,
    #[cfg(any(target_os = "windows", test))]
    SecureField,
    #[cfg(any(target_os = "windows", test))]
    KeyReleaseFailed,
}

/// These calls run in one main-thread task. The only AX query is the bounded,
/// identity-only focused-window proof (ADR 0029); never clipboard contents.
#[cfg(any(target_os = "macos", test))]
trait PostDesk {
    fn now(&self) -> Instant;
    fn stamp(&self) -> SafetyStamp;
    fn post_event_granted(&self) -> bool;
    fn current(&self, presentation: u64) -> bool;
    /// Run refresh after the initial provider reads, before the tightly
    /// bounded final window/global-focus pair. No lock follows that pair.
    fn frontmost(&self, refresh: impl FnOnce() -> Result<(), CopyReason>) -> Result<Option<Front>, CopyReason>;
    fn change_count(&self) -> isize;
    fn secure_input(&self) -> Option<bool>;
    fn post(&self, pid: i32) -> bool;
}

#[cfg(any(target_os = "macos", test))]
fn checked_post(desk: &impl PostDesk, request: &PostRequest) -> PostResult {
    if let Err(failure) = request.stamp.check(desk.stamp()) {
        return failure.result();
    }
    if desk.now() >= request.deadline {
        return PostResult::Unavailable;
    }
    if !desk.current(request.presentation) {
        return PostResult::Refused;
    }
    let target_matches = |refresh: bool| {
        let front = desk
            .frontmost(|| {
                if !refresh {
                    return Ok(());
                }
                // AX provider messaging can wait. Refresh clipboard/security after
                // it, then allow only bounded final identity reads before post.
                request.stamp.check(desk.stamp()).map_err(|failure| failure.reason())?;
                if !desk.post_event_granted() {
                    return Err(CopyReason::NoPermission);
                }
                let secure = desk.secure_input();
                if desk.now() >= request.deadline {
                    return Err(CopyReason::PasteUnavailable);
                }
                match secure {
                    Some(true) => return Err(CopyReason::SecureInput),
                    None => return Err(CopyReason::PasteUnavailable),
                    Some(false) => {}
                }
                if desk.change_count() != request.change_count {
                    return Err(CopyReason::ClipboardChanged);
                }
                if desk.now() >= request.deadline {
                    return Err(CopyReason::PasteUnavailable);
                }
                Ok(())
            })
            .map_err(|reason| match reason {
                CopyReason::TargetChanged => PostResult::TargetChanged,
                CopyReason::PasteExpired => PostResult::Expired,
                CopyReason::SecureInput => PostResult::SecureInput,
                CopyReason::ClipboardChanged => PostResult::ClipboardChanged,
                CopyReason::AccessibilityRequired => PostResult::AccessibilityRequired,
                CopyReason::NoPermission => PostResult::NoPermission,
                _ => PostResult::Unavailable,
            })?;
        if front.is_some_and(|front| {
            front.pid == request.pid
                && front.id.as_deref() == Some(&request.id)
                && request.window.is_some()
                && front.window == request.window
        }) {
            Ok(())
        } else {
            Err(PostResult::TargetChanged)
        }
    };
    if let Err(failure) = target_matches(false) {
        return failure;
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
        return PostResult::ClipboardChanged;
    }
    if desk.now() >= request.deadline {
        return PostResult::Unavailable;
    }
    if let Err(failure) = request.stamp.check(desk.stamp()) {
        return failure.result();
    }
    if desk.now() >= request.deadline {
        return PostResult::Unavailable;
    }
    // All shared-state locks precede this last target check. Its bounded AX
    // read includes lock-free completion/continuity checks before comparison.
    // Posting is still process-directed: public macOS APIs cannot atomically
    // bind Command-V to a foreign window. Keep this remaining gap explicit.
    if let Err(failure) = target_matches(true) {
        return failure;
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
fn deliver<D: Desk>(desk: &D, proof: Option<&D::Proof>, enabled: bool) -> Delivered {
    if !desk.release_keys() {
        return key_release_failed();
    }
    let unknown = Captured::unknown(SYSTEM);
    let captured = proof.and_then(CopyEvidence::captured).unwrap_or(&unknown);
    let permission = PastePermission {
        enabled,
        granted: enabled && desk.may_post(),
    };
    let copied = |reason: Option<CopyReason>| Delivered::Copied {
        reason: reason
            .and_then(|reason| serde_json::to_value(reason).ok())
            .and_then(|code| code.as_str().map(str::to_owned)),
    };
    match delivery_decision(captured, permission) {
        Decision::Copy(CopyReason::NoPermission) => {
            desk.ask_to_post();
            copied(Some(CopyReason::NoPermission))
        }
        Decision::Copy(reason) => copied(Some(reason)),
        Decision::Paste => {
            let Some(proof) = proof else {
                return copied(Some(CopyReason::NoTarget));
            };
            let deadline = proof.written() + LATE;
            if let Err(failure) = proof.check_age(desk.stamp()) {
                return copied(Some(failure.reason()));
            }
            if let Err(reason) = desk.prepare() {
                return copied(Some(reason));
            }
            let Some(pid) = captured.pid else {
                return copied(None);
            };
            // The same process and app: a process number alone can be reused.
            let id = captured.target.app.as_ref().map(|app| app.id.as_str());
            let back = |front: Option<Front>| front.is_some_and(|front| front.pid == pid && front.id.as_deref() == id);
            let started = Instant::now();
            let mut checks = 0;
            loop {
                let front = match desk.frontmost() {
                    Ok(front) => front,
                    Err(reason) => return copied(Some(reason)),
                };
                if back(front) {
                    break;
                }
                if let Err(failure) = proof.check_age(desk.stamp()) {
                    return copied(Some(failure.reason()));
                }
                if checks >= RETURN_CHECKS || started.elapsed() >= RETURN_LIMIT {
                    return copied(Some(CopyReason::TargetChanged));
                }
                desk.wait(RETURN_STEP);
                checks += 1;
            }
            desk.wait(SETTLE);
            let request = DeliveryRequest {
                pid,
                id: id.unwrap_or_default().to_owned(),
                deadline,
            };
            match desk.finish(request, proof) {
                PostResult::Pasted => Delivered::Pasted {
                    app: captured.name.clone(),
                },
                PostResult::SecureInput => copied(Some(CopyReason::SecureInput)),
                PostResult::Unavailable => copied(Some(CopyReason::PasteUnavailable)),
                PostResult::Expired => copied(Some(CopyReason::PasteExpired)),
                PostResult::TargetChanged => copied(Some(CopyReason::TargetChanged)),
                PostResult::ClipboardChanged => copied(Some(CopyReason::ClipboardChanged)),
                PostResult::AccessibilityRequired => copied(Some(CopyReason::AccessibilityRequired)),
                PostResult::NoPermission => copied(Some(CopyReason::NoPermission)),
                #[cfg(any(target_os = "windows", test))]
                PostResult::SecureField => copied(Some(CopyReason::SecureField)),
                #[cfg(any(target_os = "windows", test))]
                PostResult::KeyReleaseFailed => key_release_failed(),
                PostResult::Refused => copied(None),
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
/// final sequence proved after closing the write. Neither
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
    token: u32,
    target: CopyTarget<W>,
    sequence: u32,
    written: Instant,
    stamp: SafetyStamp,
}

/// Identity and time at request entry, before target locks or clipboard I/O.
#[cfg(any(target_os = "windows", test))]
pub(crate) struct CopyAttempt<W> {
    target: Option<CopyTarget<W>>,
    written: Instant,
    stamp: SafetyStamp,
}

#[cfg(any(target_os = "windows", test))]
impl<W> CopyEvidence for DeliveryTicket<W> {
    fn captured(&self) -> Option<&Captured> {
        self.target.captured.as_ref()
    }
    fn written(&self) -> Instant {
        self.written
    }
    fn stamp(&self) -> SafetyStamp {
        self.stamp
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn begin_copy(app: &AppHandle) -> CopyAttempt<windows::Window> {
    let stamp = SafetyStamp::now();
    let written = Instant::now();
    let generation = app.state::<crate::picker::PickerGeneration>().current();
    let target = app
        .state::<LastTarget>()
        .0
        .lock()
        .ok()
        .and_then(|last| last.target.clone())
        .filter(|target| target.generation == generation);
    CopyAttempt { target, written, stamp }
}

#[cfg(target_os = "windows")]
pub(crate) fn remember_copy(
    app: &AppHandle,
    attempt: CopyAttempt<windows::Window>,
    sequence: Option<u32>,
) -> Option<u32> {
    let state = app.state::<LastTarget>();
    let mut last = state.0.lock().ok()?;
    remember_ticket(&mut last.pending, attempt, sequence)
}

#[cfg(any(target_os = "windows", test))]
fn remember_ticket<W>(
    pending: &mut Option<DeliveryTicket<W>>,
    attempt: CopyAttempt<W>,
    sequence: Option<u32>,
) -> Option<u32> {
    *pending = attempt.target.zip(sequence).and_then(|(target, sequence)| {
        Some(DeliveryTicket {
            token: write_token()?,
            target,
            sequence,
            written: attempt.written,
            stamp: attempt.stamp,
        })
    });
    pending.as_ref().map(|ticket| ticket.token)
}

/// A stale notice must leave a newer copy's ticket alone.
#[cfg(any(target_os = "windows", test))]
fn take_ticket<W>(pending: &mut Option<DeliveryTicket<W>>, token: Option<u32>) -> Option<DeliveryTicket<W>> {
    pending.take_if(|ticket| Some(ticket.token) == token)
}

#[cfg(target_os = "windows")]
pub(crate) fn take_copy(app: &AppHandle, token: Option<u32>) -> Option<DeliveryTicket<windows::Window>> {
    take_ticket(&mut app.state::<LastTarget>().0.lock().ok()?.pending, token)
}

/// Windows checks that can be exercised with fake HWNDs on every host.
#[cfg(any(target_os = "windows", test))]
trait TicketDesk {
    type Window;
    fn stamp(&self) -> SafetyStamp {
        SafetyStamp::now()
    }
    fn release_keys(&self) -> bool;
    fn keys_unreleased(&self) -> bool;
    fn may_post(&self) -> bool;
    fn ask_to_post(&self);
    fn frontmost(&self) -> Option<Front>;
    /// UI Automation checks the focused field in the target process. Failure
    /// or timeout is unproven, not evidence of a password field.
    fn secure_field(&self, pid: i32) -> Option<bool>;
    fn paste_key(&self) -> Option<u16>;
    fn wait(&self, time: Duration);
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
    fn current(&self) -> Result<(), CopyReason> {
        // A stale presentation is distinct from replaced or unreadable clipboard evidence.
        if self.desk.generation() != self.ticket.target.generation.wrapping_add(1) || !self.desk.picker_hidden() {
            return Err(CopyReason::TargetChanged);
        }
        match self.desk.clipboard_sequence() {
            None => Err(CopyReason::PasteUnavailable),
            Some(sequence) if sequence != self.ticket.sequence => Err(CopyReason::ClipboardChanged),
            Some(_) => Ok(()),
        }
    }

    fn in_front(&self) -> bool {
        self.ticket
            .target
            .window
            .as_ref()
            .is_some_and(|window| self.desk.window_in_front(window))
    }

    fn target_in_front(&self, request: &DeliveryRequest) -> Result<(), CopyReason> {
        self.current()?;
        if self
            .desk
            .frontmost()
            .is_some_and(|front| front.pid == request.pid && front.id.as_deref() == Some(&request.id))
            && self.in_front()
        {
            Ok(())
        } else {
            Err(CopyReason::TargetChanged)
        }
    }

    fn checked(&self, request: &DeliveryRequest) -> Result<(), PostResult> {
        self.ticket
            .check_age(self.desk.stamp())
            .map_err(SafetyFailure::result)?;
        if Instant::now() >= request.deadline {
            return Err(PostResult::Expired);
        }
        self.target_in_front(request).map_err(|reason| match reason {
            CopyReason::TargetChanged => PostResult::TargetChanged,
            CopyReason::ClipboardChanged => PostResult::ClipboardChanged,
            _ => PostResult::Unavailable,
        })?;
        // Windows target/provider queries can wait. Recheck the immutable
        // pre-I/O deadline after them; SendInput has its own final native guard.
        self.ticket
            .check_age(self.desk.stamp())
            .map_err(SafetyFailure::result)?;
        if Instant::now() >= request.deadline {
            return Err(PostResult::Expired);
        }
        Ok(())
    }
}

#[cfg(any(target_os = "windows", test))]
impl<D: TicketDesk> Desk for TicketDeskGuard<'_, D> {
    type Proof = DeliveryTicket<D::Window>;

    fn stamp(&self) -> SafetyStamp {
        self.desk.stamp()
    }
    fn release_keys(&self) -> bool {
        self.desk.release_keys()
    }
    fn may_post(&self) -> bool {
        self.desk.may_post()
    }
    fn ask_to_post(&self) {
        self.desk.ask_to_post();
    }

    fn prepare(&self) -> Result<(), CopyReason> {
        self.current()?;
        if self
            .ticket
            .target
            .window
            .as_ref()
            .zip(self.ticket.target.captured.as_ref().and_then(|c| c.pid))
            .is_some_and(|(window, pid)| self.desk.restore(window, pid))
        {
            Ok(())
        } else {
            Err(CopyReason::TargetChanged)
        }
    }

    fn frontmost(&self) -> Result<Option<Front>, CopyReason> {
        self.current()?;
        Ok(self.in_front().then(|| self.desk.frontmost()).flatten())
    }

    fn finish(&self, request: DeliveryRequest, _: &Self::Proof) -> PostResult {
        if let Err(result) = self.checked(&request) {
            return result;
        }
        let secure = self.desk.secure_field(request.pid);
        if let Err(result) = self.checked(&request) {
            return result;
        }
        match secure {
            Some(true) => return PostResult::SecureField,
            None => return PostResult::Refused,
            Some(false) => {}
        }
        let Some(key) = self.desk.paste_key() else {
            return PostResult::Refused;
        };
        // Remember the failure that actually prevented injection, rather than
        // re-querying mutable evidence and inventing a different refusal reason.
        let refused = std::cell::Cell::new(None);
        if self.desk.post_if(request.pid, key, || match self.checked(&request) {
            Ok(()) => true,
            Err(result) => {
                refused.set(Some(result));
                false
            }
        }) {
            PostResult::Pasted
        } else if self.desk.keys_unreleased() {
            PostResult::KeyReleaseFailed
        } else {
            refused.get().unwrap_or(PostResult::Refused)
        }
    }

    fn wait(&self, time: Duration) {
        self.desk.wait(time);
    }
}

#[cfg(any(target_os = "windows", test))]
fn deliver_ticket<D: TicketDesk>(
    desk: &D,
    ticket: &DeliveryTicket<D::Window>,
    _text: &str,
    enabled: bool,
) -> Delivered {
    deliver(&TicketDeskGuard { desk, ticket }, Some(ticket), enabled)
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
pub(crate) fn deliver_copy(app: &AppHandle, text: &str, token: Option<u32>) -> Delivered {
    let proof = app.state::<LastTarget>().take_copy(text, token);
    #[cfg(target_os = "macos")]
    let desk = macos::Mac::new(app);
    #[cfg(target_os = "linux")]
    let desk = CopyOnly;
    let enabled = ibis_core::settings::load(&app.state::<crate::preferences::SettingsFile>().path)
        .settings
        .paste
        .unwrap_or(true);
    deliver(&desk, proof.as_ref(), enabled)
}

/// Linux captures no target, so it only copies.
#[cfg(target_os = "linux")]
struct CopyOnly;

#[cfg(target_os = "linux")]
impl Desk for CopyOnly {
    type Proof = CopyProof;

    fn may_post(&self) -> bool {
        false
    }
    fn ask_to_post(&self) {}
    fn frontmost(&self) -> Result<Option<Front>, CopyReason> {
        Ok(None)
    }
    fn finish(&self, _: DeliveryRequest, _: &CopyProof) -> PostResult {
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
        post_event_granted: Cell<bool>,
        asked: Cell<u32>,
        fronts: RefCell<Vec<i32>>,
        clock: Cell<Instant>,
        stamp: Cell<SafetyStamp>,
        window: Cell<Option<u32>>,
        focus_error: Cell<Option<CopyReason>>,
        presentation: Cell<u64>,
        hidden: bool,
        change_count: Cell<isize>,
        secure: Cell<Option<bool>>,
        query_delay: Duration,
        queue_delay: Duration,
        secure_after_queue: Option<bool>,
        front_after_queue: Option<i32>,
        replace_during_check: bool,
        after_check: Option<fn(&Fake)>,
        after_window_query: Option<fn(&Fake)>,
        in_final_query: Cell<bool>,
        replace_on_refresh_stamp: bool,
        focus_after_final_check: bool,
        posted_windows: RefCell<Vec<Option<u32>>>,
        checks: RefCell<Vec<&'static str>>,
        posts: RefCell<Vec<i32>>,
        pending: PendingPost,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                may_post: true,
                post_event_granted: Cell::new(true),
                asked: Cell::new(0),
                fronts: RefCell::new(vec![42]),
                clock: Cell::new(Instant::now()),
                stamp: Cell::new(SafetyStamp {
                    epoch: 1,
                    elapsed: Duration::ZERO,
                }),
                window: Cell::new(Some(100)),
                focus_error: Cell::new(None),
                presentation: Cell::new(PRESENTATION),
                hidden: true,
                change_count: Cell::new(COUNT),
                secure: Cell::new(Some(false)),
                query_delay: Duration::ZERO,
                queue_delay: Duration::ZERO,
                secure_after_queue: None,
                front_after_queue: None,
                replace_during_check: false,
                after_check: None,
                after_window_query: None,
                in_final_query: Cell::new(false),
                replace_on_refresh_stamp: false,
                focus_after_final_check: false,
                posted_windows: RefCell::default(),
                checks: RefCell::default(),
                posts: RefCell::default(),
                pending: PendingPost::default(),
            }
        }

        fn front(&self) -> Option<Front> {
            self.fronts.borrow().first().map(|&pid| Front {
                id: Some("com.microsoft.word".into()),
                pid,
                window: self.window.get(),
                focus_failure: None,
                name: None,
                elevated: false,
            })
        }
    }

    impl Desk for Fake {
        type Proof = CopyProof;

        fn stamp(&self) -> SafetyStamp {
            self.stamp.get()
        }

        fn may_post(&self) -> bool {
            self.may_post
        }
        fn ask_to_post(&self) {
            self.asked.set(self.asked.get() + 1);
        }
        fn frontmost(&self) -> Result<Option<Front>, CopyReason> {
            Ok(self.front())
        }
        fn finish(&self, request: DeliveryRequest, proof: &CopyProof) -> PostResult {
            let request = PostRequest::new(request, proof).queued(self.clock.get());
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

    // The fake never needs role, title, value or clipboard-content APIs.
    impl PostDesk for Fake {
        fn post_event_granted(&self) -> bool {
            self.post_event_granted.get()
        }
        fn now(&self) -> Instant {
            self.clock.get()
        }
        fn stamp(&self) -> SafetyStamp {
            if self.replace_on_refresh_stamp && self.in_final_query.get() {
                self.change_count.set(COUNT + 1);
            }
            if self.focus_after_final_check
                && self
                    .checks
                    .borrow()
                    .iter()
                    .filter(|&&check| check == "foreground")
                    .count()
                    >= 2
            {
                self.window.set(Some(200));
            }
            self.stamp.get()
        }
        fn current(&self, presentation: u64) -> bool {
            self.hidden && self.presentation.get() == presentation
        }
        fn frontmost(&self, refresh: impl FnOnce() -> Result<(), CopyReason>) -> Result<Option<Front>, CopyReason> {
            if self.checks.borrow().contains(&"foreground") {
                self.in_final_query.set(true);
            }
            if self.checks.borrow().contains(&"foreground")
                && let Some(change) = self.after_window_query
            {
                change(self);
            }
            if let Some(reason) = self.focus_error.get() {
                return Err(reason);
            }
            refresh()?;
            self.checks.borrow_mut().push("foreground");
            Ok(self.front())
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
            if let Some(change) = self.after_check {
                change(self);
            }
            self.secure.get()
        }
        fn post(&self, pid: i32) -> bool {
            self.posted_windows.borrow_mut().push(self.window.get());
            self.checks.borrow_mut().push("post");
            self.posts.borrow_mut().push(pid);
            true
        }
    }

    // This fake exercises macOS capture and PostEvent on every test host.
    fn writer(id: &str) -> Captured {
        Captured {
            target: Target {
                system: System::Macos,
                app: Some(AppId {
                    system: System::Macos,
                    id: id.into(),
                }),
                secure_field: false,
                elevated: false,
            },
            pid: Some(42),
            window: Some(100),
            focus_failure: None,
            name: Some("Writer".into()),
        }
    }

    #[test]
    fn the_footer_uses_delivery_policy_without_asking_for_permission_or_posting() {
        let permission = PastePermission {
            enabled: true,
            granted: true,
        };
        let writer = writer("com.microsoft.word");
        assert!(intended_paste(Some(&writer), permission));
        assert!(!intended_paste(None, permission));
        let mut unproven_window = writer.clone();
        unproven_window.window = None;
        assert!(!intended_paste(Some(&unproven_window), permission));
        assert!(!intended_paste(
            Some(&writer),
            PastePermission {
                enabled: false,
                ..permission
            }
        ));
        assert!(!intended_paste(
            Some(&writer),
            PastePermission {
                granted: false,
                ..permission
            }
        ));
        for system in [System::Macos, System::Windows, System::Linux] {
            let mut target = writer.clone();
            target.target.system = system;
            target.target.app = Some(AppId {
                system,
                id: if system == System::Windows {
                    "winword.exe"
                } else {
                    "com.microsoft.word"
                }
                .into(),
            });
            assert_eq!(intended_paste(Some(&target), permission), system != System::Linux);
            target.target.elevated = true;
            assert!(!intended_paste(Some(&target), permission));
            target.target.elevated = false;
            target.target.secure_field = true;
            assert!(!intended_paste(Some(&target), permission));
        }
        for (system, id) in [(System::Macos, "com.apple.Terminal"), (System::Windows, "mstsc.exe")] {
            let mut target = writer.clone();
            target.target.system = system;
            target.target.app = Some(AppId { system, id: id.into() });
            assert!(!intended_paste(Some(&target), permission));
        }
    }

    fn proof() -> CopyProof {
        CopyProof {
            token: 1,
            captured: Some(writer("com.microsoft.word")),
            text: TEXT.into(),
            change_count: COUNT,
            presentation: PRESENTATION,
            written: Instant::now(),
            stamp: SafetyStamp {
                epoch: 1,
                elapsed: Duration::ZERO,
            },
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
    fn an_editor_with_verified_window_identity_pastes_once_after_returning() {
        let desk = Fake::new();
        desk.fronts.replace(vec![7, 7, 42]);
        assert_eq!(deliver(&desk, Some(&proof()), true), pasted());
        assert_eq!(*desk.posts.borrow(), [42]);
        assert_eq!(
            *desk.checks.borrow(),
            [
                "foreground",
                "secure-input",
                "change-count",
                "secure-input",
                "change-count",
                "foreground",
                "post"
            ]
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn repeated_show_from_our_key_panel_still_pastes_once_into_the_original_external_window() {
        let desk = Fake::new();
        let last = LastTarget::default();
        last.begin_presentation(Some(writer("com.microsoft.word")));
        let active_external = Front {
            id: Some("com.microsoft.word".into()),
            pid: 42,
            window: None,
            focus_failure: None,
            name: Some("Writer".into()),
            elevated: false,
        };
        let own_key = macos::capture_front(Some(active_external), 7, true, |_| {
            panic!("own panel must keep AX bank")
        });
        last.begin_presentation(capture::captured(SYSTEM, own_key, 7));
        let token = last
            .write_copy_at(TEXT, desk.stamp.get(), || Ok(Some(COUNT)))
            .unwrap_or_else(|error| panic!("{error}"));
        let proof = last.take_copy(TEXT, token).unwrap_or_else(|| panic!("proof"));
        desk.presentation.set(proof.presentation);
        assert_eq!(deliver(&desk, Some(&proof), true), pasted());
        assert_eq!(*desk.posts.borrow(), [42]);
        assert_eq!(*desk.posted_windows.borrow(), [Some(100)]);
        assert_eq!(proof.text, TEXT);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn granting_access_cannot_repair_a_failed_capture_without_a_fresh_external_activation() {
        for (reason, message) in [
            (CopyReason::AccessibilityRequired, "accessibility-required"),
            (CopyReason::NoPermission, "no-permission"),
        ] {
            let desk = Fake::new(); // both authorizations are now present
            let last = LastTarget::default();
            let external = || Front {
                id: Some("com.microsoft.word".into()),
                pid: 42,
                window: None,
                focus_failure: None,
                name: Some("Writer".into()),
                elevated: false,
            };
            let failed = macos::capture_front(Some(external()), 7, false, |_| Err(reason));
            last.begin_presentation(capture::captured(System::Macos, failed, 7));
            let copy = || {
                let token = last
                    .write_copy_at(TEXT, desk.stamp.get(), || Ok(Some(COUNT)))
                    .unwrap_or_else(|error| panic!("{error}"));
                last.take_copy(TEXT, token).unwrap_or_else(|| panic!("proof"))
            };
            for _fresh_enter_in_failed_presentation in 0..2 {
                let proof = copy();
                assert_eq!(deliver(&desk, Some(&proof), true), copied(Some(message)));
                assert_eq!(proof.text, TEXT);
                assert!(desk.posts.borrow().is_empty());
            }
            if reason == CopyReason::AccessibilityRequired {
                assert_eq!(desk.asked.get(), 0);
            }
            let valid = macos::capture_front(Some(external()), 7, false, |_| Ok(100));
            last.begin_presentation(capture::captured(System::Macos, valid, 7));
            let proof = copy();
            desk.presentation.set(proof.presentation);
            assert_eq!(deliver(&desk, Some(&proof), true), pasted());
            assert_eq!(*desk.posted_windows.borrow(), [Some(100)]);
        }
    }

    #[test]
    fn nothing_that_can_wait_runs_between_the_last_window_check_and_posting() {
        let desk = Fake {
            focus_after_final_check: true,
            ..Fake::new()
        };
        assert_eq!(deliver(&desk, Some(&proof()), true), pasted());
        assert_eq!(
            *desk.posted_windows.borrow(),
            [Some(100)],
            "Command-V must not reach window B"
        );
        assert_eq!(
            &desk.checks.borrow()[desk.checks.borrow().len() - 2..],
            ["foreground", "post"]
        );
    }

    #[test]
    fn unavailable_safety_evidence_is_not_reported_as_expiry() {
        for stamp in [
            SafetyStamp {
                epoch: 0,
                elapsed: Duration::ZERO,
            },
            SafetyStamp {
                epoch: u64::MAX,
                elapsed: Duration::ZERO,
            },
        ] {
            let desk = Fake {
                stamp: Cell::new(stamp),
                ..Fake::new()
            };
            assert_eq!(deliver(&desk, Some(&proof()), true), copied(Some("paste-unavailable")));
            assert!(desk.posts.borrow().is_empty());
        }
    }

    #[test]
    fn final_window_proof_failure_keeps_its_own_delivery_reason() {
        for (failure, reason) in [
            (CopyReason::PasteUnavailable, "paste-unavailable"),
            (CopyReason::TargetChanged, "target-changed"),
            (CopyReason::PasteExpired, "paste-expired"),
            (CopyReason::AccessibilityRequired, "accessibility-required"),
        ] {
            let desk = Fake {
                focus_error: Cell::new(Some(failure)),
                ..Fake::new()
            };
            let proof = proof();
            assert_eq!(deliver(&desk, Some(&proof), true), copied(Some(reason)));
            assert_eq!(proof.text, TEXT);
            assert!(desk.posts.borrow().is_empty());
        }
        let desk = Fake {
            after_check: Some(|desk| desk.focus_error.set(Some(CopyReason::PasteUnavailable))),
            ..Fake::new()
        };
        assert_eq!(deliver(&desk, Some(&proof()), true), copied(Some("paste-unavailable")));
        assert!(desk.posts.borrow().is_empty());
    }

    #[test]
    fn a_changed_closed_minimised_or_unproven_window_never_receives_the_paste() {
        for window in [Some(200), None] {
            let desk = Fake {
                window: Cell::new(window),
                ..Fake::new()
            };
            let proof = proof();
            assert_eq!(deliver(&desk, Some(&proof), true), copied(Some("target-changed")));
            assert!(desk.posts.borrow().is_empty());
            assert_eq!(
                proof.text, TEXT,
                "the successful copy remains available for manual paste"
            );
        }
    }

    #[test]
    fn an_unproven_mac_window_never_prompts_for_irrelevant_post_event_permission() {
        for granted in [false, true] {
            let desk = Fake {
                may_post: granted,
                ..Fake::new()
            };
            let mut proof = proof();
            let target = proof.captured.as_mut().unwrap_or_else(|| panic!("target"));
            target.target.system = System::Macos;
            target.target.app = Some(AppId {
                system: System::Macos,
                id: "com.microsoft.word".into(),
            });
            target.window = None;
            assert_eq!(deliver(&desk, Some(&proof), true), copied(Some("paste-unavailable")));
            assert_eq!(desk.asked.get(), 0);
            assert!(desk.posts.borrow().is_empty());
        }
    }

    #[test]
    fn sleep_lock_and_user_switch_end_a_pending_paste_even_if_instant_did_not_advance() {
        for _event in ["sleep", "lock", "user-switch"] {
            let desk = Fake::new();
            let proof = proof();
            desk.stamp.set(SafetyStamp {
                epoch: proof.stamp.epoch + 1,
                ..proof.stamp
            });
            assert_eq!(deliver(&desk, Some(&proof), true), copied(Some("paste-expired")));
            assert!(desk.posts.borrow().is_empty());
            assert_eq!(proof.text, TEXT);
        }
    }

    #[test]
    fn a_sleep_aware_clock_expires_delivery_when_the_active_clock_pauses() {
        let desk = Fake::new();
        let proof = proof();
        desk.stamp.set(SafetyStamp {
            elapsed: proof.stamp.elapsed + LATE,
            ..proof.stamp
        });
        assert_eq!(deliver(&desk, Some(&proof), true), copied(Some("paste-expired")));
        assert!(desk.posts.borrow().is_empty());
    }

    #[test]
    fn a_lifecycle_change_during_the_clipboard_write_cannot_mint_fresh_delivery() {
        let last = LastTarget::default();
        last.begin_presentation(Some(writer("com.microsoft.word")));
        let desk = Fake::new();
        let token = last
            .write_copy_at(TEXT, desk.stamp.get(), || {
                desk.stamp.set(SafetyStamp {
                    epoch: 2,
                    elapsed: Duration::ZERO,
                });
                Ok(Some(COUNT))
            })
            .unwrap_or_else(|e| panic!("{e}"));
        let proof = last.take_copy(TEXT, token).unwrap_or_else(|| panic!("proof"));
        assert_eq!(proof.stamp.epoch, 1);
        assert!(!proof.stamp.valid_at(desk.stamp.get()));
        assert_eq!(proof.text, TEXT);
        assert!(desk.posts.borrow().is_empty());
    }

    #[test]
    fn a_superseded_claimed_worker_cannot_post_even_if_citation_and_counter_match() {
        let last = LastTarget::default();
        last.begin_presentation(Some(writer("com.microsoft.word")));
        let copy = || {
            let token = last
                .write_copy(TEXT, || Ok(Some(COUNT)))
                .unwrap_or_else(|e| panic!("{e}"));
            last.take_copy(TEXT, token).unwrap_or_else(|| panic!("proof"))
        };
        let request = |proof: &CopyProof| {
            PostRequest::new(
                DeliveryRequest {
                    pid: 42,
                    id: "com.microsoft.word".into(),
                    deadline: proof.written + LATE,
                },
                proof,
            )
        };
        let a = copy();
        let b = copy();
        assert_eq!(
            last.post_if_current(&request(&a), || panic!("stale A")),
            PostResult::Refused
        );
        assert_eq!(
            last.post_if_current(&request(&b), || PostResult::Pasted),
            PostResult::Pasted
        );
        assert_eq!(
            last.post_if_current(&request(&b), || panic!("repeated B")),
            PostResult::Refused
        );
        last.begin_presentation(None);
        assert_eq!(
            last.post_if_current(&request(&a), || panic!("reopened A")),
            PostResult::Refused
        );
    }

    #[test]
    fn target_and_lifecycle_are_rechecked_after_the_native_queries() {
        for change in [
            (|desk: &Fake| desk.window.set(Some(200))) as fn(&Fake),
            |desk: &Fake| {
                desk.stamp.set(SafetyStamp {
                    epoch: desk.stamp.get().epoch + 1,
                    ..desk.stamp.get()
                })
            },
        ] {
            let desk = Fake {
                after_check: Some(change),
                ..Fake::new()
            };
            assert!(matches!(
                deliver(&desk, Some(&proof()), true),
                Delivered::Copied { reason: Some(_) }
            ));
            assert!(desk.posts.borrow().is_empty());
        }
    }

    #[test]
    fn a_busy_clipboard_owner_refuses_instead_of_blocking_the_main_thread_post() {
        let last = LastTarget::default();
        let proof = proof();
        let request = PostRequest::new(
            DeliveryRequest {
                pid: 42,
                id: "com.microsoft.word".into(),
                deadline: proof.written + LATE,
            },
            &proof,
        );
        let _writing = last.0.lock().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            last.post_if_current(&request, || panic!("unowned post")),
            PostResult::Unavailable
        );
    }

    #[test]
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
    fn clipboard_replaced_during_the_final_check_is_never_read_or_pasted() {
        let desk = Fake {
            replace_during_check: true,
            ..Fake::new()
        };
        assert_eq!(deliver(&desk, Some(&proof()), true), copied(Some("clipboard-changed")));
        assert!(desk.posts.borrow().is_empty());
        assert_eq!(*desk.checks.borrow(), ["foreground", "secure-input", "change-count"]);
    }

    #[test]
    fn clipboard_and_secure_input_are_refreshed_after_the_final_ax_provider_wait() {
        for (change, reason) in [
            (
                (|desk: &Fake| desk.change_count.set(COUNT + 1)) as fn(&Fake),
                "clipboard-changed",
            ),
            ((|desk: &Fake| desk.secure.set(Some(true))) as fn(&Fake), "secure-input"),
        ] {
            let desk = Fake {
                after_window_query: Some(change),
                ..Fake::new()
            };
            assert_eq!(deliver(&desk, Some(&proof()), true), copied(Some(reason)));
            assert!(desk.posts.borrow().is_empty());
        }
    }

    #[test]
    fn lifecycle_refresh_cannot_leave_an_older_clipboard_snapshot_before_the_final_focus_pair() {
        let desk = Fake {
            replace_on_refresh_stamp: true,
            ..Fake::new()
        };
        assert_eq!(deliver(&desk, Some(&proof()), true), copied(Some("clipboard-changed")));
        assert!(desk.posts.borrow().is_empty());
    }

    #[test]
    fn revoking_post_event_during_provider_wait_preserves_its_permission_refusal() {
        let desk = Fake {
            after_window_query: Some(|desk| desk.post_event_granted.set(false)),
            ..Fake::new()
        };
        assert_eq!(deliver(&desk, Some(&proof()), true), copied(Some("no-permission")));
        assert!(desk.posts.borrow().is_empty());
    }

    #[test]
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
    fn final_task_revalidates_foreground_picker_presentation_and_visibility() {
        for (desk, reason) in [
            (
                Fake {
                    front_after_queue: Some(7),
                    ..Fake::new()
                },
                Some("target-changed"),
            ),
            (
                Fake {
                    presentation: Cell::new(PRESENTATION + 1),
                    ..Fake::new()
                },
                None,
            ),
            (
                Fake {
                    hidden: false,
                    ..Fake::new()
                },
                None,
            ),
        ] {
            assert_eq!(deliver(&desk, Some(&proof()), true), copied(reason));
            assert!(desk.posts.borrow().is_empty());
        }
    }

    #[test]
    fn overlapping_copy_deliveries_leave_the_newer_proof_available() {
        for text_a in ["[@first]", TEXT] {
            let last = LastTarget::default();
            let desk = Fake::new();
            last.begin_presentation(Some(writer("com.microsoft.word")));
            let token_a = last
                .write_copy_at(text_a, desk.stamp.get(), || Ok(Some(COUNT)))
                .unwrap_or_else(|e| panic!("{e}"));
            // A's worker is queued but has not taken its proof yet.
            let delivery_a = || {
                let proof = last.take_copy(text_a, token_a);
                deliver(&desk, proof.as_ref(), true)
            };
            last.begin_presentation(None);
            let token_b = last
                .write_copy_at(TEXT, desk.stamp.get(), || Ok(Some(COUNT + 1)))
                .unwrap_or_else(|e| panic!("{e}"));
            assert_ne!(token_a, token_b);
            desk.presentation.set(2);
            desk.change_count.set(COUNT + 1);
            let reason = if SYSTEM == System::Linux {
                "unsupported-system"
            } else {
                "no-target"
            };
            assert_eq!(delivery_a(), copied(Some(reason)));
            assert!(desk.posts.borrow().is_empty());
            let proof_b = last.take_copy(TEXT, token_b);
            assert_eq!(deliver(&desk, proof_b.as_ref(), true), pasted());
            assert!(last.take_copy(TEXT, token_b).is_none());
            assert_eq!(*desk.posts.borrow(), [42]);
        }
    }

    #[test]
    fn only_our_write_supplies_a_snapshot_and_the_proof_is_consumed_once() {
        let last = LastTarget::default();
        {
            let mut state = last.0.lock().unwrap_or_else(|e| panic!("{e}"));
            state.captured = Some(writer("com.microsoft.word"));
        }
        let token = last
            .write_copy(TEXT, || Ok(Some(COUNT)))
            .unwrap_or_else(|e| panic!("{e}"));
        let ticket = last.take_copy(TEXT, token).unwrap_or_else(|| panic!("copy proof"));
        assert_eq!(ticket.change_count, COUNT);
        assert_eq!(ticket.presentation, 0);
        assert_eq!(ticket.captured, Some(writer("com.microsoft.word")));
        assert!(last.take_copy(TEXT, token).is_none());
        let token = last
            .write_copy(TEXT, || Ok(Some(COUNT)))
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(last.take_copy("different citation", token).is_none());
        assert!(last.take_copy(TEXT, None).is_none());
        assert!(last.take_copy(TEXT, token).is_some());
        // A failed write or a concurrent replacement cannot reuse an old proof.
        for result in [Err("write failed".into()), Ok(None)] {
            let token = last
                .write_copy(TEXT, || Ok(Some(COUNT)))
                .unwrap_or_else(|e| panic!("{e}"));
            let _ = last.write_copy(TEXT, || result);
            assert!(last.take_copy(TEXT, token).is_none());
        }
    }

    #[test]
    fn delivery_deadline_starts_at_the_own_write_not_at_notice_or_queue_time() {
        let mut stale = proof();
        stale.written -= LATE;
        let desk = Fake::new();
        assert_eq!(deliver(&desk, Some(&stale), true), copied(Some("paste-expired")));
        assert!(desk.posts.borrow().is_empty());
    }

    #[test]
    fn a_new_presentation_invalidates_delivery_even_when_it_keeps_the_same_target() {
        let last = LastTarget::default();
        last.begin_presentation(Some(writer("com.microsoft.word")));
        let token = last
            .write_copy(TEXT, || Ok(Some(COUNT)))
            .unwrap_or_else(|e| panic!("{e}"));
        let ticket = last.take_copy(TEXT, token).unwrap_or_else(|| panic!("copy proof"));
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
        assert_eq!(ticket.captured, Some(writer("com.microsoft.word")));
    }

    #[test]
    fn unsafe_unproven_and_disabled_targets_only_copy() {
        let no_target = if SYSTEM == System::Linux {
            "unsupported-system"
        } else {
            "no-target"
        };
        for (captured, enabled, reason) in [
            (Some(writer("org.example.other")), true, Some("unknown-app")),
            (Some(writer("com.microsoft.word")), false, Some("paste-off")),
            (None, true, Some(no_target)),
        ] {
            let desk = Fake::new();
            let ticket = CopyProof { captured, ..proof() };
            assert_eq!(deliver(&desk, Some(&ticket), enabled), copied(reason));
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

#[cfg(test)]
mod windows_tests {
    use super::*;
    use ibis_core::destination::AppId;
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;

    struct Fake {
        may_post: bool,
        stamp: Cell<SafetyStamp>,
        asked: Cell<u32>,
        /// Front-most process after each wait.
        fronts: RefCell<Vec<i32>>,
        front_id: Cell<&'static str>,
        clipboard_reads: Cell<u32>,
        password: Option<bool>,
        password_queries: Option<std::sync::Arc<PasswordQueries>>,
        after_password: Option<fn(&Fake)>,
        after_focus_wait: Option<fn(&Fake)>,
        front_queries: Cell<u32>,
        after_front_query: Option<fn(&Fake)>,
        posts: RefCell<Vec<(i32, u16)>>,
        generation: Cell<u64>,
        hidden: Cell<bool>,
        window: Cell<usize>,
        restorable: Cell<bool>,
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
                stamp: Cell::new(SafetyStamp {
                    epoch: 1,
                    elapsed: Duration::ZERO,
                }),
                asked: Cell::new(0),
                fronts: RefCell::new(vec![42]),
                front_id: Cell::new("winword.exe"),
                clipboard_reads: Cell::new(0),
                password: Some(false),
                password_queries: None,
                after_password: None,
                after_focus_wait: None,
                front_queries: Cell::new(0),
                after_front_query: None,
                posts: RefCell::default(),
                generation: Cell::new(11),
                hidden: Cell::new(true),
                window: Cell::new(100),
                restorable: Cell::new(true),
                sequence: Cell::new(Some(if clipboard == "other text" { 18 } else { 17 })),
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

    impl TicketDesk for Fake {
        type Window = usize;
        fn stamp(&self) -> SafetyStamp {
            self.stamp.get()
        }

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
            self.front_queries.set(self.front_queries.get() + 1);
            let front = self.fronts.borrow().first().map(|&pid| Front {
                id: Some(self.front_id.get().into()),
                pid,
                window: None,
                focus_failure: None,
                name: None,
                elevated: false,
            });
            if let Some(change) = self.after_front_query {
                change(self);
            }
            front
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
        fn wait(&self, _: Duration) {
            let mut fronts = self.fronts.borrow_mut();
            if fronts.len() > 1 {
                fronts.remove(0);
            }
            if let Some(change) = self.after_focus_wait {
                change(self);
            }
        }

        fn generation(&self) -> u64 {
            self.generation.get()
        }
        fn picker_hidden(&self) -> bool {
            self.hidden.get()
        }
        fn restore(&self, window: &usize, pid: i32) -> bool {
            self.restores.borrow_mut().push((*window, pid));
            self.restorable.get()
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
        DeliveryTicket {
            token: 17,
            target: CopyTarget {
                captured: Some(writer("winword.exe")),
                window: Some(100),
                generation: 10,
            },
            sequence: 17,
            written: Instant::now(),
            stamp: SafetyStamp {
                epoch: 1,
                elapsed: Duration::ZERO,
            },
        }
    }

    fn deliver(desk: &Fake, captured: Option<&Captured>, text: &str, enabled: bool) -> Delivered {
        let ticket = DeliveryTicket {
            target: CopyTarget {
                captured: captured.cloned(),
                ..ticket().target
            },
            ..ticket()
        };
        deliver_ticket(desk, &ticket, text, enabled)
    }

    fn writer(id: &str) -> Captured {
        // Match windows::front: its HWND belongs to CopyTarget, not Front.
        capture::captured(
            System::Windows,
            Some(Front {
                id: Some(id.into()),
                pid: 42,
                window: None,
                focus_failure: None,
                name: Some("Writer".into()),
                elevated: false,
            }),
            7,
        )
        .unwrap_or_else(|| panic!("external Windows capture"))
    }

    fn copied(reason: Option<&str>) -> Delivered {
        Delivered::Copied {
            reason: reason.map(str::to_owned),
        }
    }

    #[test]
    fn enter_posts_exactly_one_paste_to_the_captured_app_once_it_is_back_in_front() {
        for id in ["winword.exe", "WINWORD.EXE", "notepad.exe"] {
            let desk = Fake::new("[@synthetic]");
            desk.front_id.set(id);
            // The picker is still in front for two checks.
            desk.fronts.replace(vec![7, 7, 42]);
            let captured = writer(id);
            assert_eq!(captured.window, None, "Windows keeps its HWND in the ticket");
            assert!(intended_paste(
                Some(&captured),
                PastePermission {
                    enabled: true,
                    granted: true,
                }
            ));
            assert_eq!(
                deliver(&desk, Some(&captured), "[@synthetic]", true),
                Delivered::Pasted {
                    app: Some("Writer".into())
                }
            );
            assert_eq!(*desk.restores.borrow(), [(100, 42)]);
            assert_eq!(*desk.posts.borrow(), [(42, 0x09)]);
            assert_eq!(desk.asked.get(), 0);
        }
    }

    #[test]
    fn nothing_is_posted_when_the_paste_is_unsafe_or_unproven() {
        let cases: [(Fake, Captured, bool, Delivered); 6] = [
            // The process number now belongs to another app.
            (
                Fake::new("[@synthetic]"),
                writer("org.example.other"),
                true,
                copied(Some("unknown-app")),
            ),
            // A macOS bundle ID does not describe a known Windows executable.
            (
                Fake::new("[@synthetic]"),
                writer("com.microsoft.word"),
                true,
                copied(Some("unknown-app")),
            ),
            // Another app replaced the clipboard.
            (
                Fake::new("other text"),
                writer("winword.exe"),
                true,
                copied(Some("clipboard-changed")),
            ),
            // The switch is off.
            (
                Fake::new("[@synthetic]"),
                writer("winword.exe"),
                false,
                copied(Some("paste-off")),
            ),
            // The captured app never came back.
            (
                Fake {
                    fronts: RefCell::new(vec![7]),
                    ..Fake::new("[@synthetic]")
                },
                writer("winword.exe"),
                true,
                copied(Some("target-changed")),
            ),
            // Nothing was in front.
            (
                Fake::new("[@synthetic]"),
                Captured::unknown(System::Windows),
                true,
                copied(Some("no-target")),
            ),
        ];
        for (desk, captured, enabled, expected) in cases {
            assert_eq!(deliver(&desk, Some(&captured), "[@synthetic]", enabled), expected);
            assert!(desk.posts.borrow().is_empty());
        }
    }

    #[test]
    fn password_fields_and_apps_with_more_rights_copy_with_a_reason() {
        let desk = Fake {
            password: Some(true),
            ..Fake::new("[@synthetic]")
        };
        assert_eq!(
            deliver(&desk, Some(&writer("winword.exe")), "[@synthetic]", true),
            copied(Some("secure-field"))
        );
        assert!(desk.posts.borrow().is_empty());

        let desk = Fake::new("[@synthetic]");
        let mut admin = writer("winword.exe");
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
        assert_eq!(
            deliver_ticket(&desk, &frozen, "[@synthetic]", true),
            copied(Some("target-changed"))
        );
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
        let mut pending = Some(DeliveryTicket { token: 18, ..ticket() });
        assert!(take_ticket(&mut pending, Some(17)).is_none());
        assert!(take_ticket(&mut pending, None).is_none());
        assert!(take_ticket(&mut pending, Some(18)).is_some());
        assert!(take_ticket(&mut pending, Some(18)).is_none());
    }

    #[test]
    fn overlapping_copy_deliveries_leave_the_newer_windows_ticket_available() {
        for text_a in ["[@first]", "[@synthetic]"] {
            let desk = Fake::new("[@synthetic]");
            let pending = RefCell::new(None);
            let token_a = remember_ticket(
                &mut pending.borrow_mut(),
                CopyAttempt {
                    target: Some(ticket().target),
                    written: Instant::now(),
                    stamp: desk.stamp.get(),
                },
                Some(17),
            );
            // Delay A's claim until after B has copied on a new presentation.
            let delivery = |token, text| {
                let frozen = take_ticket(&mut pending.borrow_mut(), token);
                frozen
                    .as_ref()
                    .map_or_else(|| copied(None), |ticket| deliver_ticket(&desk, ticket, text, true))
            };
            let target_b = CopyTarget {
                generation: 12,
                ..ticket().target
            };
            // Even a repeated native sequence cannot alias a write token.
            let token_b = remember_ticket(
                &mut pending.borrow_mut(),
                CopyAttempt {
                    target: Some(target_b),
                    written: Instant::now(),
                    stamp: desk.stamp.get(),
                },
                Some(17),
            );
            assert_ne!(token_a, token_b);
            desk.generation.set(13);
            assert_eq!(delivery(token_a, text_a), copied(None));
            assert!(desk.restores.borrow().is_empty());
            assert!(desk.posts.borrow().is_empty());
            assert_eq!(
                delivery(token_b, "[@synthetic]"),
                Delivered::Pasted {
                    app: Some("Writer".into())
                }
            );
            assert_eq!(delivery(token_b, "[@synthetic]"), copied(None));
            assert_eq!(*desk.posts.borrow(), [(42, 0x09)]);
            assert_eq!(desk.clipboard_reads.get(), 0);
        }
    }

    #[test]
    fn a_foreign_clipboard_owner_is_never_read_or_adopted_as_the_baseline() {
        for sequence in [Some(18), None] {
            let desk = Fake::new("foreign delayed text");
            let frozen = ticket(); // Sequence 17, sampled during Ibis's write.
            // Replacement occurs before the worker even starts.
            desk.sequence.set(sequence);
            assert_eq!(
                deliver_ticket(&desk, &frozen, "[@synthetic]", true),
                copied(Some(if sequence.is_some() {
                    "clipboard-changed"
                } else {
                    "paste-unavailable"
                }))
            );
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
    fn a_replaced_or_unavailable_clipboard_has_its_own_refusal_reason() {
        for (sequence, reason) in [(Some(18), "clipboard-changed"), (None, "paste-unavailable")] {
            let desk = Fake::new("[@synthetic]");
            desk.sequence.set(sequence);
            assert_eq!(
                deliver_ticket(&desk, &ticket(), "[@synthetic]", true),
                copied(Some(reason))
            );
            assert!(desk.events.borrow().is_empty());
            assert!(desk.restores.borrow().is_empty());
        }
    }

    #[test]
    fn clipboard_failures_while_waiting_for_focus_keep_their_exact_reason() {
        for (sequence, reason) in [(Some(18), "clipboard-changed"), (None, "paste-unavailable")] {
            let desk = Fake {
                after_focus_wait: Some(|desk| desk.sequence.set(None)),
                ..Fake::new("[@synthetic]")
            };
            let mut desk = desk;
            if sequence.is_some() {
                desk.after_focus_wait = Some(|desk| desk.sequence.set(Some(18)));
            }
            desk.fronts.replace(vec![7, 42]);
            assert_eq!(
                deliver_ticket(&desk, &ticket(), "[@synthetic]", true),
                copied(Some(reason))
            );
            assert!(desk.events.borrow().is_empty());
        }
    }

    #[test]
    fn expiry_during_final_target_queries_prevents_injection_even_when_the_target_is_unchanged() {
        let desk = Fake {
            after_front_query: Some(|desk| {
                if desk.front_queries.get() >= 4 {
                    desk.stamp.set(SafetyStamp {
                        elapsed: LATE,
                        ..desk.stamp.get()
                    });
                }
            }),
            ..Fake::new("[@synthetic]")
        };
        assert_eq!(
            deliver_ticket(&desk, &ticket(), "[@synthetic]", true),
            copied(Some("paste-expired"))
        );
        assert!(desk.events.borrow().is_empty());
    }

    #[test]
    fn suspension_during_clipboard_io_or_before_remembering_does_not_refresh_delivery_age() {
        for phase in ["clipboard write", "before remember_copy"] {
            let desk = Fake::new("[@synthetic]");
            let before_io = desk.stamp.get();
            let started = Instant::now();
            let attempt = CopyAttempt {
                target: Some(ticket().target),
                written: started,
                stamp: before_io,
            };
            // Clipboard I/O / publication completes only after resume.
            desk.stamp.set(SafetyStamp {
                elapsed: LATE,
                ..before_io
            });
            let mut pending = None;
            let token = remember_ticket(&mut pending, attempt, Some(17));
            let ticket = take_ticket(&mut pending, token).unwrap_or_else(|| panic!("ticket"));
            assert_eq!(
                deliver_ticket(&desk, &ticket, "[@synthetic]", true),
                copied(Some("paste-expired")),
                "{phase}"
            );
            assert_eq!(ticket.written, started);
            assert_eq!(ticket.stamp.elapsed, before_io.elapsed);
            assert!(desk.events.borrow().is_empty());
            assert!(desk.restores.borrow().is_empty());
        }
    }

    #[test]
    fn a_ticket_requires_its_window_and_the_single_copy_dismissal() {
        for (generation, hidden, window) in [(10, true, 100), (11, false, 100), (11, true, 200)] {
            let desk = Fake::new("[@synthetic]");
            desk.generation.set(generation);
            desk.hidden.set(hidden);
            desk.window.set(window);
            assert_eq!(
                deliver_ticket(&desk, &ticket(), "[@synthetic]", true),
                copied(Some("target-changed"))
            );
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
            assert_eq!(
                deliver_ticket(&desk, &ticket(), "[@synthetic]", false),
                copied(Some("paste-off"))
            );
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
        assert_eq!(
            deliver_ticket(&desk, &ticket(), "[@synthetic]", false),
            copied(Some("paste-off"))
        );
        assert_eq!(&desk.input_requests.borrow()[requests..], [vec![VUp]]);
        assert!(!desk.keys_unreleased());
        assert!(!desk.v_down.get());
    }

    #[test]
    fn recovery_rechecks_the_ticket_after_lock_and_retry_waits_before_new_key_downs() {
        let changes: [fn(&Fake); 5] = [
            |desk| desk.generation.set(13),
            |desk| desk.window.set(200),
            |desk| desk.sequence.set(Some(18)),
            |desk| {
                desk.fronts.replace(vec![84]);
            },
            |desk| desk.front_id.set("org.example.other"),
        ];
        for (index, change) in changes.into_iter().enumerate() {
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
            assert_eq!(
                deliver_ticket(&desk, &ticket(), "[@synthetic]", true),
                copied(Some(if index == 2 {
                    "clipboard-changed"
                } else {
                    "target-changed"
                }))
            );
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
        let mut ticket = ticket();
        ticket.written -= LATE;
        assert_eq!(
            deliver_ticket(&desk, &ticket, "[@synthetic]", true),
            copied(Some("paste-expired"))
        );
        assert!(desk.posts.borrow().is_empty());
        assert_eq!(*desk.input_requests.borrow(), [vec![PasteEvent::ControlUp]]);
        assert!(desk.restores.borrow().is_empty(), "expired work must not steal focus");
        assert!(!desk.keys_unreleased());
        assert!(!desk.control_down.get());
    }

    #[test]
    fn suspend_aware_expiry_precedes_focus_restoration_and_is_rechecked_after_queries() {
        let desk = Fake::new("[@synthetic]");
        desk.stamp.set(SafetyStamp {
            epoch: 1,
            elapsed: LATE,
        });
        assert_eq!(
            deliver_ticket(&desk, &ticket(), "[@synthetic]", true),
            copied(Some("paste-expired"))
        );
        assert!(desk.restores.borrow().is_empty());
        assert!(desk.events.borrow().is_empty());
        for stage in ["password", "injection lock"] {
            let mut desk = Fake::new("[@synthetic]");
            let expire = |desk: &Fake| {
                desk.stamp.set(SafetyStamp {
                    epoch: 1,
                    elapsed: LATE,
                })
            };
            if stage == "password" {
                desk.after_password = Some(expire);
            } else {
                desk.before_post = Some(expire);
            }
            assert_eq!(
                deliver_ticket(&desk, &ticket(), "[@synthetic]", true),
                copied(Some("paste-expired"))
            );
            assert!(desk.events.borrow().is_empty());
        }
    }

    #[test]
    fn closed_hidden_and_minimised_windows_cannot_be_restored_or_pasted_into() {
        for _state in ["closed", "hidden", "minimised"] {
            let desk = Fake::new("[@synthetic]");
            desk.restorable.set(false);
            assert_eq!(
                deliver_ticket(&desk, &ticket(), "[@synthetic]", true),
                copied(Some("target-changed"))
            );
            assert!(desk.posts.borrow().is_empty());
            assert!(desk.events.borrow().is_empty());
        }
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
        let changes: [fn(&Fake); 5] = [
            |desk| desk.window.set(200),
            |desk| desk.generation.set(13),
            |desk| desk.sequence.set(Some(18)),
            |desk| {
                desk.fronts.replace(vec![84]);
            },
            |desk| desk.front_id.set("org.example.other"),
        ];
        for (index, change, password) in changes
            .into_iter()
            .enumerate()
            .flat_map(|(index, change)| [Some(false), Some(true)].map(|password| (index, change, password)))
        {
            let desk = Fake {
                after_password: Some(change),
                password,
                ..Fake::new("[@synthetic]")
            };
            assert_eq!(
                deliver_ticket(&desk, &ticket(), "[@synthetic]", true),
                copied(Some(if index == 2 {
                    "clipboard-changed"
                } else {
                    "target-changed"
                }))
            );
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
            front_id: Cell::new("obsidian.exe"),
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
            id: "obsidian.exe".into(),
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
            deliver(&desk, Some(&writer("cmd.exe")), "[@synthetic]", true),
            copied(Some("terminal"))
        );
        let desk = Fake {
            may_post: false,
            ..Fake::new("[@synthetic]")
        };
        assert_eq!(
            deliver(&desk, Some(&writer("winword.exe")), "[@synthetic]", true),
            copied(Some("no-permission"))
        );
        assert_eq!(desk.asked.get(), 1);
        assert!(desk.posts.borrow().is_empty());
    }
}
