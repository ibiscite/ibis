//! Which app the picker was opened from, taken before the panel shows (#1241).
//!
//! Only app identity, its name for feedback and an opaque window number are
//! kept in memory. Window titles and document text are never read.

use ibis_core::destination::{AppId, System, Target};

/// What the system reports about the app in front.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Front {
    /// macOS bundle ID or Windows executable file name; `None` for an app without one.
    pub(crate) id: Option<String>,
    pub(crate) pid: i32,
    /// Opaque macOS window number; no title or contents.
    pub(crate) window: Option<u32>,
    /// The app's own name, as the Dock or its file description shows it.
    pub(crate) name: Option<String>,
    /// The app runs with more rights than Ibis (Windows integrity level).
    pub(crate) elevated: bool,
}

/// The app Enter pastes into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Captured {
    pub(crate) target: Target,
    pub(crate) pid: Option<i32>,
    pub(crate) window: Option<u32>,
    pub(crate) name: Option<String>,
}

impl Captured {
    /// No app to paste into: what a capture says on Linux and when nothing
    /// was in front.
    pub(crate) fn unknown(system: System) -> Self {
        Self {
            target: Target {
                system,
                app: None,
                secure_field: false,
                elevated: false,
            },
            pid: None,
            window: None,
            name: None,
        }
    }
}

/// The capture for `front`, or `None` when Ibis itself is in front (the
/// picker shown again, or another Ibis window), so the earlier capture stays.
/// Secure input is checked at delivery, after the app has regained focus;
/// its state when the picker opened cannot prove that a later paste is safe.
/// macOS checks system-wide Secure Event Input; Windows checks the focused
/// field with UI Automation. The destination table refuses terminals, remote
/// desktops and VMs on both platforms.
pub(crate) fn captured(system: System, front: Option<Front>, own: i32) -> Option<Captured> {
    let Some(front) = front else {
        return Some(Captured::unknown(system));
    };
    if front.pid == own {
        return None;
    }
    Some(Captured {
        target: Target {
            system,
            app: front.id.map(|id| AppId { system, id }),
            secure_field: false,
            elevated: front.elevated,
        },
        pid: Some(front.pid),
        window: front.window,
        name: front.name,
    })
}

#[cfg(any(target_os = "macos", test))]
pub(super) struct WindowMetadata {
    pub(super) pid: i32,
    pub(super) id: u32,
    pub(super) layer: i32,
}

/// A single visible window is the only metadata-only proof of the target.
/// Additional panels at any layer make keyboard ownership ambiguous.
#[cfg(any(target_os = "macos", test))]
pub(super) fn unambiguous_window(windows: impl IntoIterator<Item = Option<WindowMetadata>>, pid: i32) -> Option<u32> {
    let mut found = None;
    for window in windows {
        let window = window?;
        if window.pid != pid {
            continue;
        }
        if found.is_some() || window.layer != 0 || window.id == 0 {
            return None;
        }
        found = Some(window.id);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn front(pid: i32) -> Front {
        Front {
            id: Some("org.example.Writer".into()),
            pid,
            window: Some(100),
            name: Some("Writer".into()),
            elevated: false,
        }
    }

    #[test]
    fn the_app_in_front_is_kept_by_id_and_process() {
        let captured = captured(System::Macos, Some(front(42)), 7).unwrap_or_else(|| panic!("captured"));
        assert_eq!(captured.pid, Some(42));
        assert_eq!(captured.name.as_deref(), Some("Writer"));
        assert_eq!(
            captured.target.app,
            Some(AppId {
                system: System::Macos,
                id: "org.example.Writer".into()
            })
        );
    }

    #[test]
    fn ibis_in_front_keeps_the_earlier_capture_and_nothing_in_front_is_unknown() {
        assert_eq!(captured(System::Macos, Some(front(7)), 7), None);
        assert_eq!(captured(System::Macos, None, 7), Some(Captured::unknown(System::Macos)));
        let unbundled = Front { id: None, ..front(42) };
        assert_eq!(
            captured(System::Macos, Some(unbundled), 7).and_then(|c| c.target.app),
            None
        );
    }

    #[test]
    fn an_app_with_more_rights_than_ibis_is_captured_as_elevated() {
        let admin = Front {
            elevated: true,
            ..front(42)
        };
        let captured = captured(System::Windows, Some(admin), 7).unwrap_or_else(|| panic!("captured"));
        assert!(captured.target.elevated);
    }

    #[test]
    fn a_capture_holds_no_window_title_or_document_text() {
        let captured = captured(System::Macos, Some(front(42)), 7).unwrap_or_else(|| panic!("captured"));
        let target = serde_json::to_value(&captured.target).unwrap_or_else(|e| panic!("{e}"));
        let mut keys: Vec<_> = target
            .as_object()
            .map(|fields| fields.keys().cloned().collect())
            .unwrap_or_default();
        keys.sort();
        assert_eq!(keys, ["app", "elevated", "secure_field", "system"]);
        // The only strings kept are the app ID and the app's own name.
        assert_eq!(format!("{captured:?}").matches('"').count(), 4, "{captured:?}");
    }

    #[test]
    fn window_metadata_refuses_dialogs_at_every_layer_missing_and_ambiguous_windows() {
        let window = |pid, id, layer| Some(WindowMetadata { pid, id, layer });
        assert_eq!(
            unambiguous_window([window(42, 100, 0), window(7, 200, 0)], 42),
            Some(100)
        );
        for windows in [
            vec![],
            vec![None],
            vec![window(7, 100, 0)],
            vec![window(42, 0, 0)],
            vec![window(42, 100, 1)],
            vec![window(42, 100, 0), window(42, 200, 0)],
            vec![window(42, 100, 0), window(42, 200, 8)],
        ] {
            assert_eq!(unambiguous_window(windows, 42), None);
        }
    }
}
