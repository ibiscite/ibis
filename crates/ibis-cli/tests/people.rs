//! What a person reads in a terminal: each message must be true, and each next
//! step must work when followed. Runs the program with an empty home folder.
#![cfg(test)]
use std::path::{Path, PathBuf};
use std::process::Command;

const BIB: &str = r"@article{vale2024lanterns,
  title = {Lanterns on the Tidal Marsh},
  author = {Vale, Mira and van der Berg, Anna},
  journal = {Journal of Invented Studies},
  volume = {12}, pages = {45--67}, year = {2024}, doi = {10.1000/invented.1},
  note = {Invented private note}
}
@book{many2020, title={Many Hands}, author={Ash, Ada and Birch, Bo and Cedar, Cy and Dale, Di and Elm, Ed}, year=2020}
@article{same, title={First}, year=2000}
@article{same, title={Second}, year=2001}
";

struct Lab(PathBuf);

impl Lab {
    fn new(name: &str) -> Self {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("people-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn file(&self, name: &str, contents: &str) -> String {
        let path = self.0.join(name);
        std::fs::write(&path, contents).unwrap();
        path.to_str().unwrap().to_owned()
    }
    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let output = Command::new(env!("CARGO_BIN_EXE_ibis"))
            .args(args)
            .env("HOME", &self.0)
            .env("USERPROFILE", &self.0)
            .env("APPDATA", self.0.join("AppData"))
            .env("XDG_CONFIG_HOME", self.0.join("config"))
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

#[test]
fn ibis_alone_shows_what_it_can_do() {
    let lab = Lab::new("alone");
    let (code, stdout, stderr) = lab.run(&[]);
    assert_eq!((code, stdout.as_str()), (2, ""));
    assert!(stderr.contains("Commands:") && stderr.contains("search"), "{stderr}");
    assert!(!stderr.contains("requires a subcommand"), "{stderr}");
    let (code, _, stderr) = lab.run(&["settings"]);
    assert_eq!(code, 2);
    assert!(stderr.contains("export"), "{stderr}");
    let (code, stdout, _) = lab.run(&["--json"]);
    assert_eq!(code, 2);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&stdout).unwrap(),
        serde_json::json!({
            "ibis":1,"ok":false,"error":{"code":"usage", "message":{"id":"error-usage","args":{}},
            "recovery":{"action":"edit-input"},"focus":"request"}
        })
    );
}

#[test]
fn a_file_with_no_entries_is_not_called_a_failed_search() {
    let lab = Lab::new("text");
    // A `.txt` must be RIS (#1531); a `.bib` with no entries is only empty.
    let file = lab.file("notes.bib", "Shopping list\nno references here\n");
    let (code, stdout, _) = lab.run(&["search", "shopping", "--file", &file]);
    assert_eq!(code, 0);
    assert!(stdout.contains("holds any references"), "{stdout}");
    assert!(stdout.contains("open another library with --file"), "{stdout}");
}

#[test]
fn an_unknown_file_is_refused_with_supported_formats_and_a_next_step() {
    let lab = Lab::new("unknown-format");
    let file = lab.file("notes.xyz", "Invented notes without references.\n");
    let (code, stdout, stderr) = lab.run(&["search", "x", "--file", &file]);
    assert_eq!((code, stdout.as_str()), (3, ""), "{stderr}");
    for words in [
        "notes.xyz",
        "not in a format Ibis reads",
        "BibTeX",
        "BibLaTeX",
        "CSL-JSON",
        "RIS",
        "CSV",
        "Choose another file",
    ] {
        assert!(stderr.contains(words), "{stderr}");
    }
    assert!(!stderr.contains("holds any references"), "{stderr}");
    let (code, stdout, _) = lab.run(&["search", "x", "--file", &file, "--json"]);
    assert_eq!(code, 3);
    let response: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(response["error"]["code"], "bad-input");
    assert_eq!(response["error"]["reason"], "unknown-format");
}

#[test]
fn show_prints_a_reference_for_reading_and_json_keeps_everything() {
    let lab = Lab::new("show");
    let file = lab.file("refs.bib", BIB);
    let (_, stdout, _) = lab.run(&["show", "vale2024lanterns", "--file", &file]);
    for line in [
        "vale2024lanterns",
        "Title: Lanterns on the Tidal Marsh",
        "Authors: Mira Vale, Anna van der Berg",
        "Year: 2024",
        "Published in: Journal of Invented Studies",
        "Pages: 45–67",
        "DOI: 10.1000/invented.1",
    ] {
        assert!(stdout.contains(line), "{line}: {stdout}");
    }
    assert!(!stdout.contains("fingerprints"), "{stdout}");
    assert!(stdout.lines().count() < 12, "{stdout}");
    let (_, stdout, _) = lab.run(&["show", "many2020", "--file", &file]);
    assert!(
        stdout.contains("Authors: Ada Ash, Bo Birch, Cy Cedar, Di Dale, Ed Elm"),
        "{stdout}"
    );
    let (_, stdout, _) = lab.run(&["search", "many", "--file", &file]);
    assert!(stdout.contains("Ada Ash, Bo Birch, Cy Cedar, et al."), "{stdout}");
    let (_, stdout, _) = lab.run(&["show", "vale2024lanterns", "--file", &file, "--json"]);
    assert!(stdout.contains("fingerprints"), "{stdout}");
}

#[test]
fn a_damaged_entry_says_why_it_was_skipped_and_which_field_to_fix() {
    let lab = Lab::new("damage-causes");
    let damaged = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/bibtex/damaged.bib");
    let file = damaged.to_str().unwrap();
    let (code, stdout, stderr) = lab.run(&["libraries", "--file", file]);
    assert_eq!(code, 1, "{stdout}");
    assert!(
        !stderr.contains("see: ibis libraries"),
        "it is already the list: {stderr}"
    );
    assert!(
        stdout.contains("line 21: an entry could not be read, so Ibis skips it (it has no citation key)"),
        "{stdout}"
    );
    assert!(
        stdout.contains("line 25: an entry could not be read, so Ibis skips it (it never closes)"),
        "{stdout}"
    );
    assert!(
        stdout.contains("(journal uses an abbreviation that is not defined"),
        "{stdout}"
    );
    // Clients get the same as fixed codes and a field name, never the entry's text.
    let (_, json, _) = lab.run(&["libraries", "--file", file, "--json"]);
    let warnings: serde_json::Value =
        serde_json::from_str::<serde_json::Value>(&json).unwrap()["data"]["libraries"][0]["warnings"].clone();
    assert!(
        warnings
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["cause"] == "abbreviation" && w["field"] == "journal" && w["line"] == 15),
        "{warnings}"
    );
    assert!(!json.contains("notdefined"), "{json}");
    // Its key still finds the reason, though no warning carries the key.
    assert!(!json.contains("unclosed"), "{json}");
    let (code, _, stderr) = lab.run(&["cite", "unclosed", "--file", file]);
    assert_eq!(code, 4, "{stderr}");
    assert!(
        stderr.contains("the entry 'unclosed' on line 25 could not be read, so Ibis skips it (it never closes); fix it in your library."),
        "{stderr}"
    );
    let (_, json, _) = lab.run(&["cite", "unclosed", "--file", file, "--json"]);
    let error = &serde_json::from_str::<serde_json::Value>(&json).unwrap()["error"];
    assert_eq!(
        (&error["code"], &error["reason"]),
        (&"not-found".into(), &"damaged-entry".into())
    );
}

#[test]
fn damaged_library_diagnostics_do_not_echo_unknown_field_names() {
    let lab = Lab::new("private-warning-field");
    let file = lab.file(
        "refs.json",
        r#"[{"id":"one","type":"book","title":"Invented reference","invented-private-field":true}]"#,
    );
    let (_, human, _) = lab.run(&["libraries", "--file", &file]);
    assert!(!human.contains("invented-private-field"), "{human}");
    let (_, json, _) = lab.run(&["libraries", "--file", &file, "--json"]);
    let response: serde_json::Value = serde_json::from_str(&json).unwrap();
    let warnings = response["data"]["libraries"][0]["warnings"].as_array().unwrap();
    assert!(warnings.iter().any(|warning| warning["cause"] == "unknown-field"));
    assert!(!json.contains("invented-private-field"), "{json}");
}

#[test]
fn a_key_past_the_part_ibis_could_read_says_the_library_was_cut_short() {
    let lab = Lab::new("cut-short");
    let entries: String = (0..=200_000)
        .map(|i| format!("@misc{{k{i}, title={{T {i}}}}}\n"))
        .collect();
    let file = lab.file("large.bib", &entries);
    let (code, _, stderr) = lab.run(&["cite", "k200000", "--style", "apa", "--file", &file]);
    assert_eq!(code, 4, "{stderr}");
    assert!(stderr.contains("too large to read in full"), "{stderr}");
    assert!(stderr.contains("Split the library"), "{stderr}");
}

#[test]
fn attachment_results_name_available_missing_and_unchecked_references() {
    let lab = Lab::new("attachment-probing");
    lab.file("available.pdf", "invented attachment");
    let records: Vec<_> = (0..500)
        .map(|index| {
            serde_json::json!({
                "id": format!("item{index:03}"), "type": "book",
                "file": if index == 1 { "missing.pdf" } else { "available.pdf" }
            })
        })
        .collect();
    let file = lab.file("invented.json", &serde_json::to_string(&records).unwrap());
    let (code, stdout, stderr) = lab.run(&["libraries", "--file", &file, "--json"]);
    assert_eq!(code, 0, "{stderr}");
    let library = serde_json::from_str::<serde_json::Value>(&stdout).unwrap()["data"]["libraries"][0]["id"].clone();
    for (index, expected) in [(0, "on-disk"), (1, "broken-link"), (499, "unknown")] {
        let id =
            serde_json::json!({"library": library, "key": format!("attachment:record:item{index:03}:0")}).to_string();
        let (code, stdout, stderr) = lab.run(&["show", "--id", &id, "--file", &file, "--json"]);
        assert_eq!(code, 0, "{stderr}");
        let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(result["data"]["object"]["data"]["availability"], expected, "{stdout}");
        assert!(!stdout.contains("available.pdf"));
    }
}
