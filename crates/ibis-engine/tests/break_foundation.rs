//! Adversarial regressions against origin/main. Invented data only; intentionally failing.
#![allow(clippy::unwrap_used)]

use ibis_engine::settings::{self, Settings};
use serde_json::json;

#[test]
fn duplicate_settings_consent_is_rejected() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{"schema_version":2,"update_checks":false,"update_checks":true}"#,
    )
    .unwrap();
    let loaded = settings::load(&path);
    assert!(
        loaded.problem.is_some(),
        "ambiguous consent accepted: update_checks={:?}",
        loaded.settings.update_checks
    );
}

#[test]
fn settings_save_cannot_create_a_newer_schema_than_it_can_read() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let path = dir.path().join("settings.json");
    let settings = Settings {
        schema_version: settings::SCHEMA_VERSION + 1,
        ..Settings::default()
    };
    let saved = settings::save(&path, &settings);
    let loaded = settings::load(&path);
    assert!(
        saved.is_err() || loaded.problem.is_none(),
        "save succeeded, next load rejected its own output: {:?}",
        loaded.problem
    );
}

#[test]
fn flattened_unknown_settings_cannot_override_consent() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let path = dir.path().join("settings.json");
    let mut settings = Settings {
        update_checks: Some(false),
        ..Settings::default()
    };
    settings.unknown.insert("update_checks".into(), json!(true));
    let saved = settings::save(&path, &settings);
    let loaded = settings::load(&path);
    assert!(
        saved.is_err() || loaded.settings.update_checks == Some(false),
        "save silently changed consent to {:?}",
        loaded.settings.update_checks
    );
}

#[test]
fn settings_save_does_not_acknowledge_an_unreadable_nan_position() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let path = dir.path().join("settings.json");
    let mut settings = Settings::default();
    settings.picker_windows.insert(
        "invented-display".into(),
        settings::PickerWindow {
            width: 640,
            height: 480,
            custom_size: true,
            position: Some((f64::NAN, 0.5)),
        },
    );
    let saved = settings::save(&path, &settings);
    let loaded = settings::load(&path);
    assert!(
        saved.is_err() || loaded.problem.is_none(),
        "save succeeded, NaN became null and broke reload: {:?}",
        loaded.problem
    );
}

#[cfg(unix)]
#[test]
fn retargeted_settings_symlink_cannot_escape_the_locked_file() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let first = dir.path().join("invented-first.json");
    let second = dir.path().join("invented-second.json");
    let selected = dir.path().join("settings.json");
    let bytes = serde_json::to_vec(&Settings::default()).unwrap();
    std::fs::write(&first, &bytes).unwrap();
    std::fs::write(&second, &bytes).unwrap();
    symlink(&first, &selected).unwrap();
    let other_writer = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(second.with_extension("json.lock"))
        .unwrap();
    other_writer.try_lock().unwrap();
    // The closure is the transaction's existing deterministic interleaving seam.
    let result = settings::update(&selected, |settings| {
        std::fs::remove_file(&selected).unwrap();
        symlink(&second, &selected).unwrap();
        settings.update_checks = Some(false);
    });
    let second_changed = std::fs::read(&second).unwrap() != bytes;
    assert!(
        result.is_err() && !second_changed,
        "transaction accepted retargeted symlink: success={}, unlocked second target overwritten={second_changed}",
        result.is_ok()
    );
}

#[test]
fn two_settings_writers_cannot_silently_overwrite_a_newer_choice() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let path = dir.path().join("settings.json");
    settings::save(&path, &Settings::default()).unwrap();
    let mut stale = settings::load(&path).settings;
    settings::update(&path, |current| current.paste = Some(false)).unwrap();
    stale.update_checks = Some(false);
    let saved = settings::save(&path, &stale);
    let current = settings::load(&path);
    assert!(
        saved.is_err() || current.settings.paste == Some(false),
        "second writer silently erased first writer's paste=false"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn settings_watch_survives_renaming_its_parent_directory() {
    use std::sync::mpsc;
    use std::time::Duration;
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let folder = dir.path().join("invented-settings");
    std::fs::create_dir(&folder).unwrap();
    let path = folder.join("settings.json");
    std::fs::write(&path, r#"{"schema_version":2,"paste":true}"#).unwrap();
    let owner = settings::Owner::open(path.clone());
    let (send, edits) = mpsc::channel();
    let _watch = owner
        .watch(move |_, loaded| {
            let _ = send.send(loaded);
        })
        .expect("environment: native watcher unavailable");
    let replacement = folder.join("invented-replacement");
    std::fs::write(&replacement, r#"{"schema_version":2,"paste":false}"#).unwrap();
    std::fs::rename(&replacement, &path).unwrap();
    assert_eq!(
        edits
            .recv_timeout(Duration::from_secs(2))
            .expect("environment: initial native watch failed")
            .settings
            .paste,
        Some(false)
    );
    std::fs::rename(&folder, dir.path().join("invented-old-settings")).unwrap();
    let missing = edits
        .recv_timeout(Duration::from_secs(2))
        .expect("parent rename notification");
    assert_eq!(missing.primary_read, settings::PrimaryRead::Missing);
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(&path, r#"{"schema_version":2,"paste":true}"#).unwrap();
    let restored = edits.recv_timeout(Duration::from_millis(500));
    assert!(
        restored.is_ok(),
        "watch passed the initial edit, then lost all edits after parent rename: {restored:?}"
    );
}

#[test]
fn nul_cannot_merge_words_in_the_mcp_title() {
    use ibis_engine::{Cancellation, Engine, mcp};
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let path = dir.path().join("invented.json");
    std::fs::write(
        &path,
        r#"[{"id":"one","type":"book","citation-key":"one","title":"Invented\u0000hidden"}]"#,
    )
    .unwrap();
    let engine = Engine::new().unwrap().without_file_watches();
    engine.open_file_observed(&path, &Cancellation::default()).unwrap();
    let reply = mcp::handle(
        &engine,
        &mut mcp::Session::default(),
        &json!({
            "jsonrpc":"2.0", "id":1, "method":"tools/call", "params": {
                "name":"show", "arguments":{"ref":"one"}, "_meta": {
                    "io.modelcontextprotocol/protocolVersion":mcp::PROTOCOL_VERSION,
                    "io.modelcontextprotocol/clientCapabilities":{}
                }
            }
        }),
    )
    .unwrap();
    assert_eq!(
        reply["result"]["structuredContent"]["title"], "Invented hidden",
        "MCP silently changed the title: {reply}"
    );
}
