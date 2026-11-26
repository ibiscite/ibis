//! `format` end to end through the engine and the real `ibis __document`
//! child (decision 0060). Every document and reference is invented here.
#![cfg(test)]
use ibis_engine::{Cancellation, Engine, FormatRefusal, FormatRequest, FormatResult};
use ibis_sources::document::Refusal;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
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

const LIBRARY: &str = "\
@book{vale2024, author = {Vale, Avery}, title = {Lanterns of the North}, year = {2024}, publisher = {Imaginary Press}}
@book{vale2024tides, author = {Vale, Avery}, title = {Tides of the South}, year = {2024}, publisher = {Imaginary Press}}
@article{fen2023, author = {Fen, Rowan}, title = {Clouds over invented hills}, journal = {Journal of Made-up Weather}, year = {2023}, volume = {4}, pages = {1--9}}
";

struct Folder {
    folder: tempfile::TempDir,
    engine: Engine,
}

impl Folder {
    fn new() -> Self {
        let folder = tempfile::tempdir().unwrap();
        // macOS reaches its temporary folder through a link; documents are read without following one.
        let root = folder.path().canonicalize().unwrap();
        std::fs::write(root.join("refs.bib"), LIBRARY).unwrap();
        let engine = Engine::new()
            .unwrap()
            .without_file_watches()
            .with_document_reader(env!("CARGO_BIN_EXE_ibis").into());
        engine
            .open_file_observed(&root.join("refs.bib"), &Cancellation::default())
            .unwrap();
        Self { folder, engine }
    }
    fn path(&self, name: &str) -> PathBuf {
        self.folder.path().canonicalize().unwrap().join(name)
    }
    fn format(&self, input: &str, output: &str, style: &str) -> FormatResult {
        self.engine
            .format_document(
                &FormatRequest {
                    document: self.path(input),
                    output: self.path(output),
                    style: style.into(),
                    output_locale: None,
                },
                &Cancellation::default(),
            )
            .unwrap()
    }
}

/// Each paragraph's text, tabs kept, from a written DOCX or ODT.
fn text_of(path: &Path) -> Vec<String> {
    let bytes = std::fs::read(path).unwrap();
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let (name, end) = if archive.by_name("content.xml").is_ok() {
        ("content.xml", "</text:p>")
    } else {
        ("word/document.xml", "</w:p>")
    };
    let mut xml = String::new();
    archive.by_name(name).unwrap().read_to_string(&mut xml).unwrap();
    xml.split(end)
        .map(|paragraph| {
            let paragraph = paragraph.replace("<w:tab/>", "\t").replace("<text:tab/>", "\t");
            let mut text = String::new();
            let mut inside = false;
            for c in paragraph.chars() {
                match c {
                    '<' => inside = true,
                    '>' => inside = false,
                    c if !inside => text.push(c),
                    _ => {}
                }
            }
            text.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">")
        })
        .filter(|text| !text.is_empty())
        .collect()
}

const DRAFT: &str = concat!(
    r#"<w:p><w:r><w:t xml:space="preserve">Lanterns guide ships [@va</w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">le2024, p. 12]</w:t></w:r>"#,
    r#"<w:r><w:t xml:space="preserve">, and tides follow clouds [@vale2024tides; @fen2023].</w:t></w:r></w:p>"#,
    r#"<w:p><w:r><w:t xml:space="preserve">Again [@vale2024], while [@fen2023, ch. 3] disagrees.</w:t></w:r></w:p>"#,
    r#"<w:p><w:r><w:t>[bibliography]</w:t></w:r></w:p>"#,
);

#[test]
fn a_word_draft_is_cited_in_apa_then_restyled_in_ieee_from_the_same_original() {
    let folder = Folder::new();
    std::fs::write(folder.path("draft.docx"), docx(DRAFT)).unwrap();
    let before = std::fs::read(folder.path("draft.docx")).unwrap();

    let FormatResult::Written(report) = folder.format("draft.docx", "apa.docx", "apa") else {
        panic!()
    };
    assert_eq!((report.citations, report.references, report.placed), (4, 3, true));
    let apa = text_of(&folder.path("apa.docx"));
    // Two works by one author in one year get their letters from the whole document.
    assert_eq!(
        apa[..2],
        [
            "Lanterns guide ships (Vale, 2024a, p. 12), and tides follow clouds (Fen, 2023; Vale, 2024b).",
            "Again (Vale, 2024a), while (Fen, 2023, Chapter 3) disagrees.",
        ]
    );
    assert!(
        apa[2].starts_with("Fen, R. (2023). Clouds over invented hills."),
        "{apa:?}"
    );
    assert!(
        apa[3].starts_with("Vale, A. (2024a). Lanterns of the North."),
        "{apa:?}"
    );
    assert!(apa[4].starts_with("Vale, A. (2024b). Tides of the South."), "{apa:?}");

    let FormatResult::Written(_) = folder.format("draft.docx", "ieee.docx", "ieee") else {
        panic!()
    };
    let ieee = text_of(&folder.path("ieee.docx"));
    // Numbers follow first citation, and so does the list.
    assert_eq!(
        ieee[0],
        "Lanterns guide ships [1, p. 12], and tides follow clouds [2], [3]."
    );
    assert!(ieee[2].starts_with("[1]\tA. Vale, Lanterns of the North"), "{ieee:?}");
    assert!(ieee[4].starts_with("[3]\tR. Fen,"), "{ieee:?}");

    // The original is untouched, and no copy is ever replaced.
    assert_eq!(std::fs::read(folder.path("draft.docx")).unwrap(), before);
    let apa_bytes = std::fs::read(folder.path("apa.docx")).unwrap();
    assert_eq!(
        folder.format("draft.docx", "apa.docx", "ieee"),
        FormatResult::Refused(FormatRefusal::OutputExists)
    );
    assert_eq!(std::fs::read(folder.path("apa.docx")).unwrap(), apa_bytes);
    assert_eq!(
        folder.format("draft.docx", "draft.docx", "apa"),
        FormatResult::Refused(FormatRefusal::SameFile)
    );
    assert_eq!(
        folder.format("draft.docx", "final.odt", "apa"),
        FormatResult::Refused(FormatRefusal::OutputKind)
    );
}

#[test]
fn an_opendocument_draft_gets_its_reference_list_at_the_end() {
    let folder = Folder::new();
    std::fs::write(
        folder.path("draft.odt"),
        odt(concat!(
            r#"<text:p>Clouds <text:span text:style-name="T1">[@fen</text:span>2023, pp. 3-4] gather.</text:p>"#,
            r#"<text:p>As @vale2024 [p. 2] says, and later [-@vale2024].</text:p>"#,
        )),
    )
    .unwrap();
    let FormatResult::Written(report) = folder.format("draft.odt", "final.odt", "apa") else {
        panic!()
    };
    assert!(!report.placed);
    let text = text_of(&folder.path("final.odt"));
    assert_eq!(text[0], "Clouds (Fen, 2023, pp. 3–4) gather.");
    assert_eq!(text[1], "As Vale (2024, p. 2) says, and later (2024).");
    assert!(text[2].starts_with("Fen, R. (2023)."), "{text:?}");
    // A numeric style has no author to name in a sentence: refused, not guessed.
    let FormatResult::Refused(FormatRefusal::Markers(issues)) = folder.format("draft.odt", "ieee.odt", "ieee") else {
        panic!()
    };
    assert!(!issues.is_empty());
    assert!(!folder.path("ieee.odt").exists());
}

#[test]
fn every_missing_key_and_damaged_marker_is_named_and_nothing_is_written() {
    let folder = Folder::new();
    std::fs::write(
        folder.path("draft.docx"),
        docx(concat!(
            r#"<w:p><w:r><w:t xml:space="preserve">First [@nobody2020], then [@vale2024 @fen2023].</w:t></w:r></w:p>"#,
            r#"<w:p><w:r><w:t xml:space="preserve">Last [@fen2023; @ghost1999, p. 2</w:t></w:r></w:p>"#,
        )),
    )
    .unwrap();
    let FormatResult::Refused(FormatRefusal::Markers(issues)) = folder.format("draft.docx", "final.docx", "apa") else {
        panic!()
    };
    let places: Vec<(usize, usize)> = issues
        .iter()
        .map(|issue| (issue.place.paragraph, issue.place.column))
        .collect();
    assert_eq!(places, [(1, 7), (1, 27), (2, 6)]);
    assert_eq!(issues[0].place.text, "[@nobody2020]");
    assert!(!folder.path("final.docx").exists());
    std::fs::write(
        folder.path("plain.docx"),
        docx(r#"<w:p><w:r><w:t>See [@fen2023].</w:t></w:r></w:p>"#),
    )
    .unwrap();
    assert_eq!(
        folder.format("plain.docx", "notes.docx", "chicago-notes-bibliography"),
        FormatResult::Refused(FormatRefusal::NoteStyle)
    );
}

#[test]
fn a_hostile_document_is_refused_by_the_child() {
    let folder = Folder::new();
    let entries = [
        ("word/document.xml", b"<w:document/>".as_slice()),
        ("../../escape.xml", b"x".as_slice()),
    ];
    std::fs::write(folder.path("draft.docx"), archive(&entries)).unwrap();
    assert_eq!(
        folder.format("draft.docx", "final.docx", "apa"),
        FormatResult::Refused(FormatRefusal::Document(Refusal::Unsafe))
    );
    assert!(!folder.folder.path().parent().unwrap().join("escape.xml").exists());
}

#[test]
fn citation_numbers_do_not_count_as_printed_locators() {
    let folder = Folder::new();
    std::fs::write(
        folder.path("draft.docx"),
        docx(r#"<w:p><w:r><w:t>[@vale2024, p. 1; @fen2023, p. 2]</w:t></w:r></w:p>"#),
    )
    .unwrap();
    let result = folder.format("draft.docx", "nature.docx", "nature");
    let FormatResult::Refused(FormatRefusal::Markers(issues)) = result else {
        panic!("{result:?}")
    };
    let keys: Vec<_> = issues
        .iter()
        .map(|issue| match &issue.issue {
            ibis_engine::FormatIssueKind::LocatorNotPrinted { key } => key.as_str(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(keys, ["vale2024", "fen2023"]);
    assert!(!folder.path("nature.docx").exists());

    // A processor-rendered page with the same value as a citation number is valid.
    assert!(matches!(
        folder.format("draft.docx", "ieee.docx", "ieee"),
        FormatResult::Written(_)
    ));
    assert_eq!(text_of(&folder.path("ieee.docx"))[0], "[1, p. 1], [2, p. 2]");

    // Range shortening and Roman page presentation remain valid locators.
    for (index, locator) in ["321-328", "xii"].into_iter().enumerate() {
        std::fs::write(
            folder.path("range.docx"),
            docx(&format!(
                r#"<w:p><w:r><w:t>[@vale2024, p. {locator}]</w:t></w:r></w:p>"#
            )),
        )
        .unwrap();
        let result = folder.format("range.docx", &format!("range-{index}.docx"), "apa");
        assert!(matches!(result, FormatResult::Written(_)), "{locator}: {result:?}");
    }
}

#[test]
fn a_document_bibliography_keeps_authors_before_a_year_margin() {
    let folder = Folder::new();
    std::fs::write(
        folder.path("draft.odt"),
        odt("<text:p>[@vale2024]</text:p><text:p>[bibliography]</text:p>"),
    )
    .unwrap();
    let result = folder.format("draft.odt", "final.odt", "administrative-science-quarterly");
    assert!(matches!(result, FormatResult::Written(_)), "{result:?}");
    let text = text_of(&folder.path("final.odt"));
    assert_eq!(text.len(), 2);
    let entry = &text[1];
    assert!(entry.contains("Vale"), "{entry}");
    assert!(entry.find("Vale") < entry.find("2024"), "{entry}");
    assert!(entry.contains("Lanterns"), "{entry}");
}

#[test]
fn incomplete_bibliography_editors_refuse_the_document_even_with_complete_authors() {
    let folder = Folder::new();
    std::fs::write(
        folder.path("chapter.bib"),
        "@incollection{chapter, author={Vale, Avery}, editor={Fen, Robin and others}, title={Invented Lanterns}, booktitle={Invented Volume}, year={2024}, pages={1--20}, publisher={Imaginary Press}}",
    )
    .unwrap();
    folder
        .engine
        .open_file_observed(&folder.path("chapter.bib"), &Cancellation::default())
        .unwrap();
    std::fs::write(
        folder.path("draft.docx"),
        docx(r#"<w:p><w:r><w:t>[@chapter]</w:t></w:r></w:p>"#),
    )
    .unwrap();
    let result = folder.format("draft.docx", "final.docx", "apa");
    let FormatResult::Refused(FormatRefusal::Markers(issues)) = result else {
        panic!("{result:?}")
    };
    assert_eq!(issues.len(), 1);
    assert!(matches!(
        &issues[0].issue,
        ibis_engine::FormatIssueKind::Citation { key, problem: ibis_core::contract::CitationError::UnsupportedForm }
            if key == "chapter"
    ));
    assert!(!folder.path("final.docx").exists());
}
