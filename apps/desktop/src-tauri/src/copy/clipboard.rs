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
pub(crate) fn write(app: &tauri::AppHandle, content: ClipboardContent<'_>) -> Result<Option<u32>, String> {
    use tauri::Manager;
    app.state::<crate::paste::LastTarget>().write_copy(
        content.text,
        |id| app.state::<crate::destination::AppMemory>().paste(id),
        || publish_macos(&objc2_app_kit::NSPasteboard::generalPasteboard(), content),
    )
}

/// Return the count immediately after our last format write. A replacement
/// during publication invalidates the paste proof, even if writing succeeded.
#[cfg(target_os = "macos")]
fn publish_macos(board: &objc2_app_kit::NSPasteboard, content: ClipboardContent<'_>) -> Result<Option<isize>, String> {
    use objc2_foundation::{NSData, NSString};
    let owned_count = board.clearContents();
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
    let written_count = board.changeCount();
    Ok((owned_count == written_count).then_some(written_count))
}

/// One clipboard write, retried briefly while another app holds it.
#[cfg(target_os = "windows")]
pub(crate) fn write(app: &tauri::AppHandle, content: ClipboardContent<'_>) -> Result<Option<u32>, String> {
    use tauri::Manager;
    let owner = app
        .get_webview_window("main")
        .and_then(|window| window.hwnd().ok())
        .ok_or_else(|| FAILED.to_string())?;
    write_windows(owner.0, content)
}

#[cfg(target_os = "windows")]
fn write_windows(owner: clipboard_win::types::HWND, content: ClipboardContent<'_>) -> Result<Option<u32>, String> {
    use clipboard_win::{
        Clipboard,
        formats::{CF_UNICODETEXT, Html},
        options::NoClear,
        raw,
    };
    // Keep another Ibis copy from taking the same HWND's ownership between
    // closing the write and proving its final sequence.
    static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _writing = WRITING.lock().map_err(|_| FAILED.to_string())?;
    // About 100 ms in all: 20 retries, 5 ms apart.
    let open = Clipboard::new_attempts_for(owner, 20).map_err(|_| FAILED.to_string())?;
    raw::empty().map_err(|_| FAILED.to_string())?;
    let mut formats = Vec::with_capacity(3);
    for (flavor, value) in flavors(&content) {
        let (format, written) = match flavor {
            Flavor::Text => (CF_UNICODETEXT, raw::set_string_with(value, NoClear)),
            Flavor::Html => {
                let html = Html::new().ok_or_else(|| FAILED.to_string())?;
                (html.code(), raw::set_html(html.code(), value))
            }
            Flavor::Rtf => {
                let rtf = raw::register_format("Rich Text Format").ok_or_else(|| FAILED.to_string())?;
                (rtf.get(), raw::set_without_clear(rtf.get(), value.as_bytes()))
            }
        };
        written.map_err(|_| FAILED.to_string())?;
        formats.push(format);
    }
    // Each SetClipboardData advances the sequence; CloseClipboard can advance
    // it again when Windows adds synthesized text formats. Only the final
    // sequence identifies the copy seen by a recipient.
    let written = WindowsWrite {
        formats,
        sequence: clipboard_win::seq_num().map(std::num::NonZeroU32::get),
    };
    drop(open);
    Ok(windows_sequence(owner, &written))
}

#[cfg(target_os = "windows")]
fn windows_sequence(owner: clipboard_win::types::HWND, written: &WindowsWrite) -> Option<u32> {
    let owner = std::ptr::NonNull::new(owner)?;
    // Reopening without emptying leaves ownership intact and prevents a
    // foreign write during this proof. If another app wins the close/reopen
    // race, copy succeeds but automatic paste has no receipt. Never read data
    // (including a potentially blocked foreign delayed-rendering format).
    let _open = clipboard_win::Clipboard::new().ok()?;
    if clipboard_win::get_owner() != Some(owner) {
        return None;
    }
    let completed = WindowsWrite::capture();
    written.completed_sequence(completed.sequence, &completed.formats)
}

#[cfg(any(target_os = "windows", test))]
struct WindowsWrite {
    sequence: Option<u32>,
    formats: Vec<u32>,
}

#[cfg(any(target_os = "windows", test))]
impl WindowsWrite {
    #[cfg(target_os = "windows")]
    fn capture() -> Self {
        Self {
            formats: clipboard_win::EnumFormats::new().collect(),
            sequence: clipboard_win::seq_num().map(std::num::NonZeroU32::get),
        }
    }

    fn completed_sequence(&self, sequence: Option<u32>, formats: &[u32]) -> Option<u32> {
        // Closing a Unicode-text write can add CF_TEXT (1), CF_OEMTEXT (7)
        // and CF_LOCALE (16), each advancing the sequence. Account only for
        // these native additions, never a fixed increment per copy.
        if self.formats.iter().any(|format| !formats.contains(format)) {
            return None;
        }
        let mut synthesized = 0;
        for format in formats.iter().filter(|format| !self.formats.contains(format)) {
            if !matches!(format, 1 | 7 | 16) {
                return None;
            }
            synthesized += 1;
        }
        let expected = self.sequence?.wrapping_add(synthesized);
        // Ownership alone is insufficient: SetClipboardData without Empty
        // can overwrite a format while keeping our HWND as owner. Such a
        // write advances the sequence without adding a synthesized format.
        // Unknown Windows close behavior also refuses paste, without reading
        // any potentially foreign clipboard data.
        (expected != 0 && sequence == Some(expected)).then_some(expected)
    }
}

/// Text and HTML; Linux apps read HTML for formatting and rarely RTF.
#[cfg(target_os = "linux")]
pub(crate) fn write(app: &tauri::AppHandle, content: ClipboardContent<'_>) -> Result<Option<u32>, String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    let html = flavors(&content)
        .into_iter()
        .find_map(|(flavor, value)| (flavor == Flavor::Html).then_some(value));
    match html {
        Some(html) => app.clipboard().write_html(html, Some(content.text)),
        None => app.clipboard().write_text(content.text),
    }
    .map(|()| None)
    .map_err(|_| FAILED.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_clipboard_receipt_accounts_for_format_writes_and_synthesis_on_close() {
        for (formats, sequence) in [(vec![13], 19_u32), (vec![49_152, 49_153, 13], 21)] {
            // EmptyClipboard, then one SetClipboardData per representation.
            let written = WindowsWrite {
                sequence: Some(sequence),
                formats: formats.clone(),
            };
            for (synthesized, increments) in [(vec![], 0), (vec![1], 1), (vec![1, 7, 16], 3)] {
                let completed: Vec<_> = formats.iter().chain(&synthesized).copied().collect();
                let final_sequence = sequence + increments;
                assert_eq!(
                    written.completed_sequence(Some(final_sequence), &completed),
                    Some(final_sequence)
                );
            }
        }
        let wrapping = WindowsWrite {
            sequence: Some(u32::MAX - 1),
            formats: vec![13],
        };
        assert_eq!(wrapping.completed_sequence(Some(1), &[13, 1, 7, 16]), Some(1));
        assert_eq!(wrapping.completed_sequence(None, &[13, 1]), None);
    }

    #[test]
    fn windows_clipboard_receipt_rejects_foreign_changes_even_when_the_owner_stays() {
        let written = WindowsWrite {
            sequence: Some(4),
            formats: vec![49_152, 49_153, 13],
        };
        let completed = [49_152, 49_153, 13, 1, 7, 16];
        assert_eq!(written.completed_sequence(Some(7), &completed), Some(7));
        // A NoClear overwrite of Unicode or synthesized ANSI text advances
        // the sequence without changing the set of formats or the owner.
        assert_eq!(written.completed_sequence(Some(8), &completed), None);
        assert_eq!(written.completed_sequence(Some(4), &completed), None);
        assert_eq!(written.completed_sequence(None, &completed), None);
        // A bonus foreign format must not be counted as Windows synthesis.
        assert_eq!(
            written.completed_sequence(Some(8), &[49_152, 49_153, 13, 1, 7, 16, 49_154]),
            None
        );
        // A removed representation cannot pass even with a matching count.
        assert_eq!(written.completed_sequence(Some(7), &[49_152, 13, 1, 7, 16]), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "needs a macOS desktop pasteboard server; uses only a private disposable pasteboard"]
    fn paste_snapshot_covers_all_formats_and_detects_replacement_without_reading_contents() {
        // A private disposable board; never touches the person's clipboard.
        let board = objc2_app_kit::NSPasteboard::pasteboardWithUniqueName();
        let result = publish_macos(
            &board,
            ClipboardContent {
                text: "[@invented]",
                html: Some("<i>Invented</i>"),
                rtf: Some("{\\rtf1 Invented}"),
            },
        );
        let snapshot = board.changeCount();
        board.clearContents();
        let replaced = board.changeCount();
        assert_eq!(result, Ok(Some(snapshot)));
        assert_ne!(snapshot, replaced);
    }

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
        use clipboard_win::{
            Clipboard,
            formats::{CF_TEXT, CF_UNICODETEXT},
            raw,
        };
        // A stable native HWND for this clipboard-only test; no UI is shown.
        let owner = winsafe::HWND::GetDesktopWindow();
        let text = "Ŝynthetic 山田";
        let html = "<i>Ŝynthetic 山田</i>";
        let rtf = "{\\rtf1\\ansi\\uc1 {\\i \\u348?ynthetic \\u23665?\\u30000?}}";
        let sequence = write_windows(
            owner.ptr(),
            ClipboardContent {
                text,
                html: Some(html),
                rtf: Some(rtf),
            },
        )?
        .ok_or("clipboard sequence")?;
        // The receipt must match the completed write, including any sequence
        // increments from synthesized formats on close. Do not assume a fixed
        // increment per copy or a fixed count of synthesized formats.
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
        assert_eq!(clipboard_win::seq_num().map(std::num::NonZeroU32::get), Some(sequence));
        // Other apps may overwrite data without EmptyClipboard, keeping our
        // owner HWND. Neither Unicode nor synthesized ANSI replacements may
        // become an automatic-paste baseline.
        let foreign_unicode: Vec<_> = "Foreign citation"
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect();
        for (format, bytes) in [
            (CF_UNICODETEXT, foreign_unicode.as_slice()),
            (CF_TEXT, b"Foreign citation\0".as_slice()),
        ] {
            let written = {
                let _open = Clipboard::new_attempts(20).map_err(|_| "snapshot clipboard")?;
                WindowsWrite::capture()
            };
            {
                let _open = Clipboard::new_attempts(20).map_err(|_| "overwrite clipboard")?;
                raw::set_without_clear(format, bytes).map_err(|_| "foreign overwrite")?;
            }
            assert_eq!(
                clipboard_win::get_owner().map(std::ptr::NonNull::as_ptr),
                Some(owner.ptr())
            );
            assert_eq!(windows_sequence(owner.ptr(), &written), None);
        }
        // A later key-only copy must remove stale rich formats.
        let next_sequence = write_windows(
            owner.ptr(),
            ClipboardContent {
                text: "[@Synthetic2026]",
                html: None,
                rtf: None,
            },
        )?
        .ok_or("next clipboard sequence")?;
        assert_ne!(next_sequence, sequence);
        assert_eq!(
            clipboard_win::seq_num().map(std::num::NonZeroU32::get),
            Some(next_sequence)
        );
        let open = Clipboard::new_attempts(20).map_err(|_| "reopen clipboard")?;
        assert!(!raw::is_format_avail(html_format) && !raw::is_format_avail(rtf_format));
        let mut bytes = vec![];
        raw::get_string(&mut bytes).map_err(|_| "plain text")?;
        assert_eq!(bytes, b"[@Synthetic2026]");
        let written = WindowsWrite::capture();
        // Simulate another app winning the close/reopen race. Its sequence
        // must never become this copy's delivery baseline, even if its text
        // happens to match; an unavailable/NULL owner is not proof either.
        raw::empty().map_err(|_| "replace clipboard")?;
        raw::set_string("[@Synthetic2026]").map_err(|_| "foreign text")?;
        drop(open);
        assert_eq!(windows_sequence(owner.ptr(), &written), None);
        assert_eq!(windows_sequence(std::ptr::null_mut(), &written), None);
        Ok(())
    }
}
