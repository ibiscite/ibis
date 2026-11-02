//! Explicit desktop utility tests launch only a disposable fake binary.
#[cfg(unix)]
#[test]
fn toggle_succeeds_only_when_ibis_answers() {
    use std::{os::unix::fs::PermissionsExt, path::Path, process::Command, time::Duration};
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-toggle/Ibis fake app");
    std::fs::create_dir_all(&directory).unwrap_or_else(|error| panic!("{error}"));
    let desktop = directory.join("ibis-desktop");
    let cli = directory.join("ibis");
    std::fs::copy(env!("CARGO_BIN_EXE_ibis"), &cli).unwrap_or_else(|error| panic!("{error}"));
    let marker = desktop.with_file_name("ibis-desktop.args");
    // A fake Ibis records its arguments and, like the app, answers in the receipt file.
    let fake = |answers: bool| {
        let answer = if answers {
            "printf 'ibis toggle\\n' > \"$3\"\n"
        } else {
            ""
        };
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$0.part\"\nmv \"$0.part\" \"$0.args\"\n{answer}printf 'child output must be private\\n'\n"
        );
        // Replace, never rewrite in place, so a running copy cannot see a half-written script.
        let next = desktop.with_extension("next");
        std::fs::write(&next, script).unwrap_or_else(|error| panic!("{error}"));
        std::fs::set_permissions(&next, std::fs::Permissions::from_mode(0o700))
            .unwrap_or_else(|error| panic!("{error}"));
        std::fs::rename(&next, &desktop).unwrap_or_else(|error| panic!("{error}"));
    };
    let run = |extra: &[&str]| {
        let mut args = vec!["toggle", "--json"];
        args.extend_from_slice(extra);
        for _ in 0..100 {
            match Command::new(&cli).args(&args).output() {
                Err(error) if error.raw_os_error() == Some(26) => std::thread::sleep(Duration::from_millis(20)),
                result => return result.unwrap_or_else(|error| panic!("{error}")),
            }
        }
        Command::new(&cli)
            .args(&args)
            .output()
            .unwrap_or_else(|error| panic!("{error}"))
    };
    let json = |output: &std::process::Output| {
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap_or_else(|error| panic!("{error}"))
    };

    // Ibis answers, whether it opens the picker or hands the toggle to a running copy.
    fake(true);
    let output = run(&[]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        json(&output),
        serde_json::json!({"ibis":1,"ok":true,"data":{"toggled":true}})
    );
    assert!(output.stderr.is_empty());
    let args = std::fs::read_to_string(&marker).unwrap_or_else(|error| panic!("{error}"));
    let args: Vec<&str> = args.lines().collect();
    assert_eq!(&args[..2], ["toggle", "--receipt"]);
    assert!(!Path::new(args[2]).exists(), "the receipt file is removed afterwards");

    // A program that exits without answering is not Ibis.
    fake(false);
    let output = run(&[]);
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(
        json(&output),
        serde_json::json!({"ibis":1,"ok":false,"error":{"code":"bad-input","reason":"not-ibis"}})
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("is not Ibis"));

    // A program that cannot run is not Ibis either; a missing one is missing.
    std::fs::set_permissions(&desktop, std::fs::Permissions::from_mode(0o600))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(run(&[]).status.code(), Some(3));
    let missing = directory.join("no such app");
    let output = run(&["--app", missing.to_str().unwrap_or_default()]);
    assert_eq!(output.status.code(), Some(4));
    assert_eq!(
        json(&output),
        serde_json::json!({"ibis":1,"ok":false,"error":{"code":"not-found","reason":"app-missing"}})
    );
    // macOS starts a named app through Launch Services, which refuses a plain program.
    let output = run(&["--app", "/bin/true"]);
    let expected = if cfg!(target_os = "macos") { Some(4) } else { Some(3) };
    assert_eq!(
        output.status.code(),
        expected,
        "a program that is not Ibis never reports success"
    );
}
