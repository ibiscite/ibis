use ibis_core::destination::{AppPaste, CopyReason, Decision, PastePermission, System, decide, lookup};
use tauri::{AppHandle, Manager};

use super::capture::Selection;
use super::{Captured, LastTarget, permission};

pub(super) fn paste_policy(app: &AppHandle, enabled: bool, choosing: bool) -> bool {
    let state = app.state::<LastTarget>();
    let Ok(last) = state.0.lock() else { return false };
    #[cfg(not(target_os = "windows"))]
    let captured = last.captured.as_ref();
    #[cfg(target_os = "windows")]
    let captured = last.target.as_ref().and_then(|target| target.captured.as_ref());
    if choosing
        && captured.is_some_and(|target| {
            target.app_identity.is_none() && target.target.app.as_ref().is_some_and(|id| lookup(id).is_none())
        })
    {
        return false;
    }
    let preference = if choosing {
        Some(AppPaste::Paste)
    } else {
        let Ok(preference) = captured.map_or(Ok(None), |target| {
            app.state::<crate::destination::AppMemory>().for_capture(target)
        }) else {
            return false;
        };
        preference
    };
    let permission = PastePermission {
        enabled,
        granted: enabled && permission::paste_permission().granted,
        app: preference,
    };
    // What is selected now says nothing about choosing paste for this app.
    if choosing {
        return captured.is_some_and(|captured| policy_decision(captured, permission) == Decision::Paste);
    }
    intended_paste(captured, permission)
}

pub(super) fn intended_paste(captured: Option<&Captured>, permission: PastePermission) -> bool {
    captured.is_some_and(|captured| delivery_decision(captured, permission) == Decision::Paste)
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

fn policy_decision(captured: &Captured, permission: PastePermission) -> Decision {
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
