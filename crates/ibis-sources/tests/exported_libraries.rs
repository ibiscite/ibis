//! RIS exports and the CSV template as libraries (#1531, decision 0041), from
//! the invented files in `fixtures/ris/` and `fixtures/csv/`.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use ibis_core::contract::{ItemRef, LibraryObject, Record};
use ibis_sources::{LoadedLibrary, SourceError, SourceWarning, SourceWatch, WarningCause, WarningCode};
use serde_json::{Value, json};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name)
}

fn records(library: &LoadedLibrary) -> Vec<&Record> {
    library
        .snapshot
        .objects()
        .iter()
        .filter_map(|item| match &item.object {
            LibraryObject::Record(record) => Some(record),
            _ => None,
        })
        .collect()
}

/// What a citation reads from each record, without its identity, by title.
fn citation_fields(library: &LoadedLibrary) -> Vec<Value> {
    by_title(
        records(library)
            .iter()
            .map(|record| {
                assert!(record.citation_keys.is_empty(), "no record gets an invented key");
                let mut csl = serde_json::to_value(&record.csl).unwrap();
                csl.as_object_mut().unwrap().remove("id");
                csl
            })
            .collect(),
    )
}

/// A snapshot orders records by identity, not by their place in the file.
fn by_title(mut records: Vec<Value>) -> Vec<Value> {
    records.sort_by_key(|record| record["title"].to_string());
    records
}

fn ids(library: &LoadedLibrary) -> Vec<ItemRef> {
    let mut ids: Vec<_> = records(library).iter().map(|record| record.id.clone()).collect();
    ids.sort();
    ids
}

fn skipped(line: usize, cause: WarningCause, field: Option<&str>) -> SourceWarning {
    SourceWarning::at(WarningCode::Syntax, Some(line)).because(cause, field)
}

fn copy(directory: &Path, name: &str, text: &str) -> PathBuf {
    let path = directory.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn a_ris_export_reads_as_citations_without_keys() {
    let loaded = ibis_sources::load_file(&fixture("ris/library.ris")).unwrap();
    assert_eq!(loaded.warnings, []);
    assert_eq!(
        citation_fields(&loaded),
        by_title(vec![
            json!({
                "type": "article-journal",
                "author": [{"family": "Doe", "given": "Jane"}, {"family": "Roe", "given": "Rafael"}],
                "title": "Invented tides of the northern lake",
                "container-title": "Journal of Invented Studies",
                "container-title-short": "J. Invent. Stud.",
                "issued": {"date-parts": [[2026, 3, 14]]},
                "volume": "12",
                "issue": "3",
                "page": "101-118",
                "DOI": "10.0000/invented.2026.1",
                "URL": "https://example.org/tides",
            }),
            // A blank line inside a record is allowed; the range stays as written.
            json!({
                "type": "chapter",
                "author": [{"family": "Quill", "given": "Ada"}],
                "editor": [{"family": "Editor", "given": "Ena"}],
                "title": "An invented chapter",
                "container-title": "The Invented Handbook",
                "publisher": "Example Press",
                "publisher-place": "Exampleville",
                "issued": {"date-parts": [[2025]]},
                "page": "1654–6",
            }),
            // A CJK name is never split, a date with text is kept as written,
            // and a book's T2 is its series.
            json!({
                "type": "book",
                "author": [{"literal": "山田太郎"}, {"literal": "Example Collective"}],
                "title": "A book of invented seasons",
                "collection-title": "Invented Series",
                "publisher": "Example Press",
                "issued": {"literal": "2026/07//Jul-Sep2026"},
                "number-of-pages": "320",
                "ISBN": "978-0-00-000000-2",
            }),
            json!({
                "type": "article-journal",
                "author": [{"family": "Vale", "given": "Ivo"}],
                "title": "An article with only a number",
                "container-title": "Invented Letters",
                "issued": {"date-parts": [[2024]]},
                "volume": "3",
                "number": "e1234",
            }),
        ])
    );
}

#[test]
fn damaged_ris_records_are_skipped_with_their_lines() {
    let path = fixture("ris/damaged.ris");
    let loaded = ibis_sources::load_file(&path).unwrap();
    let titles: Vec<_> = citation_fields(&loaded)
        .iter()
        .map(|csl| csl["title"].clone())
        .collect();
    assert_eq!(titles, ["A good invented record", "A second good invented record"]);
    // Unknown type, two titles, dates that disagree, fields with no TY line,
    // and the record the export never finished.
    assert_eq!(
        loaded.warnings,
        [
            skipped(7, WarningCause::UnknownType, None),
            skipped(11, WarningCause::Conflicting, None),
            skipped(16, WarningCause::Conflicting, None),
            skipped(22, WarningCause::NoStart, None),
            skipped(31, WarningCause::NotClosed, None),
        ]
    );
    // A reload of the same bytes is the same library: an unfinished last record
    // is skipped, not mistaken for a file still being written.
    let reloaded = ibis_sources::reload_file(&path).unwrap();
    assert_eq!(reloaded.snapshot.fingerprints(), loaded.snapshot.fingerprints());
    assert_eq!(reloaded.warnings, loaded.warnings);
}

#[test]
fn the_same_reference_keeps_its_identity_across_exports_and_a_changed_one_does_not() {
    let directory = tempfile::tempdir().unwrap();
    let first = "TY  - JOUR\nTI  - Invented one\nPY  - 2026\nID  - 1\nER  - \nTY  - BOOK\nTI  - Invented two\nER  - \n";
    // Exported again: other order, other database fields and notes.
    let again = "TY  - BOOK\nTI  - Invented two\nN1  - new note\nER  - \nTY  - JOUR\nID  - 99\nAN  - X:1\nTI  - Invented one\nPY  - 2026\nER  - \n";
    let path = copy(directory.path(), "export.ris", first);
    let before = ibis_sources::load_file(&path).unwrap();
    std::fs::write(&path, again).unwrap();
    let after = ibis_sources::load_file(&path).unwrap();
    assert_eq!(ids(&after), ids(&before));
    std::fs::write(&path, first.replace("Invented one", "Invented one, corrected")).unwrap();
    let corrected = ibis_sources::load_file(&path).unwrap();
    let kept: Vec<_> = ids(&corrected)
        .into_iter()
        .filter(|id| ids(&before).contains(id))
        .collect();
    assert_eq!(kept.len(), 1, "only the unchanged reference keeps its identity");
    // The same citation fields twice are one reference.
    std::fs::write(&path, format!("{first}{first}")).unwrap();
    let doubled = ibis_sources::load_file(&path).unwrap();
    assert_eq!(ids(&doubled), ids(&before));
    assert_eq!(doubled.warnings, []);
}

#[test]
fn ris_is_known_by_its_content_in_utf8_with_any_line_ending() {
    let directory = tempfile::tempdir().unwrap();
    let text = std::fs::read_to_string(fixture("ris/library.ris")).unwrap();
    let expected = citation_fields(&ibis_sources::load_file(&fixture("ris/library.ris")).unwrap());
    // EndNote online saves `.txt`; Windows, old Mac and a byte-order mark.
    for (name, text) in [
        ("export.txt", text.clone()),
        ("export.ris", text.replace('\n', "\r\n")),
        ("old.ris", text.replace('\n', "\r")),
        ("marked.ris", format!("\u{feff}{text}")),
        ("named-oddly.refs", text.clone()),
    ] {
        let loaded = ibis_sources::load_file(&copy(directory.path(), name, &text)).unwrap();
        assert_eq!(citation_fields(&loaded), expected, "{name}");
    }
    // A renamed PubMed `.nbib`, RefWorks Tagged and native Web of Science have no RIS record.
    for (name, text) in [
        ("pubmed.ris", "PMID- 1\nTI  - Invented title\nAU  - Doe J\n"),
        ("refworks.txt", "RT Journal Article\nA1 Doe, Jane\nT1 Invented title\n"),
        (
            "wos.txt",
            "FN Invented Export\nPT J\nAU Doe, J\nTI Invented title\nER\nEF\n",
        ),
    ] {
        let path = copy(directory.path(), name, text);
        assert_eq!(
            ibis_sources::load_file(&path).err(),
            Some(SourceError::InvalidData),
            "{name}"
        );
    }
    // UTF-16, with or without its byte-order mark, and a legacy encoding.
    let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    for (name, bytes) in [
        ("utf16.ris", [&[0xff, 0xfe][..], &utf16].concat()),
        ("utf16-unmarked.ris", utf16.clone()),
        ("latin1.ris", b"TY  - BOOK\nTI  - Caf\xe9\nER  - \n".to_vec()),
    ] {
        let path = directory.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(
            ibis_sources::load_file(&path).err(),
            Some(SourceError::Unreadable),
            "{name}"
        );
    }
}

#[test]
fn a_csv_in_the_template_reads_as_citations_without_keys() {
    let loaded = ibis_sources::load_file(&fixture("csv/library.csv")).unwrap();
    assert_eq!(loaded.warnings, []);
    assert_eq!(
        citation_fields(&loaded),
        by_title(vec![
            json!({
                "type": "article-journal",
                "title": "Invented tides of the northern lake",
                "author": [{"family": "Doe", "given": "Jane"}, {"family": "Roe", "given": "Rafael"}],
                "issued": {"date-parts": [[2026, 3, 14]]},
                "container-title": "Journal of Invented Studies",
                "volume": "12",
                "issue": "3",
                "page": "101-118",
                "DOI": "10.0000/invented.2026.1",
            }),
            json!({
                "type": "book",
                "title": "A book of invented \"seasons\"",
                "author": [{"literal": "Example Collective"}],
                "issued": {"date-parts": [[2026]]},
            }),
            json!({
                "type": "chapter",
                "title": "An invented chapter\nover two lines",
                "author": [{"family": "Quill", "given": "Ada"}],
                "issued": {"date-parts": [[2025]]},
                "container-title": "The Invented Handbook",
                "page": "1654–6",
            }),
        ])
    );
    // Spreadsheets set to a comma decimal save with semicolons.
    let directory = tempfile::tempdir().unwrap();
    let semicolons = copy(
        directory.path(),
        "semicolons.csv",
        "type;title;author\r\nbook;Invented atlas;\"Doe, Jane; Roe, Rafael\"\r\n",
    );
    assert_eq!(
        citation_fields(&ibis_sources::load_file(&semicolons).unwrap()),
        [json!({
            "type": "book",
            "title": "Invented atlas",
            "author": [{"family": "Doe", "given": "Jane"}, {"family": "Roe", "given": "Rafael"}],
        })]
    );
}

#[test]
fn csv_rows_that_do_not_fit_the_template_are_skipped_with_their_lines() {
    let loaded = ibis_sources::load_file(&fixture("csv/damaged.csv")).unwrap();
    let titles: Vec<_> = citation_fields(&loaded)
        .iter()
        .map(|csl| csl["title"].clone())
        .collect();
    assert_eq!(titles, ["A good invented row", "A second good invented row"]);
    // One cell too many, no type, a type CSL lacks, and an unclosed quote.
    assert_eq!(
        loaded.warnings,
        [
            skipped(3, WarningCause::WrongCells, None),
            skipped(4, WarningCause::MissingField, Some("type")),
            skipped(5, WarningCause::UnknownType, None),
            skipped(8, WarningCause::NotClosed, None),
        ]
    );
}

#[test]
fn a_csv_whose_header_is_not_the_template_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    assert_eq!(
        ibis_sources::load_file(&fixture("csv/missing-title.csv")).err(),
        Some(SourceError::CsvTemplate)
    );
    for header in ["type,author", "type,title,autor", "type,title,title"] {
        let path = copy(directory.path(), "header.csv", &format!("{header}\nbook,Invented,x\n"));
        assert_eq!(
            ibis_sources::load_file(&path).err(),
            Some(SourceError::CsvTemplate),
            "{header}"
        );
    }
    // Column names ignore case and surrounding spaces.
    let path = copy(
        directory.path(),
        "header.csv",
        "Type , TITLE,doi\nbook,Invented,10.0000/x\n",
    );
    assert_eq!(records(&ibis_sources::load_file(&path).unwrap()).len(), 1);
}

#[test]
fn empty_and_missing_files() {
    let directory = tempfile::tempdir().unwrap();
    for path in [
        fixture("ris/empty.ris"),
        copy(directory.path(), "blank.ris", " \n\n"),
        copy(directory.path(), "empty.csv", ""),
        copy(directory.path(), "header-only.csv", "type,title\n"),
    ] {
        let loaded = ibis_sources::load_file(&path).unwrap();
        assert!(loaded.snapshot.objects().is_empty(), "{}", path.display());
        assert_eq!(loaded.warnings, []);
    }
    for name in ["missing.ris", "missing.csv"] {
        assert_eq!(
            ibis_sources::load_file(&directory.path().join(name)).err(),
            Some(SourceError::Unavailable)
        );
    }
}

#[test]
fn a_ris_file_changed_while_open_updates_and_keeps_unchanged_identities() {
    let directory = tempfile::tempdir().unwrap();
    let one = "TY  - BOOK\nTI  - Invented one\nER  - \n";
    let path = copy(directory.path(), "watched.ris", one);
    let (sender, events) = mpsc::channel();
    let _watch = SourceWatch::new(&path, move |result| {
        let _ = sender.send(result);
    })
    .unwrap();
    let next = || {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let remaining = deadline.checked_duration_since(Instant::now()).unwrap();
            match events.recv_timeout(remaining).unwrap() {
                Err(SourceError::Changing) => continue,
                result => return result,
            }
        }
    };
    let first = next().unwrap();
    std::fs::write(&path, format!("{one}TY  - BOOK\nTI  - Invented two\nER  - \n")).unwrap();
    let second = next().unwrap();
    assert_eq!(records(&second).len(), 2);
    assert!(ids(&second).contains(&ids(&first)[0]));
    // An unfinished record written later is skipped with its line.
    std::fs::write(&path, format!("{one}TY  - BOOK\nTI  - Unfinished")).unwrap();
    let third = next().unwrap();
    assert_eq!(ids(&third), ids(&first));
    assert_eq!(third.warnings, [skipped(4, WarningCause::NotClosed, None)]);
}

#[test]
fn ris_fields_keep_what_was_written_and_refuse_what_disagrees() {
    let directory = tempfile::tempdir().unwrap();
    let read = |record: &str| {
        let path = copy(
            directory.path(),
            "one.ris",
            &format!("TY  - JOUR\nTI  - Invented\n{record}ER  - \n"),
        );
        ibis_sources::load_file(&path).unwrap()
    };
    let field = |record: &str, name: &str| citation_fields(&read(record))[0][name].clone();
    // An abbreviated range agrees with its full end; another end refuses the record.
    assert_eq!(field("SP  - 1654–6\nEP  - 1656\n", "page"), "1654–6");
    assert_eq!(
        read("SP  - 1654–6\nEP  - 1660\n").warnings,
        [skipped(1, WarningCause::Conflicting, None)]
    );
    // A file link beside a web address: only the web address is kept.
    assert_eq!(
        field("UR  - file:///invented.pdf\nUR  - https://example.org/a\n", "URL"),
        "https://example.org/a"
    );
    // A secondary author of an article is never folded into its authors.
    assert_eq!(
        field("AU  - Doe, Jane\nA2  - Roe, Rafael\n", "author"),
        json!([{"family": "Doe", "given": "Jane"}])
    );
    assert_eq!(field("A2  - Roe, Rafael\n", "editor"), Value::Null);
    // A date with no year is dropped with a warning; the record stays.
    let monthly = read("PY  - 2026\nDA  - MAY\n");
    assert_eq!(citation_fields(&monthly)[0]["issued"], json!({"date-parts": [[2026]]}));
    assert_eq!(monthly.warnings[0].code, WarningCode::Field);
}

#[test]
fn names_dates_and_identifiers_that_agree_keep_the_record() {
    let directory = tempfile::tempdir().unwrap();
    let read = |name: &str, text: &str| ibis_sources::load_file(&copy(directory.path(), name, text)).unwrap();
    let ris = |record: &str| read("one.ris", &format!("TY  - BOOK\nTI  - Invented\n{record}ER  - \n"));
    // A name given twice is one person, in the order first given.
    let named = ris("AU  - Doe, Jane\nAU  - Roe, Rafael\nAU  - Doe, Jane\n");
    assert_eq!(
        citation_fields(&named)[0]["author"],
        json!([{"family": "Doe", "given": "Jane"}, {"family": "Roe", "given": "Rafael"}])
    );
    // A year that agrees with a date kept as text keeps the record and the text.
    let dated = ris("PY  - 2026\nY1  - 2026/07//Jul-Sep2026\n");
    assert_eq!(dated.warnings, []);
    assert_eq!(
        citation_fields(&dated)[0]["issued"],
        json!({"literal": "2026/07//Jul-Sep2026"})
    );
    // An ISBN or ISSN that fails its check digit is dropped with a warning.
    let valid = ris("SN  - 978-0-00-000000-2 (pbk.)\n");
    assert_eq!(citation_fields(&valid)[0]["ISBN"], "978-0-00-000000-2");
    let invalid = ris("SN  - 9780000000003\n");
    assert_eq!(citation_fields(&invalid)[0]["ISBN"], Value::Null);
    assert_eq!(invalid.warnings[0].code, WarningCode::Field);
    let csv = read(
        "serials.csv",
        "type,title,ISSN,ISBN\narticle-journal,Invented one,1234-5679,\nbook,Invented two,,9780000000003\n",
    );
    let fields = citation_fields(&csv);
    assert_eq!(fields[0]["ISSN"], "1234-5679");
    assert_eq!(fields[1]["ISBN"], Value::Null);
    assert_eq!(
        csv.warnings.iter().map(|w| (w.code, w.line)).collect::<Vec<_>>(),
        [(WarningCode::Field, Some(3))]
    );
}

#[test]
fn a_quote_inside_an_unquoted_cell_skips_the_row() {
    let directory = tempfile::tempdir().unwrap();
    let path = copy(
        directory.path(),
        "quote.csv",
        "type,title\nbook,Unclosed \"quote\nbook,Invented\n",
    );
    assert_eq!(
        ibis_sources::load_file(&path).unwrap().warnings,
        [skipped(2, WarningCause::UnexpectedText, None)]
    );
}

#[test]
fn the_record_limit_counts_every_record_the_file_holds() {
    let directory = tempfile::tempdir().unwrap();
    let mut text = String::from("type,title\n");
    text.push_str(&"book,Invented same\n".repeat(ibis_sources::MAX_LIBRARY_RECORDS));
    text.push_str("book,Invented past the limit\n");
    let loaded = ibis_sources::load_file(&copy(directory.path(), "many.csv", &text)).unwrap();
    assert_eq!(records(&loaded).len(), 1);
    assert_eq!(
        loaded.warnings.iter().map(|w| w.code).collect::<Vec<_>>(),
        [WarningCode::WorkLimit]
    );
    // A key past the limit is said to be possibly unread.
    assert!(loaded.cut_short);
}

#[test]
fn equivalent_values_merge_and_a_bad_one_does_not_take_the_good_ones_with_it() {
    let directory = tempfile::tempdir().unwrap();
    let ris = |record: &str| {
        let path = copy(
            directory.path(),
            "one.ris",
            &format!("TY  - BOOK\nTI  - Invented\n{record}ER  - \n"),
        );
        ibis_sources::load_file(&path).unwrap()
    };
    let named = ris("AU  - Doe, Jane\nA1  - Doe,Jane\n");
    assert_eq!(
        citation_fields(&named)[0]["author"],
        json!([{"family": "Doe", "given": "Jane"}])
    );
    // A text date's month takes part in the check against an exact date.
    assert_eq!(
        ris("PY  - 2026/07//Jul-Sep2026\nDA  - 2026/03/14/\n").warnings,
        [skipped(1, WarningCause::Conflicting, None)]
    );
    assert_eq!(
        citation_fields(&ris("SN  - 978 0 00 000000 2\n"))[0]["ISBN"],
        "978 0 00 000000 2"
    );
    let mixed = ris("SN  - 978-0-00-000000-2\nSN  - invented\n");
    assert_eq!(citation_fields(&mixed)[0]["ISBN"], "978-0-00-000000-2");
    assert_eq!(mixed.warnings[0].code, WarningCode::Field);
}
