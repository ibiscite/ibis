//! Fixed app IDs and native environment; the web view never supplies a file or command.
use crate::{errors::NativeOperation, library::CommandError};
use ibis_engine::agent_apps::{self, AgentApp, Outcome};

/// Every supported AI app, found ones first.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn agent_apps(app: tauri::AppHandle) -> Result<Vec<AgentApp>, String> {
    operation(app, Step::Status).await
}

/// Connect only if expected is the revision of the displayed entry and proposal.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn connect_agent_app(
    app: tauri::AppHandle,
    id: String,
    replace: bool,
    expected: String,
) -> Result<Vec<AgentApp>, CommandError> {
    operation(app, Step::Connect { id, replace, expected })
        .await
        .map_err(|detail| connection_failure(detail, NativeOperation::ConnectAgentApp))
}

/// Disconnect only if expected is the revision of the displayed connection.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn disconnect_agent_app(
    app: tauri::AppHandle,
    id: String,
    expected: String,
) -> Result<Vec<AgentApp>, CommandError> {
    operation(app, Step::Disconnect { id, expected })
        .await
        .map_err(|detail| connection_failure(detail, NativeOperation::DisconnectAgentApp))
}

/// Private recovery text goes only to the requesting window. The catalogue's
/// generic diagnostic remains suitable for Display; never log these arguments.
fn connection_failure(detail: String, operation: NativeOperation) -> CommandError {
    let mut problem = operation.failure().problem();
    problem.info = problem.info.argument("detail", detail);
    CommandError::Presented(problem)
}

enum Step {
    Status,
    Connect {
        id: String,
        replace: bool,
        expected: String,
    },
    Disconnect {
        id: String,
        expected: String,
    },
}

async fn operation(app: tauri::AppHandle, step: Step) -> Result<Vec<AgentApp>, String> {
    static OWNER: tauri::async_runtime::Mutex<()> = tauri::async_runtime::Mutex::const_new(());
    crate::cli_install::with_owner(&OWNER, move || {
        let home = crate::cli_install::agent_home(&app)?;
        apply(&home, step)
    })
    .await
}

fn apply(home: &agent_apps::Home, step: Step) -> Result<Vec<AgentApp>, String> {
    let (id, expected, replace, connect) = match step {
        Step::Status => return Ok(agent_apps::list(home)),
        Step::Connect { id, replace, expected } => (id, expected, replace, true),
        Step::Disconnect { id, expected } => (id, expected, false, false),
    };
    let app = agent_apps::plan(home, &[&id])
        .pop()
        .ok_or("Could not find that AI app.")?;
    if app.revision.as_str() != expected {
        return Err("The AI app settings changed. Check the connection and try again.".into());
    }
    let change = if connect {
        app.connect(home, true, false, replace)
    } else {
        app.disconnect(home, true, false)
    };
    if matches!(change.outcome, Outcome::Failed | Outcome::Blocked | Outcome::Kept) {
        return Err(change.detail);
    }
    Ok(agent_apps::list(home))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn agent_failed_add_returns_the_removed_entry_to_the_window() {
        use std::os::unix::fs::PermissionsExt;
        for (id, program) in [("claude-code", "claude"), ("codex", "codex")] {
            let dir = tempfile::TempDir::new().unwrap();
            let home = agent_apps::Home {
                root: dir.path().to_owned(),
                config: None,
                path: vec![dir.path().join("bin")],
                codex_home: None,
                ibis: None,
            };
            std::fs::create_dir_all(&home.path[0]).unwrap();
            let old = serde_json::json!({"command":"other", "env":{"INVENTED":"restore this"}});
            let settings = if id == "codex" {
                let path = home.root.join(".codex/config.toml");
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(
                    &path,
                    "[mcp_servers.ibis]\ncommand = \"other\"\n[mcp_servers.ibis.env]\nINVENTED = \"restore this\"\n",
                )
                .unwrap();
                path
            } else {
                let path = home.root.join(".claude.json");
                std::fs::write(&path, serde_json::json!({"mcpServers":{"ibis":old}}).to_string()).unwrap();
                path
            };
            let script = home.path[0].join(program);
            std::fs::write(
                &script,
                format!(
                    "#!/bin/sh\ncase \"$2\" in\nremove) /bin/rm '{}' ;;\nadd) exit 1 ;;\nesac\n",
                    settings.display()
                ),
            )
            .unwrap();
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
            let preview = agent_apps::plan(&home, &[id]).remove(0);
            let error = apply(
                &home,
                Step::Connect {
                    id: id.into(),
                    replace: true,
                    expected: preview.revision.as_str().into(),
                },
            )
            .err()
            .unwrap();
            assert!(!settings.exists());
            assert!(error.contains("the old ibis entry was taken out"), "{error}");
            assert!(error.contains(&format!("\"ibis\": {old}")), "{error}");
            let failure = connection_failure(error.clone(), NativeOperation::ConnectAgentApp);
            let wire = serde_json::to_value(&failure).unwrap();
            assert_eq!(wire["message"]["args"]["detail"], error);
            assert!(!failure.to_string().contains("restore this"));
        }
    }
    #[test]
    fn agent_replace_refuses_an_entry_changed_since_its_preview() {
        let dir = tempfile::TempDir::new().unwrap();
        let home = agent_apps::Home {
            root: dir.path().to_owned(),
            config: None,
            path: vec![],
            codex_home: None,
            ibis: None,
        };
        let path = home.root.join(".cursor/mcp.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"mcpServers":{"ibis":{"command":"before"}}}"#).unwrap();
        let preview = agent_apps::list(&home)
            .into_iter()
            .find(|app| app.id == "cursor")
            .unwrap();
        let changed = r#"{"mcpServers":{"ibis":{"command":"changed after preview"}}}"#;
        std::fs::write(&path, changed).unwrap();
        assert!(
            apply(
                &home,
                Step::Connect {
                    id: "cursor".into(),
                    replace: true,
                    expected: preview.revision.as_str().into()
                }
            )
            .is_err()
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), changed);
        let latest = agent_apps::list(&home)
            .into_iter()
            .find(|app| app.id == "cursor")
            .unwrap();
        let connected = apply(
            &home,
            Step::Connect {
                id: "cursor".into(),
                replace: true,
                expected: latest.revision.as_str().into(),
            },
        )
        .unwrap();
        let connected = connected.into_iter().find(|app| app.id == "cursor").unwrap();
        assert_eq!(connected.state, agent_apps::AgentAppState::Connected);
        let removed = apply(
            &home,
            Step::Disconnect {
                id: "cursor".into(),
                expected: connected.revision.as_str().into(),
            },
        )
        .unwrap();
        assert_eq!(
            removed.into_iter().find(|app| app.id == "cursor").unwrap().state,
            agent_apps::AgentAppState::NotConnected
        );
    }
}
