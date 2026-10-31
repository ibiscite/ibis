use ibis_core::contract::added_date;

#[test]
fn recent_added_dates_use_source_metadata_never_the_publication_year() {
    assert_eq!(added_date("2026-10-30 12:34:56"), Some("2026-10-30T12:34:56Z".into()));
    assert_eq!(added_date("2026-10-30T12:34:56Z"), Some("2026-10-30T12:34:56Z".into()));
    assert_eq!(added_date("2026-10-30"), Some("2026-10-30T00:00:00Z".into()));
    for invalid in [
        "2026",
        "2026-02-30",
        "2026-10-30T24:00:00Z",
        "2026-10-30T12:00:00+05:00",
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
