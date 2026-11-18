//! The picker window's lifecycle as one platform-free state machine.
//!
//! Every native signal about the picker is an [`Event`]: a summon, the
//! webview having drawn its snapshot, focus, a reason to stay open starting or
//! ending, a timer. [`PickerWindow::step`] is the only place the lifecycle
//! changes, and it answers with the [`Effect`]s a platform adapter carries out.
//! The adapter decides nothing: whether the picker is prepared, shown, focused,
//! kept or hidden is decided here, once, for every system.
//!
//! What it guarantees, checked over random event sequences in the tests:
//! - the window is shown only after the webview has drawn the snapshot of
//!   that same opening, and never for an opening that has since ended;
//! - this machine is the one owner of visibility: the window is on screen
//!   exactly while the phase is [`Phase::Shown`];
//! - every hide says where the keyboard goes next ([`FocusTo`]);
//! - every opening uses the one picker size on the display the person is
//!   working on. The machine never learns a display, so it cannot remember
//!   anything per display.
//!
//! A new reason to keep the picker open is one [`Reason`] case, and the
//! compiler asks which kinds of hide ([`Cause`]) it holds back.

use std::time::Duration;

/// One opening of the picker. Work, events and timers name the opening they
/// belong to; anything for an opening that is no longer current is ignored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Opening(u64);

/// A timer the machine asked for. Only the newest timer of its kind counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Timer(u64);

/// The picker's one size, in logical pixels. The adapter fits it to the
/// display it opens on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Size {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// A move or resize of the picker, numbered by the interface in the order it
/// starts them, so a late start or end never replaces a newer move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Move {
    pub(crate) opening: Opening,
    pub(crate) id: u64,
}

/// Why a shown picker stays open when it would otherwise hide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reason {
    /// The person pinned the picker beside their document.
    Pinned,
    /// A menu, file chooser or update dialog the picker owns is open.
    Dialog,
    /// The person is moving or resizing the picker; a compositor's grab can
    /// take focus away from the window being moved.
    Moving(Move),
    /// The first presentation after launch, before the panel can tell a real
    /// blur from its own setup.
    Starting,
}

/// Why the picker would hide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cause {
    /// Focus moved to another app.
    Blur,
    /// The interface is done with this opening: a citation went out, or the
    /// person pressed Escape.
    Done,
    /// Another window takes over.
    Takeover(Takeover),
    /// The person closed the picker: its close button, or the shortcut while
    /// the picker has the keyboard. Nothing keeps it.
    Close,
}

/// What a window taking over means for the picker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Takeover {
    /// Another app, such as one showing a source: a pinned picker stays
    /// beside it, as the pin promises.
    Beside,
    /// An Ibis window such as Settings: a floating picker would cover it and
    /// take its clicks, so even a pinned one leaves. The pin stays for the
    /// next opening.
    Covered,
}

impl Reason {
    /// Whether this reason holds back a hide for `cause`. Every reason answers
    /// for every cause; there is no default.
    pub(crate) fn keeps(self, cause: Cause) -> bool {
        match self {
            Reason::Pinned => matches!(cause, Cause::Blur | Cause::Takeover(Takeover::Beside)),
            // Never take away the parent of an open dialog, except when the
            // person closes the picker themselves.
            Reason::Dialog => cause != Cause::Close,
            Reason::Moving(_) | Reason::Starting => cause == Cause::Blur,
        }
    }
}

/// Where the keyboard goes when the picker hides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FocusTo {
    /// Back to the app the person was in, so the next paste lands there. On
    /// macOS the adapter also hides the app, or a hidden panel would leave
    /// Ibis active and swallow that paste.
    Previous,
    /// An Ibis window is in front, Settings: it keeps the keyboard, so the
    /// app stays active.
    OwnWindow,
}

/// What the picker is doing, as the interface sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Hidden,
    /// Off screen: capturing the app in front and drawing the snapshot.
    Preparing(Opening),
    /// On screen, owned by this opening.
    Shown(Opening),
}

/// Everything that happens to the picker window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    /// The tray, the menu bar, a notice: show a hidden picker. A shown one
    /// keeps its opening, with the search the person is using, and takes the
    /// keyboard when `focus`.
    Summon { focus: bool },
    /// The global shortcut. `focused` is whether a shown picker has the
    /// keyboard, `None` when that cannot be read. In front, it closes the
    /// picker; beside another app, it opens afresh for that app.
    Shortcut { focused: Option<bool> },
    /// A dialog of `opening` closed: give the picker the keyboard back.
    Refocus(Opening),
    /// The webview has drawn the snapshot of `opening`.
    Rendered(Opening),
    /// `opening` could not be prepared: no window or no display.
    Abandoned(Opening),
    /// The interface asks `opening` to hide.
    Done(Opening),
    /// Another window takes over from `opening`.
    Takeover(Opening, Takeover),
    /// The window's close button, or quitting the picker from the system.
    Close,
    /// The picker gained the keyboard.
    Focused,
    /// The picker lost the keyboard.
    Blurred,
    /// A reason to stay open started.
    Keep(Reason),
    /// A reason to stay open ended.
    Release(Reason),
    /// A timer asked for with [`Effect::After`] ran out.
    Elapsed(Timer),
    /// An Ibis window that takes the keyboard, Settings, opened or closed.
    OwnWindow(bool),
    /// The picker's one size changed: a choice in Settings or a settled drag.
    Size(Size),
}

/// What the platform adapter must do. It reports back with an [`Event`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Effect {
    /// Capture the app in front, place the window off screen at `size` on the
    /// display the person is working on, and send the webview the snapshot of
    /// `opening`. Answer with [`Event::Rendered`] or [`Event::Abandoned`].
    Prepare { opening: Opening, size: Size, focus: bool },
    /// Put the window on screen, or in front if it already is, for
    /// `opening`; with the keyboard when `focus`.
    Reveal { opening: Opening, focus: bool },
    /// Give the shown picker the keyboard.
    Focus(Opening),
    /// Drop what was prepared for `opening`. It was never shown, so the
    /// keyboard never left the person's app.
    Cancel(Opening),
    /// Take the window off screen and send the keyboard to `to`.
    Hide { opening: Opening, to: FocusTo },
    /// Read whether the picker has the keyboard now and answer with
    /// [`Event::Focused`] or [`Event::Blurred`]: a blur held back by a reason
    /// that has now ended may have been the last focus event to arrive.
    ReadFocus(Opening),
    /// Answer with [`Event::Elapsed`] after `delay`.
    After { delay: Duration, timer: Timer },
}

/// What differs between builds and systems. Everything else is the same.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Policy {
    /// Release builds hide on blur, like other launchers; development builds
    /// keep the picker so the developer tools stay usable.
    pub(crate) hide_on_blur: bool,
    /// How long a blur waits before it hides. On Windows, Narrator and other
    /// UI Automation clients can take focus and give it back within tens of
    /// milliseconds (#341).
    pub(crate) blur_grace: Option<Duration>,
    /// How long a move or resize may hold back a blur without its end
    /// arriving. Only Linux compositors take focus for their move grab.
    pub(crate) move_hold: Option<Duration>,
}

impl Policy {
    /// The policy of this build on this system.
    pub(crate) fn native() -> Self {
        Policy {
            hide_on_blur: !cfg!(debug_assertions),
            blur_grace: cfg!(target_os = "windows").then_some(Duration::from_millis(250)),
            // No one drags a window for a minute; a cancelled grab or a lost
            // pointer event must not keep a blurred picker shown.
            move_hold: cfg!(target_os = "linux").then_some(Duration::from_secs(60)),
        }
    }
}

/// What belongs to the shown opening only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Shown {
    /// The last focus event was a blur.
    blurred: bool,
    /// A blur waiting out the grace period.
    grace: Option<Timer>,
    /// The move holding back a blur, and its limit.
    moving: Option<(Move, Timer)>,
    /// A fresh opening being prepared behind this one, and whether it takes
    /// the keyboard.
    next: Option<(Opening, bool)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Hidden,
    Preparing { opening: Opening, focus: bool },
    Shown { opening: Opening, shown: Shown },
}

/// The picker window: its phase, its one size and the reasons it stays open.
#[derive(Debug)]
pub(crate) struct PickerWindow {
    policy: Policy,
    state: State,
    size: Size,
    pinned: bool,
    dialog: bool,
    starting: bool,
    own_window: bool,
    newest_move: u64,
    counter: u64,
}

impl PickerWindow {
    pub(crate) fn new(policy: Policy, size: Size) -> Self {
        PickerWindow {
            policy,
            state: State::Hidden,
            size,
            pinned: false,
            dialog: false,
            starting: false,
            own_window: false,
            newest_move: 0,
            counter: 0,
        }
    }

    pub(crate) fn phase(&self) -> Phase {
        match self.state {
            State::Hidden => Phase::Hidden,
            State::Preparing { opening, .. } => Phase::Preparing(opening),
            State::Shown { opening, .. } => Phase::Shown(opening),
        }
    }

    /// The opening that newer work belongs to: the one being prepared, even
    /// behind a shown one. Requests from any other opening are late.
    pub(crate) fn owner(&self) -> Option<Opening> {
        match self.state {
            State::Hidden => None,
            State::Preparing { opening, .. } => Some(opening),
            State::Shown { opening, shown } => Some(shown.next.map_or(opening, |(next, _)| next)),
        }
    }

    /// The first reason that holds back a hide for `cause`, if any.
    pub(crate) fn kept_by(&self, cause: Cause) -> Option<Reason> {
        let moving = match self.state {
            State::Shown { shown, .. } => shown.moving.map(|(held, _)| Reason::Moving(held)),
            _ => None,
        };
        [
            self.pinned.then_some(Reason::Pinned),
            self.dialog.then_some(Reason::Dialog),
            self.starting.then_some(Reason::Starting),
            moving,
        ]
        .into_iter()
        .flatten()
        .find(|reason| reason.keeps(cause))
    }

    /// Apply one event and return what the adapter must do, in order.
    pub(crate) fn step(&mut self, event: Event) -> Vec<Effect> {
        let mut effects = Vec::new();
        match event {
            Event::Summon { focus } => self.summon(focus, &mut effects),
            Event::Shortcut { focused } => match self.state {
                State::Shown { .. } if focused == Some(true) => self.hide(Cause::Close, &mut effects),
                State::Shown { opening, mut shown } => {
                    // Beside another app: open afresh for it, behind the
                    // shown snapshot, which stays until the new one is drawn.
                    if shown.next.is_none() {
                        let next = self.opening();
                        shown.next = Some((next, true));
                        self.state = State::Shown { opening, shown };
                        effects.push(self.prepare(next, true));
                    }
                }
                State::Hidden | State::Preparing { .. } => self.summon(true, &mut effects),
            },
            Event::Refocus(opening) => {
                if let State::Shown { opening: shown, .. } = self.state
                    && self.owner() == Some(opening)
                {
                    effects.push(Effect::Focus(shown));
                }
            }
            Event::Rendered(drawn) => self.rendered(drawn, &mut effects),
            Event::Abandoned(failed) => match self.state {
                State::Preparing { opening, .. } if opening == failed => {
                    self.state = State::Hidden;
                    effects.push(Effect::Cancel(failed));
                }
                State::Shown { opening, mut shown } if shown.next.map(|(next, _)| next) == Some(failed) => {
                    shown.next = None;
                    self.state = State::Shown { opening, shown };
                    effects.push(Effect::Cancel(failed));
                }
                _ => {}
            },
            Event::Done(opening) => {
                if self.owner() == Some(opening) {
                    self.hide(Cause::Done, &mut effects);
                }
            }
            Event::Takeover(opening, takeover) => {
                // Only a picker on screen gives way; one still preparing is
                // not covering anything yet.
                if matches!(self.state, State::Shown { .. }) && self.owner() == Some(opening) {
                    self.hide(Cause::Takeover(takeover), &mut effects);
                }
            }
            Event::Close => self.hide(Cause::Close, &mut effects),
            Event::Focused => {
                if let State::Shown { opening, shown } = self.state {
                    // Focus is back: no blur is pending, and any move grab
                    // that took it has ended.
                    let shown = Shown {
                        blurred: false,
                        grace: None,
                        moving: None,
                        ..shown
                    };
                    self.state = State::Shown { opening, shown };
                }
            }
            Event::Blurred => self.blurred(&mut effects),
            Event::Keep(reason) => self.keep(reason, &mut effects),
            Event::Release(reason) => self.release(reason, &mut effects),
            Event::Elapsed(timer) => self.elapsed(timer, &mut effects),
            Event::OwnWindow(open) => self.own_window = open,
            Event::Size(size) => self.size = size,
        }
        effects
    }

    fn opening(&mut self) -> Opening {
        Opening(self.count())
    }

    fn timer(&mut self) -> Timer {
        Timer(self.count())
    }

    fn count(&mut self) -> u64 {
        self.counter = self.counter.wrapping_add(1);
        self.counter
    }

    fn prepare(&self, opening: Opening, focus: bool) -> Effect {
        Effect::Prepare {
            opening,
            size: self.size,
            focus,
        }
    }

    fn summon(&mut self, focus: bool, effects: &mut Vec<Effect>) {
        match self.state {
            State::Hidden => {
                let opening = self.opening();
                self.state = State::Preparing { opening, focus };
                effects.push(self.prepare(opening, focus));
            }
            State::Preparing { opening, focus: before } => {
                self.state = State::Preparing {
                    opening,
                    focus: before || focus,
                };
            }
            State::Shown { opening, mut shown } => {
                if let Some((next, before)) = shown.next {
                    shown.next = Some((next, before || focus));
                    self.state = State::Shown { opening, shown };
                } else if focus {
                    effects.push(Effect::Focus(opening));
                }
            }
        }
    }

    fn rendered(&mut self, drawn: Opening, effects: &mut Vec<Effect>) {
        match self.state {
            State::Preparing { opening, focus } if opening == drawn => {
                self.state = State::Shown {
                    opening,
                    shown: Shown::default(),
                };
                effects.push(Effect::Reveal { opening, focus });
            }
            State::Shown { shown, .. } if shown.next.map(|(next, _)| next) == Some(drawn) => {
                let focus = shown.next.is_some_and(|(_, focus)| focus);
                // Holds and timers of the opening being replaced end with it.
                self.state = State::Shown {
                    opening: drawn,
                    shown: Shown::default(),
                };
                effects.push(Effect::Reveal { opening: drawn, focus });
            }
            _ => {}
        }
    }

    fn blurred(&mut self, effects: &mut Vec<Effect>) {
        let State::Shown { opening, mut shown } = self.state else {
            return;
        };
        shown.blurred = true;
        shown.grace = None;
        if self.policy.hide_on_blur && self.kept_by(Cause::Blur).is_none() {
            if let Some(delay) = self.policy.blur_grace {
                let timer = self.timer();
                shown.grace = Some(timer);
                effects.push(Effect::After { delay, timer });
            } else {
                self.state = State::Shown { opening, shown };
                self.hide(Cause::Blur, effects);
                return;
            }
        }
        self.state = State::Shown { opening, shown };
    }

    fn keep(&mut self, reason: Reason, effects: &mut Vec<Effect>) {
        match reason {
            Reason::Pinned => self.pinned = true,
            Reason::Dialog => self.dialog = true,
            Reason::Starting => self.starting = true,
            Reason::Moving(held) => {
                let Some(delay) = self.policy.move_hold else {
                    return;
                };
                if held.id <= self.newest_move || self.owner() != Some(held.opening) {
                    return;
                }
                let State::Shown { opening, mut shown } = self.state else {
                    return;
                };
                self.newest_move = held.id;
                let timer = self.timer();
                shown.moving = Some((held, timer));
                self.state = State::Shown { opening, shown };
                effects.push(Effect::After { delay, timer });
            }
        }
    }

    fn release(&mut self, reason: Reason, effects: &mut Vec<Effect>) {
        match reason {
            Reason::Pinned => self.pinned = false,
            Reason::Dialog => self.dialog = false,
            Reason::Starting => self.starting = false,
            Reason::Moving(held) => {
                // An end that arrives before its start still retires it.
                self.newest_move = self.newest_move.max(held.id);
                let State::Shown { opening, mut shown } = self.state else {
                    return;
                };
                if shown.moving.map(|(moving, _)| moving) != Some(held) {
                    return;
                }
                shown.moving = None;
                self.state = State::Shown { opening, shown };
            }
        }
        self.recheck_blur(effects);
    }

    fn elapsed(&mut self, timer: Timer, effects: &mut Vec<Effect>) {
        let State::Shown { opening, mut shown } = self.state else {
            return;
        };
        if shown.grace == Some(timer) {
            shown.grace = None;
            self.state = State::Shown { opening, shown };
            if shown.blurred && self.kept_by(Cause::Blur).is_none() {
                self.hide(Cause::Blur, effects);
            }
        } else if shown.moving.map(|(_, limit)| limit) == Some(timer) {
            shown.moving = None;
            self.state = State::Shown { opening, shown };
            self.recheck_blur(effects);
        }
    }

    /// A reason ended. A blur it held back may have been the last focus event,
    /// so ask for live focus rather than trusting it.
    fn recheck_blur(&self, effects: &mut Vec<Effect>) {
        if let State::Shown { opening, shown } = self.state
            && shown.blurred
            && self.policy.hide_on_blur
            && self.kept_by(Cause::Blur).is_none()
        {
            effects.push(Effect::ReadFocus(opening));
        }
    }

    fn hide(&mut self, cause: Cause, effects: &mut Vec<Effect>) {
        if self.kept_by(cause).is_some() {
            return;
        }
        match self.state {
            State::Hidden => {}
            State::Preparing { opening, .. } => {
                self.state = State::Hidden;
                effects.push(Effect::Cancel(opening));
            }
            State::Shown { opening, shown } => {
                self.state = State::Hidden;
                if let Some((next, _)) = shown.next {
                    effects.push(Effect::Cancel(next));
                }
                let to = if self.own_window || cause == Cause::Takeover(Takeover::Covered) {
                    FocusTo::OwnWindow
                } else {
                    FocusTo::Previous
                };
                effects.push(Effect::Hide { opening, to });
            }
        }
    }
}

#[cfg(test)]
mod tests;
