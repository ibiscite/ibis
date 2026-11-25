//! A folder of PDFs as a library, through the real `ibis` executable
//! (decision 0061): nothing to read until the folder is kept, then one entry
//! per PDF in its `ibis-references.bib`, holding only what each PDF said.
#![cfg(test)]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

const XMP: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:prism="http://prismstandard.org/namespaces/basic/2.0/" prism:doi="10.5555/Invented.Atlas">
<dc:title><rdf:Alt><rdf:li xml:lang="x-default">An Invented Atlas of Tides</rdf:li></rdf:Alt></dc:title>
<dc:creator><rdf:Seq><rdf:li>Ada Lee</rdf:li></rdf:Seq></dc:creator>
<prism:publicationName>Journal of Invented Oceans</prism:publicationName>
<prism:coverDate>2026-05-01</prism:coverDate>
</rdf:Description></rdf:RDF></x:xmpmeta>"#;

struct Folder {
    home: PathBuf,
    papers: PathBuf,
}

impl Folder {
    fn new(name: &str) -> Self {
        let home = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("pdf-folder-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let papers = home.join("Papers");
        std::fs::create_dir_all(papers.join("older")).unwrap();
        std::fs::write(
            papers.join("atlas.pdf"),
            ibis::pdf_details::invented_pdf_with_details(
                &[
                    "An Invented Atlas of Tides\nAda Lee",
                    "A brass lantern on the second page.",
                ],
                &[("Title", "Microsoft Word - atlas.docx")],
                Some(XMP),
            ),
        )
        .unwrap();
        std::fs::write(
            papers.join("older/Tide_tables-1931.pdf"),
            ibis::pdf_text::invented_pdf(&["Invented tide tables with a copper buoy."], None),
        )
        .unwrap();
        std::fs::write(papers.join("notes.pdf"), "Invented notes saved as text.").unwrap();
        Self { home, papers }
    }

    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let output = Command::new(env!("CARGO_BIN_EXE_ibis"))
            .args(args)
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("APPDATA", self.home.join("AppData"))
            .env("XDG_CONFIG_HOME", self.home.join("config"))
            .env("XDG_DATA_HOME", self.home.join("data"))
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8(output.stdout).unwrap(),
            String::from_utf8(output.stderr).unwrap(),
        )
    }

    fn with_folder(&self, command: &[&str]) -> (i32, String, String) {
        let papers = self.papers.display().to_string();
        let mut args = vec!["--file", papers.as_str()];
        args.extend(command);
        self.run(&args)
    }

    fn json(&self, command: &[&str]) -> Value {
        let mut command = command.to_vec();
        command.push("--json");
        let (status, stdout, stderr) = self.with_folder(&command);
        assert_eq!(status, 0, "{stdout}{stderr}");
        serde_json::from_str(&stdout).unwrap()
    }

    fn bib(&self) -> String {
        std::fs::read_to_string(self.papers.join("ibis-references.bib")).unwrap()
    }
}

#[test]
fn a_kept_folder_cites_what_its_pdfs_said_and_nothing_else() {
    let folder = Folder::new("keep");
    // Nothing is a record until the folder is kept, and reading wrote nothing.
    let (status, _, stderr) = folder.with_folder(&["search", "tides"]);
    assert_ne!(status, 0);
    assert!(stderr.contains("ibis sources keep"), "{stderr}");
    assert!(!folder.papers.join("ibis-references.bib").exists());

    let papers = folder.papers.display().to_string();
    let (status, stdout, stderr) = folder.run(&["sources", "keep", "--file", &papers]);
    assert_eq!(status, 0, "{stdout}{stderr}");
    assert!(stdout.contains("kept 3 PDFs"), "{stdout}");
    let bib = folder.bib();
    assert!(bib.contains("file = {:atlas.pdf:application/pdf}"), "{bib}");
    assert!(
        bib.contains("file = {:older/Tide_tables-1931.pdf:application/pdf}"),
        "{bib}"
    );
    assert!(bib.contains("file = {:notes.pdf:application/pdf}"), "{bib}");
    assert!(bib.contains("title = {An Invented Atlas of Tides}"), "{bib}");
    assert!(!bib.contains("Microsoft Word"), "{bib}");

    // Keeping again finds nothing new and leaves the file as it was.
    let (status, stdout, _) = folder.run(&["sources", "keep", "--file", &papers]);
    assert_eq!(status, 0);
    assert!(stdout.contains("kept 0 PDFs"), "{stdout}");
    assert_eq!(folder.bib(), bib);

    let found = folder.json(&["search", "atlas"]).to_string();
    assert!(found.contains("lee2026invented"), "{found}");
    let cited = folder.json(&["cite", "lee2026invented", "--style", "apa"]).to_string();
    assert!(cited.contains("(Lee, 2026)"), "{cited}");

    // A PDF that said nothing is listed but not cited: there is nothing to cite it by.
    let (status, stdout, stderr) = folder.with_folder(&["cite", "tide", "--style", "apa", "--json"]);
    assert_ne!(status, 0, "{stdout}");
    assert!(
        format!("{stdout}{stderr}").contains("missing-details"),
        "{stdout}{stderr}"
    );

    // Passages are searchable whatever is known.
    let outline = folder.json(&["outline", "tide"]);
    let attachment = outline["data"]["attachments"][0]["id"].as_str().unwrap().to_owned();
    let found = folder.json(&["find-in", &attachment, "copper buoy"]);
    assert_eq!(found["data"]["passages"][0]["pdf_page"], 1, "{found}");

    // The folder keeps Ibis's promises, and says when a new PDF waits.
    let (status, stdout, stderr) = folder.run(&["sources", "check", "--file", &papers]);
    assert_eq!(status, 0, "{stdout}{stderr}");
    std::fs::copy(folder.papers.join("atlas.pdf"), folder.papers.join("atlas copy.pdf")).unwrap();
    // A warning is a finding, so the command says so in its exit status.
    let (status, libraries, _) = folder.with_folder(&["libraries", "--json"]);
    assert_ne!(status, 0);
    assert!(libraries.contains("pdfs-not-kept") && libraries.contains("\"folder\""), "{libraries}");
    let _ = std::fs::remove_dir_all(&folder.home);
}

#[test]
fn a_persons_edits_win_and_keeping_only_appends() {
    let folder = Folder::new("edits");
    let papers = folder.papers.display().to_string();
    assert_eq!(folder.run(&["sources", "keep", "--file", &papers]).0, 0);
    let edited = folder
        .bib()
        .replace("title = {An Invented Atlas of Tides}", "title = {My Own Title}");
    std::fs::write(folder.papers.join("ibis-references.bib"), &edited).unwrap();
    std::fs::write(
        folder.papers.join("second.pdf"),
        ibis::pdf_text::invented_pdf(&["Invented second paper."], None),
    )
    .unwrap();
    assert_eq!(folder.run(&["sources", "keep", "--file", &papers]).0, 0);
    let bib = folder.bib();
    assert!(bib.starts_with(&edited), "{bib}");
    assert!(bib.contains("file = {:second.pdf:application/pdf}"), "{bib}");
    assert!(Path::new(&folder.papers).join("atlas.pdf").exists());
    let _ = std::fs::remove_dir_all(&folder.home);
}
