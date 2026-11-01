use ibis_core::destination::{AppPaste, CopyReason, Decision, PastePermission, System, decide, lookup};
use tauri::{AppHandle, Manager};

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
    intended_paste(
        captured,
        PastePermission {
            enabled,
            granted: enabled && permission::paste_permission().granted,
            app: preference,
        },
    )
}

pub(super) fn intended_paste(captured: Option<&Captured>, permission: PastePermission) -> bool {
    captured.is_some_and(|captured| delivery_decision(captured, permission) == Decision::Paste)
}

pub(super) fn delivery_decision(captured: &Captured, permission: PastePermission) -> Decision {
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
