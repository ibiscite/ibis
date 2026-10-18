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
fn every_file_command_refuses_oversized_libraries_with_the_core_diagnostic() {
    let path = write("cli-oversized", "synthetic.bib", "");
    std::fs::File::options()
        .write(true)
        .open(&path)
        .and_then(|file| file.set_len(ibis_core::source::MAX_LIBRARY_BYTES + 1))
        .unwrap_or_else(|e| panic!("{e}"));
    for args in [
        vec!["search", "synthetic", "--bib", s(&path), "--json"],
        vec!["cite", "synthetic", "--bib", s(&path), "--json"],
        vec!["check", s(&path), "--json"],
        vec!["csl", "--all", "--bib", s(&path)],
    ] {
        let output = ibis(&args);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            "ibis: the library file is larger than 64 MiB\n"
        );
    }
}

#[test]
fn file_errors_use_allowlisted_core_messages() {
    use ibis_core::source::SourceError;
    let invalid = write("cli-source-errors", "synthetic.bib", "synthetic private text");
    std::fs::write(&invalid, b"synthetic private text\xff").unwrap_or_else(|e| panic!("{e}"));
    for (path, error) in [
        (invalid.with_file_name("missing.bib"), SourceError::Unavailable),
        (
            invalid.parent().unwrap_or_else(|| panic!("test parent")).to_path_buf(),
            SourceError::NotRegular,
        ),
        (invalid, SourceError::Unreadable),
    ] {
        let output = ibis(&["check", s(&path)]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(String::from_utf8_lossy(&output.stderr), format!("ibis: {error}\n"));
    }
}

#[test]
fn fresh_cli_loads_still_keep_completed_entries_from_damaged_libraries() {
    let path = write(
        "cli-initial-load",
        "synthetic.bib",
        "@misc{good,title={Retained}}\n@misc{unfinished,",
    );
    let output = ibis(&["check", s(&path), "--json"]);
    assert_eq!(output.status.code(), Some(2));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(value[0]["loaded"], 1);
    assert_eq!(value[0]["warnings"].as_array().map(Vec::len), Some(1));
}

#[test]
fn json_options_explain_their_output_in_help() {
    for command in ["search", "cite", "check"] {
        let output = ibis(&[command, "--help"]);
        assert!(output.status.success());
        let help = String::from_utf8_lossy(&output.stdout);
        let option = help
            .lines()
            .find(|line| line.trim_start().starts_with("--json"))
            .unwrap_or_else(|| panic!("missing JSON option for {command}"));
        assert!(option.contains("JSON") && option.contains("warnings"), "{option}");
    }
}

#[test]
fn nested_commands_keep_references_and_report_the_omitted_field() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/bibtex/deep-commands.bib");
    let out = ibis(&["check", s(&path), "--json"]);
    assert_eq!(out.status.code(), Some(2), "{}", String::from_utf8_lossy(&out.stderr));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(report[0]["loaded"], 2);
    assert_eq!(report[0]["warnings"].as_array().map(Vec::len), Some(1));
    assert_eq!(report[0]["warnings"][0]["kind"], "field");
    assert_eq!(report[0]["warnings"][0]["key"], "deep");
    assert!(
        report[0]["warnings"][0]["message"]
            .as_str()
            .is_some_and(|message| message.contains("title") && message.contains("omitted"))
    );
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

/// Cyclic cross-references once overflowed the stack and aborted the process.
/// A subprocess proves the binary now reports them and exits normally.
#[test]
fn cross_reference_cycles_never_abort_the_process() {
    for (name, text) in [
        ("self.bib", "@book{a,title={A},crossref={a}}\n"),
        (
            "mutual.bib",
            "@book{a,title={A},crossref={b}}\n@book{b,title={B},crossref={a}}\n",
        ),
        (
            "xdata.bib",
            "@book{a,title={A},xdata={b}}\n@book{b,title={B},xdata={a}}\n",
        ),
    ] {
        let file = write("cli-cycles", name, text);
        let out = ibis(&["check", s(&file)]);
        assert_eq!(
            out.status.code(),
            Some(2),
            "{name}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(String::from_utf8_lossy(&out.stdout).contains("cycle"), "{name}");
    }
}

#[test]
fn exporting_everything_refuses_duplicate_citation_keys() {
    let a = write(
        "cli-all-a",
        "refs.bib",
        "@article{same, title={From A}}\n@article{onlya, title={Only A}}\n",
    );
    let b = write("cli-all-b", "refs.bib", "@article{same, title={From B}}\n");
    let out = ibis(&["csl", "--all", "--bib", s(&a), "--bib", s(&b)]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("same"));
    assert!(ibis(&["csl", "--all", "--bib", s(&a)]).status.success());
}

/// Doubling an empty abbreviation produces no output but exponential work.
/// The expansion step budget must stop it; a subprocess with a deadline
/// proves the binary finishes.
#[test]
fn empty_doubling_abbreviations_finish_quickly() {
    let mut text = String::from("@string{m0=\"\"}\n");
    for i in 1..=40 {
        text.push_str(&format!("@string{{m{i}=m{} # m{}}}\n", i - 1, i - 1));
    }
    text.push_str("@article{a,title={Synthetic},journal=m40}\n");
    let file = write("cli-empty-macros", "empty-macros.bib", &text);
    let mut child = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .args(["check", s(&file)])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("{e}"));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while child.try_wait().ok().flatten().is_none() {
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            panic!("ibis check did not finish within 10 seconds");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let out = child.wait_with_output().unwrap_or_else(|e| panic!("{e}"));
    assert!(String::from_utf8_lossy(&out.stdout).contains("too complex"));
}

/// Links kept from different paths must not combine into a cycle; this file
/// once overflowed the stack.
#[test]
fn links_from_different_paths_never_form_a_cycle() {
    let file = write(
        "cli-graph-cycle",
        "graph-cycle.bib",
        "@book{root,title={Root},xdata={a,b}}\n@xdata{a,publisher={Synthetic},xdata={b}}\n@xdata{b,year={2024},xdata={a}}\n",
    );
    let out = ibis(&["csl", "--all", "--bib", s(&file)]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(json[0]["id"], "root");
    assert_eq!(json[0]["publisher"], "Synthetic");
    assert_eq!(json[0]["issued"]["date-parts"][0][0], 2024);
}

#[test]
fn library_text_cannot_inject_terminal_control_sequences() {
    let file = write(
        "cli-terminal",
        "evil.bib",
        "@article{esc, title={Evil \u{1b}[31mred\u{1b}[0m and \u{202e}reversed}, year={2020}}\n",
    );
    let out = ibis(&["search", "evil", "--bib", s(&file)]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains('\u{1b}') && !stdout.contains('\u{202e}'), "{stdout:?}");
    assert!(stdout.contains("\\u{1b}"), "{stdout:?}");
}

#[test]
fn cite_json_includes_loading_problems() {
    let file = write(
        "cli-cite-json",
        "damaged.bib",
        "@article{ok, title={Fine}}\n@article{ok, title={Again}}\n",
    );
    let out = ibis(&["cite", "ok", "--bib", s(&file), "--json"]);
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(value["text"], "[@ok]");
    assert_eq!(value["load_warnings"].as_array().map(Vec::len), Some(1));
}

#[test]
fn bare_keys_say_when_page_numbers_are_dropped() {
    let out = ibis(&["cite", "k", "--format", "keys", "--page", "k=12", "--json"]);
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(value["text"], "k");
    assert_eq!(value["warnings"].as_array().map(Vec::len), Some(1));
}

#[test]
fn cite_prints_keys_exactly_as_given() {
    let out = ibis(&["cite", "a\u{200f}b", "--format", "keys"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim_end(), "a\u{200f}b");
}
