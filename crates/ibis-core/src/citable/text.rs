//! Decode source text once, keeping literal characters separate from markup.
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TextPart {
    Text(String),
    Markup(String),
}

#[derive(Debug)]
pub(super) struct Projection {
    pub parts: Vec<TextPart>,
    pub plain: String,
    pub attributes: String,
}

impl Projection {
    pub fn new(source: &str) -> Self {
        let mut parts = Vec::new();
        let mut rest = source;
        let mut attributes = String::new();
        while !rest.is_empty() {
            if let Some(length) = crate::model::rich_text_tag_len(rest) {
                let tag = &rest[..length];
                attributes.push(' ');
                attributes.push_str(&decode_entities(tag));
                let canonical = if let Some(attribute) = tag.strip_prefix("<span") {
                    let attribute = attribute.trim_start();
                    if attribute.starts_with("class=\"nocase\"") {
                        "<span class=\"nocase\">"
                    } else if attribute.starts_with("class=\"nodecor\"") {
                        "<span class=\"nodecor\">"
                    } else {
                        "<span style=\"font-variant:small-caps;\">"
                    }
                } else {
                    tag
                };
                parts.push(TextPart::Markup(canonical.into()));
                rest = &rest[length..];
            } else {
                let first = rest.chars().next().map_or(0, char::len_utf8);
                let end = rest[first..].find('<').map_or(rest.len(), |offset| first + offset);
                let decoded = decode_entities(&rest[..end]);
                if let Some(TextPart::Text(text)) = parts.last_mut() {
                    text.push_str(&decoded);
                } else {
                    parts.push(TextPart::Text(decoded));
                }
                rest = &rest[end..];
            }
        }
        // Whitespace state belongs to the whole field, including across tags.
        let mut visible = false;
        let mut line_break = false;
        for part in &mut parts {
            let TextPart::Text(text) = part else { continue };
            let mut normalized = String::with_capacity(text.len());
            for ch in text.nfc().filter(|ch| !removed(*ch)) {
                let is_break = matches!(ch, '\r' | '\n' | '\u{85}' | '\u{2028}' | '\u{2029}');
                if is_break {
                    if !line_break && visible {
                        normalized.push(' ');
                    }
                } else {
                    normalized.push(if ch.is_control() && ch != '\t' { ' ' } else { ch });
                    visible = true;
                }
                line_break = is_break;
            }
            *text = normalized;
        }
        if line_break {
            for part in parts.iter_mut().rev() {
                if let TextPart::Text(text) = part
                    && !text.is_empty()
                {
                    text.pop();
                    break;
                }
            }
        }
        let plain = parts
            .iter()
            .filter_map(|part| match part {
                TextPart::Text(text) => Some(text.as_str()),
                TextPart::Markup(_) => None,
            })
            .collect();
        let attributes = attributes.nfc().filter(|ch| !removed(*ch)).collect();
        Self {
            parts,
            plain,
            attributes,
        }
    }
}

/// Canonical CSL-JSON text. Processors such as citeproc-js and pandoc print
/// strings literally except for rich-text tags, so only what some reader could
/// decode is escaped: an ampersand starting a character reference, and a
/// literal tag. Neither becomes markup or another character on reimport.
pub(super) fn encode(parts: &[TextPart]) -> String {
    parts
        .iter()
        .map(|part| match part {
            TextPart::Markup(tag) => tag.clone(),
            TextPart::Text(text) => encode_literal(text),
        })
        .collect()
}

pub(super) fn encode_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(['&', '<']) {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let length = if let Some(after) = rest.strip_prefix('&') {
            // HTML's grammar, wider than this decoder's: `&sol;`, `&#X2F;` and
            // `&#47` cannot turn into a slash in any reader either.
            let name = after.bytes().take_while(u8::is_ascii_alphanumeric).count();
            let reference = after.starts_with('#')
                || after.starts_with(|c: char| c.is_ascii_alphabetic()) && after[name..].starts_with(';');
            out.push_str(if reference { "&amp;" } else { "&" });
            1
        } else if let Some(length) = crate::model::rich_text_tag_len(rest) {
            out.push_str(
                &rest[..length]
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
                    .replace('"', "&quot;"),
            );
            length
        } else {
            out.push('<');
            1
        };
        rest = &rest[length..];
    }
    out.push_str(rest);
    out
}

pub(crate) fn bidi_control(ch: char) -> bool {
    matches!(ch, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{206f}')
}

fn removed(ch: char) -> bool {
    bidi_control(ch) || (!ch.is_control() && crate::model::is_hidden(ch))
}

/// Inspect every encoding layer without changing the text an exporter prints.
/// Refuse values that do not stabilize within the bound, including invalid UTF-8.
pub(super) fn decode_for_privacy(source: &str) -> Option<String> {
    let mut value = source.to_owned();
    for _ in 0..8 {
        let entities = decode_entities(&value);
        let mut rest = entities.as_bytes();
        let mut bytes = Vec::with_capacity(rest.len());
        while let Some((&byte, tail)) = rest.split_first() {
            if let [b'%', high, low, ..] = rest
                && let Some((high, low)) = char::from(*high).to_digit(16).zip(char::from(*low).to_digit(16))
            {
                bytes.push((high * 16 + low) as u8);
                rest = &rest[3..];
            } else {
                bytes.push(byte);
                rest = tail;
            }
        }
        let decoded: String = String::from_utf8(bytes)
            .ok()?
            .nfc()
            .filter(|ch| !removed(*ch))
            .collect();
        if decoded == value {
            return Some(value);
        }
        value = decoded;
    }
    None
}

/// The source entity grammar, shared by projections and plain record summaries.
/// Decoded ampersands are never scanned again.
pub(crate) fn decode_entities(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let entity = rest
            .as_bytes()
            .iter()
            .take(12)
            .position(|byte| *byte == b';')
            .and_then(|end| {
                let name = &rest[1..end];
                let decoded = match name {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some('\u{a0}'),
                    // HTML's numeric references: either `x`, any leading zeros.
                    _ => name
                        .strip_prefix("#x")
                        .or_else(|| name.strip_prefix("#X"))
                        .filter(|hex| !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()))
                        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                        .or_else(|| {
                            name.strip_prefix('#')
                                .filter(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
                                .and_then(|digits| digits.parse().ok())
                        })
                        .and_then(char::from_u32),
                }?;
                Some((end + 1, decoded))
            });
        if let Some((length, decoded)) = entity {
            out.push(decoded);
            rest = &rest[length..];
        } else {
            out.push('&');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}
