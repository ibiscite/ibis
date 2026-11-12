use tauri::Monitor;
use x11rb::{
    connection::Connection,
    protocol::{
        randr::ConnectionExt as _,
        xproto::{AtomEnum, ConnectionExt as _},
    },
};

use super::Area;

fn connection() -> Option<(x11rb::rust_connection::RustConnection, usize)> {
    use gtk::prelude::*;
    // Do not use XWayland's partial view as native Wayland placement authority.
    if gtk::gdk::Display::default()?.type_().name() != "GdkX11Display" {
        return None;
    }
    x11rb::connect(None).ok()
}

pub(super) fn focused_window() -> Option<Area> {
    let (connection, screen) = connection()?;
    let root = connection.setup().roots.get(screen)?.root;
    let active = (|| {
        let atom = connection
            .intern_atom(true, b"_NET_ACTIVE_WINDOW")
            .ok()?
            .reply()
            .ok()?
            .atom;
        if atom == 0 {
            return None;
        }
        let reply = connection
            .get_property(false, root, atom, AtomEnum::WINDOW, 0, 1)
            .ok()?
            .reply()
            .ok()?;
        reply.value32().and_then(|mut values| values.next())
    })();
    let window = active
        .filter(|id| *id > 1)
        .or_else(|| connection.get_input_focus().ok()?.reply().ok().map(|reply| reply.focus))?;
    if window <= 1 {
        return None;
    }
    let geometry = connection.get_geometry(window).ok()?.reply().ok()?;
    let at = connection
        .translate_coordinates(window, root, 0, 0)
        .ok()?
        .reply()
        .ok()?;
    Some(Area {
        x: f64::from(at.dst_x),
        y: f64::from(at.dst_y),
        width: f64::from(geometry.width),
        height: f64::from(geometry.height),
    })
}

pub(super) fn display_key(monitor: &Monitor) -> Option<String> {
    let (connection, screen) = connection()?;
    let root = connection.setup().roots.get(screen)?.root;
    let resources = connection.randr_get_screen_resources_current(root).ok()?.reply().ok()?;
    for output in resources.outputs {
        let info = connection
            .randr_get_output_info(output, resources.config_timestamp)
            .ok()?
            .reply()
            .ok()?;
        if info.crtc == 0 {
            continue;
        }
        let crtc = connection
            .randr_get_crtc_info(info.crtc, resources.config_timestamp)
            .ok()?
            .reply()
            .ok()?;
        if i32::from(crtc.x) == monitor.position().x && i32::from(crtc.y) == monitor.position().y {
            let name = String::from_utf8(info.name).ok()?;
            if !name.is_empty() {
                return Some(format!("x11:{name}"));
            }
        }
    }
    None
}

pub(super) fn usable(_monitor: &Monitor) -> Option<Area> {
    None // GDK's work area; the Wayland compositor may override our request.
}
