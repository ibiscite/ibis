use ibis_core::contract::{LibraryId, LibraryObject};
use ibis_sources::zotero::{ZoteroItem, map_item};

#[test]
fn source_added_dates_are_separate_from_printed_data() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    for (name, text) in [
        (
            "synthetic.bib",
            "@book{synthetic,title={Synthetic work},year={1999},timestamp={2026-10-30}}",
        ),
        (
            "synthetic.json",
            r#"[{"id":"synthetic","type":"book","title":"Synthetic work","date-added":"2026-10-30T12:00:00Z"}]"#,
        ),
    ] {
        let path = directory.path().join(name);
        std::fs::write(&path, text)?;
        let loaded = ibis_sources::load_file(&path)?;
        let LibraryObject::Record(record) = &loaded.snapshot.objects()[0].object else {
            return Err("record".into());
        };
        assert!(
            record
                .added_at
                .as_deref()
                .is_some_and(|date| date.starts_with("2026-10-30"))
        );
        assert!(!record.csl.contains_key("date-added"));
        assert!(!record.csl.contains_key("timestamp"));
    }
    let mapped = map_item(
        LibraryId::file(b"synthetic"),
        &ZoteroItem {
            key: "SYNTH001".into(),
            item_type: "book".into(),
            fields: [
                ("title".into(), "Synthetic work".into()),
                ("dateAdded".into(), "2026-10-30 12:00:00".into()),
            ]
            .into(),
            ..ZoteroItem::default()
        },
    )?;
    assert_eq!(mapped.record.added_at.as_deref(), Some("2026-10-30T12:00:00Z"));
    assert!(!mapped.record.csl.contains_key("dateAdded"));
    assert!(mapped.warnings.is_empty());
    Ok(())
}
