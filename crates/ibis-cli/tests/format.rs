//! `ibis format` end to end, through the real `ibis` executable and its
//! office child, and the child's reading and writing on their own. Every
//! document and reference is invented here.
#![cfg(test)]

use ibis::office::*;
use ibis_core::contract::{DisplayRun, DocumentEntry};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        let method = if *name == "mimetype" {
            CompressionMethod::Stored
        } else {
            CompressionMethod::Deflated
        };
        writer
            .start_file(*name, SimpleFileOptions::default().compression_method(method))
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

const W: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"";

fn docx(body: &str) -> Vec<u8> {
    let types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {W}><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr></w:body></w:document>"#
    );
    archive(&[
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", rels.as_bytes()),
        ("word/document.xml", document.as_bytes()),
    ])
}

fn odt(body: &str) -> Vec<u8> {
    let manifest = r#"<?xml version="1.0" encoding="UTF-8"?><manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3"><manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.text"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/><manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/></manifest:manifest>"#;
    let namespaces = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.3""#;
    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document-content {namespaces}><office:automatic-styles><style:style style:name="T1" style:family="text"><style:text-properties fo:font-weight="bold"/></style:style></office:automatic-styles><office:body><office:text>{body}</office:text></office:body></office:document-content>"#
    );
    let styles = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document-styles {namespaces}><office:styles><style:style style:name="Standard" style:family="paragraph"/></office:styles></office:document-styles>"#
    );
    archive(&[
        ("mimetype", b"application/vnd.oasis.opendocument.text"),
        ("META-INF/manifest.xml", manifest.as_bytes()),
        ("content.xml", content.as_bytes()),
        ("styles.xml", styles.as_bytes()),
    ])
}

fn run(text: &str) -> DisplayRun {
    DisplayRun {
        text: text.into(),
        ..DisplayRun::default()
    }
}

fn italic(text: &str) -> DisplayRun {
    DisplayRun {
        italic: true,
        ..run(text)
    }
}

fn body(bytes: &[u8]) -> Vec<String> {
    scan(bytes)
        .unwrap()
        .paragraphs
        .into_iter()
        .filter(|paragraph| paragraph.part == Part::Body)
        .map(|paragraph| paragraph.text)
        .collect()
}

fn edits(citations: Vec<Edit>, at: Option<usize>, entries: Vec<DocumentEntry>) -> Edits {
    Edits {
        citations,
        bibliography: Bibliography {
            at,
            entries,
            hanging_indent: true,
        },
    }
}

fn entry(text: &str) -> DocumentEntry {
    DocumentEntry {
        margin: vec![],
        content: vec![run("Vale, A. (2024). "), italic(text), run(".")],
    }
}

fn part(bytes: &[u8], name: &str) -> String {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut text = String::new();
    archive.by_name(name).unwrap().read_to_string(&mut text).unwrap();
    text
}

#[test]
fn a_word_marker_split_across_runs_becomes_one_citation_in_its_run_formatting() {
    // Word splits "[@vale2024, p. 12]" where formatting or editing changed.
    let original = docx(concat!(
        r#"<w:p><w:r><w:t xml:space="preserve">See </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>[@va</w:t></w:r>"#,
        r#"<w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">le2024, p. 12]</w:t></w:r><w:r><w:t xml:space="preserve"> now.</w:t></w:r></w:p>"#,
        r#"<w:p><w:r><w:t>[bibliography]</w:t></w:r></w:p>"#,
    ));
    assert_eq!(body(&original), ["See [@vale2024, p. 12] now.", "[bibliography]"]);
    let written = write(
        &original,
        &edits(
            vec![Edit {
                paragraph: 0,
                start: 4,
                end: 22,
                runs: vec![run("(Vale, 2024, p. 12)")],
            }],
            Some(1),
            vec![entry("Lanterns of the north")],
        ),
    )
    .unwrap();
    assert_eq!(
        body(&written),
        ["See (Vale, 2024, p. 12) now.", "Vale, A. (2024). Lanterns of the north."]
    );
    let xml = part(&written, "word/document.xml");
    // The citation keeps the bold of the run it started in; the entry is italic where the style says.
    assert!(xml.contains(r#"<w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">(Vale, 2024, p. 12)</w:t></w:r>"#), "{xml}");
    assert!(xml.contains(r#"<w:rPr><w:i/><w:iCs/></w:rPr><w:t xml:space="preserve">Lanterns of the north</w:t>"#), "{xml}");
    assert!(xml.contains(r#"<w:ind w:left="720" w:hanging="720"/>"#));
    // Everything else is the original's.
    assert_eq!(part(&written, "_rels/.rels"), part(&original, "_rels/.rels"));
}

#[test]
fn without_a_bibliography_paragraph_the_entries_end_the_word_body() {
    let original = docx(r#"<w:p><w:r><w:t>As [@fen2023] wrote.</w:t></w:r></w:p>"#);
    let written = write(
        &original,
        &edits(
            vec![Edit {
                paragraph: 0,
                start: 3,
                end: 13,
                runs: vec![run("[1]")],
            }],
            None,
            vec![DocumentEntry {
                margin: vec![run("[1]")],
                content: vec![run("A. Fen, Clouds, 2023.")],
            }],
        ),
    )
    .unwrap();
    assert_eq!(body(&written), ["As [1] wrote.", "[1]\tA. Fen, Clouds, 2023."]);
    let xml = part(&written, "word/document.xml");
    assert!(xml.find("A. Fen") < xml.find("<w:sectPr>"), "{xml}");
}

#[test]
fn an_opendocument_marker_across_spans_keeps_the_span_it_starts_in() {
    let original = odt(concat!(
        r#"<text:p>See <text:span text:style-name="T1">[@vale</text:span>2024; @fen2023]<text:s text:c="2"/>now.</text:p>"#,
        r#"<text:p text:style-name="Standard">[bibliography]</text:p>"#,
    ));
    assert_eq!(body(&original), ["See [@vale2024; @fen2023]  now.", "[bibliography]"]);
    let written = write(
        &original,
        &edits(
            vec![Edit {
                paragraph: 0,
                start: 4,
                end: 25,
                runs: vec![run("(Fen, 2023; "), italic("Vale"), run(", 2024)")],
            }],
            Some(1),
            vec![entry("Lanterns")],
        ),
    )
    .unwrap();
    assert_eq!(
        body(&written),
        ["See (Fen, 2023; Vale, 2024)  now.", "Vale, A. (2024). Lanterns."]
    );
    let xml = part(&written, "content.xml");
    assert!(
        xml.contains(r#"<text:span text:style-name="T1">(Fen, 2023; <text:span text:style-name="IbisFormatI">Vale</text:span>, 2024)</text:span>"#),
        "{xml}"
    );
    assert!(xml.contains(r#"style:parent-style-name="Standard""#), "{xml}");
    assert_eq!(part(&written, "mimetype"), "application/vnd.oasis.opendocument.text");
}

#[test]
fn hostile_archives_are_refused_before_their_xml_is_read() {
    // A part that inflates past its bound.
    let bomb = archive(&[
        ("mimetype", b"application/vnd.oasis.opendocument.text"),
        ("content.xml", &vec![b' '; 64 * 1024 * 1024 + 1]),
    ]);
    assert!(bomb.len() < 1024 * 1024);
    assert_eq!(scan(&bomb), Err(Refusal::TooLarge));
    for name in ["../escape.xml", "/absolute.xml", "word\\back.xml"] {
        let mut entries = vec![("word/document.xml", b"<w:document/>".as_slice())];
        entries.push((name, b"x"));
        assert_eq!(scan(&archive(&entries)), Err(Refusal::Unsafe), "{name}");
    }
    let entity = archive(&[
        ("mimetype", b"application/vnd.oasis.opendocument.text"),
        (
            "content.xml",
            br#"<?xml version="1.0"?><!DOCTYPE x [<!ENTITY e SYSTEM "file:///etc/hosts">]><office:document-content><office:body><office:text><text:p>&e;</text:p></office:text></office:body></office:document-content>"#,
        ),
    ]);
    assert_eq!(scan(&entity), Err(Refusal::Unsafe));
    assert_eq!(scan(b"not a zip"), Err(Refusal::NotOffice));
    let deep = format!("{}{}", "<text:span>".repeat(300), "</text:span>".repeat(300));
    assert_eq!(scan(&odt(&format!("<text:p>{deep}</text:p>"))), Err(Refusal::TooLarge));
}

#[test]
fn replacements_that_do_not_fit_the_text_are_refused() {
    let original = docx(r#"<w:p><w:r><w:t>[@a]</w:t></w:r></w:p>"#);
    for (start, end) in [(0, 9), (2, 1), (0, 0)] {
        let edit = Edit {
            paragraph: 0,
            start,
            end,
            runs: vec![run("x")],
        };
        assert_eq!(write(&original, &edits(vec![edit], None, vec![])), Err(Refusal::Mismatch));
    }
    // `at` must name a body paragraph that reads `[bibliography]`.
    assert_eq!(write(&original, &edits(vec![], Some(0), vec![])), Err(Refusal::Mismatch));
}

#[test]
fn the_child_answers_one_framed_request() {
    let original = docx(r#"<w:p><w:r><w:t>[@a]</w:t></w:r></w:p>"#);
    let mut answer = Vec::new();
    assert_eq!(child(frame(&ibis::office::Command::Scan, &original).unwrap().as_slice(), &mut answer), 0);
    let line = answer.split(|byte| *byte == b'\n').next().unwrap();
    let Answer::Scanned(scanned) = serde_json::from_slice(line).unwrap() else {
        panic!()
    };
    assert_eq!(scanned.kind, Kind::Docx);
    assert_eq!(scanned.paragraphs[0].text, "[@a]");
}

/// A folder of its own, with an invented library.
struct Folder {
    root: PathBuf,
}

const LIBRARY: &str = "\
@book{vale2024, author = {Vale, Avery}, title = {Lanterns of the North}, year = {2024}, publisher = {Imaginary Press}}
@book{vale2024tides, author = {Vale, Avery}, title = {Tides of the South}, year = {2024}, publisher = {Imaginary Press}}
@article{fen2023, author = {Fen, Rowan}, title = {Clouds over invented hills}, journal = {Journal of Made-up Weather}, year = {2023}, volume = {4}, pages = {1--9}}
";

impl Folder {
    fn new(name: &str) -> Self {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("format-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("refs.bib"), LIBRARY).unwrap();
        Self { root }
    }
    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let output = Command::new(env!("CARGO_BIN_EXE_ibis"))
            .args(args)
            .arg("--file")
            .arg(self.path("refs.bib"))
            .current_dir(&self.root)
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
    fn cite(&self, args: &[&str]) -> String {
        let mut all = vec!["cite"];
        all.extend_from_slice(args);
        let (status, stdout, stderr) = self.run(&all);
        assert_eq!(status, 0, "{stdout}{stderr}");
        stdout.trim().to_owned()
    }
}

fn text_of(path: &Path) -> Vec<String> {
    body(&std::fs::read(path).unwrap())
}

const DRAFT: &str = concat!(
    r#"<w:p><w:r><w:t xml:space="preserve">Lanterns guide ships [@va</w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">le2024, p. 12]</w:t></w:r>"#,
    r#"<w:r><w:t xml:space="preserve">, and tides follow clouds [@vale2024tides; @fen2023].</w:t></w:r></w:p>"#,
    r#"<w:p><w:r><w:t xml:space="preserve">Again [@vale2024], while [@fen2023, ch. 3] disagrees.</w:t></w:r></w:p>"#,
    r#"<w:p><w:r><w:t>[bibliography]</w:t></w:r></w:p>"#,
);

#[test]
fn a_word_draft_is_cited_in_apa_then_restyled_in_ieee_from_the_same_original() {
    let folder = Folder::new("restyle");
    let draft = folder.path("draft.docx");
    std::fs::write(&draft, docx(DRAFT)).unwrap();
    let before = std::fs::read(&draft).unwrap();

    let (status, stdout, stderr) = folder.run(&["format", "draft.docx", "--style", "apa", "-o", "apa.docx"]);
    assert_eq!(status, 0, "{stdout}{stderr}");
    assert!(stdout.contains("5 citations and 3 references"), "{stdout}");
    let apa = text_of(&folder.path("apa.docx"));
    // Two works by one author in one year get their letters from the whole document.
    assert_eq!(
        apa[..2],
        [
            "Lanterns guide ships (Vale, 2024a, p. 12), and tides follow clouds (Fen, 2023; Vale, 2024b).",
            "Again (Vale, 2024a), while (Fen, 2023, Chapter 3) disagrees.",
        ]
    );
    assert!(apa[2].starts_with("Fen, R. (2023). Clouds over invented hills."), "{apa:?}");
    assert!(apa[3].starts_with("Vale, A. (2024a). Lanterns of the North."), "{apa:?}");
    assert!(apa[4].starts_with("Vale, A. (2024b). Tides of the South."), "{apa:?}");
    // Where nothing else in the document bears on a citation, it is what `ibis cite` writes.
    assert_eq!(folder.cite(&["fen2023", "--style", "apa"]), "(Fen, 2023)");
    assert!(apa[0].contains(&folder.cite(&["fen2023", "--style", "apa"])[1..10]));
    assert_eq!(
        folder.cite(&["vale2024", "--style", "apa"]),
        "(Vale, 2024)",
        "a lone citation cannot know the document's year letters"
    );

    let (status, stdout, stderr) = folder.run(&["format", "draft.docx", "--style", "ieee", "-o", "ieee.docx"]);
    assert_eq!(status, 0, "{stdout}{stderr}");
    let ieee = text_of(&folder.path("ieee.docx"));
    // Numbers follow first citation, and so does the list.
    assert_eq!(ieee[0], "Lanterns guide ships [1, p. 12], and tides follow clouds [2], [3].");
    assert!(ieee[2].starts_with("[1]\tA. Vale, Lanterns of the North"), "{ieee:?}");
    assert!(ieee[3].starts_with("[2]\tA. Vale, Tides of the South"), "{ieee:?}");
    assert!(ieee[4].starts_with("[3]\tR. Fen,"), "{ieee:?}");

    // The original is untouched, and no copy is ever replaced.
    assert_eq!(std::fs::read(&draft).unwrap(), before);
    let apa_bytes = std::fs::read(folder.path("apa.docx")).unwrap();
    let (status, _, stderr) = folder.run(&["format", "draft.docx", "--style", "ieee", "-o", "apa.docx"]);
    assert_eq!(status, 3, "{stderr}");
    assert!(stderr.contains("already exists"), "{stderr}");
    assert_eq!(std::fs::read(folder.path("apa.docx")).unwrap(), apa_bytes);
    let (status, _, stderr) = folder.run(&["format", "draft.docx", "-o", "draft.docx"]);
    assert_eq!(status, 2, "{stderr}");
}

#[test]
fn an_opendocument_draft_gets_its_reference_list_at_the_end() {
    let folder = Folder::new("odt");
    std::fs::write(
        folder.path("draft.odt"),
        odt(concat!(
            r#"<text:p>Clouds <text:span text:style-name="T1">[@fen</text:span>2023, pp. 3-4] gather.</text:p>"#,
            r#"<text:p>As @vale2024 [p. 2] says, and later [-@vale2024].</text:p>"#,
        )),
    )
    .unwrap();
    let (status, stdout, stderr) = folder.run(&["format", "draft.odt", "-o", "final.odt"]);
    assert_eq!(status, 0, "{stdout}{stderr}");
    let text = text_of(&folder.path("final.odt"));
    assert_eq!(text[0], "Clouds (Fen, 2023, pp. 3–4) gather.");
    assert_eq!(text[1], "As Vale (2024, p. 2) says, and later (2024).");
    assert!(text[2].starts_with("Fen, R. (2023)."), "{text:?}");
    // A numeric style has no author to name in a sentence: refused, not guessed.
    let (status, _, stderr) = folder.run(&["format", "draft.odt", "--style", "ieee", "-o", "ieee.odt"]);
    assert_eq!(status, 3, "{stderr}");
    assert!(stderr.contains("'vale2024'"), "{stderr}");
    let (status, _, stderr) = folder.run(&["format", "draft.odt", "-o", "final.docx"]);
    assert_eq!(status, 2, "{stderr}");
}

#[test]
fn every_missing_key_and_damaged_marker_is_named_and_nothing_is_written() {
    let folder = Folder::new("refused");
    std::fs::write(
        folder.path("draft.docx"),
        docx(concat!(
            r#"<w:p><w:r><w:t xml:space="preserve">First [@nobody2020], then [@vale2024 @fen2023].</w:t></w:r></w:p>"#,
            r#"<w:p><w:r><w:t xml:space="preserve">Last [@fen2023; @ghost1999, p. 2</w:t></w:r></w:p>"#,
        )),
    )
    .unwrap();
    let (status, stdout, stderr) = folder.run(&["format", "draft.docx", "-o", "final.docx"]);
    assert_eq!(status, 3, "{stdout}{stderr}");
    assert!(!folder.path("final.docx").exists());
    for expected in [
        "body paragraph 1, character 7, '[@nobody2020]': no open library has the key 'nobody2020'",
        "body paragraph 1, character 27, '[@vale2024 @fen2023]': two keys need a ; between them",
        "two keys need a ; between them",
        "body paragraph 2, character 6",
        "the marker has no closing ]",
    ] {
        assert!(stderr.contains(expected), "{expected}\n{stderr}");
    }
    std::fs::write(folder.path("plain.docx"), docx(r#"<w:p><w:r><w:t>See [@fen2023].</w:t></w:r></w:p>"#)).unwrap();
    let (status, _, stderr) = folder.run(&["format", "plain.docx", "-o", "notes.docx", "--style", "chicago-notes-bibliography"]);
    assert_eq!(status, 3, "{stderr}");
    assert!(stderr.contains("as footnotes"), "{stderr}");
}

#[test]
fn a_hostile_document_is_refused_by_the_child() {
    let folder = Folder::new("hostile");
    let mut entries = vec![("word/document.xml", b"<w:document/>".as_slice())];
    entries.push(("../../escape.xml", b"x"));
    std::fs::write(folder.path("draft.docx"), archive(&entries)).unwrap();
    let (status, _, stderr) = folder.run(&["format", "draft.docx", "-o", "final.docx"]);
    assert_eq!(status, 3, "{stderr}");
    assert!(stderr.contains("never write"), "{stderr}");
    assert!(!folder.root.parent().unwrap().join("escape.xml").exists());
}
