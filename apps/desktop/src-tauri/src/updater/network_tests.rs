// Feed, download and signature tests against a local mock server.
use super::*;
use serde_json::json;

#[derive(Default)]
struct InstallNotices(Mutex<Vec<&'static str>>);

impl InstallationUi for tauri::test::MockRuntime {
    fn failed(app: &AppHandle<Self>) {
        app.state::<InstallNotices>()
            .0
            .lock()
            .unwrap_or_else(|e| panic!("{e}"))
            .push(INSTALL_FAILURE_MESSAGE);
    }

    fn restart(_: &AppHandle<Self>) {
        panic!("a partial download must not reach installation or restart");
    }
}

#[test]
fn stalled_manifest_headers_and_body_time_out_and_allow_a_later_check() {
    use std::io::{BufRead, BufReader, Write};
    use tauri_plugin_updater::Error;

    for partial_body in [false, true] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap_or_else(|e| panic!("{e}"));
        let endpoint = format!(
            "http://{}/latest.json",
            listener.local_addr().unwrap_or_else(|e| panic!("{e}"))
        );
        listener.set_nonblocking(true).unwrap_or_else(|e| panic!("{e}"));
        let (release, held) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            for attempt in 0..2 {
                let deadline = std::time::Instant::now() + Duration::from_secs(3);
                let (mut stream, _) = loop {
                    match listener.accept() {
                        Ok(connection) => break connection,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && std::time::Instant::now() < deadline =>
                        {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("accept: {error}"),
                    }
                };
                // Accepted sockets can inherit the listener mode on macOS.
                stream.set_nonblocking(false).unwrap_or_else(|e| panic!("{e}"));
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap_or_else(|e| panic!("{e}"));
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap_or_else(|e| panic!("{e}"));
                let mut request = BufReader::new(&stream);
                let mut line = String::new();
                while request.read_line(&mut line).unwrap_or_else(|e| panic!("{e}")) > 2 {
                    line.clear();
                }
                if attempt == 0 {
                    if partial_body {
                        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 100\r\n\r\n{\"version\":").unwrap_or_else(|e| panic!("{e}"));
                    }
                    // A bounded server hold makes the pre-fix failure finite.
                    let _ = held.recv_timeout(Duration::from_secs(2));
                } else {
                    stream
                        .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
                        .unwrap_or_else(|e| panic!("{e}"));
                }
            }
        });
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().plugins.0.insert(
            "updater".into(),
            json!({ "pubkey": "synthetic", "endpoints": [endpoint], "dangerousInsecureTransportProtocol": true }),
        );
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .unwrap_or_else(|e| panic!("{e}"));
        let client = updater(app.handle(), Duration::from_millis(200)).unwrap_or_else(|e| panic!("{e}"));
        let stalled = tauri::async_runtime::block_on(client.check());
        let _ = release.send(());
        let recovered = tauri::async_runtime::block_on(client.check());
        server.join().unwrap_or_else(|_| panic!("server thread panicked"));
        assert!(
            matches!(&stalled, Err(Error::Reqwest(error)) if error.is_timeout()),
            "partial_body={partial_body}: expected request timeout"
        );
        assert!(matches!(recovered, Ok(None)), "later check must complete");
    }
}

#[test]
fn production_install_timeout_reports_failure_and_allows_a_later_check() {
    use std::io::{BufRead, BufReader, Write};
    use std::time::Instant;
    use tauri::Listener;

    for mode in ["headers", "body", "trickle"] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap_or_else(|e| panic!("{e}"));
        let base = format!("http://{}", listener.local_addr().unwrap_or_else(|e| panic!("{e}")));
        let manifest = json!({
            "version": "9.0.0", "url": format!("{base}/package.bin"), "signature": "synthetic"
        })
        .to_string();
        listener.set_nonblocking(true).unwrap_or_else(|e| panic!("{e}"));
        let (release, held) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            for request in 0..3 {
                let deadline = Instant::now() + Duration::from_secs(3);
                let (mut stream, _) = loop {
                    match listener.accept() {
                        Ok(connection) => break connection,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(e) => panic!("accept: {e}"),
                    }
                };
                stream.set_nonblocking(false).unwrap_or_else(|e| panic!("{e}"));
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap_or_else(|e| panic!("{e}"));
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap_or_else(|e| panic!("{e}"));
                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap_or_else(|e| panic!("{e}")) > 2 {
                    line.clear();
                }
                match request {
                    0 => write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{manifest}",
                        manifest.len()
                    )
                    .unwrap_or_else(|e| panic!("{e}")),
                    1 => {
                        if mode != "headers" {
                            stream
                                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100000\r\n\r\nx")
                                .unwrap_or_else(|e| panic!("{e}"));
                        }
                        // Hold the response open; a finite fallback bounds pre-fix failures.
                        let deadline = Instant::now() + Duration::from_secs(2);
                        while Instant::now() < deadline {
                            if held.recv_timeout(Duration::from_millis(40)).is_ok() {
                                break;
                            }
                            if mode == "trickle" && stream.write_all(b"x").is_err() {
                                break;
                            }
                        }
                    }
                    _ => stream
                        .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
                        .unwrap_or_else(|e| panic!("{e}")),
                }
            }
        });
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().plugins.0.insert(
            "updater".into(),
            json!({ "pubkey": "synthetic", "endpoints": [format!("{base}/latest.json")], "dangerousInsecureTransportProtocol": true }),
        );
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .manage(Updates::default())
            .manage(InstallNotices::default())
            .build(context)
            .unwrap_or_else(|e| panic!("{e}"));
        let delivered = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let heard = Arc::clone(&delivered);
        let _listener = app.listen_any("update:progress", move |event| {
            heard
                .lock()
                .unwrap_or_else(|e| panic!("{e}"))
                .push(serde_json::from_str(event.payload()).unwrap_or_else(|e| panic!("{e}")));
        });
        let client = updater_builder(app.handle(), Duration::from_millis(200))
            .unwrap_or_else(|e| panic!("{e}"))
            .timeout(Duration::from_millis(300))
            .no_proxy()
            .build()
            .unwrap_or_else(|e| panic!("{e}"));
        let download_timeout = if mode == "trickle" {
            Duration::from_millis(300)
        } else {
            Duration::from_secs(2)
        };
        let update = tauri::async_runtime::block_on(find_update(&client, download_timeout))
            .unwrap_or_else(|e| panic!("{e}"))
            .unwrap_or_else(|| panic!("update offer"));
        let updates = app.state::<Updates>();
        let busy = updates.begin().unwrap_or_else(|| panic!("installation"));
        let started = Instant::now();
        tauri::async_runtime::block_on(install_update(app.handle(), update, busy));
        let elapsed = started.elapsed();
        let _ = release.send(());
        let _retry = updates.begin().unwrap_or_else(|| panic!("timeout must release busy"));
        let recovered = tauri::async_runtime::block_on(client.check());
        server.join().unwrap_or_else(|_| panic!("server thread panicked"));
        let state = updates.progress.lock().unwrap_or_else(|e| panic!("{e}"));
        let state = state.0.as_ref().unwrap_or_else(|| panic!("installation progress"));
        assert_eq!((state.attempt, state.phase), (1, "failed"));
        let delivered = delivered.lock().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            delivered.first().map(|state| (&state["phase"], &state["received"])),
            Some((&json!("downloading"), &json!(0)))
        );
        assert_eq!(delivered.last().map(|state| &state["phase"]), Some(&json!("failed")));
        assert!(
            delivered.iter().all(|state| state["phase"] != "installing"),
            "partial bytes must not reach installation"
        );
        assert_eq!(
            app.state::<InstallNotices>()
                .0
                .lock()
                .unwrap_or_else(|e| panic!("{e}"))
                .as_slice(),
            [INSTALL_FAILURE_MESSAGE],
            "the production failure path must request its recovery notice"
        );
        assert!(
            elapsed >= Duration::from_millis(150),
            "{mode}: failed before the timeout"
        );
        assert!(elapsed < Duration::from_secs(1), "{mode}: exceeded download budget");
        if mode == "trickle" {
            assert!(state.received > 1, "trickle must keep making progress");
            assert!(
                elapsed >= Duration::from_millis(250),
                "progress must reset the stall timeout"
            );
        }
        assert!(matches!(recovered, Ok(None)), "later check must complete");
    }
}

/// A real check and download against a local server: the manifest shape the
/// release workflow publishes, a matching signature, and refusals.
#[test]
fn signed_updates_are_found_and_verified_and_tampered_ones_refused() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/updates");
    let read = |name: &str| std::fs::read(fixtures.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
    let package = read("package.bin");
    let signature = String::from_utf8(read("package-versioned.bin.sig")).unwrap_or_default();
    let unversioned = String::from_utf8(read("package.bin.sig")).unwrap_or_default();
    let stable = String::from_utf8(read("package-stable.bin.sig")).unwrap_or_default();
    let forged = String::from_utf8(read("package-forged-comment.bin.sig")).unwrap_or_default();
    let key = String::from_utf8(read("test-key.pub")).unwrap_or_default();
    let mut tampered = package.clone();
    tampered[0] ^= 1;

    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap_or_else(|e| panic!("{e}"));
    let base = format!("http://{}", server.local_addr().unwrap_or_else(|e| panic!("{e}")));
    let platform = json!({ "signature": signature.trim(), "url": format!("{base}/package.bin") });
    let tampered_platform = json!({ "signature": signature.trim(), "url": format!("{base}/tampered.bin") });
    let manifest = |entry: &serde_json::Value, version: &str| {
        let platforms: serde_json::Map<_, _> = [
            "darwin-aarch64-app",
            "darwin-x86_64-app",
            "windows-x86_64-nsis",
            "linux-x86_64-appimage",
        ]
        .into_iter()
        .map(|target| (target.to_string(), entry.clone()))
        .collect();
        json!({ "version": version, "notes": "Synthetic notes.", "pub_date": "2099-12-31T23:59:00Z", "platforms": platforms })
            .to_string()
            .into_bytes()
    };
    let version = "9.9.9-nightly.209912312359";
    let files = [
        ("/latest.json", manifest(&platform, version)),
        ("/prefixed.json", manifest(&platform, &format!("v{version}"))),
        ("/replay.json", manifest(&platform, "9.9.9-nightly.209912312400")),
        (
            "/stable.json",
            manifest(
                &json!({ "signature": stable.trim(), "url": format!("{base}/package.bin") }),
                "9.9.9",
            ),
        ),
        (
            "/forged.json",
            manifest(
                &json!({ "signature": forged.trim(), "url": format!("{base}/package.bin") }),
                "9.9.9-nightly.209912312400",
            ),
        ),
        ("/metadata.json", manifest(&platform, &format!("{version}+other"))),
        ("/tampered.json", manifest(&tampered_platform, version)),
        (
            "/unversioned.json",
            manifest(
                &json!({ "signature": unversioned.trim(), "url": format!("{base}/package.bin") }),
                version,
            ),
        ),
        (
            "/interrupted.json",
            manifest(
                &json!({ "signature": signature.trim(), "url": format!("{base}/interrupted.bin") }),
                version,
            ),
        ),
        ("/interrupted.bin", package[..package.len() / 2].to_vec()),
        ("/package.bin", package.clone()),
        ("/tampered.bin", tampered),
    ];
    let agents = std::sync::Arc::new(Mutex::new(Vec::new()));
    let seen = agents.clone();
    std::thread::spawn(move || {
        use std::io::{BufRead, BufReader, Write};
        for stream in server.incoming().flatten() {
            let mut reader = BufReader::new(&stream);
            let mut request = String::new();
            if reader.read_line(&mut request).is_err() {
                continue;
            }
            let mut header = String::new();
            while reader.read_line(&mut header).is_ok_and(|n| n > 2) {
                if let Some(agent) = header.to_ascii_lowercase().strip_prefix("user-agent:")
                    && let Ok(mut seen) = seen.lock()
                {
                    seen.push(agent.trim().to_string());
                }
                header.clear();
            }
            let path = request.split(' ').nth(1).unwrap_or_default();
            let body = files
                .iter()
                .find(|(name, _)| *name == path)
                .map(|(_, body)| body.as_slice());
            let mut stream = &stream;
            let _ = match body {
                Some(body) => write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len() + usize::from(path == "/interrupted.bin")
                )
                .and_then(|()| stream.write_all(body)),
                None => write!(
                    stream,
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                ),
            };
        }
    });

    let app_with_key = |pubkey: &str, current: &str, strict: bool| {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.package_info_mut().version = current.parse().unwrap_or_else(|e| panic!("{e}"));
        context.config_mut().plugins.0.insert(
            "updater".into(),
            json!({ "pubkey": pubkey, "endpoints": [], "requireSignedVersion": strict, "dangerousInsecureTransportProtocol": true }),
        );
        tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .unwrap_or_else(|e| panic!("{e}"))
    };
    let installed_dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let installed = installed_dir.path().join("synthetic-app");
    std::fs::write(&installed, b"installed app").unwrap_or_else(|e| panic!("{e}"));
    let check = |pubkey: &str, feed: &str, current: &str, strict: bool| {
        let app = app_with_key(pubkey, current, strict);
        let url = format!("{base}/{feed}").parse().unwrap_or_else(|e| panic!("{e}"));
        let updater = updater_builder(app.handle(), DOWNLOAD_STALL_TIMEOUT)
            .unwrap_or_else(|e| panic!("{e}"))
            .endpoints(vec![url])
            .unwrap_or_else(|e| panic!("{e}"))
            // Each published entry names its package type, so a .deb or
            // .rpm install is never offered the AppImage.
            .target("linux-x86_64-appimage")
            .executable_path(&installed)
            .timeout(CHECK_TIMEOUT)
            .no_proxy()
            .build()
            .unwrap_or_else(|e| panic!("{e}"));
        tauri::async_runtime::block_on(find_update(&updater, DOWNLOAD_TIMEOUT)).unwrap_or_else(|e| panic!("{e}"))
    };
    let fetch = |pubkey: &str, feed: &str| {
        let update = check(pubkey, feed, "0.1.1-nightly.20261019223000", true)
            .unwrap_or_else(|| panic!("the newer synthetic version was not offered"));
        assert_eq!(
            update.timeout,
            Some(Duration::from_secs(15 * 60)),
            "package downloads need their own overall deadline"
        );
        assert!(offer_text("0.1.0", &update).contains("Synthetic notes."));
        if cfg!(target_os = "windows") {
            assert!(offer_text("0.1.0", &update).ends_with("Install this update and restart Ibis?"));
        }
        tauri::async_runtime::block_on(update.download(|_, _| {}, || {}))
    };

    let downloaded = fetch(&key, "latest.json").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(downloaded, package);
    assert!(
        fetch(&key, "tampered.json").is_err(),
        "a changed package must be refused"
    );
    // A key the package was not signed with.
    let other_key = String::from_utf8(read("other-key.pub")).unwrap_or_default();
    assert!(
        fetch(&other_key, "latest.json").is_err(),
        "a package signed with another key must be refused"
    );
    assert_eq!(fetch(&key, "prefixed.json").unwrap_or_else(|e| panic!("{e}")), package);
    assert!(matches!(
        fetch(&key, "replay.json"),
        Err(tauri_plugin_updater::Error::SignedVersionMismatch { .. })
    ));
    assert!(matches!(
        fetch(&key, "unversioned.json"),
        Err(tauri_plugin_updater::Error::MissingSignedVersion)
    ));
    assert!(
        fetch(&key, "forged.json").is_err(),
        "trusted comment must be authenticated"
    );
    assert!(
        fetch(&key, "metadata.json").is_err(),
        "build metadata is part of the signed version"
    );
    // Legacy 0.1.x clients accept the bound signature without the flag.
    let legacy =
        check(&key, "latest.json", "0.1.1-nightly.20261019223000", false).unwrap_or_else(|| panic!("legacy offer"));
    assert!(legacy.verify(&package).is_ok());
    let invalid =
        check(&key, "tampered.json", "0.1.1-nightly.20261019223000", true).unwrap_or_else(|| panic!("tampered offer"));
    let finished = AtomicBool::new(false);
    assert!(
        tauri::async_runtime::block_on(invalid.download(|_, _| {}, || finished.store(true, Ordering::SeqCst))).is_err()
    );
    assert!(
        finished.load(Ordering::SeqCst),
        "download finish precedes signature refusal"
    );
    // Both directions of channel substitution, and a genuine downgrade.
    assert!(check(&key, "stable.json", "0.1.1-nightly.20261019223000", true).is_none());
    assert!(check(&key, "latest.json", "0.1.1", true).is_none());
    assert!(check(&key, "latest.json", "9.9.9-nightly.209912312400", true).is_none());
    assert!(check(&key, "stable.json", "10.0.0", true).is_none());
    let stable_update = check(&key, "stable.json", "0.1.1", true).unwrap_or_else(|| panic!("stable offer"));
    assert!(stable_update.verify(&package).is_ok());
    // A cut-off download must fail before any installer is reached; a
    // later complete download succeeds. Cached partial bytes also fail.
    let interrupted = check(&key, "interrupted.json", "0.1.1-nightly.20261019223000", true)
        .unwrap_or_else(|| panic!("interrupted offer"));
    assert!(tauri::async_runtime::block_on(interrupted.download_and_install(|_, _| {}, || {})).is_err());
    assert!(interrupted.install(&package[..package.len() / 2]).is_err());
    assert_eq!(
        std::fs::read(&installed).unwrap_or_else(|e| panic!("{e}")),
        b"installed app"
    );
    assert_eq!(fetch(&key, "latest.json").unwrap_or_else(|e| panic!("{e}")), package);
    // Every request, manifest and package alike, names Ibis and only Ibis.
    let agents = agents.lock().map(|a| a.clone()).unwrap_or_default();
    assert_eq!(agents.len(), 28, "{agents:?}");
    assert!(agents.iter().all(|agent| agent.starts_with("ibis/")), "{agents:?}");
}
