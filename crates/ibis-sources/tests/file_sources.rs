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

#[test]
fn csl_json_uses_citation_key_then_id_and_reports_exact_lines() -> Result<(), Box<dyn std::error::Error>> {
    use ibis_sources::WarningCode;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("synthetic.json");
    std::fs::write(
        &path,
        "[\n {\"id\":\"native\",\"citation-key\":\"Chosen2026\",\"type\":\"book\",\"title\":\"Invented\",\"custom\":{\"path\":\"/invented/private\"}},\n {\"id\":7,\"type\":\"article-journal\"},\n {\"id\":\"native\",\"type\":\"book\"},\n {\"type\":\"book\"},\n {\"id\":\"partial\",\"type\":\"book\",\"title\":{}},\n {\"id\":\"last\",\"type\":\"book\",\"URL\":\"file:///invented/private\"}\n]",
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
    assert_eq!(records.len(), 4);
    assert_eq!(
        records
            .iter()
            .find(|record| record.id.key == "record:native")
            .and_then(|record| record.citation_key().ok().flatten()),
        Some("Chosen2026")
    );
    assert_eq!(
        records
            .iter()
            .find(|record| record.id.key == "record:7")
            .and_then(|record| record.citation_key().ok().flatten()),
        Some("7")
    );
    assert!(
        loaded
            .warnings
            .iter()
            .any(|warning| warning.code == WarningCode::DuplicateKey && warning.line == Some(4))
    );
    assert!(
        loaded
            .warnings
            .iter()
            .any(|warning| warning.code == WarningCode::Syntax && warning.line == Some(5))
    );
    let wire = serde_json::to_string(&loaded.snapshot)?;
    assert!(!wire.contains("/invented/private"));
    let record = records
        .iter()
        .find(|record| record.id.key == "record:native")
        .ok_or("missing record")?;
    assert!(
        record
            .raw
            .as_ref()
            .and_then(|raw| raw.data.as_str())
            .is_some_and(|raw| raw.contains("custom"))
    );
    Ok(())
}

#[test]
fn file_identity_survives_edits_and_follows_the_canonical_location() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("synthetic.bib");
    std::fs::write(&path, "@book{one,title={First}}")?;
    let first = ibis_sources::load_file(&path)?;
    std::fs::write(&path, "@book{one,title={Changed}}")?;
    let second = ibis_sources::load_file(&path)?;
    assert_eq!(first.library, second.library);
    assert_ne!(first.snapshot.fingerprints(), second.snapshot.fingerprints());
    let moved = dir.path().join("moved.bib");
    std::fs::rename(path, &moved)?;
    assert_ne!(second.library, ibis_sources::load_file(&moved)?.library);
    Ok(())
}

#[test]
fn record_keys_cannot_collide_with_generated_attachment_keys() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("synthetic.bib");
    std::fs::write(
        &path,
        "@book{one,file={missing.pdf}}\n@book{attachment:record:one:0,title={Synthetic}}",
    )?;
    let loaded = ibis_sources::load_file(&path)?;
    assert_eq!(loaded.snapshot.objects().len(), 3);
    assert!(loaded.snapshot.objects().iter().any(|item|matches!(&item.object,LibraryObject::Attachment(attachment) if attachment.availability==Availability::BrokenLink)));
    Ok(())
}

#[test]
fn preserved_parser_keeps_exact_csl_meaning_of_invented_examples() -> Result<(), Box<dyn std::error::Error>> {
    use std::collections::BTreeMap;
    let examples = [
        "@book{one,title={An Invented {DNA} Study},author={Example, Ada and {Synthetic Organization} and others},date={2024-02-29},publisher={Invented Press},isbn={9780000000002},langid={english}}",
        "@article{two,title={Synthetic \\emph{in vivo} Study},journaltitle={Invented Journal},author={van Example, Ada},date={2023-02-30},pages={12--20},volume={2},number={3},doi={10.0000/synthetic},url={https://example.invalid/synthetic},keywords={one,two}}",
        "@xdata{base,publisher={Synthetic Press},date={2020-02-05}}\n@incollection{three,title={Invented Chapter},booktitle={Synthetic Book},xdata={base},month=mar,editor={Example, Bert},translator={Example, Cora}}",
        "@string{press={Invented Press}}\n@book{four,title={Synthetic},publisher=press,date={2001/2003},origdate={1900~},urldate={2026-10-26}}",
        "@book{five,title={Étude fictive},language={fr-FR},author={范, 明},shorttitle={Fictive},abstract={Invented abstract}}",
    ];
    let dir = tempfile::tempdir()?;
    for (index, text) in examples.into_iter().enumerate() {
        let expected = ibis_core::bibtex::load_str("synthetic-normalization", text);
        let path = dir.path().join("synthetic.bib");
        std::fs::write(&path, text)?;
        let actual = ibis_sources::load_file(&path)?;
        let expected: BTreeMap<_, _> = expected
            .references
            .into_iter()
            .map(|record| (record.citekey.clone().unwrap_or_default(), record.to_csl_json()))
            .collect();
        let actual: BTreeMap<_, _> = actual
            .snapshot
            .objects()
            .iter()
            .filter_map(|item| match &item.object {
                LibraryObject::Record(record) => Some((
                    record.citation_key().ok().flatten().unwrap_or_default().to_owned(),
                    serde_json::to_value(&record.csl),
                )),
                _ => None,
            })
            .map(|(key, value)| value.map(|value| (key, value)))
            .collect::<Result<_, _>>()?;
        assert_eq!(actual, expected, "invented case {index}");
    }
    Ok(())
}

#[test]
fn malformed_native_keys_do_not_discard_other_records() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("synthetic.bib");
    std::fs::write(
        &path,
        "@book{good,title={One}}\n@book{bad\u{0000}key,title={Bad}}\n@book{last,title={Last}}",
    )?;
    let loaded = ibis_sources::load_file(&path)?;
    assert!(
        loaded.snapshot.objects().iter().any(
            |item| matches!(&item.object,LibraryObject::Record(record) if record.citation_key()==Ok(Some("good")))
        )
    );
    assert!(
        loaded.snapshot.objects().iter().any(
            |item| matches!(&item.object,LibraryObject::Record(record) if record.citation_key()==Ok(Some("last")))
        )
    );
    assert!(!loaded.warnings.is_empty());
    Ok(())
}

#[test]
fn deeply_nested_custom_json_is_private_and_does_not_hide_other_records() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("synthetic.json");
    let depth = 20_000;
    let text = format!(
        "[{{\"id\":\"deep\",\"type\":\"book\",\"custom\":{}null{}}},{{\"id\":\"good\",\"type\":\"book\",\"title\":\"Synthetic\"}}]",
        "[".repeat(depth),
        "]".repeat(depth)
    );
    std::fs::write(&path, text)?;
    let loaded = ibis_sources::load_file(&path)?;
    assert_eq!(loaded.snapshot.objects().len(), 2);
    assert!(!loaded.warnings.is_empty());
    let wire = serde_json::to_string(&loaded.snapshot)?;
    assert!(!wire.contains("custom"));
    Ok(())
}

#[test]
fn duplicate_json_fields_are_refused_without_guessing_a_key() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("synthetic.json");
    std::fs::write(
        &path,
        r#"[{"id":"ambiguous","type":"book","citation-key":"First","citation-key":"Second"},{"id":"good","type":"book"}]"#,
    )?;
    let loaded = ibis_sources::load_file(&path)?;
    assert_eq!(loaded.snapshot.objects().len(), 1);
    assert!(!loaded.warnings.is_empty());
    Ok(())
}

#[test]
fn huge_fields_non_files_invalid_utf8_and_empty_libraries_have_explicit_outcomes()
-> Result<(), Box<dyn std::error::Error>> {
    use ibis_sources::SourceError;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("synthetic.bib");
    assert_eq!(ibis_sources::load_file(dir.path()).err(), Some(SourceError::NotRegular));
    std::fs::write(&path, [0xff])?;
    assert_eq!(ibis_sources::load_file(&path).err(), Some(SourceError::Unreadable));
    std::fs::write(&path, "")?;
    assert!(ibis_sources::load_file(&path)?.snapshot.objects().is_empty());
    std::fs::write(
        &path,
        format!(
            "@book{{huge,title={{{}}},publisher={{Synthetic Press}}}}\n@book{{last,title={{Last}}}}",
            "x".repeat(128 * 1024)
        ),
    )?;
    let loaded = ibis_sources::load_file(&path)?;
    assert_eq!(loaded.snapshot.objects().len(), 2);
    assert!(!loaded.warnings.is_empty());
    let file = std::fs::OpenOptions::new().write(true).truncate(true).open(&path)?;
    file.set_len(ibis_sources::MAX_LIBRARY_BYTES + 1)?;
    drop(file);
    assert_eq!(ibis_sources::load_file(&path).err(), Some(SourceError::TooLarge));
    Ok(())
}

#[test]
fn remote_and_ambiguous_attachment_locations_never_become_local_paths() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("synthetic.json");
    std::fs::write(
        &path,
        r#"[{"id":"one","type":"book","file":"https://example.invalid/synthetic.pdf;\\\\synthetic.invalid\\share\\file.pdf;ambiguous:thing:else"}]"#,
    )?;
    let loaded = ibis_sources::load_file(&path)?;
    assert_eq!(loaded.local_attachment_count(), 0);
    let states: Vec<_> = loaded
        .snapshot
        .objects()
        .iter()
        .filter_map(|item| match &item.object {
            LibraryObject::Attachment(attachment) => Some(attachment.availability),
            _ => None,
        })
        .collect();
    assert_eq!(
        states,
        [Availability::Online, Availability::Unknown, Availability::Unknown]
    );
    let wire = serde_json::to_string(&loaded.snapshot)?;
    assert!(!wire.contains("synthetic.invalid"));
    assert!(!loaded.warnings.is_empty());
    Ok(())
}

#[test]
fn limited_json_warns_on_initial_read_and_refuses_replacement() -> Result<(), Box<dyn std::error::Error>> {
    use ibis_sources::{SourceError, WarningCode};
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("synthetic.json");
    std::fs::write(
        &path,
        format!(
            r#"[{{"id":"huge","type":"book","title":"{}"}},{{"id":"good","type":"book"}}]"#,
            "x".repeat(1024 * 1024)
        ),
    )?;
    let loaded = ibis_sources::load_file(&path)?;
    assert_eq!(loaded.snapshot.objects().len(), 1);
    assert!(
        loaded
            .warnings
            .iter()
            .any(|warning| warning.code == WarningCode::WorkLimit)
    );
    assert_eq!(ibis_sources::reload_file(&path).err(), Some(SourceError::LoadLimit));
    Ok(())
}

#[test]
fn numeric_json_ids_use_consistent_citation_processor_spelling() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("synthetic.json");
    for (id, key) in [
        ("1.0", "1"),
        ("-0.0", "0"),
        ("1e-6", "0.000001"),
        ("1e-7", "1e-7"),
        ("1.25", "1.25"),
        ("51.248178375505404", "51.248178375505404"),
    ] {
        std::fs::write(&path, format!(r#"[{{"id":{id},"type":"book"}}]"#))?;
        let loaded = ibis_sources::load_file(&path)?;
        let key_from_source = loaded.snapshot.objects().iter().find_map(|item| match &item.object {
            LibraryObject::Record(record) => record.citation_key().ok().flatten(),
            _ => None,
        });
        assert_eq!(key_from_source, Some(key), "numeric identifier {id}");
    }
    Ok(())
}

#[test]
fn unsafe_numeric_native_id_stays_stable_when_explicit_key_changes() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("synthetic.json");
    let mut ids = Vec::new();
    for key in ["First", "Second"] {
        std::fs::write(
            &path,
            format!(r#"[{{"id":9007199254740993,"citation-key":"{key}","type":"book"}}]"#),
        )?;
        let loaded = ibis_sources::load_file(&path)?;
        ids.push(loaded.snapshot.objects()[0].object.id().clone());
    }
    assert_eq!(ids[0], ids[1]);
    Ok(())
}
