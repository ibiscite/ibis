use crate::{FormatError, input::Prepared};
use ibis_core::citable::PublicUrl;
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
        /// Cites whose citation reads the same without their locator.
        #[serde(default)]
        unprinted_locators: Vec<usize>,
        /// A bibliography's layout, from the processor.
        #[serde(default)]
        layout: Option<Layout>,
    },
}

/// How a style lays out its bibliography, written into the copied HTML and
/// RTF exactly as Zotero writes it (`Zotero.Cite.makeFormattedBibliography`
/// and `getBibliographyFormatParameters`), so a pasted bibliography keeps
/// its hanging indent and spacing.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Layout {
    hanging_indent: bool,
    line_spacing: f64,
    entry_spacing: f64,
    second_field_align: Option<Align>,
    max_offset: u32,
}

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Align {
    Flush,
    Margin,
}

impl Layout {
    fn checked(self) -> Result<Self, FormatError> {
        let spacing = |value: f64| value.is_finite() && (0.0..=10.0).contains(&value);
        (spacing(self.line_spacing) && spacing(self.entry_spacing) && self.max_offset <= 1000)
            .then_some(self)
            .ok_or(FormatError::Runtime)
    }

    /// Zotero's inline styles around entries whose only tags are the processor's.
    fn html(&self, entries: &str) -> String {
        let margins = entries.contains("<div class=\"csl-left-margin\">");
        let aligned = self.second_field_align.is_some();
        // Zotero reads both spacings as whole numbers, with a floor on line height.
        let line = self.line_spacing.trunc().max(1.35);
        let entry = self.entry_spacing.trunc();
        let offset = (f64::from(self.max_offset) - 2.0).max(1.0);
        let hanging = if self.hanging_indent {
            "padding-left: 2em; text-indent:-2em;"
        } else {
            ""
        };
        let mut body = format!("line-height: {line}; ");
        if self.hanging_indent && !margins {
            body.push_str("margin-left: 2em; text-indent:-2em;");
        }
        let count = entries.matches("<div class=\"csl-entry\">").count();
        let mut out = format!("<div class=\"csl-bib-body\" style=\"{body}\">");
        for (index, part) in entries.split("<div class=\"csl-entry\">").enumerate() {
            if index > 0 {
                let mut style = String::from(if margins { "clear: left; " } else { "" });
                if entry != 0.0 && index != count {
                    style.push_str(&format!("margin-bottom: {entry}em;"));
                }
                out.push_str(&if style.is_empty() {
                    "<div class=\"csl-entry\">".into()
                } else {
                    format!("<div class=\"csl-entry\" style=\"{style}\">")
                });
            }
            out.push_str(part);
        }
        let width = if aligned {
            format!("text-align: right; width: {offset}em;")
        } else {
            String::new()
        };
        let inset = if aligned {
            format!("{}", offset + 0.5)
        } else {
            "0".into()
        };
        out.replace(
            "<div class=\"csl-left-margin\">",
            &format!("<div class=\"csl-left-margin\" style=\"float: left; padding-right: 0.5em;{width}\">"),
        )
        .replace(
            "<div class=\"csl-right-inline\">",
            &format!("<div class=\"csl-right-inline\" style=\"margin: 0 .4em 0 {inset}em;{hanging}\">"),
        )
        .replace(
            "<div class=\"csl-indent\">",
            "<div class=\"csl-indent\" style=\"margin: .5em 0 0 2em; padding: 0 0 .2em .5em; border-left: 5px solid #ccc;\">",
        ) + "</div>"
    }

    /// Zotero's paragraph settings, in twips, ahead of the first entry.
    fn rtf(&self, rtf: &str) -> String {
        let (mut indent, mut first, mut tabs) = (0, 0, String::new());
        if self.hanging_indent {
            (indent, first) = (720, -720);
        } else if let Some(align) = &self.second_field_align {
            let at = 24 + self.max_offset * 120;
            first = -i64::from(at);
            if *align == Align::Margin {
                tabs = "\\tx0 ".into();
            } else {
                indent = at;
                tabs = format!("\\tx{at} ");
            }
        }
        let line = (240.0 * self.line_spacing).round();
        let after = (240.0 * self.entry_spacing).round();
        let settings = format!("{tabs}\\li{indent} \\fi{first} \\sl{line} \\slmult1 \\sa{after} ");
        rtf.replacen("\\f0\n", &format!("\\f0 {settings}\n"), 1)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Span {
    item: Option<usize>,
    text: String,
}

/// What the formatter's worker answers.
pub(crate) enum Formatted {
    Citation(Box<CitationResult>),
    Document(DocumentAnswer),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentAnswer {
    Formatted(DocumentCitations),
    /// The items the style cannot cite, by their place in the request; none
    /// named when the style cannot write the document's citations as a whole.
    Refused(Vec<(usize, CitationError)>),
    /// Request items whose locator made no contribution to the citation.
    UnprintedLocators(Vec<usize>),
}

pub(crate) fn refused_formatted(input: &Prepared, error: CitationError) -> Formatted {
    if input.document {
        Formatted::Document(DocumentAnswer::Refused(
            input
                .outcomes
                .iter()
                .filter_map(|outcome| match outcome {
                    ItemOutcome::Refused { item, error } => Some((*item, *error)),
                    _ => None,
                })
                .collect(),
        ))
    } else {
        Formatted::Citation(Box::new(refused(input, error)))
    }
}

#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case", deny_unknown_fields)]
enum DocumentOutputs {
    Refused,
    Rendered {
        citations: Vec<String>,
        entries: Vec<String>,
        hanging_indent: bool,
        unprinted_locators: Vec<usize>,
    },
}

/// The processor's HTML for a document, as runs: only its passive tags,
/// whitespace folded as HTML shows it.
pub(crate) fn document(json: &str, items: usize) -> Result<DocumentAnswer, FormatError> {
    let output: DocumentOutputs = serde_json::from_str(json).map_err(|_| FormatError::Runtime)?;
    let DocumentOutputs::Rendered {
        citations,
        entries,
        hanging_indent,
        unprinted_locators,
    } = output
    else {
        return Ok(DocumentAnswer::Refused(Vec::new()));
    };
    if unprinted_locators.iter().any(|&index| index >= items) {
        return Err(FormatError::Runtime);
    }
    if !unprinted_locators.is_empty() {
        return Ok(DocumentAnswer::UnprintedLocators(unprinted_locators));
    }
    let citations = citations
        .iter()
        .map(|html| {
            let runs = folded(parse_html(html)?.runs);
            if runs.is_empty() {
                return Err(FormatError::Runtime);
            }
            Ok(runs)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let entries = entries
        .iter()
        .map(|html| {
            let Parsed { runs, blocks } = parse_html(html)?;
            let entry = blocks
                .iter()
                .find(|block| block.kind == DisplayBlockKind::Entry)
                .map_or(0..runs.len(), |block| block.start..block.end);
            let margin = blocks
                .iter()
                .find(|block| block.kind == DisplayBlockKind::Margin && entry.contains(&block.start))
                .map(|block| block.start..block.end.min(entry.end));
            let preceding = margin.as_ref().is_some_and(|margin| {
                runs[entry.start..margin.start]
                    .iter()
                    .any(|run| !run.text.trim_matches(js_space).is_empty())
            });
            let (margin, content) = match margin {
                // The two-column entry cannot put text before its margin.
                // Keep the full entry in reading order, separating its blocks.
                Some(_) if preceding => {
                    let mut content = Vec::new();
                    for index in entry {
                        if blocks.iter().any(|block| {
                            block.kind != DisplayBlockKind::Entry && (block.start == index || block.end == index)
                        }) {
                            content.push(DisplayRun {
                                text: " ".into(),
                                ..DisplayRun::default()
                            });
                        }
                        content.push(runs[index].clone());
                    }
                    (Vec::new(), content)
                }
                Some(margin) => (runs[margin.clone()].to_vec(), runs[margin.end..entry.end].to_vec()),
                None => (Vec::new(), runs[entry].to_vec()),
            };
            Ok(DocumentEntry {
                margin: folded(margin),
                content: folded(content),
            })
        })
        .collect::<Result<Vec<_>, FormatError>>()?;
    if citations
        .iter()
        .chain(entries.iter().flat_map(|entry| [&entry.margin, &entry.content]))
        .flatten()
        .any(|run| unsupported_controls(&run.text))
    {
        return Err(FormatError::Runtime);
    }
    Ok(DocumentAnswer::Formatted(DocumentCitations {
        citations,
        entries,
        hanging_indent,
    }))
}

/// Runs with each stretch of whitespace one space, trimmed at both ends.
fn folded(runs: Vec<DisplayRun>) -> Vec<DisplayRun> {
    let mut out: Vec<DisplayRun> = Vec::new();
    let mut space = true;
    for mut run in runs {
        let mut text = String::with_capacity(run.text.len());
        for ch in run.text.chars() {
            if js_space(ch) {
                if !space {
                    text.push(' ');
                }
                space = true;
            } else {
                text.push(ch);
                space = false;
            }
        }
        run.text = text;
        if !run.text.is_empty() {
            out.push(run);
        }
    }
    while let Some(last) = out.last_mut() {
        let trimmed = last.text.trim_end_matches(' ').len();
        last.text.truncate(trimmed);
        if !last.text.is_empty() {
            break;
        }
        out.pop();
    }
    out
}

pub(crate) fn refused(input: &Prepared, error: CitationError) -> CitationResult {
    CitationResult::new(
        String::new(),
        None,
        None,
        DisplayTree::default(),
        input
            .outcomes
            .iter()
            .map(|outcome| match outcome {
                ItemOutcome::Rendered { item } => ItemOutcome::Refused { item: *item, error },
                refused => *refused,
            })
            .collect(),
        input.losses.clone(),
        input.notices.clone(),
    )
    .with_warnings(input.warnings.clone())
    .with_language(input.language.clone())
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
        unprinted_locators,
        layout,
    } = output
    else {
        return Ok(refused(input, CitationError::UnsupportedForm));
    };
    let layout = layout.map(Layout::checked).transpose()?;
    let candidates = input
        .outcomes
        .iter()
        .filter(|outcome| matches!(outcome, ItemOutcome::Rendered { .. }))
        .count();
    if candidates == 0 || coverage != (0..candidates).collect::<Vec<_>>() {
        return Ok(refused(input, CitationError::UnsupportedForm));
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
    let parsed = parse_html(&html)?;
    let runs = &parsed.runs;
    let exact =
        presentation_matches(&text, runs, superscripts) || presentation_matches(&presentation, runs, superscripts);
    let folded = !exact && blocks_match(&text, &html, superscripts);
    if runs.iter().any(|run| unsupported_controls(&run.text)) || !(exact || folded) {
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
        || unprinted_locators.iter().any(|&cite| cite >= items.len())
    {
        return Err(FormatError::Runtime);
    }
    let mut losses = input.losses.clone();
    losses.extend(unprinted_locators.iter().map(|&cite| Loss {
        item: Some(items[cite]),
        field: LossField::Locator,
        reason: LossReason::Unsupported,
    }));
    // Project validated presentation equivalents onto the canonical text while
    // retaining emphasis. Clipboard HTML and RTF remain untouched.
    let canonical =
        project_runs(&text, runs, superscripts, folded || text_parts.is_some()).ok_or(FormatError::Runtime)?;
    let mut offset = 0;
    let nodes = spans
        .into_iter()
        .map(|span| {
            let end = offset + span.text.len();
            let mut position = 0;
            let projected: Vec<_> = canonical
                .iter()
                .filter_map(|(run, source)| {
                    let start = position;
                    position += run.text.len();
                    let from = offset.max(start);
                    let to = end.min(position);
                    (from < to).then(|| {
                        (
                            DisplayRun {
                                text: run.text[from - start..to - start].into(),
                                ..run.clone()
                            },
                            *source,
                        )
                    })
                })
                .collect();
            let blocks = parsed
                .blocks
                .iter()
                .filter_map(|block| {
                    let mut covered = projected.iter().enumerate().filter_map(|(index, (_, source))| {
                        source
                            .filter(|source| block.start <= *source && *source < block.end)
                            .map(|_| index)
                    });
                    let start = covered.next()?;
                    let end = covered.next_back().unwrap_or(start) + 1;
                    Some(DisplayBlock {
                        kind: block.kind,
                        start,
                        end,
                    })
                })
                .collect();
            offset = end;
            DisplayNode {
                item: span.item.map(|item| items[item]),
                content: projected.into_iter().map(|(run, _)| run).collect(),
                blocks,
            }
        })
        .collect();
    if input.missing_details {
        return Ok(refused(input, CitationError::MissingDetails));
    }
    // The layout goes around the checked output, never into what was checked.
    let (html, rtf) = match &layout {
        Some(layout) => (layout.html(&html), layout.rtf(&rtf)),
        None => (html, rtf),
    };
    Ok(CitationResult::new(
        text,
        Some(html),
        Some(rtf),
        DisplayTree { nodes },
        input.outcomes.clone(),
        losses,
        input.notices.clone(),
    )
    .with_warnings(input.warnings.clone())
    .with_language(input.language.clone()))
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

/// Entries laid out in blocks or margin columns are spaced by those blocks in
/// HTML and by line breaks in text, so there the two may differ in how much
/// whitespace separates words, never in whether it does: a block edge counts as
/// a break, and every run of whitespace counts as one.
fn blocks_match(text: &str, html: &str, superscripts: &BTreeMap<char, String>) -> bool {
    const BLOCKS: [&str; 4] = ["csl-block", "csl-left-margin", "csl-right-inline", "csl-indent"];
    if !BLOCKS
        .iter()
        .any(|class| html.contains(&format!("<div class=\"{class}\">")))
    {
        return false;
    }
    let mut spaced = html.replace("</div>", "</div> ");
    for class in BLOCKS {
        let tag = format!("<div class=\"{class}\">");
        spaced = spaced.replace(&tag, &format!(" {tag}"));
    }
    let Ok(parsed) = parse_html(&spaced) else {
        return false;
    };
    let collapse = |text: &str, out: &mut String, last_space: &mut bool| {
        for ch in text.chars() {
            if js_space(ch) {
                if !*last_space {
                    out.push(' ');
                }
                *last_space = true;
            } else {
                out.push(ch);
                *last_space = false;
            }
        }
    };
    let mut last_space = true;
    let runs: Vec<_> = parsed
        .runs
        .iter()
        .map(|run| {
            let mut text = String::new();
            collapse(&run.text, &mut text, &mut last_space);
            DisplayRun { text, ..run.clone() }
        })
        .collect();
    let (mut flat, mut space) = (String::new(), true);
    collapse(text, &mut flat, &mut space);
    let mut runs = runs;
    if let Some(last) = runs.iter_mut().rev().find(|run| !run.text.is_empty()) {
        last.text = last.text.trim_end().to_owned();
    }
    presentation_matches(flat.trim_end(), &runs, superscripts)
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

// Keep the source run identity while projecting text so block boundaries survive
// glyph expansion and the processor's bibliography whitespace folding.
fn project_runs(
    text: &str,
    runs: &[DisplayRun],
    superscripts: &BTreeMap<char, String>,
    fold_blocks: bool,
) -> Option<Vec<(DisplayRun, Option<usize>)>> {
    let mut shown = runs
        .iter()
        .enumerate()
        .flat_map(|(index, run)| run.text.chars().map(move |ch| (ch, run, index)))
        .peekable();
    let mut result: Vec<(DisplayRun, Option<usize>)> = Vec::new();
    for original in text.chars() {
        if fold_blocks && !js_space(original) {
            while shown.peek().is_some_and(|(ch, _, _)| js_space(*ch)) {
                shown.next();
            }
        }
        if fold_blocks && js_space(original) && !shown.peek().is_some_and(|(ch, _, _)| js_space(*ch)) {
            let source = (original != '\n')
                .then(|| result.last().and_then(|(_, source)| *source))
                .flatten();
            result.push((
                DisplayRun {
                    text: original.to_string(),
                    ..DisplayRun::default()
                },
                source,
            ));
            continue;
        }
        let &(ch, run, source) = shown.peek()?;
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
                let (actual, style, _) = shown.next()?;
                if actual != expected || style.script != run.script {
                    return None;
                }
            }
            // The canonical character already carries its raised/lowered glyph.
            Script::Normal
        };
        if let Some((last, _)) = result.last_mut().filter(|(last, previous)| {
            *previous == Some(source)
                && last.italic == run.italic
                && last.bold == run.bold
                && last.script == script
                && last.small_caps == run.small_caps
                && last.underline == run.underline
        }) {
            last.text.push(original);
        } else {
            result.push((
                DisplayRun {
                    text: original.to_string(),
                    italic: run.italic,
                    bold: run.bold,
                    script,
                    small_caps: run.small_caps,
                    underline: run.underline,
                },
                Some(source),
            ));
        }
    }
    if fold_blocks {
        while shown.peek().is_some_and(|(ch, _, _)| js_space(*ch)) {
            shown.next();
        }
    }
    shown.next().is_none().then_some(result)
}

/// Only processor-generated passive tags are permitted in canonical clipboard HTML.
/// Output is rejected rather than silently changing a citation's visible text.
struct Parsed {
    runs: Vec<DisplayRun>,
    blocks: Vec<DisplayBlock>,
}

fn parse_html(html: &str) -> Result<Parsed, FormatError> {
    let mut runs = Vec::new();
    let mut blocks: Vec<DisplayBlock> = Vec::new();
    let mut stack = vec![DisplayRun::default()];
    let mut names: Vec<(&str, Option<usize>, Option<usize>)> = Vec::new();
    let mut rest = html;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('<') {
            let end = after.find('>').ok_or(FormatError::Runtime)?;
            let tag = &after[..end];
            // A link's address may be as long as the output allows.
            if tag.len() > 256 && !tag.starts_with("a href=\"") {
                return Err(FormatError::Runtime);
            }
            if let Some(closing) = tag.strip_prefix('/') {
                if !matches!(tag, "/i" | "/b" | "/sup" | "/sub" | "/span" | "/div" | "/a") || stack.len() == 1 {
                    return Err(FormatError::Runtime);
                }
                let (name, entry_start, block_index) = names.pop().ok_or(FormatError::Runtime)?;
                if name != closing {
                    return Err(FormatError::Runtime);
                }
                if let Some(start) = entry_start {
                    trim_entry(&mut runs[start..]);
                }
                if let Some(index) = block_index {
                    blocks[index].end = runs.len();
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
                    // The processor writes `vertical-align: baseline` this way.
                    "span style=\"baseline\"" => style.script = Script::Normal,
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
                    "div class=\"csl-entry\""
                    | "div class=\"csl-block\""
                    | "div class=\"csl-indent\""
                    | "div class=\"csl-left-margin\""
                    | "div class=\"csl-right-inline\"" => {}
                    // A record's URL or DOI as a link, as Zotero writes it. Its
                    // text is the printed value; only a public address may link.
                    value if value.starts_with("a href=\"") && value.ends_with('"') => {
                        let href = decode_entities(&value[8..value.len() - 1])?;
                        if href.contains(['"', '<', '>']) || PublicUrl::new(&href).is_none() {
                            return Err(FormatError::Runtime);
                        }
                    }
                    _ => return Err(FormatError::Runtime),
                }
                if stack.len() >= 32 {
                    return Err(FormatError::ResourceLimit);
                }
                let block = match tag {
                    "div class=\"csl-entry\"" => Some(DisplayBlockKind::Entry),
                    "div class=\"csl-left-margin\"" => Some(DisplayBlockKind::Margin),
                    "div class=\"csl-right-inline\"" => Some(DisplayBlockKind::Inline),
                    "div class=\"csl-block\"" => Some(DisplayBlockKind::Block),
                    "div class=\"csl-indent\"" => Some(DisplayBlockKind::Indent),
                    _ => None,
                };
                let block_index = block.map(|kind| {
                    let index = blocks.len();
                    blocks.push(DisplayBlock {
                        kind,
                        start: runs.len(),
                        end: runs.len(),
                    });
                    index
                });
                names.push((
                    tag.split_whitespace().next().ok_or(FormatError::Runtime)?,
                    (tag == "div class=\"csl-entry\"").then_some(runs.len()),
                    block_index,
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
    Ok(Parsed { runs, blocks })
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
mod tests {
    use super::*;

    #[test]
    fn document_entries_keep_text_and_emphasis_before_a_margin() {
        let html = concat!(
            "<div class=\"csl-entry\"><div class=\"csl-block\">Vale, <i>A.</i></div>",
            "<div class=\"csl-left-margin\">2024</div><div class=\"csl-right-inline\">Lanterns.</div></div>",
        );
        let json = serde_json::json!({
            "status": "rendered", "citations": ["(Vale, 2024)"],
            "entries": [html], "hanging_indent": false, "unprinted_locators": [],
        });
        let DocumentAnswer::Formatted(answer) = document(&json.to_string(), 1).unwrap() else {
            panic!()
        };
        assert!(answer.entries[0].margin.is_empty());
        let runs = &answer.entries[0].content;
        assert_eq!(
            runs.iter().map(|run| run.text.as_str()).collect::<String>(),
            "Vale, A. 2024 Lanterns."
        );
        assert!(runs.iter().any(|run| run.italic && run.text == "A."));
    }

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
}
