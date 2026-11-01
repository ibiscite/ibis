use ibis_core::contract::added_date;

#[test]
fn recent_added_dates_use_source_metadata_never_the_publication_year() {
    assert_eq!(added_date("2026-10-30 12:34:56"), Some("2026-10-30T12:34:56Z".into()));
    assert_eq!(added_date("2026-10-30T12:34:56Z"), Some("2026-10-30T12:34:56Z".into()));
    assert_eq!(added_date("2026-10-30"), Some("2026-10-30T00:00:00Z".into()));
    // BibDesk writes a numeric offset; Ibis keeps one UTC timeline.
    assert_eq!(
        added_date("2026-10-30 12:34:56 +0000"),
        Some("2026-10-30T12:34:56Z".into())
    );
    assert_eq!(
        added_date("2026-10-30T12:00:00+05:30"),
        Some("2026-10-30T06:30:00Z".into())
    );
    assert_eq!(
        added_date("2026-12-31 22:00:00 -0500"),
        Some("2027-01-01T03:00:00Z".into())
    );
    assert_eq!(
        added_date("2024-03-01T01:00:00+02:00"),
        Some("2024-02-29T23:00:00Z".into())
    );
    for invalid in [
        "2026",
        "2026-02-30",
        "2026-10-30T24:00:00Z",
        "2026-10-30T12:00:00+0500x",
        "2026-10-30T12:00:00+05",
        "2026-10-30T12:00:00+15:00",
        "2026-10-30T12:00:00 GMT",
        "0001-01-01T00:00:00+01:00",
        "2026-10-30T12:00:00Ž",
        "private",
    ] {
        assert_eq!(added_date(invalid), None);
    }
}

#[test]
fn added_metadata_changes_display_but_never_citation_fingerprints() -> Result<(), Box<dyn std::error::Error>> {
    use ibis_core::contract::*;
    let case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json"))?;
    let mut objects: Vec<_> = case
        .library
        .objects()
        .iter()
        .map(|object| object.object.clone())
        .collect();
    let LibraryObject::Record(record) = &mut objects[0] else {
        panic!("record")
    };
    record.added_at = added_date("2026-10-30");
    let changed = Snapshot::new(objects)?;
    assert_ne!(case.library.fingerprints().full, changed.fingerprints().full);
    assert_eq!(case.library.fingerprints().cite, changed.fingerprints().cite);
    Ok(())
}
