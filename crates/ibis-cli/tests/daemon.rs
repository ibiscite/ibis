//! The on-demand engine is a separate process and shares its method contract with stdio.
#![cfg(test)]

use ibis::daemon::{Connection, Endpoint};
use ibis_engine::protocol::{read_response_frame, write_frame};
use ibis_engine::{Cancellation, Engine, Method};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(12);
const IBIS: &str = env!("CARGO_BIN_EXE_ibis");

struct Home(tempfile::TempDir);

impl Home {
    fn new() -> Self {
        Self(tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap())
    }

    fn command(&self) -> Command {
        self.program(Path::new(IBIS))
    }

    fn program(&self, program: &Path) -> Command {
        let root = self.0.path();
        let mut command = Command::new(program);
        command
            .env("HOME", root)
            .env("APPDATA", root.join("config"))
            .env("LOCALAPPDATA", root.join("cache"))
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("XDG_RUNTIME_DIR", root)
            .stdin(Stdio::null());
        command
    }

    fn config(&self) -> PathBuf {
        if cfg!(target_os = "macos") {
            self.0.path().join("Library/Application Support")
        } else {
            self.0.path().join("config")
        }
    }

    fn connect(&self, program: &Path) -> Connection {
        let root = if cfg!(target_os = "linux") {
            Some(self.0.path().to_path_buf())
        } else if cfg!(windows) {
            Some(self.0.path().join("cache"))
        } else {
            None
        };
        let endpoint = Endpoint::new(&self.config(), root.as_deref()).unwrap();
        let deadline = Instant::now() + WAIT;
        loop {
            match endpoint.connect(program) {
                Ok(connection) => return connection,
                Err(error) => assert!(Instant::now() < deadline, "daemon unavailable: {error}"),
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn library(&self) -> PathBuf {
        let path = self.0.path().join("invented.bib");
        std::fs::write(
            &path,
            "@book{lantern,title={Invented Lantern},author={Vale, Ada},year={2026}}\n",
        )
        .unwrap();
        path
    }
}

struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn rpc(connection: &mut Connection, method: Method, params: &Value) -> Value {
    connection
        .send(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
        .unwrap();
    loop {
        let response = connection.receive().unwrap();
        if response["id"] == 1 {
            return response;
        }
    }
}

fn request(writer: &mut impl Write, method: Method, params: &Value) {
    write_frame(writer, &json!({"jsonrpc":"2.0","id":1,"method":method,"params":params})).unwrap();
}

fn reply(reader: &mut impl std::io::BufRead) -> Value {
    loop {
        let frame = read_response_frame(reader).unwrap().unwrap();
        if frame["id"] == 1 {
            return frame;
        }
    }
}

fn run_stdio(home: &Home, file: &Path, method: Method, params: &Value) -> Value {
    let mut child = home
        .command()
        .args(["serve", "--stdio", "--file"])
        .arg(file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    request(&mut child.stdin.take().unwrap(), method, params);
    reply(&mut BufReader::new(child.wait_with_output().unwrap().stdout.as_slice()))
}

fn search(child: Child) -> Value {
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice::<Value>(&output.stdout).unwrap()["data"].clone()
}

fn search_cli(home: &Home, path: &Path) -> Child {
    home.command()
        .args(["search", "lantern", "--json", "--file"])
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

#[test]
fn simultaneous_cli_and_mcp_clients_share_a_warm_library() {
    let home = Home::new();
    let path = home.library();
    let first = search_cli(&home, &path);
    let second = search_cli(&home, &path);
    let mut mcp = home
        .command()
        .args(["mcp", "--file"])
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(
        mcp.stdin.take().unwrap(),
        "{}",
        json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call","params":{
                "name":"search","arguments":{"query":"lantern"},"_meta":{
                    "io.modelcontextprotocol/protocolVersion":ibis_engine::mcp::PROTOCOL_VERSION,
                    "io.modelcontextprotocol/clientCapabilities":{}
                }
            }
        })
    )
    .unwrap();
    let first = search(first);
    assert_eq!(first, search(second));
    let mut output = String::new();
    mcp.stdout.take().unwrap().read_to_string(&mut output).unwrap();
    assert!(mcp.wait().unwrap().success());
    let response: Value = serde_json::from_str(output.trim()).unwrap();
    assert!(
        response["result"]["structuredContent"]
            .to_string()
            .contains("Invented Lantern"),
        "{response}"
    );
    let daemon = home.connect(Path::new(IBIS));
    assert_eq!(daemon.status().library_loads, 1);
    assert_eq!(daemon.status().cli_connections, 3);
    assert_eq!(daemon.status().mcp_sessions, 1);
    let other = home.0.path().join("other.bib");
    std::fs::write(&other, "@book{other,title={Another Lantern}}").unwrap();
    let other_search = search(search_cli(&home, &other));
    assert!(other_search["hits"].to_string().contains("Another Lantern"));
    assert!(!other_search["hits"].to_string().contains("Invented Lantern"));
    assert_eq!(search(search_cli(&home, &path)), first);
    let after = home.connect(Path::new(IBIS));
    assert_eq!(daemon.status().pid, after.status().pid);
    assert_eq!(after.status().library_loads, 2);
}

#[test]
fn the_socket_and_stdio_return_the_generated_methods() {
    let home = Home::new();
    let file = home.library();
    ibis_engine::identity::use_key_file(home.config().join("com.ibiscite.ibis/library-ids.key"), true);
    let engine = Engine::new().unwrap();
    engine.open_file_observed(&file, &Cancellation::default()).unwrap();
    let _process = Process(home.command().arg("daemon").spawn().unwrap());
    let mut daemon = home.connect(Path::new(IBIS));
    let opened = rpc(
        &mut daemon,
        Method::Libraries,
        &json!({"action":"open-file","path":file}),
    );
    assert_eq!(opened["result"]["libraries"].as_array().unwrap().len(), 1);
    let item = engine
        .show(&ibis_engine::request::ShowRequest {
            item: ibis_engine::request::ItemSelector::Key("lantern".into()),
        })
        .unwrap()
        .object
        .id()
        .clone();
    let output = json!({"kind":"preset", "name":"pandoc"});
    for method in Method::ALL {
        let params = match method {
            Method::Search => json!({"query":"lantern"}),
            Method::Show | Method::Entry | Method::Outline => json!({"item":{"by":"key","value":"lantern"}}),
            Method::Cite => json!({"items":[{"item":item}], "output":output}),
            Method::Records => {
                json!({"records":[{"id":"invented","type":"book","title":"Invented"}],"output":{"kind":"preset","name":"apa"}})
            }
            Method::Project => json!({"path":file,"keys":["lantern"]}),
            Method::Share => json!({"items":[{"by":"key","value":"lantern"}]}),
            Method::Read => json!({"attachment":"absent"}),
            Method::FindIn => json!({"attachment":"absent","query":"nothing"}),
            Method::Call => json!({"method":"outputs"}),
            _ => Value::Null,
        };
        let expected = match engine.dispatch(method, &params, &Cancellation::default()) {
            Ok(result) => ibis_engine::rpc::success(&ibis_engine::rpc::RequestId::Number(1.into()), result.data),
            Err(error) => ibis_engine::rpc::failure(Some(&ibis_engine::rpc::RequestId::Number(1.into())), error),
        };
        assert_eq!(
            comparable(expected.clone()),
            comparable(run_stdio(&home, &file, method, &params)),
            "{method:?} via stdio"
        );
        assert_eq!(
            comparable(expected),
            comparable(rpc(&mut daemon, method, &params)),
            "{method:?} via socket"
        );
    }
}

fn comparable(mut reply: Value) -> Value {
    if let Some(libraries) = reply.pointer_mut("/result/libraries").and_then(Value::as_array_mut) {
        for library in libraries {
            library.as_object_mut().unwrap().remove("updated_at_ms");
        }
    }
    if let Some(sources) = reply
        .pointer_mut("/result/receipt/sources")
        .and_then(Value::as_array_mut)
    {
        for source in sources {
            source.as_object_mut().unwrap().remove("last_successful_read_ms");
        }
    }
    reply
}

#[test]
fn a_newer_client_drains_the_older_build_and_starts_one_replacement() {
    let home = Home::new();
    let path = home.library();
    let older = home.0.path().join("older-ibis.exe");
    std::fs::copy(IBIS, &older).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&older)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(100))
        .unwrap();
    let mut process = Process(home.program(&older).arg("daemon").spawn().unwrap());
    let connection = home.connect(&older);
    assert_eq!(connection.status().pid, process.0.id());
    let found = search(search_cli(&home, &path));
    assert!(!found["hits"].as_array().unwrap().is_empty());
    let new = home.connect(Path::new(IBIS));
    assert_ne!(new.status().pid, process.0.id());
    assert_eq!(new.status().library_loads, 1);
    assert!(process.0.try_wait().unwrap().unwrap().success());
}

#[test]
fn a_connected_mcp_client_recovers_its_session_after_a_crash() {
    let home = Home::new();
    let path = home.library();
    let mut daemon = Process(home.command().arg("daemon").spawn().unwrap());
    let before = home.connect(Path::new(IBIS));
    let mut mcp = Process(
        home.command()
            .args(["mcp", "--file"])
            .arg(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut input = mcp.0.stdin.take().unwrap();
    let output = mcp.0.stdout.take().unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(output).lines().map_while(Result::ok) {
            if send.send(serde_json::from_str::<Value>(&line).unwrap()).is_err() {
                break;
            }
        }
    });
    writeln!(input, "{}", json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":ibis_engine::mcp::INITIALIZE_VERSIONS[0],"capabilities":{},"clientInfo":{"name":"invented","version":"1"}
    }})).unwrap();
    let initialized = receive.recv_timeout(WAIT).unwrap();
    assert!(initialized.get("result").is_some());
    let tool = |id| json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"search","arguments":{"query":"lantern"}}});
    writeln!(input, "{}", tool(2)).unwrap();
    let first = receive.recv_timeout(WAIT).unwrap();
    assert!(
        first["result"]["structuredContent"]
            .to_string()
            .contains("Invented Lantern"),
        "{first}"
    );
    daemon.0.kill().unwrap();
    daemon.0.wait().unwrap();
    writeln!(input, "{}", tool(3)).unwrap();
    let mut after = receive.recv_timeout(WAIT).unwrap();
    if after.get("error").is_some() {
        // A write racing the disconnect has an uncertain outcome. A fresh
        // request reconnects without replaying that write.
        writeln!(input, "{}", tool(4)).unwrap();
        after = receive.recv_timeout(WAIT).unwrap();
    }
    assert_eq!(after["result"], first["result"], "{after}");
    assert_ne!(home.connect(Path::new(IBIS)).status().pid, before.status().pid);
    drop(input);
    assert!(mcp.0.wait().unwrap().success());
    reader.join().unwrap();
}

#[test]
fn restoring_settings_honors_the_apps_writer_lock_and_clears_disabled_history() {
    let home = Home::new();
    let profile = home.config().join("com.ibiscite.ibis");
    std::fs::create_dir_all(&profile).unwrap();
    let settings = profile.join("settings.json");
    let history = profile.join("recents.json");
    let original = json!({"schema_version":2, "recent_citations":true});
    std::fs::write(&settings, original.to_string()).unwrap();
    std::fs::write(&history, r#"[["invented-app",[]]]"#).unwrap();
    let _daemon = Process(home.command().arg("daemon").spawn().unwrap());
    let daemon = home.connect(Path::new(IBIS));
    std::fs::write(&settings, r#"{"schema_version":2,"recent_citations":false}"#).unwrap();
    let deadline = Instant::now() + WAIT;
    while std::fs::read_to_string(&history).unwrap() != "[]" {
        assert!(Instant::now() < deadline, "the daemon did not observe the hand edit");
        std::thread::sleep(Duration::from_millis(20));
    }
    std::fs::write(&settings, original.to_string()).unwrap();
    std::fs::write(&history, r#"[["invented-app",[]]]"#).unwrap();
    let backup = home.0.path().join("invented-backup.json");
    std::fs::write(
        &backup,
        json!({
            "ibis_settings_backup":1, "made_by":"0.3.13", "made_on":"macos",
            "settings":{"schema_version":2,"recent_citations":false}
        })
        .to_string(),
    )
    .unwrap();
    let file = ibis_core::atomic_file::open_private(&settings.with_extension("json.lock")).unwrap();
    let deadline = Instant::now() + WAIT;
    while let Err(error) = file.try_lock() {
        assert!(
            matches!(error, std::fs::TryLockError::WouldBlock) && Instant::now() < deadline,
            "{error}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let restore = || {
        home.command()
            .args(["settings", "import", "--json"])
            .arg(&backup)
            .output()
            .unwrap()
    };
    let held = restore();
    assert_eq!(held.status.code(), Some(5));
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(&settings).unwrap()).unwrap(),
        original
    );
    file.unlock().unwrap();
    let saved = restore();
    assert!(saved.status.success(), "{}", String::from_utf8_lossy(&saved.stderr));
    assert_eq!(
        serde_json::from_slice::<Value>(&saved.stdout).unwrap()["data"]["restored"],
        true
    );
    assert_eq!(std::fs::read_to_string(&history).unwrap(), "[]");
    assert_eq!(daemon.status().pid, home.connect(Path::new(IBIS)).status().pid);
}
