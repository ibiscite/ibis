//! macOS launcher panel. All AppKit calls run on the main thread.

use tauri::{AppHandle, Manager};
use tauri_nspanel::{CollectionBehavior, ManagerExt, PanelLevel, WebviewWindowExt, tauri_panel};

use crate::diagnostics::{NativeIssue, checked, report};
use crate::panel_startup::InitialPresentation;

pub(crate) struct StartupPresentation(std::sync::Mutex<InitialPresentation>);

/// Preserve user-visible intent while preventing a visible NSWindow-to-NSPanel
/// conversion before AppKit has settled its launch and activation changes.
pub(crate) fn prepare_startup(context: &mut tauri::Context<tauri::Wry>) -> StartupPresentation {
    let intent = context
        .config_mut()
        .app
        .windows
        .iter_mut()
        .find(|window| window.label == "main");
    let startup = if let Some(window) = intent {
        let startup = InitialPresentation::new(window.visible, window.focus);
        window.visible = false;
        window.focus = false;
        startup
    } else {
        // configure() already rejects a missing main window; do not invent one.
        InitialPresentation::new(false, false)
    };
    StartupPresentation(std::sync::Mutex::new(startup))
}

pub(crate) fn cancel_initial_presentation(app: &AppHandle) {
    report(update_startup(app, InitialPresentation::cancel), NativeIssue::Focus);
}

pub(crate) fn allows_automatic_blur(app: &AppHandle) -> bool {
    checked(
        update_startup(app, |startup| startup.allows_automatic_blur()),
        NativeIssue::Focus,
    )
    .unwrap_or(true)
}

fn update_startup<T>(app: &AppHandle, change: impl FnOnce(&mut InitialPresentation) -> T) -> Result<T, ()> {
    let state = app.try_state::<StartupPresentation>().ok_or(())?;
    let mut startup = state.0.lock().map_err(|_| ())?;
    Ok(change(&mut startup))
}

pub(crate) fn page_loaded(app: &AppHandle) {
    startup_readiness(app, false);
}

fn startup_readiness(app: &AppHandle, panel_ready: bool) {
    let handle = app.clone();
    report(
        app.run_on_main_thread(move || {
            let intent = checked(
                update_startup(&handle, |startup| {
                    if panel_ready {
                        startup.panel_ready()
                    } else {
                        startup.page_loaded()
                    }
                }),
                NativeIssue::Focus,
            )
            .flatten();
            // Readiness is consumed on the main thread immediately before
            // presentation. An earlier explicit show/dismiss cancels it even
            // if this readiness callback had already been queued.
            if let Some(focused) = intent {
                let Some(panel) = checked(handle.get_webview_panel("main"), NativeIssue::Window) else {
                    return;
                };
                if focused {
                    panel.show_and_make_key();
                    if let Some(window) = checked(handle.get_webview_window("main").ok_or(()), NativeIssue::Window) {
                        focus_webview(&window);
                    }
                } else {
                    panel.order_front_regardless();
                }
                #[cfg(debug_assertions)]
                eprintln!(
                    "ibis: panel initial presented focused={} visible={} key={} first_responder={} outer_responder={}",
                    focused,
                    panel.as_panel().isVisible(),
                    panel.as_panel().isKeyWindow(),
                    panel.as_panel().firstResponder().is_some(),
                    outer_is_responder(panel.as_panel())
                );
            }
        }),
        NativeIssue::Focus,
    );
}

tauri_panel! {
    panel!(PickerPanel {
        config: {
            can_become_key_window: true,
            can_become_main_window: false,
            is_floating_panel: true
        }
    })
}

/// Called by Tauri setup on its main thread. Fail startup rather than silently
/// leaving a normal window that cannot reach the user's full-screen document.
pub(crate) fn configure(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    if tauri_nspanel::objc2::MainThreadMarker::new().is_none() {
        return Err("the launcher panel must be configured on the main thread".into());
    }
    app.plugin(tauri_nspanel::init())
        .map_err(|_| "the launcher panel plugin could not start")?;
    let window = app
        .get_webview_window("main")
        .ok_or("the picker window is unavailable")?;
    let panel = window
        .to_panel::<PickerPanel>()
        .map_err(|_| "the picker could not become a launcher panel")?;
    panel
        .set_style_mask(
            panel.as_panel().styleMask() | tauri_nspanel::objc2_app_kit::NSWindowStyleMask::NonactivatingPanel,
        )
        .map_err(|_| "the launcher panel could not use non-activating behavior")?;
    panel.set_collection_behavior(
        CollectionBehavior::new()
            .can_join_all_spaces()
            .full_screen_auxiliary()
            .value(),
    );
    panel.set_level(PanelLevel::Floating.value());
    panel.set_hides_on_deactivate(false);
    panel.set_works_when_modal(true);
    app.set_activation_policy(tauri::ActivationPolicy::Accessory)
        .map_err(|_| "the launcher activation policy could not be set")?;
    // Cosmetic clipping must be installed before the first presentation.
    // Failure retains the working rectangular picker with a fixed diagnostic.
    crate::native_corners::configure(panel.as_panel());
    // Conversion and activation changes run while the native window is hidden.
    // Its original visibility/focus intent is presented once both this setup
    // and the first actual page navigation have finished.
    #[cfg(debug_assertions)]
    eprintln!(
        "ibis: panel initial before visible={} key={} first_responder={} outer_responder={}",
        panel.as_panel().isVisible(),
        panel.as_panel().isKeyWindow(),
        panel.as_panel().firstResponder().is_some(),
        outer_is_responder(panel.as_panel())
    );
    startup_readiness(app, true);
    Ok(())
}

fn focus_webview(window: &tauri::WebviewWindow) {
    // Focus the actual WKWebView without activating Ibis or switching Spaces.
    let view: &tauri::Webview<tauri::Wry> = window.as_ref();
    report(view.set_focus(), NativeIssue::Focus);
}

#[cfg(debug_assertions)]
fn outer_is_responder(native: &tauri_nspanel::objc2_app_kit::NSPanel) -> bool {
    match (native.firstResponder(), native.contentView()) {
        (Some(responder), Some(content)) => std::ptr::eq(
            std::ptr::from_ref(&*responder).cast::<()>(),
            std::ptr::from_ref(&*content).cast::<()>(),
        ),
        _ => false,
    }
}

#[cfg(debug_assertions)]
pub(crate) fn observe_window_focus(app: &AppHandle, focused: bool) {
    observe_lifecycle(app, Some(focused));
}

#[cfg(debug_assertions)]
pub(crate) fn observe_page_load_finished(app: &AppHandle) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static OBSERVED: AtomicBool = AtomicBool::new(false);
    if !OBSERVED.swap(true, Ordering::Relaxed) {
        observe_lifecycle(app, None);
    }
}

/// Observe late native changes without adjusting focus or exposing page data.
#[cfg(debug_assertions)]
fn observe_lifecycle(app: &AppHandle, focused: Option<bool>) {
    let handle = app.clone();
    report(
        app.run_on_main_thread(move || {
            if tauri_nspanel::objc2::MainThreadMarker::new().is_none() {
                return;
            }
            if let Ok(panel) = handle.get_webview_panel("main") {
                let native = panel.as_panel();
                match focused {
                    Some(focused) => eprintln!(
                        "ibis: panel focus focused={} visible={} key={} first_responder={} outer_responder={}",
                        focused,
                        native.isVisible(),
                        native.isKeyWindow(),
                        native.firstResponder().is_some(),
                        outer_is_responder(native)
                    ),
                    None => eprintln!(
                        "ibis: panel page_loaded visible={} key={} first_responder={} outer_responder={}",
                        native.isVisible(),
                        native.isKeyWindow(),
                        native.firstResponder().is_some(),
                        outer_is_responder(native)
                    ),
                }
            } else {
                eprintln!("ibis: panel lifecycle available=false");
            }
        }),
        NativeIssue::Focus,
    );
}

/// A panel takes keyboard focus without activating Ibis or switching Spaces.
/// Commands and global shortcut callbacks can arrive from worker threads.
pub(crate) fn show(app: &AppHandle) {
    cancel_initial_presentation(app);
    let handle = app.clone();
    report(
        app.run_on_main_thread(move || match handle.get_webview_panel("main") {
            Ok(panel) => {
                if let Some(main_thread) = tauri_nspanel::objc2::MainThreadMarker::new() {
                    // Dismissal may have hidden Ibis after an activating file dialog.
                    tauri_nspanel::objc2_app_kit::NSApplication::sharedApplication(main_thread)
                        .unhideWithoutActivation();
                    panel.show_and_make_key();
                    // The panel helper makes its outer content NSView the
                    // responder. Route hardware keys to the WKWebView itself
                    // without activating Ibis or changing the user's Space.
                    if let Some(window) = handle.get_webview_window("main") {
                        focus_webview(&window);
                    } else {
                        report(Err::<(), ()>(()), NativeIssue::Window);
                    }
                    // Observe actual AppKit keyboard ownership independently
                    // of the WebView's accessible/DOM focused search field.
                    #[cfg(debug_assertions)]
                    eprintln!(
                        "ibis: panel show key={} first_responder={}",
                        panel.as_panel().isKeyWindow(),
                        panel.as_panel().firstResponder().is_some()
                    );
                } else {
                    report(Err::<(), ()>(()), NativeIssue::Show);
                }
            }
            Err(_) => report(Err::<(), ()>(()), NativeIssue::Show),
        }),
        NativeIssue::Show,
    );
}

/// Native file dialogs belong to Ibis and need an active application. This is
/// queued before the dialog plugin queues its own AppKit work. DialogOpen
/// already guards the picker against the resulting focus changes.
pub(crate) fn activate_for_dialog(app: &AppHandle) {
    report(
        app.run_on_main_thread(|| {
            if let Some(main_thread) = tauri_nspanel::objc2::MainThreadMarker::new() {
                // Support Tauri's minimum macOS as well as current AppKit;
                // NSApplication.activate is only available on macOS 14+.
                #[allow(deprecated)]
                tauri_nspanel::objc2_app_kit::NSApplication::sharedApplication(main_thread)
                    .activateIgnoringOtherApps(true);
            } else {
                report(Err::<(), ()>(()), NativeIssue::Focus);
            }
        }),
        NativeIssue::Focus,
    );
}
