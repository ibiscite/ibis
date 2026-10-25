use std::process::Command;
use std::time::{Duration, Instant};

// Keep the deadline outside the loader process so a regression cannot hang CI.
pub fn assert_finishes(test: &str, child_env: &str, timeout_message: &str) {
    assert_finishes_with_timeout(test, child_env, timeout_message, Duration::from_secs(2));
}

pub fn assert_finishes_with_timeout(test: &str, child_env: &str, timeout_message: &str, timeout: Duration) {
    let mut child = Command::new(std::env::current_exe().unwrap_or_else(|e| panic!("{e}")))
        .args(["--exact", test, "--nocapture"])
        .env(child_env, "1")
        .spawn()
        .unwrap_or_else(|e| panic!("{e}"));
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap_or_else(|e| panic!("{e}")) {
            assert!(status.success());
            break;
        }
        if start.elapsed() >= timeout {
            child.kill().unwrap_or_else(|e| panic!("{e}"));
            child.wait().unwrap_or_else(|e| panic!("{e}"));
            panic!("{timeout_message}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
