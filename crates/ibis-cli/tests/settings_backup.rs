//! `ibis settings export|import` takes the app's settings to another computer
//! (#1561): a bibliography in the home folder is found again under the other
//! home, and nothing changes until a whole backup has been checked.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

const BIB: &str = "@book{lee2024,\n  title = {An Invented Field Guide},\n  author = {Lee, Ada},\n  year = {2024}\n}\n";
const STABLE: &str = "com.ibiscite.ibis";
const NIGHTLY: &str = "com.ibiscite.ibis.nightly";

/// One invented computer: its own home folder and app settings.
struct Computer {
    root: PathBuf,
}

impl Computer {
    fn new(name: &str) -> Self {
        let root =
            PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("settings-backup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn settings_path(&self, app: &str) -> PathBuf {
        let config = if cfg!(target_os = "macos") {
            self.root.join("Library/Application Support")
        } else if cfg!(windows) {
            self.root.join("AppData")
        } else {
            self.root.join("config")
        };
        config.join(app).join("settings.json")
    }

    fn settings(&self, app: &str, value: &serde_json::Value) -> PathBuf {
        let path = self.settings_path(app);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, value.to_string()).unwrap();
        path
    }

    fn saved(&self, app: &str) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(self.settings_path(app)).unwrap_or_default()).unwrap_or_default()
    }

    fn library(&self) -> PathBuf {
        let path = self.root.join("Papers").join("refs.bib");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, BIB).unwrap();
        path
    }

    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let output = Command::new(env!("CARGO_BIN_EXE_ibis"))
            .args(args)
            .env("HOME", &self.root)
            .env("USERPROFILE", &self.root)
            .env("APPDATA", self.root.join("AppData"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .output()
            .unwrap();
        let text = |bytes: Vec<u8>| String::from_utf8(bytes).unwrap();
        (
            output.status.code().unwrap_or(-1),
            text(output.stdout),
            text(output.stderr),
        )
    }

    fn file(&self, name: &str) -> String {
        self.root.join(name).to_string_lossy().into_owned()
    }
}

fn json(stdout: &str) -> serde_json::Value {
    serde_json::from_str(stdout).unwrap_or_else(|error| panic!("{error}: {stdout}"))
}

#[test]
fn a_backup_restored_on_another_computer_finds_the_library_under_its_home() {
    let first = Computer::new("first");
    let library = first.library();
    first.settings(
        STABLE,
        &serde_json::json!({"library": {"bibtex": library}, "citation_style": "ieee", "update_checks": true,
            "picker_windows": {"display-1": {"width": 600, "height": 400, "custom_size": true, "x": 1, "y": 2}}}),
    );
    let backup = first.file("Ibis settings.json");
    let (code, stdout, stderr) = first.run(&["settings", "export", &backup]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("Ibis settings.json"), "{stdout}");
    assert!(
        !stdout.contains(&*first.root.to_string_lossy()),
        "no private folder is shown: {stdout}"
    );
    let written = std::fs::read_to_string(&backup).unwrap();
    assert!(written.contains("~/Papers/refs.bib"), "{written}");
    assert!(
        !written.contains("picker_windows") && !written.contains("update_checks"),
        "{written}"
    );

    let second = Computer::new("second");
    second.library();
    second.settings(
        STABLE,
        &serde_json::json!({"density": "spacious", "update_checks": false}),
    );
    let before = std::fs::read_to_string(second.settings_path(STABLE)).unwrap();
    let copied = second.file("Ibis settings.json");
    std::fs::copy(&backup, &copied).unwrap();

    // Looking first changes nothing.
    let (code, stdout, stderr) = second.run(&["settings", "import", "--dry-run", &copied]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("Citation style: default → ieee"), "{stdout}");
    assert!(stdout.contains("Library: default → refs.bib"), "{stdout}");
    assert!(stdout.contains("Nothing changed"), "{stdout}");
    assert_eq!(std::fs::read_to_string(second.settings_path(STABLE)).unwrap(), before);

    let (code, stdout, stderr) = second.run(&["settings", "import", &copied, "--json"]);
    assert_eq!(code, 0, "{stderr}");
    let report = json(&stdout);
    assert_eq!(report["data"]["restored"], true);
    assert_eq!(report["data"]["notes"], serde_json::json!([]));
    let settings: Vec<_> = report["data"]["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["setting"].clone())
        .collect();
    assert_eq!(settings, ["citation_style", "density", "library"]);
    assert!(!stdout.contains(&*first.root.to_string_lossy()) && !stdout.contains(&*second.root.to_string_lossy()));
    let saved = second.saved(STABLE);
    assert_eq!(saved["citation_style"], "ieee");
    assert_eq!(
        saved["update_checks"], false,
        "the update question stays this computer's"
    );
    assert!(saved.get("picker_windows").is_none());
    let replaced = second.settings_path(STABLE).with_extension("json.bak");
    assert_eq!(std::fs::read_to_string(replaced).unwrap(), before);

    // The restored library is this computer's own copy.
    let (code, stdout, stderr) = second.run(&["search", "invented", "--json"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("An Invented Field Guide"), "{stdout}");
}

#[test]
fn a_bibliography_missing_on_this_computer_is_restored_with_a_note() {
    let first = Computer::new("missing-first");
    first.settings(STABLE, &serde_json::json!({"library": {"bibtex": first.library()}}));
    let backup = first.file("backup.json");
    assert_eq!(first.run(&["settings", "export", &backup]).0, 0);
    let second = Computer::new("missing-second");
    let (code, stdout, stderr) = second.run(&["settings", "import", &backup]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("not on this computer"), "{stdout}");
    assert_eq!(second.saved(STABLE)["library"]["bibtex"], "~/Papers/refs.bib");
}

#[test]
fn export_goes_to_stdout_and_never_replaces_a_file_unless_asked() {
    let computer = Computer::new("stdout");
    computer.settings(STABLE, &serde_json::json!({"density": "compact", "shortcut": "Alt+K"}));
    let (code, stdout, _) = computer.run(&["settings", "export"]);
    assert_eq!(code, 0);
    let backup = json(&stdout);
    assert_eq!(backup["ibis_settings_backup"], 1);
    assert_eq!(backup["settings"]["density"], "compact");
    assert!(
        backup["settings"]["shortcut"].is_object(),
        "the shortcut names its system"
    );
    let (code, stdout, _) = computer.run(&["settings", "export", "-", "--json"]);
    assert_eq!(code, 0);
    assert_eq!(json(&stdout)["data"]["backup"]["settings"]["density"], "compact");

    let existing = computer.file("existing.json");
    std::fs::write(&existing, "keep me").unwrap();
    let (code, _, stderr) = computer.run(&["settings", "export", &existing]);
    assert_eq!(code, 2, "{stderr}");
    assert!(stderr.contains("--force"), "{stderr}");
    assert_eq!(std::fs::read_to_string(&existing).unwrap(), "keep me");
    assert_eq!(computer.run(&["settings", "export", &existing, "--force"]).0, 0);
    assert!(
        std::fs::read_to_string(&existing)
            .unwrap()
            .contains("ibis_settings_backup")
    );
    let leftovers: Vec<_> = std::fs::read_dir(&computer.root)
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".partial"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn nothing_to_back_up_or_damaged_settings_are_refused_with_the_reason() {
    let computer = Computer::new("nothing");
    let (code, _, stderr) = computer.run(&["settings", "export"]);
    assert_eq!(code, 4, "{stderr}");
    assert!(stderr.contains("no settings on this computer yet"), "{stderr}");
    let path = computer.settings_path(STABLE);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{ damaged").unwrap();
    let (code, _, stderr) = computer.run(&["settings", "export"]);
    assert_eq!(code, 3, "{stderr}");
    assert!(stderr.contains("Open Ibis to repair them"), "{stderr}");
}

#[test]
fn foreign_damaged_and_newer_backups_change_nothing() {
    let computer = Computer::new("refusals");
    computer.settings(STABLE, &serde_json::json!({"density": "spacious"}));
    let before = std::fs::read_to_string(computer.settings_path(STABLE)).unwrap();
    let write = |name: &str, text: &str| {
        let path = computer.file(name);
        std::fs::write(&path, text).unwrap();
        path
    };
    for (path, code, words) in [
        (write("notes.txt", "Invented notes"), 3, "not an Ibis settings backup"),
        (
            computer.settings_path(STABLE).to_string_lossy().into_owned(),
            3,
            "settings file, not a backup",
        ),
        (
            write("newer.json", r#"{"ibis_settings_backup":2}"#),
            6,
            "newer version of Ibis",
        ),
        (
            write(
                "inside.json",
                r#"{"ibis_settings_backup":1,"made_on":"linux","made_by":"0.2.8","settings":{"density":"roomy"}}"#,
            ),
            3,
            "settings in the backup are not valid",
        ),
        (computer.file("absent.json"), 4, "no backup file at that path"),
    ] {
        let (status, stdout, stderr) = computer.run(&["settings", "import", &path]);
        assert_eq!(status, code, "{path}: {stderr}");
        assert!(
            stderr.contains(words) && stderr.contains("Nothing was changed"),
            "{path}: {stderr}"
        );
        assert!(stdout.is_empty());
        assert!(!stderr.contains("roomy"), "no value from the file is repeated");
        assert_eq!(std::fs::read_to_string(computer.settings_path(STABLE)).unwrap(), before);
    }
    // This computer's settings are from a newer Ibis: left alone.
    let backup = computer.file("good.json");
    assert_eq!(computer.run(&["settings", "export", &backup]).0, 0);
    computer.settings(STABLE, &serde_json::json!({"schema_version": 99}));
    let (code, _, stderr) = computer.run(&["settings", "import", &backup]);
    assert_eq!(code, 6, "{stderr}");
    assert_eq!(computer.saved(STABLE), serde_json::json!({"schema_version": 99}));
}

#[test]
fn the_same_backup_twice_changes_nothing_and_nightly_is_chosen_by_name() {
    let computer = Computer::new("again");
    computer.settings(STABLE, &serde_json::json!({"density": "compact"}));
    computer.settings(NIGHTLY, &serde_json::json!({"density": "spacious"}));
    let backup = computer.file("stable.json");
    assert_eq!(computer.run(&["settings", "export", &backup]).0, 0);
    let (code, stdout, _) = computer.run(&["settings", "import", &backup]);
    assert_eq!(code, 0);
    assert!(stdout.contains("already has the settings"), "{stdout}");
    assert!(!computer.settings_path(STABLE).with_extension("json.bak").exists());
    let (code, stdout, stderr) = computer.run(&["settings", "import", &backup, "--nightly"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("Ibis Nightly"), "{stdout}");
    assert_eq!(computer.saved(NIGHTLY)["density"], "compact");
}

#[test]
fn only_nightly_settings_are_used_when_the_stable_app_has_none() {
    let computer = Computer::new("only-nightly");
    computer.settings(NIGHTLY, &serde_json::json!({"larger_text": true}));
    let (code, stdout, _) = computer.run(&["settings", "export", "--json"]);
    assert_eq!(code, 0);
    let report = json(&stdout);
    assert_eq!(report["data"]["app"], "Ibis Nightly");
    assert_eq!(report["data"]["backup"]["settings"]["larger_text"], true);
    assert!(!Path::new(&computer.settings_path(STABLE)).exists());
}
