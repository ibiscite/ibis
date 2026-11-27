use ibis_core::destination::{CopyReason, Decision, PastePermission, System, decide};

use super::Captured;
use super::capture::Selection;

/// Whether delivery would paste into `captured` under `permission`.
pub fn intended_paste(captured: Option<&Captured>, permission: PastePermission) -> bool {
    captured.is_some_and(|captured| delivery_decision(captured, permission) == Decision::Paste)
}

/// Whether to offer a paste. A missing grant can still turn into the
/// permission-specific copy fallback. This is a label and intent decision.
pub fn offers_paste(captured: &Captured, permission: PastePermission) -> bool {
    matches!(
        delivery_decision(captured, permission),
        Decision::Paste | Decision::Copy(CopyReason::NoPermission | CopyReason::AccessibilityRequired)
    )
}

pub(super) fn delivery_decision(captured: &Captured, permission: PastePermission) -> Decision {
    match policy_decision(captured, permission) {
        // A paste replaces the selection; Ibis never deletes text it did not show.
        Decision::Paste if captured.selection == Selection::Ignored => Decision::Copy(CopyReason::SelectionNotShown),
        // Only macOS can read a selection, so nowhere else can prove the old citation.
        Decision::Paste
            if matches!(captured.selection, Selection::Citation(_)) && captured.target.system != System::Macos =>
        {
            Decision::Copy(CopyReason::CitationNotSelected)
        }
        decision => decision,
    }
}

/// The engine's decision for `captured`, refined by what its capture proved.
pub fn policy_decision(captured: &Captured, permission: PastePermission) -> Decision {
    let decision = decide(&captured.target, permission);
    if let Err(reason) = captured.paste
        && matches!(
            decision,
            Decision::Paste | Decision::Copy(CopyReason::NoPermission | CopyReason::UnknownApp)
        )
    {
        return Decision::Copy(reason);
    }
    match decision {
        Decision::Paste | Decision::Copy(CopyReason::NoPermission)
            if captured.target.system == System::Macos && captured.window.is_none() =>
        {
            Decision::Copy(captured.focus_failure.unwrap_or(CopyReason::PasteUnavailable))
        }
        decision => decision,
    }
}
