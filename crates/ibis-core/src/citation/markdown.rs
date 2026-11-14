use crate::contract::{CitationResult, DisplayBlockKind, DisplayRun, Script};

/// Derive Pandoc Markdown (Pandoc 3+, `markdown-smart`) without changing the
/// canonical result or receipt. Uses native bracketed spans, emphasis, strong,
/// superscript/subscript, underline and small caps; no raw HTML is needed.
/// Entry/block/indent boundaries become paragraphs; other layout stays inline.
/// As in canonical HTML, ASCII whitespace collapses; NBSP remains nonbreaking.
/// The caller supplies the original request's item count, as for `copy_content`.
/// Missing, incomplete, hash-invalid or text-inconsistent presentation yields none.
pub fn copy_markdown(result: &CitationResult, expected_items: usize) -> Option<String> {
    let canonical = result.copy_content(expected_items)?;
    let text: String = result
        .display
        .nodes
        .iter()
        .flat_map(|n| &n.content)
        .map(|r| r.text.as_str())
        .collect();
    if text != canonical.text {
        return None;
    }
    let mut paragraphs = vec![];
    let mut runs = vec![];
    for node in &result.display.nodes {
        for index in 0..=node.content.len() {
            if node.blocks.iter().any(|b| {
                matches!(
                    b.kind,
                    DisplayBlockKind::Entry | DisplayBlockKind::Block | DisplayBlockKind::Indent
                ) && (b.start == index || b.end == index)
            }) {
                flush(&mut runs, &mut paragraphs);
            }
            if let Some(run) = node.content.get(index) {
                runs.push(run);
            }
        }
    }
    flush(&mut runs, &mut paragraphs);
    Some(paragraphs.join("\n\n"))
}

fn flush(runs: &mut Vec<&DisplayRun>, paragraphs: &mut Vec<String>) {
    let mut out = String::new();
    let mut pending_space = false;
    for run in runs.drain(..) {
        let text = run.text.split_ascii_whitespace().collect::<Vec<_>>().join(" ");
        if text.is_empty() {
            pending_space |= !run.text.is_empty();
            continue;
        }
        if !out.is_empty() && (pending_space || run.text.starts_with(|c: char| c.is_ascii_whitespace())) {
            out.push(' ');
        }
        write_run(&mut out, run, &text);
        pending_space = run.text.ends_with(|c: char| c.is_ascii_whitespace());
    }
    if !out.is_empty() {
        paragraphs.push(out);
    }
}

fn write_run(out: &mut String, run: &DisplayRun, text: &str) {
    let core = text.trim();
    if core.is_empty() {
        escape(out, text, false);
        return;
    }
    let leading = text.len() - text.trim_start().len();
    escape(out, &text[..leading], false);
    let styled = run.italic || run.bold || run.script != Script::Normal || run.underline || run.small_caps;
    // A span isolates every delimiter from neighboring runs and punctuation.
    if styled {
        out.push('[');
    }
    let wrappers = [
        (run.script == Script::Superscript, "^"),
        (run.script == Script::Subscript, "~"),
        (run.bold, "**"),
        (run.italic, "*"),
    ];
    for (_, delimiter) in wrappers.iter().filter(|w| w.0) {
        out.push_str(delimiter);
    }
    escape(out, core, run.script != Script::Normal);
    for (_, delimiter) in wrappers.iter().rev().filter(|w| w.0) {
        out.push_str(delimiter);
    }
    if styled {
        out.push_str("]{");
        if run.underline {
            out.push_str(".underline ");
        }
        if run.small_caps {
            out.push_str(".smallcaps");
        }
        out.push('}');
    }
    escape(out, &text[leading + core.len()..], false);
}

fn escape(out: &mut String, text: &str, script: bool) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            // Entities keep spaces inside native superscript/subscript.
            ' ' if script => out.push_str("&#32;"),
            ch => {
                if ch.is_ascii_punctuation() {
                    out.push('\\');
                }
                out.push(ch);
            }
        }
    }
}
