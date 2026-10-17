//! End-to-end tests of the `ibis` binary.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn ibis(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ibis"))
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("cannot run ibis: {e}"))
}

fn write(dir: &str, name: &str, text: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(dir);
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap_or_else(|e| panic!("{e}"));
    path
}

fn s(path: &Path) -> &str {
    path.to_str().unwrap_or_else(|| panic!("non-UTF-8 test path"))
}

#[test]
fn same_file_name_in_two_folders_keeps_records_distinct_and_refuses_to_guess() {
    let a = write("cli-a", "refs.bib", "@article{same, title={From A}, year={2020}}\n");
    let b = write("cli-b", "refs.bib", "@article{same, title={From B}, year={2021}}\n");

    let out = ibis(&["search", "same", "--bib", s(&a), "--bib", s(&b), "--json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}"));
    let sources: Vec<&str> = json["results"]
        .as_array()
        .map(|r| {
            r.iter()
                .filter_map(|h| h["reference"]["id"]["source"].as_str())
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(sources.len(), 2);
    assert_ne!(sources[0], sources[1]);

    for args in [
        vec!["csl", "same", "--bib", s(&a), "--bib", s(&b)],
        vec!["cite", "same", "--bib", s(&a), "--bib", s(&b)],
    ] {
        let out = ibis(&args);
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("appears in several files"));
    }
    assert!(ibis(&["csl", "same", "--bib", s(&a)]).status.success());
}

#[test]
fn loading_problems_are_visible_in_every_command() {
    let damaged = write(
        "cli-damaged",
        "damaged.bib",
        "@article{ok, title={Fine}}\n@article{ok, title={Duplicate}}\n",
    );

    let search = ibis(&["search", "fine", "--bib", s(&damaged)]);
    assert!(search.status.success());
    assert!(String::from_utf8_lossy(&search.stderr).contains("1 problem"));

    let json = ibis(&["search", "fine", "--bib", s(&damaged), "--json"]);
    let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(value["warnings"].as_array().map(Vec::len), Some(1));

    let csl = ibis(&["csl", "ok", "--bib", s(&damaged)]);
    assert!(String::from_utf8_lossy(&csl.stderr).contains("1 problem"));

    let check = ibis(&["check", s(&damaged)]);
    assert_eq!(check.status.code(), Some(2));
}

#[test]
fn unknown_keys_are_errors() {
    let file = write("cli-unknown", "refs.bib", "@article{known, title={T}}\n");
    let out = ibis(&["cite", "missing", "--bib", s(&file)]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("not found"));
}
