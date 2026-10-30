use super::*;

fn synthetic_sample(display: String, frame: PickerWindow) -> Sample {
    Sample {
        display: Display {
            key: display,
            scale: 1.0,
            origin: (0.0, 0.0),
            room: (3000.0, 2000.0),
        },
        frame,
    }
}

impl Sizing {
    fn record(&self, display: String, live: PickerWindow) -> Option<Save> {
        let mut state = self.shared.state.lock().ok()?;
        let revision = state.revision;
        state.event(Event::Geometry, Instant::now());
        state.observe(synthetic_sample(display, live), Instant::now(), true);
        (state.revision > revision)
            .then(|| state.pending_save.clone())
            .flatten()
    }
    fn present(&self, display: &str, frame: PickerWindow) {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|error| panic!("{error}"))
            .prepared(synthetic_sample(display.into(), frame));
    }
    fn persist(&self, file: &SettingsFile, save: Save) -> Option<bool> {
        self.shared.persist(file, save)
    }
}

fn dragged(width: u32, height: u32) -> PickerWindow {
    PickerWindow {
        width,
        height,
        custom_size: true,
        x: 40,
        y: 60,
    }
}

#[test]
fn sizing_default_and_minimum_frames_do_not_depend_on_content() {
    let default = plan(None, false, (1400.0, 900.0));
    assert_eq!((default.width, default.height), (720, 480));
    let minimum = plan(Some(dragged(480, 360)), false, (1400.0, 900.0));
    assert_eq!((minimum.width, minimum.height), (480, 360));
}

#[test]
fn sizing_larger_text_multiplies_only_the_default_height_without_compounding() {
    let frame = plan(None, true, (1400.0, 900.0));
    assert_eq!((frame.width, frame.height), (720, 552));
    assert_eq!(plan(None, true, (1400.0, 500.0)).height, 500);
    assert_eq!(plan(Some(dragged(640, 400)), true, (1400.0, 900.0)), dragged(640, 400));
    let css = include_str!("../../../src/tokens.css");
    assert!(css.contains(&format!("--larger-text-scale: {LARGER_TEXT_SCALE}")));
    let sizing = Sizing::default();
    sizing.set_larger_text(true);
    assert_eq!(
        plan(None, sizing.larger_text.load(Ordering::SeqCst), (1400.0, 900.0)).height,
        552
    );
    sizing.set_larger_text(false);
    assert_eq!(
        plan(None, sizing.larger_text.load(Ordering::SeqCst), (1400.0, 900.0)).height,
        480
    );
}

#[test]
fn sizing_clamps_size_and_place_to_the_current_work_area_including_tiny_displays() {
    assert_eq!(
        plan(Some(dragged(2000, 1400)), false, (800.0, 600.0)),
        PickerWindow {
            width: 800,
            height: 600,
            custom_size: true,
            x: 0,
            y: 0,
        }
    );
    let frame = plan(
        Some(PickerWindow {
            width: 200,
            height: 80,
            custom_size: true,
            x: -90,
            y: 2000,
        }),
        false,
        (800.0, 600.0),
    );
    assert_eq!(
        frame,
        PickerWindow {
            width: 480,
            height: 360,
            custom_size: true,
            x: 0,
            y: 240
        }
    );
    assert_eq!(
        plan(None, true, (400.0, 300.0)),
        PickerWindow {
            width: 400,
            height: 300,
            custom_size: false,
            x: 0,
            y: 0
        }
    );
}

#[test]
fn sizing_first_show_includes_native_chrome_when_clamping_an_oversized_saved_frame() {
    let room = content_room((720.0, 480.0), (736.0, 519.0), (1000.0, 700.0))
        .unwrap_or_else(|| panic!("synthetic native extents rejected"));
    let shown = plan(Some(dragged(2000, 1400)), false, room);
    assert_eq!((shown.x, shown.y), (0, 0));
    assert_eq!((shown.width, shown.height), (984, 661));
    assert!(f64::from(shown.x) + f64::from(shown.width) + 16.0 <= 1000.0);
    assert!(f64::from(shown.y) + f64::from(shown.height) + 39.0 <= 700.0);
}

#[test]
fn sizing_a_delayed_event_reads_the_live_frame_and_cannot_restore_an_older_drag() {
    let sizing = Sizing::default();
    let a = sizing
        .record("display-a".into(), dragged(640, 400))
        .unwrap_or_else(|| panic!("missing synthetic frame"));
    let b = sizing
        .record("display-a".into(), dragged(840, 600))
        .unwrap_or_else(|| panic!("missing synthetic frame"));
    // A native event's old payload is deliberately never consumed. Even a late
    // callback reads B's live geometry, as changed() does.
    assert!(sizing.record("display-a".into(), dragged(840, 600)).is_none());
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let file = SettingsFile::new(directory.path().join("settings.json"));
    assert_eq!(sizing.persist(&file, b), Some(true));
    assert_eq!(sizing.persist(&file, a), None);
    assert_eq!(
        ibis_core::settings::load(&file.path).settings.picker_windows["display-a"],
        dragged(840, 600)
    );
}

#[test]
fn sizing_failed_save_keeps_the_visible_frame_for_reopen_and_a_later_drag_can_save() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let path = directory.path().join("settings.json");
    std::fs::create_dir(&path).unwrap_or_else(|error| panic!("{error}"));
    let file = SettingsFile::new(path.clone());
    let sizing = Sizing::default();
    let save = sizing
        .record("display-a".into(), dragged(640, 400))
        .unwrap_or_else(|| panic!("missing synthetic frame"));
    assert_eq!(sizing.persist(&file, save), Some(false));
    assert_eq!(sizing.remembered("display-a"), Some(dragged(640, 400)));
    std::fs::remove_dir(&path).unwrap_or_else(|error| panic!("{error}"));
    let save = sizing
        .record("display-a".into(), dragged(840, 600))
        .unwrap_or_else(|| panic!("missing synthetic frame"));
    assert_eq!(sizing.persist(&file, save), Some(true));
    assert_eq!(
        ibis_core::settings::load(&path).settings.picker_windows["display-a"],
        dragged(840, 600)
    );
}

#[test]
fn sizing_hide_reopen_relaunch_and_another_display_keep_independent_sizes_and_other_settings() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let file = SettingsFile::new(directory.path().join("settings.json"));
    file.update(|settings| settings.larger_text = Some(true))
        .unwrap_or_else(|error| panic!("{error}"));
    let sizing = Sizing::default();
    let a = sizing
        .record("display-a".into(), dragged(840, 600))
        .unwrap_or_else(|| panic!("missing synthetic frame"));
    let b = sizing
        .record("display-b".into(), dragged(480, 360))
        .unwrap_or_else(|| panic!("missing synthetic frame"));
    assert_eq!(sizing.persist(&file, a), None);
    assert_eq!(sizing.persist(&file, b), Some(true));
    let settings = ibis_core::settings::load(&file.path).settings;
    assert_eq!(settings.larger_text, Some(true));
    let restarted = Sizing::default();
    restarted.load(&settings);
    assert_eq!(
        plan(restarted.remembered("display-a"), false, (1000.0, 700.0)),
        dragged(840, 600)
    );
    assert_eq!(
        plan(restarted.remembered("display-b"), true, (800.0, 600.0)),
        dragged(480, 360)
    );
    assert_eq!(
        plan(restarted.remembered("display-c"), false, (800.0, 600.0)).height,
        480
    );
    assert_eq!(
        plan(restarted.remembered("display-a"), false, (700.0, 500.0)),
        PickerWindow {
            width: 700,
            height: 500,
            custom_size: true,
            x: 0,
            y: 0,
        }
    );
}

#[test]
fn sizing_reset_invalidates_pending_drag_and_restores_only_this_displays_default() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let file = SettingsFile::new(directory.path().join("settings.json"));
    let sizing = Sizing::default();
    let old = sizing
        .record("display-a".into(), dragged(840, 600))
        .unwrap_or_else(|| panic!("missing synthetic frame"));
    sizing.record("display-b".into(), dragged(640, 400));
    let reset = sizing
        .forget("display-a")
        .unwrap_or_else(|| panic!("missing synthetic frame"));
    assert_eq!(sizing.persist(&file, reset), Some(true));
    assert_eq!(sizing.persist(&file, old), None);
    assert_eq!(sizing.remembered("display-a"), None);
    assert_eq!(sizing.remembered("display-b"), Some(dragged(640, 400)));
    let settings = ibis_core::settings::load(&file.path).settings;
    assert!(!settings.picker_windows.contains_key("display-a"));
    assert_eq!(settings.picker_windows["display-b"], dragged(640, 400));
}

#[test]
fn sizing_moving_the_default_or_crossing_displays_does_not_replace_a_displays_size() {
    let sizing = Sizing::default();
    let default = plan(None, false, (1400.0, 900.0));
    sizing.present("display-a", default);
    let moved = PickerWindow {
        x: 40,
        y: 60,
        ..default
    };
    sizing
        .record("display-a".into(), moved)
        .unwrap_or_else(|| panic!("missing synthetic frame"));
    assert!(
        !sizing
            .remembered("display-a")
            .unwrap_or_else(|| panic!("missing synthetic frame"))
            .custom_size
    );
    assert_eq!(plan(sizing.remembered("display-a"), true, (1400.0, 900.0)).height, 552);
    // Merely moving to B keeps its own default. An actual edge drag on B
    // chooses its custom frame; returning to A still uses A's default.
    sizing
        .record("display-b".into(), moved)
        .unwrap_or_else(|| panic!("missing synthetic frame"));
    assert!(
        !sizing
            .remembered("display-b")
            .unwrap_or_else(|| panic!("missing synthetic frame"))
            .custom_size
    );
    sizing
        .record("display-b".into(), dragged(840, 600))
        .unwrap_or_else(|| panic!("missing synthetic frame"));
    assert_eq!(sizing.remembered("display-b"), Some(dragged(840, 600)));
    assert!(
        !sizing
            .remembered("display-a")
            .unwrap_or_else(|| panic!("missing synthetic frame"))
            .custom_size
    );
    // Reopen A, then move its default frame to B without dragging an edge:
    // B retains the size chosen there on the previous opening.
    sizing.present("display-a", default);
    sizing.record("display-b".into(), moved);
    assert_eq!(sizing.remembered("display-b"), Some(dragged(840, 600)));
}

#[test]
fn sizing_a_save_waiting_for_the_settings_owner_is_obsolete_before_it_can_write() {
    use std::sync::mpsc;
    use std::time::Duration;

    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let file = SettingsFile::new(directory.path().join("settings.json"));
    let sizing = Sizing::default();
    let old = sizing
        .record("display-a".into(), dragged(640, 400))
        .unwrap_or_else(|| panic!("missing synthetic frame"));
    let (entered, locked) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    std::thread::scope(|scope| {
        let file_ref = &file;
        scope.spawn(move || {
            file_ref
                .update(|settings| {
                    entered.send(()).unwrap_or_else(|error| panic!("{error}"));
                    blocked
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap_or_else(|error| panic!("{error}"));
                    settings.larger_text = Some(true);
                })
                .unwrap_or_else(|error| panic!("{error}"))
        });
        locked
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|error| panic!("{error}"));
        let sizing_ref = &sizing;
        let pending = scope.spawn(move || sizing_ref.persist(file_ref, old));
        let current = sizing
            .record("display-a".into(), dragged(840, 600))
            .unwrap_or_else(|| panic!("missing synthetic frame"));
        release.send(()).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(sizing.persist(&file, current), Some(true));
        assert_eq!(
            pending.join().unwrap_or_else(|_| panic!("synthetic writer panicked")),
            None
        );
    });
    let saved = ibis_core::settings::load(&file.path).settings;
    assert_eq!(saved.picker_windows["display-a"], dragged(840, 600));
    assert_eq!(saved.larger_text, Some(true));
}

#[test]
fn sizing_appkit_keeps_the_top_edge_and_handles_displays_above_and_left() {
    let native = placement::Area {
        x: 504.0,
        y: 543.0,
        width: 720.0,
        height: 480.0,
    };
    // The current frame supplies the coordinate transform, independent of
    // either display's pixel scale or the requested new content height.
    assert_eq!(appkit_top_left((504.0, 94.0), native, (504.0, 94.0)), (504.0, 1023.0));
    assert_eq!(
        appkit_top_left((504.0, 94.0), native, (-2080.0, -1239.0)),
        (-2080.0, 2356.0)
    );
}

#[test]
fn sizing_owned_worker_flushes_on_hide_then_drains_the_final_frame_before_exit() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let file = SettingsFile::new(directory.path().join("settings.json"));
    file.update(|settings| settings.larger_text = Some(true))
        .unwrap_or_else(|error| panic!("{error}"));
    let sizing = Sizing::default();
    sizing.record("display-a".into(), dragged(840, 600));
    let shared = sizing.shared.clone();
    let worker_file = file.clone();
    let (written, received) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        shared.run(&worker_file, |revision, saved| {
            written
                .send((revision, saved))
                .unwrap_or_else(|error| panic!("{error}"));
        })
    });
    assert_eq!(sizing.shared.request_flush(false), Some(1));
    assert_eq!(
        received
            .recv_timeout(FLUSH_TIMEOUT)
            .unwrap_or_else(|error| panic!("{error}")),
        (1, true)
    );
    assert_eq!(
        ibis_core::settings::load(&file.path).settings.picker_windows["display-a"],
        dragged(840, 600)
    );
    sizing.record("display-a".into(), dragged(900, 640));
    assert!(sizing.shared.flush(FLUSH_TIMEOUT, true));
    assert!(
        sizing.record("display-a".into(), dragged(1000, 700)).is_none(),
        "an exit cannot accept a late native event"
    );
    worker
        .join()
        .unwrap_or_else(|_| panic!("synthetic geometry worker panicked"));
    let saved = ibis_core::settings::load(&file.path).settings;
    assert_eq!(saved.picker_windows["display-a"], dragged(900, 640));
    assert_eq!(saved.larger_text, Some(true));
    let relaunched = Sizing::default();
    relaunched.load(&saved);
    assert_eq!(relaunched.remembered("display-a"), Some(dragged(900, 640)));
}

#[test]
fn sizing_failed_final_io_reports_failure_and_still_finishes_the_owned_worker() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let path = directory.path().join("settings.json");
    std::fs::create_dir(&path).unwrap_or_else(|error| panic!("{error}"));
    let file = SettingsFile::new(path);
    let sizing = Sizing::default();
    sizing.record("display-a".into(), dragged(840, 600));
    let shared = sizing.shared.clone();
    let (sent, received) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        shared.run(&file, |revision, saved| {
            sent.send((revision, saved)).unwrap_or_else(|error| panic!("{error}"));
        })
    });
    assert!(
        sizing.shared.flush(FLUSH_TIMEOUT, true),
        "failed I/O is completed I/O, never an endless drain"
    );
    assert_eq!(
        received
            .recv_timeout(FLUSH_TIMEOUT)
            .unwrap_or_else(|error| panic!("{error}")),
        (1, false)
    );
    worker
        .join()
        .unwrap_or_else(|_| panic!("synthetic geometry worker panicked"));
    assert_eq!(sizing.remembered("display-a"), Some(dragged(840, 600)));
}

#[test]
fn sizing_exit_drain_is_bounded_even_when_another_settings_writer_is_blocked() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let file = SettingsFile::new(directory.path().join("settings.json"));
    let (entered, ready) = std::sync::mpsc::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let blocked_file = file.clone();
    let writer = std::thread::spawn(move || {
        blocked_file
            .update(|settings| {
                entered.send(()).unwrap_or_else(|error| panic!("{error}"));
                blocked
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap_or_else(|error| panic!("{error}"));
                settings.larger_text = Some(true);
            })
            .unwrap_or_else(|error| panic!("{error}"));
    });
    ready
        .recv_timeout(FLUSH_TIMEOUT)
        .unwrap_or_else(|error| panic!("{error}"));
    let sizing = Sizing::default();
    sizing.record("display-a".into(), dragged(840, 600));
    let shared = sizing.shared.clone();
    let worker_file = file.clone();
    let worker = std::thread::spawn(move || shared.run(&worker_file, |_, _| {}));
    let drained = sizing.shared.flush(Duration::from_millis(20), true);
    // Always release before asserting, so a failing regression cannot strand
    // the synthetic writer or worker during the rest of the test process.
    release.send(()).unwrap_or_else(|error| panic!("{error}"));
    assert!(!drained);
    assert!(sizing.shared.flush(FLUSH_TIMEOUT, true));
    writer
        .join()
        .unwrap_or_else(|_| panic!("synthetic settings writer panicked"));
    worker
        .join()
        .unwrap_or_else(|_| panic!("synthetic geometry worker panicked"));
    let saved = ibis_core::settings::load(&file.path).settings;
    assert_eq!(saved.picker_windows["display-a"], dragged(840, 600));
    assert_eq!(saved.larger_text, Some(true));
}
