//! Explicit desktop utility tests launch only a disposable fake binary.
#[cfg(unix)]
#[test]
fn toggle_forwards_one_argument_and_keeps_child_output_out_of_json() {
    use std::{
        os::unix::fs::PermissionsExt,
        path::Path,
        process::Command,
        time::{Duration, Instant},
    };
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-toggle/Ibis fake app");
    std::fs::create_dir_all(&directory).unwrap_or_else(|error| panic!("{error}"));
    let desktop = directory.join("ibis-desktop");
    let cli = directory.join("ibis");
    std::fs::write(
        &desktop,
        // Write then rename, so the test never reads a marker the shell has created but not yet filled.
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$0.part\"\nmv \"$0.part\" \"$0.args\"\nprintf 'child output must be private\\n'\n",
    )
    .unwrap_or_else(|error| panic!("{error}"));
    std::fs::copy(env!("CARGO_BIN_EXE_ibis"), &cli).unwrap_or_else(|error| panic!("{error}"));
    std::fs::set_permissions(&desktop, std::fs::Permissions::from_mode(0o700))
        .unwrap_or_else(|error| panic!("{error}"));
    let run = || {
        for _ in 0..100 {
            match Command::new(&cli).args(["toggle", "--json"]).output() {
                Err(error) if error.raw_os_error() == Some(26) => std::thread::sleep(Duration::from_millis(20)),
                result => return result,
            }
        }
        Command::new(&cli).args(["toggle", "--json"]).output()
    };
    let marker = desktop.with_file_name("ibis-desktop.args");
    if marker.exists() {
        std::fs::remove_file(&marker).unwrap_or_else(|error| panic!("{error}"));
    }
    let output = run().unwrap_or_else(|error| panic!("{error}"));
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(value, serde_json::json!({"ibis":1,"ok":true,"data":{"toggled":true}}));
    assert!(output.stderr.is_empty());
    // The fake app creates its marker before writing it, so wait for the whole line.
    let began = Instant::now();
    while !std::fs::read_to_string(&marker).is_ok_and(|text| text.ends_with('\n')) {
        assert!(began.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        std::fs::read_to_string(&marker).unwrap_or_else(|error| panic!("{error}")),
        "toggle\n"
    );
    std::fs::set_permissions(&desktop, std::fs::Permissions::from_mode(0o600))
        .unwrap_or_else(|error| panic!("{error}"));
    let output = run().unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(output.status.code(), Some(5));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap_or_else(|error| panic!("{error}")),
        serde_json::json!({"ibis":1,"ok":false,"error":{"code":"source-unavailable"}})
    );
}
