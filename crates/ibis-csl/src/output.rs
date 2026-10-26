use crate::{FormatError, input::Prepared};
use ibis_core::contract::*;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Outputs {
    text: String,
    html: String,
    rtf: String,
}

pub(crate) fn refused(input: &Prepared) -> CitationResult {
    CitationResult::new(
        String::new(),
        None,
        None,
        DisplayTree::default(),
        input.outcomes.clone(),
        input.losses.clone(),
        input.notices.clone(),
    )
}
pub(crate) fn result(json: &str, input: &Prepared) -> Result<CitationResult, FormatError> {
    let output: Outputs = serde_json::from_str(json).map_err(|_| FormatError::Runtime)?;
    if output.text.trim().is_empty() {
        return Err(FormatError::Runtime);
    }
    let runs = parse_html(&output.html)?;
    let display_text: String = runs.iter().map(|run| run.text.as_str()).collect();
    if display_text.trim() != output.text.trim() {
        return Err(FormatError::Runtime);
    }
    Ok(CitationResult::new(
        output.text,
        Some(output.html),
        Some(output.rtf),
        DisplayTree {
            nodes: vec![DisplayNode {
                item: None,
                content: runs,
            }],
        },
        input.outcomes.clone(),
        input.losses.clone(),
        input.notices.clone(),
    ))
}

/// Only processor-generated passive tags are permitted in canonical clipboard HTML.
/// Output is rejected rather than silently changing a citation's visible text.
fn parse_html(html: &str) -> Result<RichText, FormatError> {
    let mut runs = Vec::new();
    let mut stack = vec![TextRun::default()];
    let mut names = Vec::new();
    let mut rest = html;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('<') {
            let end = after.find('>').ok_or(FormatError::Runtime)?;
            let tag = &after[..end];
            if tag.len() > 256 {
                return Err(FormatError::Runtime);
            }
            if let Some(closing) = tag.strip_prefix('/') {
                if !matches!(tag, "/i" | "/b" | "/sup" | "/sub" | "/span" | "/div") || stack.len() == 1 {
                    return Err(FormatError::Runtime);
                }
                if names.pop() != Some(closing) {
                    return Err(FormatError::Runtime);
                }
                stack.pop();
            } else {
                let mut style = stack.last().cloned().ok_or(FormatError::Runtime)?;
                style.text.clear();
                match tag {
                    "i" => style.italic = true,
                    "b" => style.bold = true,
                    "sup" => style.script = Script::Superscript,
                    "sub" => style.script = Script::Subscript,
                    "span" | "span class=\"nocase\"" => {}
                    value if value.starts_with("span style=\"") && value.ends_with('"') => {
                        let css = &value[12..value.len() - 1];
                        for declaration in css.split(';').filter(|value| !value.trim().is_empty()) {
                            let Some((key, value)) = declaration.split_once(':') else {
                                return Err(FormatError::Runtime);
                            };
                            match (key.trim(), value.trim()) {
                                ("font-style", "italic") => style.italic = true,
                                ("font-style", "normal") => style.italic = false,
                                ("font-weight", "bold") => style.bold = true,
                                ("font-weight", "normal") => style.bold = false,
                                ("vertical-align", "sup") => style.script = Script::Superscript,
                                ("vertical-align", "sub") => style.script = Script::Subscript,
                                ("vertical-align", "baseline") => style.script = Script::Normal,
                                ("font-variant", "small-caps" | "normal")
                                | ("text-decoration", "none" | "underline") => {}
                                _ => return Err(FormatError::Runtime),
                            }
                        }
                    }
                    "div class=\"csl-entry\"" | "div class=\"csl-block\"" | "div class=\"csl-indent\"" => {}
                    _ => return Err(FormatError::Runtime),
                }
                if stack.len() >= 32 {
                    return Err(FormatError::ResourceLimit);
                }
                names.push(tag.split_whitespace().next().ok_or(FormatError::Runtime)?);
                stack.push(style);
            }
            rest = &after[end + 1..];
        } else {
            let end = rest.find('<').unwrap_or(rest.len());
            let mut run = stack.last().cloned().ok_or(FormatError::Runtime)?;
            run.text = decode_entities(&rest[..end])?;
            runs.push(run);
            rest = &rest[end..];
        }
    }
    if stack.len() != 1 {
        return Err(FormatError::Runtime);
    }
    Ok(runs)
}
fn decode_entities(text: &str) -> Result<String, FormatError> {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find(';') else {
            return Err(FormatError::Runtime);
        };
        let entity = &after[..end];
        let ch = match entity {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            "nbsp" => '\u{a0}',
            value if value.starts_with("#x") => u32::from_str_radix(&value[2..], 16)
                .ok()
                .and_then(char::from_u32)
                .ok_or(FormatError::Runtime)?,
            value if value.starts_with('#') => value[1..]
                .parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .ok_or(FormatError::Runtime)?,
            _ => return Err(FormatError::Runtime),
        };
        out.push(ch);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn rejects_malformed_or_active_html() {
        for html in [
            "<i>x</b>",
            "<img src=x onerror=bad()>",
            "<span onclick=bad()>x</span>",
            "<span style=\"background:url(bad)\">x</span>",
            "<i>unclosed",
        ] {
            assert!(parse_html(html).is_err());
        }
    }
    #[test]
    fn decodes_entities_once_and_preserves_allowed_styles() {
        let runs = parse_html("<i>例 &#38; &#x1F600; &amp;lt;</i>").unwrap();
        assert_eq!(runs[0].text, "例 & 😀 &lt;");
        assert!(runs[0].italic);
    }
}
