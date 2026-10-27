//! The permission a paste needs, and the guided way to give it (#1292).
//!
//! macOS lets an app post the ⌘V keystroke only with the person's permission
//! (TCC service PostEvent, listed under Privacy & Security). It keeps that
//! permission per code signature. While builds are ad-hoc signed, every update
//! is a new signature, so the switch in System Settings can look on while it
//! belongs to an older build and the paste is refused (#1242). Opening the
//! guide therefore clears Ibis's own stale entry first, asks again so one
//! fresh Ibis switch appears, and opens the pane where it lives.

use serde::Serialize;

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

/// Clear a stale entry, ask macOS to list Ibis, then open the pane with the switch.
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

#[cfg(target_os = "macos")]
fn current() -> PastePermission {
    PastePermission {
        pastes: true,
        needed: true,
        granted: objc2_core_graphics::CGPreflightPostEventAccess(),
        list: list_name(macos::major_version()).to_string(),
    }
}

/// Windows and Linux ask nothing before a paste, and for now they only copy.
#[cfg(not(target_os = "macos"))]
fn current() -> PastePermission {
    PastePermission {
        pastes: false,
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
        if !objc2_core_graphics::CGPreflightPostEventAccess() {
            // Only Ibis's own entry, and only while it does not work anyway.
            // Development builds share the release identifier, so they never
            // clear an installed copy's working permission.
            if !cfg!(debug_assertions) {
                let _ = Command::new("/usr/bin/tccutil")
                    .args(["reset", "PostEvent", identifier])
                    .status();
            }
            // Lists Ibis, switched off, so the person has one switch to turn on.
            objc2_core_graphics::CGRequestPostEventAccess();
        }
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
    #[cfg(not(target_os = "macos"))]
    fn other_systems_need_no_permission() {
        let permission = paste_permission();
        assert!(!permission.pastes);
        assert!(!permission.needed);
        assert!(permission.granted);
    }
}
