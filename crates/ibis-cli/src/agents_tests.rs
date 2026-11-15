use super::*;
use std::io::Cursor;

/// An invented home with no real app settings in it.
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

fn run(home: &Home, args: &[&str], answers: &str, interactive: bool) -> (Result<DispatchResult, Failure>, String) {
    let matches = command()
        .try_get_matches_from(std::iter::once("agents").chain(args.iter().copied()))
        .unwrap();
    let mut errors = vec![];
    let result = run_in(
        home,
        &matches,
        &mut Cursor::new(answers.as_bytes()),
        &mut errors,
        interactive,
    );
    (result, String::from_utf8(errors).unwrap())
}

fn outcome(result: &DispatchResult, id: &str) -> String {
    result.data["apps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|app| app["id"] == id)
        .map(|app| app["outcome"].as_str().unwrap().to_owned())
        .unwrap_or_default()
}

const DESKTOP: &str = "{\n    \"mcpServers\": {\n        \"other\": {\"command\": \"invented\", \"args\": []}\n    },\n    \"theme\": \"dark\"\n}\n";

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

#[test]
fn setup_merges_the_entry_and_keeps_every_other_setting_in_order() {
    let home = home("merge");
    let path = desktop(&home);
    fs::write(&path, DESKTOP).unwrap();
    let (result, _) = run(&home, &["setup", "--yes"], "", false);
    let result = result.ok().unwrap();
    assert_eq!(outcome(&result, "claude-desktop"), "connected");
    let written = fs::read_to_string(&path).unwrap();
    assert_eq!(
        written,
        "{\n    \"mcpServers\": {\n        \"other\": {\n            \"command\": \"invented\",\n            \"args\": []\n        },\n        \"ibis\": {\n            \"command\": \"ibis\",\n            \"args\": [\n                \"mcp\",\n                \"--app-libraries\"\n            ]\n        }\n    },\n    \"theme\": \"dark\"\n}\n"
    );
    assert_eq!(
        fs::read_to_string(path.with_file_name("claude_desktop_config.json.ibis-backup-1")).unwrap(),
        DESKTOP
    );
    // A second run finds it connected and writes nothing.
    let (again, _) = run(&home, &["setup", "--yes"], "", false);
    assert_eq!(outcome(&again.ok().unwrap(), "claude-desktop"), "connected");
    assert_eq!(fs::read_to_string(&path).unwrap(), written);
    // Removal takes out only Ibis's entry.
    let (removed, _) = run(&home, &["remove", "--yes"], "", false);
    assert_eq!(outcome(&removed.ok().unwrap(), "claude-desktop"), "removed");
    let left: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        left,
        json!({"mcpServers": {"other": {"command": "invented", "args": []}}, "theme": "dark"})
    );
}

#[test]
fn backups_keep_each_original_in_a_new_private_file() {
    let home = home("backup-history");
    let path = desktop(&home);
    let original = json!({"mcpServers":{"ibis":{"command":"ibis","args":["mcp","--file","invented.bib"]}}}).to_string();
    fs::write(&path, &original).unwrap();
    let (setup, _) = run(
        &home,
        &["setup", "--yes", "--replace", "--client", "claude-desktop"],
        "",
        false,
    );
    assert_eq!(outcome(&setup.ok().unwrap(), "claude-desktop"), "replaced");
    let connected = fs::read_to_string(&path).unwrap();
    let (remove, _) = run(&home, &["remove", "--yes", "--client", "claude-desktop"], "", false);
    assert_eq!(outcome(&remove.ok().unwrap(), "claude-desktop"), "removed");
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
    // Give the other writer time to observe staging, without a production hook
    // or file watcher. The replacement is ready before either writer starts.
    let original = json!({"mcpServers":{},"padding":"x".repeat(8 * 1024 * 1024)}).to_string();
    fs::write(&path, original).unwrap();
    let saved = home.root.join("saved.json");
    let changed = "{\"mcpServers\":{},\"theme\":\"new choice\"}\n";
    fs::write(&saved, changed).unwrap();
    let temporary = path.with_file_name("claude_desktop_config.json.tmp-1");
    let result = std::thread::scope(|scope| {
        let app = scope.spawn(|| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
            while !temporary.exists() {
                assert!(std::time::Instant::now() < deadline, "staging was never observed");
                std::thread::yield_now();
            }
            if link {
                fs::remove_file(&path).unwrap();
                std::os::unix::fs::symlink(&saved, &path).unwrap();
            } else {
                fs::rename(&saved, &path).unwrap();
            }
        });
        let result = edit_json(&path, "mcpServers", Some(entry(false, COMMAND)));
        app.join().unwrap();
        result
    });
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
fn a_different_ibis_entry_is_shown_and_replaced_only_on_a_yes() {
    let home = home("different");
    let path = home.root.join(".cursor").join("mcp.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let own = r#"{"mcpServers":{"ibis":{"command":"ibis","args":["mcp","--file","invented.bib"]}}}"#;
    fs::write(&path, own).unwrap();
    let (result, _) = run(&home, &["setup", "--yes"], "", false);
    let result = result.ok().unwrap();
    assert_eq!(outcome(&result, "cursor"), "kept");
    assert!(result.text.unwrap().contains("--file"));
    assert_eq!(result.exit_code, ExitCode::Findings);
    assert_eq!(fs::read_to_string(&path).unwrap(), own);
    // Asked in a terminal, a no keeps it and a yes replaces it.
    let (_, asked) = run(&home, &["setup"], "y\nn\n", true);
    assert!(asked.contains("has a different ibis entry"), "{asked}");
    assert_eq!(fs::read_to_string(&path).unwrap(), own);
    let (result, _) = run(&home, &["setup"], "y\ny\n", true);
    assert_eq!(outcome(&result.ok().unwrap(), "cursor"), "replaced");
    let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        value["mcpServers"]["ibis"],
        json!({"command": "ibis", "args": ["mcp", "--app-libraries"]})
    );
    // Removal leaves an entry setup did not write.
    fs::write(&path, own).unwrap();
    let (removed, _) = run(&home, &["remove", "--yes"], "", false);
    assert_eq!(outcome(&removed.ok().unwrap(), "cursor"), "kept");
    assert_eq!(fs::read_to_string(&path).unwrap(), own);
}

#[test]
fn without_a_person_to_ask_nothing_changes_unless_told_to() {
    let home = home("unattended");
    let path = desktop(&home);
    fs::write(&path, DESKTOP).unwrap();
    let (result, _) = run(&home, &["setup"], "", false);
    assert!(result.is_err());
    let (dry, _) = run(&home, &["setup", "--dry-run"], "", false);
    assert_eq!(outcome(&dry.ok().unwrap(), "claude-desktop"), "would-connect");
    let (declined, _) = run(&home, &["setup"], "n\n", true);
    assert_eq!(outcome(&declined.ok().unwrap(), "claude-desktop"), "unchanged");
    assert_eq!(fs::read_to_string(&path).unwrap(), DESKTOP);
}

#[test]
fn files_that_are_not_plain_json_or_are_links_are_left_alone() {
    let home = home("blocked");
    let path = home.root.join(".gemini").join("settings.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let commented = "{\n  // invented comment\n  \"mcpServers\": {}\n}\n";
    fs::write(&path, commented).unwrap();
    let (result, _) = run(&home, &["setup", "--yes"], "", false);
    let result = result.ok().unwrap();
    assert_eq!(outcome(&result, "gemini"), "blocked");
    assert!(result.text.unwrap().contains("https://ibiscite.org/setup"));
    assert_eq!(fs::read_to_string(&path).unwrap(), commented);
    #[cfg(unix)]
    {
        let target = home.root.join("dotfiles.json");
        fs::write(&target, "{}").unwrap();
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        let (result, _) = run(&home, &["setup", "--yes"], "", false);
        assert_eq!(outcome(&result.ok().unwrap(), "gemini"), "blocked");
        assert_eq!(fs::read_to_string(&target).unwrap(), "{}");
    }
}

#[test]
fn only_apps_found_here_are_set_up_and_a_missing_file_is_created() {
    let home = home("found");
    assert!(
        run(&home, &["status"], "", false).0.ok().unwrap().data["apps"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    fs::create_dir_all(home.root.join(".lmstudio")).unwrap();
    let (result, _) = run(&home, &["setup", "--yes"], "", false);
    let result = result.ok().unwrap();
    assert_eq!(result.data["apps"].as_array().unwrap().len(), 1);
    assert_eq!(outcome(&result, "lm-studio"), "connected");
    let value: Value =
        serde_json::from_str(&fs::read_to_string(home.root.join(".lmstudio/mcp.json")).unwrap()).unwrap();
    assert_eq!(
        value,
        json!({"mcpServers": {"ibis": {"command": "ibis", "args": ["mcp", "--app-libraries"]}}})
    );
    // VS Code's entries name their transport.
    fs::create_dir_all(home.config.clone().unwrap().join("Code").join("User")).unwrap();
    run(&home, &["setup", "--yes", "--client", "vscode"], "", false)
        .0
        .ok()
        .unwrap();
    let value: Value =
        serde_json::from_str(&fs::read_to_string(home.config.clone().unwrap().join("Code/User/mcp.json")).unwrap())
            .unwrap();
    assert_eq!(value["servers"]["ibis"]["type"], "stdio");
}

#[cfg(unix)]
#[test]
fn claude_code_is_changed_through_its_own_command_and_never_twice_with_the_plugin() {
    use std::os::unix::fs::PermissionsExt;
    let home = home("claude");
    let log = home.root.join("claude-args.txt");
    let script = home.root.join("bin").join("claude");
    fs::write(&script, format!("#!/bin/sh\necho \"$@\" >> '{}'\n", log.display())).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let (result, _) = run(&home, &["setup", "--yes"], "", false);
    assert_eq!(outcome(&result.ok().unwrap(), "claude-code"), "connected");
    assert_eq!(
        fs::read_to_string(&log).unwrap(),
        "mcp add --scope user --transport stdio ibis -- ibis mcp --app-libraries\n"
    );
    fs::write(
        home.root.join(".claude.json"),
        r#"{"mcpServers":{"ibis":{"type":"stdio","command":"ibis","args":["mcp","--app-libraries"],"env":{}}}}"#,
    )
    .unwrap();
    let (status, _) = run(&home, &["status"], "", false);
    assert_eq!(outcome(&status.ok().unwrap(), "claude-code"), "connected");
    run(&home, &["remove", "--yes"], "", false).0.ok().unwrap();
    assert!(
        fs::read_to_string(&log)
            .unwrap()
            .ends_with("mcp remove --scope user ibis\n")
    );
    // With the plugin installed, setup adds nothing: the server would be there twice.
    fs::remove_file(&log).unwrap();
    fs::remove_file(home.root.join(".claude.json")).unwrap();
    let plugins = home.root.join(".claude/plugins");
    fs::create_dir_all(&plugins).unwrap();
    fs::write(
        plugins.join("installed_plugins.json"),
        r#"{"plugins":{"ibis@ibiscite":[{}]}}"#,
    )
    .unwrap();
    let (result, _) = run(&home, &["setup", "--yes"], "", false);
    let result = result.ok().unwrap();
    assert_eq!(outcome(&result, "claude-code"), "connected");
    assert!(result.text.unwrap().contains("plugin"));
    assert!(!log.exists());
}

#[cfg(unix)]
#[test]
fn a_failed_program_add_prints_the_removed_entry_in_full() {
    use std::os::unix::fs::PermissionsExt;
    for (id, program) in [("claude-code", "claude"), ("codex", "codex")] {
        let home = home(&format!("failed-add-{id}"));
        let old = json!({"command":"ibis","args":["mcp","--file","invented.bib"],"env":{"INVENTED":"keep this"}});
        let settings = if id == "codex" {
            let settings = home.root.join(".codex/config.toml");
            fs::create_dir_all(settings.parent().unwrap()).unwrap();
            fs::write(
                &settings,
                toml::to_string(&json!({"mcp_servers":{"ibis":old}})).unwrap(),
            )
            .unwrap();
            settings
        } else {
            let settings = home.root.join(".claude.json");
            fs::write(&settings, json!({"mcpServers":{"ibis":old}}).to_string()).unwrap();
            settings
        };
        let log = home.root.join("program-args.txt");
        let script = home.root.join("bin").join(program);
        fs::write(
            &script,
            format!(
                "#!/bin/sh\necho \"$@\" >> '{}'\ncase \"$2\" in\nremove) rm '{}' ;;\nadd) exit 1 ;;\nesac\n",
                log.display(),
                settings.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let (result, _) = run(&home, &["setup", "--yes", "--replace", "--client", id], "", false);
        let result = result.ok().unwrap();
        assert_eq!(outcome(&result, id), "failed");
        assert!(!settings.exists(), "the fake remove took out the entry");
        let calls = fs::read_to_string(log).unwrap();
        assert!(calls.lines().next().unwrap().starts_with("mcp remove"), "{calls}");
        assert!(calls.lines().nth(1).unwrap().starts_with("mcp add"), "{calls}");
        let detail = result.data["apps"][0]["detail"].as_str().unwrap();
        assert!(detail.contains("the old ibis entry was taken out"), "{detail}");
        assert!(detail.contains(&format!("\"ibis\": {old}")), "{detail}");
        assert!(detail.contains(GUIDE), "{detail}");
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
    let (result, _) = run(&home, &["setup", "--yes", "--client", "claude-desktop"], "", false);
    assert_eq!(outcome(&result.ok().unwrap(), "claude-desktop"), "connected");
    let written = fs::read_to_string(&path).unwrap();
    assert!(!written.replace("\r\n", "").contains('\n'), "{written:?}");
    let value: Value = serde_json::from_str(&written).unwrap();
    assert_eq!(value["mcpServers"]["ibis"]["command"], command.to_str().unwrap());
    // A usable absolute ibis command is connected and is what removal takes out.
    let (status, _) = run(&home, &["status", "--client", "claude-desktop"], "", false);
    assert_eq!(outcome(&status.ok().unwrap(), "claude-desktop"), "connected");
    run(&home, &["remove", "--yes", "--client", "claude-desktop"], "", false)
        .0
        .ok()
        .unwrap();
    let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(value["mcpServers"], json!({}));
}

fn known_command(home: &mut Home) -> PathBuf {
    let command = home.root.join("bin").join("ibis");
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

#[test]
fn status_reports_an_unusable_absolute_client_command_as_stale() {
    for id in ["claude-desktop", "lm-studio"] {
        let mut home = home(&format!("stale-status-{id}"));
        for command in stale_commands(&mut home) {
            let path = absolute_settings(&home, id, &command);
            let before = fs::read(&path).unwrap();
            let (result, _) = run(&home, &["status", "--client", id], "", false);
            let result = result.ok().unwrap();
            assert_eq!(outcome(&result, id), "stale", "{command}");
            assert_eq!(result.exit_code, ExitCode::Findings);
            assert!(result.text.unwrap().contains("ibis agents setup"));
            assert_eq!(fs::read(path).unwrap(), before);
        }
        // Without a known command there is no better path to write.
        home.ibis = None;
        absolute_settings(&home, id, COMMAND);
        let (result, _) = run(&home, &["status", "--client", id], "", false);
        assert_eq!(outcome(&result.ok().unwrap(), id), "connected");
    }
}

#[test]
fn setup_repairs_a_stale_command_without_the_different_entry_question() {
    for id in ["claude-desktop", "lm-studio"] {
        let mut home = home(&format!("stale-setup-{id}"));
        for command in stale_commands(&mut home) {
            let path = absolute_settings(&home, id, &command);
            let old = json!({"command":command,"args":ARGS});
            let (result, asked) = run(&home, &["setup", "--client", id], "y\n", true);
            let result = result.ok().unwrap();
            assert_eq!(outcome(&result, id), "replaced", "{command}");
            assert!(!asked.contains("different ibis entry"), "{asked}");
            let detail = result.data["apps"][0]["detail"].as_str().unwrap();
            assert!(detail.contains(&format!("- \"ibis\": {old}")), "{detail}");
            assert!(detail.contains("+ \"ibis\":"), "{detail}");
            let written: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
            assert_eq!(written["mcpServers"]["ibis"]["command"], home.command(true));
        }
    }
}

#[test]
fn removal_takes_out_a_stale_entry_that_setup_owns() {
    for id in ["claude-desktop", "lm-studio"] {
        let mut home = home(&format!("stale-remove-{id}"));
        for command in stale_commands(&mut home) {
            let path = absolute_settings(&home, id, &command);
            let (result, _) = run(&home, &["remove", "--yes", "--client", id], "", false);
            assert_eq!(outcome(&result.ok().unwrap(), id), "removed");
            let written: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
            assert_eq!(written["mcpServers"], json!({}));
        }
    }
}

#[test]
fn a_stale_entry_changed_after_the_plan_is_left_alone() {
    struct SaveOnAnswer<'a> {
        path: &'a Path,
        saved: &'a str,
    }
    impl std::io::Read for SaveOnAnswer<'_> {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Ok(0)
        }
    }
    impl BufRead for SaveOnAnswer<'_> {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            Ok(&[])
        }
        fn consume(&mut self, _: usize) {}
        fn read_line(&mut self, answer: &mut String) -> std::io::Result<usize> {
            fs::write(self.path, self.saved)?;
            answer.push_str("y\n");
            Ok(2)
        }
    }
    for action in ["setup", "remove"] {
        let mut home = home(&format!("changed-stale-{action}"));
        known_command(&mut home);
        let path = absolute_settings(&home, "claude-desktop", COMMAND);
        let saved = json!({"mcpServers":{"ibis":{"command":home.root.join("another/ibis"),"args":ARGS}}}).to_string();
        let matches = command()
            .try_get_matches_from(["agents", action, "--client", "claude-desktop"])
            .unwrap();
        let result = run_in(
            &home,
            &matches,
            &mut SaveOnAnswer {
                path: &path,
                saved: &saved,
            },
            &mut vec![],
            true,
        )
        .ok()
        .unwrap();
        assert_eq!(outcome(&result, "claude-desktop"), "failed");
        assert!(result.text.unwrap().contains(CHANGED));
        assert_eq!(fs::read_to_string(path).unwrap(), saved);
    }
}

#[test]
fn an_entry_with_settings_of_its_own_is_never_removed() {
    let home = home("own-settings");
    let path = desktop(&home);
    let text = r#"{"mcpServers":{"ibis":{"command":"ibis","args":["mcp","--app-libraries"],"env":{"INVENTED":"1"}}}}"#;
    fs::write(&path, text).unwrap();
    let (result, _) = run(&home, &["remove", "--yes", "--client", "claude-desktop"], "", false);
    assert_eq!(outcome(&result.ok().unwrap(), "claude-desktop"), "kept");
    assert_eq!(fs::read_to_string(&path).unwrap(), text);
}

#[cfg(unix)]
#[test]
fn backup_names_taken_by_links_or_public_files_are_kept_and_skipped() {
    use std::os::unix::fs::PermissionsExt;
    let home = home("backup-link");
    let path = desktop(&home);
    fs::write(&path, DESKTOP).unwrap();
    let elsewhere = home.root.join("elsewhere.txt");
    fs::write(&elsewhere, "invented").unwrap();
    let link = path.with_file_name("claude_desktop_config.json.ibis-backup-1");
    std::os::unix::fs::symlink(&elsewhere, &link).unwrap();
    let public = path.with_file_name("claude_desktop_config.json.ibis-backup-2");
    fs::write(&public, "older backup").unwrap();
    fs::set_permissions(&public, fs::Permissions::from_mode(0o644)).unwrap();
    let (result, _) = run(&home, &["setup", "--yes", "--client", "claude-desktop"], "", false);
    assert_eq!(outcome(&result.ok().unwrap(), "claude-desktop"), "connected");
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
    let (result, _) = run(&home, &["setup", "--yes", "--client", "codex"], "", false);
    assert_eq!(outcome(&result.ok().unwrap(), "codex"), "blocked");
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
    let (status, _) = run(&home, &["status", "--client", "codex"], "", false);
    assert_eq!(outcome(&status.ok().unwrap(), "codex"), "connected");
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
