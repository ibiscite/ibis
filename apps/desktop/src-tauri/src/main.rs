//! Ibis desktop app: a thin Tauri shell over `ibis-core`.
//!
//! The window is the picker. All behaviour that can be tested without a
//! window lives in `ibis-core`; these modules only translate between the web
//! interface and the core.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bindings;
mod copy;
mod destination;
mod diagnostics;
mod feedback;
mod library;
mod login_item;
#[cfg(target_os = "macos")]
mod macos_notifications;
#[cfg(target_os = "macos")]
mod native_corners;
mod native_feel;
mod notice;
#[cfg(target_os = "macos")]
mod panel;
#[cfg(any(target_os = "macos", test))]
mod panel_startup;
mod paste;
mod picker;
mod placement;
mod preferences;
mod row_context;
mod settings_window;
mod setup;
mod shortcut_setting;
mod sizing;
mod tray;
mod updater;

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use ibis_core::settings;
use tauri::{Manager, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

use crate::diagnostics::{NativeIssue, application_message, checked, report};
use crate::library::{Library, LoadRequests, PreviewRequests, SearchRequests, Source, start_open};
use crate::picker::{
    ChooserOpen, Pinned, ShortcutStatus, dismiss_picker, hides_on_blur, show_picker, start_shortcut, toggle_picker,
};
use crate::preferences::SettingsFile;
use crate::tray::create_tray;

#[derive(Debug, PartialEq, Eq)]
enum InstanceAction {
    Show,
    Hide,
    Ignore,
}

fn is_toggle(args: &[String]) -> bool {
    matches!(args, [_, command] if command == "toggle")
}

/// Only a macOS toggle shares the shortcut's main thread; see the callback.
fn acts_on_main_thread(args: &[String], macos: bool) -> bool {
    macos && is_toggle(args)
}

fn act_on_instance(app: &tauri::AppHandle, args: &[String]) {
    let visible = || {
        app.get_webview_window("main")
            .and_then(|window| checked(window.is_visible(), NativeIssue::Visibility))
    };
    match instance_action(args, visible) {
        InstanceAction::Show => show_picker(app),
        InstanceAction::Hide => dismiss_picker(app),
        InstanceAction::Ignore => {}
    }
}

fn instance_action(args: &[String], visible: impl FnOnce() -> Option<bool>) -> InstanceAction {
    if args.iter().skip(1).any(|arg| arg == login_item::AT_LOGIN_ARG) {
        InstanceAction::Ignore
    } else if is_toggle(args) && visible() == Some(true) {
        InstanceAction::Hide
    } else {
        InstanceAction::Show
    }
}

fn main() {
    let mut context = tauri::generate_context!();
    native_feel::configure(&mut context);
    // Started by the login entry: wait in the background for the shortcut.
    if login_item::launched_at_login(std::env::args_os()) {
        login_item::start_hidden(&mut context);
    }
    #[cfg(target_os = "macos")]
    let initial_panel = panel::prepare_startup(&mut context);
    #[cfg(not(target_os = "macos"))]
    let initial_picker = picker::prepare_startup(&mut context);
    let picker_generation = picker::PickerGeneration::default();
    #[cfg(not(target_os = "macos"))]
    let initial_picker_owner = picker_generation.current();
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.manage(initial_panel);
    #[cfg(target_os = "windows")]
    let builder = builder.manage(picker::FocusChanges::default());
    let builder = builder
        // Must be registered first: a second launch brings the running picker forward.
        // A login entry starting a second copy leaves the running picker alone.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // macOS: the shortcut acts on the main thread, so a toggle decides
            // and acts there too, or the two could interleave and lose one.
            // Elsewhere, make the instance decision in the callback;
            // show_picker serializes native preparation and presentation.
            if acts_on_main_thread(&args, cfg!(target_os = "macos")) {
                let handle = app.clone();
                report(
                    app.run_on_main_thread(move || act_on_instance(&handle, &args)),
                    NativeIssue::PickerEvent,
                );
            } else {
                act_on_instance(app, &args);
            }
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build());
    let builder = if picker::desktop_owned() {
        builder
    } else {
        builder.plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle_picker(app);
                    }
                })
                .build(),
        )
    };
    let builder = builder
        .manage(Mutex::new(Library::default()))
        .manage(LoadRequests::default())
        .manage(SearchRequests::default())
        .manage(PreviewRequests::default())
        .manage(setup::Offered::default())
        .manage(ChooserOpen::default())
        .manage(row_context::RowContext::default())
        .on_menu_event(row_context::selected)
        .manage(Pinned::default())
        .manage(sizing::Sizing::default())
        .manage(picker_generation)
        .manage(picker::Registered::default())
        .manage(shortcut_setting::Changing::default())
        .manage(notice::Notice::default())
        .manage(paste::LastTarget::default())
        .manage(picker::FullScreenGuard::default())
        .manage(Mutex::new(ShortcutStatus::default()))
        .manage(updater::Updates::default())
        .invoke_handler(bindings::commands().invoke_handler())
        .on_window_event(|window, event| {
            if window.label() == settings_window::LABEL {
                if let WindowEvent::Destroyed = event {
                    settings_window::closed(window.app_handle());
                }
                return;
            }
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
                WindowEvent::Resized(_) | WindowEvent::Moved(_) | WindowEvent::ScaleFactorChanged { .. } => {
                    sizing::changed(window.app_handle(), event);
                }
                // Closing the picker hides it; the app keeps running for the shortcut.
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    dismiss_picker(window.app_handle());
                }
                WindowEvent::Focused(true) => {
                    #[cfg(target_os = "windows")]
                    window.state::<picker::FocusChanges>().record();
                    updater::picker_focused(window.app_handle());
                    library::retry_zotero_on_focus(window.app_handle());
                }
                // Like other launchers, the picker hides when it loses focus, but
                // not to its own file dialog.
                WindowEvent::Focused(false) => {
                    let chooser_open = window.state::<ChooserOpen>().is_open();
                    let pinned = window.state::<Pinned>().0.load(Ordering::SeqCst);
                    #[cfg(target_os = "macos")]
                    let startup_allows_blur = panel::allows_automatic_blur(window.app_handle());
                    #[cfg(not(target_os = "macos"))]
                    let startup_allows_blur = true;
                    #[cfg(target_os = "windows")]
                    let blur = window.state::<picker::FocusChanges>().record();
                    if startup_allows_blur && hides_on_blur(!cfg!(debug_assertions), chooser_open, pinned) {
                        #[cfg(target_os = "windows")]
                        picker::dismiss_unless_refocused(
                            window.app_handle(),
                            blur,
                            window.state::<picker::PickerGeneration>().current(),
                        );
                        #[cfg(not(target_os = "windows"))]
                        dismiss_picker(window.app_handle());
                    }
                }
                _ => {}
            }
        })
        .setup(move |app| {
            if let Ok(cache) = app.path().app_cache_dir() {
                // One attempt per launch, independent of loading a library.
                // Cleanup failures never prevent a fresh source read.
                tauri::async_runtime::spawn_blocking(move || {
                    let _ = ibis_zotero_local::closed::remove_obsolete_file_cache(&cache);
                });
            }
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let loaded = settings::load(&settings_path);
            app.manage(SettingsFile::new(settings_path.clone()));
            app.state::<sizing::Sizing>().load(&loaded.settings);
            if !sizing::start(app.handle()) {
                return Err("the picker settings worker could not start".into());
            }
            #[cfg(target_os = "macos")]
            panel::configure(app.handle())?;
            // Complete size and placement while the native window is hidden,
            // before either the panel readiness gate or a window show.
            if let Some(window) = checked(app.get_webview_window("main").ok_or(()), NativeIssue::Window) {
                if !sizing::prepare(app.handle(), &window, true, false) {
                    return Err("the picker frame could not be prepared".into());
                }
                sizing::watch_display(app.handle(), &window);
                #[cfg(not(target_os = "macos"))]
                {
                    report(window.set_skip_taskbar(true), NativeIssue::SkipTaskbar);
                }
            }
            // macOS says in its open event, readable only now, whether it
            // opened Ibis as a login item.
            #[cfg(target_os = "macos")]
            if login_item::launched_as_macos_login_item() {
                panel::cancel_initial_presentation(app.handle());
            }
            native_feel::install(app.handle());
            destination::manage(app.handle(), &settings_path);
            if let Some(problem) = &loaded.problem {
                application_message(preferences::settings_problem(problem));
            }
            if let Ok(mut library) = app.state::<Mutex<Library>>().lock() {
                library.settings_notice = preferences::settings_notice(&settings_path, &loaded);
            }
            #[cfg(target_os = "macos")]
            picker::follow_layout(app.handle());
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
            tray::show_in_menu_bar(app.handle(), loaded.settings.menu_bar.unwrap_or(true), 0);
            start_shortcut(app.handle(), loaded.settings.shortcut.as_deref());
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
            // With no menu bar item, opening Ibis opens Settings, as reopening does.
            #[cfg(target_os = "macos")]
            if loaded.settings.menu_bar == Some(false) && !login_item::launched_at_login(std::env::args_os()) {
                panel::cancel_initial_presentation(app.handle());
                settings_window::open(app.handle(), None);
            }
            #[cfg(target_os = "macos")]
            panel::frame_ready(app.handle());
            #[cfg(not(target_os = "macos"))]
            if initial_picker.0
                && let Some(window) = app.get_webview_window("main")
            {
                picker::present_window(&window, initial_picker.1, initial_picker_owner);
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
            match &event {
                tauri::RunEvent::MainEventsCleared => sizing::settled(app),
                tauri::RunEvent::ExitRequested { .. } => sizing::flush(app, true),
                _ => {}
            }
            // macOS: a repeated OS open request brings back the hidden picker.
            #[cfg(target_os = "macos")]
            // A Dock click while Settings is open brings Settings forward.
            if let tauri::RunEvent::Reopen { .. } = event {
                if settings_window::is_open(app) || tray::hidden() {
                    settings_window::open(app, None);
                } else {
                    show_picker(app);
                }
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
mod toggle_tests {
    use super::*;

    #[test]
    fn only_a_macos_toggle_waits_for_the_main_thread() {
        let toggle: Vec<String> = ["ibis-desktop", "toggle"].map(String::from).to_vec();
        let plain: Vec<String> = ["ibis-desktop"].map(String::from).to_vec();
        let login: Vec<String> = ["ibis-desktop", "--at-login"].map(String::from).to_vec();
        assert!(acts_on_main_thread(&toggle, true));
        for (args, macos) in [(&toggle, false), (&plain, true), (&plain, false), (&login, true)] {
            assert!(!acts_on_main_thread(args, macos));
        }
    }

    #[test]
    fn toggle_hides_a_visible_picker_and_shows_a_hidden_or_missing_one() {
        let args = ["/synthetic/Ibis 文献 app/ibis-desktop".into(), "toggle".into()];
        assert_eq!(instance_action(&args, || Some(true)), InstanceAction::Hide);
        assert_eq!(instance_action(&args, || Some(false)), InstanceAction::Show);
        assert_eq!(instance_action(&args, || None), InstanceAction::Show);
    }

    #[test]
    fn ordinary_launches_show_and_login_launches_leave_the_picker_alone() {
        for args in [
            vec![],
            vec!["toggle"],
            vec!["ibis-desktop"],
            vec!["ibis-desktop", "--toggle"],
            vec!["ibis-desktop", "Toggle"],
            vec!["ibis-desktop", "toggle", "extra"],
            vec!["ibis-desktop", "", "toggle"],
            vec!["ibis-desktop", "切り替え"],
        ] {
            let args: Vec<String> = args.into_iter().map(String::from).collect();
            let action = instance_action(&args, || panic!("no toggle requested"));
            assert_eq!(action, InstanceAction::Show);
        }
        for args in [
            vec!["ibis-desktop", "--at-login"],
            vec!["ibis-desktop", "toggle", "--at-login"],
        ] {
            let args: Vec<String> = args.into_iter().map(String::from).collect();
            let action = instance_action(&args, || panic!("login must stay hidden"));
            assert_eq!(action, InstanceAction::Ignore);
        }
        let huge = ["ibis-desktop".into(), "toggle".repeat(100_000)];
        let action = instance_action(&huge, || panic!("exact command only"));
        assert_eq!(action, InstanceAction::Show);
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
