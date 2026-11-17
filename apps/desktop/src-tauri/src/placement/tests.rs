use super::*;

const fn rect(x: f64, y: f64, width: f64, height: f64) -> Area {
    Area { x, y, width, height }
}

#[test]
fn settings_creation_selects_the_200_percent_display_beside_a_100_percent_display() {
    let screens = [
        CreationDisplay {
            bounds: rect(0.0, 0.0, 1920.0, 1080.0),
            usable: rect(0.0, 0.0, 1920.0, 1040.0),
            scale: 1.0,
        },
        CreationDisplay {
            bounds: rect(1920.0, 0.0, 3840.0, 2160.0),
            usable: rect(1920.0, 0.0, 3840.0, 2080.0),
            scale: 2.0,
        },
    ];
    for target in [0, 1] {
        let point = creation_origin(&screens, target, (920.0, 560.0), 32.0, true).unwrap();
        // Tao's Windows creation path tests the logical point with each
        // monitor's scale and takes the FIRST matching display.
        let chosen = screens
            .iter()
            .position(|screen| screen.logical(screen.bounds).distance(point) == 0.0);
        assert_eq!(
            chosen,
            Some(target),
            "{point:?} must choose the target before its first size uses that DPI"
        );
        let area = screens[target].logical(screens[target].usable);
        assert!(area.holds(&rect(point.0, point.1, 920.0, 560.0)));
    }
}

#[test]
fn picker_corners_respect_the_work_area_and_small_displays() {
    use ibis_engine::settings::PickerPlacement::*;
    let area = rect(-1200.0, 25.0, 1200.0, 800.0);
    for (placement, expected) in [
        (TopLeft, (-1184.0, 41.0)),
        (TopRight, (-736.0, 41.0)),
        (BottomLeft, (-1184.0, 329.0)),
        (BottomRight, (-736.0, 329.0)),
    ] {
        let point = launcher_position(area, 720.0, 480.0, 26.5, placement);
        assert_eq!(point, expected);
        assert!(area.holds(&rect(point.0, point.1, 720.0, 480.0)));
        assert_eq!(
            launcher_position(rect(10.0, 20.0, 300.0, 200.0), 720.0, 480.0, 26.5, placement),
            (10.0, 20.0)
        );
    }
}

#[test]
fn search_anchor_excludes_menu_notch_and_dock_and_ignores_frame_height() {
    let usable = rect(0.0, 38.0, 1728.0, 1015.0);
    for height in [400.0, 480.0, 600.0] {
        let (x, y) = launcher_position(
            usable,
            720.0,
            height,
            26.5,
            ibis_engine::settings::PickerPlacement::Centre,
        );
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
        let (x, y) = launcher_position(
            work,
            width,
            height,
            search,
            ibis_engine::settings::PickerPlacement::Centre,
        );
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
