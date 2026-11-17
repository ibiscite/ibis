//! Terminal scenarios use invented files and pipes; diagnostics never need a TTY.
#![cfg(test)]
use serde_json::{Value, json};
use std::{path::PathBuf, process::Command};

fn directory(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("terminal-kit-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap_or_else(|error| panic!("{error}"));
    path
}

/// Runs `ibis` in an invented, empty home, so the machine's own settings and
/// libraries never decide a result (or reach a test's output).
fn ibis() -> Command {
    let home = directory("home");
    let mut command = Command::new(env!("CARGO_BIN_EXE_ibis"));
    for name in [
        "HOME",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "XDG_CONFIG_HOME",
        "XDG_CACHE_HOME",
    ] {
        command.env(name, &home);
    }
    command
}

fn doctor(args: &[&str]) -> (i32, Value) {
    let output = ibis()
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
    assert_eq!(report["cli_version"], ibis_core::VERSION);
    assert_eq!(report["stdout_tty"], false);
    assert_eq!(report["capabilities"]["methods"], json!(ibis_engine::Method::ALL));
    assert!(
        report["capabilities"]["key_outputs"]
            .as_array()
            .is_some_and(|values| !values.is_empty())
    );
    assert!(
        report["capabilities"]["csl_styles"]["count"]
            .as_u64()
            .is_some_and(|count| count > 10_000)
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

#[test]
fn broken_config_does_not_hide_sources_or_repair_primary_or_backup() {
    let dir = directory("config");
    let config = dir.join("settings.json");
    let backup = dir.join("settings.json.bak");
    let broken = b"{synthetic private config payload";
    let saved = serde_json::to_vec(&ibis_engine::settings::Settings::default()).unwrap_or_default();
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
fn doctor_leaves_an_entire_invented_home_and_selected_sources_unchanged() {
    fn contents(root: &std::path::Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                entries.push((path.clone(), Vec::new()));
                entries.extend(contents(&path));
            } else {
                entries.push((path.clone(), std::fs::read(path).unwrap()));
            }
        }
        entries.sort();
        entries
    }
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("settings.json");
    let file = home.path().join("invented.bib");
    std::fs::write(&config, b"{invalid invented settings").unwrap();
    std::fs::write(
        config.with_extension("json.bak"),
        serde_json::to_vec(&ibis_engine::settings::Settings::default()).unwrap(),
    )
    .unwrap();
    std::fs::write(&file, "@misc{invented,title={Invented}}").unwrap();
    std::fs::create_dir_all(home.path().join("Documents")).unwrap();
    std::fs::write(home.path().join("Documents/unselected.bib"), "not reference data").unwrap();
    let before = contents(home.path());
    let output = ibis()
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("APPDATA", home.path())
        .env("XDG_CONFIG_HOME", home.path())
        .args(["doctor", "--json", "--zotero", "--zotero-group", "42", "--config"])
        .arg(config)
        .arg("--file")
        .arg(file)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["data"]["sources"][0]["status"], "ok");
    assert_eq!(report["data"]["sources"][1]["code"], "zotero-not-checked");
    assert_eq!(report["data"]["sources"][2]["code"], "zotero-not-checked");
    assert_eq!(contents(home.path()), before);
    assert!(output.stderr.is_empty());
}

#[test]
fn zotero_without_a_running_service_is_explicitly_unchecked_and_does_not_read_home() {
    let home = directory("unused-home");
    let output = ibis()
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("APPDATA", &home)
        .args(["doctor", "--zotero", "--zotero-group", "42", "--json"])
        .output()
        .unwrap_or_else(|error| panic!("{error}"));
    // A check skipped on purpose is not a finding, as the text says.
    assert_eq!(output.status.code(), Some(0));
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

#[cfg(unix)]
#[test]
fn bash_completes_commands_flags_and_values_in_their_real_contexts() {
    let generated = ibis()
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
        ("ibis help se", 2, "search", "--locator"),
        ("ibis help ci", 2, "cite", "doctor"),
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
fn a_non_utf8_source_path_is_reported_without_a_panic_or_path_disclosure() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let path = directory("non-utf8").join(OsString::from_vec(b"invented-\xff.bib".to_vec()));
    let output = ibis()
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
