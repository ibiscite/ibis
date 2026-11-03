//! Which app the picker was opened from, taken before the panel shows (#1241).
//!
//! Only app identity, its name for feedback, an opaque window token and a
//! digest of a selection are kept in memory. Window titles are never read, and
//! document text only as the selection decision 0029 allows, once per opening.

use ibis_core::contract::Fingerprint;
use ibis_core::destination::{AppId, AppPaste, CopyReason, System, Target};

/// The most text, in UTF-16 units as AX counts it, that can become the search.
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
pub(crate) const SHORT_SELECTION: usize = 120;

/// What was selected in the target when the picker opened (#1526). Ibis only
/// pastes over text the person saw in search: `Ignored` refuses the paste, and
/// `Unread` lets it through only once nothing is proved selected.
#[derive(Clone, Default, PartialEq, Eq)]
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
pub(crate) enum Selection {
    /// The app has no way to report a selection, so Ibis cannot tell and pastes.
    #[default]
    Unknown,
    /// Refused, failed or too slow at opening: the question is still open.
    Unread,
    Nothing,
    /// Long, multi-line or blank, or the app would not say what: never shown,
    /// and none of it is kept.
    Ignored,
    /// A digest only; the text itself goes to the search once.
    Short(Fingerprint),
}

/// Variant names only: a digest of a few words can be guessed.
impl std::fmt::Debug for Selection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unknown => "Unknown",
            Self::Unread => "Unread",
            Self::Nothing => "Nothing",
            Self::Ignored => "Ignored",
            Self::Short(_) => "Short",
        })
    }
}

#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
impl Selection {
    /// Decide what `text` is, and what the search should show of it.
    pub(crate) fn of_text(text: &str) -> (Self, Option<SelectedText>) {
        if text.is_empty() {
            return (Self::Nothing, None);
        }
        let shown = text.trim();
        if shown.is_empty() || text.encode_utf16().count() > SHORT_SELECTION || text.chars().any(line_break_or_control)
        {
            return (Self::Ignored, None);
        }
        (
            Self::Short(Fingerprint::of_bytes(text.as_bytes())),
            Some(SelectedText(shown.to_owned())),
        )
    }
}

#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
fn line_break_or_control(c: char) -> bool {
    c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')
}

/// Text on its one way to the search: it has no `Clone`, and `Debug` hides it.
#[derive(PartialEq, Eq)]
pub(crate) struct SelectedText(String);

#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
impl SelectedText {
    pub(crate) fn into_search(self) -> String {
        self.0
    }
}

impl std::fmt::Debug for SelectedText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SelectedText")
    }
}

/// What the system reports about the app in front.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Front {
    /// macOS bundle ID or Windows executable file name; `None` for an app without one.
    pub(crate) id: Option<String>,
    /// Digest of app ID and its OS-reported location; never the raw path.
    pub(crate) app_identity: Option<String>,
    pub(crate) pid: i32,
    /// Opaque macOS document/surface identity token; no title or contents.
    pub(crate) window: Option<u32>,
    /// Capture-time refusal, kept even if an authorization changes later.
    pub(crate) focus_failure: Option<CopyReason>,
    /// The app's own name, as the Dock or its file description shows it.
    pub(crate) name: Option<String>,
    /// The app runs with more rights than Ibis (Windows integrity level).
    pub(crate) elevated: bool,
    pub(crate) selection: Selection,
    /// Taken for the search before the rest becomes a capture.
    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    pub(crate) search: Option<SelectedText>,
}

/// The app Enter pastes into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Captured {
    pub(crate) target: Target,
    pub(crate) app_identity: Option<String>,
    /// Frozen before the clipboard write; a later chooser answer cannot authorize it.
    pub(crate) paste: Result<Option<AppPaste>, CopyReason>,
    pub(crate) pid: Option<i32>,
    pub(crate) window: Option<u32>,
    pub(crate) focus_failure: Option<CopyReason>,
    pub(crate) name: Option<String>,
    pub(crate) selection: Selection,
    #[cfg(target_os = "macos")]
    pub(crate) diagnostic: Option<super::macos::diagnostics::Snapshot>,
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
            paste: Ok(None),
            app_identity: None,
            pid: None,
            window: None,
            focus_failure: None,
            name: None,
            selection: Selection::Unknown,
            #[cfg(target_os = "macos")]
            diagnostic: None,
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
        paste: Ok(None),
        app_identity: front.app_identity,
        pid: Some(front.pid),
        window: front.window,
        focus_failure: front.focus_failure,
        name: front.name,
        selection: front.selection,
        #[cfg(target_os = "macos")]
        diagnostic: super::macos::diagnostics::capture_snapshot(),
    })
}

/// No filesystem reads or path canonicalisation: compare exactly the location
/// the OS already reports, including case on case-sensitive volumes.
#[cfg(any(target_os = "macos", target_os = "windows", test))]
pub(crate) fn app_identity(system: System, id: &str, path: &str) -> Option<String> {
    if path.is_empty() || path.chars().any(char::is_control) {
        return None;
    }
    let app = AppId { system, id: id.into() };
    let bytes = serde_json::to_vec(&(system, app.normalized(), path)).ok()?;
    Some(ibis_core::contract::Fingerprint::of_bytes(&bytes).as_str().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn front(pid: i32) -> Front {
        Front {
            app_identity: None,
            id: Some("org.example.Writer".into()),
            pid,
            window: Some(100),
            focus_failure: None,
            name: Some("Writer".into()),
            elevated: false,
            selection: Selection::Unknown,
            search: None,
        }
    }

    fn short(text: &str) -> Selection {
        Selection::of_text(text).0
    }

    #[test]
    fn short_text_on_one_line_becomes_the_search_trimmed() {
        let (selection, search) = Selection::of_text(" smith 2020 ");
        assert!(matches!(selection, Selection::Short(_)));
        assert_eq!(search.map(SelectedText::into_search).as_deref(), Some("smith 2020"));
        let exact = "x".repeat(SHORT_SELECTION);
        assert!(Selection::of_text(&exact).1.is_some());
    }

    #[test]
    fn long_or_multi_line_text_is_ignored_and_never_handed_on() {
        let long = "x".repeat(SHORT_SELECTION + 1);
        // Counted as AX counts: one emoji is two units.
        let wide = "\u{1F426}".repeat(SHORT_SELECTION / 2 + 1);
        for text in [
            long.as_str(),
            wide.as_str(),
            "first\nsecond",
            "smith 2020\n",
            "first\r\nsecond",
            "first\u{2028}second",
            "first\u{2029}second",
            "tab\tstop",
        ] {
            assert_eq!(Selection::of_text(text), (Selection::Ignored, None), "{text:?}");
        }
    }

    #[test]
    fn nothing_selected_is_not_a_selection_and_spaces_cannot_be_shown_so_are_ignored() {
        assert_eq!(Selection::of_text(""), (Selection::Nothing, None));
        assert_eq!(Selection::of_text("   "), (Selection::Ignored, None));
    }

    #[test]
    fn a_selection_is_compared_by_digest_and_logs_only_its_kind() {
        assert_eq!(short("smith 2020"), short("smith 2020"));
        assert_ne!(short("smith 2020"), short("smith 2021"));
        let (selection, search) = Selection::of_text("Invented private words");
        let debug = format!("{selection:?} {search:?}");
        assert_eq!(debug, "Short Some(SelectedText)");
    }

    #[test]
    fn the_text_never_reaches_a_capture_and_the_digest_never_prints() {
        let (selection, search) = Selection::of_text("Invented private words");
        let captured = captured(
            System::Macos,
            Some(Front {
                selection: selection.clone(),
                search,
                ..front(42)
            }),
            7,
        )
        .unwrap_or_else(|| panic!("capture"));
        assert_eq!(captured.selection, selection);
        assert!(!format!("{captured:?}").contains("Invented"));
    }

    #[test]
    fn app_locations_are_compared_as_digests_without_retaining_the_paths() {
        let id = "editor.exe";
        let first = app_identity(System::Windows, id, r"C:\Invented\Writer\editor.exe");
        let other = app_identity(System::Windows, id, r"C:\Invented\Viewer\editor.exe");
        assert_ne!(first, other);
        assert_eq!(first.as_ref().map(String::len), Some(64));
        let captured = captured(
            System::Windows,
            Some(Front {
                id: Some(id.into()),
                app_identity: first.clone(),
                ..front(42)
            }),
            7,
        )
        .unwrap_or_else(|| panic!("capture"));
        assert_eq!(captured.app_identity, first);
        assert!(!format!("{captured:?}").contains("Invented"));
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
            app_identity: None,
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
    fn an_active_app_is_kept_without_inventing_a_keyboard_window_proof() {
        let front = Front {
            app_identity: None,
            window: None,
            ..front(42)
        };
        let captured = captured(System::Macos, Some(front), 7).unwrap_or_else(|| panic!("capture"));
        assert_eq!(captured.pid, Some(42));
        assert!(captured.target.app.is_some());
        assert_eq!(
            captured.window, None,
            "activation does not establish the global keyboard owner"
        );
    }
}
