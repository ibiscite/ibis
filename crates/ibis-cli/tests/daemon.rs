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

    fn endpoint(&self) -> Endpoint {
        let root = if cfg!(target_os = "linux") {
            Some(self.0.path().to_path_buf())
        } else if cfg!(windows) {
            Some(self.0.path().join("cache"))
        } else {
            None
        };
        Endpoint::new(&self.config(), Path::new(IBIS), root.as_deref()).unwrap()
    }

    #[cfg(unix)]
    fn socket_path(&self) -> PathBuf {
        let name = format!(
            "{}.sock",
            Endpoint::namespace(&self.config(), &Endpoint::installation(Path::new(IBIS), None))
        );
        if cfg!(target_os = "linux") {
            return self.0.path().join("ibis").join(name);
        }
        let dir = format!("ibis-{}", nix::unistd::geteuid());
        let socket = std::env::temp_dir().join(&dir).join(&name);
        if nix::sys::socket::UnixAddr::new(&socket).is_ok() {
            socket
        } else {
            Path::new("/tmp").join(dir).join(name)
        }
    }

    fn connect(&self, program: &Path) -> Connection {
        let endpoint = self.endpoint();
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

    /// An older build of the same installation: a copy beside the program, as
    /// an update in place leaves it, so both share one engine instance.
    fn older_build(&self) -> PathBuf {
        let name = self.0.path().file_name().unwrap().to_string_lossy();
        let older = Path::new(IBIS).with_file_name(format!("older-ibis-{name}.exe"));
        std::fs::copy(IBIS, &older).unwrap();
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.access_mode(0x100); // FILE_WRITE_ATTRIBUTES, with no data write access
        }
        options
            .open(&older)
            .unwrap()
            .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(100))
            .unwrap();
        // The system checks a new program on its first launch, which can take
        // seconds under load; do it here, before the timed daemon start.
        let checked = spawn_copy(Command::new(&older).arg("--version").stdout(Stdio::null()))
            .wait()
            .unwrap();
        assert!(checked.success());
        older
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let name = self.0.path().file_name().unwrap().to_string_lossy().into_owned();
        let _ = std::fs::remove_file(Path::new(IBIS).with_file_name(format!("older-ibis-{name}.exe")));
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
                    "io.modelcontextprotocol/protocolVersion":ibis::mcp::PROTOCOL_VERSION,
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
fn a_daemon_at_capacity_refuses_the_connection_and_the_cli_runs_locally() {
    let home = Home::new();
    let path = home.library();
    let _process = Process(home.command().arg("daemon").spawn().unwrap());
    let mut held = (0..32).map(|_| home.connect(Path::new(IBIS))).collect::<Vec<_>>();
    let busy = home.endpoint().connect(Path::new(IBIS)).err().unwrap();
    assert_eq!(busy.kind(), std::io::ErrorKind::ResourceBusy);
    assert_eq!(search(search_cli(&home, &path))["hits"].as_array().unwrap().len(), 1);
    drop(held.pop());
    let available = home.connect(Path::new(IBIS));
    assert_eq!(available.status().pid, held[0].status().pid);
    assert_eq!(available.status().library_loads, 0);
}

#[cfg(unix)]
#[test]
fn a_listening_daemon_in_transition_is_not_replaced_at_the_connection_deadline() {
    use std::os::unix::{fs::MetadataExt, net::UnixListener};

    let home = Home::new();
    let path = home.library();
    // Prepare the private runtime. An unresponsive listener with no lease
    // simulates the window where the socket and the lease disagree.
    assert!(home.endpoint().connect(Path::new(IBIS)).is_err());
    let socket = home.socket_path();
    let listener = UnixListener::bind(&socket).unwrap();
    let inode = std::fs::metadata(&socket).unwrap().ino();
    let started = Instant::now();
    let output = search(search_cli(&home, &path));
    let after = std::fs::metadata(&socket).unwrap().ino();
    drop(listener);
    std::fs::remove_file(socket).unwrap();
    assert_eq!(after, inode, "the client must not replace a listening socket");
    assert_eq!(output["hits"].as_array().unwrap().len(), 1);
    assert!(started.elapsed() < WAIT);
}

#[cfg(unix)]
#[test]
fn the_daemon_recovers_after_running_out_of_connection_descriptors() {
    // Each existing connection holds three descriptors. Exercise each
    // remaining-budget case so both accept and the initial clone are covered.
    for limit in [24, 25, 26] {
        check_descriptor_limit(limit);
    }
}

#[cfg(unix)]
fn check_descriptor_limit(limit: u32) {
    let home = Home::new();
    let _process = Process(
        home.program(Path::new("/bin/sh"))
            .args([
                "-c",
                r#"ulimit -n "$1" && shift && exec "$@""#,
                "ibis-test",
                &limit.to_string(),
                IBIS,
                "daemon",
            ])
            .spawn()
            .unwrap(),
    );
    let mut held = vec![home.connect(Path::new(IBIS))];
    let pid = held[0].status().pid;
    let endpoint = home.endpoint();
    let error = loop {
        assert!(held.len() < 12, "the descriptor limit should be reached first");
        match endpoint.connect(Path::new(IBIS)) {
            Ok(connection) => held.push(connection),
            Err(error) => break error,
        }
    };
    assert_ne!(error.kind(), std::io::ErrorKind::ResourceBusy);
    drop(held);
    assert_eq!(
        home.connect(Path::new(IBIS)).status().pid,
        pid,
        "descriptor limit {limit}"
    );
}

#[test]
fn the_daemon_does_not_use_the_first_clients_working_directory() {
    let home = Home::new();
    let path = home.library();
    let name = path.file_name().unwrap();
    let search_from_library_folder = home
        .command()
        .current_dir(path.parent().unwrap())
        .args(["search", "lantern", "--json", "--file"])
        .arg(name)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    assert_eq!(search(search_from_library_folder)["hits"].as_array().unwrap().len(), 1);
    let mut daemon = home.connect(Path::new(IBIS));
    let relative = rpc(
        &mut daemon,
        Method::Libraries,
        &json!({"action":"open-file","path":Path::new(name)}),
    );
    assert_eq!(relative["error"]["data"]["reason"], "missing-file", "{relative}");
    let absolute = rpc(
        &mut daemon,
        Method::Libraries,
        &json!({"action":"open-file","path":path}),
    );
    assert_eq!(absolute["result"]["libraries"][0]["objects"], 1);
}

#[test]
fn the_socket_and_stdio_return_the_generated_methods() {
    let home = Home::new();
    let file = home.library();
    ibis_engine::identity::use_key_file(home.config().join("com.ibiscite.ibis/library-ids.key"), true).unwrap();
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
            Ok(result) => ibis::rpc::success(&ibis::rpc::RequestId::Number(1.into()), result.data),
            Err(error) => ibis::rpc::failure(Some(&ibis::rpc::RequestId::Number(1.into())), error),
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

#[cfg(windows)]
#[test]
fn a_single_windows_sharing_violation_keeps_the_last_successful_library_check() {
    use std::os::windows::fs::OpenOptionsExt;
    let home = Home::new();
    let path = home.library();
    let _process = Process(home.command().arg("daemon").spawn().unwrap());
    let mut daemon = home.connect(Path::new(IBIS));
    let opened = rpc(
        &mut daemon,
        Method::Libraries,
        &json!({"action":"open-file","path":path}),
    );
    assert_eq!(opened["result"]["libraries"][0]["objects"], 1);
    let locked = std::fs::OpenOptions::new().read(true).share_mode(0).open(path).unwrap();
    // The first watch check starts after 250 ms and retries sharing violations
    // for 100 ms. Release before the independent recheck.
    std::thread::sleep(Duration::from_millis(425));
    let while_locked = rpc(&mut daemon, Method::Libraries, &Value::Null);
    drop(locked);
    assert_eq!(
        while_locked["result"]["source_health"]["status"], "ok",
        "{while_locked}"
    );
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
    let older = home.older_build();
    let mut process = Process(spawn_copy(home.program(&older).arg("daemon")));
    let connection = home.connect(&older);
    assert_eq!(connection.status().pid, process.0.id());
    let found = search(search_cli(&home, &path));
    assert!(!found["hits"].as_array().unwrap().is_empty());
    let new = home.connect(Path::new(IBIS));
    assert_ne!(new.status().pid, process.0.id());
    assert_eq!(new.status().library_loads, 1);
    assert!(process.0.try_wait().unwrap().unwrap().success());
}

fn spawn_copy(command: &mut Command) -> Child {
    let deadline = Instant::now() + WAIT;
    loop {
        match command.spawn() {
            Ok(child) => return child,
            // A simultaneous fork can still hold the fd from copying the binary.
            Err(error) if error.kind() == std::io::ErrorKind::ExecutableFileBusy && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("could not run the copied build: {error}"),
        }
    }
}

#[test]
fn a_connected_mcp_client_recovers_its_session_after_a_crash() {
    check_mcp_reconnect(false);
}

#[test]
fn an_older_mcp_and_cli_keep_working_after_a_newer_client_replaces_the_daemon() {
    check_mcp_reconnect(true);
}

fn check_mcp_reconnect(upgrade: bool) {
    let home = Home::new();
    let path = home.library();
    let older = upgrade.then(|| home.older_build());
    let program = older.as_deref().unwrap_or(Path::new(IBIS));
    let mut daemon = Process(spawn_copy(home.program(program).arg("daemon")));
    let before = home.connect(program);
    let mut mcp = Process(spawn_copy(
        home.program(program)
            .args(["mcp", "--file"])
            .arg(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped()),
    ));
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
        "protocolVersion":ibis::mcp::INITIALIZE_VERSIONS[0],"capabilities":{},"clientInfo":{"name":"invented","version":"1"}
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
    if upgrade {
        assert_eq!(search(search_cli(&home, &path))["hits"].as_array().unwrap().len(), 1);
        assert!(daemon.0.wait().unwrap().success());
    } else {
        daemon.0.kill().unwrap();
        daemon.0.wait().unwrap();
    }
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
    if upgrade {
        writeln!(input, "{}", tool(5)).unwrap();
        assert_eq!(receive.recv_timeout(WAIT).unwrap()["result"], first["result"]);
        let output = home
            .program(program)
            .args(["search", "lantern", "--json", "--file"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap()["data"]["hits"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(home.connect(Path::new(IBIS)).status().mcp_sessions, 0);
    }
    drop(input);
    assert!(mcp.0.wait().unwrap().success());
    reader.join().unwrap();
}

#[test]
fn disabled_empty_history_stays_untouched_during_settings_writes() {
    let home = Home::new();
    let profile = home.config().join("com.ibiscite.ibis");
    std::fs::create_dir_all(&profile).unwrap();
    let history = profile.join("recents.json");
    std::fs::write(
        profile.join("settings.json"),
        r#"{"schema_version":2,"recent_citations":false}"#,
    )
    .unwrap();
    std::fs::write(&history, "[]").unwrap();
    let before = std::fs::metadata(&history).unwrap();
    let _daemon = Process(home.command().arg("daemon").spawn().unwrap());
    let connection = home.connect(Path::new(IBIS));
    let saves = ["Invented A", "Invented B", "Invented C"].map(|name| {
        home.command()
            .args(["projects", "create", name, "--format", "apa", "--json"])
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap()
    });
    for save in saves {
        let saved = save.wait_with_output().unwrap();
        assert!(saved.status.success(), "{}", String::from_utf8_lossy(&saved.stderr));
    }
    let after_writes = home.connect(Path::new(IBIS));
    assert_eq!(after_writes.status().pid, connection.status().pid);
    assert_eq!(after_writes.status().cli_connections, 3);
    std::thread::sleep(Duration::from_secs(3));
    let after = std::fs::metadata(&history).unwrap();
    assert_eq!(after.modified().unwrap(), before.modified().unwrap());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(after.ino(), before.ino());
    }
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
    let file = ibis_os::atomic_file::open_private(&settings.with_extension("json.lock")).unwrap();
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

#[cfg(target_os = "macos")]
#[test]
fn picker_releases_a_delivery_whose_acknowledgement_stops_mid_frame() {
    use std::fs::OpenOptions;
    use std::os::unix::{fs::OpenOptionsExt, net::UnixStream};

    let home = Home::new();
    let library = home.library();
    let endpoint = home.command().arg("__picker_endpoint").output().unwrap();
    assert!(endpoint.status.success());
    let endpoint: Value = serde_json::from_slice(&endpoint.stdout).unwrap();
    let mut wire = UnixStream::connect(endpoint["socket"].as_str().unwrap()).unwrap();
    wire.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut reader = BufReader::new(wire.try_clone().unwrap());
    write_frame(&mut wire, &endpoint["hello"]).unwrap();
    assert!(read_response_frame(&mut reader).unwrap().unwrap()["ready"].is_object());
    let mut call = |id, method, params| {
        write_frame(
            &mut wire,
            &json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
        )
        .unwrap();
        let response = read_response_frame(&mut reader).unwrap().unwrap();
        assert_eq!(response["id"], id);
        assert!(response["result"].is_object(), "{response}");
        response["result"].clone()
    };
    call(1, "picker/open", json!({"app":null,"file":library,"language":"en-US"}));
    let state = call(2, "picker/act", json!({"intent":"type","query":"lantern"}));
    let preview = &state["preview"];
    assert!(preview["ticket"].is_string(), "{state}");
    let fence = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(endpoint["delivery_lock"].as_str().unwrap())
        .unwrap();
    fence.lock().unwrap();
    let grant = call(
        4,
        "picker/deliver",
        json!({
            "ticket":preview["ticket"],"content_hash":preview["content_hash"],"paste":false,
            "delivery":"00000000000000000000000000000001"
        }),
    );
    assert_eq!(grant["text"], preview["text"]);
    let start = Instant::now();
    wire.write_all(b"Content-Length: 4\r\n\r\nt").unwrap();
    assert!(read_response_frame(&mut reader).unwrap().is_none());
    assert!(start.elapsed() >= std::time::Duration::from_millis(ibis_core::destination::PASTE_DEADLINE_MS));
    assert!(start.elapsed() < Duration::from_secs(5));
}

/// An engine a shell started for itself, answering picker turns on its
/// standard streams as when no daemon can be reached.
struct PrivateEngine {
    child: std::process::Child,
    input: std::process::ChildStdin,
    output: BufReader<std::process::ChildStdout>,
    next: u64,
}

impl PrivateEngine {
    fn start(home: &Home) -> Self {
        let mut child = home
            .command()
            .arg("__picker_stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            input,
            output,
            next: 1,
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        write_frame(
            &mut self.input,
            &json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
        )
        .unwrap();
        let response = read_response_frame(&mut self.output).unwrap().unwrap();
        assert_eq!(response["id"], id, "{response}");
        assert!(response["result"].is_object(), "{response}");
        response["result"].clone()
    }

    fn close(self) {
        let Self { mut child, input, .. } = self;
        drop(input);
        assert!(child.wait().unwrap().success());
    }
}

/// One picker opening in a private engine: open, type, then deliver with
/// `delivery` and report the write as done.
fn deliver_from_a_private_engine(home: &Home, library: &Path, delivery: &str) -> Value {
    let mut engine = PrivateEngine::start(home);
    engine.call("picker/open", json!({"app":null,"file":library,"language":"en-US"}));
    let state = engine.call("picker/act", json!({"intent":"type","query":"lantern"}));
    let preview = state["preview"].clone();
    assert!(preview["ticket"].is_string(), "{state}");
    let grant = engine.call(
        "picker/deliver",
        json!({
            "ticket":preview["ticket"],"content_hash":preview["content_hash"],
            "paste":true,"delivery":delivery
        }),
    );
    assert_eq!(grant["text"], preview["text"]);
    write_frame(&mut engine.input, &json!(true)).unwrap();
    engine.close();
    grant
}

/// The citation being built outlives the engine holding it: the next engine
/// takes it back when the shell reopens soon after, and a new opening, or a
/// delivery, leaves nothing behind.
#[test]
fn a_citation_being_built_survives_its_engine() {
    let home = Home::new();
    let library = home.library();
    let open = |resume: bool| json!({"app":null,"file":library,"language":"en-US","keep_query":resume,"resume":resume});
    let added = |state: &Value| {
        state["citation"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["added"] == true)
            .count()
    };
    let mut first = PrivateEngine::start(&home);
    first.call("picker/open", open(false));
    first.call("picker/act", json!({"intent":"type","query":"lantern"}));
    let state = first.call("picker/act", json!({"intent":"add"}));
    assert_eq!(added(&state), 1, "{state}");
    // Reopening soon after, on the same engine, starts a new citation.
    let soon = json!({"app":null,"file":library,"language":"en-US","keep_query":true});
    let reopened = first.call("picker/open", soon);
    assert_eq!(added(&reopened["state"]), 0, "{reopened}");
    first.call("picker/act", json!({"intent":"type","query":"lantern"}));
    let state = first.call("picker/act", json!({"intent":"add"}));
    assert_eq!(added(&state), 1, "{state}");
    first.close();

    // A reconnecting shell replays its opening, then opens again.
    let mut second = PrivateEngine::start(&home);
    let replayed = second.call("picker/open", open(true));
    assert_eq!(added(&replayed["state"]), 1, "{replayed}");
    let reopened = second.call("picker/open", open(true));
    assert_eq!(added(&reopened["state"]), 1, "{reopened}");
    let preview = reopened["state"]["preview"].clone();
    let grant = second.call(
        "picker/deliver",
        json!({
            "ticket":preview["ticket"],"content_hash":preview["content_hash"],
            "paste":false,"delivery":"abcdef0123456789abcdef0123456789"
        }),
    );
    assert_eq!(grant["text"], preview["text"]);
    write_frame(&mut second.input, &json!(true)).unwrap();
    second.close();

    let mut third = PrivateEngine::start(&home);
    let after = third.call("picker/open", open(true));
    assert_eq!(added(&after["state"]), 0, "a delivered citation is not kept: {after}");
    third.close();
}

/// A Return pastes at most once whichever engine answers it. A shell that lost
/// the reply and asks again of another engine, as after a crash or with no
/// daemon, gets the citation to copy, never a second paste.
#[test]
fn a_delivery_asked_for_again_of_another_engine_never_pastes_twice() {
    let home = Home::new();
    let library = home.library();
    let once = "0123456789abcdef0123456789abcdef";
    assert_eq!(deliver_from_a_private_engine(&home, &library, once)["paste"], true);
    assert_eq!(deliver_from_a_private_engine(&home, &library, once)["paste"], false);
    let next = "fedcba9876543210fedcba9876543210";
    assert_eq!(deliver_from_a_private_engine(&home, &library, next)["paste"], true);
}

#[test]
fn each_installation_keeps_its_own_engine_and_an_update_in_place_shares_one() {
    let home = Home::new();
    let config = home.config();
    let at = |program: &Path| Endpoint::namespace(&config, &Endpoint::installation(program, None));
    let here = at(Path::new(IBIS));
    assert_eq!(here.len(), 16, "a short name keeps socket paths and pipe names valid");
    // Another build in the same folder is an update in place: the same engine.
    assert_eq!(here, at(&Path::new(IBIS).with_file_name("ibis-updated")));
    // Another installation gets its own engine over the same files.
    let elsewhere = home.0.path().join("Other Ibis.app/Contents/MacOS/ibis");
    assert_ne!(here, at(&elsewhere));
    assert_ne!(
        here,
        Endpoint::namespace(
            &home.0.path().join("isolated"),
            &Endpoint::installation(Path::new(IBIS), None)
        )
    );
    // An AppImage mounts anew at every launch; its image file is the installation,
    // so two images in one folder stay apart and each launch finds the same engine.
    let folder = home.0.path().join("Applications");
    let image =
        |name: &str| Endpoint::installation(Path::new("/tmp/.mount_random/usr/bin/ibis"), Some(folder.join(name)));
    assert_eq!(image("Ibis.AppImage"), image("Ibis.AppImage"));
    assert_ne!(image("Ibis.AppImage"), image("Ibis-Nightly.AppImage"));
    // A command linked elsewhere joins the installation it points at.
    #[cfg(unix)]
    {
        let link = home.0.path().join("ibis");
        std::os::unix::fs::symlink(IBIS, &link).unwrap();
        assert_eq!(at(&link), here);
    }
}
