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

/// Publish richer formats before text: Windows recipients can choose the
/// first supported format in clipboard order. Every flavor comes from the
/// same result, so they share its content hash.
pub(crate) fn flavors<'a>(content: &ClipboardContent<'a>) -> Vec<(Flavor, &'a str)> {
    let mut flavors = Vec::with_capacity(3);
    flavors.extend(content.html.map(|html| (Flavor::Html, html)));
    flavors.extend(content.rtf.map(|rtf| (Flavor::Rtf, rtf)));
    flavors.push((Flavor::Text, content.text));
    flavors
}

const FAILED: &str = "The clipboard could not be written.";

#[cfg(target_os = "macos")]
fn macos_html(fragment: &str) -> String {
    // AppKit's HTML importer otherwise guesses a legacy encoding for UTF-8.
    format!(
        r#"<html><head><meta http-equiv="content-type" content="text/html; charset=utf-8"></head><body>{fragment}</body></html>"#
    )
}

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
            Flavor::Html => board.setString_forType(
                &NSString::from_str(&macos_html(value)),
                &NSString::from_str("public.html"),
            ),
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
pub(crate) fn write(_app: &tauri::AppHandle, content: ClipboardContent<'_>) -> Result<Option<u32>, String> {
    write_windows(content)
}

#[cfg(target_os = "windows")]
fn write_windows(content: ClipboardContent<'_>) -> Result<Option<u32>, String> {
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
    // Sample before `_open` closes: a foreign owner can never become this
    // copy's baseline. No contents need to be read during delivery.
    Ok(clipboard_win::seq_num().map(std::num::NonZeroU32::get))
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

    #[cfg(target_os = "macos")]
    #[test]
    fn appkit_html_import_preserves_non_latin_text() -> Result<(), Box<dyn std::error::Error>> {
        use std::process::Command;
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("decode.m");
        let decoder = directory.path().join("decode");
        // Use AppKit's default encoding detection, as a recipient can. This
        // tiny native decoder avoids unsafe Rust and never opens the clipboard.
        std::fs::write(
            &source,
            r#"
#import <AppKit/AppKit.h>
int main(int argc, const char **argv) { @autoreleasepool {
    if (argc != 2) return 2;
    NSData *data = [[NSString stringWithUTF8String:argv[1]] dataUsingEncoding:NSUTF8StringEncoding];
    NSAttributedString *text = [[NSAttributedString alloc] initWithHTML:data documentAttributes:NULL];
    if (!text) return 3;
    fputs([text.string UTF8String], stdout);
    return 0;
} }
"#,
        )?;
        let compiled = Command::new("clang")
            .args(["-fobjc-arc", "-framework", "AppKit", "-framework", "Foundation"])
            .arg(&source)
            .arg("-o")
            .arg(&decoder)
            .output()?;
        assert!(compiled.status.success(), "native HTML decoder did not compile");
        let decoded = Command::new(decoder)
            .arg(macos_html("<i>Ŝynthetic 山田</i>"))
            .output()?;
        assert!(decoded.status.success(), "native HTML decoder failed");
        assert_eq!(String::from_utf8(decoded.stdout)?, "Ŝynthetic 山田");
        Ok(())
    }

    #[test]
    fn first_supported_format_preserves_rich_text_and_plain_text_fallback() {
        let rich = ClipboardContent {
            text: "Ŝynthetic 山田",
            html: Some("<i>Ŝynthetic 山田</i>"),
            rtf: Some("{\\rtf1 {\\i Synthetic}}"),
        };
        // Windows enumerates formats in publication order. Rich Edit pastes
        // the first one it supports, even if richer formats follow it.
        for (supported, expected) in [
            (vec![Flavor::Html, Flavor::Rtf, Flavor::Text], Flavor::Html),
            (vec![Flavor::Rtf, Flavor::Text], Flavor::Rtf),
            (vec![Flavor::Html, Flavor::Text], Flavor::Html),
            (vec![Flavor::Text], Flavor::Text),
        ] {
            let selected = flavors(&rich)
                .into_iter()
                .find(|(flavor, _)| supported.contains(flavor));
            assert_eq!(selected.map(|(flavor, _)| flavor), Some(expected));
        }
    }

    #[test]
    fn rich_flavors_are_present_only_when_supplied_and_text_is_always_available() {
        let rich = ClipboardContent {
            text: "Invented (2026)",
            html: Some("<i>Invented</i> (2026)"),
            rtf: Some("{\\rtf1 {\\i Invented} (2026)}"),
        };
        assert_eq!(
            flavors(&rich),
            [
                (Flavor::Html, "<i>Invented</i> (2026)"),
                (Flavor::Rtf, "{\\rtf1 {\\i Invented} (2026)}"),
                (Flavor::Text, "Invented (2026)"),
            ]
        );
        let plain = ClipboardContent {
            text: "[@Invented2026]",
            html: None,
            rtf: None,
        };
        assert_eq!(flavors(&plain), [(Flavor::Text, "[@Invented2026]")]);
    }

    // Exercises the native Windows clipboard with invented data in CI.
    #[cfg(target_os = "windows")]
    #[test]
    fn windows_clipboard_preserves_format_selection_and_unicode_payloads() -> Result<(), String> {
        use clipboard_win::{Clipboard, formats::CF_UNICODETEXT, raw};
        let text = "Ŝynthetic 山田";
        let html = "<i>Ŝynthetic 山田</i>";
        let rtf = "{\\rtf1\\ansi\\uc1 {\\i \\u348?ynthetic \\u23665?\\u30000?}}";
        let sequence = write_windows(ClipboardContent {
            text,
            html: Some(html),
            rtf: Some(rtf),
        })?
        .ok_or("clipboard sequence")?;
        // The sequence sampled before closing must still identify our write
        // after closing; delivery relies on it without reading any contents.
        assert_eq!(clipboard_win::seq_num().map(std::num::NonZeroU32::get), Some(sequence));
        let html_format = raw::register_format("HTML Format").ok_or("HTML format")?.get();
        let rtf_format = raw::register_format("Rich Text Format").ok_or("RTF format")?.get();
        {
            let _open = Clipboard::new_attempts(20).map_err(|_| "open clipboard")?;
            let published: Vec<_> = raw::EnumFormats::new().collect();
            // Check the real publication order, including synthesized formats.
            for (supported, expected) in [
                (vec![html_format, rtf_format, CF_UNICODETEXT], html_format),
                (vec![rtf_format, CF_UNICODETEXT], rtf_format),
                (vec![CF_UNICODETEXT], CF_UNICODETEXT),
            ] {
                assert_eq!(
                    published.iter().find(|format| supported.contains(format)),
                    Some(&expected)
                );
            }
            let mut bytes = vec![];
            raw::get_vec(html_format, &mut bytes).map_err(|_| "HTML bytes")?;
            let header = std::str::from_utf8(&bytes).map_err(|_| "HTML UTF-8")?;
            let offset = |name: &str| -> Result<usize, String> {
                header
                    .lines()
                    .find_map(|line| line.strip_prefix(name))
                    .ok_or_else(|| format!("missing {name}"))?
                    .parse()
                    .map_err(|_| format!("invalid {name}"))
            };
            let (start, end) = (offset("StartFragment:")?, offset("EndFragment:")?);
            assert_eq!(bytes.get(start..end), Some(html.as_bytes()));
            assert!(offset("StartHTML:")? <= start && end <= offset("EndHTML:")?);
            assert!(offset("EndHTML:")? <= bytes.len());
            bytes.clear();
            raw::get_vec(rtf_format, &mut bytes).map_err(|_| "RTF bytes")?;
            assert!(bytes.starts_with(rtf.as_bytes()));
            bytes.clear();
            raw::get_string(&mut bytes).map_err(|_| "Unicode text")?;
            assert_eq!(bytes, text.as_bytes());
        }
        // A later key-only copy must remove stale rich formats.
        let next_sequence = write_windows(ClipboardContent {
            text: "[@Synthetic2026]",
            html: None,
            rtf: None,
        })?
        .ok_or("next clipboard sequence")?;
        assert_ne!(next_sequence, sequence);
        let _open = Clipboard::new_attempts(20).map_err(|_| "reopen clipboard")?;
        assert!(!raw::is_format_avail(html_format) && !raw::is_format_avail(rtf_format));
        let mut bytes = vec![];
        raw::get_string(&mut bytes).map_err(|_| "plain text")?;
        assert_eq!(bytes, b"[@Synthetic2026]");
        Ok(())
    }
}
