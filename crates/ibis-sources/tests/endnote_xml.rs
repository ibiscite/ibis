//! Invented exports through the public file lifecycle; Pandoc is required.
#![allow(clippy::unwrap_used)]

use std::{path::Path, process::Command};

use ibis_core::contract::LibraryObject;
use ibis_sources::{LoadedLibrary, SourceError, WarningCause, WarningCode, load_file};
use serde_json::{Value, json};

const XML: &str = include_str!("../../../fixtures/endnote-xml/library.xml");

fn fields(library: &LoadedLibrary) -> Vec<Value> {
    let mut fields: Vec<_> = library
        .snapshot
        .objects()
        .iter()
        .map(|entry| {
            let LibraryObject::Record(record) = &entry.object else {
                panic!("bibliographic records only")
            };
            assert!(record.citation_keys.is_empty(), "never invent a citation key");
            let mut csl = serde_json::to_value(&record.csl).unwrap();
            csl.as_object_mut().unwrap().remove("id");
            csl
        })
        .collect();
    fields.sort_by_key(|record| record["title"].to_string());
    fields
}

fn open(text: &str, name: &str) -> Result<LoadedLibrary, SourceError> {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(name);
    std::fs::write(&path, text).unwrap();
    load_file(&path)
}

#[test]
fn supported_fields_and_explicit_pandoc_differences() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/endnote-xml/library.xml");
    let loaded = load_file(&fixture).unwrap();
    assert!(loaded.warnings.is_empty());
    let expected = vec![
        json!({"type":"chapter", "title":"Invented capítulo β",
            "author":[{"family":"Leaf","given":"Léo"}], "editor":[{"family":"Reed","given":"Ria"}],
            "container-title":"The Invented Atlas", "issued":{"date-parts":[[2024,7,9]]},
            "page":"41-59", "publisher":"Example Press"}),
        json!({"type":"book", "title":"Invented forêt handbook",
            "author":[{"family":"Roe","given":"Rafael"}], "editor":[{"family":"Page","given":"Pia"}],
            "collection-title":"Invented Series", "issued":{"date-parts":[[2025]]},
            "number-of-pages":"320", "publisher":"Example Press", "publisher-place":"Exampleville"}),
        json!({"type":"article-journal", "title":"Invented marées & 星",
            "author":[{"family":"Vale","given":"Ivo"},{"family":"Quill","given":"Ada"}],
            "editor":[{"family":"Editor","given":"Ena"}], "container-title":"Journal of Invented Tides",
            "issued":{"date-parts":[[2026,3,14]]}, "volume":"12", "issue":"3", "page":"101-118",
            "DOI":"10.0000/invented.tides"}),
    ];
    assert_eq!(fields(&loaded), expected);
    let output = Command::new("pandoc")
        .args(["--from=endnotexml", "--to=csljson"])
        .arg(fixture)
        .output()
        .expect("Pandoc is required for the EndNote XML comparison");
    assert!(output.status.success(), "Pandoc must read the same fixture");
    let mut pandoc: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    pandoc.sort_by_key(|record| record["title"].to_string());
    assert_eq!(pandoc.len(), expected.len());
    for (mut theirs, ours) in pandoc.into_iter().zip(expected) {
        let record = theirs.as_object_mut().unwrap();
        assert!(record.remove("id").is_some()); // Pandoc uses rec-number; Ibis hashes CSL.
        for field in ["editor", "DOI"] {
            // Pandoc 3.13 omits these source fields.
            assert!(!record.contains_key(field));
            if let Some(value) = ours.get(field) {
                record.insert(field.into(), value.clone());
            }
        }
        assert_eq!(
            record["issued"],
            json!({"date-parts":[[ours["issued"]["date-parts"][0][0]]]})
        );
        record.insert("issued".into(), ours["issued"].clone()); // Pandoc retains only year.
        let pages = record.remove("pages").unwrap(); // Pandoc emits the non-CSL spelling.
        record.insert(
            if ours["type"] == "book" {
                "number-of-pages"
            } else {
                "page"
            }
            .into(),
            pages,
        );
        if ours["type"] == "article-journal" {
            let issue = record.remove("number").unwrap();
            record.insert("issue".into(), issue);
        }
        if ours["type"] == "book" {
            let series = record.remove("container-title").unwrap();
            record.insert("collection-title".into(), series);
        }
        assert_eq!(theirs, ours);
    }
}

#[test]
fn recognition_and_malformed_xml_are_distinct() {
    for name in ["export.xml", "export.refs"] {
        assert_eq!(fields(&open(&format!("\u{feff}{XML}"), name).unwrap()).len(), 3);
        for unrelated in ["<html/>", "<xml><settings/></xml>", "<records><record/></records>"] {
            assert_eq!(open(unrelated, name).err(), Some(SourceError::UnknownFormat));
        }
    }
    assert!(fields(&open("<xml><records/></xml>", "empty.xml").unwrap()).is_empty());
    for bad in [
        XML.replace("</records>", ""),
        XML.replace("</title>", "</wrong>"),
        XML.replace("name=\"Book\"", "name=Book"),
        format!("{XML}<xml/>"),
        XML.replace("Invented Series", "undefined &entity;"),
        XML.replace("Invented Series", "invalid \0"),
        XML.replace("Invented Series", "invalid &#1;"),
        format!("{XML}<![CDATA[stray]]>"),
        format!("{XML}&#32;"),
        format!("{XML}<?xml version=\"1.0\"?>"),
        format!("{XML}stray"),
    ] {
        assert_eq!(open(&bad, "damaged.xml").err(), Some(SourceError::InvalidData));
    }
}

#[test]
fn unsupported_records_are_named_and_valid_neighbours_survive() {
    let xml = XML.replace("<ref-type name=\"Book\">6", "<ref-type name=\"Chart\">9");
    let loaded = open(&xml, "mixed.xml").unwrap();
    assert_eq!(fields(&loaded).len(), 2);
    let warning = &loaded.warnings[0];
    assert_eq!(loaded.warnings.len(), 1);
    assert_eq!(warning.cause, Some(WarningCause::UnknownType));
    assert_eq!(warning.field.as_deref(), Some("ref-type"));
    assert_eq!(warning.entry.as_deref(), Some("Invented forêt handbook"));
    assert_eq!(warning.line, Some(14));
    let compact = "<xml><records><record><ref-type>9</ref-type></record><record><ref-type>6</ref-type><titles><title>Invented neighbour</title></titles></record></records></xml>";
    let loaded = open(compact, "compact.xml").unwrap();
    assert_eq!(fields(&loaded).len(), 1);
    assert_eq!(
        loaded.warnings[0].entry, None,
        "an unnamed record cannot borrow its neighbour's title"
    );
}

#[test]
fn export_identity_ignores_database_numbers_and_quarantines_duplicates() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("export.xml");
    std::fs::write(&path, XML).unwrap();
    let before = load_file(&path).unwrap();
    std::fs::write(&path, XML.replace("<rec-number>1", "<rec-number>91")).unwrap();
    let after = load_file(&path).unwrap();
    assert_eq!(before.snapshot.fingerprints(), after.snapshot.fingerprints());
    let first = &XML[XML.find("<record>").unwrap()..XML.find("</record>").unwrap() + "</record>".len()];
    let duplicate = first.replace("<rec-number>1", "<rec-number>92");
    std::fs::write(&path, XML.replace("</records>", &format!("{duplicate}</records>"))).unwrap();
    let loaded = load_file(&path).unwrap();
    assert_eq!(fields(&loaded).len(), 2);
    assert_eq!(loaded.duplicate_keys.len(), 1);
    assert_eq!(loaded.warnings.len(), 2);
    assert!(
        loaded
            .warnings
            .iter()
            .all(|warning| warning.code == WarningCode::DuplicateKey)
    );
}

#[test]
fn external_entities_and_dtds_are_rejected() {
    for declaration in [
        "<!DOCTYPE xml SYSTEM 'file:///invented-entity-must-not-be-opened'>",
        "<!DOCTYPE xml SYSTEM 'http://127.0.0.1:9/must-not-be-requested'>",
        "<!DOCTYPE xml [<!ENTITY external SYSTEM 'file:///invented-entity-must-not-be-opened'>]>",
        "<!DOCTYPE xml [<!ENTITY % external SYSTEM 'https://example.org/must-not-be-requested'>%external;]>",
    ] {
        let xml = XML
            .replace("Invented Series", "&external;")
            .replace("<xml>", &format!("{declaration}<xml>"));
        assert_eq!(open(&xml, "entities.xml").err(), Some(SourceError::InvalidData));
    }
}
