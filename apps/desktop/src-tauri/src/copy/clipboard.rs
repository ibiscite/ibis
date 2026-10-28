//! Writing one engine result to the clipboard: plain text, HTML and RTF in
//! one write, so Word, Pages and mail keep italics and other apps get text
//! (#26). The clipboard is not restored afterwards (decision 0009).

use ibis_core::contract::ClipboardContent;

/// One representation of the citation, in the order it is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flavor {
    Text,
    Html,
    Rtf,
}

/// What a write puts on the clipboard: always the text, then HTML and RTF
/// when the result has them. Every flavor comes from the same result, so they
/// share its content hash.
pub(crate) fn flavors<'a>(content: &ClipboardContent<'a>) -> Vec<(Flavor, &'a str)> {
    let mut flavors = vec![(Flavor::Text, content.text)];
    flavors.extend(content.html.map(|html| (Flavor::Html, html)));
    flavors.extend(content.rtf.map(|rtf| (Flavor::Rtf, rtf)));
    flavors
}

const FAILED: &str = "The clipboard could not be written.";

/// The general pasteboard, cleared once and then given every flavor.
#[cfg(target_os = "macos")]
pub(crate) fn write(_app: &tauri::AppHandle, content: ClipboardContent<'_>) -> Result<(), String> {
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::{NSData, NSString};
    let board = NSPasteboard::generalPasteboard();
    board.clearContents();
    for (flavor, value) in flavors(&content) {
        let written = match flavor {
            Flavor::Text => board.setString_forType(
                &NSString::from_str(value),
                &NSString::from_str("public.utf8-plain-text"),
            ),
            Flavor::Html => board.setString_forType(&NSString::from_str(value), &NSString::from_str("public.html")),
            Flavor::Rtf => board.setData_forType(
                Some(&NSData::with_bytes(value.as_bytes())),
                &NSString::from_str("public.rtf"),
            ),
        };
        if !written {
            return Err(FAILED.into());
        }
    }
    Ok(())
}

/// One opening of the clipboard, retried briefly while another app holds it.
#[cfg(target_os = "windows")]
pub(crate) fn write(_app: &tauri::AppHandle, content: ClipboardContent<'_>) -> Result<(), String> {
    use clipboard_win::{Clipboard, formats::Html, options::NoClear, raw};
    // About 100 ms in all: 20 retries, 5 ms apart.
    let _open = Clipboard::new_attempts(20).map_err(|_| FAILED.to_string())?;
    raw::empty().map_err(|_| FAILED.to_string())?;
    for (flavor, value) in flavors(&content) {
        match flavor {
            Flavor::Text => raw::set_string_with(value, NoClear),
            Flavor::Html => {
                let html = Html::new().ok_or_else(|| FAILED.to_string())?;
                raw::set_html(html.code(), value)
            }
            Flavor::Rtf => {
                let rtf = raw::register_format("Rich Text Format").ok_or_else(|| FAILED.to_string())?;
                raw::set_without_clear(rtf.get(), value.as_bytes())
            }
        }
        .map_err(|_| FAILED.to_string())?;
    }
    Ok(())
}

/// Text and HTML; Linux apps read HTML for formatting and rarely RTF.
#[cfg(target_os = "linux")]
pub(crate) fn write(app: &tauri::AppHandle, content: ClipboardContent<'_>) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    let html = flavors(&content)
        .into_iter()
        .find_map(|(flavor, value)| (flavor == Flavor::Html).then_some(value));
    match html {
        Some(html) => app.clipboard().write_html(html, Some(content.text)),
        None => app.clipboard().write_text(content.text),
    }
    .map_err(|_| FAILED.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_always_comes_first_and_rich_flavors_only_when_present() {
        let rich = ClipboardContent {
            text: "Invented (2026)",
            html: Some("<i>Invented</i> (2026)"),
            rtf: Some("{\\rtf1 {\\i Invented} (2026)}"),
        };
        assert_eq!(
            flavors(&rich),
            [
                (Flavor::Text, "Invented (2026)"),
                (Flavor::Html, "<i>Invented</i> (2026)"),
                (Flavor::Rtf, "{\\rtf1 {\\i Invented} (2026)}"),
            ]
        );
        let plain = ClipboardContent {
            text: "[@Invented2026]",
            html: None,
            rtf: None,
        };
        assert_eq!(flavors(&plain), [(Flavor::Text, "[@Invented2026]")]);
    }
}
