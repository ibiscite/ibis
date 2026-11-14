use crate::contract::{CitationResult, DisplayBlockKind, DisplayRun, Script};

/// Derive formatted Markdown without changing the canonical result or receipt.
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
    // Only collapsible whitespace can disappear between paragraphs, never NBSP.
    if runs.iter().any(|r| r.text.chars().any(|c| !c.is_ascii_whitespace())) {
        runs.retain(|r| !r.text.is_empty());
        let mut out = String::new();
        for (index, run) in runs.iter().enumerate() {
            let previous = index.checked_sub(1).and_then(|i| runs.get(i));
            let next = runs.get(index + 1);
            let core = run.text.trim();
            if core.is_empty() {
                escape(&mut out, &run.text);
                continue;
            }
            let leading = run.text.len() - run.text.trim_start().len();
            escape(&mut out, &run.text[..leading]);
            // Delimiter runs can merge across style changes, or fail to flank
            // punctuation inside a word. Only those emphasis boundaries use tags.
            let html_emphasis = [previous, next]
                .into_iter()
                .flatten()
                .any(|neighbor| neighbor.italic || neighbor.bold)
                || leading == 0
                    && core.starts_with(|c: char| !c.is_alphanumeric())
                    && previous.is_some_and(|p| p.text.ends_with(|c: char| !c.is_whitespace()))
                || core.len() + leading == run.text.len()
                    && core.ends_with(|c: char| !c.is_alphanumeric())
                    && next.is_some_and(|n| n.text.starts_with(|c: char| !c.is_whitespace()));
            // Fixed passive tags only; source text never supplies markup or attributes.
            let wrappers = [
                (run.script == Script::Superscript, "<sup>", "</sup>"),
                (run.script == Script::Subscript, "<sub>", "</sub>"),
                (run.underline, "<u>", "</u>"),
                (run.small_caps, "<span style=\"font-variant:small-caps;\">", "</span>"),
                (
                    run.bold,
                    if html_emphasis { "<b>" } else { "**" },
                    if html_emphasis { "</b>" } else { "**" },
                ),
                (
                    run.italic,
                    if html_emphasis { "<i>" } else { "*" },
                    if html_emphasis { "</i>" } else { "*" },
                ),
            ];
            for (_, open, _) in wrappers.iter().filter(|w| w.0) {
                out.push_str(open);
            }
            escape(&mut out, core);
            for (_, _, close) in wrappers.iter().rev().filter(|w| w.0) {
                out.push_str(close);
            }
            escape(&mut out, &run.text[leading + core.len()..]);
        }
        paragraphs.push(out);
    }
    runs.clear();
}

fn escape(out: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            ' ' if out.is_empty() => out.push_str("&#32;"),
            ch => {
                if ch.is_ascii_punctuation() {
                    out.push('\\');
                }
                out.push(ch);
            }
        }
    }
}
