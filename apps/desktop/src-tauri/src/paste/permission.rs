//! The permission a paste needs, and the guided way to give it (#1292).
//!
//! macOS lets an app post the ⌘V keystroke only with the person's permission
//! (TCC service PostEvent, listed under Privacy & Security). It keeps that
//! permission per code signature. While builds are ad-hoc signed, every update
//! is a new signature, so the switch in System Settings can look on while it
//! belongs to an older build and the paste is refused (#1242). The explicit
//! Open System Settings action can repair the stale PostEvent entry.
//! Accessibility is separately required for identity-only focus reads: checks
//! never prompt for it. The guide's explicit action repairs whichever grant is missing.

use serde::Serialize;

// AppKit's drag-source callbacks and image-contents setter require a scoped
// Objective-C boundary; all other permission code keeps denying unsafe (0029).
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod drag;

/// Whether Ibis may paste, and where the person turns that on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub(crate) struct PastePermission {
    /// Ibis pastes on this system; where it only copies, Settings offers no pasting choices.
    pub(crate) pastes: bool,
    /// This system asks the person before an app may paste for them.
    pub(crate) needed: bool,
    pub(crate) granted: bool,
    /// The list in System Settings that holds the switch, in the system's words.
    pub(crate) list: String,
}

/// The macOS 27 name of the Accessibility list in Privacy & Security.
#[cfg(any(target_os = "macos", test))]
const LIST_SINCE_27: &str = "Device Control and Data Access";
#[cfg(any(target_os = "macos", test))]
const LIST_BEFORE_27: &str = "Accessibility";

/// The list's name on a macOS major version.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn list_name(major: u32) -> &'static str {
    if major >= 27 { LIST_SINCE_27 } else { LIST_BEFORE_27 }
}

/// Whether Ibis may paste now. Read each time: the person can change it at any moment.
#[tauri::command]
#[specta::specta]
pub(crate) fn paste_permission() -> PastePermission {
    current()
}

/// On an explicit user action, repair only missing grants and open the pane.
#[tauri::command]
#[specta::specta]
pub(crate) fn open_paste_permission(app: tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let identifier = app.config().identifier.clone();
        std::thread::spawn(move || macos::guide(&identifier));
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

/// Drag only the running app bundle into the permission list.
#[tauri::command]
#[specta::specta]
pub(crate) fn drag_paste_app(window: tauri::WebviewWindow) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        drag::start(&window).map_err(str::to_owned)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window;
        Err("App dragging is available only on macOS.".to_owned())
    }
}

#[cfg(target_os = "macos")]
fn current() -> PastePermission {
    macos_permissions(
        objc2_core_graphics::CGPreflightPostEventAccess(),
        super::macos::accessibility_granted(),
        macos::major_version(),
    )
}

#[cfg(any(target_os = "macos", test))]
fn macos_permissions(post_event: bool, accessibility: bool, major: u32) -> PastePermission {
    PastePermission {
        pastes: true,
        needed: true,
        granted: post_event && accessibility,
        list: list_name(major).to_string(),
    }
}

#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Grant {
    PostEvent,
    Accessibility,
}

#[cfg(any(target_os = "macos", test))]
fn repair_missing(mut granted: impl FnMut(Grant) -> bool, mut repair: impl FnMut(Grant)) {
    for grant in [Grant::PostEvent, Grant::Accessibility] {
        if !granted(grant) {
            repair(grant);
        }
    }
}

/// Windows needs no permission to paste; Linux only copies.
#[cfg(not(target_os = "macos"))]
fn current() -> PastePermission {
    PastePermission {
        pastes: cfg!(target_os = "windows"),
        needed: false,
        granted: true,
        list: String::new(),
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::process::Command;

    /// The Privacy & Security pane that lists apps allowed to control the computer.
    const PANE: &str = "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

    pub(super) fn guide(identifier: &str) {
        super::repair_missing(
            |grant| match grant {
                super::Grant::PostEvent => objc2_core_graphics::CGPreflightPostEventAccess(),
                super::Grant::Accessibility => super::super::macos::accessibility_granted(),
            },
            |grant| {
                // Only Ibis's own entry, and only while it does not work anyway.
                // Development builds share the release identifier, so they never
                // clear an installed copy's working permission.
                if !cfg!(debug_assertions) {
                    let _ = Command::new("/usr/bin/tccutil")
                        .args([
                            "reset",
                            match grant {
                                super::Grant::PostEvent => "PostEvent",
                                super::Grant::Accessibility => "Accessibility",
                            },
                            identifier,
                        ])
                        .status();
                }
                // Lists Ibis, switched off, so the person has one switch to turn on.
                match grant {
                    super::Grant::PostEvent => {
                        objc2_core_graphics::CGRequestPostEventAccess();
                    }
                    super::Grant::Accessibility => super::super::macos::request_accessibility(),
                }
            },
        );
        let _ = Command::new("/usr/bin/open").arg(PANE).status();
    }

    /// The macOS major version, such as 27, read once; 0 when it cannot be read.
    pub(super) fn major_version() -> u32 {
        static MAJOR: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
        *MAJOR.get_or_init(|| {
            Command::new("/usr/bin/sw_vers")
                .arg("-productVersion")
                .output()
                .ok()
                .and_then(|out| String::from_utf8(out.stdout).ok())
                .and_then(|version| version.trim().split('.').next()?.parse().ok())
                .unwrap_or(0)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_named_as_each_macos_names_it() {
        assert_eq!(list_name(15), "Accessibility");
        assert_eq!(list_name(26), "Accessibility");
        assert_eq!(list_name(27), "Device Control and Data Access");
        assert_eq!(list_name(28), "Device Control and Data Access");
    }

    #[test]
    fn macos_needs_both_separate_authorizations_even_when_post_event_is_already_granted() {
        for (post_event, accessibility) in [(false, false), (false, true), (true, false), (true, true)] {
            let permission = macos_permissions(post_event, accessibility, 27);
            assert_eq!(permission.granted, post_event && accessibility);
            assert_eq!(permission.list, "Device Control and Data Access");
        }
    }

    #[test]
    fn the_explicit_guide_repairs_each_missing_grant_and_preserves_working_grants() {
        for (post_event, accessibility, expected) in [
            (true, false, vec![Grant::Accessibility]),
            (false, true, vec![Grant::PostEvent]),
            (false, false, vec![Grant::PostEvent, Grant::Accessibility]),
            (true, true, vec![]),
        ] {
            let mut repaired = Vec::new();
            repair_missing(
                |grant| match grant {
                    Grant::PostEvent => post_event,
                    Grant::Accessibility => accessibility,
                },
                |grant| repaired.push(grant),
            );
            assert_eq!(repaired, expected);
        }
    }

    #[test]
    fn a_grant_obtained_during_the_first_repair_is_not_reset_by_the_second() {
        let accessibility = std::cell::Cell::new(false);
        let mut repaired = Vec::new();
        repair_missing(
            |grant| grant == Grant::Accessibility && accessibility.get(),
            |grant| {
                repaired.push(grant);
                accessibility.set(true);
            },
        );
        assert_eq!(repaired, [Grant::PostEvent]);
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn other_systems_need_no_permission() {
        let permission = paste_permission();
        assert!(!permission.needed);
        assert!(permission.granted);
        assert!(permission.list.is_empty());
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn windows_offers_the_paste_switch_without_a_permission_guide() {
        assert!(paste_permission().pastes);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn linux_only_copies() {
        assert!(!paste_permission().pastes);
    }
}
