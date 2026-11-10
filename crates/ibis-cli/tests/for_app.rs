//! `ibis cite --for-app` gives the format Ibis uses for the app named, from the
//! app's own settings (decision 0042), from a `.bib` to final text, and says
//! whether the app pastes or copies (#1240). Each run has a home of its own,
//! so no real settings are ever read.
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

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

/// A home with the stable app's settings, or none at all.
fn home(name: &str, settings: Option<&str>) -> Result<PathBuf> {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("for-app-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root)?;
    if let Some(text) = settings {
        let folder = config(&root).join("com.ibiscite.ibis");
        std::fs::create_dir_all(&folder)?;
        std::fs::write(folder.join("settings.json"), text)?;
    }
    Ok(root)
}

fn config(root: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        root.join("Library/Application Support")
    } else if cfg!(windows) {
        root.join("AppData")
    } else {
        root.join("config")
    }
}

fn run_in(root: &Path, args: &[&str]) -> Result<(u8, String, String)> {
    let output = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .arg("--file")
        .arg(library()?)
        .args(args)
        .env("HOME", root)
        .env("APPDATA", config(root))
        .env("XDG_CONFIG_HOME", config(root))
        .output()?;
    let code = output.status.code().ok_or("signal")?;
    Ok((
        u8::try_from(code)?,
        String::from_utf8(output.stdout)?,
        String::from_utf8(output.stderr)?,
    ))
}

/// With nothing chosen in the app: no settings file at all.
fn run(args: &[&str]) -> Result<(u8, String, String)> {
    static EMPTY: std::sync::OnceLock<std::result::Result<PathBuf, String>> = std::sync::OnceLock::new();
    let root = EMPTY
        .get_or_init(|| home("empty", None).map_err(|error| error.to_string()))
        .clone()?;
    run_in(&root, args)
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
        // An unknown app and a browser are documents; a code editor takes keys.
        ("com.example.unknown", "(Taylor, 2026)\n"),
        ("com.apple.Safari", "(Taylor, 2026)\n"),
        ("com.microsoft.VSCode", "[@taylor2026]\n"),
    ];
    for (app, expected) in cases {
        let (code, text, errors) = run(&["cite", "taylor2026", "--for-app", app])?;
        assert_eq!((code, text.as_str()), (0, expected), "{app}: {errors}");
    }
    Ok(())
}

#[test]
fn style_and_format_stand_in_for_your_format_of_their_own_kind() -> Result {
    let cite = |app: &str, extra: &[&str]| -> Result<String> {
        let mut args = vec!["cite", "taylor2026", "--for-app", app];
        args.extend_from_slice(extra);
        Ok(run(&args)?.1)
    };
    assert_eq!(cite("md.obsidian", &["--format", "typst"])?, "@taylor2026\n");
    assert_eq!(
        cite("com.example.unknown", &["--style", "ieee"])?,
        "A. Taylor, Synthetic signals. Imaginary Press, 2026.\n"
    );
    // Word is a formatted app, so --style chooses its style.
    assert_eq!(
        cite("com.microsoft.Word", &["--style", "ieee"])?,
        "A. Taylor, Synthetic signals. Imaginary Press, 2026.\n"
    );
    // A format of the other kind never applies: Word stays formatted, Obsidian keys.
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
    assert_eq!(word["from"], "suggestion");

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
    assert_eq!(text, "(Taylor, 2026)\n");
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
    assert!(errors.contains("in Pandoc"), "{errors}");
    assert!(
        !errors.contains("Ibis pastes"),
        "a refused citation goes nowhere: {errors}"
    );
    Ok(())
}

#[test]
fn an_unreadable_library_fails_the_same_with_an_app_named() -> Result {
    // The library fails before any settings are read.
    use ibis_engine::Engine;
    use std::io::Cursor;
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

#[test]
fn the_apps_own_settings_choose_as_the_picker_does() -> Result {
    let root = home(
        "chosen",
        Some(
            r#"{"schema_version":2,"output":"chicago-author-date","key_output":"typst",
                "app_outputs":[{"app":{"system":"windows","id":"winword.exe"},"output":"modern-language-association"}]}"#,
        ),
    )?;
    let cite = |app: &str| -> Result<String> { Ok(run_in(&root, &["cite", "taylor2026", "--for-app", app])?.1) };
    // Word's own format, then your format of each kind.
    // Windows ids name the same app on every system that runs this.
    assert_eq!(cite("WINWORD.EXE")?, "(Taylor)\n");
    assert_eq!(cite("soffice.exe")?, "(Taylor 2026)\n");
    assert_eq!(cite("obsidian.exe")?, "@taylor2026\n");
    let (_, json, _) = run_in(&root, &["cite", "taylor2026", "--for-app", "WINWORD.EXE", "--json"])?;
    assert_eq!(
        serde_json::from_str::<Value>(&json)?["data"]["destination"]["from"],
        "app"
    );
    // Settings that cannot be read stop the command with the reason; nothing falls back.
    let damaged = home("damaged", Some("{"))?;
    let (code, text, errors) = run_in(&damaged, &["cite", "taylor2026", "--for-app", "WINWORD.EXE"])?;
    assert_eq!((code, text.as_str()), (3, ""), "{errors}");
    assert!(errors.contains("cannot read the formats chosen in Ibis"), "{errors}");
    Ok(())
}
