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
    /// What a person at a terminal reads.
    fn plain(&self, args: &[&str]) -> String {
        let root = self.0.path();
        let result = Command::new(env!("CARGO_BIN_EXE_ibis"))
            .args(args)
            .env("HOME", root)
            .env("USERPROFILE", root)
            .env("APPDATA", root.join("AppData"))
            .env("XDG_CONFIG_HOME", root.join("config"))
            .output()
            .unwrap();
        String::from_utf8(result.stdout).unwrap()
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
fn writing_format_and_bibliography_work_without_inheriting_gui_selection() {
    let home = Home::new();
    let bib = home.0.path().join("refs.bib");
    std::fs::write(
        &bib,
        "@book{vale2024,title={Invented Atlas},author={Vale, Ada},year={2024}}\n",
    )
    .unwrap();
    let (code, made, errors) = home.run(&[
        "writings",
        "create",
        "Atlas",
        "--format",
        "cite",
        "--bibliography",
        bib.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{made} {errors}");
    let id = made["data"]["writing"]["name"].as_str().unwrap();
    assert!(!made.to_string().contains(home.0.path().to_str().unwrap()));
    let (code, citation, _) = home.run(&["cite", "vale2024", "--writing", id, "--file", bib.to_str().unwrap()]);
    assert_eq!(code, 0, "{citation}");
    assert_eq!(citation["data"]["citation"]["text"], "\\cite{vale2024}");
    let (code, ordinary, _) = home.run(&["cite", "vale2024", "--file", bib.to_str().unwrap()]);
    assert_eq!(code, 0, "{ordinary}");
    assert_eq!(ordinary["data"]["text"], "[@vale2024]");
    let settings_path = home.settings();
    let mut settings: Value = serde_json::from_slice(&std::fs::read(&settings_path).unwrap()).unwrap();
    settings["writing_apps"] = json!({"macos:com.microsoft.word": "Atlas"});
    std::fs::write(&settings_path, serde_json::to_vec(&settings).unwrap()).unwrap();
    let (code, for_app, _) = home.run(&[
        "cite",
        "vale2024",
        "--for-app",
        "com.microsoft.word",
        "--file",
        bib.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{for_app}");
    assert_eq!(for_app["data"]["name"], "Atlas");
    assert_eq!(for_app["data"]["citation"]["destination"]["from"], "writing");
    assert_eq!(for_app["data"]["bibliography"]["keys"][0]["check"]["status"], "in");

    let (code, refusal, _) = home.run(&[
        "cite",
        "vale2024",
        "--writing",
        id,
        "--style",
        "apa",
        "--file",
        bib.to_str().unwrap(),
    ]);
    assert_eq!(code, 2, "{refusal}");
    let (code, renamed, _) = home.run(&["writings", "rename", id, "Revised Atlas"]);
    assert_eq!(code, 0, "{renamed}");
    assert_eq!(renamed["data"]["writing"]["name"], "Revised Atlas");
    let (code, listed, _) = home.run(&["writings", "list"]);
    assert_eq!(code, 0, "{listed}");
    assert_eq!(listed["data"]["writings"].as_array().unwrap().len(), 1);
    // The list names each writing's bibliography file, as its help says.
    assert!(home.plain(&["writings", "list"]).trim_end().ends_with("\trefs.bib"));
    assert_eq!(home.run(&["writings", "create", "Loose", "--format", "pandoc"]).0, 0);
    let rows = home.plain(&["writings", "list"]);
    assert!(rows.lines().any(|line| line == "Loose\tPandoc\tno .bib file"), "{rows}");
    assert_eq!(home.run(&["writings", "remove", "Revised Atlas"]).0, 0);
    assert_ne!(
        home.run(&["cite", "vale2024", "--writing", id, "--file", bib.to_str().unwrap()])
            .0,
        0
    );
    let (code, schema, _) = home.run(&["writings", "schema"]);
    assert_eq!(code, 0, "{schema}");
    assert!(schema["data"]["properties"]["citation"].is_object());
}

#[test]
fn an_unknown_writing_citation_key_suggests_without_citing() {
    let home = Home::new();
    let bib = home.0.path().join("refs.bib");
    std::fs::write(&bib, "@book{vale2024,title={Invented Atlas}}\n").unwrap();
    assert_eq!(home.run(&["writings", "create", "Atlas", "--format", "pandoc"]).0, 0);
    let (code, refused, errors) = home.run(&["cite", "vale202", "--writing", "Atlas", "--file", bib.to_str().unwrap()]);
    assert_eq!(code, 4, "{refused} {errors}");
    assert_eq!(refused["ok"], false);
    assert_eq!(refused["error"]["code"], "not-found");
    assert_eq!(refused["error"]["suggestions"], json!(["vale2024"]));
    assert!(errors.contains("Did you mean 'vale2024'?"), "{errors}");
    assert!(refused.get("data").is_none());
}

#[test]
fn failed_or_busy_writes_never_report_an_unsaved_writing() {
    let home = Home::new();
    let path = home.settings();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, json!({"schema_version":2,"density":"compact"}).to_string()).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert_ne!(home.run(&["writings", "create", "", "--format", "pandoc"]).0, 0);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_extension("json.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let (code, failed, _) = home.run(&["writings", "create", "Atlas", "--format", "pandoc"]);
    assert_ne!(code, 0, "{failed}");
    assert_eq!(std::fs::read(&path).unwrap(), before);
    lock.unlock().unwrap();
    assert_eq!(home.run(&["writings", "create", "Atlas", "--format", "pandoc"]).0, 0);
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(&path).unwrap()).unwrap()["density"],
        "compact"
    );
}

#[test]
fn an_unreadable_writing_bibliography_is_a_warning() {
    let home = Home::new();
    let bib = home.0.path().join("refs.bib");
    std::fs::write(&bib, "@book{vale2024,title={Invented Atlas}}\n").unwrap();
    let missing = home.0.path().join("unavailable.bib");
    let (_, made, _) = home.run(&[
        "writings",
        "create",
        "Atlas",
        "--format",
        "pandoc",
        "--bibliography",
        missing.to_str().unwrap(),
    ]);
    let id = made["data"]["writing"]["name"].as_str().unwrap();
    let (code, citation, _) = home.run(&["cite", "vale2024", "--writing", id, "--file", bib.to_str().unwrap()]);
    assert_eq!(code, 1, "{citation}");
    assert_eq!(citation["data"]["citation"]["text"], "[@vale2024]");
    assert_eq!(citation["data"]["assessment"]["state"], "ready");
    assert_eq!(
        citation["data"]["bibliography"]["keys"][0]["check"]["status"],
        "could-not-check"
    );
    let (code, strict, _) = home.run(&[
        "cite",
        "vale2024",
        "--writing",
        id,
        "--strict",
        "--file",
        bib.to_str().unwrap(),
    ]);
    assert_eq!(code, 3, "{strict}");
    assert_eq!(strict["ok"], false);
    assert_eq!(strict["error"]["reason"], "strict-citation");
    assert!(strict.get("data").is_none());
}

#[test]
fn a_known_wrong_work_key_is_refused() {
    let home = Home::new();
    let library = home.0.path().join("library.bib");
    let writing = home.0.path().join("refs.bib");
    std::fs::write(
        &library,
        "@book{vale2024,title={Intended Work},doi={10.1000/intended}}\n",
    )
    .unwrap();
    std::fs::write(&writing, "@book{vale2024,title={Other Work},doi={10.1000/other}}\n").unwrap();
    let (_, made, _) = home.run(&[
        "writings",
        "create",
        "Atlas",
        "--format",
        "pandoc",
        "--bibliography",
        writing.to_str().unwrap(),
    ]);
    let id = made["data"]["writing"]["name"].as_str().unwrap();
    for flags in [&[][..], &["--strict"][..]] {
        let mut args = vec!["cite", "vale2024", "--writing", id, "--file", library.to_str().unwrap()];
        args.extend_from_slice(flags);
        let (code, refused, _) = home.run(&args);
        if flags.is_empty() {
            assert_eq!(code, 1, "{refused}");
            assert_eq!(refused["data"]["assessment"]["state"], "conflict");
            assert_eq!(refused["data"]["citation"]["text"], "");
        } else {
            assert_eq!(code, 3, "{refused}");
            assert_eq!(refused["ok"], false);
            assert!(refused.get("data").is_none());
        }
    }
}

#[test]
fn writing_formats_accept_the_same_registry_ids_as_ordinary_citations() {
    let home = Home::new();
    for output in ibis_engine::outputs::outputs() {
        let (code, made, errors) = home.run(&[
            "writings",
            "create",
            &format!("Project {}", output.id),
            "--format",
            &output.id,
        ]);
        assert_eq!(code, 0, "{}: {made} {errors}", output.id);
        assert_eq!(
            made["data"]["writing"]["format"],
            serde_json::to_value(&output.output).unwrap()
        );
    }
}

#[test]
fn an_explicit_document_frontmatter_selects_a_writing_and_ambiguous_context_refuses() {
    let home = Home::new();
    let bibliography = home.0.path().join("refs.bib");
    let document = home.0.path().join("chapter.qmd");
    std::fs::write(&bibliography, "@book{vale2024,title={Invented Atlas}}\n").unwrap();
    std::fs::write(&document, "---\nbibliography: refs.bib\n---\nInvented text\n").unwrap();
    assert_eq!(
        home.run(&[
            "writings",
            "create",
            "Thesis",
            "--format",
            "cite",
            "--bibliography",
            bibliography.to_str().unwrap()
        ])
        .0,
        0
    );
    let args = [
        "cite",
        "vale2024",
        "--document",
        document.to_str().unwrap(),
        "--file",
        bibliography.to_str().unwrap(),
    ];
    let (code, citation, _) = home.run(&args);
    assert_eq!(code, 0, "{citation}");
    assert_eq!(citation["data"]["name"], "Thesis");
    assert_eq!(citation["data"]["citation"]["text"], "\\cite{vale2024}");
    assert_eq!(
        home.run(&[
            "writings",
            "create",
            "Article",
            "--format",
            "pandoc",
            "--bibliography",
            bibliography.to_str().unwrap()
        ])
        .0,
        0
    );
    let (code, refused, _) = home.run(&args);
    assert_eq!(code, 3, "{refused}");
    assert_eq!(refused["error"]["code"], "bad-input");
}
