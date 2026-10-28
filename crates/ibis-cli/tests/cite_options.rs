//! `ibis cite KEY --locator/--prefix/--suffix/--omit-author` builds the same
//! CitationRequest the picker sends, so both print the same envelope (#1250).
use ibis_engine::Engine;
use serde_json::{Value, json};
use std::{io::Cursor, path::PathBuf};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const LIBRARY: &str = "@book{taylor2026,title={Synthetic signals},author={Taylor, Avery},year={2026},publisher={Imaginary Press}}\n\
@article{morgan2025,title={Synthetic noise},author={Morgan, Ann and Lee, Kim},year={2025},journal={Invented Letters},pages={1--20}}\n";

fn library() -> Result<PathBuf> {
    static WRITTEN: std::sync::OnceLock<std::io::Result<PathBuf>> = std::sync::OnceLock::new();
    let written = WRITTEN.get_or_init(|| {
        let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("cite-options.bib");
        std::fs::write(&path, LIBRARY).map(|()| path)
    });
    Ok(written.as_ref().map_err(|error| error.to_string())?.clone())
}

fn run(args: &[&str], input: &[u8]) -> Result<(u8, String, String)> {
    let path = library()?;
    let mut all = vec!["ibis", "--file", path.to_str().ok_or("path")?];
    all.extend_from_slice(args);
    let (mut output, mut errors) = (vec![], vec![]);
    let code = ibis::run_with(
        Engine::new().map_err(|_| "engine")?,
        all.into_iter().map(Into::into).collect(),
        Cursor::new(input.to_vec()),
        &mut output,
        &mut errors,
        ibis::OutputMode::Plain,
    );
    Ok((code, String::from_utf8(output)?, String::from_utf8(errors)?))
}

/// The item the picker would send for a row: its id, cite fingerprint and options.
fn picked(key: &str, options: &Value) -> Result<Value> {
    let (_, shown, _) = run(&["show", key, "--json"], b"")?;
    let shown: Value = serde_json::from_str(&shown)?;
    let mut item = json!({
        "item": shown["data"]["object"]["data"]["id"],
        "expected_cite": shown["data"]["fingerprints"]["cite"],
    });
    for (field, value) in options.as_object().ok_or("options")? {
        item[field] = value.clone();
    }
    Ok(item)
}

#[test]
fn per_key_flags_match_the_picker_request_in_every_output() -> Result {
    let plain = |text: &str| json!([{"text":text,"italic":false,"bold":false,"script":"normal"}]);
    // Each key, the flags typed after it, and the options the picker attaches to that item.
    let cases: [&[(&str, &[&str], Value)]; 5] = [
        &[(
            "taylor2026",
            &["--locator", "p. 12"],
            json!({"locator":{"term":"page","value":"12"}}),
        )],
        &[
            (
                "taylor2026",
                &["--locator", "12-14"],
                json!({"locator":{"term":"page","value":"12-14"}}),
            ),
            (
                "morgan2025",
                &["--locator", "ch. 3"],
                json!({"locator":{"term":"chapter","value":"3"}}),
            ),
        ],
        &[
            (
                "taylor2026",
                &["--prefix", "see", "--suffix", "for a review"],
                json!({"prefix":plain("see"),"suffix":plain("for a review")}),
            ),
            ("morgan2025", &[], json!({})),
        ],
        &[("taylor2026", &["--omit-author"], json!({"suppress_author":true}))],
        &[
            ("taylor2026", &["--omit-author"], json!({"suppress_author":true})),
            ("morgan2025", &["--omit-author"], json!({"suppress_author":true})),
        ],
    ];
    let outputs: [(&[&str], Value); 6] = [
        (&[], json!({"kind":"key","syntax":"pandoc","command":null})),
        (
            &["--format", "autocite"],
            json!({"kind":"key","syntax":"biblatex","command":"autocite"}),
        ),
        (
            &["--format", "typst"],
            json!({"kind":"key","syntax":"typst","command":null}),
        ),
        (&["--style", "apa"], json!({"kind":"csl","style":"apa"})),
        (
            &["--style", "chicago-author-date"],
            json!({"kind":"csl","style":"chicago-author-date"}),
        ),
        (
            &["--style", "modern-language-association"],
            json!({"kind":"csl","style":"modern-language-association"}),
        ),
    ];
    for keys in cases {
        let flags: Vec<&str> = keys
            .iter()
            .flat_map(|(key, flags, _)| std::iter::once(*key).chain(flags.iter().copied()))
            .collect();
        let items = keys
            .iter()
            .map(|(key, _, options)| picked(key, options))
            .collect::<Result<Vec<_>>>()?;
        for (output_flags, output) in &outputs {
            let mut human = vec!["cite", "--json"];
            human.extend_from_slice(output_flags);
            human.extend_from_slice(&flags);
            let request = json!({
                "items": items,
                "output": output,
                "form": "normal",
                "keep_order": output["kind"] == "key",
            });
            let (code, from_flags, _) = run(&human, b"")?;
            let (request_code, from_request, _) =
                run(&["cite", "--request", "--json"], &serde_json::to_vec(&request)?)?;
            assert_eq!((code, &from_flags), (request_code, &from_request), "{human:?}");
            let envelope: Value = serde_json::from_str(&from_flags)?;
            assert_eq!(envelope["ok"], true, "{human:?}: {from_flags}");
        }
    }
    Ok(())
}

#[test]
fn flags_print_what_each_output_shows_and_report_what_it_cannot() -> Result {
    let (code, text, errors) = run(
        &[
            "cite",
            "taylor2026",
            "--locator",
            "p. 12",
            "--prefix",
            "see",
            "morgan2025",
            "--omit-author",
        ],
        b"",
    )?;
    assert_eq!(
        (code, text.as_str(), errors.as_str()),
        (0, "[see @taylor2026, p. 12; -@morgan2025]\n", "")
    );
    let (code, text, _) = run(&["cite", "--style", "apa", "taylor2026", "--locator", "pp. 12-14"], b"")?;
    assert_eq!((code, text.as_str()), (0, "(Taylor, 2026, pp. 12–14)\n"));
    let (code, text, errors) = run(&["cite", "--style", "apa", "taylor2026", "--prefix", "see"], b"")?;
    assert_eq!((code, text.as_str()), (0, "(see Taylor, 2026)\n"));
    assert!(errors.is_empty(), "{errors}");
    let (code, _, errors) = run(&["cite", "--format", "keys", "taylor2026", "--locator", "12"], b"")?;
    assert_eq!(code, 1);
    assert!(
        errors.contains("keys cannot show the locator for 'taylor2026'"),
        "{errors}"
    );
    Ok(())
}

#[test]
fn misplaced_repeated_or_unreadable_flags_are_usage_errors() -> Result {
    for (args, message) in [
        (
            &["cite", "--locator", "12", "taylor2026"][..],
            "ibis: --locator must follow the key it belongs to.\n",
        ),
        (
            &["cite", "taylor2026", "--locator", "12", "--locator", "13"],
            "ibis: --locator is given twice for 'taylor2026'.\n",
        ),
        (
            &["cite", "--omit-author", "taylor2026"],
            "ibis: --omit-author must follow the key it belongs to.\n",
        ),
        (
            &[
                "cite",
                "taylor2026",
                "--omit-author",
                "morgan2025",
                "--omit-author",
                "--omit-author",
            ],
            "ibis: --omit-author is given twice for 'morgan2025'.\n",
        ),
        (
            &["cite", "taylor2026", "--locator", "somewhere"],
            "ibis: --locator 'somewhere' is not a locator Ibis reads; write it like \"p. 12\", \"ch. 3\" or \"12-14\".\n",
        ),
    ] {
        let (code, text, errors) = run(args, b"")?;
        assert_eq!((code, text.as_str(), errors.as_str()), (2, "", message), "{args:?}");
    }
    let (code, _, _) = run(&["cite", "--request", "--locator", "12"], b"{}")?;
    assert_eq!(code, 2);
    Ok(())
}

#[test]
fn a_line_break_in_a_bad_argument_cannot_forge_a_diagnostic_line() -> Result {
    let (code, _, errors) = run(&["cite", "--bad\nibis: forged"], b"")?;
    assert_eq!(code, 2);
    assert!(!errors.lines().any(|line| line.starts_with("ibis: forged")), "{errors}");
    assert!(errors.contains("--bad\\nibis: forged"), "{errors}");
    Ok(())
}
