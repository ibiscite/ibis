use ibis_core::contract::{Availability, LibraryObject};

#[test]
fn tolerant_verification_reuses_only_complete_freshly_parsed_records() -> Result<(), Box<dyn std::error::Error>> {
    use ibis_sources::{SourceError, SourceSnapshot};
    use std::sync::Arc;
    const BIB: &str = "@string{press={Invented Press}}\n@book{parent,title={Synthetic volume},publisher=press}\n@incollection{child,title={Synthetic chapter},crossref={parent}}\n@article{identifier,title={Synthetic signal},eprint={arXiv:2101.00001},archiveprefix={arXiv}}\n@book{stable,title={Synthetic stable}}\n@book{deleted,title={Synthetic retired}}\n";
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("synthetic.bib");
    std::fs::write(&path, BIB)?;
    let previous = SourceSnapshot::read(&path)?.load()?;
    for text in [
        BIB.to_owned(),
        BIB.replace("Invented Press", "Invented House"),
        BIB.replace("Synthetic volume", "Synthetic cosmos"),
        BIB.replace("2101.00001", "2101.00002"),
        BIB.replace("@book{deleted,title={Synthetic retired}}\n", ""),
        format!("{BIB}@book{{unfinished,"),
    ] {
        let modified = std::fs::metadata(&path)?.modified()?;
        std::fs::write(&path, &text)?;
        std::fs::File::options()
            .write(true)
            .open(&path)?
            .set_modified(modified)?;
        let observation = SourceSnapshot::read(&path)?;
        let fresh = observation.load()?;
        let reused = observation.load_reusing(&previous.snapshot)?;
        assert_eq!(reused.snapshot, fresh.snapshot);
        assert_eq!(reused.warnings, fresh.warnings);
        assert_eq!(reused.library, fresh.library);
        for new in reused.snapshot.objects() {
            let old = previous
                .snapshot
                .objects()
                .iter()
                .find(|old| old.object.id() == new.object.id())
                .ok_or("missing prior record")?;
            assert_eq!(Arc::ptr_eq(old, new), old.object == new.object);
            if new.object.id().key == "record:identifier" && text.contains("2101.00002") {
                assert_eq!(old.fingerprints.cite, new.fingerprints.cite);
                assert_ne!(old.fingerprints.full, new.fingerprints.full);
            }
        }
        if text.ends_with("@book{unfinished,") {
            assert!(!reused.warnings.is_empty());
            assert_eq!(
                SourceSnapshot::reload(&path, &previous.snapshot).err(),
                Some(SourceError::Incomplete)
            );
        }
    }
    Ok(())
}

#[test]
fn nonempty_json_without_csl_records_is_a_format_error_but_empty_libraries_are_allowed()
-> Result<(), Box<dyn std::error::Error>> {
    use ibis_sources::SourceError;
    let dir = tempfile::tempdir()?;
    for extension in ["json", "csljson"] {
        let path = dir.path().join(format!("synthetic.{extension}"));
        for text in [r#"[{"theme":"dark"}]"#, "[null, 1, []]", r#"[{"id":"no-type"}]"#] {
            std::fs::write(&path, text)?;
            assert_eq!(ibis_sources::load_file(&path).err(), Some(SourceError::InvalidData));
            assert_eq!(ibis_sources::reload_file(&path).err(), Some(SourceError::InvalidData));
        }
        for text in ["", " \n", "[]", "[ \n ]"] {
            std::fs::write(&path, text)?;
            assert!(ibis_sources::load_file(&path)?.snapshot.objects().is_empty());
            assert!(ibis_sources::reload_file(&path)?.snapshot.objects().is_empty());
        }
        std::fs::write(&path, r#"[{"theme":"dark"},{"id":"synthetic","type":"book"}]"#)?;
        let loaded = ibis_sources::load_file(&path)?;
        assert_eq!(loaded.snapshot.objects().len(), 1);
        assert!(!loaded.warnings.is_empty());
    }
    Ok(())
}

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
    assert!(records.iter().any(|record| record.id.key == "record:native"));
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
    // Fixed CSL expectations keep this regression independent of the removed parser.
    let expected = [
        serde_json::json!({"ISBN":"9780000000002","author":[{"family":"Example","given":"Ada"},{"literal":"Synthetic Organization"},{"literal":"others"}],"citation-key":"one","id":"one","issued":{"date-parts":[[2024,2,29]]},"language":"en-US","publisher":"Invented Press","title":"An invented <span class=\"nocase\">DNA</span> study","type":"book"}),
        serde_json::json!({"DOI":"10.0000/synthetic","URL":"https://example.invalid/synthetic","author":[{"dropping-particle":"van","family":"Example","given":"Ada"}],"citation-key":"two","container-title":"Invented Journal","id":"two","issue":"3","issued":{"date-parts":[[2023,2]]},"keyword":"one, two","page":"12–20","title":"Synthetic <i>in vivo</i> study","type":"article-journal","volume":"2"}),
        serde_json::json!({"citation-key":"three","container-title":"Synthetic Book","editor":[{"family":"Example","given":"Bert"}],"id":"three","issued":{"date-parts":[[2020,3,5]]},"publisher":"Synthetic Press","title":"Invented chapter","translator":[{"family":"Example","given":"Cora"}],"type":"chapter"}),
        serde_json::json!({"accessed":{"date-parts":[[2026,10,26]]},"citation-key":"four","id":"four","issued":{"date-parts":[[2001],[2003]]},"original-date":{"circa":true,"date-parts":[[1900]]},"publisher":"Invented Press","title":"Synthetic","type":"book"}),
        serde_json::json!({"abstract":"Invented abstract","author":[{"family":"范","given":"明"}],"citation-key":"five","id":"five","language":"fr-FR","title":"Étude fictive","title-short":"Fictive","type":"book"}),
    ];
    let dir = tempfile::tempdir()?;
    for (index, (text, expected)) in examples.into_iter().zip(expected).enumerate() {
        let path = dir.path().join("synthetic.bib");
        std::fs::write(&path, text)?;
        let actual = ibis_sources::load_file(&path)?;
        let expected = BTreeMap::from([(
            expected["citation-key"].as_str().unwrap_or_default().to_owned(),
            expected,
        )]);
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
    let reused = ibis_sources::SourceSnapshot::read(&path)?.load_reusing(&loaded.snapshot)?;
    assert_eq!(reused.snapshot, loaded.snapshot);
    assert_eq!(reused.warnings, loaded.warnings);
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
        ("5e-324", "5e-324"),
        ("1e-324", "0"),
        ("2.2250738585072014e-308", "2.2250738585072014e-308"),
        ("9007199254740991", "9007199254740991"),
    ] {
        std::fs::write(&path, format!(r#"[{{"id":{id},"type":"book"}}]"#))?;
        let loaded = ibis_sources::load_file(&path)?;
        let key_from_source = loaded.snapshot.objects().iter().find_map(|item| match &item.object {
            LibraryObject::Record(record) => record.citation_key().ok().flatten(),
            _ => None,
        });
        assert_eq!(key_from_source, Some(key), "numeric identifier {id}");
    }
    for id in ["1e20", "1e21", "9007199254740992", "9007199254740993"] {
        std::fs::write(&path, format!(r#"[{{"id":{id},"type":"book"}}]"#))?;
        assert_eq!(
            ibis_sources::load_file(&path).err(),
            Some(ibis_sources::SourceError::InvalidData),
            "unsafe identifier {id}"
        );
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

#[test]
fn bibtex_eprints_are_searchable_without_changing_citation_output() -> Result<(), Box<dyn std::error::Error>> {
    use ibis_core::identifier::{Identifier, of_record};
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("synthetic.bib");
    std::fs::write(&path, "@article{one,title={Invented}}")?;
    let before = ibis_sources::load_file(&path)?;
    std::fs::write(
        &path,
        "@article{one,title={Invented},eprint={arXiv:2101.00001v2},archivePrefix={arXiv}}",
    )?;
    let after = ibis_sources::load_file(&path)?;
    let LibraryObject::Record(record) = &after.snapshot.objects()[0].object else {
        return Err("missing synthetic record".into());
    };
    assert_eq!(record.identifiers, [Identifier::Arxiv("2101.00001".into())]);
    assert!(of_record(record).contains(&Identifier::Doi("10.48550/arxiv.2101.00001".into())));
    assert_eq!(
        before.snapshot.objects()[0].fingerprints.cite,
        after.snapshot.objects()[0].fingerprints.cite
    );
    assert_ne!(
        before.snapshot.objects()[0].fingerprints.full,
        after.snapshot.objects()[0].fingerprints.full
    );
    Ok(())
}

#[test]
fn invalid_bibtex_native_key_does_not_discard_adjacent_records() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("synthetic.bib");
    std::fs::write(
        &path,
        "@book{first,title={Invented}}\n@book{bad\0key,title={Damaged}}\n@book{last,title={Other}}",
    )?;
    let loaded = ibis_sources::load_file(&path)?;
    assert_eq!(loaded.snapshot.objects().len(), 2);
    assert!(
        loaded
            .warnings
            .iter()
            .any(|warning| warning.code == ibis_sources::WarningCode::Syntax && warning.line == Some(2))
    );
    Ok(())
}

#[test]
fn relative_file_urls_stay_in_private_provenance() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("synthetic.json");
    for url in [
        "~/invented/private.pdf",
        "../invented/private.pdf",
        "./invented/private.pdf",
        "C:invented.pdf",
        r"\invented\private.pdf",
        r"papers\private.pdf",
        "papers/private.pdf",
        "zotero://open-pdf/library/items/INVENTED",
        "papers.zip",
        "x-devonthink-item://INVENTED",
    ] {
        std::fs::write(
            &path,
            serde_json::to_string(&serde_json::json!([{"id":"one","type":"book","URL":url}]))?,
        )?;
        let loaded = ibis_sources::load_file(&path)?;
        let wire = serde_json::to_string(&loaded.snapshot)?;
        assert!(!wire.contains("URL"), "local URL was published");
        assert!(
            loaded
                .warnings
                .iter()
                .any(|warning| warning.code == ibis_sources::WarningCode::Field)
        );
        assert!(matches!(loaded.snapshot.objects()[0].object, LibraryObject::Record(_)));
    }
    for url in [
        "https://example.invalid/invented.pdf",
        "ftp://example.invalid/paper.pdf",
        "mailto:invented@example.invalid",
        "https://doi.org/10.1000/invented",
    ] {
        std::fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!([{"id":"online","type":"book","URL":url}]))?,
        )?;
        let online = ibis_sources::load_file(&path)?;
        assert!(serde_json::to_string(&online.snapshot)?.contains(url));
        assert!(online.warnings.is_empty());
    }
    Ok(())
}

#[test]
fn attachment_limits_keep_records_and_refuse_reloads() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("synthetic.json");
    std::fs::write(dir.path().join("neighbor.pdf"), b"invented attachment")?;
    std::fs::write(
        &path,
        serde_json::to_string(&serde_json::json!([
            {"id":"many","type":"book","file":"x;".repeat(20_000)},
            {"id":"neighbor","type":"book","file":"neighbor.pdf"}
        ]))?,
    )?;
    let loaded = ibis_sources::load_file(&path)?;
    assert_eq!(
        loaded
            .snapshot
            .objects()
            .iter()
            .filter(|item| matches!(item.object, LibraryObject::Record(_)))
            .count(),
        2
    );
    assert!(loaded.snapshot.objects().len() <= 259);
    assert!(
        loaded
            .warnings
            .iter()
            .any(|warning| warning.code == ibis_sources::WarningCode::WorkLimit)
    );
    assert_eq!(
        ibis_sources::reload_file(&path).err(),
        Some(ibis_sources::SourceError::LoadLimit)
    );
    Ok(())
}

#[test]
fn record_caps_keep_a_bounded_prefix_and_refuse_reload() -> Result<(), Box<dyn std::error::Error>> {
    const LIMIT: usize = ibis_sources::MAX_LIBRARY_RECORDS;
    let dir = tempfile::tempdir()?;
    for extension in ["json", "bib"] {
        let path = dir.path().join(format!("synthetic.{extension}"));
        let entries = (0..=LIMIT)
            .map(|index| {
                if extension == "bib" {
                    // Real entries carry fields; the work budget scales with them.
                    format!("@misc{{k{index}, title = {{Synthetic title {index} for the record cap}}}}\n")
                } else {
                    format!(r#"{{"id":"k{index}","type":"book"}}"#)
                }
            })
            .collect::<Vec<_>>();
        let text = if extension == "bib" {
            entries.concat()
        } else {
            format!("[{}]", entries.join(","))
        };
        std::fs::write(&path, text)?;
        let initial = ibis_sources::load_file(&path)?;
        assert_eq!(
            initial.snapshot.objects().len(),
            LIMIT,
            "wrong record cap for {extension}"
        );
        assert!(
            initial
                .warnings
                .iter()
                .any(|warning| warning.code == ibis_sources::WarningCode::WorkLimit)
        );
        drop(initial);
        assert_eq!(
            ibis_sources::reload_file(&path).err(),
            Some(ibis_sources::SourceError::LoadLimit)
        );
    }
    Ok(())
}

#[test]
fn file_bibtex_extensions_override_a_leading_square_bracket() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    for extension in ["bib", "bibtex", "BIB", "BIBTEX"] {
        let path = dir.path().join(format!("synthetic.{extension}"));
        std::fs::write(
            &path,
            "[Invented export comment]\n@book{invented,title={Invented title}}",
        )?;
        assert_eq!(ibis_sources::load_file(&path)?.snapshot.objects().len(), 1);
        assert_eq!(ibis_sources::reload_file(&path)?.snapshot.objects().len(), 1);
    }
    Ok(())
}

#[test]
fn file_json_extensions_use_csl_json() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    for extension in ["json", "csljson", "JSON", "CSLJSON"] {
        let path = dir.path().join(format!("synthetic.{extension}"));
        std::fs::write(&path, r#"[{"id":"invented","type":"book","title":"Invented title"}]"#)?;
        assert_eq!(ibis_sources::load_file(&path)?.snapshot.objects().len(), 1);
        assert_eq!(ibis_sources::reload_file(&path)?.snapshot.objects().len(), 1);
        std::fs::write(&path, "@book{invented,title={Invented title}}")?;
        assert_eq!(
            ibis_sources::load_file(&path).err(),
            Some(ibis_sources::SourceError::InvalidData)
        );
        assert_eq!(
            ibis_sources::reload_file(&path).err(),
            Some(ibis_sources::SourceError::InvalidData)
        );
    }
    Ok(())
}
