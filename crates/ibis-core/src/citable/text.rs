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

/// Canonical CSL encoding; literal tags cannot become markup on reimport.
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
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Keep ordinary identifier punctuation verbatim, escaping text that another
/// projection could otherwise interpret as an entity or a tag.
pub(super) fn encode_plain(text: &str) -> String {
    let text = if decode_entities(text) == text {
        text.to_owned()
    } else {
        text.replace('&', "&amp;")
    };
    text.replace('<', "&lt;")
}

pub(crate) fn bidi_control(ch: char) -> bool {
    matches!(ch, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{206f}')
}

fn removed(ch: char) -> bool {
    bidi_control(ch) || (!ch.is_control() && crate::model::is_hidden(ch))
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
                    _ => name
                        .strip_prefix("#x")
                        .filter(|hex| !hex.is_empty() && hex.len() <= 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
                        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                        .or_else(|| {
                            name.strip_prefix('#')
                                .filter(|digits| {
                                    !digits.is_empty()
                                        && digits.len() <= 7
                                        && digits.bytes().all(|b| b.is_ascii_digit())
                                })
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
