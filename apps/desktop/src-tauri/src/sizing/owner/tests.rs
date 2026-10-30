use super::*;

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
    native.settle(); // The OS work-area event need not contain a window resize.
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
fn sizing_an_automatic_work_area_clamp_keeps_the_custom_size_for_a_later_larger_display() {
    let mut native = Native::new();
    native.drag(900, 640);
    native.live.display.room = (800.0, 600.0);
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
