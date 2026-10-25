//! Structured citation requests use the same engine as positional keys.

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn cite(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .arg("cite")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("cannot run ibis: {e}"));
    // Conflicting arguments may be rejected before stdin is consumed.
    let _ = child
        .stdin
        .take()
        .unwrap_or_else(|| panic!("stdin missing"))
        .write_all(input.as_bytes());
    child
        .wait_with_output()
        .unwrap_or_else(|e| panic!("cannot wait for ibis: {e}"))
}

#[test]
fn structured_citations_keep_order_and_distinct_intent_for_repeated_keys() {
    let input = r#"[
        {"key":"example","locator":{"label":"chapter","value":"3"},"prefix":"see","suffix":"emphasis added","suppress_author":true},
        {"key":"example","locator":{"label":"page","value":"7"}}
    ]"#;
    let out = cite(&["--items", "--json"], input);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        value["text"],
        "[see -@example, chap. 3, emphasis added; @example, p. 7]"
    );
    assert_eq!(value["warnings"], serde_json::json!([]));
    assert_eq!(value["load_warnings"], serde_json::json!([]));
}

#[test]
fn every_locator_label_reaches_the_engine() {
    for (label, abbreviation) in [
        ("page", "p."),
        ("chapter", "chap."),
        ("section", "sec."),
        ("paragraph", "para."),
        ("figure", "fig."),
        ("line", "l."),
        ("note", "n."),
        ("volume", "vol."),
        ("verse", "v."),
    ] {
        let input = format!(r#"[{{"key":"example","locator":{{"label":"{label}","value":"3"}}}}]"#);
        let out = cite(&["--items"], &input);
        assert!(
            out.status.success(),
            "{label}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            format!("[@example, {abbreviation} 3]\n")
        );
    }
}

#[test]
fn structured_locators_work_in_all_seven_syntaxes_with_visible_loss_warnings() {
    let input = r#"[{"key":"example","locator":{"label":"chapter","value":"3"}}]"#;
    for (format, expected) in [
        ("pandoc", "[@example, chap. 3]"),
        ("autocite", "\\autocite[chap.~3]{example}"),
        ("parencite", "\\parencite[chap.~3]{example}"),
        ("textcite", "\\textcite[chap.~3]{example}"),
        ("cite", "\\cite[chap.~3]{example}"),
        ("typst", "@example[chap. 3]"),
        ("keys", "example"),
    ] {
        let out = cite(&["--items", "--format", format, "--json"], input);
        assert!(
            out.status.success(),
            "{format}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(value["text"], expected, "{format}");
        assert_eq!(
            value["warnings"]
                .as_array()
                .is_some_and(|warnings| !warnings.is_empty()),
            format == "keys"
        );
    }
}

#[test]
fn invalid_requests_never_produce_partial_output_or_echo_request_text() {
    for input in [
        "",
        "[]",
        "{}",
        "[] []",
        r#"[{"key":"a"},{"key":""}]"#,
        r#"[{"key":"a","prefixx":"private synthetic text"}]"#,
        r#"[{"key":"a","suffix":"private synthetic text\u0007"}]"#,
        r#"[{"key":"a","locator":{"label":"page","value":" "}}]"#,
    ] {
        let out = cite(&["--items", "--json"], input);
        assert_eq!(out.status.code(), Some(1), "{input}");
        assert!(out.stdout.is_empty(), "{input}");
        assert!(!String::from_utf8_lossy(&out.stderr).contains("private synthetic text"));
    }
}

#[test]
fn structured_input_conflicts_with_keys_and_page_flags_before_reading_stdin() {
    for args in [vec!["--items", "example"], vec!["--items", "--page", "example=3"]] {
        let out = cite(&args, "[]");
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        assert!(String::from_utf8_lossy(&out.stderr).contains("cannot be used with"));
    }
}

#[test]
fn structured_input_still_checks_library_membership() {
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("structured-citation-synthetic.bib");
    std::fs::write(&path, "@book{example,title={Synthetic reference}}\n").unwrap_or_else(|e| panic!("{e}"));
    let bib = path.to_str().unwrap_or_else(|| panic!("non-UTF-8 test path"));
    for (key, success) in [("example", true), ("missing", false)] {
        let out = cite(&["--items", "--bib", bib, "--json"], &format!(r#"[{{"key":"{key}"}}]"#));
        assert_eq!(out.status.success(), success);
        if !success {
            assert!(out.stdout.is_empty());
        }
    }
}
