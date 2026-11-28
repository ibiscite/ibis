//! The Obsidian plugin against the built command: its own Node tests, with a
//! stand-in for Obsidian's API. Skipped when Node is not installed.
#![cfg(test)]

use std::path::Path;
use std::process::Command;

#[test]
fn obsidian_plugin_searches_and_inserts_a_citation() {
    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("node not found; Obsidian plugin test skipped");
        return;
    }
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/obsidian/main.test.mjs");
    // A home with no Ibis settings, so the app's own format choice cannot leak in.
    let home = Path::new(env!("CARGO_TARGET_TMPDIR")).join("obsidian-home");
    std::fs::create_dir_all(&home).unwrap_or_else(|e| panic!("{e}"));
    let out = Command::new("node")
        .env("IBIS", env!("CARGO_BIN_EXE_ibis"))
        .env("HOME", &home)
        .env("APPDATA", &home)
        .env("XDG_CONFIG_HOME", &home)
        .arg("--test")
        .arg(&tests)
        .output()
        .unwrap_or_else(|e| panic!("cannot run node: {e}"));
    let report = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{report}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(report.contains("# pass 4"), "{report}");
}
