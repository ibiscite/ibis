use super::*;

#[test]
fn remembered_fraction_survives_rearranging_and_shrinking_a_display_and_reset() {
    let mut owner = Owner::default();
    let now = Instant::now();
    let mut screen = synthetic_sample("macos:invented-display-uuid".into(), dragged(720, 480)).display;
    screen.room = (1600.0, 1000.0);
    screen.usable = screen.room;
    screen.origin = (-1600.0, 0.0);
    let moved = Frame {
        x: 800,
        y: 400,
        ..dragged(720, 480)
    };
    owner.prepared(Sample {
        display: screen.clone(),
        removed_display: None,
        frame: Frame {
            x: 100,
            y: 100,
            ..moved
        },
    });
    owner.event(Event::Geometry, now);
    owner.observe(
        Sample {
            display: screen.clone(),
            frame: moved,
            removed_display: None,
        },
        now,
        true,
    );
    let saved = owner.windows[&screen.key];
    assert_eq!(saved.position, Some((0.5, 0.4)));
    // Same identity after moving the screen above/right, with a smaller work area.
    screen.origin = (2000.0, -800.0);
    screen.room = (1000.0, 700.0);
    screen.usable = screen.room;
    let reopened = plan(Some(restore(saved, screen.room)), false, screen.room);
    assert_eq!((reopened.x, reopened.y), (280, 220));
    owner.reset(
        &screen.key,
        Sample {
            display: screen.clone(),
            frame: reopened,
            removed_display: None,
        },
        now,
    );
    let reset = owner
        .windows
        .get(&screen.key)
        .copied()
        .map(|saved| restore(saved, screen.room));
    let default = plan(reset, false, screen.room);
    assert_eq!((default.width, default.height), (720, 480));
    assert_eq!((default.x, default.y), (140, 148));
    assert_eq!(owner.windows[&screen.key].position, None);
}

fn synthetic_sample(display: String, frame: Frame) -> Sample {
    Sample {
        display: Display {
            key: display,
            scale: 1.0,
            origin: (0.0, 0.0),
            room: (3000.0, 2000.0),
            usable: (3000.0, 2000.0),
        },
        frame,
        removed_display: None,
    }
}

impl Sizing {
    fn record(&self, display: String, live: Frame) -> Option<Save> {
        let mut state = self.shared.state.lock().ok()?;
        let revision = state.revision;
        state.event(Event::Geometry, Instant::now());
        state.observe(synthetic_sample(display, live), Instant::now(), true);
        (state.revision > revision)
            .then(|| state.pending_save.clone())
            .flatten()
    }
    fn persist(&self, file: &SettingsFile, save: Save) -> Option<bool> {
        self.shared.persist(file, save)
    }
}

fn dragged(width: u32, height: u32) -> Frame {
    Frame {
        width,
        height,
        custom_size: true,
        x: 40,
        y: 60,
    }
}

#[test]
fn window_size_names_presets_and_calls_anything_else_custom() {
    assert_eq!(WindowSize::of(None), WindowSize::Medium);
    assert_eq!(WindowSize::of(Some(dragged(640, 400))), WindowSize::Small);
    assert_eq!(WindowSize::of(Some(dragged(720, 480))), WindowSize::Medium);
    assert_eq!(WindowSize::of(Some(dragged(840, 600))), WindowSize::Large);
    assert_eq!(WindowSize::of(Some(dragged(700, 500))), WindowSize::Custom);
    let moved = Frame {
        custom_size: false,
        ..dragged(900, 700)
    };
    assert_eq!(
        WindowSize::of(Some(moved)),
        WindowSize::Medium,
        "moving is not resizing"
    );
}

#[test]
fn chosen_size_keeps_the_window_centred_and_fits_a_small_display() {
    let room = (1400.0, 900.0);
    let saved = Frame {
        x: 340,
        y: 100,
        ..dragged(720, 480)
    };
    let large = chosen(Some(saved), (840, 600), false, room);
    assert_eq!((large.width, large.height, large.custom_size), (840, 600, true));
    assert_eq!((large.x + 420, large.y), (340 + 360, 100));
    let fitted = plan(Some(large), false, (800.0, 500.0));
    assert_eq!((fitted.width, fitted.height), (800, 500));
    assert_eq!(
        WindowSize::of(Some(large)),
        WindowSize::Large,
        "the choice survives a small display"
    );
    let medium = chosen(Some(large), (720, 480), true, room);
    assert!(!medium.custom_size, "Medium is the default again");
    assert_eq!(plan(Some(medium), true, room).height, 552, "larger text applies to it");
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
    assert_eq!(
        sizing.remembered("display-a", (3000.0, 2000.0)),
        Some(dragged(640, 400))
    );
    std::fs::remove_dir(&path).unwrap_or_else(|error| panic!("{error}"));
    let save = sizing
        .record("display-a".into(), dragged(840, 600))
        .unwrap_or_else(|| panic!("missing synthetic frame"));
    assert_eq!(sizing.persist(&file, save), Some(true));
    assert_eq!(
        crate::settings_fixture::read(&path).device.picker_windows["display-a"],
        remember(dragged(840, 600), (3000.0, 2000.0))
    );
}

#[test]
fn sizing_hide_reopen_relaunch_and_another_display_keep_independent_sizes_and_other_settings() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let file = SettingsFile::new(directory.path().join("settings.json"));
    file.update(|settings| settings.portable.larger_text = Some(true))
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
    let settings = crate::settings_fixture::read(&file.path);
    assert_eq!(settings.portable.larger_text, Some(true));
    let restarted = Sizing::default();
    restarted.load(&(&settings).into());
    assert_eq!(
        plan(
            restarted.remembered("display-a", (3000.0, 2000.0)),
            false,
            (1000.0, 700.0)
        ),
        dragged(840, 600)
    );
    assert_eq!(
        plan(
            restarted.remembered("display-b", (3000.0, 2000.0)),
            true,
            (800.0, 600.0)
        ),
        dragged(480, 360)
    );
    assert_eq!(
        plan(
            restarted.remembered("display-c", (3000.0, 2000.0)),
            false,
            (800.0, 600.0)
        )
        .height,
        480
    );
    assert_eq!(
        plan(
            restarted.remembered("display-a", (3000.0, 2000.0)),
            false,
            (700.0, 500.0)
        ),
        Frame {
            width: 700,
            height: 500,
            custom_size: true,
            x: 0,
            y: 0,
        }
    );
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
