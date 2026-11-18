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

#[cfg(any(target_os = "macos", target_os = "windows", test))]
use ibis_core::destination::AppPaste;
#[cfg(test)]
use ibis_core::destination::PastePermission;
use ibis_core::destination::{CopyReason, System};
use serde::{Deserialize, Serialize};

pub mod capture;
mod delivery;
mod destination;
mod input_fence;
pub mod keys;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod queries;
pub mod tickets;
#[cfg(target_os = "windows")]
pub mod windows;

pub use capture::Captured;
#[cfg(any(target_os = "linux", test))]
use capture::Front;
#[cfg(any(target_os = "macos", test))]
use capture::Selection;
#[cfg(any(target_os = "windows", test))]
use delivery::CopyEvidence;
#[cfg(target_os = "windows")]
pub use delivery::key_release_failed;
pub use delivery::with_paste_settings;
use delivery::{DeliveryRequest, Desk, deliver};
pub use destination::{intended_paste, policy_decision};
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

/// The system this build pastes on.
pub const SYSTEM: System = if cfg!(target_os = "macos") {
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
pub struct SafetyStamp {
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
}

/// The app the picker was last opened from, with the source of the unique
/// token each of Ibis's clipboard writes takes.
pub struct LastTarget(Mutex<TargetState>, fn() -> Result<u32, String>);

#[cfg(test)]
impl Default for LastTarget {
    fn default() -> Self {
        /// The tests' own writes, nonwrapping as the host's are.
        fn write_token() -> Result<u32, String> {
            static WRITES: Mutex<u32> = Mutex::new(0);
            let failed = || "The clipboard could not be written.".to_owned();
            let mut token = WRITES.lock().map_err(|_| failed())?;
            *token = token.checked_add(1).ok_or_else(failed)?;
            Ok(*token)
        }
        Self::new(write_token)
    }
}

impl LastTarget {
    /// `write_token` must give every clipboard write of the process a new,
    /// never reused token: a proof is consumed only by its own token.
    pub fn new(write_token: fn() -> Result<u32, String>) -> Self {
        Self(Mutex::default(), write_token)
    }

    /// Serialize successful policy changes with proof publication and native
    /// posting. Detached workers keep a revocable epoch after taking their proof.
    pub fn change_destination<T>(
        &self,
        revoke: bool,
        change: impl FnOnce(Option<&Captured>) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut last = self.0.lock().map_err(|_| "the destination is unavailable")?;
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

    /// Read the capture under the same lock as delivery; `None` if it is unavailable.
    pub fn with_captured<R>(&self, read: impl FnOnce(Option<&Captured>) -> R) -> Option<R> {
        let last = self.0.lock().ok()?;
        #[cfg(not(target_os = "windows"))]
        let captured = last.captured.as_ref();
        #[cfg(target_os = "windows")]
        let captured = last.target.as_ref().and_then(|target| target.captured.as_ref());
        Some(read(captured))
    }
}

#[cfg(any(target_os = "windows", test, feature = "fixtures"))]
impl LastTarget {
    #[cfg(any(test, feature = "fixtures"))]
    pub fn capture_fixture(&self, captured: Captured) {
        #[cfg(not(target_os = "windows"))]
        self.begin_presentation(Some(captured));
        #[cfg(target_os = "windows")]
        if let Ok(mut last) = self.0.lock() {
            last.target = Some(CopyTarget {
                captured: Some(captured),
                window: None,
                generation: 0,
            });
        }
    }

    /// Final native checks must never dispatch to the UI thread while this
    /// guard is held. Contention refuses instead of queuing input behind IPC.
    pub fn post_if_ready(&self, post: impl FnOnce() -> bool) -> bool {
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
    pub fn begin_presentation(&self, captured: Option<Captured>) {
        if let Ok(mut last) = self.0.lock() {
            last.presentation = last.presentation.wrapping_add(1);
            last.copied = None;
            last.owner = None;
            if let Some(captured) = captured {
                last.captured = Some(captured);
            }
        }
    }

    #[cfg(any(target_os = "macos", test))]
    pub fn write_copy(
        &self,
        text: &str,
        replaces: &[String],
        paste_for: impl FnOnce(&Captured) -> Result<Option<AppPaste>, CopyReason>,
        write: impl FnOnce() -> Result<Option<isize>, String>,
    ) -> Result<Option<u32>, String> {
        #[cfg(target_os = "macos")]
        let (stamp, diagnostic) = macos::observe_write_stamp();
        #[cfg(not(target_os = "macos"))]
        let stamp = SafetyStamp::now();
        self.write_copy_observed(
            text,
            replaces,
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
            &[],
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
        replaces: &[String],
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
            if !replaces.is_empty() {
                captured.selection = capture::Selection::Citation(capture::OldCitation::of_any(replaces));
            }
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
        let token = (self.1)()?;
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
    /// receipt does not surrender its ownership to a superseded worker. A
    /// final check that could not finish in time posted no key, so under this
    /// same lock the copy keeps its one turn for the delivery's retry.
    #[cfg(any(target_os = "macos", test))]
    fn post_if_current(&self, request: &PostRequest, post: impl FnOnce() -> PostResult) -> PostResult {
        let Ok(mut last) = self.0.try_lock() else {
            return PostResult::Unavailable;
        };
        if last.presentation != request.presentation || last.owner != Some(request.token) {
            return PostResult::Refused;
        }
        last.owner = None;
        let result = post();
        if result == PostResult::Unavailable {
            last.owner = Some(request.token);
        }
        result
    }

    #[cfg(target_os = "macos")]
    fn owns(&self, request: &PostRequest) -> bool {
        self.0
            .try_lock()
            .is_ok_and(|last| last.presentation == request.presentation && last.owner == Some(request.token))
    }
}

#[cfg(target_os = "windows")]
impl LastTarget {
    /// Remember a capture for its presentation. A newer show may finish its
    /// capture while this one's optional name is waiting: `current` is read
    /// under the lock, so an older HWND is never stamped with its generation.
    pub fn publish_capture(
        &self,
        captured: Option<Captured>,
        window: Option<windows::Window>,
        generation: u64,
        current: impl FnOnce() -> u64,
    ) {
        if let Ok(mut last) = self.0.lock() {
            publish_capture(
                &mut last.target,
                CopyTarget {
                    captured,
                    window,
                    generation,
                },
                current(),
            );
        }
    }

    /// Freeze the destination and time at request entry, before clipboard I/O.
    pub fn begin_copy(
        &self,
        generation: impl FnOnce() -> u64,
        replaces: &[String],
        paste_for: impl Fn(&Captured) -> Result<Option<AppPaste>, CopyReason>,
    ) -> CopyAttempt<windows::Window> {
        let stamp = SafetyStamp::now();
        let written = Instant::now();
        let generation = generation();
        let (target, policy) = self
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
                captured.paste = paste_for(captured);
                if !replaces.is_empty() {
                    captured.selection = capture::Selection::Citation(capture::OldCitation::of_any(replaces));
                }
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

    pub fn remember_copy(
        &self,
        attempt: CopyAttempt<windows::Window>,
        sequence: Option<u32>,
        token: u32,
    ) -> Option<u32> {
        let Ok(mut last) = self.0.lock() else {
            return Some(token);
        };
        remember_ticket(&mut last.pending, attempt, sequence, token)
    }

    pub fn take_ticket(&self, token: Option<u32>) -> Option<DeliveryTicket<windows::Window>> {
        take_ticket(&mut self.0.lock().ok()?.pending, token)
    }
}

/// What happened to a copied citation, for the notice to word.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum Delivered {
    Pasted {
        /// The app's own name; `None` when it has none.
        app: Option<String>,
    },
    Copied {
        /// Why Ibis did not paste. Every copy says why (docs/design/delivery.md).
        reason: CopiedReason,
    },
}

/// Why a citation was copied: the engine's decision before pasting, or a
/// paste that failed once it began.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum CopiedReason {
    Decided(CopyReason),
    Delivery(DeliveryFailure),
}

/// A paste that began and could not finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DeliveryFailure {
    /// The paste keys went down but could not be released.
    KeyReleaseFailed,
}

impl Delivered {
    pub fn copied(reason: CopyReason) -> Self {
        Self::Copied {
            reason: CopiedReason::Decided(reason),
        }
    }
}

/// A Windows copy freezes its destination before writing, then adds the
/// final sequence proved after closing the write. Neither
/// the notice nor its worker may adopt a later capture.
#[cfg(any(target_os = "windows", test))]
#[derive(Clone)]
pub struct CopyTarget<W> {
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
pub struct DeliveryTicket<W> {
    policy: PolicyProof,
    token: u32,
    target: CopyTarget<W>,
    sequence: u32,
    written: Instant,
    stamp: SafetyStamp,
}

/// Identity and time at request entry, before target locks or clipboard I/O.
#[cfg(any(target_os = "windows", test))]
pub struct CopyAttempt<W> {
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

/// Paste into the captured app when it is safe to; the notice then says what happened.
/// `settings` is the settings file the paste setting is read from.
#[cfg(target_os = "macos")]
pub fn deliver_copy(host: &impl macos::Host, settings: &std::path::Path, text: &str, token: Option<u32>) -> Delivered {
    let began = Instant::now();
    let proof = host.last_target().take_copy(text, token);
    let desk = macos::Mac::new(host, proof.as_ref(), began);
    let outcome = with_paste_settings(settings, |enabled| deliver(&desk, proof.as_ref(), enabled));
    desk.complete(&outcome);
    outcome
}

#[cfg(target_os = "linux")]
pub fn deliver_copy(last: &LastTarget, settings: &std::path::Path, text: &str, token: Option<u32>) -> Delivered {
    let proof = last.take_copy(text, token);
    with_paste_settings(settings, |enabled| deliver(&CopyOnly, proof.as_ref(), enabled))
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
