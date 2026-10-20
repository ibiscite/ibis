//! The menu bar (macOS) or tray (Windows, Linux) item.

use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

use crate::feedback::{idea_url, open_in_browser, problem_url};
use crate::library::{
    LibraryState, LibraryStatus, choose_library_file, current_status, reload_library_file, use_zotero_library,
};
use crate::picker::show_picker;
use crate::updater::{self, AutomaticMenuItem};

/// One line describing the library, for the menu bar or tray menu.
pub(crate) fn library_line(status: &LibraryStatus) -> String {
    let line = match (status.state, &status.error, &status.name) {
        (LibraryState::Loading, _, Some(name)) => format!("Opening {name}…"),
        (LibraryState::Reloading, _, Some(name)) => format!("Reloading {name}…"),
        (_, Some(error), _) => format!("Library could not be opened: {error}"),
        (_, None, Some(name)) => {
            let problems = match status.warnings {
                0 => String::new(),
                1 => ", 1 problem".to_string(),
                n => format!(", {n} problems"),
            };
            format!("{name}: {} references{problems}", status.references)
        }
        (_, None, None) => "No library chosen".to_string(),
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
    let open = MenuItem::with_id(app, "open", open_label, true, None::<&str>)?;
    let choose = MenuItem::with_id(app, "choose", "Choose Library File…", true, None::<&str>)?;
    let zotero = MenuItem::with_id(app, "zotero", "Use Zotero Library", true, None::<&str>)?;
    let reload = MenuItem::with_id(app, "reload", "Reload Library", true, None::<&str>)?;
    let problem = MenuItem::with_id(app, "problem", "Report a Problem…", true, None::<&str>)?;
    let idea = MenuItem::with_id(app, "idea", "Suggest an Idea…", true, None::<&str>)?;
    // "Ibis" or "Ibis Nightly", so the two apps can be told apart.
    let name = app.package_info().name.clone();
    let quit = MenuItem::with_id(app, "quit", format!("Quit {name}"), true, None::<&str>)?;
    let (first, second, third) = (
        PredefinedMenuItem::separator(app)?,
        PredefinedMenuItem::separator(app)?,
        PredefinedMenuItem::separator(app)?,
    );
    let mut items: Vec<&dyn IsMenuItem<tauri::Wry>> = vec![&open, &first, &line, &choose, &zotero, &reload, &second];
    // Builds without an update key have no update items.
    let update_items = if updater::available(app) {
        Some((
            MenuItem::with_id(app, "update_now", "Check for Updates…", true, None::<&str>)?,
            CheckMenuItem::with_id(
                app,
                "update_auto",
                "Check for Updates Automatically",
                true,
                false,
                None::<&str>,
            )?,
        ))
    } else {
        None
    };
    if let Some((now, automatic)) = &update_items {
        items.extend([now as &dyn IsMenuItem<tauri::Wry>, automatic]);
    }
    items.extend([&problem as &dyn IsMenuItem<tauri::Wry>, &idea, &third, &quit]);
    let menu = Menu::with_items(app, &items)?;
    if let Some((_, automatic)) = update_items {
        app.manage(AutomaticMenuItem(automatic));
    }
    app.manage(LibraryMenuItem(line));
    let mut tray = TrayIconBuilder::with_id("ibis")
        .tooltip(&name)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_picker(app),
            "choose" => choose_library_file(app),
            "zotero" => use_zotero_library(app),
            "reload" => reload_library_file(app),
            "problem" => open_in_browser(app, problem_url(&app.package_info().version.to_string())),
            "idea" => open_in_browser(app, idea_url()),
            "update_now" => updater::check_now(app),
            "update_auto" => updater::toggle_automatic(app),
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
            state: LibraryState::Loaded,
            zotero: false,
            has_snapshot: name.is_some(),
            name: name.map(String::from),
            snapshot_name: name.map(String::from),
            references: 12,
            warnings,
            error: error.map(String::from),
            note: None,
            settings_notice: None,
        };
        assert_eq!(library_line(&status(None, 0, None)), "No library chosen");
        for (state, expected) in [
            (LibraryState::Loading, "Opening refs.bib…"),
            (LibraryState::Reloading, "Reloading refs.bib…"),
        ] {
            assert_eq!(
                library_line(&LibraryStatus {
                    state,
                    ..status(Some("refs.bib"), 0, None)
                }),
                expected
            );
        }
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
