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

type Added = Vec<(String, Option<String>)>;

fn added_dates(name: &str, text: &str) -> Result<Added, Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join(name);
    std::fs::write(&path, text)?;
    let loaded = ibis_sources::load_file(&path)?;
    Ok(loaded
        .snapshot
        .objects()
        .iter()
        .filter_map(|object| match &object.object {
            LibraryObject::Record(record) => Some((record.id.key.clone(), record.added_at.clone())),
            _ => None,
        })
        .collect())
}

#[test]
fn bibtex_reads_bibdesk_and_jabref_added_dates_and_skips_malformed_ones() -> Result<(), Box<dyn std::error::Error>> {
    let mut dates = added_dates(
        "synthetic.bib",
        r"@book{bibdesk, title={A}, date-added={2026-10-30 21:15:00 -0500}}
@book{jabref, title={B}, creationdate={2026-10-29T08:00:00}}
@book{older, title={C}, timestamp={2026-10-28}}
@book{fallback, title={D}, date-added={yesterday}, creationdate={2026-10-27T09:30:00}}
@book{invalid, title={E}, date-added={2026-02-30 10:00:00 +0000}, timestamp={private}}
@book{missing, title={F}, year={1999}}",
    )?;
    dates.sort();
    assert_eq!(
        dates,
        [
            ("record:bibdesk", Some("2026-10-31T02:15:00Z")),
            ("record:fallback", Some("2026-10-27T09:30:00Z")),
            ("record:invalid", None),
            ("record:jabref", Some("2026-10-29T08:00:00Z")),
            ("record:missing", None),
            ("record:older", Some("2026-10-28T00:00:00Z")),
        ]
        .map(|(key, date)| (key.to_owned(), date.map(str::to_owned)))
    );
    Ok(())
}

#[test]
fn added_dates_from_bibtex_and_csl_json_share_one_utc_order() -> Result<(), Box<dyn std::error::Error>> {
    let bib = added_dates(
        "synthetic.bib",
        "@book{late, title={Late}, date-added={2026-10-30 23:00:00 -0500}}",
    )?;
    let csl = added_dates(
        "synthetic.json",
        r#"[{"id":"early","type":"book","title":"Early","date-added":"2026-10-31T03:00:00Z"}]"#,
    )?;
    let (late, early) = (bib[0].1.clone().ok_or("bib date")?, csl[0].1.clone().ok_or("csl date")?);
    // 23:00 at UTC-5 is 04:00 UTC the next day, after the CSL-JSON reference.
    assert!(late > early, "{late} must sort after {early}");
    Ok(())
}
