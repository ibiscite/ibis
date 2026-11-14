#![allow(clippy::unwrap_used)]

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
