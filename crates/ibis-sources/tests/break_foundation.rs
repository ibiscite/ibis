#![allow(clippy::unwrap_used)]

#[test]
fn csl_rejected_payloads_still_quarantine_their_identities() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let path = dir.path().join("invented.json");
    for contender in [
        r#"{"id":"same","type":false,"title":"Invented second"}"#,
        r#"{"title":"Invented second","title":"Repeated","type":"book","id":"same"}"#,
        r#"{"id":"other","id":"same","type":"book"}"#,
        r#"{"citation-key":"same","type":"book","title":"First","title":"Repeated"}"#,
    ] {
        std::fs::write(
            &path,
            format!(
                r#"[
            {{"id":"same","type":"book","title":"Invented first"}},
            {contender},
            {{"id":"neighbor","type":"book","title":"Invented neighbor"}}
        ]"#
            ),
        )
        .unwrap();
        let loaded = ibis_sources::load_file(&path).unwrap();
        assert_eq!(loaded.snapshot.objects().len(), 1, "{contender}");
        assert_eq!(loaded.snapshot.objects()[0].object.id().key, "record:neighbor");
        assert_eq!(
            loaded
                .warnings
                .iter()
                .filter(|warning| warning.code == ibis_sources::WarningCode::DuplicateKey)
                .count(),
            2,
            "{contender}"
        );
    }
}

#[test]
fn oversized_duplicate_ids_leave_diagnostics_readable() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let path = dir.path().join("invented.json");
    let prefix = "界".repeat(256);
    let id = format!("{prefix}{}", "x".repeat(9 * 1024 * 1024 - prefix.len()));
    let neighbor = format!("{prefix}neighbor");
    std::fs::write(
        &path,
        format!(
            r#"[
        {{"id":"{id}","type":"book"}},
        {{"id":"{id}","type":"book"}},
        {{"id":"{neighbor}","type":"book","title":"Invented neighbor"}}
    ]"#
        ),
    )
    .unwrap();
    let loaded = ibis_sources::load_file(&path).unwrap();
    assert_eq!(loaded.snapshot.objects().len(), 1);
    assert_eq!(
        loaded.snapshot.objects()[0].object.id().key,
        format!("record:{neighbor}")
    );
    let duplicates: Vec<_> = loaded
        .warnings
        .iter()
        .filter(|warning| warning.code == ibis_sources::WarningCode::DuplicateKey)
        .collect();
    assert_eq!(duplicates.len(), 2);
    for warning in duplicates {
        let label = warning.key.as_ref().unwrap();
        assert_eq!(label, &format!("{prefix}…"));
    }
    assert!(serde_json::to_vec(&loaded.warnings).unwrap().len() < 16 * 1024 * 1024);
}
