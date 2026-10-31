//! The Settings window (#776): a window of its own that, while open, shows in
//! the Dock and the app switcher like any Mac settings window.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::diagnostics::{NativeIssue, checked, report};

pub(crate) const LABEL: &str = "settings";
const TITLE: &str = "Ibis Settings";
/// The design canvas's size (docs/design.md).
const SIZE: (f64, f64) = (720.0, 560.0);
const MIN_SIZE: (f64, f64) = (480.0, 400.0);

/// Where Settings opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Page {
    General,
    /// General, scrolled to pasting and its permission.
    Paste,
    Look,
    Libraries,
    Advanced,
    Shortcuts,
    Citations,
    About,
}

impl Page {
    fn name(self) -> &'static str {
        match self {
            Page::General => "general",
            Page::Paste => "paste",
            Page::Look => "look",
            Page::Libraries => "libraries",
            Page::Advanced => "advanced",
            Page::Shortcuts => "shortcuts",
            Page::Citations => "citations",
            Page::About => "about",
        }
    }
}

/// Open Settings, or bring it forward, on `page` (General by default). Async so
/// the window is built from the event loop: a sync command runs inside the
/// WebView2 callback, where building a second webview deadlocks on Windows.
#[tauri::command]
#[specta::specta]
pub(crate) async fn open_settings(app: AppHandle, page: Option<Page>) {
    open(&app, page);
}

/// Close the paste guide after both grants; a fresh external activation opens
/// the picker again. Granting permission never captures a target or retries Enter.
#[tauri::command]
#[specta::specta]
pub(crate) async fn finish_paste_guide(app: AppHandle) -> Result<bool, String> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let result = (|| {
            let Some(window) = handle.get_webview_window(LABEL) else {
                return Ok(false);
            };
            finish_guide_with(
                crate::paste::permission::paste_permission().granted,
                window.is_focused().unwrap_or(false),
                || {
                    window.hide()?;
                    closed(&handle);
                    Ok::<_, tauri::Error>(())
                },
            )
            .map_err(|_| "The permission guide could not close.".to_string())
        })();
        let _ = sender.send(result);
    })
    .map_err(|_| "The permission guide could not close.".to_string())?;
    tauri::async_runtime::spawn_blocking(move || receiver.recv())
        .await
        .ok()
        .and_then(Result::ok)
        .ok_or_else(|| "The permission guide could not close.".to_string())?
}

fn finish_guide_with<E>(granted: bool, focused: bool, close: impl FnOnce() -> Result<(), E>) -> Result<bool, E> {
    if !granted || !focused {
        return Ok(false);
    }
    close()?;
    // Settings and the nonactivating picker can keep Ibis key after a grant.
    // Do not infer a new external target here: the next deliberate summon
    // captures it and retires old proofs before a fresh Enter can paste.
    Ok(true)
}

pub(crate) fn open(app: &AppHandle, page: Option<Page>) {
    let handle = app.clone();
    report(
        app.run_on_main_thread(move || present(&handle, page)),
        NativeIssue::Window,
    );
}

fn present(app: &AppHandle, page: Option<Page>) {
    #[cfg(target_os = "macos")]
    {
        report(
            app.set_activation_policy(tauri::ActivationPolicy::Regular),
            NativeIssue::Window,
        );
        // A dismissed picker hid the whole app; bring it back with Settings.
        report(app.show(), NativeIssue::Show);
    }
    if let Some(window) = app.get_webview_window(LABEL) {
        present_existing(&window, page);
        return;
    }
    let url = format!("settings.html?page={}", page.unwrap_or(Page::General).name());
    let mut builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App(url.into()))
        .title(TITLE)
        .inner_size(SIZE.0, SIZE.1)
        .min_inner_size(MIN_SIZE.0, MIN_SIZE.1)
        .center()
        .maximizable(false)
        .focused(true);
    // The sidebar runs under the traffic lights, as in System Settings.
    #[cfg(target_os = "macos")]
    {
        builder = builder
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true);
    }
    // WebView2 refuses a second webview whose arguments differ from the
    // picker's, and they keep it offline (#201), so reuse them exactly.
    if let Some(args) = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == "main")
        .and_then(|window| window.additional_browser_args.clone())
    {
        builder = builder.additional_browser_args(&args);
    }
    match checked(builder.build(), NativeIssue::Window) {
        Some(window) => report(window.set_focus(), NativeIssue::Focus),
        None => closed(app),
    }
}

/// Every presentation has a page; ordinary Settings opens on General at the top.
fn present_existing<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>, page: Option<Page>) {
    report(
        window.emit_to(LABEL, "settings:page", page.unwrap_or(Page::General)),
        NativeIssue::Window,
    );
    report(window.unminimize(), NativeIssue::Unminimize);
    report(window.show(), NativeIssue::Show);
    report(window.set_focus(), NativeIssue::Focus);
}

/// Settings is on screen, so hiding the whole app would hide it too.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn is_open(app: &AppHandle) -> bool {
    app.get_webview_window(LABEL)
        .is_some_and(|window| window.is_visible().unwrap_or(false))
}

/// Once Settings closes, Ibis leaves the Dock again and lives in the menu bar,
/// handing focus back to the app underneath unless the picker is up.
pub(crate) fn closed(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        report(
            app.set_activation_policy(tauri::ActivationPolicy::Accessory),
            NativeIssue::Window,
        );
        let picker_shown = app
            .get_webview_window("main")
            .is_some_and(|window| window.is_visible().unwrap_or(false));
        if !picker_shown {
            report(app.hide(), NativeIssue::HideApplication);
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returning_requires_both_grants_and_focus_then_waits_for_a_fresh_external_activation() {
        use std::cell::RefCell;
        for (granted, focused) in [(false, false), (false, true), (true, false), (true, true)] {
            let calls = RefCell::new(Vec::new());
            let result = finish_guide_with(granted, focused, || {
                calls.borrow_mut().push("close");
                Ok::<_, ()>(())
            });
            assert_eq!(result, Ok(granted && focused));
            assert_eq!(*calls.borrow(), if granted && focused { vec!["close"] } else { vec![] });
        }
        assert_eq!(
            finish_guide_with(true, true, || Err("close failed")),
            Err("close failed")
        );
    }

    #[test]
    fn presenting_existing_settings_resets_general_without_a_request_and_honours_explicit_requests() {
        use std::sync::mpsc;
        use std::time::Duration;
        use tauri::Listener;

        let app = tauri::test::mock_builder()
            .build(tauri::generate_context!(test = true))
            .unwrap_or_else(|e| panic!("cannot build settings fixture: {e}"));
        let window = WebviewWindowBuilder::new(&app, LABEL, Default::default())
            .build()
            .unwrap_or_else(|e| panic!("cannot build settings window: {e}"));
        let (sent, events) = mpsc::channel();
        window.listen("settings:page", move |event| {
            let _ = sent.send(event.payload().to_string());
        });

        for (requested, expected) in [
            (None, "general"),
            (Some(Page::About), "about"),
            (None, "general"),
            (Some(Page::Paste), "paste"),
            (Some(Page::Shortcuts), "shortcuts"),
            (Some(Page::Citations), "citations"),
            (Some(Page::General), "general"),
        ] {
            present_existing(&window, requested);
            let payload = events
                .recv_timeout(Duration::from_secs(3))
                .unwrap_or_else(|e| panic!("settings page event missing: {e}"));
            assert_eq!(payload, serde_json::json!(expected).to_string());
        }
    }

    #[test]
    fn each_page_has_the_name_the_page_reads() {
        for (page, name) in [
            (Page::General, "general"),
            (Page::Paste, "paste"),
            (Page::Look, "look"),
            (Page::Libraries, "libraries"),
            (Page::Advanced, "advanced"),
            (Page::Shortcuts, "shortcuts"),
            (Page::Citations, "citations"),
            (Page::About, "about"),
        ] {
            assert_eq!(page.name(), name);
            assert_eq!(serde_json::to_value(page).ok(), Some(serde_json::json!(name)));
        }
    }
}
