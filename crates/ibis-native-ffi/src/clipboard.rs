use ibis_core::contract::ClipboardContent;
use objc2_app_kit::NSPasteboard;
use objc2_foundation::{NSData, NSString};

/// The UTF-8 HTML representation used by macOS clipboard readers.
pub fn macos_html(fragment: &str) -> String {
    // Without the charset declaration AppKit may guess a legacy encoding.
    format!(
        r#"<html><head><meta http-equiv="content-type" content="text/html; charset=utf-8"></head><body>{fragment}</body></html>"#
    )
}

/// Publish the engine's representations and prove the pasteboard is still ours.
/// Returns no count if another process changed ownership during the write.
pub fn publish_macos(board: &NSPasteboard, content: ClipboardContent<'_>) -> Result<Option<isize>, String> {
    let failed = || "The clipboard could not be written.".to_owned();
    let owned_count = board.clearContents();
    if let Some(html) = content.html
        && !board.setString_forType(
            &NSString::from_str(&macos_html(html)),
            &NSString::from_str("public.html"),
        )
    {
        return Err(failed());
    }
    if let Some(rtf) = content.rtf
        && !board.setData_forType(
            Some(&NSData::with_bytes(rtf.as_bytes())),
            &NSString::from_str("public.rtf"),
        )
    {
        return Err(failed());
    }
    if !board.setString_forType(
        &NSString::from_str(content.text),
        &NSString::from_str("public.utf8-plain-text"),
    ) {
        return Err(failed());
    }
    let written_count = board.changeCount();
    Ok((owned_count == written_count).then_some(written_count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shares_the_same_text_in_every_native_flavor() -> Result<(), String> {
        let board = NSPasteboard::pasteboardWithUniqueName();
        let text = "(García, 2021)";
        let rtf = r"{\rtf1 a synthetic citation}";
        let content = ClipboardContent {
            text,
            html: Some("<i>(García, 2021)</i>"),
            rtf: Some(rtf),
        };
        assert_eq!(publish_macos(&board, content)?, Some(board.changeCount()));
        assert_eq!(
            board
                .stringForType(&NSString::from_str("public.utf8-plain-text"))
                .map(|value| value.to_string()),
            Some(text.to_owned())
        );
        let html = board
            .stringForType(&NSString::from_str("public.html"))
            .map(|value| value.to_string());
        assert!(html.is_some_and(|value| value.contains("charset=utf-8") && value.contains(text)));
        let actual = board
            .dataForType(&NSString::from_str("public.rtf"))
            .map(|value| value.to_vec());
        assert_eq!(actual.as_deref(), Some(rtf.as_bytes()));
        Ok(())
    }
}
