use super::{Client, Context, Error, Result};
use serde_json::{Value, json};
use std::path::PathBuf;

pub(super) const SKILL: &str = include_str!("../../../../plugins/ibis/skills/ibis-cite/SKILL.md");

#[derive(Debug, Clone)]
pub(super) enum Content {
    File(Vec<u8>),
    Json {
        initial: Value,
        entries: Vec<(Vec<String>, Value)>,
        comments: bool,
    },
}

#[derive(Debug, Clone)]
pub(super) struct Spec {
    pub path: PathBuf,
    pub content: Content,
}

fn entry(path: &[&str], value: Value) -> (Vec<String>, Value) {
    (path.iter().map(|s| (*s).into()).collect(), value)
}

impl Context {
    pub(super) fn specs(&self, client: Client) -> Result<Vec<Spec>> {
        let root = self.group_directory(client);
        let command = self
            .executable
            .to_str()
            .ok_or_else(|| Error::at(&self.executable, "executable name is not valid text"))?;
        // Clients interpolate ${...} in commands; an apparently literal filename
        // must never turn into a different command at the security boundary.
        if command.contains("${") {
            return Err(Error::at(
                &self.executable,
                "executable name contains client variable syntax",
            ));
        }
        let server = json!({"type":"stdio", "command":command, "args":["mcp","--app-libraries"]});
        let mut specs = vec![];
        let mut file = |path: PathBuf, content: Vec<u8>| {
            specs.push(Spec {
                path,
                content: Content::File(content),
            })
        };
        if matches!(client, Client::ClaudeCode | Client::Codex | Client::ChatgptDesktop) {
            let plugin = root.join("plugins/ibis");
            for (name, text) in [
                ("plugin.json", include_str!("../../../../plugins/ibis/plugin.json")),
                (
                    ".claude-plugin/plugin.json",
                    include_str!("../../../../plugins/ibis/.claude-plugin/plugin.json"),
                ),
            ] {
                let mut manifest: Value =
                    serde_json::from_str(text).map_err(|_| Error("invalid bundled Ibis plugin manifest".into()))?;
                if client == Client::ClaudeCode {
                    manifest["version"] = json!(self.plugin_version());
                }
                file(plugin.join(name), pretty(&manifest)?);
            }
            file(
                plugin.join(".claude-plugin/icon.png"),
                include_bytes!("../../../../plugins/ibis/.claude-plugin/icon.png").to_vec(),
            );
            file(plugin.join("skills/ibis-cite/SKILL.md"), SKILL.as_bytes().to_vec());
            for (name, template) in [
                (".mcp.json", include_str!("../../../../plugins/ibis/.mcp.json")),
                ("mcp.json", include_str!("../../../../plugins/ibis/mcp.json")),
            ] {
                let mut value: Value =
                    serde_json::from_str(template).map_err(|_| Error("invalid bundled Ibis MCP template".into()))?;
                value["mcpServers"]["ibis"] = server.clone();
                file(plugin.join(name), pretty(&value)?);
            }
            // The marketplace is generated beside its plugin, so no cwd or PATH
            // lookup is needed by either client.
            if client == Client::ClaudeCode {
                file(
                    root.join(".claude-plugin/marketplace.json"),
                    pretty(&json!({
                        "name":"ibiscite-local", "owner":{"name":"Ibis contributors"},
                        "plugins":[{"name":"ibis","source":"./plugins/ibis"}]
                    }))?,
                );
                specs.push(Spec {
                    path: self.client_directory(client).join("settings.json"),
                    content: Content::Json {
                        initial: json!({}),
                        comments: false,
                        entries: vec![
                            entry(
                                &["extraKnownMarketplaces", "ibiscite-local"],
                                json!({"source":{"source":"directory","path":root},"autoUpdate":true}),
                            ),
                            entry(&["enabledPlugins", "ibis@ibiscite-local"], json!(true)),
                        ],
                    },
                });
            } else {
                let relative = plugin
                    .strip_prefix(&self.home)
                    .map_err(|_| Error::at(&plugin, "plugin must be inside the home directory"))?;
                let relative = format!("./{}", relative.to_string_lossy().replace('\\', "/"));
                specs.push(Spec { path: self.home.join(".agents/plugins/marketplace.json"), content: Content::Json {
                    initial: json!({"name":"ibiscite-local", "plugins":[]}), comments: false,
                    entries: vec![entry(&["plugins", "@ibis"], json!({
                        "name":"ibis", "source":{"source":"local","path":relative},
                        "policy":{"installation":"AVAILABLE","authentication":"ON_INSTALL"}, "category":"Productivity"
                    }))],
                }});
            }
        } else if matches!(client, Client::Cursor | Client::Vscode) {
            let mut server = server;
            if client == Client::Vscode {
                server["tools"] = json!(["*"]);
            }
            let directory = self.client_directory(client);
            file(directory.join("skills/ibis-cite/SKILL.md"), SKILL.as_bytes().to_vec());
            specs.push(Spec {
                path: directory.join(if client == Client::Cursor {
                    "mcp.json"
                } else {
                    "mcp-config.json"
                }),
                content: Content::Json {
                    initial: json!({}),
                    comments: client == Client::Vscode,
                    entries: vec![entry(&["mcpServers", "ibis"], server)],
                },
            });
        } else if client == Client::Gemini {
            let mut server = server;
            if let Some(server) = server.as_object_mut() {
                server.remove("type");
            }
            let extension = self.home.join(".gemini/extensions/ibis");
            file(
                extension.join("gemini-extension.json"),
                pretty(&json!({
                    "name":"ibis", "version":ibis_core::VERSION,
                    "description":"Cite from your own library, read-only and offline.",
                    "mcpServers":{"ibis":server}
                }))?,
            );
            file(extension.join("skills/ibis-cite/SKILL.md"), SKILL.as_bytes().to_vec());
        } else {
            let manifest = pretty(&json!({
                "manifest_version":"0.3", "name":"ibis", "display_name":"Ibis",
                "version":ibis_core::VERSION,
                "description":"Cite from your own library, read-only and offline.",
                "author":{"name":"Ibis contributors"}, "license":"AGPL-3.0-or-later",
                "server":{"type":"binary", "entry_point":command, "mcp_config":{"command":command,"args":["mcp","--app-libraries"]}}
            }))?;
            file(root.join("manifest.json"), manifest.clone());
            file(root.join("ibis.mcpb"), bundle(&manifest)?);
        }
        Ok(specs)
    }
}

pub(super) fn pretty(value: &Value) -> Result<Vec<u8>> {
    let mut bytes =
        serde_json::to_vec_pretty(value).map_err(|_| Error("could not generate the Ibis adapter".into()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// MCPB is ZIP. A single stored manifest needs neither compression nor another
/// runtime; its explicit command launches the installed Ibis in place.
fn bundle(manifest: &[u8]) -> Result<Vec<u8>> {
    let name = b"manifest.json";
    let size = u32::try_from(manifest.len()).map_err(|_| Error("MCPB manifest is too large".into()))?;
    let crc = manifest.iter().fold(!0u32, |mut crc, b| {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
        }
        crc
    }) ^ !0;
    let mut out = vec![];
    out.extend(0x04034b50u32.to_le_bytes());
    for n in [20u16, 0, 0, 0, 33] {
        out.extend(n.to_le_bytes());
    }
    for n in [crc, size, size] {
        out.extend(n.to_le_bytes());
    }
    out.extend((name.len() as u16).to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.extend(name);
    out.extend(manifest);
    let offset = out.len() as u32;
    out.extend(0x02014b50u32.to_le_bytes());
    for n in [20u16, 20, 0, 0, 0, 33] {
        out.extend(n.to_le_bytes());
    }
    for n in [crc, size, size] {
        out.extend(n.to_le_bytes());
    }
    for n in [name.len() as u16, 0, 0, 0, 0] {
        out.extend(n.to_le_bytes());
    }
    out.extend(0u32.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(name);
    let directory_size = out.len() as u32 - offset;
    out.extend(0x06054b50u32.to_le_bytes());
    for n in [0u16, 0, 1, 1] {
        out.extend(n.to_le_bytes());
    }
    out.extend(directory_size.to_le_bytes());
    out.extend(offset.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    Ok(out)
}
