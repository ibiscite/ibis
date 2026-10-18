//! Authoritative X11 keyboard focus, independent of GTK activation events.

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tauri::{Runtime, WebviewWindow};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::ConnectionExt;

pub(super) fn query<R: Runtime>(window: &WebviewWindow<R>) -> Result<Option<bool>, ()> {
    // Tauri dispatches this getter to the window thread. Only the integer X11
    // identity crosses back; GTK objects and native pointers are never used.
    let handle = window.window_handle().map_err(|_| ())?;
    let picker = match handle.as_raw() {
        RawWindowHandle::Xlib(handle) => u32::try_from(handle.window).map_err(|_| ())?,
        RawWindowHandle::Xcb(handle) => handle.window.get(),
        _ => return Ok(None), // Wayland retains the toolkit query.
    };
    let (connection, _) = x11rb::connect(None).map_err(|_| ())?;
    query_connection(&connection, picker).map(Some)
}

fn query_connection(connection: &impl Connection, picker: u32) -> Result<bool, ()> {
    // A passive shortcut grab redirects events but does not replace the stored
    // input focus returned by this server request (covered by the native test).
    let focused = connection
        .get_input_focus()
        .map_err(|_| ())?
        .reply()
        .map_err(|_| ())?
        .focus;
    contains_focus(picker, focused, |window| {
        connection
            .query_tree(window)
            .map_err(|_| ())?
            .reply()
            .map_err(|_| ())
            .map(|tree| tree.parent)
    })
}

fn contains_focus(picker: u32, mut focused: u32, mut parent: impl FnMut(u32) -> Result<u32, ()>) -> Result<bool, ()> {
    if picker <= 1 {
        return Err(());
    }
    let mut seen = Vec::with_capacity(64);
    for depth in 0..=64 {
        if focused == picker {
            return Ok(true);
        }
        if focused == 0 {
            return Ok(false);
        }
        // PointerRoot does not identify a specific owner. Treat it as an
        // unavailable query, never a reason to hide using cached GTK focus.
        if focused == 1 || depth == 64 || seen.contains(&focused) {
            return Err(());
        }
        seen.push(focused);
        focused = parent(focused)?;
    }
    Err(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires an isolated X11 display; run with xvfb-run"]
    fn x11_server_focus_stays_with_editor_during_keyboard_grab() {
        use x11rb::protocol::xproto::{CreateWindowAux, GrabMode, GrabStatus, InputFocus, WindowClass};
        fn synthetic<T, E>(result: Result<T, E>) -> T {
            result.unwrap_or_else(|_| panic!("synthetic X11 request failed"))
        }
        let (connection, screen) = synthetic(x11rb::connect(None));
        let root = connection.setup().roots[screen].root;
        let picker = synthetic(connection.generate_id());
        let editor = synthetic(connection.generate_id());
        let child = synthetic(connection.generate_id());
        for (window, parent) in [(picker, root), (editor, root), (child, picker)] {
            synthetic(
                synthetic(connection.create_window(
                    0,
                    window,
                    parent,
                    0,
                    0,
                    80,
                    50,
                    0,
                    WindowClass::INPUT_OUTPUT,
                    0,
                    &CreateWindowAux::default(),
                ))
                .check(),
            );
            synthetic(synthetic(connection.map_window(window)).check());
        }
        synthetic(synthetic(connection.set_input_focus(InputFocus::PARENT, editor, x11rb::CURRENT_TIME)).check());
        assert_eq!(query_connection(&connection, picker), Ok(false));
        // Activating a passive key grab creates an active keyboard grab. That
        // routing change must not be mistaken for a change in stored focus.
        let grab = synthetic(
            synthetic(connection.grab_keyboard(false, root, x11rb::CURRENT_TIME, GrabMode::ASYNC, GrabMode::ASYNC))
                .reply(),
        );
        assert_eq!(grab.status, GrabStatus::SUCCESS);
        assert_eq!(synthetic(synthetic(connection.get_input_focus()).reply()).focus, editor);
        assert_eq!(query_connection(&connection, picker), Ok(false));
        synthetic(synthetic(connection.set_input_focus(InputFocus::PARENT, child, x11rb::CURRENT_TIME)).check());
        assert_eq!(query_connection(&connection, picker), Ok(true));
        synthetic(synthetic(connection.ungrab_keyboard(x11rb::CURRENT_TIME)).check());
        // Disconnecting destroys only this connection's synthetic windows.
    }

    #[test]
    fn focus_belongs_to_the_picker_or_its_descendants_only() {
        assert_eq!(contains_focus(10, 10, |_| panic!("already at picker")), Ok(true));
        assert_eq!(contains_focus(10, 12, |id| Ok(id - 1)), Ok(true));
        assert_eq!(contains_focus(10, 20, |_| Ok(0)), Ok(false));
        assert_eq!(contains_focus(10, 0, |_| panic!("no focus")), Ok(false));
        assert_eq!(
            contains_focus(10, 1, |_| panic!("pointer-root focus is ambiguous")),
            Err(())
        );
    }

    #[test]
    fn destroyed_windows_cycles_and_excessive_depth_do_not_claim_focus() {
        assert_eq!(contains_focus(10, 20, |_| Err(())), Err(()));
        assert_eq!(contains_focus(10, 20, |_| Ok(20)), Err(()));
        assert_eq!(contains_focus(10, 20, |id| Ok(if id == 20 { 21 } else { 20 })), Err(()));
        let mut queries = 0;
        assert_eq!(
            contains_focus(10, 20, |id| {
                queries += 1;
                Ok(id + 1)
            }),
            Err(())
        );
        assert_eq!(queries, 64);
    }
}
