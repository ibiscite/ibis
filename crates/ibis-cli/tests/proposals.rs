//! `ibis mcp --allow-writes` records proposals and `ibis proposals` reads and
//! rejects them; nothing here can apply one (decision 0055).
#![cfg(test)]

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const BIB: &str = "@book{lee2024,\n  title = {An Invented Field Guide},\n  author = {Lee, Ada},\n  year = {2024}\n}\n";

struct Home {
    root: PathBuf,
}

impl Home {
    fn new(name: &str) -> Self {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("proposals-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("refs.bib"), BIB).unwrap();
        Self { root }
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ibis"));
        command
            .args(args)
            .current_dir(&self.root)
            .env("HOME", &self.root)
            .env("USERPROFILE", &self.root)
            .env("APPDATA", self.root.join("AppData"))
            .env("XDG_CONFIG_HOME", self.root.join("config"));
        command
    }

    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let output = self.command(args).output().unwrap();
        let text = |bytes: Vec<u8>| String::from_utf8(bytes).unwrap();
        (
            output.status.code().unwrap_or(-1),
            text(output.stdout),
            text(output.stderr),
        )
    }

    /// MCP replies, by request id, to an initialized session sending `calls`.
    fn mcp(&self, args: &[&str], calls: &[serde_json::Value]) -> Vec<serde_json::Value> {
        let mut child = self
            .command(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        let initialize = serde_json::json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "Invented Client", "version": "1"}
        }});
        for message in std::iter::once(&initialize).chain(calls) {
            writeln!(input, "{message}").unwrap();
        }
        drop(input);
        let output = child.wait_with_output().unwrap();
        let mut replies: Vec<serde_json::Value> = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        replies.sort_by_key(|reply| reply["id"].as_i64());
        replies
    }
}

fn list(id: i64) -> serde_json::Value {
    serde_json::json!({"jsonrpc": "2.0", "id": id, "method": "tools/list", "params": {}})
}

fn propose(id: i64) -> serde_json::Value {
    serde_json::json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": "propose_add", "arguments": {
        "fields": {"type": "book", "title": "A Second Invented Guide", "author": [{"family": "Lee", "given": "Ada"}], "issued": {"date-parts": [[2024]]}}
    }}})
}

fn names(reply: &serde_json::Value) -> Vec<String> {
    reply["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn agents_propose_and_the_command_line_shows_and_rejects_but_never_applies() {
    let home = Home::new("flow");
    let replies = home.mcp(&["mcp", "--file", "refs.bib"], &[list(1), propose(2)]);
    assert!(!names(&replies[1]).iter().any(|name| name.starts_with("propose")));
    assert_eq!(replies[2]["error"]["message"], "Unknown tool");
    let (_, stdout, _) = home.run(&["proposals", "list", "--json"]);
    assert!(stdout.contains("\"proposals\":[]"), "{stdout}");

    let replies = home.mcp(&["mcp", "--allow-writes", "--file", "refs.bib"], &[list(1), propose(2)]);
    assert!(names(&replies[1]).contains(&"propose_add".to_owned()));
    let answer = &replies[2]["result"]["structuredContent"];
    let id = answer["proposal"].as_str().unwrap().to_owned();
    assert_eq!(answer["status"], "pending");
    assert!(answer["bibtex"].as_str().unwrap().starts_with("@book{lee2024second,"));
    assert_eq!(std::fs::read_to_string(home.root.join("refs.bib")).unwrap(), BIB);

    let (status, stdout, _) = home.run(&["proposals", "list"]);
    assert_eq!(status, 0);
    assert!(
        stdout.starts_with(&id) && stdout.contains("pending") && stdout.contains("refs.bib"),
        "{stdout}"
    );
    let (status, stdout, _) = home.run(&["proposals", "show", &id]);
    assert_eq!(status, 0);
    assert!(stdout.contains("from Invented Client"), "{stdout}");
    assert!(stdout.contains("After applying:\n@book{lee2024second,"), "{stdout}");
    assert!(stdout.contains("Apply or reject it in the Ibis app."), "{stdout}");

    let (status, _, stderr) = home.run(&["proposals", "apply", &id]);
    assert_eq!(status, 2, "{stderr}");
    let (status, stdout, _) = home.run(&["proposals", "reject", &id]);
    assert_eq!((status, stdout), (0, format!("Rejected {id}; no library changed.\n")));
    let (status, _, stderr) = home.run(&["proposals", "reject", &id]);
    assert_ne!(status, 0);
    assert!(stderr.contains("applied or rejected already"), "{stderr}");
    let (status, _, _) = home.run(&["proposals", "show", "0000000000000000"]);
    assert_ne!(status, 0);
    assert_eq!(std::fs::read_to_string(home.root.join("refs.bib")).unwrap(), BIB);
}
