//! Enter pastes into the app the picker was opened from (#27, #1241).
//!
//! After a clean copy has hidden the picker, Ibis posts one ⌘V (macOS) or
//! Ctrl+V (Windows, #1133), only to the captured process, once that app is in
//! front again and the clipboard still holds the citation.
//! `ibis_core::destination::decide` says when it must only copy, and the
//! notice says why. The citation stays on the clipboard either way (ADR 0009).

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

#[cfg(any(target_os = "macos", test))]
use ibis_core::destination::AppPaste;
#[cfg(test)]
use ibis_core::destination::PastePermission;
use ibis_core::destination::{AppId, CopyReason, System, lookup};
use serde::Serialize;
use tauri::{AppHandle, Manager};

pub(crate) mod capture;
mod delivery;
mod destination;
mod input_fence;
mod keys;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(test)]
mod notepad_test;
pub(crate) mod permission;
mod queries;
mod tickets;
#[cfg(target_os = "windows")]
mod windows;

pub(crate) use capture::Captured;
#[cfg(any(target_os = "linux", test))]
use capture::Front;
#[cfg(any(target_os = "windows", test))]
use delivery::CopyEvidence;
#[cfg(target_os = "windows")]
use delivery::key_release_failed;
use delivery::{DeliveryRequest, Desk, deliver, with_paste_settings};
#[cfg(test)]
use destination::intended_paste;
use destination::paste_policy;
use input_fence::PostResult;
#[cfg(any(target_os = "macos", test))]
use input_fence::{InputPress, PendingPost, PostDesk, PostRequest, check_input_since, checked_post};
#[cfg(any(target_os = "windows", test))]
use keys::{PasteEvent, PasteKeys};
#[cfg(any(target_os = "windows", test))]
use queries::{PasswordQueries, QueryWorker, display_name};
#[cfg(any(target_os = "windows", test))]
use tickets::{TicketDesk, deliver_ticket};

#[derive(Clone, Default)]
struct PolicyProof {
    epoch: Arc<AtomicU64>,
    generation: u64,
}

impl PolicyProof {
    #[cfg(any(target_os = "macos", target_os = "windows", test))]
    fn capture(epoch: &Arc<AtomicU64>) -> Self {
        Self {
            epoch: Arc::clone(epoch),
            generation: epoch.load(Ordering::Acquire),
        }
    }
    fn current(&self) -> bool {
        self.generation != u64::MAX && self.generation == self.epoch.load(Ordering::Acquire)
    }
}

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

/// The app the picker was last opened from.
#[derive(Default)]
pub(crate) struct LastTarget(Mutex<TargetState>);

#[cfg(any(target_os = "windows", test))]
impl LastTarget {
    /// Final native checks must never dispatch to the UI thread while this
    /// guard is held. Contention refuses instead of queuing input behind IPC.
    fn post_if_ready(&self, post: impl FnOnce() -> bool) -> bool {
        self.0.try_lock().is_ok_and(|_policy| post())
    }
}

#[derive(Default)]
struct TargetState {
    policy: Arc<AtomicU64>,
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

impl TargetState {
    fn invalidate_policy(&mut self) {
        self.policy
            .store(self.policy.load(Ordering::Acquire).saturating_add(1), Ordering::Release);
        #[cfg(any(not(target_os = "windows"), test))]
        {
            self.owner = None;
            self.copied = None;
        }
        #[cfg(target_os = "windows")]
        {
            self.pending = None;
        }
    }
}

/// Serialize successful policy changes with proof publication and native
/// posting. Detached workers keep a revocable epoch after taking their proof.
pub(crate) fn change_destination<T>(
    app: &AppHandle,
    revoke: bool,
    change: impl FnOnce(Option<&Captured>) -> Result<T, String>,
) -> Result<T, String> {
    let state = app.state::<LastTarget>();
    let mut last = state.0.lock().map_err(|_| "the destination is unavailable")?;
    #[cfg(not(target_os = "windows"))]
    let captured = last.captured.as_ref();
    #[cfg(target_os = "windows")]
    let captured = last.target.as_ref().and_then(|target| target.captured.as_ref());
    let result = change(captured)?;
    if revoke {
        last.invalidate_policy();
    }
    Ok(result)
}

/// Evidence from Ibis's own successful clipboard write, consumed once. The
/// text is Ibis's citation, never read back from another clipboard owner.
#[cfg(any(not(target_os = "windows"), test))]
struct CopyProof {
    policy: PolicyProof,
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
        paste_for: impl FnOnce(&Captured) -> Result<Option<AppPaste>, CopyReason>,
        write: impl FnOnce() -> Result<Option<isize>, String>,
    ) -> Result<Option<u32>, String> {
        #[cfg(target_os = "macos")]
        let (stamp, diagnostic) = macos::observe_write_stamp();
        #[cfg(not(target_os = "macos"))]
        let stamp = SafetyStamp::now();
        self.write_copy_observed(
            text,
            stamp,
            paste_for,
            write,
            #[cfg(target_os = "macos")]
            diagnostic,
        )
    }

    #[cfg(test)]
    fn write_copy_at(
        &self,
        text: &str,
        stamp: SafetyStamp,
        paste_for: impl FnOnce(&Captured) -> Result<Option<AppPaste>, CopyReason>,
        write: impl FnOnce() -> Result<Option<isize>, String>,
    ) -> Result<Option<u32>, String> {
        self.write_copy_observed(
            text,
            stamp,
            paste_for,
            write,
            #[cfg(target_os = "macos")]
            None,
        )
    }

    #[cfg(any(target_os = "macos", test))]
    fn write_copy_observed(
        &self,
        text: &str,
        stamp: SafetyStamp,
        paste_for: impl FnOnce(&Captured) -> Result<Option<AppPaste>, CopyReason>,
        write: impl FnOnce() -> Result<Option<isize>, String>,
        #[cfg(target_os = "macos")] diagnostic: Option<macos::diagnostics::Snapshot>,
    ) -> Result<Option<u32>, String> {
        let mut last = self.0.lock().map_err(|_| "The clipboard could not be written.")?;
        last.copied = None;
        last.owner = None;
        let captured = last.captured.clone().map(|mut captured| {
            captured.paste = paste_for(&captured);
            #[cfg(target_os = "macos")]
            if captured.focus_failure.is_none()
                && (stamp.epoch == 0 || stamp.epoch == u64::MAX)
                && let Some(mut diagnostic) = diagnostic
            {
                diagnostic.inherit_activation(captured.diagnostic.as_ref());
                captured.diagnostic = Some(diagnostic);
            }
            captured
        });
        let token = crate::copy::write_token()?;
        let change_count = write()?;
        if let Some(change_count) = change_count {
            last.owner = Some(token);
            last.copied = Some(CopyProof {
                policy: PolicyProof::capture(&last.policy),
                token,
                captured,
                text: text.to_owned(),
                change_count,
                presentation: last.presentation,
                written: Instant::now(),
                stamp,
            });
        }
        Ok(Some(token))
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

    #[cfg(target_os = "macos")]
    fn owns(&self, request: &PostRequest) -> bool {
        self.0
            .try_lock()
            .is_ok_and(|last| last.presentation == request.presentation && last.owner == Some(request.token))
    }
}

/// Remember the app in front, before the picker shows. Ibis in front keeps the earlier capture.
pub(crate) fn capture(app: &AppHandle) {
    let own = i32::try_from(std::process::id()).unwrap_or(i32::MAX);
    #[cfg(target_os = "macos")]
    let front = macos::frontmost(app);
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
pub(crate) fn last_app(app: &AppHandle) -> Option<(AppId, String, Option<String>)> {
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
    Some((id, name, captured.app_identity))
}

/// The footer's intended action, from the same policy as delivery. This only
/// reads the captured target and permission; secure input is checked at delivery.
pub(crate) fn intends_paste(app: &AppHandle, enabled: bool) -> bool {
    paste_policy(app, enabled, false)
}

/// Whether an ordinary target could paste after explicit approval. It uses the
/// same proof and refusal policy, so unsafe/unproven apps never ask for approval.
pub(crate) fn may_choose_paste(app: &AppHandle, enabled: bool) -> bool {
    paste_policy(app, enabled, true)
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
    policy: PolicyProof,
    token: u32,
    target: CopyTarget<W>,
    sequence: u32,
    written: Instant,
    stamp: SafetyStamp,
}

/// Identity and time at request entry, before target locks or clipboard I/O.
#[cfg(any(target_os = "windows", test))]
pub(crate) struct CopyAttempt<W> {
    policy: PolicyProof,
    target: Option<CopyTarget<W>>,
    written: Instant,
    stamp: SafetyStamp,
}

#[cfg(any(target_os = "windows", test))]
impl<W> CopyEvidence for DeliveryTicket<W> {
    fn policy_current(&self) -> bool {
        self.policy.current()
    }
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
    let (target, policy) = app
        .state::<LastTarget>()
        .0
        .lock()
        .ok()
        .map(|last| {
            (
                last.target.clone().filter(|target| target.generation == generation),
                PolicyProof::capture(&last.policy),
            )
        })
        .unwrap_or_default();
    let target = target.map(|mut target| {
        if let Some(captured) = &mut target.captured {
            captured.paste = app.state::<crate::destination::AppMemory>().for_capture(captured);
        }
        target
    });
    CopyAttempt {
        target,
        policy,
        written,
        stamp,
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn remember_copy(
    app: &AppHandle,
    attempt: CopyAttempt<windows::Window>,
    sequence: Option<u32>,
    token: u32,
) -> Option<u32> {
    let state = app.state::<LastTarget>();
    let Ok(mut last) = state.0.lock() else {
        return Some(token);
    };
    remember_ticket(&mut last.pending, attempt, sequence, token)
}

#[cfg(any(target_os = "windows", test))]
fn remember_ticket<W>(
    pending: &mut Option<DeliveryTicket<W>>,
    attempt: CopyAttempt<W>,
    sequence: Option<u32>,
    token: u32,
) -> Option<u32> {
    // Clipboard writes allocate identities before releasing their own lock.
    // Delayed bookkeeping for A must leave B's newer paste proof available.
    if pending.as_ref().is_some_and(|newer| newer.token > token) {
        return Some(token);
    }
    *pending = attempt.target.zip(sequence).map(|(target, sequence)| DeliveryTicket {
        policy: attempt.policy,
        token,
        target,
        sequence,
        written: attempt.written,
        stamp: attempt.stamp,
    });
    Some(token)
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
    with_paste_settings(&app.state::<crate::preferences::SettingsFile>().path, |enabled| {
        deliver_ticket(&desk, &ticket, text, enabled)
    })
}

/// Paste into the captured app when it is safe to; the notice then says what happened.
#[cfg(not(target_os = "windows"))]
pub(crate) fn deliver_copy(app: &AppHandle, text: &str, token: Option<u32>) -> Delivered {
    #[cfg(target_os = "macos")]
    let began = Instant::now();
    let proof = app.state::<LastTarget>().take_copy(text, token);
    #[cfg(target_os = "macos")]
    let desk = macos::Mac::new(app, proof.as_ref(), began);
    #[cfg(target_os = "linux")]
    let desk = CopyOnly;
    let outcome = with_paste_settings(&app.state::<crate::preferences::SettingsFile>().path, |enabled| {
        deliver(&desk, proof.as_ref(), enabled)
    });
    #[cfg(target_os = "macos")]
    desk.complete(&outcome);
    outcome
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
mod tests;
#[cfg(test)]
mod windows_tests;
