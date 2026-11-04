#![allow(clippy::unwrap_used)]
//! What a person reads in a terminal: each message must be true, and each next
//! step must work when followed. Runs the program with an empty home folder.
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
    assert_eq!(stdout, "{\"error\":{\"code\":\"usage\"},\"ibis\":1,\"ok\":false}\n");
}

#[test]
fn help_speaks_plainly() {
    let lab = Lab::new("help");
    let (_, root, _) = lab.run(&["--help"]);
    let (_, show, _) = lab.run(&["show", "--help"]);
    let (_, cite, _) = lab.run(&["cite", "--help"]);
    for (text, word) in [
        (&root, "loaded records"),
        (&root, "shared engine"),
        (&show, "ItemRef"),
        (&show, "object"),
        (&cite, "CitationRequest JSON object from stdin\n\n"),
    ] {
        assert!(!text.contains(word), "{word}: {text}");
    }
}

#[test]
fn a_limit_nothing_could_fill_is_refused_the_same_way_at_either_end() {
    let lab = Lab::new("limit");
    let file = lab.file("refs.bib", BIB);
    for limit in ["0", "1001"] {
        let (code, stdout, stderr) = lab.run(&["search", "lantern", "--file", &file, "--limit", limit]);
        assert_eq!((code, stdout.as_str()), (2, ""), "{limit}");
        assert!(stderr.contains("from 1 to 1000"), "{stderr}");
    }
}

#[test]
fn a_blank_search_asks_for_words() {
    let lab = Lab::new("blank");
    let file = lab.file("refs.bib", BIB);
    for words in ["", "   "] {
        let (code, stdout, stderr) = lab.run(&["search", words, "--file", &file]);
        assert_eq!((code, stdout.as_str()), (2, ""));
        assert!(stderr.contains("words to search for"), "{stderr}");
    }
}

#[test]
fn a_file_with_no_entries_is_not_called_a_failed_search() {
    let lab = Lab::new("text");
    let file = lab.file("notes.txt", "Shopping list\nno references here\n");
    let (code, stdout, _) = lab.run(&["search", "shopping", "--file", &file]);
    assert_eq!(code, 0);
    assert!(stdout.contains("holds any references"), "{stdout}");
}

#[test]
fn a_library_of_skipped_entries_is_not_called_empty() {
    let lab = Lab::new("skipped");
    let file = lab.file(
        "refs.bib",
        "@article{same, title={First}, year=2000}\n@article{same, title={Second}, year=2001}\n",
    );
    let (code, stdout, stderr) = lab.run(&["search", "first", "--file", &file]);
    assert_eq!((code, stdout.as_str()), (1, "No references found.\n"));
    assert!(stderr.contains("see: ibis libraries"), "{stderr}");
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
fn the_warning_note_gives_a_command_that_works() {
    let lab = Lab::new("note");
    let file = lab.file("refs with space.bib", BIB);
    let (code, _, stderr) = lab.run(&["search", "lantern", "--file", &file]);
    assert_eq!(code, 1);
    let command = stderr.split("see: ibis ").nth(1).unwrap().trim();
    let quoted = if cfg!(windows) {
        format!("\"{file}\"")
    } else {
        format!("'{file}'")
    };
    assert_eq!(command, format!("libraries --file {quoted}"));
    let (code, stdout, stderr) = lab.run(&["libraries", "--file", &file]);
    assert_eq!(code, 1, "{stderr}");
    assert!(stdout.contains("2 warnings"), "{stdout}");
    assert!(
        stdout.contains("line 9: more than one entry uses 'same' as its key or id"),
        "{stdout}"
    );
}

#[test]
fn call_of_search_reads_the_same_libraries_as_search() {
    let lab = Lab::new("call");
    let (code, stdout, stderr) = lab.run(&["call", "search", "--params", r#"{"query":"lantern"}"#]);
    assert_eq!((code, stdout.as_str()), (2, ""), "{stderr}");
    assert!(stderr.contains("no library is named"), "{stderr}");
    // Opening a library by parameters needs none named.
    let file = lab.file("refs.bib", BIB);
    let params = format!(r#"{{"action":"open-file","path":{file:?}}}"#);
    let (code, _, stderr) = lab.run(&["call", "libraries", "--params", &params]);
    assert_eq!(code, 1, "{stderr}");
}

#[test]
fn doctor_reads_as_sentences() {
    let lab = Lab::new("doctor");
    let file = lab.file("refs.bib", BIB);
    let (_, stdout, _) = lab.run(&["doctor", "--file", &file]);
    assert!(!stdout.contains("ms since Unix epoch"), "{stdout}");
    // The next step works for a file, Zotero and the app's libraries alike.
    assert!(
        stdout.contains("See them with ibis libraries and the same library options."),
        "{stdout}"
    );
}

#[cfg(unix)]
#[test]
fn a_link_to_nothing_is_missing_rather_than_busy() {
    let lab = Lab::new("link");
    let link = lab.0.join("link.bib");
    std::os::unix::fs::symlink(lab.0.join("gone.bib"), &link).unwrap();
    let (code, _, stderr) = lab.run(&["search", "x", "--file", link.to_str().unwrap()]);
    assert_eq!(code, 5);
    assert!(stderr.contains("there is no file at that path"), "{stderr}");
}
