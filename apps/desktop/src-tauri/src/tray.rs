//! The menu bar (macOS) or tray (Windows, Linux) item.

use std::sync::Mutex;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

use crate::diagnostics::{NativeIssue, report};
use crate::feedback::{idea_url, open_in_browser, problem_url};
use crate::library::{
    LibraryState, LibraryStatus, choose_library_file, current_status, reload_library_file, use_zotero_library,
};
use crate::login_item::{self, LoginItems, LoginMenuItem};
use crate::picker::show_picker;
use crate::updater::{self, AutomaticMenuItem, UpdateMenuItem};

/// One line describing the library, for the menu bar or tray menu.
pub(crate) fn library_line(status: &LibraryStatus) -> String {
    let line = match (status.state, &status.error, &status.name) {
        (LibraryState::Loading, _, Some(name)) => format!("Opening {name}…"),
        (LibraryState::Reloading, _, Some(name)) => format!("Reading {name}…"),
        // The last loaded references are still searched: say "not updated", not "could not be opened".
        (_, Some(error), _) if status.stale && status.kept_references => {
            let reason = status
                .error_reason
                .map(|reason| reason.to_string())
                .unwrap_or_else(|| error.clone());
            let name = status.snapshot_name.as_deref().unwrap_or("Library");
            format!("{name}: {} references, not updated: {reason}", status.references)
        }
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
    let line = if status.choice_not_saved {
        format!("{line} (not saved for next time)")
    } else {
        line
    };
    // Menus do not wrap; keep long errors readable.
    if line.chars().count() > 90 {
        format!("{}…", line.chars().take(89).collect::<String>())
    } else {
        line
    }
}

/// The disabled line naming the running build, such as "Ibis Nightly 0.1.1-nightly.20261020031200".
fn version_line(name: &str, version: &str) -> String {
    format!("{name} {version}")
}

/// "Open Picker", which names the shortcut that opens it.
pub(crate) struct OpenMenuItem(MenuItem<tauri::Wry>);

fn open_label(shortcut: Option<&str>) -> String {
    match shortcut {
        Some(shortcut) => format!("Open Picker ({shortcut})"),
        None => "Open Picker".to_string(),
    }
}

/// Name a changed shortcut in the menu at once.
pub(crate) fn show_shortcut(app: &AppHandle, shortcut: Option<&str>) {
    if let Some(item) = app.try_state::<OpenMenuItem>() {
        report(item.0.set_text(open_label(shortcut)), NativeIssue::Shortcut);
    }
}

#[cfg(target_os = "macos")]
static HIDDEN: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
static APPLIED: AtomicU64 = AtomicU64::new(0);

/// Off, the shortcut still opens the picker and opening Ibis opens Settings.
/// Saves can finish out of order, so `revision` decides; only the main thread
/// reads or changes it. Other systems always keep their tray.
#[cfg(target_os = "macos")]
pub(crate) fn show_in_menu_bar<R: Runtime>(app: &AppHandle<R>, shown: bool, revision: u64) {
    let handle = app.clone();
    report(
        app.run_on_main_thread(move || {
            if revision < APPLIED.load(Ordering::SeqCst) {
                return;
            }
            APPLIED.store(revision, Ordering::SeqCst);
            HIDDEN.store(!shown, Ordering::SeqCst);
            if let Some(tray) = handle.tray_by_id("ibis") {
                report(tray.set_visible(shown), NativeIssue::TrayCreation);
                if shown {
                    crate::menu_bar::watch(&tray);
                }
            }
        }),
        NativeIssue::TrayCreation,
    );
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn show_in_menu_bar<R: Runtime>(_: &AppHandle<R>, _: bool, _: u64) {}

#[cfg(target_os = "macos")]
pub(crate) fn hidden() -> bool {
    HIDDEN.load(Ordering::SeqCst)
}

/// ⌥-click: the version line becomes one line of local counts.
#[cfg(target_os = "macos")]
fn show_stats(app: &AppHandle) {
    let held = objc2_app_kit::NSEvent::modifierFlags_class().contains(objc2_app_kit::NSEventModifierFlags::Option);
    let Some(item) = app.try_state::<VersionMenuItem>() else {
        return;
    };
    let line = if held {
        stats_line(&current_status(app), epoch_days(std::time::SystemTime::now()))
    } else {
        item.1.clone()
    };
    report(item.0.set_text(line), NativeIssue::TrayLibrary);
}

#[cfg(target_os = "macos")]
fn epoch_days(time: std::time::SystemTime) -> u64 {
    time.duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() / 86_400)
}

/// "3 libraries · 12,408 references · opened today", from local counts only.
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
fn stats_line(status: &LibraryStatus, today: u64) -> String {
    let plural = |count: usize, noun: &str| format!("{} {noun}{}", grouped(count), if count == 1 { "" } else { "s" });
    let mut line = format!(
        "{} · {}",
        plural(status.source_counts.len().max(1), "library").replace("librarys", "libraries"),
        plural(status.references, "reference")
    );
    let opened = status
        .updated_at
        .map(|millis| today.saturating_sub(millis as u64 / 86_400_000));
    match opened {
        Some(0) => line.push_str(" · opened today"),
        Some(1) => line.push_str(" · opened yesterday"),
        Some(days) => line.push_str(&format!(" · opened {days} days ago")),
        None => {}
    }
    line
}

fn grouped(count: usize) -> String {
    let digits = count.to_string();
    let lead = digits.len() % 3;
    let mut text = digits[..lead].to_string();
    for group in digits.as_bytes()[lead..].chunks(3) {
        if !text.is_empty() {
            text.push(',');
        }
        text.push_str(std::str::from_utf8(group).unwrap_or_default());
    }
    text
}

/// The version line and its usual text.
#[cfg(target_os = "macos")]
struct VersionMenuItem(MenuItem<tauri::Wry>, String);

/// Text from Search with Ibis waiting for the picker: a launch may beat it.
#[derive(Default)]
pub(crate) struct SearchRequest(pub(crate) Mutex<Option<String>>);

/// The picker collects text from Search with Ibis once.
#[tauri::command]
#[specta::specta]
pub(crate) fn take_search_request(request: tauri::State<'_, SearchRequest>) -> Option<String> {
    request.0.lock().ok().and_then(|mut waiting| waiting.take())
}

/// A citation segment took its place: the trackpad's alignment tick.
#[tauri::command]
#[specta::specta]
pub(crate) fn alignment_tick() {
    #[cfg(target_os = "macos")]
    crate::menu_bar::tick();
}

/// The disabled menu item that shows the library line.
pub(crate) struct LibraryMenuItem(pub(crate) MenuItem<tauri::Wry>);

/// The menu bar (macOS) or tray (Windows, Linux) item and its menu.
pub(crate) fn create_tray(app: &AppHandle, shortcut: Option<&str>) -> tauri::Result<()> {
    let line = MenuItem::with_id(app, "library", library_line(&current_status(app)), false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", open_label(shortcut), true, None::<&str>)?;
    let choose = MenuItem::with_id(app, "choose", "Choose Library File…", true, None::<&str>)?;
    let zotero = MenuItem::with_id(app, "zotero", "Use Zotero Library", true, None::<&str>)?;
    let reload = MenuItem::with_id(app, "reload", "Reload Library", true, None::<&str>)?;
    let problem = MenuItem::with_id(app, "problem", "Report a Problem…", true, None::<&str>)?;
    let idea = MenuItem::with_id(app, "idea", "Suggest an Idea…", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, Some("CmdOrCtrl+,"))?;
    let reset_window = MenuItem::with_id(app, "reset_window", "Reset Window Position", true, None::<&str>)?;
    // "Ibis" or "Ibis Nightly", so the two apps can be told apart.
    let name = app.package_info().name.clone();
    let quit = MenuItem::with_id(app, "quit", format!("Quit {name}"), true, None::<&str>)?;
    let version = version_line(&name, &app.package_info().version.to_string());
    let version_text = version.clone();
    let version = MenuItem::with_id(app, "version", version, false, None::<&str>)?;
    let (first, second, third) = (
        PredefinedMenuItem::separator(app)?,
        PredefinedMenuItem::separator(app)?,
        PredefinedMenuItem::separator(app)?,
    );
    let mut items: Vec<&dyn IsMenuItem<tauri::Wry>> =
        vec![&open, &first, &version, &line, &choose, &zotero, &reload, &second];
    // Builds without an update key have no update items.
    let update_items = if updater::available(app) {
        Some((
            MenuItem::with_id(app, "update_now", updater::update_menu_label(false), true, None::<&str>)?,
            CheckMenuItem::with_id(
                app,
                updater::setting::AUTOMATIC_ON,
                "Check for Updates Automatically",
                true,
                false,
                None::<&str>,
            )?,
            CheckMenuItem::with_id(
                app,
                updater::setting::AUTOMATIC_OFF,
                "Check for Updates Automatically",
                true,
                true,
                None::<&str>,
            )?,
        ))
    } else {
        None
    };
    let automatic_position = items.len() + 1;
    if let Some((now, automatic, _)) = &update_items {
        items.extend([now as &dyn IsMenuItem<tauri::Wry>, automatic]);
    }
    // Only where the system offers login items; ticked as the system says.
    let at_login = match app.try_state::<LoginItems>() {
        Some(items) if items.available() => Some(CheckMenuItem::with_id(
            app,
            "login",
            login_item::menu_label(),
            true,
            items.read().enabled,
            None::<&str>,
        )?),
        _ => None,
    };
    if let Some(at_login) = &at_login {
        items.push(at_login);
    }
    items.extend([
        &settings as &dyn IsMenuItem<tauri::Wry>,
        &reset_window,
        &problem,
        &idea,
        &third,
        &quit,
    ]);
    let menu = Menu::with_items(app, &items)?;
    if let Some((now, off, on)) = update_items {
        app.manage(UpdateMenuItem(now));
        app.manage(AutomaticMenuItem::new(menu.clone(), [off, on], automatic_position));
    }
    #[cfg(target_os = "macos")]
    app.manage(VersionMenuItem(version.clone(), version_text));
    #[cfg(not(target_os = "macos"))]
    let _ = version_text;
    app.manage(LibraryMenuItem(line));
    app.manage(OpenMenuItem(open.clone()));
    if let Some(at_login) = at_login {
        app.manage(LoginMenuItem(at_login));
    }
    let mut tray = TrayIconBuilder::with_id("ibis")
        .tooltip(&name)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_picker(app),
            "settings" => crate::settings_window::open(app, None),
            "reset_window" => crate::sizing::reset_position(app),
            "choose" => choose_library_file(app),
            "zotero" => use_zotero_library(app),
            "reload" => reload_library_file(app),
            "problem" => open_in_browser(app, problem_url(&app.package_info().version.to_string())),
            "idea" => open_in_browser(app, idea_url()),
            "update_now" => updater::check_now(app),
            updater::setting::AUTOMATIC_ON | updater::setting::AUTOMATIC_OFF => {
                updater::setting::toggle_automatic(app, event.id.as_ref());
            }
            "login" => login_item::toggle_from_menu(app),
            "quit" => app.exit(0),
            _ => {}
        })
        // Someone may have changed it in the system's settings since.
        .on_tray_icon_event(|tray, event| {
            #[cfg(target_os = "macos")]
            if let TrayIconEvent::Click {
                button_state: tauri::tray::MouseButtonState::Down,
                ..
            } = event
            {
                show_stats(tray.app_handle());
            }
            if matches!(event, TrayIconEvent::Click { .. } | TrayIconEvent::Enter { .. }) {
                // Registry, file and ServiceManagement reads block: keep them
                // off the main thread that draws the menu.
                let app = tray.app_handle().clone();
                tauri::async_runtime::spawn_blocking(move || login_item::current(&app));
            }
        });
    #[cfg(target_os = "macos")]
    {
        tray = tray.icon(crate::menu_bar::BIRD).icon_as_template(true);
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    let _tray = tray.build(app)?;
    #[cfg(target_os = "macos")]
    crate::menu_bar::accept_drops(app, &_tray);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_line_names_the_build() {
        assert_eq!(version_line("Ibis", "0.1.0"), "Ibis 0.1.0");
        assert_eq!(
            version_line("Ibis Nightly", "0.1.1-nightly.20261020031200"),
            "Ibis Nightly 0.1.1-nightly.20261020031200"
        );
    }

    #[test]
    fn the_library_line_explains_failures_and_stays_short() {
        let status = |name: Option<&str>, warnings, error: Option<&str>| LibraryStatus {
            revision: "synthetic".into(),
            source_generation: "1".into(),
            source_is_requested: true,
            state: LibraryState::Loaded,
            zotero: false,
            zotero_libraries: Vec::new(),
            has_snapshot: name.is_some(),
            name: name.map(String::from),
            snapshot_name: name.map(String::from),
            references: 12,
            source_counts: Default::default(),
            warnings,
            error: error.map(String::from),
            problem: None,
            error_reason: None,
            kept_references: false,
            choice_not_saved: false,
            note: None,
            settings_notice: None,
            snapshot_zotero: false,
            sample: false,
            stale: false,
            updated_at: None,
            checked_at: None,
        };
        assert_eq!(library_line(&status(None, 0, None)), "No library chosen");
        for (state, expected) in [
            (LibraryState::Loading, "Opening refs.bib…"),
            (LibraryState::Reloading, "Reading refs.bib…"),
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
            choice_not_saved: true,
            ..status(Some("refs.bib"), 0, None)
        };
        assert_eq!(
            library_line(&unsaved),
            "refs.bib: 12 references (not saved for next time)"
        );
        let counted = LibraryStatus {
            source_counts: [("a.bib".to_string(), 12_000), ("b.bib".to_string(), 408)].into(),
            references: 12_408,
            updated_at: Some(20_000.0 * 86_400_000.0),
            ..status(Some("a.bib"), 0, None)
        };
        assert_eq!(
            stats_line(&counted, 20_000),
            "2 libraries · 12,408 references · opened today"
        );
        assert_eq!(
            stats_line(&counted, 20_001),
            "2 libraries · 12,408 references · opened yesterday"
        );
        let single = LibraryStatus {
            updated_at: None,
            references: 1,
            source_counts: Default::default(),
            ..counted
        };
        assert_eq!(stats_line(&single, 20_000), "1 library · 1 reference");
        let kept = LibraryStatus {
            state: LibraryState::Error,
            name: Some("refs.bib".into()),
            snapshot_name: Some("refs.bib".into()),
            stale: true,
            kept_references: true,
            error_reason: Some(crate::library::LoadError::Source(
                ibis_core::source::SourceError::Unavailable,
            )),
            ..status(Some("refs.bib"), 0, Some("presentation wording is not a status code"))
        };
        assert_eq!(
            library_line(&kept),
            "refs.bib: 12 references, not updated: the library file is unavailable; download any onlin…"
        );
        let long = "x".repeat(200);
        let line = library_line(&status(Some("refs.bib"), 0, Some(&long)));
        assert!(line.starts_with("Library could not be opened: x"));
        assert_eq!(line.chars().count(), 90);
    }
}
