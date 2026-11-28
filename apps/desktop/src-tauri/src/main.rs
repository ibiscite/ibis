//! Ibis desktop app: a thin Tauri shell over `ibis-core`.
//!
//! The window is the picker. All behaviour that can be tested without a
//! window lives in `ibis-core`; these modules only translate between the web
//! interface and the core.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent_apps;
mod app_context;
mod bindings;
mod cli_install;
mod copy;
mod delights;
mod delivery;
#[cfg(any(target_os = "linux", test))]
mod desktop_theme;
mod destination;
mod diagnostics;
mod editor_endpoint;
mod errors;
mod feedback;
mod format_settings;
mod instance;
mod language;
mod library;
mod login_item;
#[cfg(target_os = "macos")]
mod macos_notifications;
#[cfg(target_os = "macos")]
mod menu_bar;
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
// The picker window lifecycle core; the platform adapters move onto it next.
#[cfg_attr(not(test), allow(dead_code))]
mod picker_window;
mod placement;
mod preferences;
mod project;
mod proposals;
mod row_context;
mod session_references;
mod settings_backup;
#[cfg(test)]
#[path = "../../../../crates/ibis-engine/tests/support/settings.rs"]
mod settings_fixture;
mod settings_window;
mod setup;
mod shortcut_setting;
mod sizing;
mod store;
mod system_open;
mod tray;
mod updater;

use ibis_engine::{Engine, library::Owner};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use ibis_engine::settings;
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

use crate::diagnostics::{NativeIssue, application_message, checked, report};
use crate::library::{SearchRequests, Source, start_open};
use crate::picker::{
    ChooserOpen, Pinned, ShortcutStatus, dismiss_picker, hides_on_blur, start_shortcut, toggle_picker,
};
use crate::preferences::SettingsFile;
use crate::tray::create_tray;

fn main() {
    // Answer `ibis toggle` first, whether this copy opens the picker or hands
    // the toggle to the one already running.
    let args: Vec<String> = std::env::args_os()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    if let Some(receipt) = ibis_os::toggle::receipt_path(&args) {
        let _ = instance::sign_receipt(receipt);
    }
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
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.manage(initial_panel);
    #[cfg(target_os = "windows")]
    let builder = builder.manage(picker::FocusChanges::default());
    let builder = instance::install(builder)
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
        .manage(library::attachment_text::AttachmentText::default())
        .manage(SearchRequests::default())
        .manage(tray::SearchRequest::default())
        .manage(setup::Offered::default())
        .manage(ChooserOpen::default())
        .manage(settings_backup::PendingRestore::default())
        .manage(row_context::RowContext::default())
        .manage(editor_endpoint::Pending::default())
        .manage(editor_endpoint::Endpoint::default())
        .manage(session_references::SessionReferences::default())
        .on_menu_event(row_context::selected)
        .manage(Pinned::default())
        .manage(picker::Moving::default())
        .manage(sizing::Sizing::default())
        .manage(picker_generation)
        .manage(picker::Presentation::default())
        .manage(picker::Registered::default())
        .manage(shortcut_setting::Changing::default())
        .manage(notice::Notice::default())
        .manage(paste::LastTarget::default())
        .manage(picker::FullScreenGuard::default())
        .manage(Mutex::new(ShortcutStatus::default()))
        .manage(updater::Updates::default())
        .invoke_handler(bindings::handler())
        .on_webview_event(|webview, event| {
            if webview.label() != "main" {
                return;
            }
            match event {
                tauri::WebviewEvent::DragDrop(tauri::DragDropEvent::Enter { paths, .. }) => {
                    let accepted = paths.len() == 1 && library::accepts_library_file(&paths[0]);
                    let _ = webview.emit("library:file-drag", accepted);
                }
                tauri::WebviewEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) => {
                    let _ = webview.emit("library:file-drag", false);
                    if paths.len() == 1 && library::accepts_library_file(&paths[0]) {
                        let _ = webview.emit("library:file-dropped", ());
                        library::start_open(webview.app_handle(), library::Source::File(paths[0].clone()), true);
                    } else if !paths.is_empty() {
                        let _ = webview.emit("library:file-refused", ());
                    }
                }
                tauri::WebviewEvent::DragDrop(tauri::DragDropEvent::Leave) => {
                    let _ = webview.emit("library:file-drag", false);
                }
                _ => {}
            }
        })
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
                    // Focus is back after any move grab; nothing is left to check.
                    window.state::<picker::Moving>().end();
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
                    // The compositor's move or resize grab; checked when the move ends.
                    #[cfg(target_os = "linux")]
                    let moving = window
                        .state::<picker::Moving>()
                        .holds(&window.state::<picker::PickerGeneration>());
                    #[cfg(not(target_os = "linux"))]
                    let moving = false;
                    if startup_allows_blur && !moving && hides_on_blur(!cfg!(debug_assertions), chooser_open, pinned) {
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
            // First: no copy can start toward a paste before Paste-off revokes it.
            paste::revoke_when_paste_changes(app.handle());
            let startup = app.state::<Startup>();
            let (settings_path, loaded) = (&startup.settings_path, &startup.loaded);
            let interface = loaded.interface().unwrap_or_default();
            // Before any window or menu speaks: the saved language, else the
            // system's. A new choice is spoken the next time Ibis opens.
            ibis_engine::catalogue::use_language(language::resolve(
                interface
                    .interface_language
                    .and_then(settings::InterfaceLanguage::chosen),
            ));
            let cites = loaded.cites().ok();
            let remember = cites.is_some_and(|settings| settings.portable.recent_citations());
            session_references::manage(app.handle(), settings_path, remember);
            app.state::<sizing::Sizing>().load(&interface);
            if !sizing::start(app.handle()) {
                return Err("the picker settings worker could not start".into());
            }
            #[cfg(target_os = "macos")]
            panel::configure(app.handle())?;
            #[cfg(target_os = "linux")]
            desktop_theme::follow(app.handle());
            // Complete size and placement while the native window is hidden,
            // before either the panel readiness gate or a window show.
            if let Some(window) = checked(app.get_webview_window("main").ok_or(()), NativeIssue::Window) {
                // A session with no display yet (remote, headless) can still run:
                // each opening prepares the frame again.
                let no_display =
                    checked(window.available_monitors(), NativeIssue::Monitor).is_some_and(|all| all.is_empty());
                if !sizing::prepare(app.handle(), &window, true, false) && !no_display {
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
            cli_install::keep_at_launch(app.handle());
            if let Some(problem) = &loaded.problem {
                application_message(preferences::settings_problem(problem));
            }
            #[cfg(target_os = "macos")]
            picker::follow_layout(app.handle());
            // Off the main thread: the Store's startup task answers through Windows.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn_blocking(move || handle.state::<login_item::LoginItems>().refresh());
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
            tray::show_in_menu_bar(app.handle(), interface.menu_bar(), 0);
            start_shortcut(app.handle(), interface.shortcut.as_deref());
            // After the tray, so the menu item shows the saved choice.
            updater::start(app.handle(), interface.update_checks, loaded.problem.is_none());
            // The library from settings; IBIS_LIBRARY overrides it for development.
            project::watch_openings(app.handle());
            editor_endpoint::start(
                app.handle(),
                cites.and_then(|settings| settings.device.editor_endpoint.clone()),
            );
            let library = std::env::var_os("IBIS_LIBRARY")
                .map(|path| Source::File(PathBuf::from(path)))
                .or_else(|| {
                    cites
                        .and_then(|settings| settings.portable.library.clone())
                        .map(Source::from)
                });
            if let Some(source) = library {
                // Opened in the background so the window appears at once.
                start_open(app.handle(), source, false);
            }
            // With no menu bar item, opening Ibis opens Settings, as reopening does.
            #[cfg(target_os = "macos")]
            if interface.menu_bar == Some(false) && !login_item::launched_at_login(std::env::args_os()) {
                panel::cancel_initial_presentation(app.handle());
                settings_window::open(app.handle(), None);
            }
            #[cfg(target_os = "macos")]
            panel::frame_ready(app.handle());
            #[cfg(not(target_os = "macos"))]
            if initial_picker.0 {
                picker::show_picker_focused(app.handle(), initial_picker.1);
            }
            // Install after startup effects: a newer file edit must never be
            // undone by an older initial tray, shortcut or recording value.
            let watching = app.state::<SettingsFile>().watch(app.handle().clone());
            app.manage(std::sync::Mutex::new(watching));
            // Last: a launch handed over during startup is answered now.
            instance::ready(app.handle());
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
    // Every handler's state exists before the event loop can deliver anything.
    let app = builder.build(context).and_then(|app| {
        create_state(app.handle()).map_err(|error| tauri::Error::Setup(error.into()))?;
        Ok(app)
    });
    match app {
        Ok(app) => app.run(|app, event| {
            match &event {
                tauri::RunEvent::MainEventsCleared => sizing::settled(app),
                tauri::RunEvent::ExitRequested { .. } => {
                    sizing::flush(app, true);
                    let recents = app.state::<session_references::SessionReferences>();
                    let _ = recents.flush_timeout(std::time::Duration::from_secs(1));
                }
                _ => {}
            }
            // macOS: a repeated OS open request brings back the hidden picker.
            #[cfg(target_os = "macos")]
            // A Dock click while Settings is open brings Settings forward.
            if let tauri::RunEvent::Reopen { .. } = event {
                instance::reopen(app);
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

/// What setup reads from the settings as they were at launch.
struct Startup {
    settings_path: PathBuf,
    loaded: settings::Loaded,
}

/// Create every piece of state that depends on the settings, after the plugins
/// and before the event loop runs. The loop delivers every handler's input: a
/// second launch, a window, tray or menu event, a shortcut or a command. So no
/// handler can find state missing, and setup itself only starts effects.
fn create_state(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // Before the first library, so its id lasts and matches `ibis`.
    ibis_engine::identity::use_saved_key(true)?;
    let settings_path = app.path().app_config_dir()?.join("settings.json");
    let loaded = settings::load(&settings_path);
    let cites = loaded.cites().ok();
    library::engine_library::warm_formatter(cites.and_then(|settings| settings.portable.output.clone()));
    let settings_file = SettingsFile::new(settings_path.clone());
    app.manage(settings_file.clone());
    app_context::manage(app, &settings_file);
    let handle = app.clone();
    let notices = settings_file.clone();
    app.manage(Owner::new(
        Arc::new(settings_file),
        || Ok(Engine::new()?.with_formatting(library::engine_library::FORMATTING.clone())),
        move |mut status| {
            for line in &status.log {
                application_message(line);
            }
            let reveal = status.reveal;
            status.settings_notice = notices.notice();
            report(handle.emit("library:changed", status), NativeIssue::LibraryEvent);
            let native = handle.clone();
            // Tauri runs a main-thread call inline when made on the main
            // thread. Defer native work so it can read the owner after publication.
            tauri::async_runtime::spawn(async move {
                let main = native.clone();
                report(
                    native.run_on_main_thread(move || {
                        if reveal {
                            picker::reveal_picker_quietly(&main);
                        }
                        tray::refresh_library(&main);
                    }),
                    NativeIssue::TrayLibrary,
                );
            });
        },
    ));
    destination::manage(app, &settings_path);
    app.manage(login_item::for_app(app));
    native_feel::manage(app);
    app.manage(Startup { settings_path, loaded });
    Ok(())
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

#[cfg(test)]
mod parser_tests {
    /// Documents and PDFs are parsed only in the command line's children
    /// (decisions 0055 and 0060), so the app's own build turns on none of the
    /// readers' parser features. Asked of Cargo for the app alone: a test
    /// binary built with the whole workspace has them unified in.
    #[test]
    fn the_app_is_built_without_the_document_and_pdf_parsers() {
        let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let output = std::process::Command::new(cargo)
            .args([
                "tree",
                "--locked",
                "--offline",
                "-e",
                "normal,features",
                "--prefix",
                "none",
            ])
            .args(["--manifest-path", manifest])
            .output()
            .unwrap();
        let tree = String::from_utf8(output.stdout).unwrap();
        assert!(
            output.status.success() && tree.contains("ibis-sources v"),
            "cargo tree: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        for line in tree.lines() {
            for feature in ["documents", "endnote", "pdf-reader"] {
                let turned_on = format!("feature \"{feature}\"");
                assert!(
                    !(line.starts_with("ibis-") && line.contains(&turned_on)),
                    "the app builds a parser: {line}"
                );
            }
            assert!(!line.starts_with("lopdf "), "the app builds the PDF parser: {line}");
        }
    }
}

#[cfg(test)]
mod test_citations;
