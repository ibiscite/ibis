use super::*;
use ibis_core::destination::{AppId, Target};
use std::cell::{Cell, RefCell};

const TEXT: &str = "[@invented]";
const COUNT: isize = 11;
const PRESENTATION: u64 = 8;

struct Fake {
    app_id: &'static str,
    may_post: bool,
    post_event_granted: Cell<bool>,
    asked: Cell<u32>,
    fronts: RefCell<Vec<i32>>,
    returning_windows: RefCell<Vec<Option<u32>>>,
    after_return_wait: Option<fn(&Fake)>,
    input_on_first_wait: Cell<Option<fn(&Fake)>>,
    waits: Cell<u32>,
    clock: Cell<Instant>,
    stamp: Cell<SafetyStamp>,
    window: Cell<Option<u32>>,
    focus_error: Cell<Option<CopyReason>>,
    /// Accessibility reads that time out before the app answers again, as an
    /// app busy taking focus back or handling a keystroke does.
    busy_returning: Cell<u32>,
    busy_final: Cell<u32>,
    presentation: Cell<u64>,
    hidden: bool,
    change_count: Cell<isize>,
    secure: Cell<Option<bool>>,
    /// The target's focused control hides what is typed, as a password field.
    hides_typing: bool,
    query_delay: Duration,
    queue_delay: Duration,
    /// How long a readiness check takes, as on a heavily loaded system.
    ready_stall: Duration,
    secure_after_queue: Option<bool>,
    front_after_queue: Option<i32>,
    replace_during_check: bool,
    after_check: Option<fn(&Fake)>,
    after_window_query: Option<fn(&Fake)>,
    in_final_query: Cell<bool>,
    replace_on_refresh_stamp: bool,
    focus_after_final_check: bool,
    posted_windows: RefCell<Vec<Option<u32>>>,
    document: Cell<u32>,
    posted_documents: RefCell<Vec<u32>>,
    input_events: Cell<[Option<Instant>; 4]>,
    input_error: Option<f64>,
    checks: RefCell<Vec<&'static str>>,
    posts: RefCell<Vec<i32>>,
    /// What the final selection read finds, or why it fails.
    selection_now: RefCell<Option<Result<Selection, CopyReason>>>,
}

impl Fake {
    fn new() -> Self {
        Self {
            app_id: "com.microsoft.word",
            may_post: true,
            post_event_granted: Cell::new(true),
            asked: Cell::new(0),
            fronts: RefCell::new(vec![42]),
            returning_windows: RefCell::new(Vec::new()),
            after_return_wait: None,
            input_on_first_wait: Cell::new(None),
            waits: Cell::new(0),
            clock: Cell::new(Instant::now()),
            stamp: Cell::new(SafetyStamp {
                epoch: 1,
                elapsed: Duration::ZERO,
            }),
            window: Cell::new(Some(100)),
            focus_error: Cell::new(None),
            busy_returning: Cell::new(0),
            busy_final: Cell::new(0),
            presentation: Cell::new(PRESENTATION),
            hidden: true,
            change_count: Cell::new(COUNT),
            secure: Cell::new(Some(false)),
            hides_typing: false,
            query_delay: Duration::ZERO,
            queue_delay: Duration::ZERO,
            ready_stall: Duration::ZERO,
            secure_after_queue: None,
            front_after_queue: None,
            replace_during_check: false,
            after_check: None,
            after_window_query: None,
            in_final_query: Cell::new(false),
            replace_on_refresh_stamp: false,
            focus_after_final_check: false,
            posted_windows: RefCell::default(),
            document: Cell::new(1),
            posted_documents: RefCell::default(),
            input_events: Cell::new([None; 4]),
            input_error: None,
            checks: RefCell::default(),
            posts: RefCell::default(),
            selection_now: RefCell::new(None),
        }
    }

    fn front(&self) -> Option<Front> {
        self.fronts.borrow().first().map(|&pid| Front {
            app_identity: None,
            id: Some(self.app_id.into()),
            pid,
            window: self.window.get(),
            focus_failure: None,
            name: None,
            elevated: false,
            selection: Selection::Unknown,
            search: None,
        })
    }
}

impl Desk for Fake {
    type Proof = CopyProof;

    fn delivery_began(&self) -> Instant {
        self.clock.get()
    }

    fn delivery_clock(&self) -> Instant {
        self.clock.get()
    }

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
        if self.busy_returning.get() > 0 {
            self.busy_returning.set(self.busy_returning.get() - 1);
            return Err(CopyReason::PasteUnavailable);
        }
        if let Some(window) = self.returning_windows.borrow().first() {
            self.window.set(*window);
        }
        Ok(self.front())
    }
    fn settle(&self) {}
    fn ready(&self, request: &DeliveryRequest, proof: &CopyProof) -> Result<bool, CopyReason> {
        self.clock.set(self.clock.get() + self.ready_stall);
        let front = Desk::frontmost(self)?;
        Ok(front.is_some_and(|front| {
            front.pid == request.pid
                && front.id.as_deref() == Some(&request.id)
                && front.window.is_some()
                && front.window == proof.captured.as_ref().and_then(|captured| captured.window)
        }))
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
        // Each final check owns its pending post, as on the Mac.
        PendingPost::default().run(|| checked_post(self, &request))
    }
    fn wait(&self, time: Duration) {
        self.waits.set(self.waits.get() + 1);
        self.clock.set(self.clock.get() + time);
        let mut fronts = self.fronts.borrow_mut();
        if fronts.len() > 1 {
            fronts.remove(0);
        }
        drop(fronts);
        let mut windows = self.returning_windows.borrow_mut();
        if windows.len() > 1 {
            windows.remove(0);
        }
        drop(windows);
        if self.waits.get() == 1
            && let Some(event) = self.input_on_first_wait.get()
        {
            event(self);
        }
        if let Some(change) = self.after_return_wait {
            change(self);
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
        if self.busy_final.get() > 0 {
            self.busy_final.set(self.busy_final.get() - 1);
            return Err(CopyReason::PasteUnavailable);
        }
        refresh()?;
        self.checks.borrow_mut().push("foreground");
        Ok(self.front())
    }
    fn refused(&self, reason: CopyReason) -> CopyReason {
        if self.hides_typing {
            CopyReason::SecureField
        } else {
            reason
        }
    }
    fn selection_unchanged(&self, selection: &Selection) -> Result<(), CopyReason> {
        self.checks.borrow_mut().push("selection");
        match (self.selection_now.borrow().as_ref(), selection) {
            (_, Selection::Unknown) => Ok(()),
            (Some(Ok(now)), expected) if now == expected => Ok(()),
            (Some(Ok(_)), _) => Err(CopyReason::SelectionNotShown),
            (Some(Err(reason)), _) => Err(*reason),
            (None, _) => Err(CopyReason::PasteUnavailable),
        }
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
    fn seconds_since_input(&self, event: InputPress) -> f64 {
        if let Some(error) = self.input_error {
            return error;
        }
        let index = match event {
            InputPress::Key => 0,
            InputPress::LeftMouse => 1,
            InputPress::RightMouse => 2,
            InputPress::OtherMouse => 3,
        };
        self.input_events.get()[index].map_or(f64::MAX, |event| self.clock.get().duration_since(event).as_secs_f64())
    }
    fn input_unchanged(&self, began: Instant) -> Result<(), PostResult> {
        self.checks.borrow_mut().push("input");
        check_input_since(self, began)
    }
    fn post(&self, pid: i32) -> bool {
        self.posted_windows.borrow_mut().push(self.window.get());
        self.posted_documents.borrow_mut().push(self.document.get());
        self.checks.borrow_mut().push("post");
        self.posts.borrow_mut().push(pid);
        true
    }
}

// This fake exercises macOS capture and PostEvent on every test host.
fn writer(id: &str) -> Captured {
    Captured {
        app_identity: capture::app_identity(System::Macos, id, "/invented/Writer.app"),
        paste: Ok(None),
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
        selection: Selection::Unknown,
        #[cfg(target_os = "macos")]
        diagnostic: None,
    }
}

#[test]
fn the_footer_uses_delivery_policy_without_asking_for_permission_or_posting() {
    let permission = PastePermission {
        enabled: true,
        granted: true,
        app: None,
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
        policy: PolicyProof::default(),
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

fn copied(reason: &str) -> Delivered {
    Delivered::Copied {
        reason: serde_json::from_value(serde_json::json!(reason)).unwrap_or_else(|_| panic!("no copy reason {reason}")),
    }
}

fn pasted() -> Delivered {
    Delivered::Pasted {
        app: Some("Writer".into()),
    }
}

#[test]
fn settings_failures_keep_the_citation_copied_after_reopening() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let path = directory.path().join("settings.json");
    let desk = Fake::new();
    let deliver = || with_paste_settings(&path, |enabled| super::deliver(&desk, Some(&proof()), enabled));
    assert_eq!(deliver(), pasted(), "first-run defaults still allow paste");
    desk.posts.borrow_mut().clear();
    std::fs::write(&path, r#"{"paste":false}"#).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(deliver(), copied("paste-off"));
    // The same read is used again after closing and reopening the picker.
    for _ in 0..2 {
        std::fs::write(&path, "{").unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(path.with_extension("json.bak"), r#"{"paste":true}"#).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(deliver(), copied("paste-unavailable"));
        std::fs::write(&path, format!("{{}}{}", " ".repeat(1024 * 1024))).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            deliver(),
            copied("paste-unavailable"),
            "oversized settings cannot use backup consent"
        );
        std::fs::remove_file(&path).unwrap_or_else(|e| panic!("{e}"));
        std::fs::create_dir(&path).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            deliver(),
            copied("paste-unavailable"),
            "failed I/O cannot use backup consent"
        );
        std::fs::remove_dir(&path).unwrap_or_else(|e| panic!("{e}"));
    }
    assert!(desk.posts.borrow().is_empty());
    assert_eq!(desk.asked.get(), 0);
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
            "selection",
            "secure-input",
            "change-count",
            "foreground",
            "input",
            "post"
        ]
    );
}

#[test]
fn input_switching_a_tab_or_conversation_while_focus_returns_refuses_even_with_the_same_ax_window() {
    let events: [fn(&Fake); 4] = [
        |desk| {
            let mut events = desk.input_events.get();
            events[0] = Some(desk.clock.get());
            desk.input_events.set(events);
        },
        |desk| {
            let mut events = desk.input_events.get();
            events[1] = Some(desk.clock.get());
            desk.input_events.set(events);
        },
        |desk| {
            let mut events = desk.input_events.get();
            events[2] = Some(desk.clock.get());
            desk.input_events.set(events);
        },
        |desk| {
            let mut events = desk.input_events.get();
            events[3] = Some(desk.clock.get());
            desk.input_events.set(events);
        },
    ];
    for app_id in ["com.google.chrome", "com.anthropic.claudefordesktop"] {
        for event in events {
            let desk = Fake { app_id, ..Fake::new() };
            desk.returning_windows.replace(vec![None, None, Some(100)]);
            let mut proof = proof();
            proof.captured = Some(writer(app_id));
            // One input switches the interior document at the first wait;
            // another wait follows it, before the original AX window returns.
            let desk = Fake {
                after_return_wait: Some(|desk| {
                    if desk.waits.get() == 1 {
                        desk.document.set(2);
                    }
                }),
                ..desk
            };
            // Use the same first-wait event for each of the four native types,
            // including a middle-click that closes the current Chrome tab.
            // The event itself is emitted by the fake while waiting below.
            desk.input_on_first_wait.set(Some(event));
            assert_eq!(deliver(&desk, Some(&proof), true), copied("target-changed"));
            assert!(desk.posts.borrow().is_empty());
            assert!(desk.posted_documents.borrow().is_empty());
            assert_eq!(proof.text, TEXT);
            assert_eq!(desk.waits.get(), 2);
        }
    }
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
            app_identity: None,
            id: Some("com.microsoft.word".into()),
            pid: 42,
            window: None,
            focus_failure: None,
            name: Some("Writer".into()),
            elevated: false,
            selection: Selection::Unknown,
            search: None,
        };
        let failed = macos::capture_front(Some(external()), 7, false, |_| Err(reason));
        last.begin_presentation(capture::captured(System::Macos, failed, 7));
        let copy = || {
            let token = last
                .write_copy_at(TEXT, desk.stamp.get(), |_| Ok(None), || Ok(Some(COUNT)))
                .unwrap_or_else(|error| panic!("{error}"));
            last.take_copy(TEXT, token).unwrap_or_else(|| panic!("proof"))
        };
        for _fresh_enter_in_failed_presentation in 0..2 {
            let proof = copy();
            assert_eq!(deliver(&desk, Some(&proof), true), copied(message));
            assert_eq!(proof.text, TEXT);
            assert!(desk.posts.borrow().is_empty());
        }
        if reason == CopyReason::AccessibilityRequired {
            assert_eq!(desk.asked.get(), 2);
        }
        let valid = macos::capture_front(Some(external()), 7, false, |_| Ok((100, Selection::Unknown, None)));
        last.begin_presentation(capture::captured(System::Macos, valid, 7));
        let proof = copy();
        desk.presentation.set(proof.presentation);
        assert_eq!(deliver(&desk, Some(&proof), true), pasted());
        assert_eq!(*desk.posted_windows.borrow(), [Some(100)]);
    }
}

#[test]
fn a_changed_closed_minimised_or_unproven_window_never_receives_the_paste() {
    for window in [Some(200), None] {
        let desk = Fake {
            window: Cell::new(window),
            ..Fake::new()
        };
        let proof = proof();
        assert_eq!(deliver(&desk, Some(&proof), true), copied("target-changed"));
        assert!(desk.posts.borrow().is_empty());
        assert_eq!(
            proof.text, TEXT,
            "the successful copy remains available for manual paste"
        );
    }
}

#[test]
fn a_copy_older_than_its_limit_on_the_delivery_clock_never_pastes() {
    // The fake clock stands in for a busy system: no real time passes.
    let desk = Fake::new();
    let proof = proof();
    desk.clock.set(proof.written + LATE);
    assert_eq!(deliver(&desk, Some(&proof), true), copied("paste-expired"));
    assert!(desk.posts.borrow().is_empty());
}

#[test]
fn a_superseded_claimed_worker_cannot_post_even_if_citation_and_counter_match() {
    let last = LastTarget::default();
    last.begin_presentation(Some(writer("com.microsoft.word")));
    let copy = || {
        let token = last
            .write_copy(TEXT, &[], |_| Ok(None), || Ok(Some(COUNT)))
            .unwrap_or_else(|e| panic!("{e}"));
        last.take_copy(TEXT, token).unwrap_or_else(|| panic!("proof"))
    };
    let request = |proof: &CopyProof| {
        PostRequest::new(
            DeliveryRequest {
                pid: 42,
                id: "com.microsoft.word".into(),
                began: proof.written,
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
fn a_concurrent_destination_holder_cannot_block_windows_paste_or_queue_key_downs() {
    let last = Arc::new(LastTarget::default());
    let (sent, received) = std::sync::mpsc::channel();
    // Model an IPC command holding destination ownership on the UI thread.
    let holder = last.0.lock().unwrap_or_else(|e| panic!("{e}"));
    let worker_target = last.clone();
    let posted = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker_posted = posted.clone();
    let worker = std::thread::spawn(move || {
        let result = worker_target.post_if_ready(|| {
            worker_posted.store(true, Ordering::Release);
            true
        });
        sent.send(result).unwrap_or_else(|e| panic!("{e}"));
    });
    let result = received.recv_timeout(Duration::from_secs(1));
    // Release even on a failed assertion: the regression must never hang tests.
    drop(holder);
    worker.join().unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(
        result.ok(),
        Some(false),
        "paste must refuse while the UI owns destination state"
    );
    assert!(
        !posted.load(Ordering::Acquire),
        "refused input must not run after the holder releases"
    );
}

#[test]
fn an_app_that_never_answers_in_time_is_copied_with_that_reason_and_never_pasted() {
    for desk in [
        Fake {
            busy_returning: Cell::new(u32::MAX),
            ..Fake::new()
        },
        Fake {
            busy_final: Cell::new(u32::MAX),
            ..Fake::new()
        },
    ] {
        assert_eq!(deliver(&desk, Some(&proof()), true), copied("paste-unavailable"));
        assert!(desk.posts.borrow().is_empty());
    }
}

#[test]
fn a_field_that_hides_typing_says_so_whether_secure_typing_or_an_unproved_target_refused_it() {
    for desk in [
        Fake {
            secure: Cell::new(Some(true)),
            hides_typing: true,
            ..Fake::new()
        },
        Fake {
            focus_error: Cell::new(Some(CopyReason::PasteUnavailable)),
            hides_typing: true,
            ..Fake::new()
        },
    ] {
        assert_eq!(deliver(&desk, Some(&proof()), true), copied("secure-field"));
        assert!(desk.posts.borrow().is_empty());
    }
}

#[test]
fn clipboard_replaced_during_the_final_check_is_never_read_or_pasted() {
    let desk = Fake {
        replace_during_check: true,
        ..Fake::new()
    };
    assert_eq!(deliver(&desk, Some(&proof()), true), copied("clipboard-changed"));
    assert!(desk.posts.borrow().is_empty());
    assert_eq!(*desk.checks.borrow(), ["foreground", "secure-input", "change-count"]);
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
fn overlapping_copy_deliveries_leave_the_newer_proof_available() {
    for text_a in ["[@first]", TEXT] {
        let last = LastTarget::default();
        let desk = Fake::new();
        last.begin_presentation(Some(writer("com.microsoft.word")));
        let token_a = last
            .write_copy_at(text_a, desk.stamp.get(), |_| Ok(None), || Ok(Some(COUNT)))
            .unwrap_or_else(|e| panic!("{e}"));
        // A's worker is queued but has not taken its proof yet.
        let delivery_a = || {
            let proof = last.take_copy(text_a, token_a);
            deliver(&desk, proof.as_ref(), true)
        };
        last.begin_presentation(None);
        let token_b = last
            .write_copy_at(TEXT, desk.stamp.get(), |_| Ok(None), || Ok(Some(COUNT + 1)))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_ne!(token_a, token_b);
        desk.presentation.set(2);
        desk.change_count.set(COUNT + 1);
        let reason = if SYSTEM == System::Linux {
            "unsupported-system"
        } else {
            "no-target"
        };
        assert_eq!(delivery_a(), copied(reason));
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
        .write_copy(TEXT, &[], |_| Ok(None), || Ok(Some(COUNT)))
        .unwrap_or_else(|e| panic!("{e}"));
    let ticket = last.take_copy(TEXT, token).unwrap_or_else(|| panic!("copy proof"));
    assert_eq!(ticket.change_count, COUNT);
    assert_eq!(ticket.presentation, 0);
    assert_eq!(ticket.captured, Some(writer("com.microsoft.word")));
    assert!(last.take_copy(TEXT, token).is_none());
    let token = last
        .write_copy(TEXT, &[], |_| Ok(None), || Ok(Some(COUNT)))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(last.take_copy("different citation", token).is_none());
    assert!(last.take_copy(TEXT, None).is_none());
    assert!(last.take_copy(TEXT, token).is_some());
    // A failed write or a concurrent replacement cannot reuse an old proof.
    for result in [Err("write failed".into()), Ok(None)] {
        let token = last
            .write_copy(TEXT, &[], |_| Ok(None), || Ok(Some(COUNT)))
            .unwrap_or_else(|e| panic!("{e}"));
        let _ = last.write_copy(TEXT, &[], |_| Ok(None), || result);
        assert!(last.take_copy(TEXT, token).is_none());
    }
}

#[test]
fn unsafe_unproven_and_disabled_targets_only_copy() {
    let no_target = if SYSTEM == System::Linux {
        "unsupported-system"
    } else {
        "no-target"
    };
    for (captured, enabled, reason) in [
        (Some(writer("org.example.other")), true, "unknown-app"),
        (Some(writer("com.microsoft.word")), false, "paste-off"),
        (None, true, no_target),
    ] {
        let desk = Fake::new();
        let ticket = CopyProof { captured, ..proof() };
        assert_eq!(deliver(&desk, Some(&ticket), enabled), copied(reason));
        assert!(desk.posts.borrow().is_empty());
    }
}

#[test]
fn unknown_app_approval_is_frozen_before_io_and_each_fresh_enter_pastes_once() {
    let last = LastTarget::default();
    let approved = Cell::new(None);
    let desk = Fake {
        app_id: "org.example.writer",
        ..Fake::new()
    };
    last.begin_presentation(Some(writer(desk.app_id)));
    let token = last
        .write_copy_at(
            TEXT,
            desk.stamp.get(),
            |_| Ok(approved.get()),
            || {
                // Even an approval arriving during the write cannot release this copy.
                approved.set(Some(AppPaste::Paste));
                Ok(Some(COUNT))
            },
        )
        .unwrap_or_else(|e| panic!("{e}"));
    let before = last.take_copy(TEXT, token).unwrap_or_else(|| panic!("copy proof"));
    assert_eq!(deliver(&desk, Some(&before), true), copied("unknown-app"));
    assert!(desk.posts.borrow().is_empty());
    let token = last
        .write_copy_at(TEXT, desk.stamp.get(), |_| Ok(approved.get()), || Ok(Some(COUNT)))
        .unwrap_or_else(|e| panic!("{e}"));
    let after = last
        .take_copy(TEXT, token)
        .unwrap_or_else(|| panic!("fresh copy proof"));
    desk.presentation.set(after.presentation);
    assert_eq!(deliver(&desk, Some(&after), true), pasted());
    assert_eq!(*desk.posts.borrow(), [42]);
    assert!(last.take_copy(TEXT, token).is_none(), "one delivery per fresh copy");
}

#[test]
fn a_remembered_copy_only_choice_never_posts() {
    let desk = Fake {
        app_id: "org.example.writer",
        ..Fake::new()
    };
    let mut captured = writer(desk.app_id);
    captured.paste = Ok(Some(AppPaste::CopyOnly));
    let proof = CopyProof {
        captured: Some(captured),
        ..proof()
    };
    assert_eq!(deliver(&desk, Some(&proof), true), copied("copy-only"));
    assert!(desk.posts.borrow().is_empty());
}

#[test]
fn a_saved_delivery_change_revokes_a_detached_proof_without_touching_the_clipboard() {
    let last = LastTarget::default();
    let desk = Fake {
        app_id: "org.example.writer",
        ..Fake::new()
    };
    last.begin_presentation(Some(writer(desk.app_id)));
    let token = last
        .write_copy_at(
            TEXT,
            desk.stamp.get(),
            |_| Ok(Some(AppPaste::Paste)),
            || Ok(Some(COUNT)),
        )
        .unwrap_or_else(|e| panic!("{e}"));
    let proof = last.take_copy(TEXT, token).unwrap_or_else(|| panic!("copy proof"));
    // The worker already took its proof; removing the pending slot alone cannot revoke it.
    last.0.lock().unwrap_or_else(|e| panic!("{e}")).invalidate_policy();
    assert_eq!(deliver(&desk, Some(&proof), true), copied("paste-unavailable"));
    assert!(desk.posts.borrow().is_empty());
    assert_eq!(desk.change_count.get(), COUNT);
    let fresh = last
        .write_copy_at(
            TEXT,
            desk.stamp.get(),
            |_| Ok(Some(AppPaste::CopyOnly)),
            || Ok(Some(COUNT)),
        )
        .unwrap_or_else(|e| panic!("{e}"));
    let fresh = last.take_copy(TEXT, fresh).unwrap_or_else(|| panic!("fresh proof"));
    assert_eq!(deliver(&desk, Some(&fresh), true), copied("copy-only"));
    assert!(desk.posts.borrow().is_empty());
}

#[test]
#[cfg(target_os = "macos")]
fn terminals_and_missing_postevent_permission_keep_the_existing_flow() {
    let desk = Fake::new();
    let terminal = CopyProof {
        captured: Some(writer("com.apple.Terminal")),
        ..proof()
    };
    assert_eq!(deliver(&desk, Some(&terminal), true), copied("terminal"));
    let desk = Fake {
        may_post: false,
        ..Fake::new()
    };
    assert_eq!(deliver(&desk, Some(&proof()), true), copied("no-permission"));
    assert_eq!(desk.asked.get(), 1);
    assert!(desk.posts.borrow().is_empty());
}

fn with_selection(text: &str) -> CopyProof {
    let mut proof = proof();
    if let Some(captured) = &mut proof.captured {
        captured.selection = Selection::of_text(text).0;
    }
    proof
}

#[test]
fn enter_pastes_over_the_selection_the_picker_opened_on() {
    let desk = Fake::new();
    *desk.selection_now.borrow_mut() = Some(Ok(Selection::of_text("smith 2020").0));
    assert_eq!(deliver(&desk, Some(&with_selection("smith 2020")), true), pasted());
    assert_eq!(*desk.posts.borrow(), [42]);
    assert!(desk.checks.borrow().contains(&"selection"));
}

#[test]
fn a_selection_changed_before_enter_is_left_alone_and_says_why() {
    for now in ["jones 2021", ""] {
        let desk = Fake::new();
        *desk.selection_now.borrow_mut() = Some(Ok(Selection::of_text(now).0));
        assert_eq!(
            deliver(&desk, Some(&with_selection("smith 2020")), true),
            copied("selection-not-shown")
        );
        assert!(desk.posts.borrow().is_empty());
    }
}

#[test]
fn an_opening_that_could_not_read_never_pastes_over_text_found_at_enter() {
    let unread = || {
        let mut proof = proof();
        if let Some(captured) = &mut proof.captured {
            captured.selection = Selection::Unread;
        }
        proof
    };
    let desk = Fake::new();
    *desk.selection_now.borrow_mut() = Some(Err(CopyReason::SelectionNotShown));
    assert_eq!(deliver(&desk, Some(&unread()), true), copied("selection-not-shown"));
    assert!(desk.posts.borrow().is_empty());
    let empty = Fake::new();
    *empty.selection_now.borrow_mut() = Some(Ok(Selection::Unread));
    assert_eq!(deliver(&empty, Some(&unread()), true), pasted());
    assert_eq!(*empty.posts.borrow(), [42]);
}

fn replacing(old: &str) -> CopyProof {
    let mut proof = proof();
    if let Some(captured) = &mut proof.captured {
        captured.selection = Selection::Citation(capture::OldCitation::of(old));
    }
    proof
}

#[test]
fn a_changed_citation_pastes_over_the_selected_old_citation() {
    let desk = Fake::new();
    *desk.selection_now.borrow_mut() = Some(Ok(Selection::Citation(capture::OldCitation::of("(Smith, 2020)"))));
    assert_eq!(deliver(&desk, Some(&replacing("(Smith, 2020)")), true), pasted());
    assert_eq!(*desk.posts.borrow(), [42]);
}

#[test]
fn a_changed_citation_never_pastes_when_the_old_one_is_not_selected() {
    let desk = Fake::new();
    *desk.selection_now.borrow_mut() = Some(Err(CopyReason::CitationNotSelected));
    assert_eq!(
        deliver(&desk, Some(&replacing("(Smith, 2020)")), true),
        copied("citation-not-selected")
    );
    assert!(desk.posts.borrow().is_empty());
}
