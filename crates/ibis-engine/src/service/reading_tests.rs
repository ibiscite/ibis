use super::*;
use serde_json::json;

fn paged(pages: &[&str]) -> DocumentText {
    let pages: Vec<String> = pages.iter().map(|page| (*page).to_owned()).collect();
    let count = u32::try_from(pages.len()).ok();
    DocumentText {
        body: Body::Pages(pages),
        indexed_pages: count,
        total_pages: count,
        cite_pages: None,
    }
}

fn whole(text: &str) -> DocumentText {
    DocumentText {
        body: Body::Whole(text.to_owned()),
        indexed_pages: None,
        total_pages: None,
        cite_pages: None,
    }
}

fn ask(first_page: Option<u32>, last_page: Option<u32>, offset: Option<usize>) -> ReadRequest {
    ReadRequest {
        attachment: String::new(),
        first_page,
        last_page,
        offset,
    }
}

fn numbers(pages: &[PageText]) -> Vec<Option<u32>> {
    pages.iter().map(|page| page.pdf_page).collect()
}

fn reason(failure: EngineError) -> Option<ErrorReason> {
    failure.reason
}

#[test]
fn a_read_gives_at_most_five_marked_pages_and_says_where_to_go_on() {
    let text = paged(&["One.", "Two.", "Three.", "Four.", "Five.", "Six.", "Seven."]);
    let (pages, next) = read_pages(&text, &ask(None, None, None)).unwrap();
    assert_eq!(numbers(&pages), [1, 2, 3, 4, 5].map(Some));
    assert!(pages.iter().all(|page| page.cite_page.is_none() && !page.cut));
    assert_eq!(
        next,
        Some(ReadNext {
            first_page: Some(6),
            offset: None
        })
    );
    let (pages, next) = read_pages(&text, &ask(Some(6), Some(20), None)).unwrap();
    assert_eq!(numbers(&pages), [Some(6), Some(7)]);
    assert_eq!(pages[1].text, "Seven.");
    assert_eq!(next, None);
    let (pages, _) = read_pages(&text, &ask(Some(3), Some(3), None)).unwrap();
    assert_eq!(numbers(&pages), [Some(3)]);
    for (first, last) in [(Some(0), None), (Some(8), None), (Some(4), Some(3))] {
        assert_eq!(
            read_pages(&text, &ask(first, last, None)).err().and_then(reason),
            Some(ErrorReason::NoSuchPages)
        );
    }
}

#[test]
fn cite_pages_come_only_from_the_pdfs_labels() {
    let mut text = paged(&["Preface.", "Chapter with a lantern.", "Plate."]);
    text.cite_pages = Some(vec![Some("xii".into()), Some("1".into()), None]);
    let (pages, _) = read_pages(&text, &ask(None, None, None)).unwrap();
    assert_eq!(
        pages.iter().map(|page| page.cite_page.as_deref()).collect::<Vec<_>>(),
        [Some("xii"), Some("1"), None]
    );
    let (passages, _) = find_passages(&text, "lantern", &Cancellation::default()).unwrap();
    assert_eq!(
        (passages[0].pdf_page, passages[0].cite_page.as_deref()),
        (Some(2), Some("1"))
    );
}

#[test]
fn a_read_stops_at_32_kib_and_continues_a_long_page_where_it_stopped() {
    let long = "é".repeat(MAX_READ_BYTES);
    let short = "a".repeat(MAX_READ_BYTES / 2 + 1);
    let text = paged(&[&short, &short, &long]);
    let (pages, next) = read_pages(&text, &ask(None, None, None)).unwrap();
    assert_eq!(numbers(&pages), [Some(1)]);
    assert_eq!(
        next,
        Some(ReadNext {
            first_page: Some(2),
            offset: None
        })
    );
    let (pages, next) = read_pages(&text, &ask(Some(3), None, None)).unwrap();
    assert!(pages[0].cut);
    assert_eq!(pages[0].text.len(), MAX_READ_BYTES);
    let Some(ReadNext {
        first_page: Some(3),
        offset: Some(offset),
    }) = next
    else {
        panic!("a cut page goes on");
    };
    let (rest, next) = read_pages(&text, &ask(Some(3), None, Some(offset))).unwrap();
    assert_eq!(format!("{}{}", pages[0].text, rest[0].text), long);
    assert_eq!(next, None);
    let total: usize = pages.iter().map(|page| page.text.len()).sum();
    assert!(total <= MAX_READ_BYTES);
}

#[test]
fn text_without_verified_pages_reads_by_offset_with_no_page_numbers() {
    let text = whole(&"Invented words. ".repeat(4096));
    let (pages, next) = read_pages(&text, &ask(None, None, None)).unwrap();
    assert_eq!(numbers(&pages), [None]);
    assert_eq!(pages[0].text.len(), MAX_READ_BYTES);
    assert_eq!(
        next,
        Some(ReadNext {
            first_page: None,
            offset: Some(MAX_READ_BYTES)
        })
    );
    assert_eq!(
        read_pages(&text, &ask(Some(1), None, None)).err().and_then(reason),
        Some(ErrorReason::NoSuchPages)
    );
    assert_eq!(
        read_pages(&text, &ask(None, None, Some(usize::MAX)))
            .err()
            .and_then(reason),
        Some(ErrorReason::NoSuchPages)
    );
}

#[test]
fn text_loses_hidden_and_control_characters_but_keeps_its_lines() {
    let text = paged(&["Invented\u{202e} line\u{7}\r\nnext\tline"]);
    let (pages, _) = read_pages(&text, &ask(None, None, None)).unwrap();
    assert_eq!(pages[0].text, "Invented line\nnext\tline");
}

#[test]
fn find_in_ignores_case_and_line_breaks_and_gives_short_one_line_passages() {
    let filler = "word ".repeat(200);
    let page = format!("{filler}a Brass\n  LANTERN glows {filler}");
    let text = paged(&["Nothing here.", &page]);
    let query = fold("brass lantern").0;
    let (passages, complete) = find_passages(&text, &query, &Cancellation::default()).unwrap();
    assert!(complete);
    assert_eq!(passages.len(), 1);
    assert_eq!(passages[0].pdf_page, Some(2));
    assert_eq!(passages[0].cite_page, None);
    let found = &passages[0].text;
    assert!(found.contains("a Brass LANTERN glows"), "{found}");
    assert!(found.starts_with('…') && found.ends_with('…'));
    assert!(!found.contains('\n'));
    assert!(found.chars().count() <= PASSAGE_CHARS + 2);
}

#[test]
fn find_in_gives_ten_passages_at_most_and_says_when_there_are_more() {
    let page = "lantern ".repeat(100);
    let text = whole(&page);
    let (passages, complete) = find_passages(&text, "lantern", &Cancellation::default()).unwrap();
    assert!(passages.len() > 1 && passages.len() < 10, "neighbours share a passage");
    assert!(complete);
    let spread = format!("lantern{}", " ".repeat(2 * PASSAGE_CHARS)).repeat(11);
    let (passages, complete) = find_passages(&whole(&spread), "lantern", &Cancellation::default()).unwrap();
    assert_eq!(passages.len(), MAX_PASSAGES);
    assert!(!complete);
    assert!(passages.iter().all(|passage| passage.pdf_page.is_none()));
}

fn engine_with(source: &str) -> (Engine, tempfile::TempDir, std::path::PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("invented.bib");
    std::fs::write(&path, source).unwrap();
    let engine = Engine::new().unwrap();
    engine
        .dispatch(
            Method::Libraries,
            &json!({"action":"open-file","path":path}),
            &Cancellation::default(),
        )
        .unwrap();
    (engine, directory, path)
}

fn outline(engine: &Engine, key: &str) -> Result<OutlineResult, EngineError> {
    let request = OutlineRequest {
        item: ItemSelector::Key(key.into()),
    };
    engine
        .outline(&request, &Cancellation::default())
        .map(|checked| checked.value)
}

#[test]
fn without_a_pdf_reader_a_files_attachments_are_outlined_but_not_read() {
    let (engine, _directory, path) = engine_with(
        "@book{atlas,title={Invented atlas},file={atlas.pdf;https://example.invalid/atlas}}\n@book{bare,title={Bare}}\n",
    );
    let found = outline(&engine, "atlas").unwrap();
    assert_eq!(found.attachments.len(), 2);
    let file = &found.attachments[0];
    assert!(file.id.starts_with("att:"));
    assert!(!file.id.contains("atlas.pdf"));
    assert_eq!(file.text, TextAvailability::NotYet);
    assert!(!file.pages_known && !file.cite_pages_known);
    assert_eq!(found.attachments[1].text, TextAvailability::NoFile);
    assert!(outline(&engine, "bare").unwrap().attachments.is_empty());
    let read = engine.read_attachment(
        &ReadRequest {
            attachment: file.id.clone(),
            ..ReadRequest::default()
        },
        &Cancellation::default(),
    );
    let refused = read.err().unwrap();
    assert_eq!(
        (refused.code, refused.reason),
        (ErrorCode::SourceUnavailable, Some(ErrorReason::NoAttachmentText))
    );
    // Any change to the reference retires the ids issued for it.
    std::fs::write(
        &path,
        "@book{atlas,title={Invented atlas, revised},file={atlas.pdf}}\n@book{bare,title={Bare}}\n",
    )
    .unwrap();
    engine
        .dispatch(Method::Libraries, &json!({"action":"reload"}), &Cancellation::default())
        .unwrap();
    for id in [file.id.as_str(), "att:invented", "id:invented", ""] {
        let find = FindInRequest {
            attachment: id.into(),
            query: "atlas".into(),
        };
        assert_eq!(
            engine.find_in(&find, &Cancellation::default()).err().map(|e| e.code),
            Some(ErrorCode::NotFound),
            "{id}"
        );
    }
    assert_ne!(outline(&engine, "atlas").unwrap().attachments[0].id, file.id);
}

#[cfg(unix)]
#[test]
fn a_files_pdf_is_read_by_the_child_and_only_while_the_file_holds_the_reference() {
    use std::os::unix::fs::PermissionsExt;
    let (engine, directory, path) = engine_with("@book{atlas,title={Invented atlas},file={atlas.pdf;notes.txt}}\n");
    // A stand-in for `ibis __extract-pdf`, answering for a one-page PDF.
    let program = directory.path().join("invented-ibis");
    std::fs::write(
        &program,
        "#!/bin/sh\ncat >/dev/null\nprintf '{\"read\":{\"pages\":1,\"labels\":[\"7\"],\"text\":[\"Invented lantern.\"]}}'\n",
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    let engine = engine.with_pdf_reader(program);
    let found = outline(&engine, "atlas").unwrap();
    assert_eq!(found.attachments[0].text, TextAvailability::Missing);
    std::fs::write(directory.path().join("atlas.pdf"), "%PDF-1.7\n").unwrap();
    let found = outline(&engine, "atlas").unwrap();
    let pdf = &found.attachments[0];
    assert_eq!(pdf.text, TextAvailability::Available);
    assert!(pdf.pages_known && pdf.cite_pages_known && pdf.pages == Some(1));
    // Only PDFs are handed to the reader.
    assert_eq!(found.attachments[1].text, TextAvailability::NotYet);
    let request = FindInRequest {
        attachment: pdf.id.clone(),
        query: "lantern".into(),
    };
    let passages = engine
        .find_in(&request, &Cancellation::default())
        .unwrap()
        .value
        .passages;
    assert_eq!(passages[0].cite_page.as_deref(), Some("7"));
    // An edit not yet reloaded retires the id rather than read another file.
    std::fs::write(&path, "@book{atlas,title={Invented atlas, revised},file={atlas.pdf}}\n").unwrap();
    assert_eq!(
        engine.find_in(&request, &Cancellation::default()).err().map(|e| e.code),
        Some(ErrorCode::NotFound)
    );
}

#[test]
fn attachment_text_is_allowed_only_for_the_libraries_the_person_named() {
    let (engine, directory, path) = engine_with("@book{atlas,title={Invented atlas},file={atlas.pdf}}\n");
    let library = engine.libraries().unwrap().libraries[0].id.clone();
    let id = outline(&engine, "atlas").unwrap().attachments[0].id.clone();
    assert_eq!(engine.attachment_library(&id).unwrap(), library);
    let mut settings = crate::settings::Settings::default();
    assert!(!engine.attachment_text_allowed(&settings, &library).unwrap());
    settings.device.attachment_text = vec![crate::settings::LibrarySetting::Zotero(
        serde_json::from_value(json!("user")).unwrap(),
    )];
    assert!(!engine.attachment_text_allowed(&settings, &library).unwrap());
    settings.device.attachment_text = vec![crate::settings::LibrarySetting::Bibtex(
        directory.path().join("other.bib"),
    )];
    assert!(!engine.attachment_text_allowed(&settings, &library).unwrap());
    settings.device.attachment_text = vec![crate::settings::LibrarySetting::Bibtex(path)];
    assert!(engine.attachment_text_allowed(&settings, &library).unwrap());
    assert!(
        !engine
            .attachment_text_allowed(&settings, &LibraryId::digest(b"synthetic"))
            .unwrap()
    );
}
