use crate::{FormatError, input::Prepared};
use ibis_core::contract::*;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case", deny_unknown_fields)]
enum Outputs {
    Refused,
    Rendered {
        text: String,
        html: String,
        rtf: String,
        // Preserve processor text before the adapter folds bibliography line breaks.
        text_parts: Option<Vec<String>>,
        coverage: Vec<usize>,
        spans: Vec<Span>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Span {
    item: Option<usize>,
    text: String,
}

pub(crate) fn refused(input: &Prepared) -> CitationResult {
    CitationResult::new(
        String::new(),
        None,
        None,
        DisplayTree::default(),
        input
            .outcomes
            .iter()
            .map(|outcome| match outcome {
                ItemOutcome::Rendered { item } => ItemOutcome::Refused {
                    item: *item,
                    error: CitationError::UnsupportedForm,
                },
                refused => *refused,
            })
            .collect(),
        input.losses.clone(),
        input.notices.clone(),
    )
    .with_warnings(input.warnings.clone())
}
pub(crate) fn result(
    json: &str,
    input: &Prepared,
    superscripts: &BTreeMap<char, String>,
) -> Result<CitationResult, FormatError> {
    let output: Outputs = serde_json::from_str(json).map_err(|_| FormatError::Runtime)?;
    let Outputs::Rendered {
        text,
        html,
        rtf,
        text_parts,
        coverage,
        spans,
    } = output
    else {
        return Ok(refused(input));
    };
    let candidates = input
        .outcomes
        .iter()
        .filter(|outcome| matches!(outcome, ItemOutcome::Rendered { .. }))
        .count();
    if candidates == 0 || coverage != (0..candidates).collect::<Vec<_>>() {
        return Ok(refused(input));
    }
    if text.trim().is_empty() || [&text, &html, &rtf].into_iter().any(|text| unsupported_controls(text)) {
        return Err(FormatError::Runtime);
    }
    let presentation = if let Some(parts) = &text_parts {
        if parts.is_empty()
            || parts.len() > 1000
            || parts.iter().any(|part| unsupported_controls(part))
            || parts.iter().map(|part| one_line(part)).collect::<Vec<_>>().join("\n") != text
        {
            return Err(FormatError::Runtime);
        }
        parts
            .iter()
            .map(|part| one_line(&escape_space_pairs(part)))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        escape_space_pairs(&text)
    };
    let runs = parse_html(&html)?;
    if runs.iter().any(|run| unsupported_controls(&run.text))
        || !(presentation_matches(&text, &runs, superscripts)
            || presentation_matches(&presentation, &runs, superscripts))
    {
        return Err(FormatError::Runtime);
    }
    let items: Vec<_> = input
        .outcomes
        .iter()
        .filter_map(|outcome| match outcome {
            ItemOutcome::Rendered { item } => Some(*item),
            _ => None,
        })
        .collect();
    if spans.iter().map(|span| span.text.as_str()).collect::<String>() != text
        || spans
            .iter()
            .any(|span| span.item.is_some_and(|item| item >= items.len()))
        || (0..items.len()).any(|item| !spans.iter().any(|span| span.item == Some(item)))
    {
        return Err(FormatError::Runtime);
    }
    // Project validated presentation equivalents onto the canonical text while
    // retaining emphasis. Clipboard HTML and RTF remain untouched.
    let canonical = canonical_runs(&text, &runs, superscripts);
    let mut offset = 0;
    let nodes = spans
        .into_iter()
        .map(|span| {
            let end = offset + span.text.len();
            let content = if let Some(runs) = &canonical {
                let mut position = 0;
                runs.iter()
                    .filter_map(|run| {
                        let start = position;
                        position += run.text.len();
                        let from = offset.max(start);
                        let to = end.min(position);
                        (from < to).then(|| DisplayRun {
                            text: run.text[from - start..to - start].into(),
                            ..run.clone()
                        })
                    })
                    .collect()
            } else {
                vec![DisplayRun {
                    text: span.text,
                    ..DisplayRun::default()
                }]
            };
            offset = end;
            DisplayNode {
                item: span.item.map(|item| items[item]),
                content,
            }
        })
        .collect();
    Ok(CitationResult::new(
        text,
        Some(html),
        Some(rtf),
        DisplayTree { nodes },
        input.outcomes.clone(),
        input.losses.clone(),
        input.notices.clone(),
    )
    .with_warnings(input.warnings.clone()))
}

/// Unicode paragraph/line separators and conventional text whitespace remain data.
pub(crate) fn unsupported_controls(text: &str) -> bool {
    text.chars()
        .any(|ch| ch.is_control() && !matches!(ch, '\t' | '\r' | '\n' | '\u{85}'))
}

// ECMAScript WhiteSpace + LineTerminator, exactly the pinned processor's \s.
// Rust's Unicode whitespace differs for NEL and the byte-order mark.
fn js_space(ch: char) -> bool {
    matches!(
        ch,
        '\t' | '\u{b}'
            | '\u{c}'
            | ' '
            | '\u{a0}'
            | '\u{feff}'
            | '\r'
            | '\n'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
    )
}

/// The bundled HTML text_escape replaces whitespace pairs, preserving their count.
fn escape_space_pairs(text: &str) -> String {
    let mut chars = text.chars().peekable();
    let mut result = String::with_capacity(text.len());
    while let Some(ch) = chars.next() {
        if js_space(ch) && chars.peek().is_some_and(|next| js_space(*next)) {
            chars.next();
            result.push_str("\u{a0} ");
        } else {
            result.push(ch);
        }
    }
    result
}

/// Exact counterpart to the adapter's /\s*\n\s*/g plus trim, before joining entries.
fn one_line(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(js_space) {
        result.push_str(&rest[..start]);
        rest = &rest[start..];
        let end = rest.find(|ch| !js_space(ch)).unwrap_or(rest.len());
        let spaces = &rest[..end];
        result.push_str(if spaces.contains('\n') { " " } else { spaces });
        rest = &rest[end..];
    }
    result.push_str(rest);
    result.trim_matches(js_space).to_owned()
}

/// Consume exact text, allowing only processor-known glyph presentation and
/// nonbreaking spaces. Do not fold case, compatibility characters or space counts.
fn presentation_matches(text: &str, runs: &[DisplayRun], superscripts: &BTreeMap<char, String>) -> bool {
    let mut shown = runs
        .iter()
        .flat_map(|run| run.text.chars().map(move |ch| (ch, run.script)))
        .peekable();
    for original in text.chars() {
        let Some(&(ch, script)) = shown.peek() else {
            return false;
        };
        if original == ch || matches!((original, ch), (' ', '\u{a0}') | ('\u{a0}', ' ')) {
            shown.next();
            continue;
        }
        let expansion = match script {
            Script::Superscript => superscripts.get(&original).cloned(),
            Script::Subscript => match original {
                '₀'..='₉' => char::from_u32(original as u32 - '₀' as u32 + '0' as u32).map(|ch| ch.to_string()),
                '₊' => Some("+".into()),
                '₋' => Some("−".into()),
                '₌' => Some("=".into()),
                '₍' => Some("(".into()),
                '₎' => Some(")".into()),
                _ => None,
            },
            Script::Normal => None,
        };
        let Some(expansion) = expansion else {
            return false;
        };
        for expected in expansion.chars() {
            if shown.next() != Some((expected, script)) {
                return false;
            }
        }
    }
    shown.next().is_none()
}

fn canonical_runs(text: &str, runs: &[DisplayRun], superscripts: &BTreeMap<char, String>) -> Option<Vec<DisplayRun>> {
    let mut shown = runs
        .iter()
        .flat_map(|run| run.text.chars().map(move |ch| (ch, run)))
        .peekable();
    let mut result: Vec<DisplayRun> = Vec::new();
    for original in text.chars() {
        let &(ch, run) = shown.peek()?;
        let script = if original == ch || js_space(original) && js_space(ch) {
            shown.next();
            run.script
        } else {
            let expansion = match run.script {
                Script::Superscript => superscripts.get(&original)?.clone(),
                Script::Subscript => match original {
                    '₀'..='₉' => char::from_u32(original as u32 - '₀' as u32 + '0' as u32)?.to_string(),
                    '₊' => "+".into(),
                    '₋' => "−".into(),
                    '₌' => "=".into(),
                    '₍' => "(".into(),
                    '₎' => ")".into(),
                    _ => return None,
                },
                Script::Normal => return None,
            };
            for expected in expansion.chars() {
                let (actual, style) = shown.next()?;
                if actual != expected || style.script != run.script {
                    return None;
                }
            }
            // The canonical character already carries its raised/lowered glyph.
            Script::Normal
        };
        if let Some(last) = result.last_mut().filter(|last| {
            last.italic == run.italic
                && last.bold == run.bold
                && last.script == script
                && last.small_caps == run.small_caps
                && last.underline == run.underline
        }) {
            last.text.push(original);
        } else {
            result.push(DisplayRun {
                text: original.to_string(),
                italic: run.italic,
                bold: run.bold,
                script,
                small_caps: run.small_caps,
                underline: run.underline,
            });
        }
    }
    shown.next().is_none().then_some(result)
}

/// Only processor-generated passive tags are permitted in canonical clipboard HTML.
/// Output is rejected rather than silently changing a citation's visible text.
fn parse_html(html: &str) -> Result<Vec<DisplayRun>, FormatError> {
    let mut runs = Vec::new();
    let mut stack = vec![DisplayRun::default()];
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
                let (name, entry_start) = names.pop().ok_or(FormatError::Runtime)?;
                if name != closing {
                    return Err(FormatError::Runtime);
                }
                if let Some(start) = entry_start {
                    trim_entry(&mut runs[start..]);
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
                                ("font-variant", "small-caps") => style.small_caps = true,
                                ("font-variant", "normal") => style.small_caps = false,
                                ("text-decoration", "underline") => style.underline = true,
                                ("text-decoration", "none") => style.underline = false,
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
                names.push((
                    tag.split_whitespace().next().ok_or(FormatError::Runtime)?,
                    (tag == "div class=\"csl-entry\"").then_some(runs.len()),
                ));
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

// Bibliography text trims each entry; HTML may keep a space just inside its div.
// Ignore only those entry boundaries, preserving all interior text and spacing.
fn trim_entry(runs: &mut [DisplayRun]) {
    for run in runs.iter_mut() {
        run.text = run.text.trim_start_matches(js_space).to_owned();
        if !run.text.is_empty() {
            break;
        }
    }
    for run in runs.iter_mut().rev() {
        run.text = run.text.trim_end_matches(js_space).to_owned();
        if !run.text.is_empty() {
            break;
        }
    }
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
    #[test]
    fn passive_style_resets_and_canonical_projection_preserve_every_display_flag() {
        let runs = parse_html("<span style=\"font-variant: small-caps;text-decoration:underline\">Caps<span style=\"font-variant:normal;text-decoration:none\">Plain</span>Tail</span>").unwrap();
        assert!(runs[0].small_caps && runs[0].underline);
        assert!(!runs[1].small_caps && !runs[1].underline);
        assert!(runs[2].small_caps && runs[2].underline);
        assert_eq!(canonical_runs("CapsPlainTail", &runs, &BTreeMap::new()), Some(runs));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod review_tests {
    use super::*;
    use serde_json::json;
    fn prepared() -> Prepared {
        Prepared {
            json: String::new(),
            outcomes: vec![ItemOutcome::Rendered { item: 0 }],
            losses: vec![],
            notices: vec![],
            warnings: vec![],
            empty: false,
        }
    }
    fn output(text: &str, html: &str, rtf: &str) -> Result<CitationResult, FormatError> {
        result(
            &json!({"status":"rendered","coverage":[0],"spans":[{"item":0,"text":text}],"text":text,"html":html,"rtf":rtf}).to_string(),
            &prepared(),
            &BTreeMap::from([('²', "2".into()), ('™', "TM".into())]),
        )
    }
    #[test]
    fn presentation_equivalents_preserve_canonical_representations() {
        for (text, html) in [
            ("m²", "m<sup>2</sup>"),
            ("™", "<sup>TM</sup>"),
            ("CO2", "CO<sub>2</sub>"),
            ("Synthetic  area", "Synthetic \u{a0}area"),
            ("Synthetic\t\tarea", "Synthetic\u{a0} area"),
            ("Synthetic \tarea", "Synthetic\u{a0} area"),
            ("Synthetic\u{3000}\u{3000}area", "Synthetic\u{a0} area"),
            ("Synthetic\u{2028}one\u{2029}two", "Synthetic\u{2028}one\u{2029}two"),
        ] {
            let result = output(text, html, "{\\rtf1 canonical}").unwrap();
            assert_eq!(result.text, text);
            assert_eq!(result.html.as_deref(), Some(html));
            assert_eq!(result.rtf.as_deref(), Some("{\\rtf1 canonical}"));
        }
    }
    #[test]
    fn visible_text_changes_are_not_presentation_equivalents() {
        for (text, html) in [
            ("m²", "m2"),
            ("m²", "m<sub>2</sub>"),
            ("m²", "m<sup>3</sup>"),
            ("m2", "m<sup>3</sup>"),
            ("Synthetic area", "Synthetic  area"),
            ("Synthetic  area", "Synthetic area"),
            ("Word", "word"),
            ("Ａ", "A"),
        ] {
            assert!(output(text, html, "{\\rtf1 canonical}").is_err());
        }
    }
    #[test]
    fn bibliography_entry_boundaries_allow_only_outer_whitespace() {
        let html = "<div class=\"csl-entry\"> <i>avery</i> Taylor </div>";
        assert!(output("avery Taylor", html, "canonical").is_ok());
        assert!(output("averyTaylor", html, "canonical").is_err());
        assert!(output("avery  Taylor", html, "canonical").is_err());
        // Inline citation spacing is still exact.
        assert!(output("avery Taylor", " <i>avery</i> Taylor", "canonical").is_err());
    }
    #[test]
    fn bibliography_line_folding_requires_original_text_evidence() {
        let json = json!({"status":"rendered","coverage":[0],"spans":[{"item":0,"text":"Synthetic area"}],"text":"Synthetic area", "html":"Synthetic\u{a0} area", "rtf":"{\\rtf1 canonical}", "text_parts":["Synthetic\r\narea\n"]});
        let rendered = result(&json.to_string(), &prepared(), &BTreeMap::new()).unwrap();
        assert_eq!(rendered.text, "Synthetic area");
        assert_eq!(rendered.html.as_deref(), Some("Synthetic\u{a0} area"));
        let mut wrong = json;
        wrong["text_parts"] = json!(["Different\r\narea\n"]);
        assert!(result(&wrong.to_string(), &prepared(), &BTreeMap::new()).is_err());
        assert!(output("Synthetic area", "Synthetic\u{a0} area", "canonical").is_err());
    }

    #[test]
    fn incomplete_or_duplicate_representation_coverage_never_exposes_candidates() {
        for coverage in [vec![], vec![1], vec![0, 0]] {
            let output = json!({"status":"rendered","coverage":coverage,"spans":[{"item":0,"text":"Synthetic"}],"text":"Synthetic","html":"Synthetic","rtf":"canonical"});
            let refused = result(&output.to_string(), &prepared(), &BTreeMap::new()).unwrap();
            assert!(refused.copy_content(1).is_none());
            assert!(refused.text.is_empty());
            assert_eq!(
                refused.outcomes,
                vec![ItemOutcome::Refused {
                    item: 0,
                    error: CitationError::UnsupportedForm
                }]
            );
        }
    }

    #[test]
    fn nul_in_any_canonical_representation_is_rejected() {
        for (text, html, rtf) in [
            ("Synthetic\0text", "Synthetic\0text", "{\\rtf1 canonical}"),
            ("Synthetic text", "Synthetic\0text", "{\\rtf1 canonical}"),
            ("Synthetic text", "Synthetic text", "{\\rtf1\0canonical}"),
        ] {
            assert!(output(text, html, rtf).is_err());
        }
    }
}
