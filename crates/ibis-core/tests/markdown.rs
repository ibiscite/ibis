//! Verify: IBIS_REQUIRE_PANDOC=1 RUSTC_WRAPPER= CARGO_TARGET_DIR="$PWD/target" cargo test -p ibis-core --locked --test markdown
#![allow(clippy::unwrap_used)]
use ibis_core::{citation::copy_markdown, contract::*};
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};
use unicode_normalization::UnicodeNormalization;

fn fixture(name: &str) -> CitationResult {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../fixtures/formatted/{name}.json"));
    let case: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut result: CitationResult = serde_json::from_value(case["expected"]["data"].clone()).unwrap();
    result.receipt = Some(
        serde_json::from_value(json!({
            "request":case["request"], "sources":[], "content_hash":result.content_hash,
            "ibis_version":"test", "meaning_version":1
        }))
        .unwrap(),
    );
    result
}

fn run(text: &str, style: u8) -> DisplayRun {
    DisplayRun {
        text: text.into(),
        italic: style & 1 != 0,
        bold: style & 2 != 0,
        ..DisplayRun::default()
    }
}

fn emphasis(text: &str, style: u8) -> String {
    let text = text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let (i, ie) = if style & 1 != 0 { ("<i>", "</i>") } else { ("", "") };
    let (b, be) = if style & 2 != 0 { ("<b>", "</b>") } else { ("", "") };
    format!("{i}{b}{text}{be}{ie}")
}

fn result(runs: Vec<DisplayRun>, html: &str) -> CitationResult {
    CitationResult::new(
        runs.iter().map(|r| r.text.as_str()).collect(),
        Some(html.into()),
        None,
        DisplayTree {
            nodes: vec![DisplayNode {
                item: Some(0),
                content: runs,
                blocks: vec![],
            }],
        },
        vec![ItemOutcome::Rendered { item: 0 }],
        vec![],
        vec![],
    )
}

fn pandoc(input: &str, from: &str, to: &str) -> Vec<u8> {
    let mut child = Command::new("pandoc")
        .args(["-f", from, "-t", to])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("Pandoc is required for Markdown differential comparisons: {error}"));
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    output.stdout
}

// Compare each visible character's presentation and each paragraph boundary,
// independent of Pandoc's choice of nested emphasis or adjacent Str nodes.
fn inlines(nodes: &Value, style: u8, out: &mut Vec<(char, u8)>) {
    for node in nodes.as_array().unwrap() {
        let content = &node["c"];
        match node["t"].as_str().unwrap() {
            "Str" => out.extend(
                content
                    .as_str()
                    .unwrap()
                    .nfc()
                    .map(|c| (c, if c.is_whitespace() { 0 } else { style })),
            ),
            "Space" | "SoftBreak" | "LineBreak" => out.push((' ', 0)),
            "Emph" => inlines(content, style | 1, out),
            "Strong" => inlines(content, style | 2, out),
            "Superscript" => inlines(content, style | 4, out),
            "Subscript" => inlines(content, style | 8, out),
            "Underline" => inlines(content, style | 16, out),
            "SmallCaps" => inlines(content, style | 32, out),
            "Span" => inlines(&content[1], style, out),
            tag => panic!("unexpected inline {tag}: {node}"),
        }
    }
}

fn blocks(nodes: &Value, out: &mut Vec<Vec<(char, u8)>>) {
    for node in nodes.as_array().unwrap() {
        match node["t"].as_str().unwrap() {
            "Div" => blocks(&node["c"][1], out),
            "Para" | "Plain" => {
                let mut visible = vec![];
                inlines(&node["c"], 0, &mut visible);
                out.push(visible);
            }
            tag => panic!("unexpected block {tag}: {node}"),
        }
    }
}

fn visible(html: &str) -> Vec<Vec<(char, u8)>> {
    let ast: Value = serde_json::from_slice(&pandoc(html, "html", "json")).unwrap();
    let mut out = vec![];
    blocks(&ast["blocks"], &mut out);
    out
}

fn compare(result: &CitationResult) -> Vec<Vec<(char, u8)>> {
    let before = result.clone();
    let markdown = copy_markdown(result, result.outcomes.len()).unwrap();
    assert_eq!(*result, before, "conversion changed the canonical result or receipt");
    let expected = visible(result.html.as_deref().unwrap());
    for reader in ["commonmark", "markdown-smart"] {
        let html = String::from_utf8(pandoc(&markdown, reader, "html")).unwrap();
        assert_eq!(visible(&html), expected, "{reader}: {markdown}");
    }
    expected
}

#[test]
fn copy_requires_complete_hash_valid_content_and_matching_display_text() {
    let original = fixture("apa-bibliography");
    assert_eq!(
        copy_markdown(&original, 1).unwrap(),
        r"Taylor\, A\. \(2026\)\. *Synthetic signals*\. Imaginary Press\."
    );
    assert!(copy_markdown(&original, 2).is_none());
    let corruptions: [fn(&mut CitationResult); 6] = [
        |r| r.outcomes.clear(),
        |r| {
            r.outcomes[0] = ItemOutcome::Refused {
                item: 0,
                error: CitationError::MissingItem,
            }
        },
        |r| r.outcomes[0] = ItemOutcome::Rendered { item: 1 },
        |r| r.text.push('!'),
        |r| r.html.as_mut().unwrap().push('!'),
        |r| r.rtf.as_mut().unwrap().push('!'),
    ];
    for corrupt in corruptions {
        let mut changed = original.clone();
        corrupt(&mut changed);
        assert!(copy_markdown(&changed, 1).is_none());
    }
    let mut changed = original.clone();
    changed.display.nodes[0].content[1].text.push('!');
    assert!(changed.copy_content(1).is_some());
    assert!(copy_markdown(&changed, 1).is_none());
    changed.display.nodes.clear();
    assert!(copy_markdown(&changed, 1).is_none());
    let bare = CitationResult::new(
        original.text,
        None,
        None,
        original.display,
        original.outcomes,
        vec![],
        vec![],
    );
    assert!(copy_markdown(&bare, 1).unwrap().contains("*Synthetic signals*"));
}

#[test]
fn pandoc_differential_nonbreaking_spaces() {
    for runs in [
        vec![run("left\u{a0}right", 0)],
        vec![run("left\u{a0}", 1), run("\u{a0}right", 2)],
        vec![run("left", 1), run("\u{a0}", 3), run("right", 2)],
    ] {
        let html: String = runs
            .iter()
            .map(|r| emphasis(&r.text, u8::from(r.italic) | (u8::from(r.bold) << 1)))
            .collect();
        let shown = compare(&result(runs, &html));
        assert!(shown[0].iter().any(|(ch, _)| *ch == '\u{a0}'));
    }
    let mut citation = result(
        vec![run("left", 0), run("\u{a0}", 1), run("right", 2)],
        "<div class=\"csl-entry\">left<div class=\"csl-block\"><i>&nbsp;</i></div><b>right</b></div>",
    );
    citation.display.nodes[0].blocks = [(DisplayBlockKind::Entry, 0, 3), (DisplayBlockKind::Block, 1, 2)]
        .into_iter()
        .map(|(kind, start, end)| DisplayBlock { kind, start, end })
        .collect();
    assert_eq!(compare(&citation).len(), 3);
}

#[test]
fn pandoc_differential_block_paragraphs_with_nested_and_equal_ranges() {
    let children = "<div class=\"csl-block\">first</div><div class=\"csl-block\"><i>second</i></div>";
    for parents in [0, 2] {
        let html = format!(
            "<div class=\"csl-entry\">{}{children}{}</div>",
            "<div class=\"csl-indent\">".repeat(parents),
            "</div>".repeat(parents),
        );
        let mut citation = result(vec![run("first", 0), run("second", 1)], &html);
        citation.display.nodes[0].blocks = std::iter::once((DisplayBlockKind::Entry, 0, 2))
            .chain(std::iter::repeat_n((DisplayBlockKind::Indent, 0, 2), parents))
            .chain([(DisplayBlockKind::Block, 0, 1), (DisplayBlockKind::Block, 1, 2)])
            .map(|(kind, start, end)| DisplayBlock { kind, start, end })
            .collect();
        assert_eq!(compare(&citation).len(), 2);
        assert_eq!(copy_markdown(&citation, 1).unwrap(), "first\n\n*second*");
    }
}

#[test]
fn pandoc_differential_display_order_across_nodes_and_shared_ranges() {
    // Bibliography order is display order, which need not be request item order.
    let entries = [
        fixture("apa-citation"),
        fixture("apa-bibliography"),
        fixture("ieee-document-context"),
    ];
    let mut nodes = vec![];
    for (position, entry) in entries.iter().enumerate() {
        compare(entry);
        let mut node = entry.display.nodes[0].clone();
        node.item = Some([2, 0, 1][position]);
        if position > 0 {
            node.content.insert(0, run("\n", 0));
        }
        node.blocks = vec![DisplayBlock {
            kind: DisplayBlockKind::Entry,
            start: usize::from(position > 0),
            end: node.content.len(),
        }];
        nodes.push(node);
    }
    let html = entries
        .iter()
        .map(|e| format!("<div class=\"csl-entry\">{}</div>", e.html.as_deref().unwrap()))
        .collect::<Vec<_>>()
        .join("\n");
    let mut several = result(nodes.iter().flat_map(|node| node.content.clone()).collect(), &html);
    several.display.nodes = nodes;
    several.outcomes = (0..3).map(|item| ItemOutcome::Rendered { item }).collect();
    let shown = compare(&several);
    assert_eq!(shown.len(), 3);
    // The same entry ranges can share a node; node boundaries are not paragraphs.
    let mut shared = several;
    let mut node = shared.display.nodes.remove(0);
    node.item = None;
    for group in shared.display.nodes.drain(..) {
        let offset = node.content.len();
        node.blocks.extend(group.blocks.iter().map(|b| DisplayBlock {
            kind: b.kind,
            start: b.start + offset,
            end: b.end + offset,
        }));
        node.content.extend(group.content);
    }
    shared.display.nodes = vec![node];
    assert_eq!(compare(&shared), shown);
}

#[test]
fn pandoc_differential_text_and_emphasis() {
    for (left, middle, right) in [("a", "b", "c"), ("word", "!", "next"), ("a", "b\n\nc", "d")] {
        for a in 0..4 {
            for b in 0..4 {
                for c in 0..4 {
                    let html = format!("{}{}{}", emphasis(left, a), emphasis(middle, b), emphasis(right, c));
                    compare(&result(vec![run(left, a), run(middle, b), run(right, c)], &html));
                }
            }
        }
    }
    let literal = "日本語 café e\u{301} 🐦 * _ ~ ^ $ # ! . + - | \\ [x](y) [@key] <script>x</script> &amp; `x` `````";
    assert!(
        copy_markdown(&result(vec![run(literal, 0)], ""), 1)
            .unwrap()
            .contains("e\u{301}")
    );
    for style in 0..4 {
        compare(&result(vec![run(literal, style)], &emphasis(literal, style)));
    }
    compare(&result(
        vec![run("e\u{301}", 0), run("!", 1), run("next", 0)],
        "e\u{301}<i>!</i>next",
    ));
    compare(&result(
        vec![run(" leading ", 1), run("orchid", 0), run(" trailing ", 1)],
        " <i>leading</i> orchid <i>trailing</i> ",
    ));
    for text in ["*", "_", "&", "<", "`", "🧪", "a\n\nb", "a  b", "a\tb"] {
        for style in 0..4 {
            compare(&result(
                vec![run("left", 0), run(text, style), run("right", 0)],
                &format!("left{}right", emphasis(text, style)),
            ));
        }
    }
    let mut passive = vec![run("rise", 1), run("fall", 2), run("under", 0), run("Caps", 3)];
    passive[0].script = Script::Superscript;
    passive[1].script = Script::Subscript;
    passive[2].underline = true;
    passive[3].small_caps = true;
    compare(&result(
        passive,
        "<sup><i>rise</i></sup><sub><b>fall</b></sub><u>under</u><span style=\"font-variant:small-caps;\"><b><i>Caps</i></b></span>",
    ));
}
