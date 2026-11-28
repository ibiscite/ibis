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
    home.ibis = Some(IbisCommand::Installed(command.clone()));
    command
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
fn an_entry_with_settings_of_its_own_is_never_removed() {
    let home = home("own-settings");
    let path = desktop(&home);
    let text = r#"{"mcpServers":{"ibis":{"command":"ibis","args":["mcp","--app-libraries"],"env":{"INVENTED":"1"}}}}"#;
    fs::write(&path, text).unwrap();
    let result = changes(&home, "remove", &["claude-desktop"], false);
    assert_eq!(outcome(&result, "claude-desktop"), "kept");
    assert_eq!(fs::read_to_string(&path).unwrap(), text);
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

/// The Store copy names its own alias in every connection, since PATH may not
/// hold it or may hold another package's `ibis`; a bare `ibis` it finds is stale.
#[test]
fn the_store_copy_names_its_own_alias_in_every_connection() {
    let mut home = home("packaged");
    let alias = home.root.join("WindowsApps").join("ibis.exe");
    home.ibis = Some(IbisCommand::Packaged(alias.clone()));
    let alias_text = alias.to_string_lossy().into_owned();
    for id in ["vscode", "cursor", "claude-desktop"] {
        let client = CLIENTS.iter().find(|client| client.id == id).unwrap();
        let Store::Json { file, key, typed, .. } = client.store else {
            panic!("JSON client")
        };
        let path = file(&home).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, format!("{{\"{key}\":{{\"ibis\":{}}}}}", entry(typed, COMMAND))).unwrap();
        let app = plan(&home, &[id]).remove(0);
        assert_eq!(app.state, AgentAppState::Stale, "{id}");
        assert_eq!(
            app.connect(&home, true, false, false).outcome,
            Outcome::Replaced,
            "{id}"
        );
        let written: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written[key]["ibis"]["command"], alias_text.as_str(), "{id}");
        assert_eq!(plan(&home, &[id]).remove(0).state, AgentAppState::Connected, "{id}");
    }
    let Store::Program { add, .. } = CLIENTS.iter().find(|client| client.id == "codex").unwrap().store else {
        panic!("program client")
    };
    assert_eq!(
        add_args(add, &alias_text),
        [
            "mcp",
            "add",
            "ibis",
            "--",
            alias_text.as_str(),
            "mcp",
            "--app-libraries"
        ]
    );
    home.ibis = Some(IbisCommand::Installed(alias.clone()));
    assert_eq!(
        command(CLIENTS.iter().find(|client| client.id == "vscode").unwrap(), &home),
        COMMAND
    );
}
