//! `ibis lookup` through the real executable. Every case here is settled
//! before any request could be made, so none reaches the network; the
//! requests themselves are tested in `ibis-engine`'s lookup module.
#![cfg(test)]

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;

/// A home folder of its own with one invented library and the app's settings.
fn home(name: &str, settings: Option<Value>) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("lookup-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("refs.json"),
        json!([{"id": "held", "type": "book", "title": "Held Lanterns", "author": [{"family": "Vale"}],
                "issued": {"date-parts": [[2024]]}, "DOI": "10.5555/HELD-1"}])
        .to_string(),
    )
    .unwrap();
    if let Some(settings) = settings {
        write_settings(&root, "com.ibiscite.ibis", settings);
    }
    root
}

fn write_settings(root: &Path, app: &str, settings: Value) {
    let config = if cfg!(target_os = "macos") {
        root.join("Library/Application Support")
    } else if cfg!(windows) {
        root.join("AppData")
    } else {
        root.join("config")
    };
    let path = config.join(app).join("settings.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, settings.to_string()).unwrap();
}

fn command(root: &Path, doi: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ibis"));
    command
        .args(["lookup", doi, "--json"])
        .env("HOME", root)
        .env("USERPROFILE", root)
        .env("APPDATA", root.join("AppData"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .stdin(std::process::Stdio::null());
    command
}

fn run(command: &mut Command) -> (i32, Value, String) {
    let output = command.output().unwrap();
    (
        output.status.code().unwrap_or(-1),
        serde_json::from_slice(&output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

fn lookup(root: &Path, doi: &str) -> (i32, Value, String) {
    run(command(root, doi).arg("--file").arg(root.join("refs.json")))
}

#[test]
fn a_held_doi_is_cited_from_the_library_with_lookup_never_set_up() {
    let root = home("held", None);
    let (status, envelope, stderr) = lookup(&root, "https://doi.org/10.5555/held-1");
    assert_eq!(status, 0, "{envelope}{stderr}");
    assert_eq!(envelope["data"]["origin"]["from"], "library");
    assert_eq!(envelope["data"]["citation"]["text"], "(Vale, 2024)");
}

#[test]
fn a_doi_no_library_holds_is_refused_while_lookup_is_off_or_unconfirmed() {
    for (name, settings, reason) in [
        ("unset", json!({"schema_version": 2}), "lookup-off"),
        (
            "off",
            json!({"schema_version": 2, "online_lookup": false}),
            "lookup-off",
        ),
        (
            "unconfirmed",
            json!({"schema_version": 2, "online_lookup": true}),
            "lookup-unconfirmed",
        ),
    ] {
        let root = home(name, Some(settings));
        let (status, envelope, stderr) = lookup(&root, "10.5555/new-1");
        assert_eq!(status, 5, "{name}: {envelope}{stderr}");
        assert_eq!(envelope["error"]["code"], "source-unavailable");
        assert_eq!(envelope["error"]["reason"], reason, "{name}");
        assert_eq!(envelope["error"]["recovery"]["action"], "review-settings");
    }
    let root = home("words", None);
    let (status, envelope, _) = lookup(&root, "lanterns");
    assert_eq!((status, &envelope["error"]["reason"]), (3, &json!("not-a-doi")));
}

#[test]
fn nightly_lookup_checks_nightlys_library_before_its_network_consent() {
    let root = home("nightly", None);
    let stable = root.join("stable.json");
    std::fs::write(&stable, r#"[{"id":"other","type":"book","title":"Invented"}]"#).unwrap();
    write_settings(&root, "com.ibiscite.ibis", json!({"library": {"bibtex": stable}}));
    write_settings(
        &root,
        "com.ibiscite.ibis.nightly",
        json!({"library": {"bibtex": root.join("refs.json")}, "online_lookup": true}),
    );
    let (status, envelope, stderr) = run(command(&root, "10.5555/held-1").arg("--nightly"));
    assert_eq!(status, 0, "{envelope}{stderr}");
    assert_eq!(envelope["data"]["origin"]["from"], "library");
}

#[test]
fn a_first_lookup_can_reach_the_consent_check_without_a_chosen_library() {
    let root = home("unchosen", Some(json!({"online_lookup": true})));
    let (status, envelope, stderr) = run(&mut command(&root, "10.5555/new-1"));
    assert_eq!(status, 5, "{envelope}{stderr}");
    assert_eq!(envelope["error"]["reason"], "lookup-unconfirmed");
}

#[test]
fn duplicate_dois_list_exact_library_matches() {
    let root = home("several", None);
    let records = json!([
        {"id": "first", "type": "book", "citation-key": "10.5555/compound"},
        {"id": "second", "type": "book", "citation-key": "suffix"},
        {"id": "held", "type": "book", "DOI": "10.5555/compound,suffix"},
        {"id": "also-held", "type": "book", "DOI": "10.5555/compound,suffix"}
    ]);
    std::fs::write(root.join("refs.json"), records.to_string()).unwrap();
    let (status, envelope, stderr) = lookup(&root, "10.5555/compound,suffix");
    assert_eq!(status, 0, "{envelope}{stderr}");
    let hits = envelope["data"]["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().all(|hit| hit["matched"] == json!(["doi"])));
}
