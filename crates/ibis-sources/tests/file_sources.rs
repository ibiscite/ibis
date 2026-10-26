use ibis_core::contract::{Availability, LibraryObject};

#[test]
fn damaged_bibtex_keeps_records_and_private_attachments() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("synthetic.pdf"), b"invented attachment")?;
    let path = dir.path().join("synthetic.bib");
    std::fs::write(
        &path,
        "@book{one,title={Invented},file={synthetic.pdf}}\n@book{broken,title={Unclosed}\n@book{two,title={Second}}\n",
    )?;
    let loaded = ibis_sources::load_file(&path)?;
    let records: Vec<_> = loaded
        .snapshot
        .objects()
        .iter()
        .filter_map(|item| match &item.object {
            LibraryObject::Record(record) => Some(record),
            _ => None,
        })
        .collect();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].citation_key()?, Some("one"));
    assert!(!loaded.warnings.is_empty());
    assert!(loaded.snapshot.objects().iter().any(|item| matches!(&item.object, LibraryObject::Attachment(attachment) if attachment.availability == Availability::OnDisk)));
    let wire = serde_json::to_string(&loaded.snapshot)?;
    assert!(!wire.contains(dir.path().to_string_lossy().as_ref()));
    assert!(!wire.contains("synthetic.pdf"));
    Ok(())
}
