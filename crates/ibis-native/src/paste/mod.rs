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

#[cfg(test)]
use ibis_core::destination::PastePermission;
use ibis_core::destination::{AppPaste, CopyReason, System};
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
#[cfg(target_os = "windows")]
pub use delivery::key_release_failed;
pub use delivery::with_paste_setting;
use delivery::{DeliveryRequest, Desk, deliver};
pub use destination::{intended_paste, policy_decision};
use input_fence::PostResult;
#[cfg(any(target_os = "macos", test))]
use input_fence::{InputPress, PendingPost, PostDesk, PostRequest, check_input_since, checked_post};
#[cfg(test)]
use keys::{PasteEvent, PasteKeys};
#[cfg(test)]
use queries::{PasswordQueries, QueryWorker, display_name};
#[cfg(test)]
use tickets::{TicketDesk, deliver_ticket};

#[derive(Clone, Default)]
struct PolicyProof {
    epoch: Arc<AtomicU64>,
    generation: u64,
}

impl PolicyProof {
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

/// The app the picker was last opened from, and the owner of every Ibis
/// clipboard write on every system: each write takes its unique token here,
/// one write at a time, and only a write that began from a copy attempt can
/// leave a delivery ticket. Writes hold their own lock, never the target
/// state the UI thread reads: a Windows write can wait on the UI thread.
pub struct LastTarget(Mutex<TargetState>, fn() -> Result<u32, String>, Mutex<()>);

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
    /// never reused token: a ticket is taken only by its own token.
    pub fn new(write_token: fn() -> Result<u32, String>) -> Self {
        Self(Mutex::default(), write_token, Mutex::new(()))
    }

    /// Serialize successful policy changes with ticket publication and native
    /// posting. Detached workers keep a revocable epoch after taking their ticket.
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

    /// Freeze the destination, policy and time at request entry, before
    /// clipboard I/O. `presentation` is the host's count of picker
    /// presentations; only Windows keeps it there, macOS and Linux count
    /// theirs in `begin_presentation`.
    pub fn begin_copy(
        &self,
        presentation: impl FnOnce() -> u64,
        replaces: &[String],
        paste_for: impl FnOnce(&Captured) -> Result<Option<AppPaste>, CopyReason>,
    ) -> CopyAttempt {
        #[cfg(target_os = "macos")]
        let (stamp, diagnostic) = macos::observe_write_stamp();
        #[cfg(not(target_os = "macos"))]
        let stamp = SafetyStamp::now();
        self.begin_copy_observed(
            presentation,
            replaces,
            stamp,
            paste_for,
            #[cfg(target_os = "macos")]
            diagnostic,
        )
    }

    fn begin_copy_observed(
        &self,
        presentation: impl FnOnce() -> u64,
        replaces: &[String],
        stamp: SafetyStamp,
        paste_for: impl FnOnce(&Captured) -> Result<Option<AppPaste>, CopyReason>,
        #[cfg(target_os = "macos")] diagnostic: Option<macos::diagnostics::Snapshot>,
    ) -> CopyAttempt {
        let written = Instant::now();
        #[cfg(target_os = "windows")]
        let presentation = presentation();
        #[cfg(not(target_os = "windows"))]
        let _ = presentation;
        let (target, policy) = self
            .0
            .lock()
            .ok()
            .map(|last| {
                #[cfg(target_os = "windows")]
                let target = last.target.clone().filter(|target| target.presentation == presentation);
                #[cfg(not(target_os = "windows"))]
                let target = Some(CopyTarget {
                    captured: last.captured.clone(),
                    window: None,
                    presentation: last.presentation,
                });
                (target, PolicyProof::capture(&last.policy))
            })
            .unwrap_or_default();
        let target = target.map(|mut target| {
            if let Some(captured) = &mut target.captured {
                captured.paste = paste_for(captured);
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
            }
            target
        });
        CopyAttempt {
            policy,
            target,
            written,
            stamp,
        }
    }

    /// The one way Ibis writes the clipboard, on every system: one write at
    /// a time, after retiring the earlier ticket, with the next token. `write`
    /// returns the system's clipboard count right after the write when it can
    /// prove nothing else wrote in between; only then, and only for a copy
    /// `attempt`, does the write leave a ticket. A plain copy passes `None`
    /// and still retires the earlier ticket. The token is the write's receipt.
    pub fn write_copy(
        &self,
        attempt: Option<CopyAttempt>,
        write: impl FnOnce() -> Result<Option<i64>, String>,
    ) -> Result<u32, String> {
        write_ticket(&self.0, &self.2, self.1, attempt, write)
    }

    /// Take the ticket of the write `token` named, once. A stale delivery
    /// leaves a newer write's ticket alone.
    pub fn take_ticket(&self, token: Option<u32>) -> Option<DeliveryTicket> {
        self.0.lock().ok()?.take_ticket(token)
    }
}

/// Writes take `writes` in turn, so tokens and tickets follow write order by
/// construction: no later bookkeeping can let an older write's ticket
/// replace a newer one's. The target state is held only to retire and to
/// publish, never across clipboard I/O.
fn write_ticket<W>(
    state: &Mutex<TargetState<W>>,
    writes: &Mutex<()>,
    write_token: fn() -> Result<u32, String>,
    attempt: Option<CopyAttempt<W>>,
    write: impl FnOnce() -> Result<Option<i64>, String>,
) -> Result<u32, String> {
    let failed = || "The clipboard could not be written.".to_owned();
    let _writing = writes.lock().map_err(|_| failed())?;
    // Retired before the write: no earlier ticket can paste over this copy.
    state.lock().map_err(|_| failed())?.retire_ticket();
    let token = write_token()?;
    let clipboard = write()?;
    if let (
        Some(CopyAttempt {
            policy,
            target: Some(target),
            written,
            stamp,
        }),
        Some(clipboard),
        Ok(mut last),
    ) = (attempt, clipboard, state.lock())
    {
        #[cfg(any(not(target_os = "windows"), test))]
        {
            last.owner = Some(token);
        }
        last.pending = Some(DeliveryTicket {
            policy,
            token,
            target,
            clipboard,
            written,
            stamp,
        });
    }
    Ok(token)
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
                presentation: 0,
            });
        }
    }

    /// Final native checks must never dispatch to the UI thread while this
    /// guard is held. Contention refuses instead of queuing input behind IPC.
    pub fn post_if_ready(&self, post: impl FnOnce() -> bool) -> bool {
        self.0.try_lock().is_ok_and(|_policy| post())
    }
}

struct TargetState<W = HostWindow> {
    policy: Arc<AtomicU64>,
    /// The ticket of the newest write, until its delivery takes it.
    pending: Option<DeliveryTicket<W>>,
    #[cfg(any(not(target_os = "windows"), test))]
    owner: Option<u32>,
    #[cfg(any(not(target_os = "windows"), test))]
    captured: Option<Captured>,
    #[cfg(any(not(target_os = "windows"), test))]
    presentation: u64,
    #[cfg(target_os = "windows")]
    target: Option<CopyTarget<W>>,
}

impl<W> Default for TargetState<W> {
    fn default() -> Self {
        Self {
            policy: Arc::default(),
            pending: None,
            #[cfg(any(not(target_os = "windows"), test))]
            owner: None,
            #[cfg(any(not(target_os = "windows"), test))]
            captured: None,
            #[cfg(any(not(target_os = "windows"), test))]
            presentation: 0,
            #[cfg(target_os = "windows")]
            target: None,
        }
    }
}

impl<W> TargetState<W> {
    fn take_ticket(&mut self, token: Option<u32>) -> Option<DeliveryTicket<W>> {
        self.pending.take_if(|ticket| Some(ticket.token) == token)
    }

    fn retire_ticket(&mut self) {
        self.pending = None;
        #[cfg(any(not(target_os = "windows"), test))]
        {
            self.owner = None;
        }
    }

    fn invalidate_policy(&mut self) {
        self.policy
            .store(self.policy.load(Ordering::Acquire).saturating_add(1), Ordering::Release);
        self.retire_ticket();
    }
}

#[cfg(any(not(target_os = "windows"), test))]
impl LastTarget {
    /// Showing the picker invalidates an earlier delivery. Hide/blur events
    /// may happen more than once and must not retire the successful copy.
    pub fn begin_presentation(&self, captured: Option<Captured>) {
        if let Ok(mut last) = self.0.lock() {
            last.presentation = last.presentation.wrapping_add(1);
            last.retire_ticket();
            if let Some(captured) = captured {
                last.captured = Some(captured);
            }
        }
    }

    #[cfg(test)]
    fn begin_copy_at(
        &self,
        stamp: SafetyStamp,
        paste_for: impl FnOnce(&Captured) -> Result<Option<AppPaste>, CopyReason>,
    ) -> CopyAttempt {
        self.begin_copy_observed(
            || 0,
            &[],
            stamp,
            paste_for,
            #[cfg(target_os = "macos")]
            None,
        )
    }

    /// Serialize own writes, presentation retirement and posting. Taking a
    /// ticket does not surrender its ownership to a superseded worker. A
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
                    presentation: generation,
                },
                current(),
            );
        }
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

/// The window a Windows ticket returns to. macOS proves its window inside
/// `Captured`, and Linux captures none.
#[cfg(target_os = "windows")]
pub type HostWindow = windows::Window;
#[cfg(not(target_os = "windows"))]
pub type HostWindow = ();

/// The destination a copy freezes before writing. Neither the notice nor its
/// worker may adopt a later capture.
#[derive(Clone)]
pub struct CopyTarget<W = HostWindow> {
    captured: Option<Captured>,
    #[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
    window: Option<W>,
    /// The picker presentation the capture belongs to.
    presentation: u64,
}

/// Publish a complete capture only for the presentation that requested it.
/// Ibis in front keeps the previous app/window under the new generation.
#[cfg(any(target_os = "windows", test))]
fn publish_capture<W>(kept: &mut Option<CopyTarget<W>>, next: CopyTarget<W>, current: u64) {
    if next.presentation != current {
        return;
    }
    let target = match (next.captured.is_none(), kept.take()) {
        (true, Some(previous)) => CopyTarget {
            presentation: next.presentation,
            ..previous
        },
        _ => next,
    };
    *kept = Some(target);
}

/// What one of Ibis's own clipboard writes proves, on every system: its
/// frozen destination and policy, its time, and the system's clipboard count
/// right after it. Delivery takes it once by its write's token; it is never
/// cloned or serialized. Only a write the owner could prove leaves one, so
/// Linux, which keeps no clipboard count, has receipts and never a ticket.
pub struct DeliveryTicket<W = HostWindow> {
    policy: PolicyProof,
    token: u32,
    target: CopyTarget<W>,
    /// macOS's change count or Windows' final sequence after this write.
    #[cfg_attr(not(any(target_os = "macos", target_os = "windows", test)), allow(dead_code))]
    clipboard: i64,
    written: Instant,
    stamp: SafetyStamp,
}

impl<W> DeliveryTicket<W> {
    fn policy_current(&self) -> bool {
        self.policy.current()
    }
    fn captured(&self) -> Option<&Captured> {
        self.target.captured.as_ref()
    }

    fn check_age(&self, stamp: SafetyStamp, now: Instant) -> Result<(), SafetyFailure> {
        self.stamp.check(stamp)?;
        if now >= self.written + LATE {
            return Err(SafetyFailure::Expired);
        }
        Ok(())
    }
}

/// Identity and time at request entry, before target locks or clipboard I/O.
pub struct CopyAttempt<W = HostWindow> {
    policy: PolicyProof,
    target: Option<CopyTarget<W>>,
    written: Instant,
    stamp: SafetyStamp,
}

/// Paste into the captured app when it is safe to; the notice then says what happened.
/// `setting` reads the paste setting from its owner when delivery decides.
#[cfg(target_os = "macos")]
pub fn deliver_copy(host: &impl macos::Host, setting: impl FnOnce() -> Option<bool>, token: Option<u32>) -> Delivered {
    let began = Instant::now();
    let ticket = host.last_target().take_ticket(token);
    let desk = macos::Mac::new(host, ticket.as_ref(), began);
    let outcome = with_paste_setting(setting, |enabled| deliver(&desk, ticket.as_ref(), enabled));
    desk.complete(&outcome);
    outcome
}

/// Linux keeps no clipboard count, so its writes leave no ticket: it only copies.
#[cfg(target_os = "linux")]
pub fn deliver_copy(last: &LastTarget, setting: impl FnOnce() -> Option<bool>, token: Option<u32>) -> Delivered {
    let ticket = last.take_ticket(token);
    with_paste_setting(setting, |enabled| deliver(&CopyOnly, ticket.as_ref(), enabled))
}

/// Linux captures no target, so it only copies.
#[cfg(any(target_os = "linux", test))]
struct CopyOnly;

#[cfg(any(target_os = "linux", test))]
impl Desk for CopyOnly {
    type Window = HostWindow;

    fn may_post(&self) -> bool {
        false
    }
    fn ask_to_post(&self) {}
    fn frontmost(&self) -> Result<Option<Front>, CopyReason> {
        Ok(None)
    }
    fn finish(&self, _: DeliveryRequest, _: &DeliveryTicket) -> PostResult {
        PostResult::Refused
    }
    fn wait(&self, _: Duration) {}
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod windows_tests;
