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
fn fresh_cli_errors_never_claim_that_references_were_retained() {
    let invalid = write("cli-neutral-errors", "synthetic.bib", "");
    std::fs::write(&invalid, b"synthetic private text\xff").unwrap_or_else(|e| panic!("{e}"));
    for (path, message) in [
        (invalid.with_file_name("missing.bib"), "the library file is unavailable"),
        (invalid, "the library file is not UTF-8 text; save it as UTF-8"),
    ] {
        for args in [
            vec!["search", "synthetic", "--bib", s(&path), "--json"],
            vec!["cite", "synthetic", "--bib", s(&path), "--json"],
            vec!["check", s(&path), "--json"],
            vec!["csl", "--all", "--bib", s(&path)],
        ] {
            let output = ibis(&args);
            assert_eq!(output.status.code(), Some(1));
            assert!(output.stdout.is_empty());
            assert_eq!(String::from_utf8_lossy(&output.stderr), format!("ibis: {message}\n"));
        }
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

#[test]
fn usage_errors_exit_2_while_help_and_version_exit_0() {
    // Nothing to do, an unknown command, options that exclude each other and
    // a number that does not fit are all usage errors: code 2 and no output
    // on stdout that a script could mistake for a result.
    let file = write("cli-usage", "refs.bib", "@article{k, title={T}}\n");
    for args in [
        vec![],
        vec!["bogus"],
        vec!["csl", "--all", "k", "--bib", s(&file)],
        vec!["search", "k", "--bib", s(&file), "--limit", "99999999999999999999"],
        vec!["cite"],
        vec!["check"],
    ] {
        let out = ibis(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(
            out.stdout.is_empty(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stdout)
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("Usage:") || stderr.contains("try '--help'"),
            "{args:?}: {stderr}"
        );
    }

    let version = ibis(&["--version"]);
    assert_eq!(version.status.code(), Some(0));
    assert!(version.stderr.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim_end(),
        format!("ibis {}", env!("CARGO_PKG_VERSION"))
    );

    let help = ibis(&["--help"]);
    assert_eq!(help.status.code(), Some(0));
    assert!(help.stderr.is_empty());
    let text = String::from_utf8_lossy(&help.stdout);
    for command in ["search", "cite", "check", "csl"] {
        assert!(
            text.lines().any(|line| line.trim_start().starts_with(command)),
            "{command} missing from --help"
        );
    }
}

#[test]
fn empty_library_files_load_cleanly_in_every_command() {
    let empty = write("cli-empty", "empty.bib", "");

    let check = ibis(&["check", s(&empty)]);
    assert_eq!(check.status.code(), Some(0));
    assert!(check.stderr.is_empty());
    assert!(
        String::from_utf8_lossy(&check.stdout).ends_with(": 0 references loaded, 0 problems\n"),
        "{}",
        String::from_utf8_lossy(&check.stdout)
    );

    let json = ibis(&["check", s(&empty), "--json"]);
    assert_eq!(json.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(report[0]["loaded"], 0);
    assert_eq!(report[0]["warnings"], serde_json::json!([]));

    let search = ibis(&["search", "anything", "--bib", s(&empty)]);
    assert_eq!(search.status.code(), Some(0));
    assert!(search.stderr.is_empty());
    assert_eq!(String::from_utf8_lossy(&search.stdout), "No matches.\n");

    let csl = ibis(&["csl", "--all", "--bib", s(&empty)]);
    assert_eq!(csl.status.code(), Some(0));
    assert!(csl.stderr.is_empty());
    assert_eq!(String::from_utf8_lossy(&csl.stdout).trim_end(), "[]");

    // An empty file has no keys, so citing one against it is the usual
    // not-found error, not a crash or a silent success.
    let cite = ibis(&["cite", "k", "--bib", s(&empty)]);
    assert_eq!(cite.status.code(), Some(1));
    assert!(cite.stdout.is_empty());
    assert!(String::from_utf8_lossy(&cite.stderr).contains("not found"));
}

#[test]
fn byte_order_marks_and_windows_line_endings_are_read_as_plain_text() {
    // Files saved by Windows editors start with U+FEFF and end lines with CRLF.
    let file = write(
        "cli-bom-crlf",
        "windows.bib",
        "\u{feff}@article{bom, title={With BOM}, year={2020}}\r\n@article{crlf, title={Second}, year={2021}}\r\n",
    );

    let check = ibis(&["check", s(&file)]);
    assert_eq!(
        check.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&check.stdout)
    );
    assert!(String::from_utf8_lossy(&check.stdout).contains("2 references loaded, 0 problems"));

    let csl = ibis(&["csl", "--all", "--bib", s(&file)]);
    assert_eq!(csl.status.code(), Some(0), "{}", String::from_utf8_lossy(&csl.stderr));
    let items: serde_json::Value = serde_json::from_slice(&csl.stdout).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(items[0]["id"], "bom");
    assert_eq!(items[0]["title"], "With BOM");
    assert_eq!(items[0]["issued"]["date-parts"][0][0], 2020);
    assert_eq!(items[1]["id"], "crlf");
    assert_eq!(items[1]["title"], "Second");
    assert_eq!(items[1]["issued"]["date-parts"][0][0], 2021);
    // Neither the mark nor the carriage return survives into any value.
    let text = String::from_utf8_lossy(&csl.stdout);
    assert!(
        !text.contains('\u{feff}') && !text.contains("\\r") && !text.contains('\r'),
        "{text:?}"
    );

    let search = ibis(&["search", "bom", "--bib", s(&file), "--json"]);
    let value: serde_json::Value = serde_json::from_slice(&search.stdout).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(value["results"][0]["reference"]["citekey"], "bom");
    assert_eq!(value["warnings"], serde_json::json!([]));
}

#[test]
fn page_options_must_name_a_cited_key_with_a_printable_value() {
    for (args, message) in [
        (vec!["cite", "k", "--page", "k"], "--page needs KEY=PAGES, got `k`"),
        (
            vec!["cite", "k", "--page", "other=3"],
            "--page refers to `other`, which is not being cited",
        ),
        (
            vec!["cite", "k", "--page", "k=1\u{7}"],
            "text contains a control character",
        ),
    ] {
        let out = ibis(&args);
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
        assert_eq!(
            String::from_utf8_lossy(&out.stderr),
            format!("ibis: {message}\n"),
            "{args:?}"
        );
    }
}

#[test]
fn page_locators_render_in_every_syntax_from_the_command_line() {
    for (format, expected) in [
        ("pandoc", "[@k, pp. 12-14]"),
        ("autocite", "\\autocite[12--14]{k}"),
        ("parencite", "\\parencite[12--14]{k}"),
        ("textcite", "\\textcite[12--14]{k}"),
        ("cite", "\\cite[pp.~12--14]{k}"),
        ("typst", "@k[pp. 12\u{2013}14]"),
    ] {
        let out = ibis(&["cite", "k", "--format", format, "--page", "k=12-14"]);
        assert_eq!(out.status.code(), Some(0), "{format}");
        assert!(
            out.stderr.is_empty(),
            "{format}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim_end(), expected, "{format}");
    }
    // Bare keys cannot carry a page: the key is printed and the loss is said on stderr.
    let keys = ibis(&["cite", "k", "--format", "keys", "--page", "k=12"]);
    assert_eq!(keys.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&keys.stdout).trim_end(), "k");
    assert!(String::from_utf8_lossy(&keys.stderr).starts_with("note: "));
}

/// An empty page value currently prints `[@k, p. ]` with exit code 0 (#607).
#[test]
#[ignore = "exposes #607: an empty --page value is rendered as a blank locator instead of being refused"]
fn empty_page_values_are_refused() {
    for value in ["k=", "k= "] {
        let out = ibis(&["cite", "k", "--page", value]);
        assert_eq!(out.status.code(), Some(1), "{value:?}");
        assert!(
            out.stdout.is_empty(),
            "{value:?}: {}",
            String::from_utf8_lossy(&out.stdout)
        );
    }
}
