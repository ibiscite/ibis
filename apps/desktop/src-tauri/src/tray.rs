//! The menu bar (macOS) or tray (Windows, Linux) item.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

use crate::library::{LibraryStatus, choose_library_file, current_status, reload_library_file};
use crate::picker::show_picker;

/// One line describing the library, for the menu bar or tray menu.
pub(crate) fn library_line(status: &LibraryStatus) -> String {
    let line = match (&status.error, &status.name) {
        (Some(error), _) => format!("Library could not be opened: {error}"),
        (None, Some(name)) => {
            let problems = match status.warnings {
                0 => String::new(),
                1 => ", 1 problem".to_string(),
                n => format!(", {n} problems"),
            };
            format!("{name}: {} references{problems}", status.references)
        }
        (None, None) => "No library chosen".to_string(),
    };
    let line = match status.note {
        Some(_) => format!("{line} (not saved for next time)"),
        None => line,
    };
    // Menus do not wrap; keep long errors readable.
    if line.chars().count() > 90 {
        format!("{}…", line.chars().take(89).collect::<String>())
    } else {
        line
    }
}

/// The disabled menu item that shows the library line.
pub(crate) struct LibraryMenuItem(pub(crate) MenuItem<tauri::Wry>);

/// The menu bar (macOS) or tray (Windows, Linux) item and its menu.
pub(crate) fn create_tray(app: &AppHandle, shortcut: Option<&str>) -> tauri::Result<()> {
    let open_label = match shortcut {
        Some(shortcut) => format!("Open Picker ({shortcut})"),
        None => "Open Picker".to_string(),
    };
    let line = MenuItem::with_id(app, "library", library_line(&current_status(app)), false, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open", open_label, true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &line,
            &MenuItem::with_id(app, "choose", "Choose Library File…", true, None::<&str>)?,
            &MenuItem::with_id(app, "reload", "Reload Library", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", "Quit Ibis", true, None::<&str>)?,
        ],
    )?;
    app.manage(LibraryMenuItem(line));
    let mut tray = TrayIconBuilder::with_id("ibis")
        .tooltip("Ibis")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_picker(app),
            "choose" => choose_library_file(app),
            "reload" => reload_library_file(app),
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_library_line_explains_failures_and_stays_short() {
        let status = |name: Option<&str>, warnings, error: Option<&str>| LibraryStatus {
            name: name.map(String::from),
            references: 12,
            warnings,
            error: error.map(String::from),
            note: None,
        };
        assert_eq!(library_line(&status(None, 0, None)), "No library chosen");
        assert_eq!(
            library_line(&status(Some("refs.bib"), 0, None)),
            "refs.bib: 12 references"
        );
        assert_eq!(
            library_line(&status(Some("refs.bib"), 2, None)),
            "refs.bib: 12 references, 2 problems"
        );
        let unsaved = LibraryStatus {
            note: Some("could not be saved".into()),
            ..status(Some("refs.bib"), 0, None)
        };
        assert_eq!(
            library_line(&unsaved),
            "refs.bib: 12 references (not saved for next time)"
        );
        let long = "x".repeat(200);
        let line = library_line(&status(Some("refs.bib"), 0, Some(&long)));
        assert!(line.starts_with("Library could not be opened: x"));
        assert_eq!(line.chars().count(), 90);
    }
}
