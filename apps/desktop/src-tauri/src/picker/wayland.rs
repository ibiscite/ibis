//! Select one shortcut owner. Wayland never registers an XWayland grab or saves a trigger.

use std::sync::Mutex;
use tauri::{AppHandle, Manager};

#[cfg(any(target_os = "linux", test))]
use super::DesktopShortcut;
use super::ShortcutStatus;

#[cfg(any(target_os = "linux", test))]
fn is_wayland(session: Option<&str>, display: Option<&str>) -> bool {
    session == Some("wayland") || display.is_some_and(|value| !value.is_empty())
}

pub(crate) fn desktop_owned() -> bool {
    #[cfg(target_os = "linux")]
    return is_wayland(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
    );
    #[cfg(not(target_os = "linux"))]
    false
}

#[cfg(any(target_os = "linux", test))]
fn toggle_binary(executable: Option<&std::path::Path>, nightly: bool) -> &'static str {
    match executable
        .and_then(std::path::Path::file_name)
        .and_then(|name| name.to_str())
    {
        Some("ibis-desktop") => "ibis-desktop",
        Some("ibis-nightly") => "ibis-nightly",
        _ if nightly => "ibis-nightly",
        _ => "ibis-desktop",
    }
}

#[cfg(any(target_os = "linux", test))]
fn fallback(desktop: &str, binary: &str) -> String {
    let command = if binary == "ibis-nightly" {
        "ibis toggle --app ibis-nightly"
    } else {
        "ibis toggle"
    };
    let sway = format!(
        "Sway: add `bindsym --no-repeat Ctrl+Shift+y exec {command}` to ~/.config/sway/config, then run `swaymsg reload`."
    );
    let hyprland = format!(
        "Hyprland 0.55+: add `hl.bind(\"CTRL + SHIFT + Y\", hl.dsp.exec_cmd(\"{command}\"))` to ~/.config/hypr/hyprland.lua. For older hyprland.conf: `bind = CTRL SHIFT, Y, exec, {command}`."
    );
    let desktop = desktop.to_ascii_lowercase();
    let binding = if desktop.split(':').any(|name| name == "sway") {
        sway
    } else if desktop.split(':').any(|name| name == "hyprland") {
        hyprland
    } else {
        format!("Bind `{command}` to Ctrl+Shift+Y in your desktop's keyboard settings. {sway} {hyprland}")
    };
    format!(
        "Open Ibis from your application launcher. {binding} Without the ibis CLI, use {} toggle instead.",
        binary
    )
}

#[cfg(any(target_os = "linux", test))]
fn status(trigger: Option<String>, configurable: bool, guidance: String, error: Option<String>) -> ShortcutStatus {
    ShortcutStatus {
        registered: trigger.is_some(),
        shortcut: trigger.unwrap_or_default(),
        error,
        desktop: Some(DesktopShortcut { configurable, guidance }),
    }
}

#[cfg(any(target_os = "linux", test))]
fn reported_status(
    trigger: Option<String>,
    configurable: bool,
    guidance: &str,
    desktop: &str,
    binary: &str,
) -> ShortcutStatus {
    let missing = trigger.is_none();
    let guidance = if missing {
        fallback(desktop, binary)
    } else {
        guidance.into()
    };
    status(
        trigger,
        configurable,
        guidance,
        missing.then(|| "Your desktop did not report a shortcut.".into()),
    )
}

#[cfg(any(target_os = "linux", test))]
fn focus_token(token: Option<String>) -> Option<String> {
    token.filter(|value| !value.is_empty() && value.len() <= 4096 && !value.contains('\0'))
}

#[cfg(any(target_os = "linux", test))]
pub(super) fn x11_state(wayland: bool, read: impl FnOnce() -> Result<Option<bool>, ()>) -> Result<Option<bool>, ()> {
    if wayland { Ok(None) } else { read() }
}

/// Called after the tray exists; portal consent never blocks startup.
pub(crate) fn start_shortcut(app: &AppHandle, setting: Option<&str>) {
    #[cfg(target_os = "linux")]
    if desktop_owned() {
        linux::start(app);
        return;
    }
    let (shortcut, label, setting_error) = super::configured_shortcut(setting);
    let registration = super::register_shortcut(app, shortcut);
    let status = ShortcutStatus {
        shortcut: label,
        registered: registration.is_ok(),
        error: match registration {
            Ok(note) => setting_error.or(note.map(str::to_string)),
            Err(error) => Some(error.to_string()),
        },
        desktop: None,
    };
    if let Some(error) = &status.error {
        crate::diagnostics::report(Err::<(), _>(error), crate::diagnostics::NativeIssue::Shortcut);
    }
    if let Ok(mut current) = app.state::<Mutex<ShortcutStatus>>().lock() {
        *current = status.clone();
    }
    crate::tray::show_shortcut(app, status.registered.then_some(status.shortcut.as_str()));
}

/// Only Settings may request the desktop's configuration UI. It never saves.
#[tauri::command]
#[specta::specta]
pub(crate) async fn configure_shortcuts(app: AppHandle) -> Result<(), &'static str> {
    #[cfg(target_os = "linux")]
    return linux::configure(&app).await;
    #[cfg(not(target_os = "linux"))]
    {
        let _ = app;
        Err("The desktop shortcut settings are unavailable.")
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::super::portal::{Configure, Event, Failure, Portal};
    use super::*;
    use crate::diagnostics::{NativeIssue, report};
    use gtk::prelude::GtkWindowExt;
    use tauri::Emitter;

    fn publish(app: &AppHandle, next: ShortcutStatus) {
        if let Ok(mut current) = app.state::<Mutex<ShortcutStatus>>().lock() {
            *current = next.clone();
        }
        let handle = app.clone();
        let label = next.registered.then_some(next.shortcut.clone());
        report(
            app.run_on_main_thread(move || crate::tray::show_shortcut(&handle, label.as_deref())),
            NativeIssue::Shortcut,
        );
        report(app.emit("shortcut:status", next), NativeIssue::Shortcut);
    }

    pub(super) fn start(app: &AppHandle) {
        app.manage(Mutex::new(None::<Configure>));
        publish(
            app,
            status(None, false, "Waiting for the desktop shortcut…".into(), None),
        );
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
            let nightly = app.config().identifier == "com.ibiscite.ibis.nightly";
            let executable = std::env::current_exe().ok();
            let binary = toggle_binary(executable.as_deref(), nightly);
            let result = async {
                let portal = Portal::connect().await?;
                let configure = portal.configure.clone();
                let configurable = configure.version >= 2;
                let guidance = if configurable {
                    "Your desktop manages this shortcut."
                } else if desktop.to_ascii_lowercase().split(':').any(|name| name == "kde") {
                    "Change this shortcut in System Settings > Keyboard > Shortcuts > Ibis."
                } else if desktop.to_ascii_lowercase().split(':').any(|name| name == "gnome") {
                    "Change this shortcut in Settings > Keyboard > View and Customize Shortcuts > Ibis."
                } else {
                    "Change this shortcut in your desktop's keyboard shortcut settings under Ibis."
                };
                portal
                    .run(|event| match event {
                        Event::Trigger(trigger) => {
                            if let Ok(mut current) = app.state::<Mutex<Option<Configure>>>().lock() {
                                *current = Some(configure.clone());
                            }
                            publish(&app, reported_status(trigger, configurable, guidance, &desktop, binary));
                        }
                        Event::Activated(token) => {
                            let handle = app.clone();
                            // The loop stays responsive while all native window operations
                            // and the one-use focus token run together on the GTK thread.
                            report(
                                app.run_on_main_thread(move || {
                                    super::super::toggle_picker_with(&handle, || {
                                        if let Some(token) = focus_token(token)
                                            && let Some(window) = handle.get_webview_window("main")
                                            && let Ok(window) = window.gtk_window()
                                        {
                                            window.set_startup_id(&token);
                                        }
                                    });
                                }),
                                NativeIssue::Shortcut,
                            );
                        }
                    })
                    .await
            }
            .await;
            if let Ok(mut current) = app.state::<Mutex<Option<Configure>>>().lock() {
                *current = None;
            }
            let error = if result == Err(Failure::Denied) {
                "The desktop shortcut request was cancelled. Restart Ibis to request it again."
            } else {
                "The desktop shortcut is unavailable. Restart Ibis after the desktop portal is available."
            };
            publish(
                &app,
                status(None, false, fallback(&desktop, binary), Some(error.into())),
            );
        });
    }

    pub(super) async fn configure(app: &AppHandle) -> Result<(), &'static str> {
        let configure = app
            .try_state::<Mutex<Option<Configure>>>()
            .and_then(|state| state.lock().ok().and_then(|value| value.clone()))
            .ok_or("The desktop shortcut settings are unavailable.")?;
        configure
            .open()
            .await
            .map_err(|_| "The desktop could not open its shortcut settings. Try again.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_session_without_a_trigger_shows_bindings_and_keeps_configuration() {
        for configurable in [false, true] {
            for binary in ["ibis-desktop", "ibis-nightly"] {
                let next = reported_status(None, configurable, "Desktop settings", "Hyprland", binary);
                assert!(!next.registered);
                assert!(next.shortcut.is_empty());
                assert!(
                    next.error.is_some(),
                    "the picker must show guidance even though the session succeeded"
                );
                let desktop = next.desktop.unwrap_or_else(|| panic!("missing desktop status"));
                assert_eq!(desktop.configurable, configurable);
                assert!(desktop.guidance.contains("hl.bind(\"CTRL + SHIFT + Y\""));
                assert!(desktop.guidance.contains("bind = CTRL SHIFT, Y, exec, ibis toggle"));
                assert!(desktop.guidance.contains(&format!("{binary} toggle")));
                assert!(!desktop.guidance.contains("Restart"));
            }
        }
        let assigned = reported_status(
            Some("Alt+Y".into()),
            true,
            "Desktop settings",
            "Hyprland",
            "ibis-desktop",
        );
        assert!(assigned.registered);
        assert_eq!(assigned.shortcut, "Alt+Y");
        assert!(assigned.error.is_none());
        assert_eq!(
            assigned.desktop.map(|desktop| desktop.guidance).as_deref(),
            Some("Desktop settings")
        );
    }

    #[test]
    fn wayland_never_uses_xwayland_focus_or_full_screen_state() {
        assert_eq!(
            x11_state(true, || panic!("XWayland cannot report native app state")),
            Ok(None)
        );
        assert_eq!(x11_state(false, || Ok(Some(false))), Ok(Some(false)));
        assert_eq!(x11_state(false, || Err(())), Err(()));
    }

    #[test]
    fn focus_tokens_reject_empty_oversized_and_nul_values_before_ffi() {
        assert_eq!(
            focus_token(Some("synthetic-研究-<opaque>".into())),
            Some("synthetic-研究-<opaque>".into())
        );
        for token in [
            None,
            Some(String::new()),
            Some("x".repeat(4097)),
            Some("bad\0token".into()),
        ] {
            assert_eq!(focus_token(token), None);
        }
    }

    #[test]
    fn wayland_uses_the_portal_even_with_an_xwayland_display() {
        assert!(is_wayland(Some("wayland"), None));
        assert!(is_wayland(Some("x11"), Some("wayland-0")));
        assert!(!is_wayland(Some("x11"), Some("")));
        assert!(!is_wayland(None, None));
    }

    #[test]
    fn missing_portals_give_exact_bindings_and_channel_without_private_paths() {
        let sway = fallback("custom:SWAY", "ibis-desktop");
        assert!(sway.contains("bindsym --no-repeat Ctrl+Shift+y exec ibis toggle"));
        assert!(sway.contains("~/.config/sway/config"));
        assert!(!sway.contains("hl.bind"));
        let hypr = fallback("Hyprland", "ibis-nightly");
        assert!(hypr.contains("hl.bind(\"CTRL + SHIFT + Y\", hl.dsp.exec_cmd(\"ibis toggle --app ibis-nightly\"))"));
        assert!(hypr.contains("bind = CTRL SHIFT, Y, exec, ibis toggle --app ibis-nightly"));
        assert!(hypr.contains("ibis-nightly toggle"));
        let unknown = fallback("unknown", "ibis-desktop");
        assert!(unknown.contains("bindsym"));
        assert!(unknown.contains("hl.bind"));
    }

    #[test]
    fn fallback_preserves_the_running_binary_then_the_channel_without_private_paths() {
        for (path, nightly, expected) in [
            (Some("/synthetic/bin/ibis-nightly"), false, "ibis-nightly"),
            (Some("/synthetic/debug/ibis-desktop"), true, "ibis-desktop"),
            (Some("/synthetic/private path/other"), true, "ibis-nightly"),
            (None, true, "ibis-nightly"),
            (None, false, "ibis-desktop"),
        ] {
            let binary = toggle_binary(path.map(std::path::Path::new), nightly);
            assert_eq!(binary, expected);
            let guidance = fallback("sway", binary);
            assert!(guidance.contains(&format!("{expected} toggle")));
            assert!(!guidance.contains("/synthetic"));
        }
    }

    #[test]
    fn desktop_status_displays_only_reported_triggers_and_is_not_a_setting() {
        for (trigger, registered) in [(Some("Strg + Umschalt + Y".into()), true), (None, false)] {
            let next = status(trigger.clone(), true, "Desktop settings".into(), None);
            assert_eq!(next.shortcut, trigger.unwrap_or_default());
            assert_eq!(next.registered, registered);
            let json = serde_json::to_value(next).unwrap_or_default();
            assert_eq!(json["desktop"]["configurable"], true);
        }
    }
}
