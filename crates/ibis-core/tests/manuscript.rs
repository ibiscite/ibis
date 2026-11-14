#![allow(clippy::unwrap_used)]

use ibis_core::manuscript::{Limits, Occurrence, ScanError, scan_tex};

fn scan(text: &str) -> Vec<Occurrence<'_>> {
    scan_tex(text, Limits::default(), &|| false).unwrap()
}

fn keys(text: &str) -> Vec<&str> {
    scan(text).into_iter().map(|o| o.key).collect()
}

#[test]
fn literal_commands_keep_written_order_repetitions_and_byte_ranges() {
    let text = "é中 \\cite{alpha, beta,alpha} \\textcite{文献}";
    assert_eq!(
        scan(text).into_iter().map(|o| (o.key, o.range)).collect::<Vec<_>>(),
        [("alpha", 12..17), ("beta", 19..23), ("alpha", 24..29), ("文献", 41..47)]
    );
    for occurrence in scan(text) {
        assert_eq!(&text[occurrence.range], occurrence.key);
    }
}

#[test]
fn shared_reader_commands_and_notes_have_one_key_grammar() {
    for command in [
        "cite",
        "citep",
        "citet",
        "citeauthor",
        "citeyearpar",
        "autocite",
        "parencite",
        "textcite",
    ] {
        let text = format!(r"\{command}*[see {{[nested]}}][p. 4]{{a:b_1, a:b_1}}");
        assert_eq!(keys(&text), ["a:b_1", "a:b_1"]);
    }
    for command in ["autocites", "parencites", "textcites"] {
        let text = format!(r"\{command}[see]{{first}}[p. 2]{{second,first}}");
        assert_eq!(keys(&text), ["first", "second", "first"]);
    }
}

#[test]
fn citations_nested_in_notes_cannot_be_silently_dropped() {
    for citation in [
        r"\cite[see {\cite{nested}}]{outer}",
        r"\citep[see \textcite{nested}][p. 4]{outer}",
        r"\autocites{first}[see {\cite{nested}}]{outer}",
    ] {
        let text = format!(r"\cite{{earlier}} {citation} \cite{{later}}");
        assert!(
            matches!(
                scan_tex(&text, Limits::default(), &|| false),
                Err(ScanError::Unsupported { .. })
            ),
            "nested citation requires interpretation: {citation}"
        );
    }
}

#[test]
fn comment_environment_requires_its_complete_terminator_line() {
    for newline in ["\n", "\r\n"] {
        let text = [
            r"\cite{earlier}",
            r"\begin{comment}",
            r"embedded \end{comment} \cite{hidden}",
            r"\end{comment} trailing \cite{also-hidden}",
            r" \end{comment}",
            r"\cite{still-hidden}",
            r"\end{comment}   ",
            r"\cite{later}",
        ]
        .join(newline);
        assert_eq!(keys(&text), ["earlier", "later"]);
    }
    let unclosed = "\\begin{comment}\nembedded \\end{comment} \\cite{hidden}";
    assert!(matches!(
        scan_tex(unclosed, Limits::default(), &|| false),
        Err(ScanError::Incomplete { .. })
    ));
}

#[test]
fn token_interpretation_cannot_expose_quoted_citations() {
    for quoted in [
        r"\detokenize{\cite{hidden}}",
        r"\string\cite{hidden}",
        r"\meaning\cite{hidden}",
        r"\noexpand\cite{hidden}",
        r"\unexpanded{\cite{hidden}}",
        r"\scantokens{\cite{hidden}}",
        r"\lowercase{\cite{HIDDEN}}",
        r"\uppercase{\cite{hidden}}",
        r"\toks0={\cite{hidden}}",
        r"\cite[\detokenize{\cite{hidden}}]{outer}",
    ] {
        let text = format!(r"\cite{{earlier}} {quoted} \cite{{later}}");
        assert!(
            matches!(
                scan_tex(&text, Limits::default(), &|| false),
                Err(ScanError::Unsupported { .. })
            ),
            "quoted citation requires interpretation: {quoted}"
        );
    }
}

#[test]
fn end_document_discards_even_unsupported_or_incomplete_tails() {
    for tail in [r"\cite{discarded}", r"\input{discarded}", r"\cite{", r"\begin{comment}"] {
        let text = format!(r"\begin{{document}}\cite{{earlier,earlier}}\end{{document}}{tail}");
        assert_eq!(keys(&text), ["earlier", "earlier"]);
    }
    assert_eq!(keys(r"\end{document}\cite{discarded}"), Vec::<&str>::new());
    assert_eq!(keys(r"\end{other}\cite{kept}"), ["kept"]);
}

#[test]
fn comments_verbatim_and_other_formats_never_contribute_keys() {
    let text = concat!(
        "% \\cite{comment}\r\n",
        "\\verb|\\cite{inline}| \\verb*+\\cite{starred}+\n",
        "\\begin{verbatim}\\cite{environment}\\end{verbatim}\n",
        "\\begin{Verbatim}[options]\\cite{fancy}\\end{Verbatim}\n",
        "\\begin{lstlisting}\\cite{listing}\\end{lstlisting}\n",
        "\\begin{minted}{tex}\\cite{minted}\\end{minted}\n",
        "\\\\cite{escaped-command} \\\\% \\cite{still-comment}\n",
        "\\% \\cite{real} \\\\\\% \\cite{also-real}\n",
        "[@pandoc] #cite(<typst>) @bare <cite data-cite=\"html\">",
    );
    assert_eq!(keys(text), ["real", "also-real"]);
    assert!(scan("").is_empty());
}

#[test]
fn malformed_arguments_and_unclosed_exclusions_are_incomplete() {
    for damaged in [
        r"\cite",
        r"\cite{",
        r"\cite{key",
        r"\cite[see]",
        r"\cite[see",
        r"\cite[see]{",
        r"\cite{}",
        r"\verb|unclosed",
        "\\verb|line\nbreak|",
        r"\begin{verbatim}no end",
        r"\begin{verbatim",
        r"\begin",
    ] {
        let text = format!(r"\cite{{earlier}} {damaged}");
        let Err(ScanError::Incomplete { range }) = scan_tex(&text, Limits::default(), &|| false) else {
            panic!("expected incomplete: {damaged:?}");
        };
        assert!(!range.is_empty() && text.get(range).is_some());
    }
}

#[test]
fn evaluation_and_unsupported_commands_cannot_claim_completeness() {
    for unsupported in [
        r"\cite{\key}",
        r"\cite{prefix#1}",
        r"\cite{{nested}}",
        r"\cite{a~b}",
        "\\cite{a%comment\nb}",
        r"\newcommand{\foo}{\cite{body}} \foo",
        r"\def\foo{\cite{body}}",
        r"\renewcommand{\cite}[1]{}",
        r"\let\cite\foo",
        r"\NewDocumentCommand{\foo}{m}{\cite{#1}}",
        r"\cite[\def\foo{\cite{body}}]{key}",
        r"\cite[\input{chapter}]{key}",
        r"\input{chapter}",
        r"\include{chapter}",
        r"\subfile{chapter}",
        r"\nocite{*}",
        r"\footcite{key}",
        r"\iftrue\cite{conditional}\fi",
        r"\catcode`\%=12",
        r"\lstinline|\cite{inline}|",
    ] {
        let text = format!(r"\cite{{earlier}} {unsupported} \cite{{later}}");
        let Err(ScanError::Unsupported { range }) = scan_tex(&text, Limits::default(), &|| false) else {
            panic!("expected unsupported: {unsupported:?}");
        };
        assert!(!range.is_empty() && text.get(range).is_some());
    }
}

#[test]
fn cancellation_and_bounds_discard_all_occurrences() {
    let text = format!(r"\cite{{earlier}} {} \cite{{later}}", "ordinary text ".repeat(1000));
    assert_eq!(scan_tex(&text, Limits::default(), &|| true), Err(ScanError::Cancelled));
    let long = "文".repeat(1000);
    for tail in [
        text.clone(),
        format!(r"\cite[{long}]{{later}}"),
        format!(r"\begin{{verbatim}}{long}\end{{verbatim}}"),
        format!("\\begin{{comment}}\n{long}\n\\end{{comment}}"),
        format!("% {long}"),
    ] {
        let polls = std::cell::Cell::new(0);
        let stop = || {
            polls.set(polls.get() + 1);
            polls.get() >= 3
        };
        let input = format!(r"\cite{{earlier}} {tail}");
        assert_eq!(scan_tex(&input, Limits::default(), &stop), Err(ScanError::Cancelled));
        assert_eq!(polls.get(), 3);
    }
    for limits in [
        Limits {
            max_bytes: text.len() - 1,
            ..Limits::default()
        },
        Limits {
            max_occurrences: 1,
            ..Limits::default()
        },
    ] {
        assert_eq!(scan_tex(&text, limits, &|| false), Err(ScanError::Limit));
    }
}
