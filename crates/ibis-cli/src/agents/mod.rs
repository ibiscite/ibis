//! Local assistant connections, shared by the CLI and the future Settings UI.
//! Planning is read-only. Applying a plan rechecks every input before writing.
mod adapters;
pub(crate) mod cli;
mod files;
mod json;
mod readiness;

use adapters::Content;
use files::{Snapshot, Target};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fmt,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

pub use readiness::readiness;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
pub struct Error(String);
impl Error {
    fn at(path: &Path, message: &str) -> Self {
        Self(format!("{}: {message}", path.display()))
    }
    fn io(path: &Path, error: std::io::Error) -> Self {
        Self::at(path, &format!("cannot safely access this file ({})", error.kind()))
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Client {
    ClaudeCode,
    ClaudeDesktop,
    Codex,
    ChatgptDesktop,
    Cursor,
    Vscode,
    Gemini,
}

impl Client {
    pub const ALL: [Self; 7] = [
        Self::ClaudeCode,
        Self::ClaudeDesktop,
        Self::Codex,
        Self::ChatgptDesktop,
        Self::Cursor,
        Self::Vscode,
        Self::Gemini,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::ClaudeDesktop => "claude-desktop",
            Self::Codex => "codex",
            Self::ChatgptDesktop => "chatgpt-desktop",
            Self::Cursor => "cursor",
            Self::Vscode => "vscode",
            Self::Gemini => "gemini",
        }
    }
    fn group(self) -> &'static str {
        if self == Self::ChatgptDesktop {
            "codex"
        } else {
            self.name()
        }
    }
    fn pending(self) -> Option<&'static str> {
        match self {
            Self::ClaudeCode => Some("restart Claude Code to finish loading the Ibis plugin"),
            Self::Codex | Self::ChatgptDesktop => {
                Some("finish installing Ibis in the app's Plugins screen, then start a new session")
            }
            Self::ClaudeDesktop => {
                Some("confirm the Ibis extension in Claude Desktop; the citation skill is not available there")
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Context {
    home: PathBuf,
    data: PathBuf,
    executable: PathBuf,
    claude: PathBuf,
    codex: PathBuf,
    copilot: PathBuf,
    system_discovery: bool,
}

impl Context {
    /// Explicit roots let the desktop use the same behavior and tests use a
    /// temporary home without mutating process-wide environment variables.
    pub fn new(home: PathBuf, data: PathBuf, executable: PathBuf) -> Result<Self> {
        let original_home = home;
        let home = original_home.canonicalize().map_err(|e| Error::io(&original_home, e))?;
        let data = if let Ok(relative) = data.strip_prefix(&original_home) {
            home.join(relative)
        } else {
            data
        };
        if !data.starts_with(&home) {
            return Err(Error::at(
                &data,
                "Ibis data directory must be inside the home directory",
            ));
        }
        let executable = executable.canonicalize().map_err(|e| Error::io(&executable, e))?;
        if !executable.is_absolute() || !executable.is_file() {
            return Err(Error::at(&executable, "Ibis executable is missing"));
        }
        Ok(Self {
            claude: home.join(".claude"),
            codex: home.join(".codex"),
            copilot: home.join(".copilot"),
            home,
            data: data.join("agents"),
            executable,
            system_discovery: false,
        })
    }
    pub fn from_environment() -> Result<Self> {
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| Error("cannot find the home directory".into()))?;
        let data = if cfg!(target_os = "macos") {
            home.join("Library/Application Support/com.ibiscite.ibis")
        } else if cfg!(windows) {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("AppData/Local"))
                .join("Ibis")
        } else {
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home.join(".local/share"))
                .join("ibis")
        };
        let mut context = Self::new(
            home,
            data,
            std::env::current_exe().map_err(|_| Error("cannot locate the running ibis".into()))?,
        )?;
        for (name, root) in [
            ("CLAUDE_CONFIG_DIR", &mut context.claude),
            ("CODEX_HOME", &mut context.codex),
            ("COPILOT_HOME", &mut context.copilot),
        ] {
            if let Some(path) = std::env::var_os(name).filter(|s| !s.is_empty()).map(PathBuf::from) {
                *root = path;
            }
        }
        context.system_discovery = true;
        Ok(context)
    }
    fn target(&self, path: PathBuf) -> Target {
        Target {
            home: self.home.clone(),
            path,
        }
    }
    fn group_directory(&self, client: Client) -> PathBuf {
        self.data.join(client.group())
    }
    fn plugin_version(&self) -> String {
        let mut hash = std::hash::DefaultHasher::new();
        self.executable.hash(&mut hash);
        adapters::SKILL.hash(&mut hash);
        ibis_core::VERSION.hash(&mut hash);
        format!("{}+ibis.{:016x}", ibis_core::VERSION, hash.finish())
    }
    fn receipt_path(&self, client: Client) -> PathBuf {
        self.data.join(format!("{}.json", client.group()))
    }
    fn configuration_home(&self, client: Client) -> PathBuf {
        if matches!(client, Client::Codex | Client::ChatgptDesktop) {
            self.codex.clone()
        } else {
            self.client_directory(client)
        }
    }
    pub fn client_directory(&self, client: Client) -> PathBuf {
        match client {
            Client::ClaudeCode => self.claude.clone(),
            Client::Codex | Client::ChatgptDesktop => self.home.join(".agents"),
            Client::Cursor => self.home.join(".cursor"),
            Client::Vscode => self.copilot.clone(),
            Client::Gemini => self.home.join(".gemini"),
            Client::ClaudeDesktop => self.group_directory(client),
        }
    }

    fn receipt(&self, client: Client) -> Result<(Target, Snapshot, Receipt)> {
        let target = self.target(self.receipt_path(client));
        let snapshot = target.read()?;
        let receipt = snapshot
            .bytes
            .as_ref()
            .map(|bytes| {
                serde_json::from_slice::<Receipt>(bytes).map_err(|_| {
                    Error::at(
                        &target.path,
                        "invalid Ibis ownership record; no client settings were changed",
                    )
                })
            })
            .transpose()?
            .unwrap_or_default();
        Ok((target, snapshot, receipt))
    }

    fn gemini_settings(&self) -> Result<(ReadDependency, Option<Value>)> {
        let target = self.target(self.home.join(".gemini/settings.json"));
        let before = target.read()?;
        let text = std::str::from_utf8(before.bytes.as_deref().unwrap_or(b"{}")).map_err(|_| {
            Error::at(
                &target.path,
                "settings are not valid JSON text; leaving the file untouched",
            )
        })?;
        let server = json::get(text, &["mcpServers".into(), "ibis".into()], true).map_err(|e| {
            Error::at(
                &target.path,
                &format!("cannot read JSON ({e}); leaving the file untouched"),
            )
        })?;
        Ok((
            ReadDependency {
                target,
                before,
                recheck: true,
            },
            server,
        ))
    }

    /// Produce exactly the same plan for a terminal, dry run or Settings button.
    pub fn setup(&self, clients: &[Client]) -> Result<Plan> {
        self.plan(clients, false)
    }
    pub fn remove(&self, clients: &[Client]) -> Result<Plan> {
        self.plan(clients, true)
    }

    fn plan(&self, clients: &[Client], remove: bool) -> Result<Plan> {
        let mut grouped: BTreeMap<&str, Vec<Client>> = BTreeMap::new();
        for &client in clients {
            let selected = grouped.entry(client.group()).or_default();
            if !selected.contains(&client) {
                selected.push(client);
            }
        }
        let mut plan = Plan {
            changes: vec![],
            journals: vec![],
            finished: vec![],
            actions: vec![],
            confirmations: vec![],
            messages: vec![],
            reads: vec![],
        };
        for selected in grouped.values() {
            let client = selected[0];
            let configuration = self.configuration_home(client);
            if !configuration.is_absolute() || !configuration.starts_with(&self.home) {
                return Err(Error::at(
                    &configuration,
                    "client settings must be inside the home directory",
                ));
            }
            if !remove && client == Client::Gemini {
                let (read, server) = self.gemini_settings()?;
                if server.is_some() {
                    return Err(Error::at(
                        &read.target.path,
                        "an ibis server here overrides extensions and is managed outside this setup; leaving it untouched",
                    ));
                }
                plan.reads.push(read);
            }
            let (receipt_target, receipt_snapshot, mut receipt) = self.receipt(client)?;
            if !receipt.files.is_empty() && receipt.config_home != self.configuration_home(client) {
                return Err(Error("the client's settings directory changed; remove its Ibis connection using the original directory first".into()));
            }
            if remove {
                receipt.clients.retain(|c| !selected.contains(c));
                if !receipt.clients.is_empty() {
                    let after = adapters::pretty(
                        &serde_json::to_value(&receipt)
                            .map_err(|_| Error("cannot record connection ownership".into()))?,
                    )?;
                    plan.add(receipt_target, receipt_snapshot, Some(after), false);
                    plan.messages
                        .push("Codex and ChatGPT share this connection; it is still in use by the other app.".into());
                    continue;
                }
            } else {
                for &c in selected {
                    if !receipt.clients.contains(&c) {
                        receipt.clients.push(c);
                    }
                }
            }
            if remove
                && matches!(client, Client::Codex | Client::ChatgptDesktop)
                && !receipt.files.is_empty()
                && let Some((target, before, after)) = self.disable_openai_plugin(&receipt, &mut plan.reads)?
            {
                plan.add(target, before, Some(after), true);
            }
            let mut desired = receipt.clone();
            desired.version = ibis_core::VERSION.into();
            desired.command = self.executable.clone();
            desired.config_home = self.configuration_home(client);
            desired.plugin_version = self.plugin_version();
            for spec in self.specs(client)? {
                let id = spec
                    .path
                    .strip_prefix(&self.home)
                    .map_err(|_| Error::at(&spec.path, "settings must be inside the home directory"))?
                    .to_string_lossy()
                    .into_owned();
                let target = self.target(spec.path.clone());
                let before = target.read()?;
                plan.reads.push(ReadDependency {
                    target: target.clone(),
                    before: before.clone(),
                    recheck: false,
                });
                let owned = receipt.files.get(&id);
                if remove && owned.is_none() {
                    continue;
                }
                let (after, ownership) = match &spec.content {
                    Content::File(bytes) => {
                        if let Some(existing) = &before.bytes
                            && !owned.is_some_and(|o| o.accepted.contains(existing))
                        {
                            return Err(Error::at(
                                &spec.path,
                                "this file was not added by Ibis, or has been changed; leaving it untouched",
                            ));
                        }
                        if remove {
                            if owned.is_none() {
                                continue;
                            }
                            (None, None)
                        } else {
                            if matches!(client, Client::ClaudeCode | Client::Codex | Client::ChatgptDesktop)
                                && spec
                                    .path
                                    .file_name()
                                    .is_some_and(|n| n == "mcp.json" || n == ".mcp.json")
                            {
                                let value: Value = serde_json::from_slice(bytes)
                                    .map_err(|_| Error("invalid generated MCP adapter".into()))?;
                                let name = spec
                                    .path
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .ok_or_else(|| Error("invalid MCP filename".into()))?;
                                let history = desired.cached_mcp.entry(name.into()).or_default();
                                if !history.contains(&value) {
                                    history.push(value);
                                }
                            }
                            let mut accepted = owned.map(|o| o.accepted.clone()).unwrap_or_default();
                            accepted.retain(|b| b != bytes);
                            accepted.push(bytes.clone());
                            (
                                Some(bytes.clone()),
                                Some(OwnedFile {
                                    created: true,
                                    accepted,
                                    entries: vec![],
                                }),
                            )
                        }
                    }
                    Content::Json {
                        initial,
                        entries,
                        comments,
                    } => {
                        if remove && owned.is_none() {
                            continue;
                        }
                        let initial = adapters::pretty(initial)?;
                        let bytes = before.bytes.as_ref().unwrap_or(&initial);
                        let mut text = std::str::from_utf8(bytes)
                            .map_err(|_| {
                                Error::at(
                                    &spec.path,
                                    "settings are not valid JSON text; leaving the file untouched",
                                )
                            })?
                            .to_owned();
                        if !remove && matches!(client, Client::Codex | Client::ChatgptDesktop) {
                            let name = json::get(&text, &["name".into()], *comments)
                                .map_err(|e| Error::at(&spec.path, &e))?
                                .and_then(|v| v.as_str().map(str::to_owned))
                                .filter(|name| {
                                    !name.is_empty()
                                        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
                                })
                                .ok_or_else(|| {
                                    Error::at(&spec.path, "the marketplace needs a valid name; leaving it untouched")
                                })?;
                            if !desired.marketplaces.contains(&name) {
                                desired.marketplaces.push(name);
                            }
                        }
                        let mut out = owned.cloned().unwrap_or(OwnedFile {
                            created: before.bytes.is_none(),
                            accepted: vec![],
                            entries: vec![],
                        });
                        for (path, value) in entries {
                            let current = json::get(&text, path, *comments).map_err(|e| {
                                Error::at(
                                    &spec.path,
                                    &format!("cannot read JSON ({e}); leaving the file untouched"),
                                )
                            })?;
                            let old = owned.and_then(|o| o.entries.iter().find(|e| &e.path == path));
                            if current
                                .as_ref()
                                .is_some_and(|current| !old.is_some_and(|o| o.accepted.contains(current)))
                            {
                                return Err(Error::at(
                                    &spec.path,
                                    "the Ibis entry already exists and is not owned by this installation, or has been changed; leaving it untouched",
                                ));
                            }
                            if remove {
                                if let Some(old) = old {
                                    // Generated Claude plugins default to disabled;
                                    // removing our enablement also revokes cached copies.
                                    text = json::set(&text, path, None, *comments)
                                        .map_err(|e| Error::at(&spec.path, &e))?;
                                    for container in old.containers.iter().rev() {
                                        if container.is_empty()
                                            || container.len() >= path.len()
                                            || !path.starts_with(container)
                                        {
                                            return Err(Error::at(
                                                &spec.path,
                                                "invalid Ibis ownership record; no client settings were changed",
                                            ));
                                        }
                                        if json::removable_empty(&text, container, *comments)
                                            .map_err(|e| Error::at(&spec.path, &e))?
                                        {
                                            text = json::set(&text, container, None, *comments)
                                                .map_err(|e| Error::at(&spec.path, &e))?;
                                        }
                                    }
                                }
                            } else {
                                let containers = old.map(|o| o.containers.clone()).unwrap_or(
                                    json::missing(&text, path, *comments).map_err(|e| Error::at(&spec.path, &e))?,
                                );
                                let mut accepted = old.map(|o| o.accepted.clone()).unwrap_or_default();
                                accepted.retain(|v| v != value);
                                accepted.push(value.clone());
                                out.entries.retain(|e| &e.path != path);
                                out.entries.push(OwnedEntry {
                                    path: path.clone(),
                                    accepted,
                                    containers,
                                });
                                if current.as_ref() != Some(value) {
                                    text = json::set(&text, path, Some(value), *comments)
                                        .map_err(|e| Error::at(&spec.path, &e))?;
                                }
                            }
                        }
                        let empty = text.chars().filter(|c| !c.is_whitespace()).collect::<String>() == "{}";
                        // A newly created marketplace has its own name and plugins
                        // scaffold. Remove it only while that scaffold is unchanged.
                        let empty_marketplace = remove
                            && matches!(client, Client::Codex | Client::ChatgptDesktop)
                            && text.as_bytes() == initial;
                        let after = if remove && out.created && (empty || empty_marketplace) {
                            None
                        } else {
                            Some(text.into_bytes())
                        };
                        (after, if remove { None } else { Some(out) })
                    }
                };
                if let Some(ownership) = ownership {
                    desired.files.insert(id.clone(), ownership);
                } else {
                    desired.files.remove(&id);
                }
                plan.add(target, before, after, owned.is_none_or(|o| !o.created));
            }
            if remove {
                // Ownership survives a partial removal, until every target has
                // been removed successfully. A subsequent remove can resume.
                plan.add(receipt_target, receipt_snapshot, None, false);
                if client == Client::ClaudeDesktop && !receipt.files.is_empty() {
                    plan.messages.push(
                        "claude-desktop: uninstall the Ibis extension in Claude Desktop to finish disconnecting."
                            .into(),
                    );
                }
            } else {
                let after = adapters::pretty(
                    &serde_json::to_value(&desired).map_err(|_| Error("cannot record connection ownership".into()))?,
                )?;
                if receipt_snapshot.bytes.as_ref() != Some(&after) {
                    plan.actions.push(Action {
                        path: receipt_target.path.clone(),
                        operation: "record ownership".into(),
                    });
                    plan.journals.push(Change {
                        target: receipt_target.clone(),
                        before: receipt_snapshot.clone(),
                        after: Some(after.clone()),
                        backup: false,
                    });
                    // During publication either old or new bytes are ours. Once
                    // complete, retain only the new identity, so later user edits
                    // cannot revive an old ownership claim.
                }
                for file in desired.files.values_mut() {
                    if let Some(last) = file.accepted.pop() {
                        file.accepted = vec![last];
                    }
                    for entry in &mut file.entries {
                        if let Some(last) = entry.accepted.pop() {
                            entry.accepted = vec![last];
                        }
                    }
                }
                let compact = adapters::pretty(
                    &serde_json::to_value(&desired).map_err(|_| Error("cannot record connection ownership".into()))?,
                )?;
                if compact != after {
                    if receipt_snapshot.bytes.as_ref() == Some(&after) {
                        plan.actions.push(Action {
                            path: receipt_target.path.clone(),
                            operation: "finish ownership record".into(),
                        });
                    }
                    plan.finished.push(Finish {
                        target: receipt_target,
                        expected: after.clone(),
                        after: compact,
                        before: (receipt_snapshot.bytes.as_ref() == Some(&after)).then_some(receipt_snapshot),
                    });
                }
                if let Some(message) = client.pending() {
                    plan.messages.push(format!("{}: {message}", client.name()));
                }
                if client == Client::ClaudeDesktop {
                    plan.confirmations.push(self.group_directory(client).join("ibis.mcpb"));
                }
            }
        }
        Ok(plan)
    }

    pub fn found(&self, client: Client) -> bool {
        let directory = match client {
            Client::Codex => self.codex.clone(),
            Client::ChatgptDesktop => self.home.join("Library/Application Support/com.openai.chat"),
            Client::ClaudeDesktop => self.home.join("Library/Application Support/Claude"),
            _ => self.client_directory(client),
        };
        if directory.is_dir() {
            return true;
        }
        let binary = match client {
            Client::ClaudeCode => Some("claude"),
            Client::Codex => Some("codex"),
            Client::Gemini => Some("gemini"),
            Client::Cursor => Some("cursor"),
            Client::Vscode => Some("code"),
            _ => None,
        };
        if self.system_discovery
            && let Some(binary) = binary
        {
            let name = if cfg!(windows) {
                format!("{binary}.exe")
            } else {
                binary.into()
            };
            if std::env::var_os("PATH")
                .is_some_and(|path| std::env::split_paths(&path).any(|p| p.is_absolute() && p.join(&name).is_file()))
            {
                return true;
            }
        }
        let app = match client {
            Client::ClaudeCode | Client::ClaudeDesktop => "Claude.app",
            Client::Codex => "Codex.app",
            Client::ChatgptDesktop => "ChatGPT.app",
            Client::Cursor => "Cursor.app",
            Client::Vscode => "Visual Studio Code.app",
            Client::Gemini => return false,
        };
        self.home.join("Applications").join(app).is_dir()
            || (self.system_discovery && Path::new("/Applications").join(app).is_dir())
    }

    pub fn status(&self) -> Result<Vec<ClientStatus>> {
        Ok(Client::ALL
            .into_iter()
            .map(|client| {
                self.client_status(client).unwrap_or_else(|_| ClientStatus {
                    client,
                    found: self.found(client),
                    state: Connection::Unavailable,
                    connected: false,
                    command: None,
                    message: format!(
                        "connection could not be checked; run ibis agents setup --client {} for the reason",
                        client.name()
                    ),
                })
            })
            .collect())
    }

    fn client_status(&self, client: Client) -> Result<ClientStatus> {
        let found = self.found(client);
        if client == Client::Gemini
            && let Some(server) = self.gemini_settings()?.1
        {
            let command = server_command(&serde_json::json!({"mcpServers":{"ibis":server}}));
            let old = command.as_ref().is_some_and(|c| !c.is_file() || *c != self.executable);
            let connected = command.is_some() && !old;
            return Ok(ClientStatus {
                client,
                found,
                connected,
                command,
                state: if connected {
                    Connection::Connected
                } else if old {
                    Connection::OldOrMissing
                } else {
                    Connection::Changed
                },
                message: if connected {
                    "connected; this connection is managed outside ibis agents setup"
                } else if old {
                    "connected to an old or missing ibis; this connection is managed outside ibis agents setup"
                } else {
                    "found; Gemini's own ibis server overrides the extension; check Gemini settings"
                }
                .into(),
            });
        }
        let (_, _, receipt) = self.receipt(client)?;
        if !receipt.files.is_empty() && receipt.config_home != self.configuration_home(client) {
            return Err(Error("client settings directory changed".into()));
        }
        let mut connected = !receipt.files.is_empty();
        let mut changed = false;
        let mut actual_command = None;
        for spec in self.specs(client)? {
            let snapshot = self.target(spec.path.clone()).read()?;
            if let Some(bytes) = &snapshot.bytes
                && let Ok(value) = serde_json::from_slice::<Value>(bytes)
                && let Some(command) = server_command(&value)
            {
                actual_command = Some(command);
            }
            if let Content::Json { comments, entries, .. } = &spec.content
                && entries.iter().any(|(path, _)| path == &["mcpServers", "ibis"])
            {
                let text = std::str::from_utf8(snapshot.bytes.as_deref().unwrap_or(b"{}"))
                    .map_err(|_| Error::at(&spec.path, "invalid JSON text"))?;
                if let Some(server) = json::get(text, &["mcpServers".into(), "ibis".into()], *comments)
                    .map_err(|e| Error::at(&spec.path, &e))?
                {
                    actual_command = server_command(&serde_json::json!({"mcpServers":{"ibis":server}}));
                }
            }
            let id = spec
                .path
                .strip_prefix(&self.home)
                .map_err(|_| Error::at(&spec.path, "invalid client location"))?
                .to_string_lossy();
            let Some(owned) = receipt.files.get(id.as_ref()) else {
                connected = false;
                continue;
            };
            match spec.content {
                Content::File(_) => {
                    if !snapshot.bytes.as_ref().is_some_and(|b| owned.accepted.contains(b)) {
                        connected = false;
                        changed = snapshot.bytes.is_some();
                    }
                }
                Content::Json { comments, .. } => {
                    let text = std::str::from_utf8(snapshot.bytes.as_deref().unwrap_or(b"{}"))
                        .map_err(|_| Error::at(&spec.path, "invalid JSON text"))?;
                    for entry in &owned.entries {
                        let current = json::get(text, &entry.path, comments).map_err(|e| Error::at(&spec.path, &e))?;
                        if !current.as_ref().is_some_and(|v| entry.accepted.contains(v)) {
                            connected = false;
                            changed |= current.is_some();
                        }
                    }
                }
            }
        }
        let unmanaged = receipt.files.is_empty()
            && actual_command.is_some()
            && matches!(client, Client::Cursor | Client::Vscode | Client::Gemini);
        connected |= unmanaged;
        let mut command =
            actual_command.or_else(|| (!receipt.command.as_os_str().is_empty()).then(|| receipt.command.clone()));
        let mut old = command
            .as_ref()
            .is_some_and(|c| !c.is_absolute() || !c.is_file() || *c != self.executable)
            || (!receipt.version.is_empty() && receipt.version != ibis_core::VERSION);
        let mut installed = client.pending().is_none();
        if connected && matches!(client, Client::Codex | Client::ChatgptDesktop) {
            let (verified, outdated, cached_command) = self.openai_installed(&receipt)?;
            installed = verified;
            old |= outdated;
            if cached_command.is_some() {
                command = cached_command;
            }
        }
        if connected && client == Client::ClaudeCode {
            let cache = self
                .claude
                .join("plugins/cache/ibiscite-local/ibis")
                .join(self.plugin_version());
            let (verified, outdated) = self.verify_cache(client, &cache)?;
            installed = verified;
            old |= outdated;
        }
        let shared =
            connected && !receipt.clients.contains(&client) && matches!(client, Client::Codex | Client::ChatgptDesktop);
        let message = if shared {
            "a shared Ibis connection remains for the other app; remove it there too to disconnect".into()
        } else if old && connected {
            "connected to an old or missing ibis; run ibis agents setup to reconnect".into()
        } else if changed {
            "found, not connected: the Ibis entry or skill was changed; leaving it untouched".into()
        } else if unmanaged {
            "connected; this connection is managed outside ibis agents setup".into()
        } else if connected {
            if installed {
                "connected"
            } else {
                client.pending().unwrap_or("connected")
            }
            .into()
        } else if found {
            if client == Client::ClaudeDesktop {
                "found; check the Ibis extension in Claude Desktop to confirm installation or removal".into()
            } else {
                "found, not connected".into()
            }
        } else {
            "not found".into()
        };
        let state = if shared {
            Connection::Shared
        } else if connected && old {
            Connection::OldOrMissing
        } else if changed {
            Connection::Changed
        } else if connected && installed {
            Connection::Connected
        } else if connected {
            Connection::FinishInApp
        } else if found {
            Connection::NotConnected
        } else {
            Connection::NotFound
        };
        Ok(ClientStatus {
            client,
            found,
            state,
            connected: connected && !old && installed,
            command: if connected { command } else { None },
            message,
        })
    }

    fn verify_cache(&self, client: Client, cache: &Path) -> Result<(bool, bool)> {
        let source = self.group_directory(client).join("plugins/ibis");
        let mut complete = true;
        let mut old = false;
        for spec in self.specs(client)? {
            if let Ok(relative) = spec.path.strip_prefix(&source) {
                let Content::File(expected) = spec.content else {
                    continue;
                };
                let snapshot = self.target(cache.join(relative)).read()?;
                complete &= snapshot.bytes.as_ref() == Some(&expected);
                if let Some(bytes) = snapshot.bytes
                    && let Ok(value) = serde_json::from_slice::<Value>(&bytes)
                    && let Some(command) = server_command(&value)
                {
                    old |= !command.is_file() || command != self.executable;
                }
            }
        }
        Ok((complete, old))
    }

    fn openai_installed(&self, receipt: &Receipt) -> Result<(bool, bool, Option<PathBuf>)> {
        let marketplace = self.target(self.home.join(".agents/plugins/marketplace.json")).read()?;
        let Some(bytes) = marketplace.bytes else {
            return Ok((false, false, None));
        };
        let text =
            std::str::from_utf8(&bytes).map_err(|_| Error("cannot read the personal plugin marketplace".into()))?;
        let name = json::get(text, &["name".into()], false)
            .map_err(Error)?
            .and_then(|v| v.as_str().map(str::to_owned))
            .filter(|name| valid_marketplace(name))
            .ok_or_else(|| Error("the personal plugin marketplace has no valid name".into()))?;
        let mut names = receipt.marketplaces.clone();
        if !names.contains(&name) {
            names.push(name);
        }
        let config = self.target(self.codex.join("config.toml")).read()?;
        let Some(bytes) = config.bytes else {
            return Ok((false, false, None));
        };
        let config = std::str::from_utf8(&bytes)
            .ok()
            .and_then(|s| s.parse::<toml_edit::DocumentMut>().ok())
            .ok_or_else(|| Error("cannot read Codex config.toml; leaving it untouched".into()))?;
        for name in names.iter().rev() {
            if !valid_marketplace(name) {
                return Err(Error("invalid Ibis marketplace ownership record".into()));
            }
            let key = format!("ibis@{name}");
            if config
                .get("plugins")
                .and_then(|p| p.get(&key))
                .and_then(|p| p.get("enabled"))
                .and_then(toml_edit::Item::as_bool)
                != Some(true)
            {
                continue;
            }
            let cache = self.codex.join("plugins/cache").join(name).join("ibis/local");
            for filename in ["mcp.json", ".mcp.json"] {
                if let Some(bytes) = self.target(cache.join(filename)).read()?.bytes
                    && let Ok(value) = serde_json::from_slice::<Value>(&bytes)
                    && let Some(command) = server_command(&value)
                {
                    let old = !command.is_file() || command != self.executable;
                    let (complete, cache_old) = self.verify_cache(Client::Codex, &cache)?;
                    return Ok((complete && !old, old || cache_old, Some(command)));
                }
            }
        }
        Ok((false, false, None))
    }

    fn disable_openai_plugin(
        &self,
        receipt: &Receipt,
        reads: &mut Vec<ReadDependency>,
    ) -> Result<Option<(Target, Snapshot, Vec<u8>)>> {
        let target = self.target(self.codex.join("config.toml"));
        let before = target.read()?;
        let dependency = reads.len();
        reads.push(ReadDependency {
            target: target.clone(),
            before: before.clone(),
            recheck: true,
        });
        let Some(bytes) = &before.bytes else {
            return Ok(None);
        };
        let mut config = std::str::from_utf8(bytes)
            .ok()
            .and_then(|s| s.parse::<toml_edit::DocumentMut>().ok())
            .ok_or_else(|| Error::at(&target.path, "cannot read TOML; leaving the file untouched"))?;
        let mut changed = false;
        // A catalog name is unrelated user configuration and may have changed.
        // Revoke the identities under which Ibis was actually registered.
        for name in &receipt.marketplaces {
            if !valid_marketplace(name) {
                return Err(Error("invalid Ibis marketplace ownership record".into()));
            }
            let key = format!("ibis@{name}");
            if config
                .get("plugins")
                .and_then(|p| p.get(&key))
                .and_then(|p| p.get("enabled"))
                .and_then(toml_edit::Item::as_bool)
                != Some(true)
            {
                continue;
            }
            let cache = self.codex.join("plugins/cache").join(name).join("ibis/local");
            if !self.cached_mcp_owned(receipt, Client::Codex, &cache, reads)? {
                return Err(Error::at(
                    &target.path,
                    "the installed Ibis plugin could not be verified; remove it in the app's Plugins screen",
                ));
            }
            if let Some(enabled) = config
                .get_mut("plugins")
                .and_then(|p| p.get_mut(&key))
                .and_then(|p| p.get_mut("enabled"))
            {
                let decor = enabled.as_value().map(|v| v.decor().clone());
                *enabled = toml_edit::value(false);
                if let (Some(decor), Some(value)) = (decor, enabled.as_value_mut()) {
                    *value.decor_mut() = decor;
                }
                changed = true;
            }
        }
        if !changed {
            return Ok(None);
        }
        // The write target rechecks its own snapshot; afterwards our mutation
        // must not invalidate a read guard for the old config contents.
        reads[dependency].recheck = false;
        Ok(Some((target, before, config.to_string().into_bytes())))
    }

    fn cached_mcp_owned(
        &self,
        receipt: &Receipt,
        client: Client,
        cache: &Path,
        reads: &mut Vec<ReadDependency>,
    ) -> Result<bool> {
        let mut found = false;
        for filename in ["mcp.json", ".mcp.json"] {
            let path = cache.join(filename);
            let target = self.target(path.clone());
            let before = target.read()?;
            reads.push(ReadDependency {
                target,
                before: before.clone(),
                recheck: true,
            });
            if let Some(bytes) = before.bytes {
                let source = self.group_directory(client).join("plugins/ibis").join(filename);
                let id = source
                    .strip_prefix(&self.home)
                    .map_err(|_| Error("invalid plugin location".into()))?
                    .to_string_lossy();
                let historical = std::str::from_utf8(&bytes)
                    .ok()
                    .and_then(|text| json::get(text, &[], false).ok().flatten())
                    .is_some_and(|value| {
                        receipt
                            .cached_mcp
                            .get(filename)
                            .is_some_and(|history| history.contains(&value))
                    });
                if !historical
                    && !receipt
                        .files
                        .get(id.as_ref())
                        .is_some_and(|file| file.accepted.contains(&bytes))
                {
                    return Err(Error::at(
                        &path,
                        "the installed Ibis plugin was changed; remove it in the app's Plugins screen",
                    ));
                }
                found = true;
            }
        }
        Ok(found)
    }
}

fn valid_marketplace(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
}

fn server_command(value: &Value) -> Option<PathBuf> {
    let server = value
        .pointer("/mcpServers/ibis")
        .or_else(|| value.pointer("/server/mcp_config"))?;
    if server["args"] != serde_json::json!(["mcp", "--app-libraries"]) {
        return None;
    }
    server["command"].as_str().map(PathBuf::from)
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Connection {
    NotFound,
    NotConnected,
    Connected,
    OldOrMissing,
    FinishInApp,
    Changed,
    Shared,
    Unavailable,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientStatus {
    pub client: Client,
    pub found: bool,
    pub state: Connection,
    pub connected: bool,
    #[serde(skip)]
    pub command: Option<PathBuf>,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Receipt {
    version: String,
    command: PathBuf,
    config_home: PathBuf,
    plugin_version: String,
    cached_mcp: BTreeMap<String, Vec<Value>>,
    marketplaces: Vec<String>,
    clients: Vec<Client>,
    files: BTreeMap<String, OwnedFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OwnedFile {
    created: bool,
    accepted: Vec<Vec<u8>>,
    entries: Vec<OwnedEntry>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OwnedEntry {
    path: Vec<String>,
    accepted: Vec<Value>,
    containers: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Action {
    pub path: PathBuf,
    pub operation: String,
}
#[derive(Debug)]
struct Change {
    target: Target,
    before: Snapshot,
    after: Option<Vec<u8>>,
    backup: bool,
}
#[derive(Debug)]
struct Finish {
    target: Target,
    expected: Vec<u8>,
    after: Vec<u8>,
    before: Option<Snapshot>,
}
#[derive(Debug)]
struct ReadDependency {
    target: Target,
    before: Snapshot,
    recheck: bool,
}

#[derive(Debug)]
pub struct Plan {
    changes: Vec<Change>,
    journals: Vec<Change>,
    finished: Vec<Finish>,
    actions: Vec<Action>,
    confirmations: Vec<PathBuf>,
    messages: Vec<String>,
    reads: Vec<ReadDependency>,
}

impl Plan {
    fn add(&mut self, target: Target, before: Snapshot, after: Option<Vec<u8>>, backup: bool) {
        if before.bytes == after {
            return;
        }
        self.actions.push(Action {
            path: target.path.clone(),
            operation: if after.is_none() {
                "remove"
            } else if before.bytes.is_none() {
                "create"
            } else {
                "update"
            }
            .into(),
        });
        self.changes.push(Change {
            target,
            before,
            after,
            backup,
        });
    }
    pub fn actions(&self) -> &[Action] {
        &self.actions
    }
    pub fn messages(&self) -> &[String] {
        &self.messages
    }
    pub fn confirmations(&self) -> &[PathBuf] {
        &self.confirmations
    }
    /// Write the ownership journal before publishing registrations. Failed writes
    /// leave it in place so setup/remove can safely resume with either version.
    pub fn apply(self) -> Result<()> {
        for read in &self.reads {
            read.target.check(&read.before)?;
        }
        for change in self.journals.iter().chain(&self.changes) {
            change.target.check(&change.before)?;
        }
        for finish in &self.finished {
            if let Some(before) = &finish.before {
                finish.target.check(before)?;
            }
        }
        for change in self.journals.iter().chain(&self.changes) {
            for read in self.reads.iter().filter(|read| read.recheck) {
                read.target.check(&read.before)?;
            }
            change
                .target
                .write(&change.before, change.after.as_deref(), change.backup)?;
        }
        for finish in &self.finished {
            let before = finish.target.read()?;
            if before.bytes.as_ref() != Some(&finish.expected) {
                return Err(Error::at(&finish.target.path, "ownership record changed; try again"));
            }
            finish.target.write(&before, Some(&finish.after), false)?;
        }
        Ok(())
    }
}

/// Ask the OS to show the client's own extension installation confirmation.
/// This is deliberately separate from apply so the desktop can own the UI step.
pub fn open_confirmation(bundle: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("/usr/bin/open")
        .args(["-b", "com.anthropic.claudefordesktop"])
        .arg(bundle)
        .status();
    #[cfg(target_os = "linux")]
    let status = std::process::Command::new("/usr/bin/xdg-open").arg(bundle).status();
    #[cfg(windows)]
    let status = std::process::Command::new("C:\\Windows\\explorer.exe")
        .arg(bundle)
        .status();
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    return Err(Error::at(
        bundle,
        "open this MCPB in Claude Desktop to finish installing",
    ));
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    if status.is_ok_and(|s| s.success()) {
        Ok(())
    } else {
        Err(Error::at(
            bundle,
            "the extension is ready; open it in Claude Desktop to confirm installation",
        ))
    }
}
