use super::*;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

struct Notifications {
    state: Arc<Mutex<Owner>>,
    now: Instant,
    live: Rc<RefCell<Sample>>,
    reads: Rc<Cell<usize>>,
    queue: VecDeque<Box<dyn FnOnce()>>,
}

impl Notifications {
    fn new() -> Self {
        let native = Native::new();
        Self {
            state: Arc::new(Mutex::new(native.owner)),
            now: native.now,
            live: Rc::new(RefCell::new(native.live)),
            reads: Rc::new(Cell::new(0)),
            queue: VecDeque::new(),
        }
    }

    fn notify(&mut self) {
        let state = self.state.clone();
        let reads = self.reads.clone();
        let live = self.live.clone();
        let now = self.now;
        display_changed(&self.state, now, || {
            self.queue.push_back(Box::new(move || {
                display_settled(&state, || {
                    reads.set(reads.get() + 1);
                    locked(&state).observe(live.borrow().clone(), now, true);
                });
            }));
        });
    }

    fn dispatch(&mut self) {
        // Only queued work runs: no manual MainEventsCleared or unrelated input.
        while let Some(task) = self.queue.pop_front() {
            task();
        }
    }
}

#[test]
fn sizing_display_notification_without_other_events_queues_one_read() {
    let mut native = Notifications::new();
    native.live.borrow_mut().display.room = (800.0, 600.0);
    native.notify();
    assert_eq!(
        native.queue.len(),
        1,
        "a GTK notification alone must enqueue a Tao turn"
    );
    assert_eq!(
        native.reads.get(),
        0,
        "a notification cannot read during the native turn"
    );
    native.dispatch();
    assert_eq!(native.reads.get(), 1);
    assert_eq!(
        locked(&native.state).last.as_ref().map(|sample| sample.display.room),
        Some((800.0, 600.0))
    );
    for _ in 0..100 {
        native.dispatch();
        settled(&native.state, || panic!("the queued call cannot rearm itself"));
    }
    assert_eq!(native.reads.get(), 1);
    assert!(native.queue.is_empty());
}

#[test]
fn sizing_display_notification_storm_has_one_queued_wake_and_reads_the_latest_area() {
    let mut native = Notifications::new();
    for height in 600..700 {
        native.live.borrow_mut().display.room = (1000.0, f64::from(height));
        native.notify();
    }
    assert_eq!(native.queue.len(), 1);
    native.dispatch();
    assert_eq!(native.reads.get(), 1);
    assert_eq!(
        locked(&native.state).last.as_ref().map(|sample| sample.display.room),
        Some((1000.0, 699.0))
    );
    native.notify();
    assert_eq!(
        native.queue.len(),
        1,
        "a later real notification can request a fresh read"
    );
    native.dispatch();
    assert_eq!(native.reads.get(), 2);
}

#[test]
fn sizing_a_queued_display_wake_does_not_repeat_a_read_already_consumed_by_cleared() {
    let mut native = Notifications::new();
    native.notify();
    settled(&native.state, || native.reads.set(native.reads.get() + 1));
    native.dispatch();
    assert_eq!(native.reads.get(), 1);
    native.notify();
    assert_eq!(
        native.queue.len(),
        1,
        "the drained wake releases coalescing for a later event"
    );
    native.dispatch();
    assert_eq!(native.reads.get(), 2);
}

#[test]
fn sizing_cleared_events_do_not_release_a_still_queued_display_wake() {
    let mut native = Notifications::new();
    native.notify();
    settled(&native.state, || {});
    for _ in 0..100 {
        native.notify();
    }
    assert_eq!(
        native.queue.len(),
        1,
        "consuming the read must not duplicate the queued task"
    );
    native.dispatch();
    assert_eq!(native.reads.get(), 1);
}

#[test]
fn sizing_failed_display_enqueue_retries_only_on_a_later_notification() {
    let mut native = Notifications::new();
    native.notify();
    native.queue.clear(); // The app rejected this enqueue: no callback will run.
    display_wake_failed(&native.state);
    native.dispatch();
    assert_eq!(native.reads.get(), 0);
    assert!(native.queue.is_empty(), "failure cannot schedule its own retry");
    native.notify();
    assert_eq!(native.queue.len(), 1);
    native.dispatch();
    assert_eq!(native.reads.get(), 1);
}

#[test]
fn sizing_close_reopen_keeps_one_queued_wake_and_reads_the_current_opening() {
    let mut native = Notifications::new();
    native.notify();
    locked(&native.state).flush(native.now, false);
    locked(&native.state).prepared(native.live.borrow().clone());
    for height in 600..700 {
        native.live.borrow_mut().display.room = (1000.0, f64::from(height));
        native.notify();
    }
    assert_eq!(
        native.queue.len(),
        1,
        "reopen must not forget an existing queued callback"
    );
    native.dispatch();
    assert_eq!(native.reads.get(), 1);
    assert_eq!(
        locked(&native.state).last.as_ref().map(|sample| sample.display.room),
        Some((1000.0, 699.0))
    );
}

fn locked(state: &Mutex<Owner>) -> std::sync::MutexGuard<'_, Owner> {
    state.lock().unwrap_or_else(|_| panic!("synthetic owner poisoned"))
}

#[test]
fn sizing_idle_cleared_events_do_no_native_reads_hidden_or_visible() {
    for visible in [false, true] {
        let state = Mutex::new(Native::new().owner);
        let reads = Cell::new(0);
        let samples = Cell::new(0);
        for _ in 0..100 {
            settled(&state, || {
                reads.set(reads.get() + 1);
                if visible {
                    samples.set(samples.get() + 1);
                }
            });
        }
        assert_eq!(reads.get(), 0, "idle cleared events must not even look up the window");
        assert_eq!(samples.get(), 0);
    }
}

#[test]
fn sizing_one_change_consumes_one_read_even_when_the_getter_wakes_the_loop() {
    let mut native = Native::new();
    native.live.frame = frame(840, 600);
    let state = Mutex::new(native.owner);
    let reads = Cell::new(0);
    locked(&state).event(Event::Geometry, native.now);
    for _ in 0..100 {
        settled(&state, || {
            reads.set(reads.get() + 1);
            locked(&state).observe(native.live.clone(), native.now, true);
        });
    }
    assert_eq!(reads.get(), 1);
    assert_eq!(locked(&state).windows["a"].width, 840);
}

#[test]
fn sizing_failed_or_hidden_reads_are_consumed_and_show_prepares_a_fresh_sample() {
    for visibility in [Ok(false), Err(())] {
        let native = Native::new();
        let state = Mutex::new(native.owner);
        locked(&state).event(Event::Geometry, native.now);
        let reads = Cell::new(0);
        for _ in 0..100 {
            settled(&state, || {
                reads.set(reads.get() + 1);
                if visibility != Ok(true) {
                    return;
                }
                panic!("hidden windows and failed visibility reads must not sample geometry");
            });
        }
        assert_eq!(reads.get(), 1);
        let mut reopened = native.live;
        reopened.display.room = (800.0, 600.0);
        // Every show already samples and prepares while hidden, before presentation.
        locked(&state).prepared(reopened);
        settled(&state, || panic!("show preparation already supplied the sample"));
        assert_eq!(
            locked(&state)
                .constraints
                .as_ref()
                .unwrap_or_else(|| panic!("missing prepared sample"))
                .room,
            (800.0, 600.0)
        );
    }
}

#[test]
fn sizing_dpi_alone_keeps_its_read_until_after_the_native_turn() {
    let native = Native::new();
    let state = Mutex::new(native.owner);
    locked(&state).event(
        Event::Dpi {
            scale: 2.0,
            logical: (720, 480),
        },
        native.now,
    );
    let mut live = native.live;
    live.display.scale = 2.0;
    live.frame = frame(360, 240); // Before SetWindowPos: old physical client area.
    locked(&state).observe(live.clone(), native.now, false);
    live.frame = frame(720, 480); // The native turn completes; no extra resize event is required.
    let reads = Cell::new(0);
    settled(&state, || {
        reads.set(reads.get() + 1);
        locked(&state).observe(live, native.now, true);
    });
    settled(&state, || panic!("the getter's wakeup cannot read again"));
    assert_eq!(reads.get(), 1);
    assert_eq!(
        locked(&state)
            .last
            .as_ref()
            .unwrap_or_else(|| panic!("missing prepared sample"))
            .display
            .scale,
        2.0
    );
    assert!(
        locked(&state).windows.is_empty(),
        "automatic DPI resize is not saved as a drag"
    );
}

#[test]
fn sizing_a_change_during_a_read_belongs_to_the_next_cleared_event() {
    let native = Native::new();
    let state = Mutex::new(native.owner);
    locked(&state).event(Event::Geometry, native.now);
    settled(&state, || {
        locked(&state).event(Event::Geometry, native.now);
        // Accepting this sample cannot consume the reentrant event's read.
        locked(&state).observe(native.live, native.now, true);
    });
    let reads = Cell::new(0);
    settled(&state, || reads.set(reads.get() + 1));
    settled(&state, || panic!("the second change is consumed"));
    assert_eq!(reads.get(), 1);
}

#[test]
fn sizing_refused_changes_never_request_native_reads() {
    let native = Native::new();
    for owner in [
        Owner::default(),
        Owner {
            applying: true,
            ..Native::new().owner
        },
        Owner {
            stopping: true,
            ..native.owner
        },
    ] {
        let state = Mutex::new(owner);
        locked(&state).event(Event::Geometry, native.now);
        settled(&state, || panic!("refused change must not look up a native window"));
        display_changed(&state, native.now, || {
            panic!("refused notification cannot queue a wake")
        });
    }
}

#[test]
fn sizing_close_retires_a_pending_read_and_reopen_accepts_the_next_change() {
    let native = Native::new();
    let state = Mutex::new(native.owner);
    locked(&state).event(Event::Geometry, native.now);
    locked(&state).flush(native.now, false);
    settled(&state, || panic!("close retired the old opening's read"));
    locked(&state).prepared(native.live);
    locked(&state).event(Event::Geometry, native.now);
    let reads = Cell::new(0);
    for _ in 0..100 {
        settled(&state, || reads.set(reads.get() + 1));
    }
    assert_eq!(reads.get(), 1);
}

fn frame(width: u32, height: u32) -> PickerWindow {
    PickerWindow {
        width,
        height,
        x: 40,
        y: 60,
        custom_size: false,
    }
}

fn screen(key: &str, scale: f64, room: (f64, f64)) -> Display {
    Display {
        key: key.into(),
        scale,
        origin: (0.0, 0.0),
        room,
    }
}

struct Native {
    now: Instant,
    owner: Owner,
    live: Sample,
    maximum: (f64, f64),
}

impl Native {
    fn new() -> Self {
        let live = Sample {
            display: screen("a", 1.0, (1000.0, 700.0)),
            frame: frame(720, 480),
        };
        let mut owner = Owner::default();
        owner.prepared(live.clone());
        Self {
            now: Instant::now(),
            owner,
            maximum: live.display.room,
            live,
        }
    }
    fn advance(&mut self, ms: u64) {
        self.now += Duration::from_millis(ms);
    }
    fn event(&mut self, event: Event) {
        self.owner.event(event, self.now);
    }
    fn settle(&mut self) {
        if !self.owner.take_read() {
            return;
        }
        let frame = self.live.frame;
        if let Some(display) = self.owner.observe(self.live.clone(), self.now, true) {
            self.maximum = display.room;
            self.owner.constraints = Some(display);
        }
        assert_eq!(
            self.live.frame, frame,
            "refreshing constraints cannot resize or move the picker"
        );
    }
    fn drag(&mut self, width: u32, height: u32) {
        self.live.frame = frame(width, height);
        self.event(Event::Geometry);
        self.settle();
    }
}

#[test]
fn sizing_rejects_negative_uninitialized_and_nonfinite_chrome_before_first_show() {
    for outer in [(40.0, 60.0), (0.0, 0.0), (720.0, 0.0), (f64::NAN, 480.0)] {
        assert!(content_room((720.0, 480.0), outer, (1000.0, 700.0)).is_none());
    }
    // GTK's direct GDK frame extents, unlike tao's pre-configure position cache.
    let room = content_room((720.0, 480.0), (736.0, 519.0), (1000.0, 700.0))
        .unwrap_or_else(|| panic!("valid synthetic frame extents rejected"));
    assert_eq!(room, (984.0, 661.0));
}

#[test]
fn sizing_dpi_dispatch_does_not_turn_the_old_client_area_into_a_new_drag() {
    let mut native = Native::new();
    let mut custom = frame(840, 600);
    custom.custom_size = true;
    native.owner.windows.insert("b".into(), custom);
    native.live.display = screen("b", 2.0, (1280.0, 800.0));
    // WM_DPICHANGED reports 200% before SetWindowPos doubles the client area.
    native.live.frame = frame(360, 240);
    native.event(Event::Dpi {
        scale: 2.0,
        logical: (720, 480),
    });
    native.owner.observe(native.live.clone(), native.now, false);
    assert_eq!(native.owner.windows["b"], custom);
    assert_eq!(native.owner.revision, 0, "unsettled DPI geometry is never accepted");
    native.live.frame = frame(720, 480);
    native.event(Event::Geometry);
    native.settle();
    assert_eq!((native.live.frame.width, native.live.frame.height), (720, 480));
    assert_eq!(native.owner.windows["b"], custom);
    native.drag(900, 640);
    assert_eq!(
        (native.owner.windows["b"].width, native.owner.windows["b"].height),
        (900, 640)
    );
}

#[test]
fn sizing_new_display_and_work_area_refresh_the_maximum_without_moving_the_frame() {
    let mut native = Native::new();
    native.live.display = screen("b", 1.0, (2000.0, 1200.0));
    native.event(Event::Geometry);
    native.settle();
    assert_eq!(native.maximum, (2000.0, 1200.0));
    native.live.display.room = (2200.0, 1300.0);
    native.event(Event::Geometry); // OS work-area notification, without a window resize.
    native.settle();
    assert_eq!(native.maximum, (2200.0, 1300.0));
    native.drag(1800, 1000);
    assert!(f64::from(native.live.frame.width) <= native.maximum.0);
}

#[test]
fn sizing_a_real_edge_drag_during_a_dpi_transition_still_chooses_the_new_size() {
    let mut native = Native::new();
    native.live.display = screen("b", 2.0, (1280.0, 800.0));
    native.event(Event::Dpi {
        scale: 2.0,
        logical: (720, 480),
    });
    native.drag(900, 640);
    assert_eq!(
        (native.owner.windows["b"].width, native.owner.windows["b"].height),
        (900, 640)
    );
    let revision = native.owner.revision;
    native.event(Event::Dpi {
        scale: 2.0,
        logical: (720, 480),
    }); // delayed old suggestion
    native.settle();
    assert_eq!(native.owner.revision, revision);
    assert_eq!(
        (native.owner.windows["b"].width, native.owner.windows["b"].height),
        (900, 640)
    );
}

#[test]
fn sizing_an_old_dpi_suggestion_cannot_consume_a_new_drag_at_the_already_accepted_scale() {
    let mut native = Native::new();
    native.live.display = screen("b", 2.0, (1280.0, 800.0));
    native.event(Event::Dpi {
        scale: 2.0,
        logical: (720, 480),
    });
    native.settle();
    native.drag(800, 580);
    native.live.frame = frame(840, 600);
    native.event(Event::Geometry);
    // An old automatic suggestion happens to equal the person's latest drag.
    native.event(Event::Dpi {
        scale: 2.0,
        logical: (840, 600),
    });
    native.settle();
    assert_eq!(
        (native.owner.windows["b"].width, native.owner.windows["b"].height),
        (840, 600)
    );
}

#[test]
fn sizing_reset_undo_restores_the_visible_frame_and_only_the_removed_display_setting() {
    let mut native = Native::new();
    native.drag(840, 600);
    let previous = native.live.clone();
    let saved = native.owner.windows["a"];
    native.owner.windows.insert("b".into(), frame(900, 640));
    let revision = native
        .owner
        .reset("a", previous.clone(), native.now)
        .unwrap_or_else(|| panic!("reset refused"));
    assert!(!native.owner.windows.contains_key("a"));
    let undo = native
        .owner
        .undo_for(revision)
        .unwrap_or_else(|| panic!("reset has no Undo"));
    assert_eq!(undo.previous.frame, previous.frame);
    assert!(native.owner.restore(revision, native.now));
    assert_eq!(native.owner.windows["a"], saved);
    assert_eq!(native.owner.windows["b"], frame(900, 640));
    assert!(!native.owner.restore(revision, native.now));
}

#[test]
fn sizing_later_drag_reset_and_expiry_each_retire_only_their_own_undo() {
    let mut native = Native::new();
    native.drag(840, 600);
    let a = native
        .owner
        .reset("a", native.live.clone(), native.now)
        .unwrap_or_else(|| panic!("reset refused"));
    let b = native
        .owner
        .reset("a", native.live.clone(), native.now)
        .unwrap_or_else(|| panic!("reset refused"));
    assert!(!native.owner.restore(a, native.now));
    native.owner.dismiss_undo(a);
    assert!(native.owner.undo_for(b).is_some());
    native.owner.dismiss_undo(b);
    assert!(!native.owner.restore(b, native.now));
    let c = native
        .owner
        .reset("a", native.live.clone(), native.now)
        .unwrap_or_else(|| panic!("reset refused"));
    native.drag(900, 640);
    assert!(!native.owner.restore(c, native.now));
    assert_eq!(native.owner.windows["a"].width, 900);
}

#[test]
fn sizing_undo_of_a_default_preserves_the_absence_of_a_custom_size() {
    let mut native = Native::new();
    let revision = native
        .owner
        .reset("a", native.live.clone(), native.now)
        .unwrap_or_else(|| panic!("reset refused"));
    assert!(native.owner.restore(revision, native.now));
    assert!(!native.owner.windows.contains_key("a"));
}

#[test]
fn sizing_an_automatic_work_area_clamp_keeps_the_custom_size_for_a_later_larger_display() {
    let mut native = Native::new();
    native.drag(900, 640);
    native.live.display.room = (800.0, 600.0);
    native.event(Event::Geometry);
    native.settle();
    assert_eq!(native.maximum, (800.0, 600.0));
    // The platform enforces the new limits; this is not an explicit edge drag.
    native.live.frame = frame(800, 600);
    native.event(Event::Geometry);
    native.settle();
    assert_eq!(
        (native.owner.windows["a"].width, native.owner.windows["a"].height),
        (900, 640)
    );
    native.live.display.room = (1400.0, 900.0);
    native.event(Event::Geometry);
    native.settle();
    native.drag(1100, 700);
    assert_eq!(native.owner.windows["a"].width, 1100);
}

#[test]
fn sizing_opening_and_reopening_do_not_accept_delayed_programmatic_events_as_drags() {
    let mut native = Native::new();
    native.event(Event::Geometry);
    native.settle();
    assert_eq!(native.owner.revision, 0);
    native.drag(840, 600);
    let revision = native.owner.revision;
    native.owner.prepared(native.live.clone());
    native.event(Event::Geometry);
    native.settle();
    assert_eq!(native.owner.revision, revision);
    assert_eq!(native.owner.windows["a"].width, 840);
}

#[test]
fn sizing_drag_burst_writes_only_the_final_frame_after_500_ms_of_quiet() {
    let mut native = Native::new();
    for width in [760, 800, 840, 880] {
        native.drag(width, 600);
        assert!(native.owner.take_save(native.now).is_none());
        native.advance(100);
    }
    native.advance(399);
    assert!(native.owner.take_save(native.now).is_none());
    native.advance(1);
    let save = native
        .owner
        .take_save(native.now)
        .unwrap_or_else(|| panic!("missing synthetic final save"));
    assert_eq!(save.windows["a"].width, 880);
    assert!(native.owner.take_save(native.now).is_none());
}

#[test]
fn sizing_hide_and_exit_drain_the_latest_revision_without_waiting_for_debounce() {
    for stop in [false, true] {
        let mut native = Native::new();
        native.drag(840, 600);
        let revision = native.owner.flush(native.now, stop);
        let save = native
            .owner
            .take_save(native.now)
            .unwrap_or_else(|| panic!("closing must force the pending final save"));
        assert_eq!(save.revision, revision);
        assert!(!native.owner.drained(revision, stop));
        native.owner.complete(save.revision);
        native.owner.finished = stop;
        assert!(native.owner.drained(revision, stop));
        assert_eq!(native.owner.stopping, stop);
        assert_eq!(save.windows["a"].width, 840);
    }
}

#[test]
fn sizing_late_native_events_and_save_completions_cannot_consume_a_newer_drag() {
    let mut native = Native::new();
    native.drag(800, 580);
    native.advance(500);
    let old = native
        .owner
        .take_save(native.now)
        .unwrap_or_else(|| panic!("missing synthetic first save"));
    native.drag(900, 640);
    native.owner.complete(old.revision);
    native.event(Event::Geometry); // Delayed A reads B's live geometry.
    native.settle();
    native.advance(500);
    let current = native
        .owner
        .take_save(native.now)
        .unwrap_or_else(|| panic!("missing synthetic current save"));
    assert!(current.revision > old.revision);
    assert_eq!(current.windows["a"].width, 900);
}
