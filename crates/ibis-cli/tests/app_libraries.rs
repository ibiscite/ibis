//! With no library named, commands read the libraries chosen in the Ibis app,
//! read-only, and stop with the reason when those settings cannot be used.
#![allow(clippy::unwrap_used)]

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
    assert!(stdout.contains("(chosen in Ibis Nightly)"), "{stdout}");
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
    let (status, stdout, _) = home.run(&["mcp", "--app-libraries", "--json"]);
    assert_eq!((status, code(&stdout).as_str()), (6, "newer-format"));
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
    for (text, expected, exit, words) in [
        ("{\"schema_version\":99}", "newer-format", 6, "Update this ibis command"),
        ("{not json", "bad-input", 3, "Open Ibis to repair"),
        ("{\"schema_version\":2}", "usage", 2, "no library is chosen in Ibis yet"),
        (
            "{\"schema_version\":2,\"library\":{\"bibtex\":\"/invented/elsewhere/refs.bib\"}}",
            "source-unavailable",
            5,
            "It is the library chosen in Ibis;",
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
fn no_settings_ask_for_a_library_by_name() {
    let home = Home::new("none");
    let (status, stdout, stderr) = home.run(&["search", "invented", "--json"]);
    assert_eq!((status, code(&stdout).as_str()), (2, "usage"));
    assert!(stderr.contains("name one with --file PATH"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn settings_this_program_may_not_read_are_explained() {
    use std::os::unix::fs::PermissionsExt;
    let home = Home::new("denied");
    let library = home.library("refs.bib");
    let settings = home.choose(STABLE, &library);
    std::fs::set_permissions(&settings, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(&settings).is_ok() {
        eprintln!("running with permission to read anything; the refusal cannot be shown here");
        return;
    }
    let (status, stdout, stderr) = home.run(&["search", "invented", "--json"]);
    assert_eq!((status, code(&stdout).as_str()), (5, "source-unavailable"));
    assert!(stderr.contains("not allowed to read its settings file"), "{stderr}");
}
