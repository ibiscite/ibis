#![allow(clippy::unwrap_used)]
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Command;

struct Home(tempfile::TempDir);
impl Home {
    fn new() -> Self {
        Self(tempfile::tempdir().unwrap())
    }
    fn settings(&self) -> PathBuf {
        let root = self.0.path();
        let folder = if cfg!(target_os = "macos") {
            root.join("Library/Application Support")
        } else if cfg!(windows) {
            root.join("AppData")
        } else {
            root.join("config")
        };
        folder.join("com.ibiscite.ibis/settings.json")
    }
    fn run(&self, args: &[&str]) -> (i32, Value, String) {
        let root = self.0.path();
        let result = Command::new(env!("CARGO_BIN_EXE_ibis"))
            .args(args)
            .arg("--json")
            .env("HOME", root)
            .env("USERPROFILE", root)
            .env("APPDATA", root.join("AppData"))
            .env("XDG_CONFIG_HOME", root.join("config"))
            .output()
            .unwrap();
        (
            result.status.code().unwrap_or(-1),
            serde_json::from_slice(&result.stdout).unwrap(),
            String::from_utf8(result.stderr).unwrap(),
        )
    }
}

#[test]
fn project_format_and_bibliography_work_without_inheriting_gui_selection() {
    let home = Home::new();
    let bib = home.0.path().join("refs.bib");
    std::fs::write(
        &bib,
        "@book{vale2024,title={Invented Atlas},author={Vale, Ada},year={2024}}\n",
    )
    .unwrap();
    let (code, made, errors) = home.run(&[
        "projects",
        "create",
        "Atlas",
        "--format",
        "cite",
        "--bibliography",
        bib.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{made} {errors}");
    let id = made["data"]["project"]["id"].as_str().unwrap();
    assert!(!made.to_string().contains(home.0.path().to_str().unwrap()));
    let (code, citation, _) = home.run(&["cite", "vale2024", "--project", id, "--file", bib.to_str().unwrap()]);
    assert_eq!(code, 0, "{citation}");
    assert_eq!(citation["data"]["citation"]["text"], "\\cite{vale2024}");
    let (code, ordinary, _) = home.run(&["cite", "vale2024", "--file", bib.to_str().unwrap()]);
    assert_eq!(code, 0, "{ordinary}");
    assert_eq!(ordinary["data"]["text"], "[@vale2024]");
    let (code, refusal, _) = home.run(&[
        "cite",
        "vale2024",
        "--project",
        id,
        "--style",
        "apa",
        "--file",
        bib.to_str().unwrap(),
    ]);
    assert_eq!(code, 2, "{refusal}");
    let (code, renamed, _) = home.run(&["projects", "rename", id, "Revised Atlas"]);
    assert_eq!(code, 0, "{renamed}");
    assert_eq!(renamed["data"]["project"]["id"], id);
    assert_eq!(renamed["data"]["project"]["name"], "Revised Atlas");
    let (code, listed, _) = home.run(&["projects", "list"]);
    assert_eq!(code, 0, "{listed}");
    assert_eq!(listed["data"]["projects"].as_array().unwrap().len(), 1);
    assert_eq!(home.run(&["projects", "remove", id]).0, 0);
    assert_ne!(
        home.run(&["cite", "vale2024", "--project", id, "--file", bib.to_str().unwrap()])
            .0,
        0
    );
    let (code, schema, _) = home.run(&["projects", "schema"]);
    assert_eq!(code, 0, "{schema}");
    assert!(schema["data"]["properties"]["citation"].is_object());
}

#[test]
fn failed_or_busy_writes_never_report_an_unsaved_project() {
    let home = Home::new();
    let path = home.settings();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, json!({"schema_version":2,"density":"compact"}).to_string()).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert_ne!(home.run(&["projects", "create", "", "--format", "pandoc"]).0, 0);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_extension("json.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let (code, failed, _) = home.run(&["projects", "create", "Atlas", "--format", "pandoc"]);
    assert_ne!(code, 0, "{failed}");
    assert_eq!(std::fs::read(&path).unwrap(), before);
    lock.unlock().unwrap();
    assert_eq!(home.run(&["projects", "create", "Atlas", "--format", "pandoc"]).0, 0);
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(&path).unwrap()).unwrap()["density"],
        "compact"
    );
}

#[test]
fn missing_project_bibliography_reports_uncertainty_alongside_the_citation() {
    let home = Home::new();
    let bib = home.0.path().join("refs.bib");
    std::fs::write(&bib, "@book{vale2024,title={Invented Atlas}}\n").unwrap();
    let missing = home.0.path().join("unavailable.bib");
    let (_, made, _) = home.run(&[
        "projects",
        "create",
        "Atlas",
        "--format",
        "pandoc",
        "--bibliography",
        missing.to_str().unwrap(),
    ]);
    let id = made["data"]["project"]["id"].as_str().unwrap();
    let (code, citation, _) = home.run(&["cite", "vale2024", "--project", id, "--file", bib.to_str().unwrap()]);
    assert_eq!(code, 1, "{citation}");
    assert_eq!(citation["data"]["citation"]["text"], "[@vale2024]");
    assert_eq!(
        citation["data"]["project"]["keys"][0]["check"]["status"],
        "could-not-check"
    );
}

#[test]
fn project_formats_accept_the_same_registry_ids_as_ordinary_citations() {
    let home = Home::new();
    for output in ibis_engine::outputs::outputs().unwrap() {
        let (code, made, errors) = home.run(&[
            "projects",
            "create",
            &format!("Project {}", output.id),
            "--format",
            &output.id,
        ]);
        assert_eq!(code, 0, "{}: {made} {errors}", output.id);
        assert_eq!(
            made["data"]["project"]["output"],
            serde_json::to_value(&output.output).unwrap()
        );
    }
}
