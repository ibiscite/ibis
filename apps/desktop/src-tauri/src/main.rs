//! Ibis desktop app: a thin Tauri shell over `ibis-core`.
//!
//! The window is the picker. All behaviour that can be tested without a
//! window lives in `ibis-core`; these modules only translate between the web
//! interface and the core.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bindings;
mod copy;
mod diagnostics;
mod library;
mod picker;
mod preferences;
mod tray;

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use ibis_core::settings;
use tauri::{Manager, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::diagnostics::{NativeIssue, checked, report};
use crate::library::{Library, LibraryWatch, LoadRequests, start_open};
use crate::picker::{DialogOpen, ShortcutStatus, configured_shortcut, hides_on_blur, show_picker, toggle_picker};
use crate::preferences::SettingsFile;
use crate::tray::create_tray;

fn main() {
    let app = tauri::Builder::default()
        // Must be registered first: a second launch brings the running picker forward.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_picker(app)))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle_picker(app);
                    }
                })
                .build(),
        )
        .manage(Mutex::new(Library::default()))
        .manage(LoadRequests::default())
        .manage(LibraryWatch::default())
        .manage(DialogOpen::default())
        .manage(Mutex::new(ShortcutStatus::default()))
        .invoke_handler(bindings::commands().invoke_handler())
        .on_window_event(|window, event| match event {
            // Closing the picker hides it; the app keeps running for the shortcut.
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                report(window.hide(), NativeIssue::Hide);
            }
            // Like other launchers, the picker hides when it loses focus, but
            // not to its own file dialog.
            WindowEvent::Focused(false) => {
                let dialog_open = window.state::<DialogOpen>().0.load(Ordering::SeqCst);
                if hides_on_blur(!cfg!(debug_assertions), dialog_open) {
                    report(window.hide(), NativeIssue::Hide);
                }
            }
            _ => {}
        })
        .setup(|app| {
            // Place the launcher near the top of the usable screen, clear of
            // menu bars and taskbars. Later shows keep the user's placement.
            if let Some(window) = checked(app.get_webview_window("main").ok_or(()), NativeIssue::Window)
                && let (Some(Some(monitor)), Some(size)) = (
                    checked(window.current_monitor(), NativeIssue::Monitor),
                    checked(window.outer_size(), NativeIssue::Size),
                )
            {
                let area = monitor.work_area();
                let x = area.position.x + (area.size.width.saturating_sub(size.width) / 2) as i32;
                let y = area.position.y + (area.size.height.saturating_sub(size.height) / 5) as i32;
                report(
                    window.set_position(tauri::PhysicalPosition::new(x, y)),
                    NativeIssue::Position,
                );
            }
            // Settings live in the app's configuration folder, e.g. on macOS
            // ~/Library/Application Support/io.github.ibiscite.ibis/settings.json
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let loaded = settings::load(&settings_path);
            app.manage(SettingsFile::new(settings_path.clone()));
            if let Some(problem) = &loaded.problem {
                eprintln!("ibis: {problem}");
            }
            let (shortcut, label, setting_error) = configured_shortcut(loaded.settings.shortcut.as_deref());
            let result = app.global_shortcut().register(shortcut);
            let registration_error = result
                .err()
                .map(|e| format!("the shortcut could not be registered ({e}); another app may be using it"));
            let status = ShortcutStatus {
                shortcut: label,
                registered: registration_error.is_none(),
                error: registration_error.or(setting_error),
            };
            if let Some(error) = &status.error {
                eprintln!("ibis: {error}");
            }
            if let Ok(mut current) = app.state::<Mutex<ShortcutStatus>>().lock() {
                *current = status;
            }
            let shown_shortcut = app
                .state::<Mutex<ShortcutStatus>>()
                .lock()
                .ok()
                .filter(|s| s.registered)
                .map(|s| s.shortcut.clone());
            // A missing tray library on Linux panics inside the tray crate; the
            // picker's own library controls still work without the tray.
            let tray = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                create_tray(app.handle(), shown_shortcut.as_deref())
            }));
            match tray {
                Ok(Ok(())) => {}
                Ok(Err(e)) => eprintln!("ibis: the menu bar item could not be created ({e})"),
                Err(_) => eprintln!("ibis: the menu bar item is not available on this system"),
            }
            // The library from settings; IBIS_LIBRARY overrides it for development.
            let library = std::env::var("IBIS_LIBRARY")
                .ok()
                .or_else(|| loaded.settings.library.as_ref().map(|p| p.display().to_string()));
            if let Some(path) = library {
                // Opened in the background so the window appears at once.
                start_open(app.handle(), PathBuf::from(path), false);
            }
            Ok(())
        })
        .build(tauri::generate_context!());
    match app {
        Ok(app) => app.run(|app, event| {
            // macOS: clicking the Dock icon brings back the hidden picker.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                show_picker(app);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        }),
        Err(err) => {
            eprintln!("ibis: the app could not start: {err}");
            std::process::exit(1);
        }
    }
}
