use super::*;
use ibis_core::contract::Fingerprint;

#[test]
fn folder_identity_follows_the_directory_and_detects_its_replacement() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("Papers");
    let moved = root.path().join("Moved");
    std::fs::create_dir(&path).unwrap();
    let identity = FolderIdentity::of(&path).unwrap();
    std::fs::write(path.join("atlas.pdf"), b"%PDF-1.7\n").unwrap();
    assert_eq!(FolderIdentity::of(&path).as_ref(), Some(&identity));
    std::fs::rename(&path, &moved).unwrap();
    assert_eq!(FolderIdentity::of(&moved).as_ref(), Some(&identity));
    std::fs::create_dir(&path).unwrap();
    assert_ne!(FolderIdentity::of(&path), Some(identity));
    assert_eq!(FolderIdentity::of(&path.join("missing")), None);
    assert_eq!(FolderIdentity::of(&moved.join("atlas.pdf")), None);
    assert!(!path.join(LIBRARY_FILE).exists());
}

fn details() -> PdfDetails {
    PdfDetails {
        revision: Fingerprint::of_bytes(b"invented"),
        pages: 3,
        title: Some("An Invented Atlas of Tides".into()),
        authors: vec!["Ada Lee".into(), "Venn-Harrow, Bo".into(), "Quillfeather".into()],
        container: Some("Journal of Invented Oceans".into()),
        publisher: None,
        volume: Some("12".into()),
        issue: None,
        page: Some("101-119".into()),
        issued: Some("2026-05".into()),
        doi: Some("10.5555/invented.atlas".into()),
        page_doi: Some("10.5555/never.written".into()),
    }
}

#[test]
fn an_entry_holds_only_what_the_pdf_said_and_its_file() {
    let fields = csl(&details());
    assert_eq!(fields["type"], "article-journal");
    assert_eq!(
        fields["author"],
        json!([{"family": "Lee", "given": "Ada"}, {"family": "Venn-Harrow", "given": "Bo"}, {"literal": "Quillfeather"}])
    );
    assert_eq!(fields["issued"], json!({"date-parts": [[2026, 5]]}));
    assert!(!fields.contains_key("publisher") && !fields.contains_key("issue"));
    // A DOI printed on the first page is a candidate, never the record's.
    assert_eq!(fields["DOI"], "10.5555/invented.atlas");
    let text = entry("lee2026invented", fields, "maps/atlas.pdf").unwrap();
    assert!(
        text.starts_with("@article{lee2026invented,\n  file = {:./maps/atlas.pdf:application/pdf},\n"),
        "{text}"
    );
    assert!(!text.contains("never.written"));
    let file = BibFile::read(&text).unwrap();
    assert!(file.key_taken("lee2026invented"));
}

#[test]
fn a_pdf_that_said_nothing_is_keyed_by_its_file_name_and_holds_only_its_file() {
    let fields = Map::new();
    let keyed = key_fields(&fields, "scans/Tide_tables-1931.pdf");
    let key = crate::write::citation_key(&CitableFields::new(keyed), |_| false).unwrap();
    assert_eq!(key, "tide");
    let text = entry(&key, fields, "scans/Tide_tables-1931.pdf").unwrap();
    assert_eq!(text.lines().count(), 3, "{text}");
    assert!(text.starts_with("@misc{tide,\n  file = {:./scans/Tide_tables-1931.pdf:application/pdf},"));
}
