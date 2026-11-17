use super::*;

fn frame(width: u32, height: u32) -> Frame {
    Frame {
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
        usable: room,
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
            removed_display: None,
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
fn sizing_reset_undo_restores_the_visible_frame_and_only_the_removed_display_setting() {
    let mut native = Native::new();
    native.drag(840, 600);
    let previous = native.live.clone();
    let saved = native.owner.windows["a"];
    native
        .owner
        .windows
        .insert("b".into(), remember(frame(900, 640), (1000.0, 700.0)));
    let revision = native
        .owner
        .reset("a", previous.clone(), native.now)
        .unwrap_or_else(|| panic!("reset refused"));
    assert_eq!(native.owner.windows["a"].position, None);
    let undo = native
        .owner
        .undo_for(revision)
        .unwrap_or_else(|| panic!("reset has no Undo"));
    assert_eq!(undo.previous.frame, previous.frame);
    assert!(native.owner.restore(revision, native.now));
    assert_eq!(native.owner.windows["a"], saved);
    assert_eq!(native.owner.windows["b"], remember(frame(900, 640), (1000.0, 700.0)));
    assert!(!native.owner.restore(revision, native.now));
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
