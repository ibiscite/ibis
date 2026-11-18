//! A BibTeX library's PDF attachments, read through the real `ibis`
//! executable: the extractor runs as its child process (decision 0055).
#![cfg(test)]

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

/// A folder of its own, with a library whose attachments are invented files.
struct Library {
    root: PathBuf,
}

impl Library {
    fn allow_outside(&self, on: bool) {
        let config = if cfg!(target_os = "macos") {
            self.root.join("Library/Application Support")
        } else if cfg!(windows) {
            self.root.join("AppData")
        } else {
            self.root.join("config")
        };
        let path = config.join("com.ibiscite.ibis/settings.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            serde_json::json!({"schema_version":2, "attachment_text_outside_library":on}).to_string(),
        )
        .unwrap();
    }

    fn new(name: &str) -> Self {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("pdf-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("papers")).unwrap();
        let pdf = ibis::pdf_text::invented_pdf(
            &[
                "Invented preface.",
                "Invented chapter opening.",
                "Invented page that holds a brass lantern.",
            ],
            Some(&[(0, Some("r"), "", 1), (1, Some("D"), "", 1)]),
        );
        std::fs::write(root.join("papers/atlas.pdf"), pdf).unwrap();
        std::fs::write(root.join("papers/notes.pdf"), "Invented notes saved as text.").unwrap();
        std::fs::write(
            root.join("refs.bib"),
            "@book{atlas2026,\n  title = {An Invented Atlas},\n  author = {Lee, Ada},\n  year = {2026},\n  \
             file = {Invented atlas:papers/atlas.pdf:application/pdf;Invented notes:papers/notes.pdf:application/pdf;\
             Gone:papers/gone.pdf:application/pdf}\n}\n",
        )
        .unwrap();
        Self { root }
    }

    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let library = self.root.join("refs.bib");
        let output = Command::new(env!("CARGO_BIN_EXE_ibis"))
            .arg("--file")
            .arg(&library)
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

    fn json(&self, args: &[&str]) -> Value {
        let mut args = args.to_vec();
        args.push("--json");
        let (status, stdout, stderr) = self.run(&args);
        assert_eq!(status, 0, "{stdout}{stderr}");
        serde_json::from_str(&stdout).unwrap()
    }
}

#[test]
fn outside_attachments_need_the_device_permission_for_outline_read_and_find_in() {
    let mut library = Library::new("outside");
    let inner = library.root.join("library");
    std::fs::create_dir(&inner).unwrap();
    library.root = inner;
    std::fs::write(
        library.root.join("refs.bib"),
        "@book{atlas2026, title={Invented atlas}, file={../papers/atlas.pdf}}",
    )
    .unwrap();
    let (status, stdout, stderr) = library.run(&["outline", "atlas2026", "--json"]);
    assert_ne!(status, 0);
    assert!(stdout.contains("attachment-outside-library"), "{stdout}{stderr}");
    assert!(!stdout.contains("atlas.pdf"));
    library.allow_outside(true);
    let outline = library.json(&["outline", "atlas2026"]);
    let attachment = &outline["data"]["attachments"][0];
    assert_eq!(attachment["text"], "available");
    let id = attachment["id"].as_str().unwrap();
    assert_eq!(
        library.json(&["read", id])["data"]["pages"][0]["text"],
        "Invented preface."
    );
    assert_eq!(
        library.json(&["find-in", id, "lantern"])["data"]["passages"][0]["pdf_page"],
        3
    );
    library.allow_outside(false);
    for args in [
        vec!["read", id],
        vec!["find-in", id, "lantern"],
        vec![
            "call",
            "outline",
            "--params",
            r#"{"item":{"by":"key","value":"atlas2026"}}"#,
        ],
    ] {
        let (status, stdout, stderr) = library.run(&args);
        assert_ne!(status, 0);
        assert!(stderr.contains("outside the library’s folder"), "{stdout}{stderr}");
        assert!(stderr.contains("Settings › AI Apps"), "{stderr}");
        assert!(!stderr.contains("atlas.pdf"), "{stderr}");
    }
}

#[cfg(unix)]
#[test]
fn a_symlink_inside_the_library_cannot_authorize_an_outside_attachment() {
    let mut library = Library::new("outside-link");
    let target = library.root.join("papers/atlas.pdf");
    let inner = library.root.join("library");
    std::fs::create_dir(&inner).unwrap();
    std::os::unix::fs::symlink(target, inner.join("escape.pdf")).unwrap();
    library.root = inner;
    std::fs::write(
        library.root.join("refs.bib"),
        "@book{atlas2026, title={Invented atlas}, file={escape.pdf}}",
    )
    .unwrap();
    let (status, stdout, stderr) = library.run(&["outline", "atlas2026"]);
    assert_ne!(status, 0);
    assert!(stderr.contains("outside the library’s folder"), "{stdout}{stderr}");
    assert!(!stderr.contains("escape.pdf"));
}

#[test]
fn ibis_read_gives_a_bib_pdf_attachments_pages_with_their_printed_labels() {
    let library = Library::new("read");
    let outline = library.json(&["outline", "atlas2026"]);
    let attachments = outline["data"]["attachments"].as_array().unwrap();
    assert_eq!(
        attachments
            .iter()
            .map(|attachment| attachment["text"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["available", "not-pdf", "missing"]
    );
    let atlas = &attachments[0];
    assert_eq!(
        (atlas["pages"].as_u64(), atlas["pages_known"].as_bool()),
        (Some(3), Some(true))
    );
    assert_eq!(atlas["cite_pages_known"], true);
    let id = atlas["id"].as_str().unwrap().to_owned();
    let outline = serde_json::to_string(&outline).unwrap();
    assert!(
        !outline.contains("papers") && !outline.contains("atlas.pdf"),
        "no path leaves the engine: {outline}"
    );

    let read = library.json(&["read", &id, "--pages", "1-2"]);
    let pages = read["data"]["pages"].as_array().unwrap();
    assert_eq!(
        (&pages[0]["pdf_page"], &pages[0]["cite_page"], &pages[0]["text"]),
        (&Value::from(1), &Value::from("i"), &Value::from("Invented preface."))
    );
    assert_eq!(pages[1]["cite_page"], "1");
    assert_eq!(read["data"]["next"]["first_page"], 3);

    let (status, stdout, _) = library.run(&["find-in", &id, "brass LANTERN"]);
    assert_eq!(status, 0);
    assert_eq!(
        stdout,
        "[file page 3, printed page 2]\tInvented page that holds a brass lantern.\n"
    );
    let (status, stdout, _) = library.run(&["outline", "atlas2026"]);
    assert_eq!(status, 0);
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        lines[0].ends_with("\ttext, 3 pages with printed page numbers"),
        "{stdout}"
    );
    assert!(lines[1].ends_with("\tno text: the file is not a PDF"), "{stdout}");
    assert!(
        lines[2].ends_with("\tno text: no file where the library says"),
        "{stdout}"
    );

    // A file that is not a PDF is refused, with outline saying why.
    let notes = attachments[1]["id"].as_str().unwrap();
    let (status, _, stderr) = library.run(&["read", notes]);
    assert_ne!(status, 0);
    assert!(stderr.contains("ibis outline"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn a_linked_pdf_is_never_followed() {
    let library = Library::new("link");
    let papers = library.root.join("papers");
    std::fs::rename(papers.join("atlas.pdf"), library.root.join("elsewhere.pdf")).unwrap();
    std::os::unix::fs::symlink(library.root.join("elsewhere.pdf"), papers.join("atlas.pdf")).unwrap();
    let outline = library.json(&["outline", "atlas2026"]);
    assert_eq!(outline["data"]["attachments"][0]["text"], "not-local");
}

#[test]
fn contradicting_pdf_labels_give_no_cite_pages_in_outline_read_or_find_in() {
    let library = Library::new("wrong-labels");
    let pdf = ibis::pdf_text::invented_pdf(
        &["Invented preface.\niv", "78\nAn invented brass lantern."],
        Some(&[(0, Some("r"), "", 5), (1, Some("D"), "", 79)]),
    );
    std::fs::write(library.root.join("papers/atlas.pdf"), pdf).unwrap();
    let outline = library.json(&["outline", "atlas2026"]);
    let attachment = &outline["data"]["attachments"][0];
    assert_eq!(attachment["cite_pages_known"], false);
    let id = attachment["id"].as_str().unwrap();
    let read = library.json(&["read", id]);
    for page in read["data"]["pages"].as_array().unwrap() {
        assert_eq!(page["cite_page"], Value::Null);
    }
    let found = library.json(&["find-in", id, "lantern"]);
    assert_eq!(found["data"]["passages"][0]["cite_page"], Value::Null);
}

#[test]
fn share_previews_then_writes_a_pack_and_never_replaces_it() {
    let library = Library::new("share");
    let (status, preview, stderr) = library.run(&["share", "atlas2026", "--pdfs"]);
    assert_eq!(status, 0, "{preview}{stderr}");
    assert!(preview.contains("PDFs/Lee 2026 - An Invented Atlas.pdf\t"), "{preview}");
    assert!(
        preview.contains("3 files") && preview.contains("Add --to PATH"),
        "{preview}"
    );
    let pack = library.root.join("pack.zip");
    let written = library.json(&["share", "atlas2026", "--pdfs", "--to", pack.to_str().unwrap()]);
    assert_eq!(written["data"]["written"], true, "{written}");
    assert_eq!(written["data"]["files"].as_array().unwrap().len(), 3);
    assert!(
        !written.to_string().contains(library.root.to_str().unwrap()),
        "{written}"
    );
    let size = std::fs::metadata(&pack).unwrap().len();
    let (status, stdout, stderr) = library.run(&["share", "atlas2026", "--to", pack.to_str().unwrap()]);
    assert_eq!(status, 3, "{stdout}{stderr}");
    assert!(stderr.contains("already there"), "{stderr}");
    assert_eq!(std::fs::metadata(&pack).unwrap().len(), size);
}
