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
            app_identity: None,
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
        policy: PolicyProof::default(),
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
            app_identity: None,
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
fn a_detached_windows_ticket_cannot_post_after_its_policy_is_revoked() {
    let desk = Fake::new("[@synthetic]");
    let ticket = ticket();
    ticket.policy.epoch.store(1, Ordering::Release);
    assert_eq!(
        deliver_ticket(&desk, &ticket, "[@synthetic]", true),
        copied(Some("paste-unavailable"))
    );
    assert!(desk.posts.borrow().is_empty());
    assert!(desk.restores.borrow().is_empty());
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
                app: None,
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
fn a_windows_copy_without_a_target_or_native_sequence_still_has_a_receipt() {
    for (target, sequence) in [(None, Some(17)), (Some(ticket().target), None)] {
        let mut pending = Some(ticket());
        let token = remember_ticket(
            &mut pending,
            CopyAttempt {
                policy: PolicyProof::default(),
                target,
                written: Instant::now(),
                stamp: SafetyStamp::now(),
            },
            sequence,
            19,
        );
        assert!(token.is_some());
        assert!(pending.is_none(), "a receipt is not authority to paste");
    }
}

#[test]
fn delayed_windows_bookkeeping_cannot_replace_a_newer_copy_proof() {
    let mut pending = None;
    for token in [2, 1] {
        remember_ticket(
            &mut pending,
            CopyAttempt {
                policy: PolicyProof::default(),
                target: Some(ticket().target),
                written: Instant::now(),
                stamp: SafetyStamp::now(),
            },
            Some(17),
            token,
        );
    }
    assert!(take_ticket(&mut pending, Some(1)).is_none());
    assert!(take_ticket(&mut pending, Some(2)).is_some());
}

#[test]
fn overlapping_copy_deliveries_leave_the_newer_windows_ticket_available() {
    for text_a in ["[@first]", "[@synthetic]"] {
        let desk = Fake::new("[@synthetic]");
        let pending = RefCell::new(None);
        let token_a = remember_ticket(
            &mut pending.borrow_mut(),
            CopyAttempt {
                policy: PolicyProof::default(),
                target: Some(ticket().target),
                written: Instant::now(),
                stamp: desk.stamp.get(),
            },
            Some(17),
            1,
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
                policy: PolicyProof::default(),
                target: Some(target_b),
                written: Instant::now(),
                stamp: desk.stamp.get(),
            },
            Some(17),
            2,
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
            policy: PolicyProof::default(),
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
        let token = remember_ticket(&mut pending, attempt, Some(17), 18);
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
