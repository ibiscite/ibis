//! Incomplete entries must not make the CLI rescan the remaining file repeatedly.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn many_incomplete_entries_finish_before_the_deadline() {
    let text = "@article{broken,\n".repeat(40_000) + "@article{kept,title={Retained}}\n";
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-linear-recovery");
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
    let file = dir.join("synthetic.bib");
    std::fs::write(&file, text).unwrap_or_else(|e| panic!("{e}"));
    let mut child = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .arg("check")
        .arg(file)
        // Thousands of warnings must not fill a pipe while we wait for exit.
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| panic!("{e}"));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap_or_else(|e| panic!("{e}")) {
            assert_eq!(status.code(), Some(2), "damaged entries must be reported");
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap_or_else(|e| panic!("{e}"));
            child.wait().unwrap_or_else(|e| panic!("{e}"));
            panic!("ibis check did not finish within 10 seconds");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
