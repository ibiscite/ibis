//! The public website prints these exact CLI results, including the refusal.
use std::{io::Cursor, path::Path};

#[test]
fn website_transcripts_match_the_cli() -> Result<(), Box<dyn std::error::Error>> {
    // Isolate terminal environment from other parallel tests and the caller's
    // window size. The child stdout is a pipe; OutputMode still selects the
    // terminal renderer, with an explicit 80-column fallback.
    if std::env::var_os("IBIS_TERMINAL_TRANSCRIPT").is_none() {
        for no_color in [true, false] {
            let mut child = std::process::Command::new(std::env::current_exe()?);
            child
                .args(["--exact", "website_transcripts_match_the_cli", "--nocapture"])
                .env("IBIS_TERMINAL_TRANSCRIPT", "1")
                .env("COLUMNS", "80")
                .env("CLICOLOR_FORCE", "1");
            if no_color {
                child.env("NO_COLOR", "").env("TERM", "xterm");
            } else {
                child.env_remove("NO_COLOR").env("TERM", "dumb");
            }
            let output = child.output()?;
            assert!(
                output.status.success(),
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        return Ok(());
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let temporary = tempfile::tempdir()?;
    ibis_engine::identity::use_key_file(temporary.path().join("library-ids.key"), true);
    let transcript = std::fs::read_to_string(root.join("website/content/terminal.txt"))?;
    let mut commands = 0;
    let blocks: Vec<_> = transcript
        .strip_prefix("$ ")
        .ok_or("missing command")?
        .split("\n$ ")
        .collect();
    for (index, block) in blocks.iter().enumerate() {
        let (command, expected) = block.split_once('\n').ok_or("missing output")?;
        let expected = if index + 1 < blocks.len() {
            format!("{expected}\n")
        } else {
            expected.to_owned()
        };
        // The transcript's only shell syntax is double-quoted arguments. Do not invoke a shell.
        let mut quoted = false;
        let mut word = String::new();
        let mut args = vec![];
        for character in command.chars().chain(std::iter::once(' ')) {
            match character {
                '"' => quoted = !quoted,
                c if c.is_whitespace() && !quoted => {
                    if !word.is_empty() {
                        args.push(std::mem::take(&mut word));
                    }
                }
                c => word.push(c),
            }
        }
        assert!(!quoted, "unclosed quote: {command}");
        assert_eq!(args.first().map(String::as_str), Some("ibis"));
        let file = args
            .windows(2)
            .position(|pair| pair == ["--file", "sample.bib"])
            .ok_or("sample library required")?;
        args[file + 1] = root
            .join("fixtures/website/sample.bib")
            .to_str()
            .ok_or("path")?
            .to_owned();
        let (mut output, mut errors) = (vec![], vec![]);
        let code = ibis::run_with(
            ibis_engine::Engine::new().map_err(|_| "engine")?,
            args.into_iter().map(Into::into).collect(),
            Cursor::new([]),
            &mut output,
            &mut errors,
            ibis::OutputMode::Terminal,
        );
        if expected.starts_with("ibis: ") {
            assert_ne!(code, 0, "{command}");
            assert!(output.is_empty(), "refusals never print a citation");
            let shown = String::from_utf8(errors)?.replace(
                root.join("fixtures/website/sample.bib").to_str().ok_or("path")?,
                "sample.bib",
            );
            assert_eq!(shown, expected, "{command}");
        } else {
            assert_eq!(code, 0, "{command}: {}", String::from_utf8_lossy(&errors));
            assert!(errors.is_empty(), "{command}");
            assert_eq!(String::from_utf8(output)?, expected, "{command}");
        }
        commands += 1;
    }
    assert!(commands >= 3, "keep search, citation and refusal examples");
    // These outputs also appear in the home tabs and the fixed shorthand example.
    let apps: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(
        root.join("website/content/writing-apps.json"),
    )?)?;
    for app in apps {
        let (mut output, mut errors) = (vec![], vec![]);
        let args = vec![
            "ibis".into(),
            "cite".into(),
            "vale2024lanterns".into(),
            "--locator".into(),
            "p. 12".into(),
            "--format".into(),
            app["format"].as_str().ok_or("format")?.into(),
            "--file".into(),
            root.join("fixtures/website/sample.bib").into_os_string(),
        ];
        let code = ibis::run_with(
            ibis_engine::Engine::new().map_err(|_| "engine")?,
            args,
            Cursor::new([]),
            &mut output,
            &mut errors,
            ibis::OutputMode::Plain,
        );
        assert_eq!(code, 0, "{}", String::from_utf8_lossy(&errors));
        assert!(errors.is_empty());
        assert_eq!(
            String::from_utf8(output)?,
            format!("{}\n", app["output"].as_str().ok_or("output")?)
        );
    }
    Ok(())
}
