use super::*;

const fn rect(x: f64, y: f64, width: f64, height: f64) -> Area {
    Area { x, y, width, height }
}

#[test]
fn search_anchor_excludes_menu_notch_and_dock_and_ignores_frame_height() {
    let usable = rect(0.0, 38.0, 1728.0, 1015.0);
    for height in [400.0, 480.0, 600.0] {
        let (x, y) = launcher_position(usable, 720.0, height, 26.5);
        assert_eq!(x, 504.0);
        assert_eq!(y + 26.5, 38.0 + 1015.0 / 4.0);
        assert!(usable.holds(&rect(x, y, 720.0, height)));
    }
    // visibleFrame and safe-area insets are intersected, never added twice.
    assert_eq!(
        rect(0.0, 25.0, 1728.0, 1028.0).intersection(rect(0.0, 38.0, 1728.0, 1079.0)),
        usable
    );
}

#[test]
fn taskbars_on_every_edge_at_150_percent_dpi_leave_the_whole_picker_visible() {
    let scale = 1.5;
    for work in [
        rect(0.0, 60.0, 1920.0, 1020.0),
        rect(0.0, 0.0, 1920.0, 1020.0),
        rect(60.0, 0.0, 1860.0, 1080.0),
        rect(0.0, 0.0, 1860.0, 1080.0),
    ] {
        let (width, height, search) = (720.0 * scale, 480.0 * scale, 26.5 * scale);
        let (x, y) = launcher_position(work, width, height, search);
        assert_eq!(x + width / 2.0, work.x + work.width / 2.0);
        assert_eq!(y + search, work.y + work.height / 4.0);
        assert!(work.holds(&rect(x, y, width, height)));
    }
}

#[test]
fn focused_window_on_second_mixed_scale_screen_beats_pointer_and_old_picker() {
    // Shared macOS points (2x laptop, 1x external), or Windows physical pixels.
    let screens = [
        Display {
            bounds: area(0, 0, 3456, 2234, 2.0),
            usable: area(0, 76, 3456, 2030, 2.0),
        },
        Display {
            bounds: rect(-2560.0, -200.0, 2560.0, 1440.0),
            usable: rect(-2560.0, -175.0, 2560.0, 1360.0),
        },
    ];
    let focused = rect(-2200.0, 100.0, 1400.0, 900.0);
    let picker = rect(500.0, 200.0, 720.0, 480.0);
    assert_eq!(
        choose(&screens, Some(focused), Some((100.0, 100.0)), Some(picker), Some(0)),
        Placement::Centre(1)
    );
    // Unknown focus uses pointer; unknown pointer uses current screen/primary.
    assert_eq!(
        choose(&screens, None, Some((100.0, 100.0)), Some(picker), Some(1)),
        Placement::Centre(0)
    );
    assert_eq!(
        choose(&screens, None, None, Some(picker), Some(1)),
        Placement::Centre(0)
    );
    assert_eq!(choose(&screens, None, None, None, Some(1)), Placement::Centre(1));
    assert_eq!(choose(&[], None, None, None, None), Placement::Keep);
}

#[test]
fn focused_window_spanning_screens_uses_its_largest_overlap() {
    let screens = [
        Display {
            bounds: rect(0.0, 0.0, 1000.0, 900.0),
            usable: rect(0.0, 0.0, 1000.0, 900.0),
        },
        Display {
            bounds: rect(1000.0, 0.0, 1000.0, 900.0),
            usable: rect(1000.0, 0.0, 1000.0, 900.0),
        },
    ];
    assert_eq!(
        choose(&screens, Some(rect(800.0, 100.0, 900.0, 600.0)), None, None, None),
        Placement::Centre(1)
    );
}

#[test]
fn small_work_area_clamps_default_anchor_and_oversized_window() {
    assert_eq!(
        launcher_position(rect(10.0, 20.0, 800.0, 500.0), 720.0, 480.0, 26.5),
        (50.0, 40.0)
    );
    assert_eq!(
        launcher_position(rect(10.0, 20.0, 300.0, 200.0), 720.0, 480.0, 26.5),
        (10.0, 20.0)
    );
}

#[test]
fn explicit_debug_previews_keep_their_position_but_release_recomputes_the_default() {
    let screens = [Display {
        bounds: rect(0.0, 0.0, 1400.0, 1000.0),
        usable: rect(0.0, 25.0, 1400.0, 900.0),
    }];
    let preview = WindowConfig {
        x: Some(200.0),
        y: Some(100.0),
        ..WindowConfig::default()
    };
    for debug in [true, false] {
        assert_eq!(
            choose_for_show(
                &screens,
                None,
                None,
                || Some(rect(200.0, 100.0, 720.0, 480.0)),
                Some(0),
                Some(&preview),
                debug
            ),
            if debug { Placement::Keep } else { Placement::Centre(0) }
        );
    }
}
