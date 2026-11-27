//! RIS exports and the CSV template as libraries (#1531, decision 0043), from
//! the invented files in `fixtures/ris/` and `fixtures/csv/`.
#![cfg(test)]

use std::path::{Path, PathBuf};

use ibis_core::contract::{ItemRef, LibraryObject, Record};
use ibis_sources::{LoadedLibrary, SourceError, SourceWarning, WarningCause, WarningCode};
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

// Zotero's RIS translator writes webpages as ELEC and accessDate as Y2.
#[test]
fn a_zotero_ris_export_keeps_access_dates_separate_from_publication_dates() {
    let loaded = ibis_sources::load_file(&fixture("ris/zotero-accessed.ris")).unwrap();
    assert_eq!(loaded.warnings, []);
    let fields = citation_fields(&loaded);
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0]["title"], "Invented tidal charts");
    assert_eq!(fields[0]["accessed"], json!({"date-parts": [[2026, 3, 14]]}));
    assert!(fields[0].get("issued").is_none());
    assert_eq!(fields[1]["title"], "Invented tide pools");
    assert_eq!(fields[1]["issued"], json!({"date-parts": [[2021, 4, 8]]}));
    assert_eq!(fields[1]["accessed"], json!({"date-parts": [[2026, 3, 15]]}));
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
    // Repeated content identities quarantine every contender.
    std::fs::write(&path, format!("{first}{first}")).unwrap();
    let doubled = ibis_sources::load_file(&path).unwrap();
    assert!(ids(&doubled).is_empty());
    assert_eq!(doubled.warnings.len(), 4);
    assert!(
        doubled
            .warnings
            .iter()
            .all(|warning| warning.code == WarningCode::DuplicateKey)
    );
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
            Some(SourceError::Missing)
        );
    }
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
    // Particles and suffixes split as Zotero splits them.
    assert_eq!(
        field("AU  - van Example, Ada, Jr.\n", "author"),
        json!([{"family": "Example", "given": "Ada", "non-dropping-particle": "van", "suffix": "Jr."}])
    );
    // A date with no year is dropped with a warning; the record stays.
    let monthly = read("PY  - 2026\nDA  - MAY\n");
    assert_eq!(citation_fields(&monthly)[0]["issued"], json!({"date-parts": [[2026]]}));
    assert_eq!(monthly.warnings[0].code, WarningCode::Field);
    assert_eq!(monthly.warnings[0].entry.as_deref(), Some("Invented"));
}

#[test]
fn the_record_limit_counts_every_record_the_file_holds() {
    let directory = tempfile::tempdir().unwrap();
    let mut text = String::from("type,title\n");
    text.push_str(&"book,Invented same\n".repeat(ibis_sources::MAX_LIBRARY_RECORDS));
    text.push_str("book,Invented past the limit\n");
    let loaded = ibis_sources::load_file(&copy(directory.path(), "many.csv", &text)).unwrap();
    assert!(records(&loaded).is_empty());
    assert_eq!(
        loaded
            .warnings
            .iter()
            .filter(|w| w.code == WarningCode::DuplicateKey)
            .count(),
        ibis_sources::MAX_LIBRARY_RECORDS
    );
    assert_eq!(
        loaded
            .warnings
            .iter()
            .filter(|w| w.code == WarningCode::WorkLimit)
            .count(),
        1
    );
    // A key past the limit is said to be possibly unread.
    assert!(loaded.cut_short);
}

/// One record bound for every reader (1 MiB of CSL data): a RIS record or CSV
/// row just under it is kept, one just over is skipped with a work-limit warning.
#[test]
fn records_past_the_shared_size_bound_are_skipped_in_ris_and_csv() {
    let directory = tempfile::tempdir().unwrap();
    for (format, write) in [
        (
            "ris",
            (|title: &str| format!("TY  - BOOK\r\nTI  - {title}\r\nPY  - 2026\r\nER  - \r\n")) as fn(&str) -> String,
        ),
        ("csv", |title: &str| {
            format!("type,title,author,issued,container-title,volume,issue,page,DOI\r\nbook,{title},,2026,,,,,\r\n")
        }),
    ] {
        let mut kept_and_skipped = vec![];
        for title_bytes in [1024 * 1024 - 200, 1024 * 1024 + 1] {
            let path = directory.path().join(format!("invented-{title_bytes}.{format}"));
            std::fs::write(&path, write(&"x".repeat(title_bytes))).unwrap();
            let library = ibis_sources::load_file(&path).unwrap();
            let skipped = library.warnings.iter().any(|w| w.code == WarningCode::WorkLimit);
            kept_and_skipped.push((records(&library).len(), skipped));
        }
        assert_eq!(kept_and_skipped, [(1, false), (0, true)], "{format}");
    }
}
