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
    let mut home = home("unattended");
    known_command(&mut home);
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
