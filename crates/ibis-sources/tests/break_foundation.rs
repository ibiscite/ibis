//! Adversarial source inputs. Invented data only; intentionally failing regressions.
#![allow(clippy::unwrap_used)]

use ibis_core::contract::LibraryObject;

#[test]
fn nul_in_csl_title_is_not_accepted_as_clean_citation_text() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let path = dir.path().join("invented.json");
    std::fs::write(&path, r#"[{"id":"one","type":"book","title":"Invented\u0000hidden"}]"#).unwrap();
    if let Ok(loaded) = ibis_sources::load_file(&path) {
        let kept_nul = loaded.snapshot.objects().iter().any(|item| matches!(&item.object,
            LibraryObject::Record(record) if record.csl.get("title").and_then(serde_json::Value::as_str).is_some_and(|title| title.contains('\0'))));
        assert!(
            !kept_nul || !loaded.warnings.is_empty(),
            "NUL reached a citation field without a source warning"
        );
    }
}

#[test]
fn csl_duplicate_id_is_quarantined_even_when_one_type_is_invalid() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let path = dir.path().join("invented.json");
    std::fs::write(&path, r#"[{"id":"same","type":"book","title":"Invented first"},{"id":"same","type":false,"title":"Invented second"}]"#).unwrap();
    if let Ok(loaded) = ibis_sources::load_file(&path) {
        assert!(
            loaded.snapshot.objects().is_empty(),
            "duplicate id still resolves to {} record(s)",
            loaded.snapshot.objects().len()
        );
    }
}

#[test]
fn wrong_date_part_type_cannot_pass_as_clean_csl() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let path = dir.path().join("invented.json");
    std::fs::write(
        &path,
        r#"[{"id":"one","type":"book","title":"Invented","issued":{"date-parts":[[2026,{},1]]}}]"#,
    )
    .unwrap();
    let loaded = ibis_sources::load_file(&path).unwrap();
    assert!(!loaded.warnings.is_empty());
}

#[test]
fn hostile_source_controls_keep_valid_neighbours_and_reject_mixed_encoding() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let path = dir.path().join("invented.bib");
    for text in [
        "\u{feff}@book{good,title={Invented}}\r\n".to_owned(),
        "@string{a=b}\n@string{b=a}\n@book{bad,title=a}\n@book{good,title={Invented}}".to_owned(),
        format!(
            "@book{{deep,title={}{}}}\n@book{{good,title={{Invented}}}}",
            "{".repeat(1024),
            "}".repeat(1024)
        ),
        format!(
            "@book{{huge,title={{{}}}}}\n@book{{good,title={{Invented}}}}",
            "x".repeat(300_000)
        ),
    ] {
        std::fs::write(&path, text).unwrap();
        let loaded = ibis_sources::load_file(&path).unwrap();
        assert!(
            loaded
                .snapshot
                .objects()
                .iter()
                .any(|item| item.object.id().key == "record:good")
        );
    }
    std::fs::write(&path, b"@book{good,title={Invented}}\n@book{bad,title={\xff}}").unwrap();
    assert_eq!(
        ibis_sources::load_file(&path).err(),
        Some(ibis_sources::SourceError::Unreadable)
    );
    let path = dir.path().join("invented.json");
    let nested = format!("{}null{}", "[".repeat(10_000), "]".repeat(10_000));
    std::fs::write(&path, format!(r#"[{{"id":"good","type":"book","custom":{nested}}}]"#)).unwrap();
    assert_eq!(ibis_sources::load_file(&path).unwrap().snapshot.objects().len(), 1);
    let huge = format!(
        r#"[{{"id":"good","type":"book"}},{}]"#,
        std::iter::repeat_n("null", ibis_sources::MAX_LIBRARY_RECORDS)
            .collect::<Vec<_>>()
            .join(",")
    );
    std::fs::write(&path, huge).unwrap();
    let loaded = ibis_sources::load_file(&path).unwrap();
    assert!(loaded.cut_short);
    assert_eq!(loaded.snapshot.objects().len(), 1);

    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let path = dir
            .path()
            .join(std::ffi::OsString::from_vec(b"invented-\xff.bib".to_vec()));
        std::fs::write(&path, "@book{good,title={Invented}}").unwrap();
        assert_eq!(ibis_sources::load_file(&path).unwrap().snapshot.objects().len(), 1);
        let mut long = dir.path().to_path_buf();
        for _ in 0..8 {
            long.push("i".repeat(200));
        }
        std::fs::create_dir_all(&long).unwrap();
        let path = long.join(format!("{}.bib", "i".repeat(240)));
        std::fs::write(&path, "@book{good,title={Invented}}").unwrap();
        assert_eq!(ibis_sources::load_file(&path).unwrap().snapshot.objects().len(), 1);
    }
}
