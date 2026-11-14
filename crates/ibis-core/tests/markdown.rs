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
        script: if style & 4 != 0 {
            Script::Superscript
        } else if style & 8 != 0 {
            Script::Subscript
        } else {
            Script::default()
        },
        underline: style & 16 != 0,
        small_caps: style & 32 != 0,
    }
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

fn pandoc_available() -> bool {
    let available = Command::new("pandoc").arg("--version").output().is_ok_and(|output| {
        output.status.success()
            && String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .nth(1)
                .and_then(|v| v.split('.').next()?.parse::<u32>().ok())
                .is_some_and(|major| major >= 3)
    });
    assert!(
        available || std::env::var_os("IBIS_REQUIRE_PANDOC").is_none(),
        "Pandoc 3 or newer is required"
    );
    if !available {
        eprintln!("Skipping optional Markdown consumer checks: Pandoc 3 or newer is unavailable");
    }
    available
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

fn visible(input: &str, reader: &str) -> Vec<Vec<(char, u8)>> {
    let ast: Value = serde_json::from_slice(&pandoc(input, reader, "json")).unwrap();
    let mut out = vec![];
    blocks(&ast["blocks"], &mut out);
    out
}

fn compare(result: &CitationResult) -> Vec<Vec<(char, u8)>> {
    let markdown = copy_markdown(result, result.outcomes.len()).unwrap();
    let html = result.html.as_deref().unwrap();
    let expected = visible(html, "html");
    assert_eq!(visible(&markdown, "markdown-smart"), expected, "{markdown}");
    let latex = String::from_utf8(pandoc(&markdown, "markdown-smart", "latex")).unwrap();
    // Compare direct LaTeX semantically: empty span groups are presentation-neutral.
    let canonical_latex = String::from_utf8(pandoc(html, "html", "latex")).unwrap();
    assert_eq!(visible(&latex, "latex"), visible(&canonical_latex, "latex"), "{latex}");
    expected
}

#[test]
fn copy_requires_complete_hash_valid_content_and_matching_display_text() {
    let original = fixture("apa-bibliography");
    let before = original.clone();
    let markdown = copy_markdown(&original, 1).unwrap();
    assert_eq!(original, before, "canonical content and receipt are immutable");
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
    assert_eq!(copy_markdown(&bare, 1), Some(markdown));
}

#[test]
fn pandoc_adjacent_emphasis() {
    if !pandoc_available() {
        return;
    }
    let citation = result(vec![run("alpha", 1), run("beta", 2)], "<i>alpha</i><b>beta</b>");
    compare(&citation);
    let markdown = copy_markdown(&citation, 1).unwrap();
    let latex = String::from_utf8(pandoc(&markdown, "markdown-smart", "latex")).unwrap();
    assert_eq!(latex.trim(), r"{\emph{alpha}}{\textbf{beta}}");
}

#[test]
fn pandoc_direct_parsing_preserves_punctuation_nbsp_and_passive_styles() {
    if !pandoc_available() {
        return;
    }
    let literal = "日本語 café e\u{301} 🐦 * _ ~ ^ $ # ! . + - | \\ [x](y) [@key] <script>x</script> &amp; `x`";
    let html = literal.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    compare(&result(vec![run(literal, 3)], &format!("<b><i>{html}</i></b>")));
    let shown = compare(&result(
        vec![
            run("left\u{a0}", 1),
            run("\u{a0}", 3),
            run("right", 2),
            run("!", 1),
            run("next", 0),
        ],
        "<i>left&nbsp;</i><b><i>&nbsp;</i></b><b>right</b><i>!</i>next",
    ));
    assert_eq!(shown[0].iter().filter(|(ch, _)| *ch == '\u{a0}').count(), 2);
    compare(&result(
        vec![
            run("rise high", 5),
            run("fall low", 10),
            run("under", 16),
            run("Caps", 35),
        ],
        "<sup><i>rise high</i></sup><sub><b>fall low</b></sub><u>under</u><span style=\"font-variant:small-caps;\"><b><i>Caps</i></b></span>",
    ));
    compare(&result(
        vec![run(" a\n\nb", 1), run("", 0), run(" \t ", 0), run("c\td ", 1)],
        "<i> a\n\nb</i> \t <i>c\td </i>",
    ));
}

#[test]
fn pandoc_paragraphs_preserve_nbsp_and_nested_equal_block_ranges() {
    if !pandoc_available() {
        return;
    }
    let mut citation = result(
        vec![run("first", 0), run("\u{a0}", 1), run("second", 1)],
        "<div class=\"csl-entry\"><div class=\"csl-indent\"><div class=\"csl-block\">first</div><div class=\"csl-block\"><i>&nbsp;</i></div><div class=\"csl-block\"><i>second</i></div></div></div>",
    );
    citation.display.nodes[0].blocks = [
        (DisplayBlockKind::Entry, 0, 3),
        (DisplayBlockKind::Indent, 0, 3),
        (DisplayBlockKind::Indent, 0, 3),
        (DisplayBlockKind::Block, 0, 1),
        (DisplayBlockKind::Block, 1, 2),
        (DisplayBlockKind::Block, 2, 3),
    ]
    .into_iter()
    .map(|(kind, start, end)| DisplayBlock { kind, start, end })
    .collect();
    let shown = compare(&citation);
    assert_eq!(shown.len(), 3);
    // The same display order can span nodes, independently of request order.
    let node = citation.display.nodes.remove(0);
    citation.display.nodes = node
        .content
        .into_iter()
        .enumerate()
        .map(|(item, run)| DisplayNode {
            item: Some(2 - item),
            content: vec![run],
            blocks: vec![DisplayBlock {
                kind: DisplayBlockKind::Entry,
                start: 0,
                end: 1,
            }],
        })
        .collect();
    citation.outcomes = (0..3).map(|item| ItemOutcome::Rendered { item }).collect();
    assert_eq!(compare(&citation), shown);
}

#[test]
fn consumers_are_optional_but_required_mode_refuses_unavailable_pandoc() {
    let missing = tempfile::tempdir().unwrap();
    let check = |path: &std::path::Path| {
        for required in [false, true] {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", "pandoc_adjacent_emphasis", "--nocapture"])
                .env("PATH", path)
                .env_remove("IBIS_REQUIRE_PANDOC");
            if required {
                command.env("IBIS_REQUIRE_PANDOC", "1");
            }
            let output = command.output().unwrap();
            let diagnostic = String::from_utf8_lossy(&output.stderr);
            assert_eq!(output.status.success(), !required, "{diagnostic}");
            if required {
                assert!(diagnostic.contains("Pandoc 3 or newer is required"));
            }
        }
    };
    check(missing.path());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let unsupported = tempfile::tempdir().unwrap();
        let executable = unsupported.path().join("pandoc");
        std::fs::write(&executable, "#!/bin/sh\nprintf 'pandoc 2.0\\n'\n").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        check(unsupported.path());
    }
}
