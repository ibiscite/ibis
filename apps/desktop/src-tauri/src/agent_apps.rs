//! Fixed app IDs and native environment; the web view never supplies a file or command.
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
) -> Result<Vec<AgentApp>, String> {
    operation(app, Step::Connect { id, replace, expected }).await
}

/// Disconnect only if expected is the revision of the displayed connection.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn disconnect_agent_app(
    app: tauri::AppHandle,
    id: String,
    expected: String,
) -> Result<Vec<AgentApp>, String> {
    operation(app, Step::Disconnect { id, expected }).await
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
    let apps = agent_apps::list(home);
    let (id, expected, replace, connect) = match step {
        Step::Status => return Ok(apps),
        Step::Connect { id, replace, expected } => (id, expected, replace, true),
        Step::Disconnect { id, expected } => (id, expected, false, false),
    };
    let app = apps
        .iter()
        .find(|item| item.id == id)
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
        return Err(
            "Could not change the AI app connection. Check the app’s settings and folder permissions, then try again."
                .into(),
        );
    }
    Ok(agent_apps::list(home))
}

#[cfg(test)]
mod tests {
    use super::*;
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
