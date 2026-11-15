//! `ibis agents setup|status|remove`: connect Ibis's read-only MCP server to
//! the AI apps on this computer in one step, and undo it.
//!
//! Every app gets the same server entry the plugin ships
//! (`plugins/ibis/.mcp.json`): `ibis mcp --app-libraries`. Apps with their own
//! command for adding a server (Claude Code, Codex) are changed through that
//! command; for the others Ibis merges the entry into the app's JSON file and
//! leaves every other server and setting as it was. A different `ibis` entry
//! is shown before and after and replaced only when the person says yes.
//! Removal takes out only the entry setup writes. Ibis never reads or
//! writes anything else in these files, and a file that changes while Ibis
//! works on it, is a symbolic link or is not plain JSON is left alone with the
//! step to do by hand.

use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use clap::{Arg, ArgAction, ArgMatches, Command};
use ibis_core::contract::{ErrorCode, ExitCode};
use ibis_engine::DispatchResult;
use indexmap::IndexMap;
use serde_json::value::RawValue;
use serde_json::{Value, json};

use crate::Failure;

/// The server every app is given, as the plugin ships it.
const COMMAND: &str = "ibis";
const ARGS: [&str; 2] = ["mcp", "--app-libraries"];
const GUIDE: &str = "https://ibiscite.org/setup";
const CHANGED: &str = "its settings changed while Ibis was working; nothing was written, so run the command again";

pub(crate) fn command() -> Command {
    let clients = Arg::new("client")
        .long("client")
        .value_name("APP")
        .action(ArgAction::Append)
        .value_parser(CLIENTS.iter().map(|client| client.id).collect::<Vec<_>>())
        .help("Only this app; repeat for several");
    let dry_run = Arg::new("dry-run")
        .long("dry-run")
        .action(ArgAction::SetTrue)
        .help("Show what would change and change nothing");
    let yes = Arg::new("yes")
        .long("yes")
        .short('y')
        .action(ArgAction::SetTrue)
        .help("Do it without asking; needed when no one can answer, such as in a script");
    Command::new("agents")
        .about("Connect Ibis to the AI apps on this computer, show them, or disconnect them")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("setup")
                .about("Add Ibis's read-only MCP server to each AI app found here")
                .after_help(format!(
                    "Other servers and settings are kept. A different ibis entry is shown and replaced only if you agree (or with --replace).\nUndo with: ibis agents remove\nBy hand: {GUIDE}"
                ))
                .arg(clients.clone())
                .arg(dry_run.clone())
                .arg(yes.clone())
                .arg(
                    Arg::new("replace")
                        .long("replace")
                        .action(ArgAction::SetTrue)
                        .help("Replace a different ibis entry without asking"),
                ),
        )
        .subcommand(Command::new("status").about("Show which AI apps have Ibis connected").arg(clients.clone()))
        .subcommand(
            Command::new("remove")
                .about("Take Ibis's server out of each AI app; other servers are kept")
                .arg(clients)
                .arg(dry_run)
                .arg(yes),
        )
}

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
        file: fn(&Home) -> Option<PathBuf>,
        read: fn(&str) -> Result<Value, String>,
    },
    /// Set up in the app's own window.
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
            file: |home| Some(home.root.join(".claude.json")),
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
            file: |home| Some(home.codex().join("config.toml")),
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
            steps: "In ChatGPT desktop: Settings › MCP servers › Add server › STDIO, command ibis, arguments mcp --app-libraries.",
        },
        found: |home| home.app_config("ChatGPT").is_some_and(|dir| dir.is_dir()),
    },
];

/// Where apps keep their settings, from the environment, so tests never touch real ones.
struct Home {
    root: PathBuf,
    /// macOS Application Support, Windows Roaming AppData or Linux XDG config.
    config: Option<PathBuf>,
    path: Vec<PathBuf>,
    codex_home: Option<PathBuf>,
    /// The `ibis` that apps given a full path start: the one on PATH, else this one.
    ibis: Option<PathBuf>,
}

impl Home {
    fn here() -> Option<Self> {
        let root = std::env::home_dir()?;
        let config = if cfg!(target_os = "macos") {
            Some(root.join("Library").join("Application Support"))
        } else if cfg!(windows) {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .or_else(|| Some(root.join(".config")))
        };
        let mut home = Self {
            config,
            path: std::env::var_os("PATH")
                .map(|path| std::env::split_paths(&path).collect())
                .unwrap_or_default(),
            codex_home: std::env::var_os("CODEX_HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute()),
            ibis: None,
            root,
        };
        home.ibis = home.find(COMMAND).or_else(|| std::env::current_exe().ok());
        Some(home)
    }

    /// The command an app is given: the full path where the app needs one.
    fn command(&self, absolute: bool) -> String {
        match &self.ibis {
            Some(path) if absolute => path.to_string_lossy().into_owned(),
            _ => COMMAND.to_owned(),
        }
    }

    fn app_config(&self, app: &str) -> Option<PathBuf> {
        self.config.as_ref().map(|dir| dir.join(app))
    }

    fn codex(&self) -> PathBuf {
        self.codex_home.clone().unwrap_or_else(|| self.root.join(".codex"))
    }

    fn find(&self, program: &str) -> Option<PathBuf> {
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

    fn on_path(&self, program: &str) -> bool {
        self.find(program).is_some()
    }
}

fn executable(path: &Path) -> bool {
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
#[derive(Debug, Clone, PartialEq)]
enum Found {
    /// No entry, and nothing in the way of adding one.
    Missing,
    /// The entry Ibis would write, or one that runs the same command from another place.
    Connected,
    /// Setup's entry, but its command needs an absolute executable path.
    Stale(Value),
    /// The Ibis plugin already gives this app the server.
    Plugin,
    /// Another `ibis` entry: shown, and replaced only on a yes.
    Different(Value),
    /// The file can't be changed safely; the reason says why.
    Blocked(String),
}

/// A server entry that runs `ibis mcp`, wherever `ibis` lives.
fn runs_ibis(entry: &Value) -> bool {
    let program = entry["command"].as_str().map(Path::new).and_then(Path::file_stem);
    program.is_some_and(|stem| stem.eq_ignore_ascii_case(COMMAND)) && entry["args"].get(0) == Some(&json!("mcp"))
}

/// The entry setup writes, from any `ibis`: nothing added that removal would lose.
fn ours(entry: &Value) -> bool {
    runs_ibis(entry)
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
            let servers = match file(home).map(fs::read_to_string) {
                None => Value::Null,
                Some(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => Value::Null,
                Some(Err(_)) => return Found::Blocked("its settings file can't be read".into()),
                Some(Ok(text)) => match read(&text) {
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
            let needs_path = home.ibis.is_some()
                && matches!(client.store, Store::Json { absolute: true, .. })
                && entry["command"]
                    .as_str()
                    .map(Path::new)
                    .is_none_or(|path| !path.is_absolute() || !executable(path));
            if needs_path {
                Found::Stale(entry.clone())
            } else {
                Found::Connected
            }
        }
        Some(entry) => Found::Different(entry.clone()),
    }
}

/// One app's line in a report.
struct Line {
    client: &'static Client,
    outcome: &'static str,
    detail: String,
}

pub(crate) fn run<R: BufRead, E: Write>(
    matches: &ArgMatches,
    input: &mut R,
    errors: &mut E,
    interactive: bool,
) -> Result<DispatchResult, Failure> {
    let home =
        Home::here().ok_or_else(|| Failure::new(ErrorCode::SourceUnavailable, "no home folder is known here"))?;
    run_in(&home, matches, input, errors, interactive)
}

fn run_in<R: BufRead, E: Write>(
    home: &Home,
    matches: &ArgMatches,
    input: &mut R,
    errors: &mut E,
    interactive: bool,
) -> Result<DispatchResult, Failure> {
    let (action, args) = matches.subcommand().unwrap_or(("status", matches));
    let chosen: Vec<&String> = args.get_many::<String>("client").into_iter().flatten().collect();
    let clients: Vec<&'static Client> = CLIENTS
        .iter()
        .filter(|client| {
            if chosen.is_empty() {
                (client.found)(home)
            } else {
                chosen.contains(&&client.id.to_owned())
            }
        })
        .collect();
    let flag = |name: &str| args.try_get_one::<bool>(name).ok().flatten().copied().unwrap_or(false);
    let (dry_run, yes) = (flag("dry-run"), flag("yes"));
    let mut lines = vec![];
    match action {
        "status" => {
            for client in &clients {
                let (outcome, detail) = match (state(client, home), client.store) {
                    (_, Store::ByHand { steps }) => ("by-hand", steps.to_owned()),
                    (Found::Connected, _) => ("connected", String::new()),
                    (Found::Plugin, _) => ("connected", "through the Ibis plugin".into()),
                    (Found::Missing, _) => ("not-connected", "run: ibis agents setup".into()),
                    (Found::Stale(_), _) => (
                        "stale",
                        "its ibis command needs a usable full path; run: ibis agents setup".into(),
                    ),
                    (Found::Different(entry), _) => ("different", format!("its ibis entry is {entry}")),
                    (Found::Blocked(reason), _) => ("blocked", reason),
                };
                lines.push(Line {
                    client,
                    outcome,
                    detail,
                });
            }
        }
        "setup" | "remove" => {
            let setup = action == "setup";
            let plan: Vec<(&'static Client, Found)> =
                clients.iter().map(|client| (*client, state(client, home))).collect();
            let changes = plan
                .iter()
                .filter(|(client, found)| match found {
                    Found::Missing | Found::Different(_) => setup && !matches!(client.store, Store::ByHand { .. }),
                    Found::Connected => !setup,
                    Found::Stale(_) => true,
                    _ => false,
                })
                .count();
            let go = dry_run
                || changes == 0
                || yes
                || (interactive
                    && ask(
                        input,
                        errors,
                        &format!(
                            "{} Ibis {} {changes} app{}. Continue?",
                            if setup { "Connect" } else { "Disconnect" },
                            if setup { "to" } else { "from" },
                            if changes == 1 { "" } else { "s" }
                        ),
                    ));
            if !go && !interactive {
                return Err(Failure::new(
                    ErrorCode::Usage,
                    "nothing was changed. Add --yes to change other apps' settings without being asked, or --dry-run to see the plan.",
                ));
            }
            for (client, found) in plan {
                let (outcome, detail) = if setup {
                    connect(
                        client,
                        found,
                        home,
                        go && !dry_run,
                        dry_run,
                        flag("replace"),
                        interactive,
                        input,
                        errors,
                    )
                } else {
                    disconnect(client, found, home, go && !dry_run, dry_run)
                };
                lines.push(Line {
                    client,
                    outcome,
                    detail,
                });
            }
        }
        _ => return Err(Failure::new(ErrorCode::Usage, "unknown agents command")),
    }
    Ok(report(action, &lines, home, dry_run))
}

fn ask<R: BufRead, E: Write>(input: &mut R, errors: &mut E, question: &str) -> bool {
    let _ = write!(errors, "{question} [y/N] ");
    let _ = errors.flush();
    let mut answer = String::new();
    input.read_line(&mut answer).is_ok() && matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

#[allow(clippy::too_many_arguments)]
fn connect<R: BufRead, E: Write>(
    client: &'static Client,
    found: Found,
    home: &Home,
    write: bool,
    dry_run: bool,
    replace: bool,
    interactive: bool,
    input: &mut R,
    errors: &mut E,
) -> (&'static str, String) {
    let planned = found.clone();
    let replacing = match found {
        Found::Connected => return ("connected", "already connected".into()),
        Found::Plugin => {
            return (
                "connected",
                "the Ibis plugin already connects it; nothing added, so it isn't there twice".into(),
            );
        }
        Found::Blocked(reason) => return ("blocked", format!("{reason}. Set it up by hand: {GUIDE}")),
        Found::Missing => None,
        Found::Different(old) | Found::Stale(old) => Some(old),
    };
    if let Store::ByHand { steps } = client.store {
        return ("by-hand", steps.to_owned());
    }
    let wanted = match client.store {
        Store::Json { typed, absolute, .. } => entry(typed, &home.command(absolute)),
        _ => entry(false, COMMAND),
    };
    let diff = replacing
        .as_ref()
        .map(|old| format!("- \"ibis\": {old}\n+ \"ibis\": {wanted}"));
    if dry_run {
        return match diff {
            Some(diff) => ("would-replace", diff),
            None => ("would-connect", String::new()),
        };
    }
    if !write {
        return ("unchanged", String::new());
    }
    if let Some(diff) = &diff
        && matches!(planned, Found::Different(_))
    {
        let agreed = replace
            || (interactive
                && ask(
                    input,
                    errors,
                    &format!("{} has a different ibis entry:\n{diff}\nReplace it?", client.name),
                ));
        if !agreed {
            return (
                "kept",
                format!("its own ibis entry was kept; replace it with --replace\n{diff}"),
            );
        }
    }
    if state(client, home) != planned {
        return ("failed", CHANGED.into());
    }
    let done = match client.store {
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
        (Ok(()), Some(_)) => ("replaced", diff.unwrap_or_default()),
        (Ok(()), None) => ("connected", "added; start a new session in the app".into()),
        (Err(reason), _) => ("failed", format!("{reason}. Set it up by hand: {GUIDE}")),
    }
}

fn disconnect(
    client: &'static Client,
    found: Found,
    home: &Home,
    write: bool,
    dry_run: bool,
) -> (&'static str, String) {
    let planned = found.clone();
    match found {
        Found::Missing => return ("not-connected", String::new()),
        Found::Plugin => {
            return (
                "kept",
                "connected through the Ibis plugin; remove it with: claude plugin uninstall ibis@ibiscite".into(),
            );
        }
        Found::Different(old) => {
            return (
                "kept",
                format!("its ibis entry is not the one setup adds, so it was kept: {old}"),
            );
        }
        Found::Blocked(reason) => return ("blocked", reason),
        Found::Connected | Found::Stale(_) => {}
    }
    if let Store::ByHand { steps } = client.store {
        return ("by-hand", steps.to_owned());
    }
    if dry_run {
        return ("would-remove", String::new());
    }
    if !write {
        return ("unchanged", String::new());
    }
    if state(client, home) != planned {
        return ("failed", CHANGED.into());
    }
    let done = match client.store {
        Store::Json { file, key, .. } => file(home)
            .ok_or_else(|| "its settings folder is unknown here".to_owned())
            .and_then(|path| edit_json(&path, key, None)),
        Store::Program { program, remove, .. } => run_program(home, program, remove),
        Store::ByHand { .. } => Ok(()),
    };
    match done {
        Ok(()) => ("removed", String::new()),
        Err(reason) => ("failed", reason),
    }
}

fn run_program(home: &Home, program: &str, args: &[&str]) -> Result<(), String> {
    let path = home
        .find(program)
        .ok_or_else(|| format!("the {program} command isn't on PATH"))?;
    let status = std::process::Command::new(path)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|_| format!("{program} could not be started"))?;
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
    match &entry {
        Some(entry) => {
            let raw = RawValue::from_string(entry.clone()).map_err(|_| "the entry could not be written")?;
            servers.insert(COMMAND.to_owned(), raw);
        }
        None => {
            servers.shift_remove(COMMAND);
        }
    }
    let raw = RawValue::from_string(serde_json::to_string(&servers).map_err(|_| "the entry could not be written")?)
        .map_err(|_| "the entry could not be written")?;
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
    let staged = ibis_core::atomic_file::AtomicFile::new(path, written.as_bytes())
        .map_err(|_| "the file could not be written".to_owned())?;
    let backup = if before.is_some() {
        let message = "a backup of the file could not be written; nothing was changed";
        let (backup, mut file) = ibis_core::atomic_file::create_numbered(path, ".ibis-backup").map_err(|_| message)?;
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
    // Another program's save wins, including one during staging or backup.
    // read_file also refuses a newly substituted link. A noncooperating app
    // can still save in the tiny window between this check and the rename.
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
    if before.is_some() {
        staged.replace()
    } else {
        staged.create()
    }
    .map_err(|_| "the file could not be written".to_owned())
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

fn report(action: &str, lines: &[Line], home: &Home, dry_run: bool) -> DispatchResult {
    let mut text = String::new();
    let on_path = home.on_path(COMMAND);
    if lines.is_empty() {
        text.push_str(&format!(
            "No supported AI app was found here. Set one up by hand: {GUIDE}\n"
        ));
    }
    for line in lines {
        let detail = if line.detail.is_empty() {
            String::new()
        } else {
            format!(": {}", line.detail)
        };
        text.push_str(&format!(
            "{:<16} {}{detail}\n",
            line.client.name,
            line.outcome.replace('-', " ")
        ));
    }
    if !on_path && action != "remove" {
        text.push_str("\nThe ibis command isn't on PATH, so apps can't start it. Install it first: ");
        text.push_str(GUIDE);
        text.push('\n');
    }
    match action {
        "setup" if dry_run => text.push_str("\nNothing was changed (--dry-run).\n"),
        "setup"
            if lines
                .iter()
                .any(|line| matches!(line.outcome, "connected" | "replaced")) =>
        {
            text.push_str("\nRestart each app, or start a new session, to use Ibis. To undo: ibis agents remove\n");
        }
        "remove" if dry_run => text.push_str("\nNothing was changed (--dry-run).\n"),
        _ => {}
    }
    let problem = lines
        .iter()
        .any(|line| matches!(line.outcome, "failed" | "blocked" | "different" | "stale" | "kept"));
    DispatchResult {
        data: json!({
            "action": action,
            "dry_run": dry_run,
            "ibis_on_path": on_path,
            "server": {"command": COMMAND, "args": ARGS},
            "apps": lines.iter().map(|line| json!({
                "id": line.client.id, "name": line.client.name, "outcome": line.outcome, "detail": line.detail,
            })).collect::<Vec<_>>(),
        }),
        text: Some(text.trim_end().to_owned()),
        source_findings: false,
        exit_code: if problem { ExitCode::Findings } else { ExitCode::Ok },
    }
}

#[cfg(test)]
#[path = "agents_tests.rs"]
mod tests;
