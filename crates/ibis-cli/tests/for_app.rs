//! `ibis cite --for-app` gives the output Ibis uses for the app named, from a
//! `.bib` to final text, and says whether the app pastes or copies (#1240).
use ibis_engine::Engine;
use serde_json::Value;
use std::{io::Cursor, path::PathBuf};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const LIBRARY: &str =
    "@book{taylor2026,title={Synthetic signals},author={Taylor, Avery},year={2026},publisher={Imaginary Press}}\n";

fn library() -> Result<PathBuf> {
    static WRITTEN: std::sync::OnceLock<std::io::Result<PathBuf>> = std::sync::OnceLock::new();
    let written = WRITTEN.get_or_init(|| {
        let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("for-app.bib");
        std::fs::write(&path, LIBRARY).map(|()| path)
    });
    Ok(written.as_ref().map_err(|error| error.to_string())?.clone())
}

fn run(args: &[&str]) -> Result<(u8, String, String)> {
    let path = library()?;
    let mut all = vec!["ibis", "--file", path.to_str().ok_or("path")?];
    all.extend_from_slice(args);
    let (mut output, mut errors) = (vec![], vec![]);
    let code = ibis::run_with(
        Engine::new().map_err(|_| "engine")?,
        all.into_iter().map(Into::into).collect(),
        Cursor::new(vec![]),
        &mut output,
        &mut errors,
        ibis::OutputMode::Plain,
    );
    Ok((code, String::from_utf8(output)?, String::from_utf8(errors)?))
}

#[test]
fn each_app_gets_its_output_from_the_bib_to_final_text() -> Result {
    let cases = [
        // The Word row, on each system's ID, with the output in final text.
        ("com.microsoft.Word", "(Taylor, 2026)\n"),
        ("WINWORD.EXE", "(Taylor, 2026)\n"),
        (
            r"C:\Program Files\Microsoft Office\root\Office16\WINWORD.EXE",
            "(Taylor, 2026)\n",
        ),
        ("md.obsidian", "[@taylor2026]\n"),
        ("obsidian.exe", "[@taylor2026]\n"),
        ("TeXShop", "\\autocite{taylor2026}\n"),
        ("texstudio.exe", "\\autocite{taylor2026}\n"),
        // An unknown app takes the default output, as does an app that asks.
        ("com.example.unknown", "[@taylor2026]\n"),
        ("com.apple.Safari", "[@taylor2026]\n"),
    ];
    for (app, expected) in cases {
        let (code, text, errors) = run(&["cite", "taylor2026", "--for-app", app])?;
        assert_eq!((code, text.as_str()), (0, expected), "{app}: {errors}");
    }
    Ok(())
}

#[test]
fn style_and_format_are_the_default_an_app_without_its_own_output_takes() -> Result {
    let cite = |app: &str, extra: &[&str]| -> Result<String> {
        let mut args = vec!["cite", "taylor2026", "--for-app", app];
        args.extend_from_slice(extra);
        Ok(run(&args)?.1)
    };
    assert_eq!(cite("com.example.unknown", &["--format", "typst"])?, "@taylor2026\n");
    assert_eq!(cite("com.example.unknown", &["--style", "apa"])?, "(Taylor, 2026)\n");
    // Word is a formatted app, so --style chooses its style.
    assert_eq!(
        cite("com.microsoft.Word", &["--style", "ieee"])?,
        "A. Taylor, Synthetic signals. Imaginary Press, 2026.\n"
    );
    // The table wins over a fallback: Word stays formatted, Obsidian stays Pandoc.
    assert_eq!(cite("com.microsoft.Word", &["--format", "typst"])?, "(Taylor, 2026)\n");
    assert_eq!(cite("md.obsidian", &["--style", "apa"])?, "[@taylor2026]\n");
    Ok(())
}

#[test]
fn the_answer_says_whether_the_app_pastes_or_copies() -> Result {
    let destination = |app: &str| -> Result<Value> {
        let (code, text, _) = run(&["cite", "taylor2026", "--for-app", app, "--json"])?;
        assert_eq!(code, 0, "{app}");
        Ok(serde_json::from_str::<Value>(&text)?["data"]["destination"].clone())
    };
    let word = destination("com.microsoft.Word")?;
    assert_eq!(word["name"], "Microsoft Word");
    assert_eq!(word["delivery"]["action"], "paste");
    assert_eq!(word["output"]["style"], "apa");

    let terminal = destination("com.apple.Terminal")?;
    assert_eq!(terminal["delivery"]["action"], "copy");
    assert_eq!(terminal["delivery"]["reason"], "terminal");

    assert_eq!(destination("com.apple.Safari")?["output_question"], "browser");

    let unknown = destination("com.example.unknown")?;
    assert_eq!(unknown["name"], Value::Null);
    assert_eq!(unknown["delivery"]["action"], "copy");
    Ok(())
}

#[test]
fn plain_output_stays_the_citation_and_the_note_goes_to_stderr() -> Result {
    let (_, text, errors) = run(&["cite", "taylor2026", "--for-app", "com.apple.Terminal"])?;
    assert_eq!(text, "[@taylor2026]\n");
    assert!(errors.contains("Ibis copies for Terminal"), "{errors}");

    let (_, _, errors) = run(&["cite", "taylor2026", "--for-app", "md.obsidian"])?;
    assert!(errors.contains("Ibis pastes into Obsidian"), "{errors}");

    let (_, _, errors) = run(&["cite", "taylor2026", "--for-app", "com.google.Chrome"])?;
    assert!(errors.contains("asks once which output Chrome needs"), "{errors}");

    // Without the flag nothing changes.
    let (_, _, errors) = run(&["cite", "taylor2026"])?;
    assert_eq!(errors, "");
    Ok(())
}

#[test]
fn a_missing_id_or_a_request_with_its_own_output_is_refused() -> Result {
    let (code, _, errors) = run(&["cite", "taylor2026", "--for-app", ""])?;
    assert_eq!(code, 2, "{errors}");
    let (code, _, errors) = run(&["cite", "--request", "--for-app", "md.obsidian"])?;
    assert_eq!(code, 2, "{errors}");
    Ok(())
}

#[test]
fn a_refused_citation_names_the_output_the_app_chose() -> Result {
    // Word's formatted output and an unknown key both fail as any citation does.
    let (code, text, errors) = run(&["cite", "nobody2026", "--for-app", "com.microsoft.Word"])?;
    assert_eq!((code, text.as_str()), (4, ""), "{errors}");

    // Obsidian's Pandoc cannot print a bibliography; the app's choice is what is named.
    let (code, text, errors) = run(&[
        "cite",
        "taylor2026",
        "--for-app",
        "md.obsidian",
        "--style",
        "apa",
        "--bibliography",
    ])?;
    assert_eq!((code, text.as_str()), (1, ""), "{errors}");
    assert!(errors.contains("pandoc"), "{errors}");
    assert!(
        !errors.contains("Ibis pastes"),
        "a refused citation goes nowhere: {errors}"
    );
    Ok(())
}

#[test]
fn an_unreadable_library_fails_the_same_with_an_app_named() -> Result {
    let (mut output, mut errors) = (vec![], vec![]);
    let code = ibis::run_with(
        Engine::new().map_err(|_| "engine")?,
        [
            "ibis",
            "--file",
            "/invented/missing.bib",
            "cite",
            "taylor2026",
            "--for-app",
            "md.obsidian",
        ]
        .into_iter()
        .map(Into::into)
        .collect(),
        Cursor::new(vec![]),
        &mut output,
        &mut errors,
        ibis::OutputMode::Plain,
    );
    assert_ne!(code, 0);
    assert!(output.is_empty());
    assert!(!String::from_utf8(errors)?.contains("Ibis pastes"));
    Ok(())
}
