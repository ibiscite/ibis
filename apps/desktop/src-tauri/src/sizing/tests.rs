use super::*;
use ibis_engine::settings::Purpose;

#[test]
fn size_choices_and_bottom_right_resizes_leave_default_placement_automatic() {
    let now = Instant::now();
    for preset in [false, true] {
        let mut owner = Owner::default();
        let initial = synthetic_sample("macos:invented-display".into(), plan(None, false, (3000.0, 2000.0)));
        let mut resized = initial.clone();
        resized.frame.width = 840;
        resized.frame.height = 600;
        owner.prepared(initial);
        if preset {
            owner.choose(&resized.display.key, resized.frame, resized.display.usable, now);
        } else {
            owner.event(Event::Geometry, now);
            owner.observe(resized.clone(), now, true);
        }
        let saved = owner.windows[&resized.display.key];
        assert_eq!(saved.position, None, "size alone never pins the default position");
        let reopened = plan(Some(restore(saved, (1600.0, 1200.0))), false, (1600.0, 1200.0));
        assert_eq!((reopened.x, reopened.y), (380, 273));
    }
}

#[test]
fn automatic_topology_clamp_preserves_the_original_fraction_and_unknown_ids_are_ephemeral() {
    let now = Instant::now();
    let mut owner = Owner::default();
    let first = synthetic_sample("macos:invented-display".into(), dragged(840, 600));
    owner.observe(first.clone(), now, true);
    let original = owner.windows[&first.display.key];
    let mut shrunk = first;
    shrunk.display.room = (900.0, 650.0);
    shrunk.display.usable = shrunk.display.room;
    shrunk.display.origin = (2000.0, -800.0);
    shrunk.frame.x = 60;
    shrunk.frame.y = 50;
    owner.event(Event::Geometry, now);
    owner.observe(shrunk.clone(), now, true);
    assert_eq!(owner.windows[&shrunk.display.key], original);
    shrunk.display.key.clear();
    owner.event(Event::Geometry, now);
    owner.observe(shrunk, now, true);
    assert!(!owner.windows.contains_key(""));
}

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

#[test]
fn sizing_hidden_choice_without_display_identity_survives_reopen_only_in_session_regression() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let file = SettingsFile::new(directory.path().join("settings.json"));
    let sizing = Sizing::default();
    let room = (3000.0, 2000.0);
    let large = chosen(None, (840, 600), false, room);
    sizing
        .shared
        .state
        .lock()
        .unwrap_or_else(|error| panic!("{error}"))
        .choose("", large, room, Instant::now());
    assert_eq!(WindowSize::of(sizing.remembered("", room)), WindowSize::Large);
    let reopened = plan(sizing.remembered("", room), false, room);
    assert_eq!((reopened.width, reopened.height), (840, 600));
    sizing.present("", reopened);
    sizing
        .shared
        .state
        .lock()
        .unwrap_or_else(|error| panic!("{error}"))
        .flush(Instant::now(), false);
    assert_eq!(WindowSize::of(sizing.remembered("", room)), WindowSize::Large);

    // Saving a different, identified display cannot leak the session choice.
    let save = sizing
        .record("macos:invented-display".into(), dragged(640, 400))
        .unwrap_or_else(|| panic!("missing identified display save"));
    assert_eq!(save.windows.len(), 1);
    assert!(!save.windows.contains_key(""));
    assert_eq!(sizing.persist(&file, save), Some(true));
    let settings = ibis_engine::settings::load(&file.path)
        .usable(Purpose::Interface)
        .unwrap_or_else(|error| panic!("{error}"))
        .clone();
    assert!(!settings.device.picker_windows.contains_key(""));
    let relaunched = Sizing::default();
    relaunched.load(&settings);
    assert_eq!(WindowSize::of(relaunched.remembered("", room)), WindowSize::Medium);
    assert_eq!(WindowSize::of(sizing.remembered("", room)), WindowSize::Large);
}

#[test]
fn sizing_unplug_relocation_preserves_destination_placement_regression() {
    let now = Instant::now();
    let room = (3000.0, 2000.0);
    let remembered = PickerWindow {
        width: 720,
        height: 480,
        custom_size: false,
        position: Some((0.6, 0.5)),
    };
    for (connected, intermediate, destination) in [
        (false, false, None),
        (false, false, Some(remembered)),
        (false, true, None),
        (false, true, Some(remembered)),
        (true, false, Some(remembered)),
    ] {
        let mut owner = Owner::default();
        if let Some(saved) = destination {
            owner.windows.insert("a".into(), saved);
        }
        let mut before = synthetic_sample("b".into(), plan(None, false, room));
        before.display.origin = (3000.0, 0.0);
        owner.prepared(before.clone());
        let observe = |owner: &mut Owner, mut sample: Sample| {
            sample.removed_display = removed_display(
                owner.last.as_ref().map(|last| last.display.key.as_str()),
                &sample.display.key,
                || {
                    Some(if connected {
                        vec!["a".into(), "b".into()]
                    } else {
                        vec!["a".into()]
                    })
                },
            );
            owner.event(Event::Geometry, now);
            owner.observe(sample, now, true);
        };
        if intermediate {
            // X11 can report B's removal before the window manager moves the
            // picker: current_monitor is A, but the frame is still off-screen.
            let mut offscreen = synthetic_sample("a".into(), before.frame);
            offscreen.frame.x += 3000;
            observe(&mut owner, offscreen);
        }
        let mut relocated = synthetic_sample("a".into(), before.frame);
        relocated.frame.x = 240;
        relocated.frame.y = 180;
        let expected = if connected {
            // Crossing between connected displays is still a deliberate move.
            relocated.frame
        } else {
            plan(destination.map(|saved| restore(saved, room)), false, room)
        };
        observe(&mut owner, relocated);
        let saved = owner.windows.get("a").copied();
        assert_eq!(
            plan(saved.map(|saved| restore(saved, room)), false, room),
            expected,
            "only unplug relocation preserves A's remembered spot or automatic default"
        );
        assert_eq!(
            owner.pending_save.is_some(),
            connected,
            "only a deliberate crossing should schedule a save"
        );
    }
}

impl Sizing {
    fn forget(&self, display: &str) -> Option<Save> {
        self.shared.state.lock().ok()?.forget(display, Instant::now())
    }
    fn record(&self, display: String, live: Frame) -> Option<Save> {
        let mut state = self.shared.state.lock().ok()?;
        let revision = state.revision;
        state.event(Event::Geometry, Instant::now());
        state.observe(synthetic_sample(display, live), Instant::now(), true);
        (state.revision > revision)
            .then(|| state.pending_save.clone())
            .flatten()
    }
    fn present(&self, display: &str, frame: Frame) {
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
fn chosen_size_is_saved_once_and_a_late_write_cannot_replace_it() {
    let mut owner = Owner::default();
    let now = Instant::now();
    let frame = dragged(840, 600);
    owner.choose("synthetic", frame, (3000.0, 2000.0), now);
    owner.choose("synthetic", frame, (3000.0, 2000.0), now);
    assert_eq!(owner.revision, 1, "choosing the same size again saves nothing");
    let first = owner.pending_save.clone().map(|save| save.revision);
    owner.choose("synthetic", dragged(640, 400), (3000.0, 2000.0), now);
    assert_ne!(owner.pending_save.as_ref().map(|save| save.revision), first);
    assert_eq!(
        owner.windows.get("synthetic"),
        Some(&PickerWindow {
            position: None,
            ..remember(dragged(640, 400), (3000.0, 2000.0))
        })
    );
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
fn sizing_takes_the_text_size_at_dispatch_and_keeps_it_if_its_save_fails() {
    use crate::preferences::{Accent, PickerPreferencesPatch, picker_preferences_saved, save_picker_preferences};

    for failed_newer_flush in [false, true] {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("settings.json");
        let app = tauri::test::mock_builder()
            .manage(SettingsFile::new(path.clone()))
            .manage(Sizing::default())
            .build(tauri::generate_context!(test = true))
            .unwrap_or_else(|error| panic!("{error}"));
        let window = tauri::WebviewWindowBuilder::new(&app, "settings", Default::default())
            .build()
            .unwrap_or_else(|error| panic!("{error}"));
        let save = |patch| {
            let receipt = save_picker_preferences(
                patch,
                app.state::<SettingsFile>(),
                app.handle().clone(),
                window.as_ref().window(),
            )
            .unwrap_or_else(|error| panic!("{error}"));
            tauri::async_runtime::block_on(picker_preferences_saved(receipt, app.state::<SettingsFile>()))
        };
        save(PickerPreferencesPatch {
            larger_text: Some(false),
            accent: Some(Accent::Ibis),
            ..Default::default()
        })
        .unwrap_or_else(|error| panic!("{error}"));
        let read_only = |on: bool| {
            let mut permissions = std::fs::metadata(&path)
                .unwrap_or_else(|error| panic!("{error}"))
                .permissions();
            permissions.set_readonly(on);
            std::fs::set_permissions(&path, permissions).unwrap_or_else(|error| panic!("{error}"));
        };
        if failed_newer_flush {
            read_only(true);
        }
        let newer = save(PickerPreferencesPatch {
            larger_text: Some(true),
            ..Default::default()
        });
        assert_eq!(newer.is_err(), failed_newer_flush);
        let sizing = app.state::<Sizing>();
        let height = || plan(None, sizing.larger_text.load(Ordering::SeqCst), (1400.0, 900.0)).height;
        assert_eq!(height(), 552, "the chosen text size applies even if its save fails");
        if failed_newer_flush {
            read_only(false);
        }
        let geometry = sizing
            .record("display-a".into(), dragged(840, 600))
            .unwrap_or_else(|| panic!("missing synthetic frame"));
        assert_eq!(sizing.persist(&app.state::<SettingsFile>(), geometry), Some(true));
        let saved = ibis_engine::settings::load(&path)
            .usable(Purpose::Interface)
            .unwrap_or_else(|error| panic!("{error}"))
            .clone();
        assert_eq!(
            saved.device.picker_windows["display-a"],
            remember(dragged(840, 600), (3000.0, 2000.0))
        );
        assert_eq!(
            saved.unknown["accent"], "ibis",
            "the earlier choice's other field is saved"
        );
        assert_eq!(saved.portable.larger_text.unwrap_or(false), !failed_newer_flush);
        let restarted = Sizing::default();
        restarted.load(&saved);
        assert_eq!(
            restarted.remembered("display-a", (3000.0, 2000.0)),
            Some(dragged(840, 600))
        );
        assert_eq!(
            plan(None, restarted.larger_text.load(Ordering::SeqCst), (1400.0, 900.0)).height,
            if failed_newer_flush { 480 } else { 552 }
        );
    }
}

#[test]
fn sizing_clamps_size_and_place_to_the_current_work_area_including_tiny_displays() {
    assert_eq!(
        plan(Some(dragged(2000, 1400)), false, (800.0, 600.0)),
        Frame {
            width: 800,
            height: 600,
            custom_size: true,
            x: 0,
            y: 0,
        }
    );
    let frame = plan(
        Some(Frame {
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
        Frame {
            width: 480,
            height: 360,
            custom_size: true,
            x: 0,
            y: 240
        }
    );
    assert_eq!(
        plan(None, true, (400.0, 300.0)),
        Frame {
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
fn sizing_failed_undo_save_keeps_the_restored_frame_and_rejects_the_old_reset_write() {
    let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let path = directory.path().join("settings.json");
    std::fs::create_dir(&path).unwrap_or_else(|error| panic!("{error}"));
    let file = SettingsFile::new(path);
    let sizing = Sizing::default();
    sizing.record("display-a".into(), dragged(840, 600));
    let (reset, restored) = {
        let mut state = sizing.shared.state.lock().unwrap_or_else(|error| panic!("{error}"));
        let previous = state.last.clone().unwrap_or_else(|| panic!("missing synthetic frame"));
        let revision = state
            .reset("display-a", previous, Instant::now())
            .unwrap_or_else(|| panic!("reset refused"));
        let reset = state
            .pending_save
            .clone()
            .unwrap_or_else(|| panic!("missing reset save"));
        assert!(state.restore(revision, Instant::now()));
        (
            reset,
            state
                .pending_save
                .clone()
                .unwrap_or_else(|| panic!("missing Undo save")),
        )
    };
    assert_eq!(sizing.persist(&file, reset), None);
    assert_eq!(sizing.persist(&file, restored), Some(false));
    assert_eq!(
        sizing.remembered("display-a", (3000.0, 2000.0)),
        Some(dragged(840, 600))
    );
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
        ibis_engine::settings::load(&file.path)
            .usable(Purpose::Interface)
            .unwrap_or_else(|error| panic!("{error}"))
            .clone()
            .device
            .picker_windows["display-a"],
        remember(dragged(840, 600), (3000.0, 2000.0))
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
        ibis_engine::settings::load(&path)
            .usable(Purpose::Interface)
            .unwrap_or_else(|error| panic!("{error}"))
            .clone()
            .device
            .picker_windows["display-a"],
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
    let settings = ibis_engine::settings::load(&file.path)
        .usable(Purpose::Interface)
        .unwrap_or_else(|error| panic!("{error}"))
        .clone();
    assert_eq!(settings.portable.larger_text, Some(true));
    let restarted = Sizing::default();
    restarted.load(&settings);
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
fn sizing_reset_invalidates_pending_drag_and_clears_only_this_displays_spot() {
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
    assert_eq!(
        sizing
            .shared
            .state
            .lock()
            .ok()
            .and_then(|state| state.windows.get("display-a").copied())
            .and_then(|frame| frame.position),
        None
    );
    assert_eq!(
        sizing.remembered("display-b", (3000.0, 2000.0)),
        Some(dragged(640, 400))
    );
    let settings = ibis_engine::settings::load(&file.path)
        .usable(Purpose::Interface)
        .unwrap_or_else(|error| panic!("{error}"))
        .clone();
    assert_eq!(settings.device.picker_windows["display-a"].position, None);
    let reopened = plan(
        Some(restore(settings.device.picker_windows["display-a"], (3000.0, 2000.0))),
        false,
        (3000.0, 2000.0),
    );
    assert_eq!(
        (reopened.width, reopened.height),
        (840, 600),
        "Reset clears the spot and keeps the chosen size"
    );
    assert_eq!(
        settings.device.picker_windows["display-b"],
        remember(dragged(640, 400), (3000.0, 2000.0))
    );
}

#[test]
fn sizing_moving_the_default_or_crossing_displays_does_not_replace_a_displays_size() {
    let sizing = Sizing::default();
    let default = plan(None, false, (1400.0, 900.0));
    sizing.present("display-a", default);
    let moved = Frame {
        x: 40,
        y: 60,
        ..default
    };
    sizing
        .record("display-a".into(), moved)
        .unwrap_or_else(|| panic!("missing synthetic frame"));
    assert!(
        !sizing
            .remembered("display-a", (3000.0, 2000.0))
            .unwrap_or_else(|| panic!("missing synthetic frame"))
            .custom_size
    );
    assert_eq!(
        plan(sizing.remembered("display-a", (3000.0, 2000.0)), true, (1400.0, 900.0)).height,
        552
    );
    // Merely moving to B keeps its own default. An actual edge drag on B
    // chooses its custom frame; returning to A still uses A's default.
    sizing
        .record("display-b".into(), moved)
        .unwrap_or_else(|| panic!("missing synthetic frame"));
    assert!(
        !sizing
            .remembered("display-b", (3000.0, 2000.0))
            .unwrap_or_else(|| panic!("missing synthetic frame"))
            .custom_size
    );
    sizing
        .record("display-b".into(), dragged(840, 600))
        .unwrap_or_else(|| panic!("missing synthetic frame"));
    assert_eq!(
        sizing.remembered("display-b", (3000.0, 2000.0)),
        Some(dragged(840, 600))
    );
    assert!(
        !sizing
            .remembered("display-a", (3000.0, 2000.0))
            .unwrap_or_else(|| panic!("missing synthetic frame"))
            .custom_size
    );
    // Reopen A, then move its default frame to B without dragging an edge:
    // B retains the size chosen there on the previous opening.
    sizing.present("display-a", default);
    sizing.record("display-b".into(), moved);
    assert_eq!(
        sizing.remembered("display-b", (3000.0, 2000.0)),
        Some(dragged(840, 600))
    );
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
                    settings.portable.larger_text = Some(true);
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
    let saved = ibis_engine::settings::load(&file.path)
        .usable(Purpose::Interface)
        .unwrap_or_else(|error| panic!("{error}"))
        .clone();
    assert_eq!(
        saved.device.picker_windows["display-a"],
        remember(dragged(840, 600), (3000.0, 2000.0))
    );
    assert_eq!(saved.portable.larger_text, Some(true));
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
    file.update(|settings| settings.portable.larger_text = Some(true))
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
        ibis_engine::settings::load(&file.path)
            .usable(Purpose::Interface)
            .unwrap_or_else(|error| panic!("{error}"))
            .clone()
            .device
            .picker_windows["display-a"],
        remember(dragged(840, 600), (3000.0, 2000.0))
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
    let saved = ibis_engine::settings::load(&file.path)
        .usable(Purpose::Interface)
        .unwrap_or_else(|error| panic!("{error}"))
        .clone();
    assert_eq!(
        saved.device.picker_windows["display-a"],
        remember(dragged(900, 640), (3000.0, 2000.0))
    );
    assert_eq!(saved.portable.larger_text, Some(true));
    let relaunched = Sizing::default();
    relaunched.load(&saved);
    assert_eq!(
        relaunched.remembered("display-a", (3000.0, 2000.0)),
        Some(dragged(900, 640))
    );
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
    assert_eq!(
        sizing.remembered("display-a", (3000.0, 2000.0)),
        Some(dragged(840, 600))
    );
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
                settings.portable.larger_text = Some(true);
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
    let saved = ibis_engine::settings::load(&file.path)
        .usable(Purpose::Interface)
        .unwrap_or_else(|error| panic!("{error}"))
        .clone();
    assert_eq!(
        saved.device.picker_windows["display-a"],
        remember(dragged(840, 600), (3000.0, 2000.0))
    );
    assert_eq!(saved.portable.larger_text, Some(true));
}
