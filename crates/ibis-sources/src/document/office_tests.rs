//! The office reader and writer on their own, in the child. Every document and
//! reference is invented here.
use super::*;
use ibis_core::contract::DocumentEntry;
use std::io::{Cursor, Read, Write};
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
    paragraphs(bytes)
        .unwrap()
        .1
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

/// Resolve attributes as well as element names: an ordinary XML reader does
/// not reject unbound prefixes, including the `r:id` on a section's header.
fn assert_namespaces(xml: &str) {
    use quick_xml::events::Event;
    use quick_xml::name::ResolveResult;
    let mut reader = quick_xml::NsReader::from_str(xml);
    loop {
        let event = reader.read_event().unwrap();
        match event {
            Event::Start(element) | Event::Empty(element) => {
                assert!(
                    !matches!(
                        reader.resolver().resolve_element(element.name()).0,
                        ResolveResult::Unknown(_)
                    ),
                    "unbound element {:?}",
                    element.name()
                );
                for attribute in element.attributes() {
                    let attribute = attribute.unwrap();
                    assert!(
                        !matches!(
                            reader.resolver().resolve_attribute(attribute.key).0,
                            ResolveResult::Unknown(_)
                        ),
                        "unbound attribute {:?}",
                        attribute.key
                    );
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
}

#[test]
fn opendocument_generated_styles_declare_their_namespaces() {
    // Only the prefixes used by the original content are declared. Styles
    // can be absent, empty, or already populated without style:/fo: bindings.
    for automatic in [
        "",
        "<office:automatic-styles/>",
        "<office:automatic-styles></office:automatic-styles>",
    ] {
        let xml = format!(
            r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.3">{automatic}<office:body><office:text><text:p>[@vale2024]</text:p><text:p>[bibliography]</text:p></office:text></office:body></office:document-content>"#
        );
        assert_namespaces(&xml);
        let original = archive(&[
            ("mimetype", b"application/vnd.oasis.opendocument.text"),
            ("content.xml", xml.as_bytes()),
        ]);
        let written = write(&original, &[vec![italic("(Vale, 2024)")]], &[entry("Lanterns")], true).unwrap();
        assert_namespaces(&part(&written, "content.xml"));
        assert_eq!(body(&written), ["(Vale, 2024)", "Vale, A. (2024). Lanterns."]);
    }
}

#[test]
fn word_bibliography_properties_keep_inherited_namespace_bindings() {
    const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    let seed = docx("");
    let types = part(&seed, "[Content_Types].xml").replace(
        "</Types>",
        r#"<Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/></Types>"#,
    );
    let rels = part(&seed, "_rels/.rels");
    let document_rels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdHeader" Type="{RELATIONSHIPS}/header" Target="header1.xml"/></Relationships>"#
    );
    let header = format!(r#"<w:hdr {W}><w:p><w:r><w:t>Invented header.</w:t></w:r></w:p></w:hdr>"#);
    // Bindings on either discarded ancestor must reach the preserved section.
    // Equivalent escaped bindings must not rebind r:id to a different URI,
    // even when a usable binding already surrounds the replacement.
    for (root, paragraph, properties) in [
        (String::new(), format!(r#" xmlns:r="{RELATIONSHIPS}""#), String::new()),
        (String::new(), String::new(), format!(r#" xmlns:r="{RELATIONSHIPS}""#)),
        (
            format!(r#" xmlns:r="{RELATIONSHIPS}""#),
            String::new(),
            r#" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationship&#115;""#.into(),
        ),
        (
            String::new(),
            r#" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationship&#x73;""#.into(),
            String::new(),
        ),
    ] {
        let draft = docx(&format!(
            r#"<w:p><w:r><w:t>[@vale2024]</w:t></w:r></w:p><w:p{paragraph}><w:pPr{properties}><w:sectPr><w:headerReference w:type="default" r:id="rIdHeader"/></w:sectPr></w:pPr><w:r><w:t>[bibliography]</w:t></w:r></w:p><w:p><w:r><w:t>Next section.</w:t></w:r></w:p>"#
        ));
        let document = part(&draft, "word/document.xml").replacen("<w:document ", &format!("<w:document{root} "), 1);
        let original = archive(&[
            ("[Content_Types].xml", types.as_bytes()),
            ("_rels/.rels", rels.as_bytes()),
            ("word/document.xml", document.as_bytes()),
            ("word/_rels/document.xml.rels", document_rels.as_bytes()),
            ("word/header1.xml", header.as_bytes()),
        ]);
        assert_namespaces(&part(&original, "word/document.xml"));
        for entries in [vec![entry("Lanterns"), entry("Tides")], vec![]] {
            let written = write(&original, &[vec![run("(Vale, 2024)")]], &entries, true).unwrap();
            let xml = part(&written, "word/document.xml");
            assert_namespaces(&xml);
            let mut reader = quick_xml::NsReader::from_str(&xml);
            let mut headers = 0;
            let mut header_id = String::new();
            loop {
                match reader.read_event().unwrap() {
                    quick_xml::events::Event::Empty(element) if element.name().into_inner() == "w:headerReference" => {
                        headers += 1;
                        let id = element
                            .attributes()
                            .map(Result::unwrap)
                            .find(|attr| attr.key.into_inner() == "r:id")
                            .unwrap();
                        header_id = id
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .unwrap()
                            .into_owned();
                        assert_eq!(header_id, "rIdHeader");
                        assert_eq!(
                            reader.resolver().resolve_attribute(id.key).0,
                            quick_xml::name::ResolveResult::Bound(quick_xml::name::Namespace(RELATIONSHIPS))
                        );
                    }
                    quick_xml::events::Event::Eof => break,
                    _ => {}
                }
            }
            assert_eq!(headers, 1);
            // Follow the preserved relationship to the header part, rather
            // than merely checking that an r:id string survived.
            let relationships = part(&written, "word/_rels/document.xml.rels");
            assert_namespaces(&relationships);
            let mut reader = quick_xml::Reader::from_str(&relationships);
            let mut target = None;
            loop {
                match reader.read_event().unwrap() {
                    quick_xml::events::Event::Empty(element) if element.name().into_inner() == "Relationship" => {
                        let value = |name| {
                            element
                                .try_get_attribute(name)
                                .unwrap()
                                .unwrap()
                                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .unwrap()
                                .into_owned()
                        };
                        if value("Id") == header_id {
                            assert_eq!(value("Type"), format!("{RELATIONSHIPS}/header"));
                            target = Some(value("Target"));
                        }
                    }
                    quick_xml::events::Event::Eof => break,
                    _ => {}
                }
            }
            assert_eq!(part(&written, &format!("word/{}", target.unwrap())), header);
            assert_eq!(body(&written).last().unwrap(), "Next section.");
        }
    }
}

#[test]
fn the_office_child_refuses_invalid_xml_output_without_returning_a_copy() {
    let command = super::super::Command::WriteOffice {
        citations: vec![vec![run("(Vale, 2024)")]],
        entries: vec![entry("Lanterns")],
        hanging_indent: false,
    };
    for original in [
        docx(r#"<w:p r:id="unbound"><w:r><w:t>[@vale2024]</w:t></w:r></w:p>"#),
        odt(r#"<text:p xlink:href="unbound">[@vale2024]</text:p>"#),
        docx(r#"<w:p><w:r><w:t>[@vale2024]</w:t></w:r><unbound:element/></w:p>"#),
        docx(r#"<w:p w:rsidR="a" w:rsidR="b"><w:r><w:t>[@vale2024]</w:t></w:r></w:p>"#),
        docx(r#"<w:p extra="&undeclared;"><w:r><w:t>[@vale2024]</w:t></w:r></w:p>"#),
        // This source is valid: text: is scoped to its original paragraph.
        // A generated bibliography paragraph outside that scope is invalid.
        archive(&[
            ("mimetype", b"application/vnd.oasis.opendocument.text"),
            (
                "content.xml",
                br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"><office:body><office:text><text:p xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">[@vale2024]</text:p></office:text></office:body></office:document-content>"#,
            ),
        ]),
    ] {
        // Reading markers succeeds; validating the rewritten XML must fail.
        assert_eq!(scan(&original).unwrap().citations.len(), 1);
        let request = super::super::frame(&command, &original).unwrap();
        let mut answer = Vec::new();
        assert_eq!(super::super::child::serve(&request[..], &mut answer), 0);
        assert!(matches!(
            super::super::split(answer).unwrap(),
            (super::super::Answer::Refused(Refusal::Damaged), bytes) if bytes.is_empty()
        ));
    }
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
    let written = write_edits(
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
        [
            "See (Vale, 2024, p. 12) now.",
            "Vale, A. (2024). Lanterns of the north."
        ]
    );
    let xml = part(&written, "word/document.xml");
    // The citation keeps the bold of the run it started in; the entry is italic where the style says.
    assert!(
        xml.contains(r#"<w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">(Vale, 2024, p. 12)</w:t></w:r>"#),
        "{xml}"
    );
    assert!(
        xml.contains(r#"<w:rPr><w:i/><w:iCs/></w:rPr><w:t xml:space="preserve">Lanterns of the north</w:t>"#),
        "{xml}"
    );
    assert!(xml.contains(r#"<w:ind w:left="720" w:hanging="720"/>"#));
    // Everything else is the original's.
    assert_eq!(part(&written, "_rels/.rels"), part(&original, "_rels/.rels"));
}

#[test]
fn without_a_bibliography_paragraph_the_entries_end_the_word_body() {
    let original = docx(r#"<w:p><w:r><w:t>As [@fen2023] wrote.</w:t></w:r></w:p>"#);
    let written = write_edits(
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
    let written = write_edits(
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
    assert_eq!(paragraphs(&bomb), Err(Refusal::TooLarge));
    for name in ["../escape.xml", "/absolute.xml", "word\\back.xml"] {
        let mut entries = vec![("word/document.xml", b"<w:document/>".as_slice())];
        entries.push((name, b"x"));
        assert_eq!(paragraphs(&archive(&entries)), Err(Refusal::Unsafe), "{name}");
    }
    let entity = archive(&[
        ("mimetype", b"application/vnd.oasis.opendocument.text"),
        (
            "content.xml",
            br#"<?xml version="1.0"?><!DOCTYPE x [<!ENTITY e SYSTEM "file:///etc/hosts">]><office:document-content><office:body><office:text><text:p>&e;</text:p></office:text></office:body></office:document-content>"#,
        ),
    ]);
    assert_eq!(paragraphs(&entity), Err(Refusal::Unsafe));
    assert_eq!(paragraphs(b"not a zip"), Err(Refusal::Unsupported));
    let deep = format!("{}{}", "<text:span>".repeat(300), "</text:span>".repeat(300));
    assert_eq!(
        paragraphs(&odt(&format!("<text:p>{deep}</text:p>"))),
        Err(Refusal::TooLarge)
    );
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
        assert_eq!(
            write_edits(&original, &edits(vec![edit], None, vec![])),
            Err(Refusal::Mismatch)
        );
    }
    // `at` must name a body paragraph that reads `[bibliography]`.
    assert_eq!(
        write_edits(&original, &edits(vec![], Some(0), vec![])),
        Err(Refusal::Mismatch)
    );
}

#[test]
fn the_office_child_refuses_marker_counts_before_returning_an_answer() {
    let body = "<text:p>[@a]</text:p>".repeat(super::super::MAX_CITATIONS + 1);
    let request = super::super::frame(&super::super::Command::ScanOffice, &odt(&body)).unwrap();
    let mut answer = Vec::new();
    assert_eq!(super::super::child::serve(&request[..], &mut answer), 0);
    assert!(matches!(
        super::super::split(answer).unwrap(),
        (super::super::Answer::Refused(Refusal::TooLarge), bytes) if bytes.is_empty()
    ));
}

#[test]
fn the_office_child_bounds_one_paragraph_references_and_diagnostics() {
    let limit = super::super::MAX_CITATIONS;
    let at_limit = odt(&format!("<text:p>{}</text:p>", "[@a]".repeat(limit)));
    assert_eq!(scan(&at_limit).unwrap().citations.len(), limit);
    for body in [
        format!("<text:p>{}</text:p>", "[@a]".repeat(limit + 1)),
        format!("<text:p>{}</text:p>", "[@a; @b]".repeat(limit / 2 + 1)),
        format!("<text:p>{}</text:p>", "[@a @b]".repeat(limit + 1)),
        format!(
            "<text:note-body><text:p>{}</text:p></text:note-body>",
            "[@a]".repeat(limit + 1)
        ),
    ] {
        assert!(matches!(scan(&odt(&body)), Err(Refusal::TooLarge)));
    }
}

#[test]
fn opendocument_cdata_counts_as_text_and_can_hold_part_of_a_marker() {
    let original = odt(concat!(
        "<text:p><![CDATA[KEEP<&]]>[bibliography]</text:p>",
        "<text:p><![CDATA[É [@va]]><![CDATA[le2024]]]>&#x20;now.</text:p>",
    ));
    let scanned = scan(&original).unwrap();
    assert!(!scanned.placed, "CDATA before the placeholder is ordinary text");
    assert_eq!(scanned.citations.len(), 1);
    assert_eq!(scanned.citations[0].place.column, 3);
    let written = write(&original, &[vec![run("(Vale, 2024)")]], &[entry("Lanterns")], true).unwrap();
    assert_eq!(
        body(&written),
        [
            "KEEP<&[bibliography]",
            "É (Vale, 2024) now.",
            "Vale, A. (2024). Lanterns."
        ]
    );
    assert!(part(&written, "content.xml").contains("<![CDATA[KEEP<&]]>"));
}

#[test]
fn replacing_a_word_bibliography_keeps_its_section_break_once_after_the_entries() {
    let section = r#"<w:sectPr xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:headerReference w:type="default" r:id="rId2"/><w:type w:val="nextPage"/><w:pgSz w:w="15840" w:h="12240"/></w:sectPr>"#;
    let original = docx(&format!(
        r#"<w:p><w:r><w:t>[@vale2024]</w:t></w:r></w:p><w:p><w:pPr><w:keepNext/>{section}</w:pPr><w:r><w:t>[bibliography]</w:t></w:r></w:p><w:p><w:r><w:t>Next section.</w:t></w:r></w:p>"#
    ));
    for entries in [vec![entry("Lanterns"), entry("Tides")], vec![]] {
        let written = write(&original, &[vec![run("(Vale, 2024)")]], &entries, true).unwrap();
        let xml = part(&written, "word/document.xml");
        assert_eq!(xml.matches(section).count(), 1, "{xml}");
        let paragraphs: Vec<_> = xml.split("</w:p>").collect();
        let last = entries.len().max(1);
        assert!(paragraphs[last].contains(section), "{xml}");
        assert!(paragraphs[last + 1].contains("Next section."), "{xml}");
    }
}

#[test]
fn hidden_word_runs_neither_add_citations_nor_change_visible_marker_offsets() {
    for vanish in [
        "<w:vanish/>",
        "<w:vanish w:val=\"true\"></w:vanish>",
        "<w:vanish w:val=\"1\"/>",
    ] {
        let original = docx(&format!(
            r#"<w:p><w:r><w:t xml:space="preserve">See </w:t></w:r><w:r><w:rPr>{vanish}</w:rPr><w:t>[@hidden]</w:t></w:r><w:r><w:t>[@vale2024]</w:t></w:r></w:p>"#
        ));
        let scanned = scan(&original).unwrap();
        assert_eq!(scanned.citations.len(), 1, "{vanish}");
        assert_eq!(scanned.citations[0].cites[0].key, "vale2024");
        assert_eq!(scanned.citations[0].place.column, 5);
        let written = write(&original, &[vec![run("(Vale, 2024)")]], &[entry("Lanterns")], true).unwrap();
        assert_eq!(body(&written)[0], "See (Vale, 2024)");
        assert!(part(&written, "word/document.xml").contains("<w:t>[@hidden]</w:t>"));
    }
    for value in ["false", "0", "off"] {
        let original = docx(&format!(
            r#"<w:p><w:r><w:rPr><w:vanish w:val="{value}"/></w:rPr><w:t>[@vale2024]</w:t></w:r></w:p>"#
        ));
        assert_eq!(scan(&original).unwrap().citations.len(), 1, "{value}");
    }
}
