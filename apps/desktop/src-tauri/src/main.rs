//! Ibis desktop app: a thin Tauri shell over `ibis-core`.
//!
//! The window is the picker. All behaviour that can be tested without a
//! window lives in `ibis-core`; these modules only translate between the web
//! interface and the core.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bindings;
mod copy;
mod diagnostics;
mod feedback;
#[cfg(debug_assertions)]
mod gallery;
mod identity;
mod library;
mod login_item;
#[cfg(target_os = "macos")]
mod native_corners;
mod native_feel;
mod notice;
#[cfg(target_os = "macos")]
mod panel;
#[cfg(any(target_os = "macos", test))]
mod panel_startup;
mod picker;
mod placement;
mod preferences;
mod setup;
mod tray;
mod updater;

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use ibis_core::settings;
use tauri::{Manager, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

use crate::diagnostics::{NativeIssue, application_message, checked, report};
use crate::library::{Library, LibraryWatch, LoadRequests, SearchRequests, Source, start_open};
use crate::picker::{
    DialogOpen, Pinned, ShortcutStatus, configured_shortcut, dismiss_picker, hides_on_blur, register_shortcut,
    show_picker, toggle_picker,
};
use crate::placement::place_on_active_display;
use crate::preferences::SettingsFile;
use crate::tray::create_tray;

fn main() {
    let mut context = tauri::generate_context!();
    native_feel::configure(&mut context);
    // Started by the login entry: wait in the background for the shortcut.
    if login_item::launched_at_login(std::env::args_os()) {
        login_item::start_hidden(&mut context);
    }
    #[cfg(target_os = "macos")]
    let initial_panel = panel::prepare_startup(&mut context);
    #[cfg(debug_assertions)]
    report(gallery::configure(&mut context), NativeIssue::GalleryNavigation);
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.manage(initial_panel);
    #[cfg(target_os = "windows")]
    let builder = builder.manage(picker::FocusChanges::default());
    let builder = builder
        // Must be registered first: a second launch brings the running picker forward.
        // A login entry starting a second copy leaves the running picker alone.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if !args.iter().skip(1).any(|arg| arg == login_item::AT_LOGIN_ARG) {
                show_picker(app);
            }
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
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
        .manage(SearchRequests::default())
        .manage(LibraryWatch::default())
        .manage(setup::Offered::default())
        .manage(DialogOpen::default())
        .manage(Pinned::default())
        .manage(notice::Notice::default())
        .manage(picker::FullScreenGuard::default())
        .manage(Mutex::new(ShortcutStatus::default()))
        .manage(updater::Updates::default())
        .invoke_handler(bindings::commands().invoke_handler())
        .on_window_event(|window, event| {
            // Only the picker hides on blur or close; the Copied notice never has focus.
            if window.label() != "main" {
                return;
            }
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
                WindowEvent::Focused(true) => {
                    #[cfg(target_os = "windows")]
                    window.state::<picker::FocusChanges>().record();
                    library::retry_zotero_on_focus(window.app_handle());
                }
                // Like other launchers, the picker hides when it loses focus, but
                // not to its own file dialog.
                WindowEvent::Focused(false) => {
                    let dialog_open = window.state::<DialogOpen>().is_open();
                    let pinned = window.state::<Pinned>().0.load(Ordering::SeqCst);
                    #[cfg(target_os = "macos")]
                    let startup_allows_blur = panel::allows_automatic_blur(window.app_handle());
                    #[cfg(not(target_os = "macos"))]
                    let startup_allows_blur = true;
                    #[cfg(target_os = "windows")]
                    let blur = window.state::<picker::FocusChanges>().record();
                    if startup_allows_blur && hides_on_blur(!cfg!(debug_assertions), dialog_open, pinned) {
                        #[cfg(target_os = "windows")]
                        picker::dismiss_unless_refocused(window.app_handle(), blur);
                        #[cfg(not(target_os = "windows"))]
                        dismiss_picker(window.app_handle());
                    }
                }
                _ => {}
            }
        })
        .setup(|app| {
            #[cfg(target_os = "macos")]
            panel::configure(app.handle())?;
            // macOS says in its open event, readable only now, whether it
            // opened Ibis as a login item.
            #[cfg(target_os = "macos")]
            if login_item::launched_as_macos_login_item() {
                panel::cancel_initial_presentation(app.handle());
            }
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
                // The window option alone does not reach GTK's first mapping.
                #[cfg(not(target_os = "macos"))]
                report(window.set_skip_taskbar(true), NativeIssue::SkipTaskbar);
            }
            let location = identity::settings_location(&app.path().app_config_dir()?, &app.config().identifier);
            let settings_path = location.path;
            let loaded = settings::load(&settings_path);
            app.manage(SettingsFile::new(settings_path.clone()));
            if let Some(problem) = &loaded.problem {
                application_message(preferences::settings_problem(problem));
            }
            if let Ok(mut library) = app.state::<Mutex<Library>>().lock() {
                library.settings_notice = preferences::settings_notice(&settings_path, &loaded);
                if location.using_legacy {
                    library.settings_notice = Some(match library.settings_notice.take() {
                        Some(problem) => format!("{} {problem}", identity::MIGRATION_NOTICE),
                        None => identity::MIGRATION_NOTICE.to_string(),
                    });
                }
            }
            let (shortcut, label, setting_error) = configured_shortcut(loaded.settings.shortcut.as_deref());
            let registration_error = register_shortcut(app.handle(), shortcut).err().map(str::to_string);
            let status = ShortcutStatus {
                shortcut: label,
                registered: registration_error.is_none(),
                error: registration_error.or(setting_error),
            };
            if let Some(error) = &status.error {
                report(Err::<(), _>(error), NativeIssue::Shortcut);
            }
            if let Ok(mut current) = app.state::<Mutex<ShortcutStatus>>().lock() {
                *current = status;
            }
            let login_items = login_item::for_app(app.handle());
            login_items.refresh();
            app.manage(login_items);
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
                Ok(Err(error)) => report(Err::<(), _>(error), NativeIssue::TrayCreation),
                Err(_) => application_message("the menu bar item is not available on this system"),
            }
            // After the tray, so the menu item shows the saved choice.
            updater::start(app.handle(), loaded.settings.update_checks, loaded.problem.is_none());
            // The library from settings; IBIS_LIBRARY overrides it for development.
            let library = std::env::var_os("IBIS_LIBRARY")
                .map(|path| Source::File(PathBuf::from(path)))
                .or_else(|| loaded.settings.library.clone().map(Source::from));
            if let Some(source) = library {
                // Opened in the background so the window appears at once.
                start_open(app.handle(), source, false);
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
            report(Err::<(), _>(err), NativeIssue::Startup);
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
