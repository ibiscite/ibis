//! Invented documents built here, never real files.
use super::*;
use ibis_core::contract::{DisplayRun, DocumentEntry};
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

pub(crate) fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
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

pub(crate) fn docx(body: &str) -> Vec<u8> {
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

pub(crate) fn odt(body: &str) -> Vec<u8> {
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
    archive::Archive::open(bytes).unwrap().text(name).unwrap().unwrap()
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
    assert_eq!(child(frame(&Command::Scan, &original).unwrap().as_slice(), &mut answer), 0);
    let line = answer.split(|byte| *byte == b'\n').next().unwrap();
    let Answer::Scanned(scanned) = serde_json::from_slice(line).unwrap() else {
        panic!()
    };
    assert_eq!(scanned.kind, Kind::Docx);
    assert_eq!(scanned.paragraphs[0].text, "[@a]");
}
