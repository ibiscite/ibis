//! Your styles (#19, decision 0007): `ibis styles --keep`, `--add` and
//! `--remove`. A style of yours wins over the bundled style with its id, keeps
//! the forms its rules were checked for, and is refused rather than replaced
//! by the bundled style when it cannot be used. Each case is its own computer.
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;

const BIB: &str = "@book{example2026,\n  author = {Example, Avery},\n  title = {Invented Title},\n  publisher = {Invented Press},\n  year = {2026}\n}\n";
/// APA's citation layout, which a copy of yours changes to square brackets.
const ROUND: &str = r#"<layout delimiter="; " prefix="(" suffix=")">"#;
const SQUARE: &str = r#"<layout delimiter="; " prefix="[" suffix="]">"#;

struct Computer {
    root: PathBuf,
}

impl Computer {
    fn new(name: &str) -> Self {
        let root =
            PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("your-styles-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("refs.bib"), BIB).unwrap();
        Self { root }
    }

    fn styles(&self) -> PathBuf {
        let config = if cfg!(target_os = "macos") {
            self.root.join("Library/Application Support")
        } else if cfg!(windows) {
            self.root.join("AppData")
        } else {
            self.root.join("config")
        };
        config.join("com.ibiscite.ibis").join("styles")
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

    fn cite(&self, style: &str, form: &str) -> (i32, String, String) {
        let file = self.root.join("refs.bib");
        self.run(&[
            "cite",
            "example2026",
            "--file",
            file.to_str().unwrap(),
            "--style",
            style,
            "--form",
            form,
        ])
    }

    /// A file outside your styles, as a person would download or edit one.
    fn file(&self, name: &str, text: &str) -> String {
        let path = self.root.join(name);
        std::fs::write(&path, text).unwrap();
        path.to_str().unwrap().to_owned()
    }
}

#[test]
fn a_kept_style_stays_as_it_was_and_keeps_its_checked_forms() {
    let computer = Computer::new("keep");
    let (_, bundled, _) = computer.cite("apa", "normal");
    let (code, text, _) = computer.run(&["styles", "--keep", "apa"]);
    assert_eq!(code, 0);
    assert_eq!(
        text,
        "Kept apa as it is now: later versions of Ibis will not change it.\n"
    );
    assert_eq!(
        computer.run(&["styles", "--keep", "apa"]).1,
        "You already keep this version of apa.\n"
    );
    // The same citation, and the forms checked against Zotero for these rules.
    assert_eq!(computer.cite("apa", "normal").1, bundled);
    assert_eq!(
        computer.cite("apa", "author-only"),
        (0, "Example\n".into(), String::new())
    );
    let (_, list, _) = computer.run(&["styles", "apa"]);
    assert!(list.lines().next().unwrap().starts_with("apa\t"), "{list}");
    assert!(list.lines().next().unwrap().ends_with("\tyours"), "{list}");
}

#[test]
fn keeping_a_dependent_style_keeps_the_rules_it_uses() {
    let computer = Computer::new("dependent");
    let (code, text, _) = computer.run(&["styles", "--keep", "nature-communications"]);
    assert_eq!(code, 0);
    assert!(
        text.ends_with("It uses the rules of nature, which is kept too.\n"),
        "{text}"
    );
    assert!(computer.styles().join("nature.csl").is_file());
    let (code, removed, _) = computer.run(&["styles", "--remove", "nature"]);
    assert_eq!(code, 0);
    assert!(
        removed.ends_with("Your nature-communications used its rules.\n"),
        "{removed}"
    );
    // It falls back to the bundled rules, which still exist.
    assert_eq!(computer.cite("nature-communications", "normal").0, 0);
}

#[test]
fn a_style_you_add_wins_until_you_remove_it() {
    let computer = Computer::new("add");
    computer.run(&["styles", "--keep", "apa"]);
    let yours = std::fs::read_to_string(computer.styles().join("apa.csl")).unwrap();
    let edited = yours.replace(ROUND, SQUARE).replace(
        "<title>APA Style 7th edition</title>",
        "<title>Invented house style</title>",
    );
    assert_ne!(edited, yours);
    // Keeping refuses to replace a version of yours; adding replaces it, saying so.
    let path = computer.file("house.csl", &edited);
    assert_eq!(
        computer.run(&["styles", "--add", &path]).1,
        "Replaced your apa with this file.\n"
    );
    assert_eq!(computer.cite("apa", "normal").1, "[Example, 2026]\n");
    let (code, _, error) = computer.run(&["styles", "--keep", "apa"]);
    assert_eq!(code, 3);
    assert!(
        error.contains("You already have a different version of this style."),
        "{error}"
    );
    // Changed rules lose the forms that were checked for the bundled ones,
    // and the list of outputs says so.
    assert_ne!(computer.cite("apa", "author-only").0, 0);
    let (_, outputs, _) = computer.run(&["outputs", "--json"]);
    let outputs: serde_json::Value = serde_json::from_str(&outputs).unwrap();
    let apa = outputs["data"]["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|output| output["id"] == "apa")
        .unwrap();
    assert!(
        !apa["forms"].as_array().unwrap().contains(&"author-only".into()),
        "{apa}"
    );
    let (_, list, _) = computer.run(&["styles", "apa"]);
    assert!(list.starts_with("apa\tInvented house style\t2026-"), "{list}");
    assert_eq!(computer.run(&["styles", "--remove", "apa"]).0, 0);
    assert_eq!(computer.cite("apa", "normal").1, "(Example, 2026)\n");
    assert_eq!(
        computer.run(&["styles", "--remove", "apa"]).1,
        "You have no style called apa.\n"
    );
}

#[test]
fn a_file_that_is_not_a_usable_style_is_refused_and_nothing_is_saved() {
    let computer = Computer::new("refused");
    computer.run(&["styles", "--keep", "apa"]);
    let apa = std::fs::read_to_string(computer.styles().join("apa.csl")).unwrap();
    let id = r"<id>http://www.zotero.org/styles/apa</id>";
    let as_new = |id_text: &str| apa.replace(id, id_text);
    for (name, text) in [
        ("empty.csl", String::new()),
        ("text.csl", "Not a style".to_owned()),
        ("no-id.csl", as_new("")),
        ("bad-id.csl", as_new("<id>http://www.zotero.org/styles/Invented Style</id>")),
        (
            "lost-parent.csl",
            r#"<style xmlns="http://purl.org/net/xbiblio/csl" version="1.0"><info><title>Invented</title><id>invented-journal</id><link href="http://www.zotero.org/styles/invented-missing-parent" rel="independent-parent"/><updated>2026-01-01T00:00:00+00:00</updated></info></style>"#.to_owned(),
        ),
        (
            "unknown-language.csl",
            as_new("<id>invented-language</id>").replacen("<style ", r#"<style default-locale="xx-INVENTED" "#, 1),
        ),
        ("huge.csl", format!("{}{}", as_new("<id>invented-huge</id>"), " ".repeat(1024 * 1024))),
    ] {
        let path = computer.file(name, &text);
        let (code, _, error) = computer.run(&["styles", "--add", &path]);
        assert_eq!(code, 3, "{name}: {error}");
        assert!(!error.is_empty(), "{name}");
    }
    let mut saved: Vec<_> = std::fs::read_dir(computer.styles())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    saved.sort();
    assert_eq!(saved, ["apa.csl"]);
}

#[test]
fn a_copy_of_yours_that_cannot_be_used_is_refused_never_replaced_by_the_bundled_one() {
    let computer = Computer::new("broken");
    computer.run(&["styles", "--keep", "apa"]);
    let path = computer.styles().join("apa.csl");
    let apa = std::fs::read_to_string(&path).unwrap();
    // Edited by hand to describe another style, then emptied.
    for broken in [
        apa.replace("styles/apa</id>", "styles/apa-invented</id>"),
        String::new(),
    ] {
        std::fs::write(&path, broken).unwrap();
        let (code, text, error) = computer.cite("apa", "normal");
        assert_eq!((code, text.as_str()), (3, ""), "{error}");
        assert!(error.contains("This style cannot be used"), "{error}");
    }
    #[cfg(unix)]
    {
        // A link could point at any file; it is refused, not followed.
        std::fs::write(computer.root.join("elsewhere.csl"), &apa).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(computer.root.join("elsewhere.csl"), &path).unwrap();
        assert_eq!(computer.cite("apa", "normal").0, 3);
    }
}

#[test]
fn a_running_server_cites_with_your_style_as_it_is_now() {
    use std::io::{BufRead, BufReader, Write};
    let computer = Computer::new("running");
    let file = computer.root.join("refs.bib");
    let mut child = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .args(["mcp", "--file", file.to_str().unwrap()])
        .env("HOME", &computer.root)
        .env("USERPROFILE", &computer.root)
        .env("APPDATA", computer.root.join("AppData"))
        .env("XDG_CONFIG_HOME", computer.root.join("config"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut replies = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut call = |id: u64, method: &str, params: serde_json::Value| {
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
        )
        .unwrap();
        let reply: serde_json::Value = serde_json::from_str(&replies.next().unwrap().unwrap()).unwrap();
        reply["result"].clone()
    };
    call(
        1,
        "initialize",
        serde_json::json!({"protocolVersion":ibis_engine::mcp::LEGACY_VERSIONS[0],"capabilities":{},"clientInfo":{"name":"test","version":"1"}}),
    );
    let found = call(
        2,
        "tools/call",
        serde_json::json!({"name":"search","arguments":{"query":"invented"}}),
    );
    let reference = found["structuredContent"]["hits"][0]["ref"].clone();
    let mut cite = |id: u64| {
        let cited = call(
            id,
            "tools/call",
            serde_json::json!({"name":"cite","arguments":{"items":[{"ref":reference}],"style":"apa"}}),
        );
        cited["structuredContent"]["text"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    };
    assert_eq!(cite(3), "(Example, 2026)");
    // Another process changes your styles while this one keeps its warm style.
    computer.run(&["styles", "--keep", "apa"]);
    let path = computer.styles().join("apa.csl");
    let square = std::fs::read_to_string(&path).unwrap().replace(ROUND, SQUARE);
    std::fs::write(&path, square).unwrap();
    assert_eq!(cite(4), "[Example, 2026]");
    std::fs::remove_file(&path).unwrap();
    assert_eq!(cite(5), "(Example, 2026)");
    drop(stdin);
    child.wait().unwrap();
}
