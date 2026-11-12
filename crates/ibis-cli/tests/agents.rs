//! Settings integration uses invented homes, never the caller's client settings.
#![allow(clippy::unwrap_used)]
#![cfg(unix)]
use ibis::agents::{Client, Context};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

fn context(home: &Path) -> Context {
    Context::new(
        home.to_owned(),
        home.join("ibis-data"),
        std::env::current_exe().unwrap(),
    )
    .unwrap()
}

fn primary(home: &Path, client: Client) -> std::path::PathBuf {
    home.join(match client {
        Client::ClaudeCode => ".claude/settings.json",
        Client::ClaudeDesktop => "ibis-data/agents/claude-desktop/manifest.json",
        Client::Codex | Client::ChatgptDesktop => ".agents/plugins/marketplace.json",
        Client::Cursor => ".cursor/mcp.json",
        Client::Vscode => ".copilot/mcp-config.json",
        Client::Gemini => ".gemini/extensions/ibis/gemini-extension.json",
    })
}

fn files(root: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        for item in fs::read_dir(dir).unwrap() {
            let path = item.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.push(path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut out = vec![];
    walk(root, root, &mut out);
    out.sort();
    out
}

#[test]
fn every_adapter_is_offline_repeatable_and_removes_only_its_own_files() {
    for client in Client::ALL {
        let home = tempfile::tempdir().unwrap();
        let ctx = context(home.path());
        let plan = ctx.setup(&[client]).unwrap();
        assert!(!plan.actions().is_empty(), "{client:?}");
        assert!(files(home.path()).is_empty(), "planning/dry-run must not write");
        plan.apply().unwrap();
        let made = files(home.path());
        let group = if client == Client::ChatgptDesktop {
            "codex"
        } else {
            client.name()
        };
        let mut expected = vec![format!("ibis-data/agents/{group}.json")];
        if matches!(client, Client::ClaudeCode | Client::Codex | Client::ChatgptDesktop) {
            for file in [
                "plugin.json",
                ".claude-plugin/plugin.json",
                ".claude-plugin/icon.png",
                "skills/ibis-cite/SKILL.md",
                ".mcp.json",
                "mcp.json",
            ] {
                expected.push(format!("ibis-data/agents/{group}/plugins/ibis/{file}"));
            }
            if client == Client::ClaudeCode {
                expected.push("ibis-data/agents/claude-code/.claude-plugin/marketplace.json".into());
                expected.push(".claude/settings.json".into());
            } else {
                expected.push(".agents/plugins/marketplace.json".into());
            }
        } else {
            expected.extend(
                match client {
                    Client::ClaudeDesktop => vec![
                        "ibis-data/agents/claude-desktop/manifest.json",
                        "ibis-data/agents/claude-desktop/ibis.mcpb",
                    ],
                    Client::Cursor => vec![".cursor/mcp.json", ".cursor/skills/ibis-cite/SKILL.md"],
                    Client::Vscode => vec![".copilot/mcp-config.json", ".copilot/skills/ibis-cite/SKILL.md"],
                    Client::Gemini => vec![
                        ".gemini/extensions/ibis/gemini-extension.json",
                        ".gemini/extensions/ibis/skills/ibis-cite/SKILL.md",
                    ],
                    _ => unreachable!(),
                }
                .into_iter()
                .map(str::to_owned),
            );
        }
        expected.sort();
        assert_eq!(made, expected, "{client:?}: exactly the documented adapter files");
        for file in made.iter().filter(|f| {
            f.ends_with("mcp.json")
                || f.ends_with("mcp-config.json")
                || f.ends_with("gemini-extension.json")
                || f.ends_with("manifest.json")
        }) {
            let config: Value = serde_json::from_slice(&fs::read(home.path().join(file)).unwrap()).unwrap();
            let server = if file.ends_with("manifest.json") {
                &config["server"]["mcp_config"]
            } else {
                &config["mcpServers"]["ibis"]
            };
            assert_eq!(
                server["command"],
                std::env::current_exe()
                    .unwrap()
                    .canonicalize()
                    .unwrap()
                    .to_str()
                    .unwrap()
            );
            assert_eq!(server["args"], json!(["mcp", "--app-libraries"]));
            if client == Client::Vscode {
                assert_eq!(server["tools"], json!(["*"]));
            }
        }
        if client == Client::ClaudeDesktop {
            let result = Command::new("/usr/bin/unzip")
                .arg("-p")
                .arg(home.path().join("ibis-data/agents/claude-desktop/ibis.mcpb"))
                .arg("manifest.json")
                .output()
                .unwrap();
            assert!(result.status.success(), "a real ZIP reader can open the MCPB");
            let manifest: Value = serde_json::from_slice(&result.stdout).unwrap();
            assert_eq!(manifest["manifest_version"], "0.3");
            assert_eq!(
                manifest["server"]["mcp_config"]["args"],
                json!(["mcp", "--app-libraries"])
            );
        }
        assert!(
            context(home.path()).setup(&[client]).unwrap().actions().is_empty(),
            "{client:?}: reopened setup"
        );
        let state = ctx.status().unwrap().into_iter().find(|s| s.client == client).unwrap();
        assert!(
            state.message.contains("connected")
                || state.message.contains("finish")
                || state.message.contains("confirm"),
            "{state:?}"
        );
        // A file in the same folder belongs to someone else.
        fs::write(home.path().join("keep.txt"), "other application").unwrap();
        ctx.remove(&[client]).unwrap().apply().unwrap();
        assert_eq!(
            fs::read_to_string(home.path().join("keep.txt")).unwrap(),
            "other application"
        );
        assert!(
            files(home.path())
                .iter()
                .all(|p| p == "keep.txt" || p.ends_with(".ibis-backup")),
            "{client:?}: {:?}",
            files(home.path())
        );
        assert!(ctx.remove(&[client]).unwrap().actions().is_empty());
    }
}

#[test]
fn configurations_keep_other_servers_comments_and_order_exactly() {
    for (client, relative, before) in [
        (
            Client::Cursor,
            ".cursor/mcp.json",
            "{\n  \"mcpServers\": {\"zeta\": { \"command\": \"other\" }, \"alpha\": {\"args\": [\"one\"]}},\n  \"theme\": \"dark\"\n}\n",
        ),
        (
            Client::Vscode,
            ".copilot/mcp-config.json",
            "{\n // keep this comment\n \"mcpServers\": {\"other\": {\"command\":\"other\"},},\n}\n",
        ),
        (
            Client::ClaudeCode,
            ".claude/settings.json",
            "{\"enabledPlugins\":{\"other@local\":true},\"extraKnownMarketplaces\":{\"other\":{\"source\":{\"source\":\"directory\",\"path\":\"/invented/other\"}}}}\n",
        ),
        (
            Client::Codex,
            ".agents/plugins/marketplace.json",
            "{\"name\":\"personal\",\"plugins\":[{\"name\":\"other\", \"source\": {\"source\":\"local\",\"path\":\"./other\"}}]}\n",
        ),
        (
            Client::ChatgptDesktop,
            ".agents/plugins/marketplace.json",
            "{\"name\":\"personal\",\"plugins\":[{\"name\":\"other\", \"source\": {\"source\":\"local\",\"path\":\"./other\"}}]}\n",
        ),
    ] {
        let home = tempfile::tempdir().unwrap();
        let ctx = context(home.path());
        let path = home.path().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, before).unwrap();
        ctx.setup(&[client]).unwrap().apply().unwrap();
        assert_eq!(
            fs::read_to_string(path.with_extension("json.ibis-backup")).unwrap(),
            before
        );
        ctx.remove(&[client]).unwrap().apply().unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), before, "{client:?}");
    }
}

#[test]
fn changed_config_and_malformed_or_ambiguous_json_are_refused_untouched() {
    for client in Client::ALL {
        let home = tempfile::tempdir().unwrap();
        let ctx = context(home.path());
        let plan = ctx.setup(&[client]).unwrap();
        // A new file appearing after planning is a concurrent edit, too.
        let path = primary(home.path(), client);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "concurrent edit").unwrap();
        assert!(plan.apply().unwrap_err().to_string().contains("changed"), "{client:?}");
        assert_eq!(fs::read_to_string(path).unwrap(), "concurrent edit");
    }
    for (client, bad) in Client::ALL
        .into_iter()
        .flat_map(|client| ["{broken", "{\"mcpServers\":{},\"mcpServers\":{}}"].map(|bad| (client, bad)))
    {
        let home = tempfile::tempdir().unwrap();
        let path = primary(home.path(), client);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bad).unwrap();
        let error = context(home.path()).setup(&[client]).unwrap_err().to_string();
        assert!(error.contains(path.file_name().unwrap().to_str().unwrap()), "{error}");
        assert_eq!(fs::read_to_string(path).unwrap(), bad);
    }
}

#[cfg(unix)]
#[test]
fn escaping_symlinks_and_world_writable_client_directories_are_refused() {
    use std::os::unix::{fs::PermissionsExt, fs::symlink};
    for client in Client::ALL {
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let ctx = context(home.path());
        let path = ctx.client_directory(client);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        symlink(outside.path(), &path).unwrap();
        assert!(ctx.setup(&[client]).is_err(), "{client:?}");
        assert!(files(outside.path()).is_empty());
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(ctx.setup(&[client]).is_err(), "{client:?}");
    }
}

#[test]
fn removing_one_openai_client_preserves_the_other_and_modified_entries_are_never_removed() {
    let home = tempfile::tempdir().unwrap();
    let ctx = context(home.path());
    assert!(ctx.status().unwrap().iter().all(|s| s.message == "not found"));
    ctx.setup(&[Client::Codex, Client::ChatgptDesktop])
        .unwrap()
        .apply()
        .unwrap();
    ctx.remove(&[Client::Codex]).unwrap().apply().unwrap();
    assert!(home.path().join(".agents/plugins/marketplace.json").exists());
    ctx.remove(&[Client::ChatgptDesktop]).unwrap().apply().unwrap();
    assert!(files(home.path()).is_empty());

    ctx.setup(&[Client::Cursor]).unwrap().apply().unwrap();
    let path = home.path().join(".cursor/mcp.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    config["mcpServers"]["ibis"]["args"] = json!(["someone else's server"]);
    let changed = config.to_string();
    fs::write(&path, &changed).unwrap();
    assert!(ctx.remove(&[Client::Cursor]).is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), changed);
}

#[test]
fn noninteractive_setup_requires_yes_and_dry_run_writes_nothing() {
    let home = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_ibis"))
            .args(args)
            .env_remove("CODEX_HOME")
            .env_remove("CLAUDE_CONFIG_DIR")
            .env_remove("COPILOT_HOME")
            .env("HOME", home.path())
            .env("USERPROFILE", home.path())
            .env("XDG_CONFIG_HOME", home.path().join(".config"))
            .env("APPDATA", home.path().join("AppData/Roaming"))
            .env("XDG_DATA_HOME", home.path().join(".local/share"))
            .env("PATH", "")
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    };
    let refused = run(&["agents", "setup"]);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("--yes"));
    assert!(!run(&["agents", "setup", "--client", "cursor"]).status.success());
    for client in Client::ALL {
        let dry = run(&["agents", "setup", "--client", client.name(), "--dry-run", "--yes"]);
        assert!(dry.status.success(), "{}", String::from_utf8_lossy(&dry.stderr));
        assert!(String::from_utf8_lossy(&dry.stdout).contains("Would"));
    }
    assert!(files(home.path()).is_empty());
}

#[cfg(unix)]
#[test]
fn replaced_ancestor_hard_links_permissions_and_backups_keep_their_boundaries() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    for client in Client::ALL {
        let home = tempfile::tempdir().unwrap();
        let ctx = context(home.path());
        let directory = ctx.client_directory(client);
        fs::create_dir_all(&directory).unwrap();
        let plan = ctx.setup(&[client]).unwrap();
        let moved = home.path().join("moved");
        fs::rename(&directory, &moved).unwrap();
        symlink(&moved, &directory).unwrap();
        assert!(plan.apply().is_err(), "{client:?}: an ancestor changed after planning");
        assert!(files(&moved).is_empty());
    }
    let home = tempfile::tempdir().unwrap();
    let ctx = context(home.path());
    let path = home.path().join(".cursor/mcp.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "{\"mcpServers\":{\"other\":{\"command\":\"other\"}}}\n").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    let backup = path.with_extension("json.ibis-backup");
    fs::write(&backup, "preexisting backup").unwrap();
    ctx.setup(&[Client::Cursor]).unwrap().apply().unwrap();
    assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o640);
    assert_eq!(fs::read_to_string(&backup).unwrap(), "preexisting backup");
    let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value["mcpServers"]["later"] = json!({"command":"later"});
    fs::write(&path, value.to_string()).unwrap();
    ctx.remove(&[Client::Cursor]).unwrap().apply().unwrap();
    let value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(value["mcpServers"]["later"]["command"], "later");
    assert_eq!(value["mcpServers"]["other"]["command"], "other");
    assert!(value["mcpServers"].get("ibis").is_none());
    assert_eq!(fs::read_to_string(&backup).unwrap(), "preexisting backup");
    let link = home.path().join("hard-link");
    fs::hard_link(&path, &link).unwrap();
    assert!(ctx.setup(&[Client::Cursor]).is_err());
}

#[test]
fn status_distinguishes_missing_apps_discovered_apps_pending_installs_and_old_commands() {
    let home = tempfile::tempdir().unwrap();
    let ctx = context(home.path());
    fs::create_dir_all(home.path().join(".cursor")).unwrap();
    assert!(
        ctx.status()
            .unwrap()
            .into_iter()
            .find(|s| s.client == Client::Cursor)
            .unwrap()
            .message
            .contains("found, not connected")
    );
    let old = home.path().join("old-ibis");
    fs::write(&old, "invented older executable").unwrap();
    let old_context = Context::new(home.path().into(), home.path().join("ibis-data"), old.clone()).unwrap();
    for client in Client::ALL {
        old_context.setup(&[client]).unwrap().apply().unwrap();
        let state = ctx.status().unwrap().into_iter().find(|s| s.client == client).unwrap();
        assert!(state.message.contains("old or missing"), "{state:?}");
    }
    fs::remove_file(&old).unwrap();
    assert!(
        ctx.status()
            .unwrap()
            .iter()
            .all(|s| s.message.contains("old or missing"))
    );
    // Reconnect with this binary, then mimic the documented OpenAI installation.
    ctx.setup(&[Client::Codex]).unwrap().apply().unwrap();
    let state = ctx
        .status()
        .unwrap()
        .into_iter()
        .find(|s| s.client == Client::Codex)
        .unwrap();
    assert!(state.message.contains("finish"), "{state:?}");
    let cache = home.path().join(".codex/plugins/cache/ibiscite-local/ibis/local");
    fs::create_dir_all(&cache).unwrap();
    for file in [
        "mcp.json",
        ".mcp.json",
        "plugin.json",
        ".claude-plugin/plugin.json",
        ".claude-plugin/icon.png",
        "skills/ibis-cite/SKILL.md",
    ] {
        let target = cache.join(file);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(
            target,
            fs::read(home.path().join("ibis-data/agents/codex/plugins/ibis").join(file)).unwrap(),
        )
        .unwrap();
    }
    let toml = "# keep this\n[plugins.\"ibis@ibiscite-local\"]\nenabled = true\n";
    fs::write(home.path().join(".codex/config.toml"), toml).unwrap();
    assert!(
        ctx.status()
            .unwrap()
            .into_iter()
            .find(|s| s.client == Client::Codex)
            .unwrap()
            .connected
    );
    assert_eq!(
        fs::read_to_string(home.path().join(".codex/config.toml")).unwrap(),
        toml
    );
}

#[test]
fn actual_setup_launches_this_ibis_without_path_and_readiness_is_read_only_and_private() {
    use std::io::Write;
    let home = tempfile::tempdir().unwrap();
    let command = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ibis"));
        command
            .env_remove("CODEX_HOME")
            .env_remove("CLAUDE_CONFIG_DIR")
            .env_remove("COPILOT_HOME");
        command
            .env("HOME", home.path())
            .env("USERPROFILE", home.path())
            .env("XDG_CONFIG_HOME", home.path().join("config"))
            .env("APPDATA", home.path().join("AppData"))
            .env("XDG_DATA_HOME", home.path().join(".local/share"))
            .env("PATH", "");
        command
    };
    let result = command()
        .args(["agents", "setup", "--client", "cursor", "--yes"])
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let config: Value = serde_json::from_slice(&fs::read(home.path().join(".cursor/mcp.json")).unwrap()).unwrap();
    let server = &config["mcpServers"]["ibis"];
    assert_eq!(
        server["command"],
        fs::canonicalize(env!("CARGO_BIN_EXE_ibis")).unwrap().to_str().unwrap()
    );
    assert_eq!(server["args"], json!(["mcp", "--app-libraries"]));
    let config_dir = if cfg!(target_os = "macos") {
        home.path().join("Library/Application Support")
    } else if cfg!(windows) {
        home.path().join("AppData")
    } else {
        home.path().join("config")
    };
    let settings = config_dir.join("com.ibiscite.ibis/settings.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    let library = home.path().join("PRIVATE-LIBRARY.bib");
    fs::write(
        &library,
        "@book{invented2026,title={Invented work},year={2026},note={PRIVATE-NOTE},url={https://private.example}}\n",
    )
    .unwrap();
    fs::write(
        &settings,
        json!({"schema_version":2,"library":{"bibtex":library}}).to_string(),
    )
    .unwrap();
    let before = files(home.path());
    let status = command().args(["agents", "status", "--json"]).output().unwrap();
    assert!(status.status.success(), "{}", String::from_utf8_lossy(&status.stderr));
    let status: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["data"]["libraries_chosen"], true);
    assert_eq!(status["data"]["libraries"][0]["item_count"], 1);
    assert_eq!(
        files(home.path()),
        before,
        "CLI readiness never writes settings or identity files"
    );
    let mut process = command()
        .args(server["args"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(process.stdin.take().unwrap(), "{}", json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"status","arguments":{},"_meta":{"io.modelcontextprotocol/protocolVersion":ibis_engine::mcp::PROTOCOL_VERSION,"io.modelcontextprotocol/clientCapabilities":{}}}})).unwrap();
    let mcp = process.wait_with_output().unwrap();
    assert!(mcp.status.success());
    let mcp: Value = serde_json::from_slice(&mcp.stdout).unwrap();
    assert_eq!(mcp["result"]["structuredContent"]["libraries"][0]["item_count"], 1);
    for output in [status.to_string(), mcp.to_string()] {
        for private in [
            "PRIVATE-LIBRARY",
            "PRIVATE-NOTE",
            "private.example",
            home.path().to_str().unwrap(),
        ] {
            assert!(!output.contains(private), "{output}");
        }
    }
    fs::remove_file(&library).unwrap();
    let status = command().args(["agents", "status", "--json"]).output().unwrap();
    let status: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["data"]["libraries_chosen"], true);
    assert_eq!(status["data"]["libraries"][0]["item_count"], 0);
    let mut process = command()
        .args(["mcp", "--app-libraries"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    writeln!(process.stdin.take().unwrap(), "{}", json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"status","arguments":{},"_meta":{"io.modelcontextprotocol/protocolVersion":ibis_engine::mcp::PROTOCOL_VERSION,"io.modelcontextprotocol/clientCapabilities":{}}}})).unwrap();
    let output = process.wait_with_output().unwrap();
    let mcp: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(mcp["result"]["structuredContent"]["libraries_chosen"], true);
    assert_eq!(
        mcp["result"]["structuredContent"]["libraries"],
        status["data"]["libraries"]
    );
}

fn installed_openai(home: &Path) -> Context {
    let ctx = context(home);
    ctx.setup(&[Client::Codex]).unwrap().apply().unwrap();
    let cache = home.join(".codex/plugins/cache/ibiscite-local/ibis/local");
    fs::create_dir_all(&cache).unwrap();
    fs::copy(
        home.join("ibis-data/agents/codex/plugins/ibis/mcp.json"),
        cache.join("mcp.json"),
    )
    .unwrap();
    fs::write(
        home.join(".codex/config.toml"),
        "[plugins.\"ibis@ibiscite-local\"]\nenabled = true\n",
    )
    .unwrap();
    ctx
}

#[test]
fn a_pending_codex_removal_refuses_a_plugin_enabled_after_planning() {
    for previous in [None, Some("[plugins.\"ibis@ibiscite-local\"]\nenabled = false\n")] {
        let home = tempfile::tempdir().unwrap();
        let ctx = installed_openai(home.path());
        let config = home.path().join(".codex/config.toml");
        if let Some(previous) = previous {
            fs::write(&config, previous).unwrap();
        } else {
            fs::remove_file(&config).unwrap();
        }
        let plan = ctx.remove(&[Client::Codex]).unwrap();
        let enabled = "[plugins.\"ibis@ibiscite-local\"]\nenabled = true\n";
        fs::write(&config, enabled).unwrap();
        assert!(plan.apply().unwrap_err().to_string().contains("changed"));
        assert_eq!(fs::read_to_string(config).unwrap(), enabled);
        assert!(home.path().join("ibis-data/agents/codex.json").exists());
    }
}

#[test]
fn a_pending_desktop_confirmation_can_be_retried_without_rewriting_the_bundle() {
    let home = tempfile::tempdir().unwrap();
    let ctx = context(home.path());
    ctx.setup(&[Client::ClaudeDesktop]).unwrap().apply().unwrap();
    // The caller did not approve/open the client confirmation on that attempt.
    let retry = ctx.setup(&[Client::ClaudeDesktop]).unwrap();
    assert!(retry.actions().is_empty());
    assert_eq!(
        retry.confirmations(),
        &[ctx.client_directory(Client::ClaudeDesktop).join("ibis.mcpb")]
    );
}

#[test]
fn a_renamed_personal_catalog_does_not_leave_its_original_plugin_enabled_after_removal() {
    let home = tempfile::tempdir().unwrap();
    let ctx = installed_openai(home.path());
    let marketplace = home.path().join(".agents/plugins/marketplace.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&marketplace).unwrap()).unwrap();
    value["name"] = json!("personal");
    fs::write(&marketplace, value.to_string()).unwrap();
    ctx.remove(&[Client::Codex]).unwrap().apply().unwrap();
    assert!(
        fs::read_to_string(home.path().join(".codex/config.toml"))
            .unwrap()
            .contains("enabled = false")
    );
    let value: Value = serde_json::from_slice(&fs::read(&marketplace).unwrap()).unwrap();
    assert_eq!(value["name"], "personal");
    assert_eq!(value["plugins"], json!([]));
}

#[test]
fn a_changed_or_new_cached_plugin_after_planning_cannot_be_revoked_as_ours() {
    for filename in ["mcp.json", ".mcp.json"] {
        let home = tempfile::tempdir().unwrap();
        let ctx = installed_openai(home.path());
        let plan = ctx.remove(&[Client::Codex]).unwrap();
        let path = home
            .path()
            .join(".codex/plugins/cache/ibiscite-local/ibis/local")
            .join(filename);
        fs::write(&path, "{\"mcpServers\":{\"ibis\":{\"command\":\"someone-else\"}}}").unwrap();
        assert!(plan.apply().unwrap_err().to_string().contains("changed"));
        assert!(
            fs::read_to_string(home.path().join(".codex/config.toml"))
                .unwrap()
                .contains("enabled = true")
        );
        assert!(home.path().join("ibis-data/agents/codex.json").exists());
    }
}

#[test]
fn an_installed_cache_from_before_an_upgrade_is_still_owned_and_can_be_disconnected() {
    let home = tempfile::tempdir().unwrap();
    let old = home.path().join("old-ibis");
    fs::write(&old, "invented older executable").unwrap();
    let old_ctx = Context::new(home.path().into(), home.path().join("ibis-data"), old.clone()).unwrap();
    old_ctx.setup(&[Client::Codex]).unwrap().apply().unwrap();
    let cache = home.path().join(".codex/plugins/cache/ibiscite-local/ibis/local");
    fs::create_dir_all(&cache).unwrap();
    fs::copy(
        home.path().join("ibis-data/agents/codex/plugins/ibis/mcp.json"),
        cache.join("mcp.json"),
    )
    .unwrap();
    fs::write(
        home.path().join(".codex/config.toml"),
        "[plugins.\"ibis@ibiscite-local\"]\nenabled = true\n",
    )
    .unwrap();
    let ctx = context(home.path());
    ctx.setup(&[Client::Codex]).unwrap().apply().unwrap();
    let state = ctx
        .status()
        .unwrap()
        .into_iter()
        .find(|s| s.client == Client::Codex)
        .unwrap();
    assert!(state.message.contains("old or missing"), "{state:?}");
    assert_eq!(state.command, Some(old.canonicalize().unwrap()));
    ctx.remove(&[Client::Codex]).unwrap().apply().unwrap();
    assert!(
        fs::read_to_string(home.path().join(".codex/config.toml"))
            .unwrap()
            .contains("enabled = false")
    );
}

#[test]
fn manual_connections_are_visible_but_setup_and_remove_do_not_take_ownership() {
    for (client, relative) in [
        (Client::Cursor, ".cursor/mcp.json"),
        (Client::Vscode, ".copilot/mcp-config.json"),
        (Client::Gemini, ".gemini/extensions/ibis/gemini-extension.json"),
    ] {
        let home = tempfile::tempdir().unwrap();
        let ctx = context(home.path());
        let path = home.path().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let value = json!({"mcpServers":{"ibis":{"command":std::env::current_exe().unwrap().canonicalize().unwrap(),"args":["mcp","--app-libraries"]}}});
        let text = value.to_string();
        fs::write(&path, &text).unwrap();
        let state = ctx.status().unwrap().into_iter().find(|s| s.client == client).unwrap();
        assert!(
            state.connected && state.message.contains("managed outside"),
            "{state:?}"
        );
        assert!(ctx.setup(&[client]).is_err());
        ctx.remove(&[client]).unwrap().apply().unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), text);
    }
}

#[test]
fn gemini_global_settings_cannot_silently_override_a_generated_connection() {
    let home = tempfile::tempdir().unwrap();
    let ctx = context(home.path());
    let settings = home.path().join(".gemini/settings.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    let text = json!({"mcpServers":{"ibis":{"command":std::env::current_exe().unwrap().canonicalize().unwrap(),"args":["mcp","--app-libraries"]}}}).to_string();
    fs::write(&settings, &text).unwrap();
    assert!(ctx.setup(&[Client::Gemini]).is_err());
    let state = ctx
        .status()
        .unwrap()
        .into_iter()
        .find(|s| s.client == Client::Gemini)
        .unwrap();
    assert!(
        state.connected && state.message.contains("managed outside"),
        "{state:?}"
    );
    ctx.remove(&[Client::Gemini]).unwrap().apply().unwrap();
    assert_eq!(fs::read_to_string(&settings).unwrap(), text);
    let other = "{\n // keep this comment\n \"mcpServers\": { \"other\": { \"command\": \"other\" } }\n}\n";
    fs::write(&settings, other).unwrap();
    ctx.setup(&[Client::Gemini]).unwrap().apply().unwrap();
    assert_eq!(fs::read_to_string(&settings).unwrap(), other);
    ctx.remove(&[Client::Gemini]).unwrap().apply().unwrap();
    assert_eq!(fs::read_to_string(&settings).unwrap(), other);
}

#[test]
fn gemini_global_override_appearing_after_planning_refuses_before_publishing() {
    let home = tempfile::tempdir().unwrap();
    let ctx = context(home.path());
    let plan = ctx.setup(&[Client::Gemini]).unwrap();
    let settings = home.path().join(".gemini/settings.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    let text = "{\"mcpServers\":{\"ibis\":{\"command\":\"another-command\"}}}";
    fs::write(&settings, text).unwrap();
    assert!(plan.apply().unwrap_err().to_string().contains("changed"));
    assert!(
        !home
            .path()
            .join(".gemini/extensions/ibis/gemini-extension.json")
            .exists()
    );
    assert_eq!(fs::read_to_string(settings).unwrap(), text);
}

#[test]
fn removal_disables_a_verified_installed_openai_plugin_without_reformatting_other_toml() {
    let home = tempfile::tempdir().unwrap();
    let ctx = context(home.path());
    ctx.setup(&[Client::Codex]).unwrap().apply().unwrap();
    let cache = home.path().join(".codex/plugins/cache/ibiscite-local/ibis/local");
    fs::create_dir_all(&cache).unwrap();
    fs::copy(
        home.path().join("ibis-data/agents/codex/plugins/ibis/mcp.json"),
        cache.join("mcp.json"),
    )
    .unwrap();
    let path = home.path().join(".codex/config.toml");
    let text = "# retain this order and comment\nmodel = 'example'\n\n[mcp_servers.other]\ncommand = 'other'\n\n[plugins.\"ibis@ibiscite-local\"]\nenabled = true # retain this too\n\n[plugins.\"other@personal\"]\nenabled=true\n";
    fs::write(&path, text).unwrap();
    ctx.remove(&[Client::Codex]).unwrap().apply().unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        text.replace("enabled = true #", "enabled = false #")
    );
    assert_eq!(
        fs::read_to_string(path.with_extension("toml.ibis-backup")).unwrap(),
        text
    );
    assert!(
        !fs::read(cache.join("mcp.json")).unwrap().is_empty(),
        "client-managed cache is left in place, disabled"
    );
}

#[test]
fn failed_io_is_resumable_and_a_resumed_upgrade_releases_its_old_identity() {
    let home = tempfile::tempdir().unwrap();
    let old = home.path().join("old-ibis");
    fs::write(&old, "invented old binary").unwrap();
    let old_context = Context::new(home.path().into(), home.path().join("ibis-data"), old).unwrap();
    let path = home.path().join(".cursor/mcp.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "{\"mcpServers\":{\"other\":{\"command\":\"other\"}}}\n").unwrap();
    old_context.setup(&[Client::Cursor]).unwrap().apply().unwrap();
    let old_config = fs::read(&path).unwrap();
    let backup = path.with_extension("json.ibis-backup");
    fs::remove_file(&backup).unwrap();
    fs::create_dir(&backup).unwrap(); // a later I/O failure after publishing the journal
    let ctx = context(home.path());
    assert!(ctx.setup(&[Client::Cursor]).unwrap().apply().is_err());
    assert_eq!(fs::read(&path).unwrap(), old_config);
    fs::remove_dir(&backup).unwrap();
    // The identical journal must still compact after a successful retry.
    ctx.setup(&[Client::Cursor]).unwrap().apply().unwrap();
    assert!(ctx.setup(&[Client::Cursor]).unwrap().actions().is_empty());
    fs::write(&path, &old_config).unwrap();
    assert!(
        ctx.remove(&[Client::Cursor]).is_err(),
        "the old identity is no longer owned after a complete upgrade"
    );
    assert_eq!(fs::read(&path).unwrap(), old_config);
}

#[test]
fn removing_a_cached_claude_plugin_disables_its_default_and_preserves_other_plugins() {
    let home = tempfile::tempdir().unwrap();
    let ctx = context(home.path());
    let settings = home.path().join(".claude/settings.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    fs::write(&settings, "{\"enabledPlugins\":{\"other@local\":true}}\n").unwrap();
    ctx.setup(&[Client::ClaudeCode]).unwrap().apply().unwrap();
    let manifest: Value = serde_json::from_slice(
        &fs::read(
            home.path()
                .join("ibis-data/agents/claude-code/plugins/ibis/.claude-plugin/plugin.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let cache = home
        .path()
        .join(".claude/plugins/cache/ibiscite-local/ibis")
        .join(manifest["version"].as_str().unwrap());
    fs::create_dir_all(&cache).unwrap();
    fs::copy(
        home.path().join("ibis-data/agents/claude-code/plugins/ibis/.mcp.json"),
        cache.join(".mcp.json"),
    )
    .unwrap();
    assert_eq!(
        manifest["defaultEnabled"], false,
        "cached plugins are disabled when their enablement entry is removed"
    );
    ctx.remove(&[Client::ClaudeCode]).unwrap().apply().unwrap();
    let value: Value = serde_json::from_slice(&fs::read(settings).unwrap()).unwrap();
    assert!(value["enabledPlugins"].get("ibis@ibiscite-local").is_none());
    assert_eq!(value["enabledPlugins"]["other@local"], true);
    assert!(value.get("extraKnownMarketplaces").is_none());
    assert!(cache.join(".mcp.json").exists());
}

#[test]
fn removal_keeps_comments_added_inside_an_ibis_created_container() {
    let home = tempfile::tempdir().unwrap();
    let ctx = context(home.path());
    ctx.setup(&[Client::Vscode]).unwrap().apply().unwrap();
    let path = home.path().join(".copilot/mcp-config.json");
    let value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let text = format!(
        "{{\"mcpServers\":{{/* keep this user comment */\"ibis\":{}}}}}\n",
        value["mcpServers"]["ibis"]
    );
    fs::write(&path, &text).unwrap();
    ctx.remove(&[Client::Vscode]).unwrap().apply().unwrap();
    assert_eq!(
        fs::read_to_string(path).unwrap(),
        "{\"mcpServers\":{/* keep this user comment */}}\n"
    );
}
