//! Ibis desktop app: a thin Tauri shell over `ibis-core`.
//!
//! The window is the picker. All behaviour that can be tested without a
//! window lives in `ibis-core`; these modules only translate between the web
//! interface and the core.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bindings;
mod copy;
mod diagnostics;
#[cfg(debug_assertions)]
mod gallery;
mod library;
mod native_feel;
#[cfg(target_os = "macos")]
mod panel;
#[cfg(any(target_os = "macos", test))]
mod panel_startup;
mod picker;
mod placement;
mod preferences;
mod tray;

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use ibis_core::settings;
use tauri::{Manager, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

#[cfg(debug_assertions)]
use crate::diagnostics::report;
use crate::diagnostics::{NativeIssue, checked};
use crate::library::{Library, LibraryWatch, LoadRequests, start_open};
use crate::picker::{
    DialogOpen, ShortcutStatus, configured_shortcut, dismiss_picker, hides_on_blur, register_shortcut, show_picker,
    toggle_picker,
};
use crate::placement::place_on_active_display;
use crate::preferences::SettingsFile;
use crate::tray::create_tray;

fn main() {
    let mut context = tauri::generate_context!();
    native_feel::configure(&mut context);
    #[cfg(target_os = "macos")]
    let initial_panel = panel::prepare_startup(&mut context);
    #[cfg(debug_assertions)]
    report(gallery::configure(&mut context), NativeIssue::GalleryNavigation);
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.manage(initial_panel);
    let builder = builder
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
        .on_window_event(|window, event| {
            #[cfg(all(target_os = "macos", debug_assertions))]
            if window.label() == "main"
                && let WindowEvent::Focused(focused) = event
            {
                panel::observe_window_focus(window.app_handle(), *focused);
            }
            match event {
                // Closing the picker hides it; the app keeps running for the shortcut.
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    dismiss_picker(window.app_handle());
                }
                // Like other launchers, the picker hides when it loses focus, but
                // not to its own file dialog.
                WindowEvent::Focused(false) => {
                    let dialog_open = window.state::<DialogOpen>().0.load(Ordering::SeqCst);
                    #[cfg(target_os = "macos")]
                    let startup_allows_blur = panel::allows_automatic_blur(window.app_handle());
                    #[cfg(not(target_os = "macos"))]
                    let startup_allows_blur = true;
                    if startup_allows_blur && hides_on_blur(!cfg!(debug_assertions), dialog_open) {
                        dismiss_picker(window.app_handle());
                    }
                }
                _ => {}
            }
        })
        .setup(|app| {
            #[cfg(target_os = "macos")]
            panel::configure(app.handle())?;
            native_feel::install(app.handle());
            // Debug builds may opt into the synthetic evidence gallery. Keep
            // both environment reads out of release binaries entirely.
            #[cfg(debug_assertions)]
            {
                let enabled = std::env::var("IBIS_GALLERY").ok();
                if enabled.as_deref() == Some("1")
                    && let Some(window) = checked(app.get_webview_window("main").ok_or(()), NativeIssue::Window)
                {
                    let state = std::env::var("IBIS_GALLERY_STATE").ok();
                    report(
                        gallery::install_readiness(
                            app.handle(),
                            &window,
                            state.as_deref(),
                            std::env::var_os("IBIS_GALLERY_READY_FILE").map(PathBuf::from),
                        ),
                        NativeIssue::GalleryNavigation,
                    );
                }
            }
            // The picker opens near the top of the display with the pointer.
            if let Some(window) = checked(app.get_webview_window("main").ok_or(()), NativeIssue::Window) {
                place_on_active_display(app.handle(), &window, true);
            }
            // Settings live in the app's configuration folder, e.g. on macOS
            // ~/Library/Application Support/io.github.ibiscite.ibis/settings.json
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let loaded = settings::load(&settings_path);
            app.manage(SettingsFile::new(settings_path.clone()));
            if let Some(problem) = &loaded.problem {
                eprintln!("ibis: {}", preferences::settings_problem(problem));
            }
            if let Ok(mut library) = app.state::<Mutex<Library>>().lock() {
                library.settings_notice = preferences::settings_notice(&settings_path, &loaded);
            }
            let (shortcut, label, setting_error) = configured_shortcut(loaded.settings.shortcut.as_deref());
            let registration_error = register_shortcut(app.handle(), shortcut).err().map(str::to_string);
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
                .or_else(|| match &loaded.settings.library {
                    Some(settings::LibrarySetting::Bibtex(path)) => Some(path.display().to_string()),
                    Some(settings::LibrarySetting::Zotero(_)) => {
                        // Written by hand or by a newer build; reading Zotero
                        // from the app arrives with #13.
                        eprintln!("ibis: this build cannot open Zotero libraries yet; choose a library file instead");
                        None
                    }
                    None => None,
                });
            if let Some(path) = library {
                // Opened in the background so the window appears at once.
                start_open(app.handle(), PathBuf::from(path), false);
            }
            Ok(())
        });
    #[cfg(target_os = "macos")]
    let builder = builder.on_page_load(|webview, payload| {
        if webview.label() == "main" && payload.event() == tauri::webview::PageLoadEvent::Finished {
            #[cfg(debug_assertions)]
            panel::observe_page_load_finished(webview.app_handle());
            panel::page_loaded(webview.app_handle());
        }
    });
    let app = builder.build(context);
    match app {
        Ok(app) => app.run(|app, event| {
            // macOS: a repeated OS open request brings back the hidden picker.
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

#[cfg(test)]
mod offline_tests {
    /// WebView2 on Windows fetched updates and other background data over
    /// HTTPS while Ibis ran, although PRIVACY.md promises no network use
    /// (#201). These arguments replace wry's defaults, so they must keep them.
    #[test]
    fn the_windows_webview_keeps_wry_defaults_and_has_no_route_to_the_network() {
        let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap_or_default();
        let args = config["app"]["windows"][0]["additionalBrowserArgs"]
            .as_str()
            .unwrap_or_default();
        for arg in [
            "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection",
            "--disable-background-networking",
            "--disable-component-update",
            "--proxy-server=http://127.0.0.1:9",
        ] {
            assert!(args.split(' ').any(|a| a == arg), "missing {arg}");
        }
        assert!(
            !args.contains("proxy-bypass-list"),
            "the app's own *.localhost pages must keep bypassing the proxy"
        );
    }
}
