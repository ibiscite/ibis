use super::*;
use serde_json::Value;
use std::fs;
use std::path::Path;
const CHANGED: &str = "its settings changed while Ibis was working; nothing was written, so run the command again";
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

#[test]
fn status_and_dry_run_keep_the_terminal_and_json_report_bytes() {
    let mut home = home("report-bytes");
    known_command(&mut home);
    let (status, questions) = run(&home, &["status", "--client", "cursor"], "", false);
    let status = status.ok().unwrap();
    assert_eq!(questions, "");
    assert_eq!(status.exit_code, ExitCode::Ok);
    assert_eq!(
        status.text.unwrap(),
        "Cursor           not connected: run: ibis agents setup"
    );
    assert_eq!(
        serde_json::to_string(&status.data).unwrap(),
        r#"{"action":"status","apps":[{"detail":"run: ibis agents setup","id":"cursor","name":"Cursor","outcome":"not-connected"}],"dry_run":false,"ibis_on_path":true,"server":{"args":["mcp","--app-libraries"],"command":"ibis"}}"#
    );
    let (preview, questions) = run(&home, &["setup", "--dry-run", "--client", "cursor"], "", false);
    let preview = preview.ok().unwrap();
    assert_eq!(questions, "");
    assert_eq!(preview.exit_code, ExitCode::Ok);
    assert_eq!(
        preview.text.unwrap(),
        "Cursor           would connect\n\nNothing was changed (--dry-run)."
    );
    assert_eq!(
        serde_json::to_string(&preview.data).unwrap(),
        r#"{"action":"setup","apps":[{"detail":"","id":"cursor","name":"Cursor","outcome":"would-connect"}],"dry_run":true,"ibis_on_path":true,"server":{"args":["mcp","--app-libraries"],"command":"ibis"}}"#
    );
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
                "[mcp_servers.ibis]\ncommand = \"ibis\"\nargs = [\"mcp\", \"--file\", \"invented.bib\"]\n[mcp_servers.ibis.env]\nINVENTED = \"keep this\"\n",
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
                "#!/bin/sh\necho \"$@\" >> '{}'\ncase \"$2\" in\nremove) /bin/rm '{}' ;;\nadd) exit 1 ;;\nesac\n",
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
    let path = match id {
        "claude-desktop" => home.config.as_ref().unwrap().join("Claude/claude_desktop_config.json"),
        "lm-studio" => home.root.join(".lmstudio/mcp.json"),
        _ => panic!("absolute-path fixture"),
    };
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
            assert_eq!(
                written["mcpServers"]["ibis"]["command"],
                home.ibis.as_ref().unwrap().to_string_lossy().into_owned()
            );
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
