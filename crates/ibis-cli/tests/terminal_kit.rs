//! Terminal scenarios use invented files and pipes; diagnostics never need a TTY.
use serde_json::{Value, json};
use std::{path::PathBuf, process::Command};

fn directory(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("terminal-kit-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap_or_else(|error| panic!("{error}"));
    path
}

fn doctor(args: &[&str]) -> (i32, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .args(["doctor", "--json"])
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(output.stderr.is_empty(), "{:?}", output.stderr);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(report["ibis"], 1);
    assert_eq!(report["ok"], true);
    (output.status.code().unwrap_or(-1), report["data"].clone())
}

#[test]
fn success_without_a_tty_reports_versions_capabilities_and_freshness_without_contents() {
    let (code, report) = doctor(&["--file", "../../fixtures/bibtex/fidelity.bib"]);
    assert_eq!(code, 0);
    assert_eq!(report["contract_version"], 1);
    assert_eq!(report["cli_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(report["stdout_tty"], false);
    assert_eq!(report["capabilities"]["methods"], json!(ibis_engine::Method::ALL));
    assert!(
        report["capabilities"]["key_outputs"]
            .as_array()
            .is_some_and(|values| !values.is_empty())
    );
    assert!(
        report["capabilities"]["csl_styles"]
            .as_array()
            .is_some_and(|styles| styles.iter().any(|style| style["id"] == "apa"))
    );
    assert_eq!(
        report["capabilities"]["key_outputs"][0]["single_item_forms"],
        json!(["narrative"])
    );
    assert_eq!(report["sources"][0]["status"], "ok");
    assert_eq!(report["sources"][0]["freshness"]["state"], "current");
    assert!(
        report["sources"][0]["freshness"]["updated_at_ms"]
            .as_u64()
            .is_some_and(|time| time > 0)
    );
    let text = report.to_string();
    for private in ["fidelity.bib", "\"csl\"", "accents2021", "Garcia", "fingerprints"] {
        assert!(!text.contains(private), "{text}");
    }
    assert!(!report["sources"].to_string().contains("\"title\""));
}

#[test]
fn no_selected_library_is_a_finding_and_plain_output_explains_the_fix() {
    let (code, report) = doctor(&[]);
    assert_eq!(code, 1);
    assert_eq!(report["library_check"]["code"], "no-library");
    assert_eq!(report["sources"], json!([]));
    let output = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .arg("doctor")
        .output()
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap_or_else(|error| panic!("{error}"));
    assert!(text.contains("no-library"));
    assert!(text.contains("--file"));
    assert!(!text.contains('\u{1b}'));
    assert!(output.stderr.is_empty());
}

#[test]
fn failed_sources_do_not_hide_successes_or_echo_private_paths() {
    let missing = directory("missing").join("synthetic-private-name.bib");
    let (code, report) = doctor(&[
        "--file",
        missing.to_str().unwrap_or_default(),
        "--file",
        "../../fixtures/bibtex/empty.bib",
        "--file",
        "../../fixtures/bibtex/fidelity.bib",
    ]);
    assert_eq!(code, 1);
    assert_eq!(report["sources"][0]["code"], "library-missing");
    assert_eq!(report["sources"][0]["freshness"]["state"], "unknown");
    assert_eq!(report["sources"][1]["status"], "ok");
    assert_eq!(report["sources"][2]["status"], "ok");
    assert!(!report.to_string().contains("synthetic-private-name"));
    assert!(!missing.exists());
}

#[cfg(unix)]
#[test]
fn unreadable_library_is_reported_without_changing_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let path = directory("unreadable").join("library.bib");
    std::fs::write(&path, b"@book{private, title={Synthetic private title}}").unwrap_or_else(|error| panic!("{error}"));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap_or_else(|error| panic!("{error}"));
    let result = doctor(&["--file", path.to_str().unwrap_or_default()]);
    let permissions = std::fs::metadata(&path)
        .unwrap_or_else(|error| panic!("{error}"))
        .permissions()
        .mode();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(result.0, 1);
    assert_eq!(result.1["sources"][0]["code"], "library-unavailable");
    assert_eq!(permissions & 0o777, 0);
    assert!(!result.1.to_string().contains("Synthetic private title"));
}

#[test]
fn broken_config_does_not_hide_sources_or_repair_primary_or_backup() {
    let dir = directory("config");
    let config = dir.join("settings.json");
    let backup = dir.join("settings.json.bak");
    let broken = b"{synthetic private config payload";
    let saved = serde_json::to_vec(&ibis_core::settings::Settings::default()).unwrap_or_default();
    std::fs::write(&config, broken).unwrap_or_else(|error| panic!("{error}"));
    std::fs::write(&backup, &saved).unwrap_or_else(|error| panic!("{error}"));
    let (code, report) = doctor(&[
        "--config",
        config.to_str().unwrap_or_default(),
        "--file",
        "../../fixtures/bibtex/empty.bib",
    ]);
    assert_eq!(code, 1);
    assert_eq!(report["config"]["code"], "config-invalid");
    assert_eq!(report["config"]["from_backup"], true);
    assert_eq!(report["sources"][0]["status"], "ok");
    assert_eq!(std::fs::read(config).unwrap_or_default(), broken);
    assert_eq!(std::fs::read(backup).unwrap_or_default(), saved);
    assert!(!report.to_string().contains("private config"));
    assert_eq!(
        std::fs::read_dir(dir).unwrap_or_else(|error| panic!("{error}")).count(),
        2
    );
}

#[test]
fn zotero_without_a_running_service_is_explicitly_unchecked_and_does_not_read_home() {
    let home = directory("unused-home");
    let output = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("APPDATA", &home)
        .args(["doctor", "--zotero", "--zotero-group", "42", "--json"])
        .output()
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(output.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap_or_default();
    assert_eq!(report["data"]["sources"][0]["code"], "zotero-not-checked");
    assert_eq!(report["data"]["sources"][1]["code"], "zotero-not-checked");
    assert_eq!(report["data"]["sources"][0]["freshness"]["state"], "unknown");
    assert!(output.stderr.is_empty());
    assert_eq!(
        std::fs::read_dir(home)
            .unwrap_or_else(|error| panic!("{error}"))
            .count(),
        0
    );
}

#[test]
fn each_invocation_reads_current_files_and_never_remembers_a_previous_success() {
    let path = directory("reopen").join("library.bib");
    std::fs::write(&path, b"@book{synthetic, title={Invented}}").unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(doctor(&["--file", path.to_str().unwrap_or_default()]).0, 0);
    std::fs::remove_file(&path).unwrap_or_else(|error| panic!("{error}"));
    let (code, report) = doctor(&["--file", path.to_str().unwrap_or_default()]);
    assert_eq!(code, 1);
    assert_eq!(report["sources"][0]["code"], "library-missing");
}

#[test]
fn completion_generation_does_not_open_sources_and_json_wraps_the_script() {
    for shell in ["bash", "zsh", "fish"] {
        let output = Command::new(env!("CARGO_BIN_EXE_ibis"))
            .args(["completions", shell, "--file", "/invented/never-opened.bib"])
            .output()
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(output.status.success(), "{:?}", output.stderr);
        assert!(output.stderr.is_empty());
        let script = String::from_utf8(output.stdout).unwrap_or_default();
        for word in [
            "doctor",
            "--config",
            "completions",
            "--request",
            "--locator",
            "--zotero-group",
            "autocite",
        ] {
            // Fish spells long switches as `-l config`.
            assert!(script.contains(word.trim_start_matches('-')), "{shell}: {word}");
        }
        let output = Command::new(env!("CARGO_BIN_EXE_ibis"))
            .args(["completions", shell, "--json"])
            .output()
            .unwrap_or_else(|error| panic!("{error}"));
        let report: Value = serde_json::from_slice(&output.stdout).unwrap_or_default();
        assert!(output.status.success());
        assert_eq!(report["data"]["shell"], shell);
        assert_eq!(report["data"]["script"].as_str(), Some(script.trim_end_matches('\n')));
    }
}

#[cfg(unix)]
#[test]
fn bash_completes_commands_flags_and_values_in_their_real_contexts() {
    let generated = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .args(["completions", "bash"])
        .output()
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(generated.status.success());
    let script = String::from_utf8(generated.stdout).unwrap_or_default();
    for (words, index, expected, excluded) in [
        ("ibis doc", 1, "doctor", "--locator"),
        ("ibis cite --lo", 2, "--locator", "--config"),
        ("ibis cite synthetic --omit-author --lo", 4, "--locator", "--config"),
        ("ibis --file cite doctor --co", 4, "--config", "--locator"),
        ("ibis cite --format pa", 3, "pandoc", "doctor"),
        ("ibis cite --format = pa", 4, "pandoc", "doctor"),
        ("ibis cite --format=pa", 2, "--format=pandoc", "doctor"),
        ("ibis completions ba", 2, "bash", "doctor"),
    ] {
        let output = Command::new("bash")
            .args([
                "--noprofile",
                "--norc",
                "-c",
                &format!(
                    "{script}\nCOMP_WORDS=({words}); COMP_CWORD={index}; _ibis; printf '%s\\n' \"${{COMPREPLY[@]}}\""
                ),
            ])
            .output()
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(output.status.success(), "{:?}", output.stderr);
        let results = String::from_utf8(output.stdout).unwrap_or_default();
        assert!(results.lines().any(|line| line == expected), "{words}: {results}");
        assert!(!results.lines().any(|line| line == excluded), "{words}: {results}");
    }
}

#[cfg(unix)]
#[test]
fn zsh_completes_flags_and_values_without_running_ibis_again() {
    if Command::new("zsh").args(["-f", "-c", ":"]).output().is_err() {
        eprintln!("zsh is unavailable; the generated command tree is still checked");
        return;
    }
    let generated = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .args(["completions", "zsh"])
        .output()
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(generated.status.success());
    let script = String::from_utf8(generated.stdout).unwrap_or_default();
    for (words, index, expected, excluded) in [
        ("ibis doctor --co", 3, "--config", "--locator"),
        ("ibis --file cite doctor --co", 5, "--config", "--locator"),
        ("ibis cite --format pa", 4, "pandoc", "doctor"),
        ("ibis cite synthetic --omit-author --lo", 5, "--locator", "--config"),
        ("ibis completions ba", 3, "bash", "doctor"),
    ] {
        let output = Command::new("zsh").args(["-f", "-c", &format!(
            "compdef() {{ :; }}\n{script}\ncompadd() {{ printf '%s\\n' \"$@\"; }}\nwords=({words}); CURRENT={index}; _ibis"
        )]).output().unwrap_or_else(|error| panic!("{error}"));
        assert!(output.status.success(), "{:?}", output.stderr);
        let results = String::from_utf8(output.stdout).unwrap_or_default();
        assert!(results.lines().any(|line| line == expected), "{words}: {results}");
        assert!(!results.lines().any(|line| line == excluded), "{words}: {results}");
    }
}

#[test]
fn config_and_file_refusals_have_stable_codes_and_keep_other_checks_visible() {
    let dir = directory("refusals");
    let config = dir.join("missing.json");
    assert_eq!(
        doctor(&["--config", config.to_str().unwrap_or_default()]).1["config"]["code"],
        "config-missing"
    );
    std::fs::write(&config, b"{\"schema_version\":999,\"private\":\"do not echo\"}")
        .unwrap_or_else(|error| panic!("{error}"));
    let (code, report) = doctor(&[
        "--config",
        config.to_str().unwrap_or_default(),
        "--file",
        dir.to_str().unwrap_or_default(),
        "--file",
        "../../fixtures/bibtex/damaged.bib",
    ]);
    assert_eq!(code, 1);
    assert_eq!(report["config"]["code"], "config-newer-format");
    assert_eq!(report["sources"][0]["code"], "library-not-regular");
    assert_eq!(report["sources"][1]["code"], "source-warnings");
    assert_eq!(report["sources"][1]["freshness"]["state"], "current");
    assert!(!report.to_string().contains("do not echo"));
    assert!(!report.to_string().contains("damaged.bib"));
}

#[test]
fn terminal_kit_usage_errors_keep_the_existing_exit_code_and_envelope() {
    for args in [
        vec!["doctor", "--config"],
        vec!["completions"],
        vec!["completions", "unknown-shell"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_ibis"))
            .args(args)
            .arg("--json")
            .output()
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap_or_default(),
            json!({"ibis":1,"ok":false,"error":{"code":"usage"}})
        );
        assert!(output.stderr.is_empty());
    }
}

#[cfg(unix)]
#[test]
fn a_non_utf8_source_path_is_reported_without_a_panic_or_path_disclosure() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let path = directory("non-utf8").join(OsString::from_vec(b"invented-\xff.bib".to_vec()));
    let output = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .args(["doctor", "--json", "--file"])
        .arg(&path)
        .output()
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap_or_default();
    assert_eq!(report["data"]["sources"][0]["code"], "library-invalid");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("invented-"));
}

#[cfg(unix)]
#[test]
fn fzf_recipe_requires_a_nonblank_search_before_reading_a_library() {
    for args in [
        vec!["/invented/unused.bib"],
        vec!["/invented/unused.bib", ""],
        vec!["/invented/unused.bib", " \t\r\n"],
        vec!["/invented/unused.bib", "Invented", "extra"],
        vec![],
    ] {
        let output = Command::new("bash")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/recipes/ibis-fzf"))
            .args(args)
            .output()
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let message = String::from_utf8_lossy(&output.stderr);
        assert!(message.contains("Usage: bash ibis-fzf"), "{message}");
        assert!(message.contains("<search words>"), "{message}");
        assert!(!message.contains("search failed"), "{message}");
    }
}

#[cfg(unix)]
#[test]
fn fzf_recipe_filters_real_titles_and_preserves_the_selected_identity() {
    for tool in ["fzf", "jq"] {
        match Command::new(tool).arg("--version").output() {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("SKIP real fzf recipe regression: {tool} is not installed");
                return;
            }
            Err(error) => panic!("cannot run {tool}: {error}"),
            Ok(output) => assert!(output.status.success(), "{tool} --version failed"),
        }
    }
    let dir = directory("real-fzf");
    let library = dir.join("invented library.bib");
    std::fs::write(
        &library,
        concat!(
            "@book{firstpick, title={Invented comet}}\n",
            "@book{secondpick, title={Invented lantern}}\n",
        ),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let binary_dir = std::path::Path::new(env!("CARGO_BIN_EXE_ibis"))
        .parent()
        .unwrap_or(&dir);
    let command_path = std::env::join_paths(
        std::iter::once(binary_dir.to_path_buf())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    for (filter, code, expected) in [
        ("comet", 0, "[@firstpick]\n"),
        ("lantern", 0, "[@secondpick]\n"),
        ("no-such-title", 1, ""),
    ] {
        // Real fzf's --filter avoids a TTY while retaining the recipe's options.
        // --nth operates on the fields transformed by --with-nth (fzf manual).
        let output = Command::new("bash")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/recipes/ibis-fzf"))
            .arg(&library)
            .arg("Invented")
            .env("PATH", &command_path)
            .env("FZF_DEFAULT_OPTS", format!("--filter={filter}"))
            .env_remove("FZF_DEFAULT_OPTS_FILE")
            .env_remove("FZF_DEFAULT_COMMAND")
            .output()
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(output.status.code(), Some(code), "filter {filter}: {:?}", output.stderr);
        assert_eq!(output.stdout, expected.as_bytes(), "filter {filter}");
        assert!(output.stderr.is_empty(), "filter {filter}: {:?}", output.stderr);
    }
}

#[cfg(unix)]
#[test]
fn fzf_recipe_cites_identities_and_refuses_a_changed_selection_or_failed_source() {
    use std::os::unix::fs::PermissionsExt;
    if Command::new("jq").arg("--version").output().is_err() {
        eprintln!("recipe integration needs jq; generation and Rust scenarios still run");
        return;
    }
    let dir = directory("recipe");
    let fzf = dir.join("fzf");
    let library = dir.join("invented library.bib");
    let rows = dir.join("rows.tsv");
    let binary_dir = std::path::Path::new(env!("CARGO_BIN_EXE_ibis"))
        .parent()
        .unwrap_or(&dir);
    let run = || {
        Command::new("bash")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/recipes/ibis-fzf"))
            .arg(&library)
            .arg("Invented")
            .env(
                "PATH",
                format!("{}:{}:/usr/bin:/bin", dir.display(), binary_dir.display()),
            )
            .env("IBIS_TEST_LIBRARY", &library)
            .env("IBIS_TEST_ROWS", &rows)
            .output()
            .unwrap_or_else(|error| panic!("{error}"))
    };
    std::fs::write(
        &library,
        "@book{synthetic, title={Invented \u{202e}\u{009b}\u{001b}[2J title}}",
    )
    .unwrap_or_else(|error| panic!("{error}"));
    std::fs::write(&fzf, "#!/bin/sh\ncat > \"$IBIS_TEST_ROWS\"\ncat \"$IBIS_TEST_ROWS\"\n")
        .unwrap_or_else(|error| panic!("{error}"));
    std::fs::set_permissions(&fzf, std::fs::Permissions::from_mode(0o700)).unwrap_or_else(|error| panic!("{error}"));
    let output = run();
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(output.stdout, b"[@synthetic]\n");
    let rows = std::fs::read_to_string(&rows).unwrap_or_default();
    let display = rows.split_once('\t').map_or("", |(_, display)| display);
    for control in ['\u{202e}', '\u{009b}', '\u{001b}'] {
        assert!(
            !display.contains(control),
            "fzf display must not contain a terminal control"
        );
    }
    // fzf returns the original selected row, then the library changes before cite.
    std::fs::write(
        &fzf,
        "#!/bin/sh\ncat\nprintf '@book{synthetic, title={Changed title}}\\n' > \"$IBIS_TEST_LIBRARY\"\n",
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let output = run();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("changed"));
    // No TTY / fzf failure must not turn an empty selection into a citation.
    std::fs::write(&library, b"@book{synthetic, title={Invented title}}").unwrap_or_else(|error| panic!("{error}"));
    std::fs::write(&fzf, "#!/bin/sh\ncat >/dev/null\nexit 2\n").unwrap_or_else(|error| panic!("{error}"));
    let output = run();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    std::fs::remove_file(&library).unwrap_or_else(|error| panic!("{error}"));
    let output = run();
    assert_eq!(output.status.code(), Some(5));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("search failed"));
}
