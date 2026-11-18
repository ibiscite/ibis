use super::*;

const SIZE: Size = Size {
    width: 720,
    height: 480,
};
const GRACE: Duration = Duration::from_millis(250);
const MOVE_LIMIT: Duration = Duration::from_secs(60);
const RELEASE: Policy = Policy {
    hide_on_blur: true,
    blur_grace: None,
    move_hold: None,
};

/// A picker shown for one opening, with the keyboard.
fn shown(policy: Policy) -> (PickerWindow, Opening) {
    let mut window = PickerWindow::new(policy, SIZE);
    let [Effect::Prepare { opening, .. }] = window.step(Event::Summon { focus: true })[..] else {
        panic!("a summon prepares an opening");
    };
    assert_eq!(
        window.step(Event::Rendered(opening)),
        [Effect::Reveal { opening, focus: true }]
    );
    (window, opening)
}

#[test]
fn the_window_waits_for_the_snapshot_of_its_own_opening() {
    let mut window = PickerWindow::new(RELEASE, SIZE);
    let effects = window.step(Event::Summon { focus: false });
    let [Effect::Prepare { opening, size, focus }] = effects[..] else {
        panic!("expected one Prepare, got {effects:?}");
    };
    assert_eq!((size, focus), (SIZE, false));
    assert_eq!(window.phase(), Phase::Preparing(opening));
    assert_eq!(window.step(Event::Rendered(Opening(opening.0 + 7))), []);
    // A second summon while preparing joins it, and asks for the keyboard.
    assert_eq!(window.step(Event::Summon { focus: true }), []);
    assert_eq!(
        window.step(Event::Rendered(opening)),
        [Effect::Reveal { opening, focus: true }]
    );
    assert_eq!(window.phase(), Phase::Shown(opening));
}

#[test]
fn a_late_render_never_brings_back_a_closed_picker() {
    let mut window = PickerWindow::new(RELEASE, SIZE);
    let [Effect::Prepare { opening, .. }] = window.step(Event::Summon { focus: true })[..] else {
        panic!("a summon prepares an opening");
    };
    assert_eq!(window.step(Event::Close), [Effect::Cancel(opening)]);
    assert_eq!(window.step(Event::Rendered(opening)), []);
    assert_eq!(window.phase(), Phase::Hidden);
}

#[test]
fn a_pinned_picker_stays_beside_another_app_but_gives_way_to_settings() {
    let (mut window, opening) = shown(RELEASE);
    assert_eq!(window.step(Event::Keep(Reason::Pinned)), []);
    assert_eq!(window.step(Event::Blurred), []);
    assert_eq!(window.step(Event::Takeover(opening, Takeover::Beside)), []);
    assert_eq!(window.kept_by(Cause::Blur), Some(Reason::Pinned));
    assert_eq!(
        window.step(Event::Takeover(opening, Takeover::Covered)),
        [Effect::Hide {
            opening,
            to: FocusTo::OwnWindow
        }]
    );
    // The pin stays for the next opening.
    let [Effect::Prepare { opening, .. }] = window.step(Event::Summon { focus: true })[..] else {
        panic!("a summon prepares an opening");
    };
    window.step(Event::Rendered(opening));
    assert_eq!(window.step(Event::Blurred), []);
}

#[test]
fn an_owned_dialog_keeps_its_parent_until_it_closes_then_focus_is_read() {
    let (mut window, opening) = shown(RELEASE);
    window.step(Event::Keep(Reason::Dialog));
    assert_eq!(window.step(Event::Blurred), []);
    assert_eq!(window.step(Event::Done(opening)), []);
    assert_eq!(
        window.step(Event::Release(Reason::Dialog)),
        [Effect::ReadFocus(opening)]
    );
    assert_eq!(
        window.step(Event::Blurred),
        [Effect::Hide {
            opening,
            to: FocusTo::Previous
        }]
    );
}

#[test]
fn the_person_can_always_close_the_picker() {
    let (mut window, opening) = shown(RELEASE);
    window.step(Event::Keep(Reason::Dialog));
    window.step(Event::Keep(Reason::Pinned));
    assert_eq!(
        window.step(Event::Shortcut { focused: Some(true) }),
        [Effect::Hide {
            opening,
            to: FocusTo::Previous
        }]
    );
}

#[test]
fn a_blur_on_windows_waits_for_focus_to_come_back() {
    let (mut window, opening) = shown(Policy {
        blur_grace: Some(GRACE),
        ..RELEASE
    });
    let [Effect::After { delay, timer: first }] = window.step(Event::Blurred)[..] else {
        panic!("a blur waits out its grace");
    };
    assert_eq!(delay, GRACE);
    window.step(Event::Focused);
    assert_eq!(window.step(Event::Elapsed(first)), []);
    let [Effect::After { timer: second, .. }] = window.step(Event::Blurred)[..] else {
        panic!("a blur waits out its grace");
    };
    assert_eq!(window.step(Event::Elapsed(first)), []);
    assert_eq!(
        window.step(Event::Elapsed(second)),
        [Effect::Hide {
            opening,
            to: FocusTo::Previous
        }]
    );
}

#[test]
fn a_move_holds_a_blur_until_its_own_end_or_its_limit() {
    let (mut window, opening) = shown(Policy {
        move_hold: Some(MOVE_LIMIT),
        ..RELEASE
    });
    let moving = |id| Reason::Moving(Move { opening, id });
    let [Effect::After { delay, .. }] = window.step(Event::Keep(moving(2)))[..] else {
        panic!("a move asks for its limit");
    };
    assert_eq!(delay, MOVE_LIMIT);
    assert_eq!(window.step(Event::Blurred), []);
    assert_eq!(window.step(Event::Release(moving(1))), []);
    assert_eq!(window.step(Event::Release(moving(2))), [Effect::ReadFocus(opening)]);
    window.step(Event::Focused);
    // An end that arrives before its start retires it.
    window.step(Event::Release(moving(5)));
    assert_eq!(window.step(Event::Keep(moving(5))), []);
    let [Effect::After { timer, .. }] = window.step(Event::Keep(moving(6)))[..] else {
        panic!("a move asks for its limit");
    };
    assert_eq!(window.step(Event::Blurred), []);
    assert_eq!(window.step(Event::Elapsed(timer)), [Effect::ReadFocus(opening)]);
}

#[test]
fn moves_hold_nothing_where_no_compositor_takes_focus() {
    let (mut window, opening) = shown(RELEASE);
    assert_eq!(window.step(Event::Keep(Reason::Moving(Move { opening, id: 1 }))), []);
    assert_eq!(
        window.step(Event::Blurred),
        [Effect::Hide {
            opening,
            to: FocusTo::Previous
        }]
    );
}

#[test]
fn the_shortcut_beside_another_app_opens_afresh_behind_the_shown_snapshot() {
    let (mut window, first) = shown(RELEASE);
    let [Effect::Prepare { opening: second, .. }] = window.step(Event::Shortcut { focused: Some(false) })[..] else {
        panic!("the shortcut prepares a fresh opening");
    };
    assert_eq!(window.phase(), Phase::Shown(first));
    assert_eq!(window.owner(), Some(second));
    assert_eq!(window.step(Event::Done(first)), []);
    assert_eq!(
        window.step(Event::Rendered(second)),
        [Effect::Reveal {
            opening: second,
            focus: true
        }]
    );
    assert_eq!(window.phase(), Phase::Shown(second));
}

#[test]
fn closing_while_a_fresh_opening_is_prepared_drops_it_first() {
    let (mut window, first) = shown(RELEASE);
    let [Effect::Prepare { opening: second, .. }] = window.step(Event::Shortcut { focused: None })[..] else {
        panic!("the shortcut prepares a fresh opening");
    };
    assert_eq!(
        window.step(Event::Close),
        [
            Effect::Cancel(second),
            Effect::Hide {
                opening: first,
                to: FocusTo::Previous
            }
        ]
    );
}

#[test]
fn every_opening_has_the_one_size() {
    let (mut window, _) = shown(RELEASE);
    let dragged = Size {
        width: 840,
        height: 520,
    };
    window.step(Event::Size(dragged));
    window.step(Event::Close);
    let [Effect::Prepare { size, .. }] = window.step(Event::Summon { focus: true })[..] else {
        panic!("a summon prepares an opening");
    };
    assert_eq!(size, dragged);
}

#[test]
fn settings_keeps_the_keyboard_when_the_picker_hides() {
    let (mut window, opening) = shown(RELEASE);
    window.step(Event::OwnWindow(true));
    assert_eq!(
        window.step(Event::Done(opening)),
        [Effect::Hide {
            opening,
            to: FocusTo::OwnWindow
        }]
    );
}

#[test]
fn development_builds_never_hide_on_blur() {
    let (mut window, opening) = shown(Policy {
        hide_on_blur: false,
        ..RELEASE
    });
    assert_eq!(window.step(Event::Blurred), []);
    assert_eq!(
        window.step(Event::Takeover(opening, Takeover::Beside)),
        [Effect::Hide {
            opening,
            to: FocusTo::Previous
        }]
    );
}

/// SplitMix64: a seeded source of choices, so a failure names its seed.
struct Choices(u64);

impl Choices {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).unwrap_or(0)
    }

    fn coin(&mut self) -> bool {
        self.next() & 1 == 1
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> Option<T> {
        (!items.is_empty()).then(|| items[self.below(items.len())])
    }
}

/// What can be seen from outside the machine: the native window, the
/// webview and the reasons the test has started.
struct World {
    policy: Policy,
    on_screen: Option<Opening>,
    /// Prepared and not yet revealed or dropped.
    pending: Option<Opening>,
    /// Every opening whose snapshot reached the webview.
    sent: Vec<Opening>,
    timers: Vec<Timer>,
    size: Size,
    pinned: bool,
    dialog: bool,
    starting: bool,
    own_window: bool,
}

impl World {
    fn new(policy: Policy) -> Self {
        World {
            policy,
            on_screen: None,
            pending: None,
            sent: Vec::new(),
            timers: Vec::new(),
            size: SIZE,
            pinned: false,
            dialog: false,
            starting: false,
            own_window: false,
        }
    }

    /// An opening the interface might name: usually the current one,
    /// sometimes an old one, sometimes one that never existed.
    fn opening(&self, choices: &mut Choices, window: &PickerWindow) -> Opening {
        match choices.below(4) {
            0 | 1 => window.owner().or_else(|| choices.pick(&self.sent)),
            2 => choices.pick(&self.sent),
            _ => None,
        }
        .unwrap_or(Opening(u64::MAX - choices.next() % 4))
    }

    fn event(&self, choices: &mut Choices, window: &PickerWindow) -> Event {
        let reason = |choices: &mut Choices| match choices.below(4) {
            0 => Reason::Pinned,
            1 => Reason::Dialog,
            2 => Reason::Starting,
            _ => Reason::Moving(Move {
                opening: self.opening(choices, window),
                id: choices.next() % 8,
            }),
        };
        let takeover = |choices: &mut Choices| {
            if choices.coin() {
                Takeover::Beside
            } else {
                Takeover::Covered
            }
        };
        match choices.below(17) {
            0 => Event::Summon { focus: choices.coin() },
            1 => Event::Shortcut {
                focused: choices.pick(&[None, Some(true), Some(false)]).flatten(),
            },
            2 | 3 => Event::Rendered(if choices.below(5) == 0 {
                self.opening(choices, window)
            } else {
                choices
                    .pick(&self.sent)
                    .unwrap_or_else(|| self.opening(choices, window))
            }),
            4 => Event::Abandoned(self.opening(choices, window)),
            5 => Event::Done(self.opening(choices, window)),
            6 => Event::Takeover(self.opening(choices, window), takeover(choices)),
            7 => Event::Refocus(self.opening(choices, window)),
            8 => Event::Close,
            9 => Event::Focused,
            10 | 11 => Event::Blurred,
            12 => Event::Keep(reason(choices)),
            13 => Event::Release(reason(choices)),
            14 => Event::Elapsed(choices.pick(&self.timers).unwrap_or(Timer(u64::MAX))),
            15 => Event::OwnWindow(choices.coin()),
            _ => Event::Size(Size {
                width: 400 + u32::try_from(choices.next() % 800).unwrap_or(0),
                height: 300 + u32::try_from(choices.next() % 600).unwrap_or(0),
            }),
        }
    }

    /// Whether, by the reasons the test itself started, nothing keeps the
    /// picker from hiding for `cause`. Moves are checked by their own tests.
    fn may_hide(&self, cause: Cause) -> bool {
        match cause {
            Cause::Close => true,
            Cause::Blur => self.policy.hide_on_blur && !self.pinned && !self.dialog && !self.starting,
            Cause::Done | Cause::Takeover(Takeover::Covered) => !self.dialog,
            Cause::Takeover(Takeover::Beside) => !self.dialog && !self.pinned,
        }
    }

    /// Apply `effects` to the outside world, failing on any that breaks an
    /// invariant.
    fn check(
        &mut self,
        owner: Option<Opening>,
        event: Event,
        effects: &[Effect],
        window: &PickerWindow,
    ) -> Result<(), String> {
        let late = match event {
            Event::Done(named) | Event::Takeover(named, _) | Event::Refocus(named) => owner != Some(named),
            _ => false,
        };
        if late && !effects.is_empty() {
            return Err("a request from another opening had an effect".into());
        }
        for effect in effects {
            match *effect {
                Effect::Prepare { opening, size, .. } => {
                    if !matches!(event, Event::Summon { .. } | Event::Shortcut { .. }) {
                        return Err("prepared without a summon".into());
                    }
                    if self.pending.is_some() {
                        return Err("two openings prepared at once".into());
                    }
                    if self.sent.contains(&opening) {
                        return Err("an opening was prepared twice".into());
                    }
                    // No per-display memory: the one size, wherever it opens.
                    if size != self.size {
                        return Err(format!("prepared at {size:?}, not the one size {:?}", self.size));
                    }
                    self.pending = Some(opening);
                    self.sent.push(opening);
                }
                Effect::Reveal { opening, .. } => {
                    if event != Event::Rendered(opening) || self.pending != Some(opening) {
                        return Err("shown before its own snapshot was drawn".into());
                    }
                    self.pending = None;
                    self.on_screen = Some(opening);
                }
                Effect::Cancel(opening) => {
                    if self.pending != Some(opening) {
                        return Err("dropped an opening that was not being prepared".into());
                    }
                    self.pending = None;
                }
                Effect::Hide { opening, to } => {
                    if self.on_screen != Some(opening) || self.pending.is_some() {
                        return Err("hid a window that was not shown, or left a preparation behind".into());
                    }
                    let cause = match event {
                        Event::Blurred | Event::Elapsed(_) => Cause::Blur,
                        Event::Done(_) => Cause::Done,
                        Event::Takeover(_, takeover) => Cause::Takeover(takeover),
                        Event::Close | Event::Shortcut { .. } => Cause::Close,
                        _ => return Err("hid without a reason to".into()),
                    };
                    if !self.may_hide(cause) {
                        return Err(format!("hid for {cause:?} while a reason kept it open"));
                    }
                    let expected = if self.own_window || cause == Cause::Takeover(Takeover::Covered) {
                        FocusTo::OwnWindow
                    } else {
                        FocusTo::Previous
                    };
                    if to != expected {
                        return Err(format!("focus went to {to:?}, not {expected:?}"));
                    }
                    self.on_screen = None;
                }
                Effect::Focus(opening) | Effect::ReadFocus(opening) => {
                    if self.on_screen != Some(opening) {
                        return Err("asked about focus of a window not on screen".into());
                    }
                }
                Effect::After { timer, .. } => self.timers.push(timer),
            }
        }
        match event {
            Event::Keep(Reason::Pinned) | Event::Release(Reason::Pinned) => {
                self.pinned = matches!(event, Event::Keep(_))
            }
            Event::Keep(Reason::Dialog) | Event::Release(Reason::Dialog) => {
                self.dialog = matches!(event, Event::Keep(_))
            }
            Event::Keep(Reason::Starting) | Event::Release(Reason::Starting) => {
                self.starting = matches!(event, Event::Keep(_));
            }
            Event::OwnWindow(open) => self.own_window = open,
            Event::Size(size) => self.size = size,
            _ => {}
        }
        // One owner of visibility: on screen exactly while shown.
        match window.phase() {
            Phase::Shown(opening) if self.on_screen == Some(opening) => Ok(()),
            Phase::Hidden | Phase::Preparing(_) if self.on_screen.is_none() => Ok(()),
            phase => Err(format!("phase {phase:?} but the window shows {:?}", self.on_screen)),
        }?;
        match window.phase() {
            Phase::Preparing(opening) if self.pending != Some(opening) => Err("preparing without a preparation".into()),
            Phase::Hidden if self.pending.is_some() => Err("hidden with a preparation left over".into()),
            _ => Ok(()),
        }
    }
}

#[test]
fn random_event_sequences_keep_every_invariant() {
    let policies = [false, true].into_iter().flat_map(|hide_on_blur| {
        [None, Some(GRACE)].into_iter().flat_map(move |blur_grace| {
            [None, Some(MOVE_LIMIT)].into_iter().map(move |move_hold| Policy {
                hide_on_blur,
                blur_grace,
                move_hold,
            })
        })
    });
    for policy in policies {
        for seed in 0..400 {
            let mut choices = Choices(seed);
            let mut window = PickerWindow::new(policy, SIZE);
            let mut world = World::new(policy);
            let mut trace = Vec::new();
            for _ in 0..300 {
                let event = world.event(&mut choices, &window);
                let owner = window.owner();
                let effects = window.step(event);
                trace.push(format!("{event:?} -> {effects:?}"));
                if let Err(broken) = world.check(owner, event, &effects, &window) {
                    let recent = trace[trace.len().saturating_sub(25)..].join("\n  ");
                    panic!("{broken}\npolicy {policy:?}, seed {seed}, last events:\n  {recent}");
                }
            }
        }
    }
}
