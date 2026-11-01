//! Round the picker window's corners natively.
//!
//! macOS: clip the content using AppKit's existing backing layer. Install only
//! after native materials configure a transparent window before creation and
//! the launcher panel conversion has completed. The content mask clips the
//! WebView and effect-view descendants; the WindowServer shadow is separate and
//! must remain enabled on the window.
//!
//! Windows 11 rounds ordinary windows itself but leaves a transparent,
//! undecorated one square, so Ibis asks for the system's own round corner
//! (8 px, as on every Windows 11 window). Windows 10 has no rounded windows
//! and refuses the request; the picker stays square there like its neighbours.

#![deny(clippy::print_stderr)]

#[cfg(target_os = "macos")]
use tauri_nspanel::{NSPanel, objc2::MainThreadMarker};

#[cfg(target_os = "macos")]
// Canvas C15 uses sixteen logical points on every macOS version, independent
// of text and display scale. Keep the CSS window role and native mask aligned.
pub(crate) const CORNER_RADIUS_POINTS: f64 = 16.0;

/// Corners are cosmetic. A missing prerequisite keeps the existing rectangular
/// picker usable rather than turning an appearance failure into startup failure.
#[cfg(target_os = "macos")]
pub(crate) fn configure(panel: &NSPanel) {
    if let Err(reason) = apply(panel) {
        warning(reason);
    }
}

fn warning(reason: &'static str) {
    // Every reason below is fixed text; never log a native error payload.
    crate::diagnostics::application_message(&format!(
        "native picker corners unavailable: {reason}; using rectangular window"
    ));
}

#[cfg(all(test, target_os = "macos"))]
pub(super) fn test_warning() {
    warning("the picker content layer is unavailable");
}

#[cfg(target_os = "macos")]
fn apply(panel: &NSPanel) -> Result<(), &'static str> {
    if MainThreadMarker::new().is_none() {
        return Err("the picker corners must be configured on the main thread");
    }
    if panel.isOpaque() {
        return Err("the picker corners require a transparent window");
    }
    let content = panel.contentView().ok_or("the picker content view is unavailable")?;
    content.setWantsLayer(true);
    let layer = content.layer().ok_or("the picker content layer is unavailable")?;
    layer.setCornerRadius(CORNER_RADIUS_POINTS);
    layer.setMasksToBounds(true);
    panel.invalidateShadow();
    Ok(())
}

/// Ask Windows 11 for its round window corner. Windows 10 does not know the
/// attribute and keeps the window square, as it keeps every window, so that
/// refusal is expected and not reported.
#[cfg(target_os = "windows")]
pub(crate) fn configure(app: &tauri::AppHandle) {
    use winsafe::{DwmAttr, co};
    let Some(window) = crate::paste::picker_window(app) else {
        warning("the picker window is unavailable");
        return;
    };
    if window
        .DwmSetWindowAttribute(DwmAttr::WindowCornerPreference(co::DWMWCP::ROUND))
        .is_err()
        && windows_11()
    {
        warning("Windows refused the round corner");
    }
}

/// Windows 11 is Windows 10.0 from build 22000.
#[cfg(target_os = "windows")]
fn windows_11() -> bool {
    windows::System::Profile::AnalyticsInfo::VersionInfo()
        .and_then(|info| info.DeviceFamilyVersion())
        .ok()
        .and_then(|version| version.to_string().parse::<u64>().ok())
        .is_some_and(|version| version >> 48 == 10 && (version >> 16) & 0xffff >= 22000)
}
