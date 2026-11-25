use super::*;
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
            selection: Selection::Unknown,
            search: None,
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
            presentation: 10,
        },
        clipboard: 17,
        written: Instant::now(),
        stamp: SafetyStamp {
            epoch: 1,
            elapsed: Duration::ZERO,
        },
    }
}

fn deliver(desk: &Fake, captured: Option<&Captured>, enabled: bool) -> Delivered {
    let ticket = DeliveryTicket {
        target: CopyTarget {
            captured: captured.cloned(),
            ..ticket().target
        },
        ..ticket()
    };
    deliver_ticket(desk, &ticket, enabled)
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
            selection: Selection::Unknown,
            search: None,
        }),
        7,
    )
    .unwrap_or_else(|| panic!("external Windows capture"))
}

fn copied(reason: &str) -> Delivered {
    Delivered::Copied {
        reason: serde_json::from_value(serde_json::json!(reason)).unwrap_or_else(|_| panic!("no copy reason {reason}")),
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
                app: None,
            }
        ));
        assert_eq!(
            deliver(&desk, Some(&captured), true),
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
            copied("unknown-app"),
        ),
        // A macOS bundle ID does not describe a known Windows executable.
        (
            Fake::new("[@synthetic]"),
            writer("com.microsoft.word"),
            true,
            copied("unknown-app"),
        ),
        // Another app replaced the clipboard.
        (
            Fake::new("other text"),
            writer("winword.exe"),
            true,
            copied("clipboard-changed"),
        ),
        // The switch is off.
        (
            Fake::new("[@synthetic]"),
            writer("winword.exe"),
            false,
            copied("paste-off"),
        ),
        // The captured app never came back.
        (
            Fake {
                fronts: RefCell::new(vec![7]),
                ..Fake::new("[@synthetic]")
            },
            writer("winword.exe"),
            true,
            copied("target-changed"),
        ),
        // Nothing was in front.
        (
            Fake::new("[@synthetic]"),
            Captured::unknown(System::Windows),
            true,
            copied("no-target"),
        ),
    ];
    for (desk, captured, enabled, expected) in cases {
        assert_eq!(deliver(&desk, Some(&captured), enabled), expected);
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
        deliver(&desk, Some(&writer("winword.exe")), true),
        copied("secure-field")
    );
    assert!(desk.posts.borrow().is_empty());

    let desk = Fake::new("[@synthetic]");
    let mut admin = writer("winword.exe");
    admin.target.elevated = true;
    assert_eq!(deliver(&desk, Some(&admin), true), copied("elevated-app"));
    assert!(desk.posts.borrow().is_empty());
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
            presentation: 11,
        },
        11,
    );
    publish_capture(&mut kept, delayed_a, 11);
    let b = kept.as_ref().unwrap_or_else(|| panic!("B capture"));
    assert_eq!(b.captured.as_ref(), Some(&app_b));
    assert_eq!(b.window, Some(200));
    assert_eq!(b.presentation, 11);
    // Showing again with Ibis in front retains B, with that show's epoch.
    publish_capture(
        &mut kept,
        CopyTarget {
            captured: None,
            window: Some(7),
            presentation: 12,
        },
        12,
    );
    let retained = kept.unwrap_or_else(|| panic!("retained B"));
    assert_eq!(retained.captured, Some(app_b));
    assert_eq!(retained.window, Some(200));
    assert_eq!(retained.presentation, 12);
}

#[test]
fn overlapping_copy_deliveries_leave_the_newer_windows_ticket_available() {
    fn tokens() -> Result<u32, String> {
        static WRITES: std::sync::Mutex<u32> = std::sync::Mutex::new(0);
        let mut token = WRITES.lock().map_err(|_| "poisoned")?;
        *token += 1;
        Ok(*token)
    }
    let desk = Fake::new("[@synthetic]");
    let state = std::sync::Mutex::new(TargetState::<usize>::default());
    let writes = std::sync::Mutex::new(());
    // Even a repeated native sequence cannot alias a write token.
    let write = |target| {
        let attempt = CopyAttempt {
            policy: PolicyProof::default(),
            target: Some(target),
            written: Instant::now(),
            stamp: desk.stamp.get(),
        };
        write_ticket(&state, &writes, tokens, Some(attempt), || Ok(Some(17))).unwrap_or_else(|error| panic!("{error}"))
    };
    let delivery = |token| {
        let frozen = state.lock().ok().and_then(|mut state| state.take_ticket(Some(token)));
        frozen.as_ref().map_or_else(
            || copied("paste-unavailable"),
            |ticket| deliver_ticket(&desk, ticket, true),
        )
    };
    let token_a = write(ticket().target);
    // A's worker has not taken its ticket when B copies on a new presentation.
    let token_b = write(CopyTarget {
        presentation: 12,
        ..ticket().target
    });
    assert_ne!(token_a, token_b);
    desk.generation.set(13);
    assert_eq!(delivery(token_a), copied("paste-unavailable"));
    assert!(desk.restores.borrow().is_empty());
    assert!(desk.posts.borrow().is_empty());
    assert_eq!(
        delivery(token_b),
        Delivered::Pasted {
            app: Some("Writer".into())
        }
    );
    assert_eq!(delivery(token_b), copied("paste-unavailable"));
    assert_eq!(*desk.posts.borrow(), [(42, 0x09)]);
    assert_eq!(desk.clipboard_reads.get(), 0);
}

#[test]
fn a_foreign_clipboard_owner_is_never_read_or_adopted_as_the_baseline() {
    for sequence in [Some(18), None] {
        let desk = Fake::new("foreign delayed text");
        let frozen = ticket(); // Sequence 17, sampled during Ibis's write.
        // Replacement occurs before the worker even starts.
        desk.sequence.set(sequence);
        assert_eq!(
            deliver_ticket(&desk, &frozen, true),
            copied(if sequence.is_some() {
                "clipboard-changed"
            } else {
                "paste-unavailable"
            })
        );
        assert_eq!(desk.clipboard_reads.get(), 0);
        assert!(desk.restores.borrow().is_empty());
        assert!(desk.posts.borrow().is_empty());
    }
    let desk = Fake::new("contents must not be requested even for our sequence");
    assert!(matches!(
        deliver_ticket(&desk, &ticket(), true),
        Delivered::Pasted { .. }
    ));
    assert_eq!(desk.clipboard_reads.get(), 0);
    assert_eq!(*desk.restores.borrow(), [(100, 42)]);
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
        let delivered = deliver_ticket(&desk, &ticket(), true);
        if inserted == 4 {
            assert!(matches!(delivered, Delivered::Pasted { .. }));
        } else {
            assert_eq!(delivered, copied("paste-unavailable"), "prefix {inserted}");
        }
        assert_eq!(*desk.events.borrow(), expected, "prefix {inserted}");
        assert!(!desk.control_down.get(), "Control left down by prefix {inserted}");
        assert!(!desk.v_down.get(), "V left down by prefix {inserted}");
    }
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
        assert_eq!(deliver_ticket(&desk, &ticket(), true), copied("key-release-failed"));
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
        assert_eq!(deliver_ticket(&desk, &ticket(), true), copied("key-release-failed"));
        assert_eq!(desk.posts.borrow().len(), 1);
        assert_eq!(desk.input_requests.borrow().len() - requests, 3 * releases);
        assert_eq!(desk.release_warnings.get(), 2);

        // The next copy-only delivery still releases the retained keys.
        assert_eq!(deliver_ticket(&desk, &ticket(), false), copied("paste-off"));
        assert!(!desk.keys_unreleased());
        assert!(!desk.control_down.get());
        assert!(!desk.v_down.get());
        assert_eq!(desk.posts.borrow().len(), 1);
        assert!(matches!(
            deliver_ticket(&desk, &ticket(), true),
            Delivered::Pasted { .. }
        ));
    }
}

#[test]
fn an_expired_delivery_can_release_pending_keys_but_cannot_inject_a_new_paste() {
    let desk = Fake::new("[@synthetic]");
    desk.keys.borrow_mut().control = true;
    desk.control_down.set(true);
    let mut ticket = ticket();
    ticket.written -= LATE;
    assert_eq!(deliver_ticket(&desk, &ticket, true), copied("paste-expired"));
    assert!(desk.posts.borrow().is_empty());
    assert_eq!(*desk.input_requests.borrow(), [vec![PasteEvent::ControlUp]]);
    assert!(desk.restores.borrow().is_empty(), "expired work must not steal focus");
    assert!(!desk.keys_unreleased());
    assert!(!desk.control_down.get());
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

#[test]
#[cfg(target_os = "macos")]
fn terminals_and_missing_permission_copy_with_a_reason_and_ask_once_per_copy() {
    let desk = Fake::new("[@synthetic]");
    assert_eq!(deliver(&desk, Some(&writer("cmd.exe")), true), copied("terminal"));
    let desk = Fake {
        may_post: false,
        ..Fake::new("[@synthetic]")
    };
    assert_eq!(
        deliver(&desk, Some(&writer("winword.exe")), true),
        copied("no-permission")
    );
    assert_eq!(desk.asked.get(), 1);
    assert!(desk.posts.borrow().is_empty());
}
