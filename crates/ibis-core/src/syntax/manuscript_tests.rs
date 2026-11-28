use super::*;
use std::path::Path;

fn keys(kind: TextKind, text: &str) -> Vec<(usize, usize, Vec<String>)> {
    let read = scan(kind, text, &|| false);
    assert!(read.problems.is_empty(), "{:?}", read.problems);
    read.citations
        .into_iter()
        .map(|cited| {
            (
                cited.place.line,
                cited.place.column,
                cited.cites.into_iter().map(|cite| cite.key).collect(),
            )
        })
        .collect()
}

fn of(entries: &[(usize, usize, &[&str])]) -> Vec<(usize, usize, Vec<String>)> {
    entries
        .iter()
        .map(|(line, column, keys)| (*line, *column, keys.iter().map(|key| (*key).to_owned()).collect()))
        .collect()
}

#[test]
fn markdown_citations_are_read_outside_code_comments_and_front_matter() {
    let text = "---\ntitle: \"Mail @nobody\"\nbibliography: refs.bib\n---\n\
Lanterns guide ships [@vale2024, p. 12; -@fen2023].\n\
Écrit par @okafor2031 [ch. 2], see @fig-map and [@sec-intro].\n\n\
```python\nprint(\"@inside\")\n```\n\
Inline `@code` and <!-- @hidden\nstill --> text, mail me@example.org.\n";
    assert_eq!(
        keys(TextKind::Markdown, text),
        of(&[(5, 22, &["vale2024", "fen2023"]), (6, 11, &["okafor2031"])])
    );
    let read = scan(TextKind::Markdown, text, &|| false);
    assert!(read.complete());
    assert_eq!(read.citations[1].cites[0].locator.as_ref().unwrap().value, "2");
    assert!(read.citations[1].narrative);
}

#[test]
fn a_damaged_marker_stops_only_its_paragraph() {
    let text = "Opened [@vale2024, p. 12 and never closed.\r\n\r\nLater [@fen2023].\n";
    let read = scan(TextKind::Markdown, text, &|| false);
    assert_eq!(
        read.problems,
        [ScanFinding {
            place: Place { line: 1, column: 8 },
            kind: ScanProblem::Unclosed
        }]
    );
    assert_eq!(read.citations.len(), 1);
    assert_eq!(read.citations[0].place, Place { line: 3, column: 7 });
    assert!(read.complete());
}

#[test]
fn an_unclosed_fence_hides_the_rest_and_unclosed_front_matter_hides_nothing() {
    assert!(keys(TextKind::Markdown, "Text.\n~~~~\n[@hidden]\n").is_empty());
    assert_eq!(keys(TextKind::Markdown, "---\n[@shown]\n"), of(&[(2, 1, &["shown"])]));
}

#[test]
fn latex_commands_of_every_package_are_read_and_comments_skipped() {
    let text = "As \\citet{vale2024} shows % \\cite{commented}\n\
and \\parencite[see][p.~4]{fen2023, okafor2031}, also \\autocites(all)[p.~1]{vale2024}[ch.~2]{marsh2030}.\n\
\\begin{verbatim}\n\\cite{verbatim}\n\\end{verbatim}\n\
\\verb|\\cite{inline}| \\volcite{3}[45]{lark2029} \\nocite{*} \\citestyle{apa} \\% \\cite{after}\n";
    assert_eq!(
        keys(TextKind::Latex, text),
        of(&[
            (1, 4, &["vale2024"]),
            (2, 5, &["fen2023", "okafor2031"]),
            (2, 54, &["vale2024", "marsh2030"]),
            (6, 22, &["lark2029"]),
            (6, 78, &["after"]),
        ])
    );
    let read = scan(TextKind::Latex, text, &|| false);
    assert!(read.cites_everything);
    assert!(read.citations[0].narrative);
}

#[test]
fn an_unclosed_latex_citation_is_a_problem() {
    let read = scan(TextKind::Latex, "Fine \\cite{vale2024}. Broken \\cite{fen2023", &|| {
        false
    });
    assert_eq!(read.citations.len(), 1);
    assert_eq!(read.problems[0].kind, ScanProblem::Unclosed);
    assert_eq!(read.problems[0].place, Place { line: 1, column: 30 });
}

#[test]
fn typst_references_to_the_documents_own_labels_are_not_citations() {
    let text = "= Intro <intro>\nSee @intro and @vale2024[p. 7], #cite(<fen2023>) and #cite(label(\"okafor-2031\")).\n\
// @commented https://example.org/@path\n/* @a /* @b */ @c */ `@raw` mail me@example.org \\@escaped.\n\
#link(\"https://example.org/@handle\")[x] ends @marsh2030.\n";
    assert_eq!(
        keys(TextKind::Typst, text),
        of(&[
            (2, 16, &["vale2024"]),
            (2, 33, &["fen2023"]),
            (2, 54, &["okafor-2031"]),
            (5, 46, &["marsh2030"]),
        ])
    );
}

#[test]
fn a_pathological_text_is_read_in_part_and_says_so() {
    let text = "[@a ".repeat(200_000);
    let read = scan(TextKind::Markdown, &text, &|| false);
    assert!(!read.complete() || read.problems.iter().any(|p| p.kind == ScanProblem::Unclosed));
    let brackets = "[".repeat(300_000);
    let read = scan(TextKind::Markdown, &brackets, &|| false);
    assert!(read.citations.is_empty());
    let read = scan(TextKind::Latex, &"\\cite[".repeat(100_000), &|| false);
    assert!(read.citations.is_empty());
    let cancelled = scan(TextKind::Typst, &"@a ".repeat(100_000), &|| true);
    assert!(!cancelled.complete());
}

#[test]
fn kinds_follow_the_extension() {
    for (name, kind) in [
        ("a.md", Some(TextKind::Markdown)),
        ("a.QMD", Some(TextKind::Markdown)),
        ("a.Rmd", Some(TextKind::Markdown)),
        ("a.tex", Some(TextKind::Latex)),
        ("a.typ", Some(TextKind::Typst)),
        ("a.docx", None),
        ("a", None),
    ] {
        assert_eq!(TextKind::of(Path::new(name)), kind, "{name}");
    }
}

#[test]
fn comments_and_code_spans_are_read_in_one_pass_within_paragraphs() {
    assert_eq!(
        keys(TextKind::Markdown, "Use `<!--` then [@ghost].\n"),
        of(&[(1, 17, &["ghost"])])
    );
    assert_eq!(
        keys(TextKind::Markdown, "`\r\n\r\n[@ghost]\r\n\r\n`\r\n"),
        of(&[(3, 1, &["ghost"])])
    );
    assert_eq!(
        keys(TextKind::Markdown, "`\n  \n[@ghost]\n \n`\n"),
        of(&[(3, 1, &["ghost"])])
    );
}

#[test]
fn typst_strings_are_code_only_and_comments_never_hide_a_citation() {
    assert_eq!(
        keys(TextKind::Typst, "#cite(/* why */ <ghost>)\n"),
        of(&[(1, 1, &["ghost"])])
    );
    assert!(keys(TextKind::Typst, "#let note = \"@ghost\"\nText.\n").is_empty());
    assert_eq!(
        keys(TextKind::Typst, "#text(\"hi\") \"See @ghost\"\n"),
        of(&[(1, 18, &["ghost"])])
    );
    assert_eq!(
        keys(TextKind::Typst, "#figure([See @ghost], caption: \"@not\")\n"),
        of(&[(1, 14, &["ghost"])])
    );
}

#[test]
fn damaged_latex_and_unusual_delimiters_are_read_safely() {
    let read = scan(TextKind::Latex, "Fine \\cite{a}. \\cite[see", &|| false);
    assert_eq!(read.citations.len(), 1);
    assert_eq!(read.problems[0].kind, ScanProblem::Unclosed);
    assert_eq!(
        keys(TextKind::Latex, "\\verb§x§ \\cite{ghost}\n"),
        of(&[(1, 10, &["ghost"])])
    );
    let begins = "\\begin{".repeat(200_000);
    let _ = scan(TextKind::Latex, &begins, &|| false);
}

#[test]
fn typst_statements_strings_and_depth_follow_the_language() {
    assert_eq!(
        keys(TextKind::Typst, "#let x = 1; @ghost\n"),
        of(&[(1, 13, &["ghost"])])
    );
    assert!(keys(TextKind::Typst, "#text(\"#cite(<ghost>)\")\n").is_empty());
    let deep = format!("{}@ghost{}", "#box([".repeat(300), "])".repeat(300));
    assert!(!scan(TextKind::Typst, &deep, &|| false).complete());
    let verbatim = "\\begin{verbatim}\n😀😀 \\cite{hidden}\n\\end{verbatim} \\cite{shown}";
    assert_eq!(keys(TextKind::Latex, verbatim), of(&[(3, 16, &["shown"])]));
}

#[test]
fn a_typst_code_block_keeps_its_strings_until_it_closes() {
    assert!(keys(TextKind::Typst, r#"#let x = { let y = 1; "@ghost" };"#).is_empty());
    assert_eq!(
        keys(TextKind::Typst, "#let x = {\n  1\n}; then @ghost\n"),
        of(&[(3, 9, &["ghost"])])
    );
}

#[test]
fn typst_math_never_changes_code_depth() {
    assert_eq!(
        keys(TextKind::Typst, "#let x = $ \\{ $\n@ghost\n"),
        of(&[(2, 1, &["ghost"])])
    );
    assert!(keys(TextKind::Typst, "$ a @b $\n").is_empty());
    assert_eq!(
        keys(TextKind::Typst, "$ \"dollar: $\" $ @ghost\n"),
        of(&[(1, 17, &["ghost"])])
    );
}

#[test]
fn a_comment_inside_typst_math_holds_no_delimiter() {
    assert_eq!(
        keys(TextKind::Typst, "$ x /* \" */ $ @ghost\n"),
        of(&[(1, 15, &["ghost"])])
    );
}

#[test]
fn nested_comments_inside_typst_math_hold_no_delimiter() {
    assert_eq!(
        keys(TextKind::Typst, "$ x /* \" /* */ \" */ $ @ghost\n"),
        of(&[(1, 23, &["ghost"])])
    );
}

#[test]
fn a_quotation_just_before_a_citation_is_read_without_markup() {
    let quote = |kind, text: &str| -> Vec<Option<String>> {
        scan(kind, text, &|| false)
            .citations
            .into_iter()
            .map(|cited| cited.quote)
            .collect()
    };
    let some = |text: &str| Some(text.to_owned());
    assert_eq!(
        quote(
            TextKind::Markdown,
            "Vale wrote \"lanterns *guide* the\nships\" [@vale2024, p. 12], \u{201c}curly words\u{201d} [@fen2023] and [@noor2026].\n"
        ),
        [some("lanterns guide the ships"), some("curly words"), None]
    );
    assert_eq!(
        quote(
            TextKind::Latex,
            "As ``lanterns \\emph{guide} ships''~\\cite[p.~12]{vale2024} and \\enquote{tides\\,turn} \\parencite{fen2023}.\n"
        ),
        [some("lanterns guide ships"), some("tides turn")]
    );
    assert_eq!(
        quote(
            TextKind::Typst,
            "\"Lanterns _guide_ ships\" @vale2024 and #cite(<fen2023>).\n"
        ),
        [some("Lanterns guide ships"), None]
    );
    // A quotation in an earlier paragraph is not this citation's.
    assert_eq!(quote(TextKind::Markdown, "\"Lanterns.\"\n\n[@vale2024]\n"), [None]);
}

#[test]
fn a_latex_postnote_is_the_locator_of_its_one_key() {
    let read = scan(
        TextKind::Latex,
        "\\cite[p.~12]{vale2024} \\parencite[see][ch.~3]{fen2023} \\cite[12]{a,b} \\cite[see]{c}\n",
        &|| false,
    );
    let locators: Vec<Option<String>> = read
        .citations
        .iter()
        .flat_map(|cited| &cited.cites)
        .map(|cite| {
            cite.locator
                .as_ref()
                .map(|locator| format!("{:?} {}", locator.term, locator.value))
        })
        .collect();
    assert_eq!(
        locators,
        [Some("Page 12".into()), Some("Chapter 3".into()), None, None, None]
    );
}

#[test]
fn a_typst_supplement_is_the_locator_of_its_one_key() {
    let read = scan(
        TextKind::Typst,
        "See @vale2024[p. 12] and #cite(<fen2023>, supplement: [ch. 3]) and #cite(<a>, <b>, supplement: [p. 4]) and @c[see].\n",
        &|| false,
    );
    let locators: Vec<(String, Option<String>)> = read
        .citations
        .iter()
        .flat_map(|cited| &cited.cites)
        .map(|cite| {
            (
                cite.key.clone(),
                cite.locator
                    .as_ref()
                    .map(|locator| format!("{:?} {}", locator.term, locator.value)),
            )
        })
        .collect();
    assert_eq!(
        locators,
        [
            ("vale2024".into(), Some("Page 12".into())),
            ("fen2023".into(), Some("Chapter 3".into())),
            ("a".into(), None),
            ("b".into(), None),
            ("c".into(), None)
        ]
    );
}
