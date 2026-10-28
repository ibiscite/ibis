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
    Shortcuts,
    Citations,
    About,
}

impl Page {
    fn name(self) -> &'static str {
        match self {
            Page::General => "general",
            Page::Paste => "paste",
            Page::Shortcuts => "shortcuts",
            Page::Citations => "citations",
            Page::About => "about",
        }
    }
}

/// Open Settings, or bring it forward, on `page` when one is named. Async so
/// the window is built from the event loop: a sync command runs inside the
/// WebView2 callback, where building a second webview deadlocks on Windows.
#[tauri::command]
#[specta::specta]
pub(crate) async fn open_settings(app: AppHandle, page: Option<Page>) {
    open(&app, page);
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
        if let Some(page) = page {
            report(window.emit_to(LABEL, "settings:page", page), NativeIssue::Window);
        }
        report(window.unminimize(), NativeIssue::Unminimize);
        report(window.show(), NativeIssue::Show);
        report(window.set_focus(), NativeIssue::Focus);
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
    fn each_page_has_the_name_the_page_reads() {
        for (page, name) in [
            (Page::General, "general"),
            (Page::Paste, "paste"),
            (Page::Shortcuts, "shortcuts"),
            (Page::Citations, "citations"),
            (Page::About, "about"),
        ] {
            assert_eq!(page.name(), name);
            assert_eq!(serde_json::to_value(page).ok(), Some(serde_json::json!(name)));
        }
    }
}
