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
    presentation: Cell<u64>,
    hidden: bool,
    change_count: Cell<isize>,
    secure: Cell<Option<bool>>,
    query_delay: Duration,
    queue_delay: Duration,
    /// Real time a readiness check takes, as on a heavily loaded system.
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
    mouse_moved: Cell<Option<Instant>>,
    input_error: Option<f64>,
    checks: RefCell<Vec<&'static str>>,
    posts: RefCell<Vec<i32>>,
    pending: PendingPost,
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
            presentation: Cell::new(PRESENTATION),
            hidden: true,
            change_count: Cell::new(COUNT),
            secure: Cell::new(Some(false)),
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
            mouse_moved: Cell::new(None),
            input_error: None,
            checks: RefCell::default(),
            posts: RefCell::default(),
            pending: PendingPost::default(),
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
        })
    }
}

impl Desk for Fake {
    type Proof = CopyProof;

    fn delivery_began(&self) -> Instant {
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
        if let Some(window) = self.returning_windows.borrow().first() {
            self.window.set(*window);
        }
        Ok(self.front())
    }
    fn settle(&self) {}
    fn ready(&self, request: &DeliveryRequest, proof: &CopyProof) -> Result<bool, CopyReason> {
        std::thread::sleep(self.ready_stall);
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
        self.pending.run(|| checked_post(self, &request))
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
fn settings_failures_keep_the_citation_copied_after_reopening() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let path = directory.path().join("settings.json");
    let desk = Fake::new();
    let deliver = || with_paste_settings(&path, |enabled| super::deliver(&desk, Some(&proof()), enabled));
    assert_eq!(deliver(), pasted(), "first-run defaults still allow paste");
    desk.posts.borrow_mut().clear();
    std::fs::write(&path, r#"{"paste":false}"#).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(deliver(), copied(Some("paste-off")));
    // The same read is used again after closing and reopening the picker.
    for _ in 0..2 {
        std::fs::write(&path, "{").unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(path.with_extension("json.bak"), r#"{"paste":true}"#).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(deliver(), copied(Some("paste-unavailable")));
        std::fs::write(&path, format!("{{}}{}", " ".repeat(1024 * 1024))).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            deliver(),
            copied(Some("paste-unavailable")),
            "oversized settings cannot use backup consent"
        );
        std::fs::remove_file(&path).unwrap_or_else(|e| panic!("{e}"));
        std::fs::create_dir(&path).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            deliver(),
            copied(Some("paste-unavailable")),
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
            "secure-input",
            "change-count",
            "foreground",
            "input",
            "post"
        ]
    );
}

#[test]
fn readiness_waits_for_the_original_surface_even_while_its_app_stays_active() {
    let desk = Fake::new();
    desk.returning_windows.replace(vec![None, None, None, Some(100)]);
    assert_eq!(deliver(&desk, Some(&proof()), true), pasted());
    assert_eq!(desk.waits.get(), 3);
    assert_eq!(*desk.posted_windows.borrow(), [Some(100)]);
}

#[test]
fn readiness_does_not_add_a_fixed_sleep_to_an_already_focused_document() {
    let desk = Fake::new();
    assert_eq!(deliver(&desk, Some(&proof()), true), pasted());
    assert_eq!(desk.waits.get(), 0);
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
            assert_eq!(deliver(&desk, Some(&proof), true), copied(Some("target-changed")));
            assert!(desk.posts.borrow().is_empty());
            assert!(desk.posted_documents.borrow().is_empty());
            assert_eq!(proof.text, TEXT);
            assert_eq!(desk.waits.get(), 2);
        }
    }
}

#[test]
fn input_during_the_final_ax_read_and_unreadable_input_timing_never_post() {
    let desk = Fake {
        after_window_query: Some(|desk| {
            desk.clock.set(desk.clock.get() + Duration::from_millis(1));
            desk.document.set(2);
            desk.input_events.set([Some(desk.clock.get()), None, None, None]);
        }),
        ..Fake::new()
    };
    assert_eq!(deliver(&desk, Some(&proof()), true), copied(Some("target-changed")));
    assert!(desk.posts.borrow().is_empty());
    for value in [f64::NAN, f64::INFINITY, -1.0] {
        let desk = Fake {
            input_error: Some(value),
            ..Fake::new()
        };
        assert_eq!(deliver(&desk, Some(&proof()), true), copied(Some("paste-unavailable")));
        assert!(desk.posts.borrow().is_empty());
    }
}

#[test]
fn the_activating_enter_and_mouse_movement_do_not_cancel_a_paste() {
    let desk = Fake {
        after_return_wait: Some(|desk| desk.mouse_moved.set(Some(desk.clock.get()))),
        ..Fake::new()
    };
    desk.input_events
        .set([Some(desk.clock.get() - Duration::from_millis(1)), None, None, None]);
    desk.returning_windows.replace(vec![None, Some(100)]);
    assert_eq!(deliver(&desk, Some(&proof()), true), pasted());
    assert_eq!(*desk.posted_documents.borrow(), [1]);
}

#[test]
fn a_programmatic_interior_document_change_with_no_input_is_not_detected() {
    let desk = Fake {
        after_return_wait: Some(|desk| desk.document.set(2)),
        ..Fake::new()
    };
    desk.returning_windows.replace(vec![None, Some(100)]);
    assert_eq!(deliver(&desk, Some(&proof()), true), pasted());
    assert_eq!(*desk.posted_documents.borrow(), [2]);
}

/// Under a full compile a Windows readiness check took over a second. A
/// check that proves the right window is in front still pastes inside the
/// paste's deadline; past it the notice says the paste expired, never that
/// the window changed.
#[test]
fn a_slow_readiness_check_pastes_in_time_and_expires_after() {
    let desk = Fake {
        ready_stall: RETURN_LIMIT + Duration::from_millis(50),
        ..Fake::new()
    };
    assert_eq!(deliver(&desk, Some(&proof()), true), pasted());
    assert_eq!(*desk.posts.borrow(), [42]);

    let desk = Fake {
        ready_stall: LATE + Duration::from_millis(50),
        ..Fake::new()
    };
    assert_eq!(deliver(&desk, Some(&proof()), true), copied(Some("paste-expired")));
    assert!(desk.posts.borrow().is_empty());
}

#[test]
fn a_replacement_document_during_readiness_is_never_adopted() {
    let desk = Fake::new();
    desk.returning_windows.replace(vec![None, Some(200)]);
    let proof = proof();
    assert_eq!(deliver(&desk, Some(&proof), true), copied(Some("target-changed")));
    assert!(desk.posts.borrow().is_empty());
    assert_eq!(proof.text, TEXT);
    assert_eq!(desk.waits.get(), RETURN_CHECKS);
}

#[test]
fn reopening_the_picker_while_focus_returns_retires_the_old_delivery() {
    let desk = Fake {
        after_return_wait: Some(|desk| desk.presentation.set(PRESENTATION + 1)),
        ..Fake::new()
    };
    desk.returning_windows.replace(vec![None, Some(100)]);
    assert!(matches!(deliver(&desk, Some(&proof()), true), Delivered::Copied { .. }));
    assert!(desk.posts.borrow().is_empty());
}

#[test]
fn a_lifecycle_change_during_readiness_never_posts_after_focus_returns() {
    let desk = Fake {
        after_return_wait: Some(|desk| {
            desk.stamp.set(SafetyStamp {
                epoch: 2,
                ..desk.stamp.get()
            })
        }),
        ..Fake::new()
    };
    desk.returning_windows.replace(vec![None, Some(100)]);
    assert_eq!(deliver(&desk, Some(&proof()), true), copied(Some("paste-expired")));
    assert!(desk.posts.borrow().is_empty());
}

#[test]
#[cfg(target_os = "macos")]
fn repeated_show_from_our_key_panel_still_pastes_once_into_the_original_external_window() {
    let desk = Fake::new();
    let last = LastTarget::default();
    last.begin_presentation(Some(writer("com.microsoft.word")));
    let active_external = Front {
        app_identity: None,
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
        .write_copy_at(TEXT, desk.stamp.get(), |_| Ok(None), || Ok(Some(COUNT)))
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
            app_identity: None,
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
                .write_copy_at(TEXT, desk.stamp.get(), |_| Ok(None), || Ok(Some(COUNT)))
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
            assert_eq!(desk.asked.get(), 2);
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
fn only_the_final_input_fence_follows_the_last_window_check_before_posting() {
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
        &desk.checks.borrow()[desk.checks.borrow().len() - 3..],
        ["foreground", "input", "post"]
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
        .write_copy_at(
            TEXT,
            desk.stamp.get(),
            |_| Ok(None),
            || {
                desk.stamp.set(SafetyStamp {
                    epoch: 2,
                    elapsed: Duration::ZERO,
                });
                Ok(Some(COUNT))
            },
        )
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
            .write_copy(TEXT, |_| Ok(None), || Ok(Some(COUNT)))
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
            began: proof.written,
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
        .write_copy(TEXT, |_| Ok(None), || Ok(Some(COUNT)))
        .unwrap_or_else(|e| panic!("{e}"));
    let ticket = last.take_copy(TEXT, token).unwrap_or_else(|| panic!("copy proof"));
    assert_eq!(ticket.change_count, COUNT);
    assert_eq!(ticket.presentation, 0);
    assert_eq!(ticket.captured, Some(writer("com.microsoft.word")));
    assert!(last.take_copy(TEXT, token).is_none());
    let token = last
        .write_copy(TEXT, |_| Ok(None), || Ok(Some(COUNT)))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(last.take_copy("different citation", token).is_none());
    assert!(last.take_copy(TEXT, None).is_none());
    assert!(last.take_copy(TEXT, token).is_some());
    // A failed write or a concurrent replacement cannot reuse an old proof.
    for result in [Err("write failed".into()), Ok(None)] {
        let token = last
            .write_copy(TEXT, |_| Ok(None), || Ok(Some(COUNT)))
            .unwrap_or_else(|e| panic!("{e}"));
        let _ = last.write_copy(TEXT, |_| Ok(None), || result);
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
        .write_copy(TEXT, |_| Ok(None), || Ok(Some(COUNT)))
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
    assert_eq!(deliver(&desk, Some(&before), true), copied(Some("unknown-app")));
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
fn approved_unknown_apps_keep_window_accessibility_expiry_and_permission_guards() {
    for (window, failure, age, may_post, reason) in [
        (
            None,
            Some(CopyReason::AccessibilityRequired),
            Duration::ZERO,
            true,
            "accessibility-required",
        ),
        (None, None, Duration::ZERO, true, "paste-unavailable"),
        (Some(200), None, Duration::ZERO, true, "target-changed"),
        (Some(100), None, LATE, true, "paste-expired"),
        (Some(100), None, Duration::ZERO, false, "no-permission"),
    ] {
        let desk = Fake {
            app_id: "org.example.writer",
            may_post,
            ..Fake::new()
        };
        let mut captured = writer(desk.app_id);
        captured.paste = Ok(Some(AppPaste::Paste));
        captured.focus_failure = failure;
        if window.is_none() {
            captured.window = None;
        }
        desk.window.set(window);
        desk.stamp.set(SafetyStamp { epoch: 1, elapsed: age });
        let proof = CopyProof {
            captured: Some(captured),
            ..proof()
        };
        assert_eq!(deliver(&desk, Some(&proof), true), copied(Some(reason)));
        assert!(desk.posts.borrow().is_empty());
    }
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
    assert_eq!(deliver(&desk, Some(&proof), true), copied(Some("copy-only")));
    assert!(desk.posts.borrow().is_empty());
}

#[test]
fn unreadable_app_choices_copy_without_guessing_that_copy_only_was_off() {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let path = directory.path().join("apps.json");
    std::fs::write(&path, "unreadable invented preferences").unwrap_or_else(|e| panic!("{e}"));
    let memory = crate::destination::AppMemory::open(path);
    let desk = Fake::new();
    let last = LastTarget::default();
    last.begin_presentation(Some(writer(desk.app_id)));
    let token = last
        .write_copy_at(
            TEXT,
            desk.stamp.get(),
            |target| memory.for_capture(target),
            || Ok(Some(COUNT)),
        )
        .unwrap_or_else(|e| panic!("{e}"));
    let proof = last.take_copy(TEXT, token).unwrap_or_else(|| panic!("copy proof"));
    desk.presentation.set(proof.presentation);
    // Repairing preferences cannot authorize the already written citation.
    memory.forget().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(deliver(&desk, Some(&proof), true), copied(Some("paste-unavailable")));
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
    assert_eq!(deliver(&desk, Some(&proof), true), copied(Some("paste-unavailable")));
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
    assert_eq!(deliver(&desk, Some(&fresh), true), copied(Some("copy-only")));
    assert!(desk.posts.borrow().is_empty());
}

#[test]
fn a_delayed_app_answer_after_a_newer_copy_cannot_authorize_it_or_consume_its_proof() {
    let last = LastTarget::default();
    let approved = Cell::new(None);
    let desk = Fake {
        app_id: "org.example.writer",
        ..Fake::new()
    };
    last.begin_presentation(Some(writer(desk.app_id)));
    let a = last
        .write_copy_at(
            "[@older]",
            desk.stamp.get(),
            |_| Ok(approved.get()),
            || Ok(Some(COUNT - 1)),
        )
        .unwrap_or_else(|e| panic!("{e}"));
    let b = last
        .write_copy_at(TEXT, desk.stamp.get(), |_| Ok(approved.get()), || Ok(Some(COUNT)))
        .unwrap_or_else(|e| panic!("{e}"));
    // An answer held by A arrives after B has written the clipboard.
    approved.set(Some(AppPaste::Paste));
    assert!(last.take_copy("[@older]", a).is_none());
    let b = last
        .take_copy(TEXT, b)
        .unwrap_or_else(|| panic!("B still owns its proof"));
    assert_eq!(deliver(&desk, Some(&b), true), copied(Some("unknown-app")));
    assert!(desk.posts.borrow().is_empty());
    assert_eq!(b.text, TEXT, "the newest copy and its reason are retained");
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
