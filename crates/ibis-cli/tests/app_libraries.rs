//! With no library named, commands read the libraries chosen in the Ibis app,
//! read-only, and stop with the reason when those settings cannot be used.
#![cfg(test)]

use std::path::{Path, PathBuf};
use std::process::Command;

const BIB: &str = "@book{lee2024,\n  title = {An Invented Field Guide},\n  author = {Lee, Ada},\n  year = {2024}\n}\n";
const STABLE: &str = "com.ibiscite.ibis";
const NIGHTLY: &str = "com.ibiscite.ibis.nightly";

/// A home of its own for each scenario, so no real settings are ever read.
struct Home {
    root: PathBuf,
}

impl Home {
    fn new(name: &str) -> Self {
        let root =
            PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("app-libraries-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    /// Where the app keeps its settings on this platform.
    fn config(&self) -> PathBuf {
        if cfg!(target_os = "macos") {
            self.root.join("Library/Application Support")
        } else if cfg!(windows) {
            self.root.join("AppData")
        } else {
            self.root.join("config")
        }
    }

    fn settings(&self, app: &str, text: &str) -> PathBuf {
        let path = self.config().join(app).join("settings.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }

    fn library(&self, name: &str) -> PathBuf {
        let path = self.root.join(name);
        std::fs::write(&path, BIB).unwrap();
        path
    }

    fn choose(&self, app: &str, library: &Path) -> PathBuf {
        let value = serde_json::json!({"schema_version": 2, "library": {"bibtex": library}});
        self.settings(app, &value.to_string())
    }

    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let output = Command::new(env!("CARGO_BIN_EXE_ibis"))
            .args(args)
            .env("HOME", &self.root)
            .env("USERPROFILE", &self.root)
            .env("APPDATA", self.root.join("AppData"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8(output.stdout).unwrap(),
            String::from_utf8(output.stderr).unwrap(),
        )
    }
}

fn code(stdout: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(stdout).unwrap();
    value["error"]["code"].as_str().unwrap_or_default().to_owned()
}

#[test]
fn the_library_chosen_in_the_app_is_read_when_none_is_named() {
    let home = Home::new("chosen");
    let library = home.library("refs.bib");
    home.choose(NIGHTLY, &library);
    let (status, stdout, stderr) = home.run(&["cite", "lee2024", "--style", "apa"]);
    assert_eq!((status, stderr.as_str()), (0, ""), "{stdout}");
    assert!(stdout.contains("Lee"), "{stdout}");
    let (status, stdout, _) = home.run(&["libraries"]);
    assert_eq!(status, 0);
    assert!(stdout.contains("Library chosen in Ibis Nightly"), "{stdout}");
    assert!(
        !stdout.contains("refs.bib"),
        "an app-chosen path is never shown: {stdout}"
    );
    // The stable app wins when both have settings, even when Nightly chose something else.
    let other = home.root.join("other.bib");
    std::fs::write(&other, BIB.replace("lee2024", "kim2023")).unwrap();
    home.choose(STABLE, &other);
    let (status, stdout, _) = home.run(&["search", "invented", "--json"]);
    assert_eq!(status, 0);
    assert!(stdout.contains("kim2023") && !stdout.contains("lee2024"), "{stdout}");
}

#[test]
fn named_libraries_replace_the_app_and_servers_read_it_only_when_asked() {
    let home = Home::new("named");
    let settings = home.settings(STABLE, "{\"schema_version\":99}");
    let library = home.library("refs.bib");
    let file = library.to_str().unwrap();
    // Unusable settings are never read when a library is named, or by servers and
    // commands that need no library.
    for args in [
        vec!["search", "invented", "--file", file],
        vec!["styles"],
        vec!["schema", "search"],
        vec!["mcp"],
    ] {
        let (status, _, stderr) = home.run(&args);
        assert_eq!((status, stderr.as_str()), (0, ""), "{args:?}");
    }
    // The MCP server starts and says why through its tools; closing stdin ends it.
    let (status, stdout, stderr) = home.run(&["mcp", "--app-libraries", "--json"]);
    assert_eq!((status, stdout.as_str()), (0, ""));
    assert!(stderr.contains("newer version of Ibis"), "{stderr}");
    let (status, stdout, _) = home.run(&["libraries", "--app-libraries", "--file", file, "--json"]);
    assert_eq!((status, code(&stdout).as_str()), (6, "newer-format"));
    // Reading never changes the settings file.
    assert_eq!(std::fs::read_to_string(settings).unwrap(), "{\"schema_version\":99}");
}

#[test]
fn unusable_settings_stop_with_their_reason_and_never_fall_back() {
    let home = Home::new("unusable");
    let library = home.library("refs.bib");
    // A good Nightly choice must not hide a broken stable one.
    home.choose(NIGHTLY, &library);
    // A full path on every platform; "/invented/..." is relative on Windows.
    let missing = serde_json::json!({
        "schema_version": 2,
        "library": {"bibtex": home.root.join("private-project").join("refs.bib")},
    })
    .to_string();
    for (text, expected, exit, words) in [
        ("{\"schema_version\":99}", "newer-format", 6, "Update this ibis command"),
        ("{not json", "bad-input", 3, "Open Ibis to repair"),
        ("{\"schema_version\":2}", "usage", 2, "no library is chosen in Ibis yet"),
        (
            missing.as_str(),
            "source-unavailable",
            5,
            "cannot open the library chosen in Ibis: The library file was moved",
        ),
        (
            "{\"schema_version\":2,\"library\":{\"bibtex\":\"private-project/refs.bib\"}}",
            "bad-input",
            3,
            "not a full path",
        ),
        (
            "{\"schema_version\":2,\"density\":\"PRIVATE_SETTINGS_VALUE\"}",
            "bad-input",
            3,
            "its settings file is not valid",
        ),
        (
            "{\"schema_version\":2,\"library\":{\"zotero\":\"user\"}}",
            "source-unavailable",
            5,
            "cannot read the Zotero library",
        ),
    ] {
        home.settings(STABLE, text);
        let (status, stdout, stderr) = home.run(&["search", "invented", "--json"]);
        let found = code(&stdout);
        assert!(
            stderr.starts_with("ibis: ") && stderr.contains(words),
            "{text}: {stderr}"
        );
        assert_eq!(stderr.lines().count(), 1, "{stderr}");
        // Nothing from inside the settings reaches stderr, which agents read too.
        assert!(
            !stderr.contains("private-project") && !stderr.contains("PRIVATE"),
            "{stderr}"
        );
        assert_eq!(found, expected, "{text}: {stderr}");
        assert_eq!(status, exit, "{text}");
    }
}

#[test]
fn a_backup_is_not_used_in_place_of_missing_settings() {
    let home = Home::new("backup");
    let library = home.library("refs.bib");
    let settings = home.choose(STABLE, &library);
    std::fs::rename(&settings, settings.with_extension("json.bak")).unwrap();
    let (status, stdout, stderr) = home.run(&["search", "invented", "--json"]);
    assert_eq!((status, code(&stdout).as_str()), (5, "source-unavailable"), "{stderr}");
    assert!(stderr.contains("does not use its backup"), "{stderr}");
    assert!(!settings.exists(), "the backup must not be restored");
}

#[test]
fn the_plugin_declares_local_execution_and_ships_its_listing_icons() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/ibis");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("plugin.json")).unwrap()).unwrap();
    let openai = &manifest["extensions"]["com.openai"];
    assert_eq!(openai["requires_local_executor"], true);
    let interface = &openai["interface"];
    for (field, limit) in [("displayName", 30), ("shortDescription", 30), ("longDescription", 4000)] {
        let text = interface[field].as_str().unwrap();
        assert!(!text.is_empty() && text.chars().count() <= limit, "{field}");
    }
    for field in ["logo", "composerIcon"] {
        let path = interface[field].as_str().unwrap();
        assert!(path.starts_with("./"));
        assert!(root.join(path).is_file(), "missing {field}");
    }
}

#[test]
fn plugin_versions_move_with_each_release() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let json =
        |path: &str| -> serde_json::Value { serde_json::from_slice(&std::fs::read(root.join(path)).unwrap()).unwrap() };
    let extra_files = json("release-please-config.json")["packages"]["."]["extra-files"].clone();
    for manifest in ["plugins/ibis/plugin.json", "plugins/ibis/.claude-plugin/plugin.json"] {
        assert_eq!(json(manifest)["version"], ibis_core::VERSION, "{manifest}");
        assert!(
            extra_files
                .as_array()
                .unwrap()
                .iter()
                .any(|file| file["path"] == manifest),
            "the release does not update {manifest}"
        );
    }
}
#[cfg(unix)]
#[test]
fn a_named_pipe_is_refused_without_waiting_for_a_writer() {
    let home = Home::new("pipe");
    let pipe = home.root.join("refs.bib");
    assert!(Command::new("mkfifo").arg(&pipe).status().unwrap().success());
    let mut child = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .args(["search", "invented", "--json", "--file"])
        .arg(&pipe)
        .env("HOME", &home.root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            panic!("ibis waited on a named pipe");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    assert!(!status.success());
}

#[test]
fn doctor_reports_broken_requested_app_settings_even_with_a_healthy_named_file() {
    let home = Home::new("doctor-broken-and-named");
    let library = home.library("invented.bib");
    let file = library.to_str().unwrap();
    for (settings, code, words) in [
        ("{\"schema_version\":99}", "newer-format", "Update this ibis command"),
        ("{not json", "bad-input", "Open Ibis to repair"),
        (
            "{\"schema_version\":2}",
            "no-library",
            "no library is chosen in Ibis yet",
        ),
    ] {
        home.settings(STABLE, settings);
        let (status, stdout, stderr) = home.run(&["doctor", "--app-libraries", "--file", file, "--json"]);
        assert_eq!(status, 1, "{stdout}{stderr}");
        let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(report["data"]["library_check"]["status"], "failed");
        assert_eq!(report["data"]["library_check"]["code"], code);
        assert!(
            report["data"]["library_check"]["message"]
                .as_str()
                .unwrap()
                .contains(words),
            "{stdout}"
        );
        assert_eq!(report["data"]["sources"][0]["status"], "ok");
        let (status, stdout, _) = home.run(&["doctor", "--app-libraries", "--file", file]);
        assert_eq!(status, 1, "{stdout}");
        assert!(
            stdout.contains(words) && stdout.contains("fail  Libraries:"),
            "{stdout}"
        );
        assert!(!stdout.contains("All good"), "{stdout}");
    }
}
