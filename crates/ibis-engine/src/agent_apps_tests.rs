use super::*;

std::thread_local! {
    // One shot and local to this test thread, so parallel tests cannot consume it.
    static BEFORE_FINAL_CHECK: std::cell::Cell<Option<Box<dyn FnOnce()>>> = const { std::cell::Cell::new(None) };
}

pub(super) fn before_final_check() {
    if let Some(hook) = BEFORE_FINAL_CHECK.take() {
        hook();
    }
}

#[test]
fn agent_full_path_clients_without_a_command_are_blocked_without_writing() {
    for id in ["claude-desktop", "lm-studio"] {
        for existing in [None, Some(COMMAND), Some("/missing/ibis")] {
            let home = home(&format!("no-command-{id}-{}", existing.is_some()));
            let path = absolute_settings(&home, id, existing.unwrap_or("other"));
            if existing.is_none() {
                fs::remove_file(&path).unwrap();
            }
            let before = fs::read(&path).ok();
            let app = plan(&home, &[id]).remove(0);
            assert_eq!(app.state, AgentAppState::Blocked);
            assert_eq!(app.detail, "Install the ibis command first");
            let change = app.connect(&home, true, false, true);
            assert_eq!(change.outcome, Outcome::Blocked);
            assert_eq!(change.detail, "Install the ibis command first");
            assert_eq!(fs::read(&path).ok(), before);
        }
    }
}

#[test]
fn agent_connected_and_stale_revisions_pin_the_observed_entry() {
    for id in ["cursor", "claude-desktop"] {
        let mut home = home(&format!("pin-entry-{id}"));
        known_command(&mut home);
        let path = absolute_settings(&home, id, "/one/ibis");
        let first = plan(&home, &[id]).remove(0);
        absolute_settings(&home, id, "/two/ibis");
        let changed = fs::read(&path).unwrap();
        let second = plan(&home, &[id]).remove(0);
        assert_ne!(first.revision, second.revision, "{id}");
        assert_eq!(first.disconnect(&home, true, false).outcome, Outcome::Failed);
        assert_eq!(fs::read(&path).unwrap(), changed);
        // JSON key order and whitespace do not change the observed entry.
        fs::write(
            &path,
            r#"{ "mcpServers": { "ibis": { "args": ["mcp", "--app-libraries"], "command": "/two/ibis" } } }"#,
        )
        .unwrap();
        assert_eq!(second.revision, plan(&home, &[id]).remove(0).revision);
    }
}

#[cfg(unix)]
#[test]
fn agent_hung_program_is_killed_and_reaped_at_the_deadline() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};
    let home = home("hung-program");
    let script = home.root.join("bin/fake-agent");
    fs::write(&script, "#!/bin/sh\nexec /bin/sleep 1\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let child = std::process::Command::new(script).spawn().unwrap();
    let pid = child.id().to_string();
    let result = finish_program(child, "fake-agent", &[], Instant::now() + Duration::from_millis(100));
    assert!(result.unwrap_err().contains("did not finish in time"));
    assert!(
        !std::process::Command::new("/bin/kill")
            .args(["-0", &pid])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success(),
        "child still exists or was not reaped"
    );
}

fn home(name: &str) -> Home {
    let root = std::env::temp_dir().join(format!("ibis-agents-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("bin")).unwrap();
    Home {
        config: Some(root.join("config")),
        path: vec![root.join("bin")],
        codex_home: None,
        ibis: None,
        root,
    }
}

fn desktop(home: &Home) -> PathBuf {
    let path = home
        .config
        .clone()
        .unwrap()
        .join("Claude")
        .join("claude_desktop_config.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    path
}

fn known_command(home: &mut Home) -> PathBuf {
    let command = home
        .root
        .join("bin")
        .join(format!("ibis{}", std::env::consts::EXE_SUFFIX));
    fs::write(&command, "invented command").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&command, fs::Permissions::from_mode(0o755)).unwrap();
    }
    home.ibis = Some(command.clone());
    command
}

fn stale_commands(home: &mut Home) -> Vec<String> {
    known_command(home);
    let missing = home.root.join("missing/ibis");
    let directory = home.root.join("directory/ibis");
    fs::create_dir_all(&directory).unwrap();
    #[allow(unused_mut)] // Only Unix has a separate executable permission bit.
    let mut commands = vec![
        COMMAND.to_owned(),
        missing.display().to_string(),
        directory.display().to_string(),
    ];
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let plain = home.root.join("nonexecutable/ibis");
        fs::create_dir_all(plain.parent().unwrap()).unwrap();
        fs::write(&plain, "not executable").unwrap();
        fs::set_permissions(&plain, fs::Permissions::from_mode(0o644)).unwrap();
        commands.push(plain.display().to_string());
    }
    commands
}

fn absolute_settings(home: &Home, id: &str, command: &str) -> PathBuf {
    let client = CLIENTS.iter().find(|client| client.id == id).unwrap();
    let Store::Json { file, .. } = client.store else {
        panic!("JSON client")
    };
    let path = file(home).unwrap();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        json!({"mcpServers":{"ibis":{"command":command,"args":ARGS}}}).to_string(),
    )
    .unwrap();
    path
}

const DESKTOP: &str = "{\n    \"mcpServers\": {\n        \"other\": {\"command\": \"invented\", \"args\": []}\n    },\n    \"theme\": \"dark\"\n}\n";

fn changes(home: &Home, action: &str, chosen: &[&str], replace: bool) -> Vec<(AgentApp, Change)> {
    plan(home, chosen)
        .into_iter()
        .map(|app| {
            let change = match action {
                "setup" => app.connect(home, true, false, replace),
                "remove" => app.disconnect(home, true, false),
                "status" => app.status(),
                _ => panic!("unknown fixture action"),
            };
            (app, change)
        })
        .collect()
}
fn outcome(result: &[(AgentApp, Change)], id: &str) -> String {
    result
        .iter()
        .find(|(app, _)| app.id == id)
        .map(|(_, change)| change.outcome.as_str().to_owned())
        .unwrap_or_default()
}

#[test]
fn setup_merges_the_entry_and_keeps_every_other_setting_in_order() {
    let mut home = home("merge");
    let command = known_command(&mut home);
    let path = desktop(&home);
    fs::write(&path, DESKTOP).unwrap();
    let result = changes(&home, "setup", &[], false);
    assert_eq!(outcome(&result, "claude-desktop"), "connected");
    let written = fs::read_to_string(&path).unwrap();
    assert_eq!(
        written,
        "{\n    \"mcpServers\": {\n        \"other\": {\n            \"command\": \"invented\",\n            \"args\": []\n        },\n        \"ibis\": {\n            \"command\": \"ibis\",\n            \"args\": [\n                \"mcp\",\n                \"--app-libraries\"\n            ]\n        }\n    },\n    \"theme\": \"dark\"\n}\n".replace("\"command\": \"ibis\"", &format!("\"command\": {}", json!(command)))
    );
    assert_eq!(
        fs::read_to_string(path.with_file_name("claude_desktop_config.json.ibis-backup-1")).unwrap(),
        DESKTOP
    );
    // A second run finds it connected and writes nothing.
    let again = changes(&home, "setup", &[], false);
    assert_eq!(outcome(&again, "claude-desktop"), "connected");
    assert_eq!(fs::read_to_string(&path).unwrap(), written);
    // Removal takes out only Ibis's entry.
    let removed = changes(&home, "remove", &[], false);
    assert_eq!(outcome(&removed, "claude-desktop"), "removed");
    let left: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        left,
        json!({"mcpServers": {"other": {"command": "invented", "args": []}}, "theme": "dark"})
    );
}

#[test]
fn backups_keep_each_original_in_a_new_private_file() {
    let mut home = home("backup-history");
    known_command(&mut home);
    let path = desktop(&home);
    let original = json!({"mcpServers":{"ibis":{"command":"ibis","args":["mcp","--file","invented.bib"]}}}).to_string();
    fs::write(&path, &original).unwrap();
    let setup = changes(&home, "setup", &["claude-desktop"], true);
    assert_eq!(outcome(&setup, "claude-desktop"), "replaced");
    let connected = fs::read_to_string(&path).unwrap();
    let remove = changes(&home, "remove", &["claude-desktop"], false);
    assert_eq!(outcome(&remove, "claude-desktop"), "removed");
    for (number, expected) in [(1, original), (2, connected)] {
        let backup = path.with_file_name(format!("claude_desktop_config.json.ibis-backup-{number}"));
        assert_eq!(fs::read_to_string(&backup).unwrap(), expected);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(backup).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }
}

#[cfg(unix)]
fn change_during_staging(link: bool) {
    let home = home(if link { "staging-link" } else { "staging-save" });
    let path = desktop(&home);
    fs::write(&path, "{\"mcpServers\":{}}\n").unwrap();
    let saved = home.root.join("saved.json");
    let changed = "{\"mcpServers\":{},\"theme\":\"new choice\"}\n";
    fs::write(&saved, changed).unwrap();
    BEFORE_FINAL_CHECK.set(Some(Box::new({
        let path = path.clone();
        move || {
            if link {
                fs::remove_file(&path).unwrap();
                std::os::unix::fs::symlink(&saved, &path).unwrap();
            } else {
                fs::rename(&saved, &path).unwrap();
            }
        }
    })));
    let result = edit_json(&path, "mcpServers", Some(entry(false, COMMAND)));
    assert!(result.is_err(), "a concurrent change must abort: {result:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), changed);
    assert_eq!(fs::symlink_metadata(&path).unwrap().is_symlink(), link);
    assert!(
        fs::read_dir(path.parent().unwrap()).unwrap().all(|entry| {
            let name = entry.unwrap().file_name();
            let name = name.to_string_lossy();
            !name.contains(".ibis-backup") && !name.contains(".tmp-")
        }),
        "an aborted edit leaves neither staging nor its backup"
    );
}

#[cfg(unix)]
#[test]
fn an_app_save_during_staging_wins() {
    change_during_staging(false);
}

#[cfg(unix)]
#[test]
fn a_link_swap_during_staging_is_refused() {
    change_during_staging(true);
}

#[test]
fn files_that_are_not_plain_json_or_are_links_are_left_alone() {
    let home = home("blocked");
    let path = home.root.join(".gemini").join("settings.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let commented = "{\n  // invented comment\n  \"mcpServers\": {}\n}\n";
    fs::write(&path, commented).unwrap();
    let result = changes(&home, "setup", &[], false);
    assert_eq!(outcome(&result, "gemini"), "blocked");
    assert!(result[0].1.detail.contains("https://ibiscite.org/setup"));
    assert_eq!(fs::read_to_string(&path).unwrap(), commented);
    #[cfg(unix)]
    {
        let target = home.root.join("dotfiles.json");
        fs::write(&target, "{}").unwrap();
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        let result = changes(&home, "setup", &[], false);
        assert_eq!(outcome(&result, "gemini"), "blocked");
        assert_eq!(fs::read_to_string(&target).unwrap(), "{}");
    }
}

#[test]
fn indentation_follows_the_file_and_strings_are_untouched() {
    assert_eq!(
        indent(r#"{"a":[],"b":{"c":"x, {y}: [z]"}}"#, "\t"),
        "{\n\t\"a\": [],\n\t\"b\": {\n\t\t\"c\": \"x, {y}: [z]\"\n\t}\n}"
    );
    assert_eq!(indent_unit("{\n    \"a\": 1\n}"), "    ");
    assert_eq!(indent_unit("{}"), "  ");
}

/// The plugin, the setup guide and this command give every app the same
/// server, so no one ends up with it twice under different settings.
#[test]
fn the_plugin_the_guide_and_setup_write_one_server_entry() {
    let wanted = json!({"command": COMMAND, "args": ARGS});
    for text in [
        include_str!("../../../plugins/ibis/.mcp.json"),
        include_str!("../../../plugins/ibis/mcp.json"),
    ] {
        let value: Value = serde_json::from_str(text).unwrap();
        let mut server = value["mcpServers"]["ibis"].clone();
        if let Some(server) = server.as_object_mut() {
            server.remove("type");
        }
        assert_eq!(server, wanted, "{text}");
    }
    let guide = include_str!("../../../docs/agents.md");
    assert!(guide.contains(r#"{"mcpServers":{"ibis":{"command":"ibis","args":["mcp","--app-libraries"]}}}"#));
    assert!(guide.contains("claude mcp add --scope user --transport stdio ibis -- ibis mcp --app-libraries"));
    assert!(guide.contains("codex mcp add ibis -- ibis mcp --app-libraries"));
    let Store::Program { add, .. } = CLIENTS[0].store else {
        panic!("Claude Code runs its own command")
    };
    assert_eq!(
        add.join(" "),
        "mcp add --scope user --transport stdio ibis -- ibis mcp --app-libraries"
    );
    assert_eq!(serde_json::from_str::<Value>(&entry(false, COMMAND)).unwrap(), wanted);
}

#[test]
fn apps_opened_from_the_dock_get_the_full_path_and_line_endings_are_kept() {
    let mut home = home("absolute");
    let command = known_command(&mut home);
    let path = desktop(&home);
    fs::write(&path, "{\r\n  \"mcpServers\": { },\r\n  \"theme\": \"dark\"\r\n}\r\n").unwrap();
    let result = changes(&home, "setup", &["claude-desktop"], false);
    assert_eq!(outcome(&result, "claude-desktop"), "connected");
    let written = fs::read_to_string(&path).unwrap();
    assert!(!written.replace("\r\n", "").contains('\n'), "{written:?}");
    let value: Value = serde_json::from_str(&written).unwrap();
    assert_eq!(value["mcpServers"]["ibis"]["command"], command.to_str().unwrap());
    // A usable absolute ibis command is connected and is what removal takes out.
    let status = changes(&home, "status", &["claude-desktop"], false);
    assert_eq!(outcome(&status, "claude-desktop"), "connected");
    changes(&home, "remove", &["claude-desktop"], false);
    let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(value["mcpServers"], json!({}));
}

#[test]
fn removal_takes_out_a_stale_entry_that_setup_owns() {
    for id in ["claude-desktop", "lm-studio"] {
        let mut home = home(&format!("stale-remove-{id}"));
        for command in stale_commands(&mut home) {
            let path = absolute_settings(&home, id, &command);
            let result = changes(&home, "remove", &[id], false);
            assert_eq!(outcome(&result, id), "removed");
            let written: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
            assert_eq!(written["mcpServers"], json!({}));
        }
    }
}

#[test]
fn an_entry_with_settings_of_its_own_is_never_removed() {
    let home = home("own-settings");
    let path = desktop(&home);
    let text = r#"{"mcpServers":{"ibis":{"command":"ibis","args":["mcp","--app-libraries"],"env":{"INVENTED":"1"}}}}"#;
    fs::write(&path, text).unwrap();
    let result = changes(&home, "remove", &["claude-desktop"], false);
    assert_eq!(outcome(&result, "claude-desktop"), "kept");
    assert_eq!(fs::read_to_string(&path).unwrap(), text);
}

#[cfg(unix)]
#[test]
fn backup_names_taken_by_links_or_public_files_are_kept_and_skipped() {
    use std::os::unix::fs::PermissionsExt;
    let mut home = home("backup-link");
    known_command(&mut home);
    let path = desktop(&home);
    fs::write(&path, DESKTOP).unwrap();
    let elsewhere = home.root.join("elsewhere.txt");
    fs::write(&elsewhere, "invented").unwrap();
    let link = path.with_file_name("claude_desktop_config.json.ibis-backup-1");
    std::os::unix::fs::symlink(&elsewhere, &link).unwrap();
    let public = path.with_file_name("claude_desktop_config.json.ibis-backup-2");
    fs::write(&public, "older backup").unwrap();
    fs::set_permissions(&public, fs::Permissions::from_mode(0o644)).unwrap();
    let result = changes(&home, "setup", &["claude-desktop"], false);
    assert_eq!(outcome(&result, "claude-desktop"), "connected");
    assert_eq!(fs::read_to_string(&elsewhere).unwrap(), "invented");
    assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(fs::read_to_string(&public).unwrap(), "older backup");
    assert_eq!(fs::metadata(public).unwrap().permissions().mode() & 0o777, 0o644);
    let backup = path.with_file_name("claude_desktop_config.json.ibis-backup-3");
    assert!(!fs::symlink_metadata(&backup).unwrap().file_type().is_symlink());
    assert_eq!(fs::metadata(&backup).unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(fs::read_to_string(backup).unwrap(), DESKTOP);
}

#[cfg(unix)]
#[test]
fn an_app_whose_settings_cannot_be_read_is_left_alone() {
    use std::os::unix::fs::PermissionsExt;
    let home = home("unreadable-codex");
    let log = home.root.join("codex-args.txt");
    let script = home.root.join("bin").join("codex");
    fs::write(&script, format!("#!/bin/sh\necho \"$@\" >> '{}'\n", log.display())).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    fs::create_dir_all(home.root.join(".codex")).unwrap();
    fs::write(home.root.join(".codex/config.toml"), "[mcp_servers.ibis\ncommand = ").unwrap();
    let result = changes(&home, "setup", &["codex"], false);
    assert_eq!(outcome(&result, "codex"), "blocked");
    assert!(!log.exists(), "codex was never asked to add a server");
    // A linked dotfile is only read, so it works.
    let real = home.root.join("dotfiles-config.toml");
    fs::write(
        &real,
        "[mcp_servers.ibis]\ncommand = \"ibis\"\nargs = [\"mcp\", \"--app-libraries\"]\n",
    )
    .unwrap();
    fs::remove_file(home.root.join(".codex/config.toml")).unwrap();
    std::os::unix::fs::symlink(&real, home.root.join(".codex/config.toml")).unwrap();
    let status = changes(&home, "status", &["codex"], false);
    assert_eq!(outcome(&status, "codex"), "connected");
}

#[test]
fn a_command_on_path_must_be_an_absolute_executable() {
    let mut home = home("path");
    fs::write(home.root.join("bin").join("claude"), "not a program").unwrap();
    home.path.push(PathBuf::from("relative-bin"));
    #[cfg(unix)]
    assert!(!home.on_path("claude"));
    assert!(home.find("missing").is_none());
}

#[test]
fn every_supported_app_is_listed_found_first_with_manual_steps_and_a_replacement_diff() {
    let home = home("list");
    fs::create_dir_all(home.root.join(".cursor")).unwrap();
    let apps = list(&home);
    assert_eq!(apps.len(), 9);
    assert_eq!(apps[0].id, "cursor");
    assert!(apps[0].found);
    assert!(apps[1..].iter().all(|app| !app.found));
    let manual = apps.iter().find(|app| app.id == "chatgpt").unwrap();
    assert_eq!(manual.state, AgentAppState::ByHand);
    assert!(manual.detail.contains("Add server"));
    let path = home.root.join(".cursor/mcp.json");
    fs::write(path, r#"{"mcpServers":{"ibis":{"command":"other"}}}"#).unwrap();
    let app = list(&home).remove(0);
    assert_eq!(app.state, AgentAppState::Different);
    assert!(app.diff.unwrap().contains("- \"ibis\": {\"command\":\"other\"}"));
}

#[test]
fn a_change_after_observation_is_refused() {
    let mut home = home("observation");
    known_command(&mut home);
    let path = desktop(&home);
    fs::write(&path, DESKTOP).unwrap();
    let app = list(&home).into_iter().find(|app| app.id == "claude-desktop").unwrap();
    let saved = r#"{"mcpServers":{"ibis":{"command":"other"}}}"#;
    fs::write(&path, saved).unwrap();
    let change = app.connect(&home, true, false, true);
    assert_eq!(change.outcome.as_str(), "failed");
    assert_eq!(change.detail, CHANGED);
    assert_eq!(fs::read_to_string(path).unwrap(), saved);
}
