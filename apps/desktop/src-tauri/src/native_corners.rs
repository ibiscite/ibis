//! Clip the macOS picker's content using AppKit's existing backing layer.
//!
//! Install only after native materials configure a transparent window before
//! creation and the launcher panel conversion has completed. The content mask
//! clips the WebView and effect-view descendants; the WindowServer shadow is
//! separate and must remain enabled on the window.

#![deny(clippy::print_stderr)]

use tauri_nspanel::{NSPanel, objc2::MainThreadMarker};

// Canvas C15 uses sixteen logical points on every macOS version, independent
// of text and display scale. Keep the CSS window role and native mask aligned.
pub(crate) const CORNER_RADIUS_POINTS: f64 = 16.0;

/// Corners are cosmetic. A missing prerequisite keeps the existing rectangular
/// picker usable rather than turning an appearance failure into startup failure.
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

#[cfg(test)]
pub(super) fn test_warning() {
    warning("the picker content layer is unavailable");
}

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
