//! Shared read-only MCP setup for Settings and `ibis agents`.
//! Client commands own their formats; JSON edits preserve other settings,
//! keep private backups and refuse changed files, links and non-JSON.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::Serialize;
use serde_json::value::RawValue;
use serde_json::{Value, json};

/// The server every app is given, as the plugin ships it.
pub const COMMAND: &str = "ibis";
pub const ARGS: [&str; 2] = ["mcp", "--app-libraries"];
pub const GUIDE: &str = "https://ibiscite.org/setup";
const CHANGED: &str = "its settings changed while Ibis was working; nothing was written, so run the command again";

/// How an app keeps its MCP servers.
#[derive(Clone, Copy)]
enum Store {
    /// A JSON file with servers under `key`; `typed` entries carry `"type": "stdio"`.
    /// Apps opened from the Dock or Start menu may not see the terminal's PATH,
    /// so `absolute` ones are given the full path of `ibis`.
    Json {
        file: fn(&Home) -> Option<PathBuf>,
        key: &'static str,
        typed: bool,
        absolute: bool,
    },
    /// The app's own command adds and removes servers; its file is only read.
    Program {
        program: &'static str,
        add: &'static [&'static str],
        remove: &'static [&'static str],
        file: fn(&Home) -> PathBuf,
        read: fn(&str) -> Result<Value, String>,
    },
    /// Set up in the app's own window: `steps` names the catalogue message.
    ByHand { steps: &'static str },
}

struct Client {
    id: &'static str,
    name: &'static str,
    store: Store,
    /// Whether the app is on this computer.
    found: fn(&Home) -> bool,
}

const CLIENTS: [Client; 9] = [
    Client {
        id: "claude-code",
        name: "Claude Code",
        store: Store::Program {
            program: "claude",
            add: &[
                "mcp",
                "add",
                "--scope",
                "user",
                "--transport",
                "stdio",
                "ibis",
                "--",
                "ibis",
                "mcp",
                "--app-libraries",
            ],
            remove: &["mcp", "remove", "--scope", "user", "ibis"],
            file: |home| home.root.join(".claude.json"),
            read: |text| json_servers(text, "mcpServers"),
        },
        found: |home| home.on_path("claude") || home.root.join(".claude.json").is_file(),
    },
    Client {
        id: "claude-desktop",
        name: "Claude Desktop",
        store: Store::Json {
            file: |home| {
                home.app_config("Claude")
                    .map(|dir| dir.join("claude_desktop_config.json"))
            },
            key: "mcpServers",
            typed: false,
            absolute: true,
        },
        found: |home| home.app_config("Claude").is_some_and(|dir| dir.is_dir()),
    },
    Client {
        id: "codex",
        name: "Codex",
        store: Store::Program {
            program: "codex",
            add: &["mcp", "add", "ibis", "--", "ibis", "mcp", "--app-libraries"],
            remove: &["mcp", "remove", "ibis"],
            file: |home| home.codex().join("config.toml"),
            read: toml_servers,
        },
        found: |home| home.on_path("codex") || home.codex().is_dir(),
    },
    Client {
        id: "cursor",
        name: "Cursor",
        store: Store::Json {
            file: |home| Some(home.root.join(".cursor").join("mcp.json")),
            key: "mcpServers",
            typed: false,
            absolute: false,
        },
        found: |home| home.root.join(".cursor").is_dir(),
    },
    Client {
        id: "vscode",
        name: "VS Code",
        store: Store::Json {
            file: |home| home.app_config("Code").map(|dir| dir.join("User").join("mcp.json")),
            key: "servers",
            typed: true,
            absolute: false,
        },
        found: |home| home.app_config("Code").is_some_and(|dir| dir.join("User").is_dir()),
    },
    Client {
        id: "gemini",
        name: "Gemini CLI",
        store: Store::Json {
            file: |home| Some(home.root.join(".gemini").join("settings.json")),
            key: "mcpServers",
            typed: false,
            absolute: false,
        },
        found: |home| home.root.join(".gemini").is_dir(),
    },
    Client {
        id: "windsurf",
        name: "Windsurf",
        store: Store::Json {
            file: |home| Some(home.root.join(".codeium").join("windsurf").join("mcp_config.json")),
            key: "mcpServers",
            typed: false,
            absolute: false,
        },
        found: |home| home.root.join(".codeium").join("windsurf").is_dir(),
    },
    Client {
        id: "lm-studio",
        name: "LM Studio",
        store: Store::Json {
            file: |home| Some(home.root.join(".lmstudio").join("mcp.json")),
            key: "mcpServers",
            typed: false,
            absolute: true,
        },
        found: |home| home.root.join(".lmstudio").is_dir(),
    },
    Client {
        id: "chatgpt",
        name: "ChatGPT desktop",
        store: Store::ByHand {
            steps: "app-agent-steps-chatgpt",
        },
        found: |home| home.app_config("ChatGPT").is_some_and(|dir| dir.is_dir()),
    },
];

/// Where apps keep their settings, from the environment, so tests never touch real ones.
pub struct Home {
    pub root: PathBuf,
    /// macOS Application Support, Windows Roaming AppData or Linux XDG config.
    pub config: Option<PathBuf>,
    pub path: Vec<PathBuf>,
    pub codex_home: Option<PathBuf>,
    /// A durable CLI path for apps that need one; None blocks connecting them.
    pub ibis: Option<PathBuf>,
}

impl Home {
    fn app_config(&self, app: &str) -> Option<PathBuf> {
        self.config.as_ref().map(|dir| dir.join(app))
    }

    fn codex(&self) -> PathBuf {
        self.codex_home.clone().unwrap_or_else(|| self.root.join(".codex"))
    }

    pub fn find(&self, program: &str) -> Option<PathBuf> {
        let names: Vec<String> = if cfg!(windows) {
            ["exe", "cmd", "bat"]
                .iter()
                .map(|ext| format!("{program}.{ext}"))
                .collect()
        } else {
            vec![program.to_owned()]
        };
        // A relative PATH entry would resolve against wherever this runs.
        self.path
            .iter()
            .filter(|dir| dir.is_absolute())
            .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
            .find(|candidate| executable(candidate))
    }

    pub fn on_path(&self, program: &str) -> bool {
        self.find(program).is_some()
    }
}

pub fn executable(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else { return false };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

/// What an app has for `ibis` now.
#[derive(Debug, Clone, PartialEq, Serialize)]
enum Found {
    /// No entry, and nothing in the way of adding one.
    Missing,
    /// The entry Ibis would write, or one that runs the same command from another place.
    Connected(Value),
    /// Setup's entry, but its command needs an absolute executable path.
    Stale(Value),
    /// The Ibis plugin already gives this app the server.
    Plugin,
    /// Another `ibis` entry: shown, and replaced only on a yes.
    Different(Value),
    /// The file can't be changed safely; the reason says why.
    Blocked(String),
}

/// The entry setup writes, from any `ibis`: nothing added that removal would lose.
fn ours(entry: &Value) -> bool {
    entry["command"]
        .as_str()
        .map(Path::new)
        .and_then(Path::file_stem)
        .is_some_and(|stem| stem.eq_ignore_ascii_case(COMMAND))
        && entry["args"] == json!(ARGS)
        && entry.as_object().is_some_and(|fields| {
            fields.iter().all(|(name, value)| match name.as_str() {
                "command" | "args" => true,
                "type" => value == "stdio",
                // Claude Code writes an empty `env` of its own.
                "env" => value.as_object().is_some_and(serde_json::Map::is_empty),
                _ => false,
            })
        })
}

/// The entry as written, in the order people read it: `serde_json` would sort its keys.
fn entry(typed: bool, command: &str) -> String {
    let args = ARGS.map(|arg| format!("\"{arg}\"")).join(",");
    let kind = if typed { "\"type\":\"stdio\"," } else { "" };
    let command = Value::String(command.to_owned());
    format!("{{{kind}\"command\":{command},\"args\":[{args}]}}")
}

/// The servers in an app's own settings file; `Null` when it has none yet.
fn json_servers(text: &str, key: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_str(text).map_err(|_| "its settings file can't be read as JSON".to_owned())?;
    Ok(value.get(key).cloned().unwrap_or(Value::Null))
}

fn toml_servers(text: &str) -> Result<Value, String> {
    let table: toml::Table = toml::from_str(text).map_err(|_| "its config.toml can't be read".to_owned())?;
    table.get("mcp_servers").map_or(Ok(Value::Null), |servers| {
        serde_json::to_value(servers).map_err(|_| "its config.toml can't be read".to_owned())
    })
}

fn read_file(path: &Path) -> Result<Option<String>, String> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("its settings file can't be read".into()),
        Ok(meta) if meta.file_type().is_symlink() => Err("its settings file is a link, so Ibis leaves it alone".into()),
        Ok(meta) if !meta.is_file() => Err("its settings file is not a regular file".into()),
        Ok(_) => fs::read_to_string(path)
            .map(Some)
            .map_err(|_| "its settings file can't be read as text".into()),
    }
}

fn plugin_installed(home: &Home) -> bool {
    let path = home.root.join(".claude").join("plugins").join("installed_plugins.json");
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .is_some_and(|value| {
            value["plugins"]
                .as_object()
                .is_some_and(|plugins| plugins.keys().any(|name| name.starts_with("ibis@")))
        })
}

fn state(client: &Client, home: &Home) -> Found {
    let servers = match client.store {
        Store::ByHand { .. } => return Found::Missing,
        Store::Json { file, key, .. } => {
            let Some(path) = file(home) else {
                return Found::Blocked("its settings folder is unknown here".into());
            };
            match read_file(&path) {
                Err(reason) => return Found::Blocked(reason),
                Ok(None) => return Found::Missing,
                Ok(Some(text)) if text.trim().is_empty() => return Found::Missing,
                Ok(Some(text)) => match serde_json::from_str::<Value>(&text) {
                    Ok(Value::Object(object)) => match object.get(key) {
                        None => return Found::Missing,
                        Some(Value::Object(servers)) => Value::Object(servers.clone()),
                        Some(_) => return Found::Blocked(format!("its \"{key}\" setting is not an object")),
                    },
                    _ => return Found::Blocked("its settings file is not plain JSON (it may hold comments)".into()),
                },
            }
        }
        Store::Program { file, read, .. } => {
            // Only read, so a linked dotfile is fine; the app's own command writes it.
            let servers = match fs::read_to_string(file(home)) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Value::Null,
                Err(_) => return Found::Blocked("its settings file can't be read".into()),
                Ok(text) => match read(&text) {
                    Ok(servers) => servers,
                    Err(reason) => return Found::Blocked(reason),
                },
            };
            if servers.get(COMMAND).is_none() && client.id == "claude-code" && plugin_installed(home) {
                return Found::Plugin;
            }
            servers
        }
    };
    match servers.get(COMMAND) {
        None => Found::Missing,
        Some(entry) if ours(entry) => {
            let needs_path = matches!(client.store, Store::Json { absolute: true, .. })
                && entry["command"]
                    .as_str()
                    .map(Path::new)
                    .is_none_or(|path| !path.is_absolute() || !executable(path));
            if needs_path {
                Found::Stale(entry.clone())
            } else {
                Found::Connected(entry.clone())
            }
        }
        Some(entry) => Found::Different(entry.clone()),
    }
}

/// A connection's state; the same observation feeds Settings and the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AgentAppState {
    Connected,
    NotConnected,
    Stale,
    Different,
    Blocked,
    Plugin,
    ByHand,
}

/// One app's immutable plan. Only this module can construct a plan or choose its file.
#[derive(Clone, Serialize, schemars::JsonSchema)]
pub struct AgentApp {
    pub id: &'static str,
    pub name: &'static str,
    pub found: bool,
    pub state: AgentAppState,
    /// Pins the entry and proposed command shown in this observation.
    pub revision: ibis_core::contract::Fingerprint,
    /// The terminal's status detail, or the reason/steps for blocked/by-hand apps.
    pub detail: String,
    /// The exact before/after entry, also used by the terminal's replacement question.
    pub diff: Option<String>,
    pub needs_command: bool,
    pub ibis_on_path: bool,
    #[serde(skip)]
    client: &'static Client,
    #[serde(skip)]
    planned: Found,
}

/// Stable terminal/report outcomes, also used to distinguish refused native changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Connected,
    NotConnected,
    Stale,
    Different,
    Blocked,
    ByHand,
    WouldReplace,
    WouldConnect,
    Unchanged,
    Kept,
    Failed,
    Replaced,
    WouldRemove,
    Removed,
}
impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::NotConnected => "not-connected",
            Self::Stale => "stale",
            Self::Different => "different",
            Self::Blocked => "blocked",
            Self::ByHand => "by-hand",
            Self::WouldReplace => "would-replace",
            Self::WouldConnect => "would-connect",
            Self::Unchanged => "unchanged",
            Self::Kept => "kept",
            Self::Failed => "failed",
            Self::Replaced => "replaced",
            Self::WouldRemove => "would-remove",
            Self::Removed => "removed",
        }
    }
}

/// The result of a planned change, including unchanged and dry-run outcomes.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Change {
    pub outcome: Outcome,
    pub detail: String,
}

impl Change {
    fn new(outcome: Outcome, detail: String) -> Self {
        Self { outcome, detail }
    }
}

fn wanted(client: &Client, home: &Home) -> String {
    match client.store {
        Store::Json { typed, absolute, .. } => {
            let command = home
                .ibis
                .as_ref()
                .filter(|_| absolute)
                .map(|path| path.to_string_lossy());
            entry(typed, command.as_deref().unwrap_or(COMMAND))
        }
        _ => entry(false, COMMAND),
    }
}

/// The steps for an app set up by hand, from the catalogue message `steps` names.
fn by_hand(steps: &str) -> String {
    crate::catalogue::text(steps, None, &[]).unwrap_or_else(|| steps.to_owned())
}

fn missing_full_path(client: &Client, home: &Home) -> bool {
    matches!(client.store, Store::Json { absolute: true, .. }) && home.ibis.is_none()
}

fn observe(client: &'static Client, home: &Home) -> AgentApp {
    let planned = state(client, home);
    let (state, detail) = match (&planned, client.store) {
        (_, Store::ByHand { steps }) => (AgentAppState::ByHand, by_hand(steps)),
        (Found::Connected(_), _) => (AgentAppState::Connected, String::new()),
        (Found::Plugin, _) => (AgentAppState::Plugin, "through the Ibis plugin".into()),
        (Found::Missing | Found::Stale(_) | Found::Different(_), _) if missing_full_path(client, home) => {
            (AgentAppState::Blocked, "Install the ibis command first".into())
        }
        (Found::Missing, _) => (AgentAppState::NotConnected, "run: ibis agents setup".into()),
        (Found::Stale(_), _) => (
            AgentAppState::Stale,
            "its ibis command needs a usable full path; run: ibis agents setup".into(),
        ),
        (Found::Different(entry), _) => (AgentAppState::Different, format!("its ibis entry is {entry}")),
        (Found::Blocked(reason), _) => (AgentAppState::Blocked, reason.clone()),
    };
    let wanted = wanted(client, home);
    let diff = match &planned {
        Found::Different(old) | Found::Stale(old) => Some(format!("- \"ibis\": {old}\n+ \"ibis\": {wanted}")),
        _ => None,
    };
    let revision =
        ibis_core::contract::Fingerprint::of_bytes(json!([client.id, &planned, &wanted]).to_string().as_bytes());
    AgentApp {
        revision,
        id: client.id,
        name: client.name,
        found: (client.found)(home),
        state,
        detail,
        diff,
        needs_command: !matches!(client.store, Store::Json { absolute: true, .. })
            || (state == AgentAppState::Blocked && missing_full_path(client, home)),
        ibis_on_path: home.on_path(COMMAND),
        client,
        planned,
    }
}

/// Every supported app, with found apps first and catalogue order within each group.
pub fn list(home: &Home) -> Vec<AgentApp> {
    let mut apps: Vec<_> = CLIENTS.iter().map(|client| observe(client, home)).collect();
    apps.sort_by_key(|app| !app.found);
    apps
}

/// The CLI's plan, in catalogue order: named apps, or only the apps found here.
pub fn plan(home: &Home, chosen: &[&str]) -> Vec<AgentApp> {
    CLIENTS
        .iter()
        .filter(|client| {
            if chosen.is_empty() {
                (client.found)(home)
            } else {
                chosen.contains(&client.id)
            }
        })
        .map(|client| observe(client, home))
        .collect()
}

/// Supported IDs for command-line validation, from the same client table.
pub fn ids() -> impl Iterator<Item = &'static str> {
    CLIENTS.iter().map(|client| client.id)
}

impl AgentApp {
    pub fn would_change(&self, setup: bool) -> bool {
        if setup && self.state == AgentAppState::Blocked {
            return false;
        }
        match self.planned {
            Found::Missing | Found::Different(_) => setup && !matches!(self.client.store, Store::ByHand { .. }),
            Found::Connected(_) => !setup,
            Found::Stale(_) => true,
            _ => false,
        }
    }

    pub fn status(&self) -> Change {
        let outcome = match self.state {
            AgentAppState::Connected | AgentAppState::Plugin => Outcome::Connected,
            AgentAppState::NotConnected => Outcome::NotConnected,
            AgentAppState::Stale => Outcome::Stale,
            AgentAppState::Different => Outcome::Different,
            AgentAppState::Blocked => Outcome::Blocked,
            AgentAppState::ByHand => Outcome::ByHand,
        };
        Change {
            outcome,
            detail: self.detail.clone(),
        }
    }

    /// Replacement is explicit; operations never prompt.
    pub fn connect(&self, home: &Home, write: bool, dry_run: bool, replace: bool) -> Change {
        let client = self.client;
        let planned = &self.planned;
        let replacing = match planned {
            Found::Connected(_) => return Change::new(Outcome::Connected, "already connected".into()),
            Found::Plugin => {
                return Change::new(
                    Outcome::Connected,
                    "the Ibis plugin already connects it; nothing added, so it isn't there twice".into(),
                );
            }
            Found::Blocked(reason) => {
                return Change::new(Outcome::Blocked, format!("{reason}. Set it up by hand: {GUIDE}"));
            }
            Found::Missing => None,
            Found::Different(old) | Found::Stale(old) => Some(old),
        };
        if let Store::ByHand { steps } = client.store {
            return Change::new(Outcome::ByHand, by_hand(steps));
        }
        if missing_full_path(client, home) {
            return Change::new(Outcome::Blocked, "Install the ibis command first".into());
        }
        let wanted = wanted(client, home);
        let diff = replacing
            .as_ref()
            .map(|old| format!("- \"ibis\": {old}\n+ \"ibis\": {wanted}"));
        if dry_run {
            return match diff {
                Some(diff) => Change::new(Outcome::WouldReplace, diff),
                None => Change::new(Outcome::WouldConnect, String::new()),
            };
        }
        if !write {
            return Change::new(Outcome::Unchanged, String::new());
        }
        if let Some(diff) = &diff
            && matches!(planned, Found::Different(_))
            && !replace
        {
            return Change::new(
                Outcome::Kept,
                format!("its own ibis entry was kept; replace it with --replace\n{diff}"),
            );
        }
        if state(client, home) != *planned {
            return Change::new(Outcome::Failed, CHANGED.into());
        }
        let done =
            match client.store {
                Store::Json { file, key, .. } => file(home)
                    .ok_or_else(|| "its settings folder is unknown here".to_owned())
                    .and_then(|path| edit_json(&path, key, Some(wanted))),
                Store::Program {
                    program, add, remove, ..
                } => {
                    let removed = if replacing.is_some() {
                        run_program(home, program, remove)
                    } else {
                        Ok(())
                    };
                    removed.and_then(|()| {
                        run_program(home, program, add).map_err(|reason| match &replacing {
                    Some(old) => {
                        format!("{reason}; the old ibis entry was taken out. To restore it, put back:\n\"ibis\": {old}")
                    }
                    None => reason,
                })
                    })
                }
                Store::ByHand { .. } => Ok(()),
            };
        match (done, replacing) {
            (Ok(()), Some(_)) => Change::new(Outcome::Replaced, diff.unwrap_or_default()),
            (Ok(()), None) => Change::new(Outcome::Connected, "added; start a new session in the app".into()),
            (Err(reason), _) => Change::new(Outcome::Failed, format!("{reason}. Set it up by hand: {GUIDE}")),
        }
    }

    /// Remove only the entry setup owns, after rechecking the plan.
    pub fn disconnect(&self, home: &Home, write: bool, dry_run: bool) -> Change {
        let client = self.client;
        let planned = &self.planned;
        match planned {
            Found::Missing => return Change::new(Outcome::NotConnected, String::new()),
            Found::Plugin => {
                return Change::new(
                    Outcome::Kept,
                    "connected through the Ibis plugin; remove it with: claude plugin uninstall ibis@ibiscite".into(),
                );
            }
            Found::Different(old) => {
                return Change::new(
                    Outcome::Kept,
                    format!("its ibis entry is not the one setup adds, so it was kept: {old}"),
                );
            }
            Found::Blocked(reason) => return Change::new(Outcome::Blocked, reason.clone()),
            Found::Connected(_) | Found::Stale(_) => {}
        }
        if let Store::ByHand { steps } = client.store {
            return Change::new(Outcome::ByHand, by_hand(steps));
        }
        if dry_run {
            return Change::new(Outcome::WouldRemove, String::new());
        }
        if !write {
            return Change::new(Outcome::Unchanged, String::new());
        }
        if state(client, home) != *planned {
            return Change::new(Outcome::Failed, CHANGED.into());
        }
        let done = match client.store {
            Store::Json { file, key, .. } => file(home)
                .ok_or_else(|| "its settings folder is unknown here".to_owned())
                .and_then(|path| edit_json(&path, key, None)),
            Store::Program { program, remove, .. } => run_program(home, program, remove),
            Store::ByHand { .. } => Ok(()),
        };
        match done {
            Ok(()) => Change::new(Outcome::Removed, String::new()),
            Err(reason) => Change::new(Outcome::Failed, reason),
        }
    }
}

fn run_program(home: &Home, program: &str, args: &[&str]) -> Result<(), String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let path = home
        .find(program)
        .ok_or_else(|| format!("the {program} command isn't on PATH"))?;
    let mut command = std::process::Command::new(path);
    if let Some(config) = &home.config {
        command.env("APPDATA", config).env("XDG_CONFIG_HOME", config);
    }
    let child = command
        .args(args)
        .env("HOME", &home.root)
        .env("USERPROFILE", &home.root)
        .env("CODEX_HOME", home.codex())
        .env(
            "PATH",
            std::env::join_paths(&home.path).map_err(|_| "the command PATH could not be used")?,
        )
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| format!("{program} could not be started"))?;
    finish_program(child, program, args, deadline)
}

/// Kill and reap a client that hangs, so later changes can proceed.
fn finish_program(
    mut child: std::process::Child,
    program: &str,
    args: &[&str],
    deadline: std::time::Instant,
) -> Result<(), String> {
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{program} could not be waited for"));
            }
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{program} did not finish in time"));
        }
        std::thread::sleep(remaining.min(std::time::Duration::from_millis(10)));
    };
    if status.success() {
        Ok(())
    } else {
        Err(format!("`{program} {}` did not succeed", args.join(" ")))
    }
}

/// Put `entry` under `key` as `ibis`, or take `ibis` out when it is `None`,
/// keeping every other value's text and order, and the file's indentation.
fn edit_json(path: &Path, key: &str, entry: Option<String>) -> Result<(), String> {
    let before = read_file(path)?;
    let text = before.as_deref().unwrap_or_default();
    let mut top: IndexMap<String, Box<RawValue>> = if text.trim().is_empty() {
        IndexMap::new()
    } else {
        serde_json::from_str(text)
            .map_err(|_| "its settings file is not plain JSON (it may hold comments)".to_owned())?
    };
    let mut servers: IndexMap<String, Box<RawValue>> = match top.get(key) {
        Some(raw) => serde_json::from_str(raw.get()).map_err(|_| format!("its \"{key}\" setting is not an object"))?,
        None => IndexMap::new(),
    };
    match entry {
        Some(entry) => {
            let raw = RawValue::from_string(entry).map_err(|_| "the entry could not be written")?;
            servers.insert(COMMAND.to_owned(), raw);
        }
        None => {
            servers.shift_remove(COMMAND);
        }
    }
    let raw = serde_json::value::to_raw_value(&servers).map_err(|_| "the entry could not be written")?;
    top.insert(key.to_owned(), raw);
    let compact = serde_json::to_string(&top).map_err(|_| "the file could not be written")?;
    let mut written = indent(&compact, &indent_unit(text));
    written.push('\n');
    if text.contains("\r\n") {
        written = written.replace('\n', "\r\n");
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| "its settings folder could not be created")?;
    }
    let staged = ibis_os::atomic_file::AtomicFile::new(path, written.as_bytes())
        .map_err(|_| "the file could not be written".to_owned())?;
    let backup = if before.is_some() {
        let message = "a backup of the file could not be written; nothing was changed";
        let (backup, mut file) = ibis_os::atomic_file::create_numbered(path, ".ibis-backup").map_err(|_| message)?;
        file.write_all(text.as_bytes())
            .and_then(|()| file.sync_all())
            .inspect_err(|_| {
                let _ = fs::remove_file(&backup);
            })
            .map_err(|_| message)?;
        Some(backup)
    } else {
        None
    };
    // Recheck after staging/backup, including link swaps. An external writer
    // can still race the rename.
    #[cfg(test)]
    tests::before_final_check();
    let checked = read_file(path).and_then(|latest| {
        if latest == before {
            Ok(())
        } else {
            Err(CHANGED.to_owned())
        }
    });
    if let Err(reason) = checked {
        if let Some(backup) = backup {
            fs::remove_file(backup).map_err(|_| format!("{reason}; its new backup could not be removed"))?;
        }
        return Err(reason);
    }
    let written = if before.is_some() {
        staged.replace()
    } else {
        staged.create()
    };
    // A backup is kept only beside a change that was made.
    written.map_err(|_| {
        if let Some(backup) = backup {
            let _ = fs::remove_file(backup);
        }
        "the file could not be written".to_owned()
    })
}

/// The file's own indentation, two spaces when it has none yet.
fn indent_unit(text: &str) -> String {
    text.lines()
        .skip(1)
        .find_map(|line| {
            let unit: String = line.chars().take_while(|ch| *ch == ' ' || *ch == '\t').collect();
            (!unit.is_empty()).then_some(unit)
        })
        .unwrap_or_else(|| "  ".to_owned())
}

/// Pretty-print compact JSON without reordering or reparsing anything.
fn indent(compact: &str, unit: &str) -> String {
    let mut out = String::with_capacity(compact.len() * 2);
    let mut depth = 0usize;
    let (mut in_string, mut escaped) = (false, false);
    let mut chars = compact.chars().peekable();
    let newline = |out: &mut String, depth: usize| {
        out.push('\n');
        out.push_str(&unit.repeat(depth));
    };
    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            match (escaped, ch) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                out.push(ch);
            }
            '{' | '[' => {
                out.push(ch);
                // Kept values carry their own spacing, as in `{ }`.
                while chars.next_if(|next| next.is_whitespace()).is_some() {}
                if matches!(chars.peek(), Some('}' | ']')) {
                    if let Some(close) = chars.next() {
                        out.push(close);
                    }
                } else {
                    depth += 1;
                    newline(&mut out, depth);
                }
            }
            '}' | ']' => {
                depth = depth.saturating_sub(1);
                newline(&mut out, depth);
                out.push(ch);
            }
            ',' => {
                out.push(ch);
                newline(&mut out, depth);
            }
            ':' => out.push_str(": "),
            ch if ch.is_whitespace() => {}
            ch => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
#[path = "agent_apps_tests.rs"]
mod tests;
